//! AI 標注建議：產生、列出、接受（可以先修改）、拒絕。

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::Actor,
    dataset::Span,
    db,
    error::{AppError, AppResult},
    llm::{JsonTask, Llm},
    models::{Origin, QuestionView, SpanInput, Suggestion, SuggestionStatus},
    state::AppState,
    store::{self, NewQuestion},
    suggest::{self, Draft, Dropped, Mode},
};

const SUGGESTION_COLUMNS: &str =
    "id, article_id, question, is_impossible, answer_start, answer_text,
    reference_answer, provider, model, status, question_id, requested_by, created_at, decided_at";

fn llm(state: &AppState) -> AppResult<&Llm> {
    state.llm.as_ref().ok_or_else(|| {
        AppError::Unavailable(
            state
                .llm_disabled_reason()
                .unwrap_or("AI 輔助標注沒有開啟")
                .to_string(),
        )
    })
}

async fn suggestion(state: &AppState, id: i64) -> AppResult<Suggestion> {
    sqlx::query_as::<_, Suggestion>(&format!(
        "SELECT {SUGGESTION_COLUMNS} FROM suggestions WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("找不到這個建議"))
}

/// 文章還沒處理的建議。
pub async fn list(
    State(state): State<AppState>,
    Path(article_id): Path<i64>,
) -> AppResult<Json<Vec<Suggestion>>> {
    store::article(&state.db, article_id).await?;
    Ok(Json(
        sqlx::query_as::<_, Suggestion>(&format!(
            "SELECT {SUGGESTION_COLUMNS} FROM suggestions WHERE article_id = ? AND status = 'open' ORDER BY id"
        ))
        .bind(article_id)
        .fetch_all(&state.db)
        .await?,
    ))
}

#[derive(Serialize)]
pub struct SuggestResponse {
    suggestions: Vec<Suggestion>,
    /// 模型給了、但沒通過檢查的建議（例如答案不是原文片段）。
    dropped: Vec<Dropped>,
}

#[derive(Deserialize)]
pub struct GenerateInput {
    count: Option<usize>,
    annotator_id: Option<i64>,
}

/// 請模型替文章出題。已經有的問題和還沒處理的建議都會告訴模型，避免重複。
pub async fn generate(
    State(state): State<AppState>,
    actor: Actor,
    Path(article_id): Path<i64>,
    Json(input): Json<GenerateInput>,
) -> AppResult<Json<SuggestResponse>> {
    let llm = llm(&state)?;
    let requested_by = actor.annotator(&state.db, input.annotator_id).await?;
    let article = store::article(&state.db, article_id).await?;
    let existing: Vec<String> = sqlx::query_scalar(
        "SELECT text FROM questions WHERE article_id = ?1
         UNION ALL SELECT question FROM suggestions WHERE article_id = ?1 AND status = 'open'",
    )
    .bind(article_id)
    .fetch_all(&state.db)
    .await?;
    let count = suggest::clamp_count(input.count);
    let (system, user) = suggest::generate_prompt(&article.context, &existing, count);
    let raw = ask(llm, &system, &user).await?;
    let (drafts, dropped) = suggest::validate(&article.context, &raw, Mode::Generate, &existing);
    let suggestions = save(
        &state,
        llm,
        article_id,
        drafts.into_iter().take(count),
        requested_by,
    )
    .await?;
    Ok(Json(SuggestResponse {
        suggestions,
        dropped,
    }))
}

#[derive(Deserialize)]
pub struct AnswerInput {
    question: String,
    annotator_id: Option<i64>,
}

/// 請模型替人寫好的問題找答案（或判斷無法回答）。
pub async fn answer(
    State(state): State<AppState>,
    actor: Actor,
    Path(article_id): Path<i64>,
    Json(input): Json<AnswerInput>,
) -> AppResult<Json<SuggestResponse>> {
    let llm = llm(&state)?;
    let requested_by = actor.annotator(&state.db, input.annotator_id).await?;
    let question = input.question.trim();
    if question.is_empty() {
        return Err(AppError::bad("先寫好問題，再請 AI 找答案"));
    }
    let article = store::article(&state.db, article_id).await?;
    let (system, user) = suggest::answer_prompt(&article.context, question);
    let raw = ask(llm, &system, &user).await?;
    let (drafts, dropped) = suggest::validate(&article.context, &raw, Mode::Answer, &[]);
    // 模型可能把問題改寫了一點；建議一律掛在人寫的問題上。
    let drafts = drafts.into_iter().take(1).map(|d| Draft {
        question: question.to_string(),
        ..d
    });
    let suggestions = save(&state, llm, article_id, drafts, requested_by).await?;
    Ok(Json(SuggestResponse {
        suggestions,
        dropped,
    }))
}

async fn ask(llm: &Llm, system: &str, user: &str) -> AppResult<serde_json::Value> {
    let schema = suggest::schema();
    let task = JsonTask {
        system,
        user,
        schema_name: suggest::SCHEMA_NAME,
        schema: &schema,
    };
    llm.generate_json(&task).await.map_err(|e| {
        tracing::warn!("AI 建議失敗：{e}");
        AppError::BadGateway(e.to_string())
    })
}

async fn save(
    state: &AppState,
    llm: &Llm,
    article_id: i64,
    drafts: impl Iterator<Item = Draft>,
    requested_by: Option<i64>,
) -> AppResult<Vec<Suggestion>> {
    let now = db::now();
    let mut out = Vec::new();
    for d in drafts {
        out.push(
            sqlx::query_as::<_, Suggestion>(&format!(
                "INSERT INTO suggestions (article_id, question, is_impossible, answer_start, answer_text,
                     reference_answer, provider, model, requested_by, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING {SUGGESTION_COLUMNS}"
            ))
            .bind(article_id)
            .bind(&d.question)
            .bind(d.is_impossible)
            .bind(d.answer_start.map(|s| s as i64))
            .bind(&d.answer_text)
            .bind(&d.reference_answer)
            .bind(llm.config.kind.id())
            .bind(&llm.config.model)
            .bind(requested_by)
            .bind(&now)
            .fetch_one(&state.db)
            .await?,
        );
    }
    Ok(out)
}

/// 接受建議。可以先修改：沒送的欄位照建議原樣。
#[derive(Deserialize)]
pub struct AcceptInput {
    annotator_id: Option<i64>,
    question: Option<String>,
    is_impossible: Option<bool>,
    reference_answer: Option<String>,
    /// 沒送就用建議的答案；送空陣列表示不要答案。
    answers: Option<Vec<SpanInput>>,
    /// 把答案加到這個已經存在的問題上（「替我寫好的問題找答案」時用），不另外建立問題。
    question_id: Option<i64>,
}

pub async fn accept(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<i64>,
    Json(input): Json<AcceptInput>,
) -> AppResult<Json<QuestionView>> {
    let s = suggestion(&state, id).await?;
    if s.status != SuggestionStatus::Open {
        return Err(AppError::conflict("這個建議已經處理過了"));
    }
    let article = store::article(&state.db, s.article_id).await?;
    let annotator = actor.annotator(&state.db, input.annotator_id).await?;
    let suggested: Vec<SpanInput> = match (s.answer_start, &s.answer_text) {
        (Some(start), Some(text)) => vec![SpanInput {
            start: start as usize,
            text: text.clone(),
        }],
        _ => Vec::new(),
    };
    let answers = input.answers.unwrap_or_else(|| suggested.clone());
    let spans = answers
        .iter()
        .map(|a| store::check_span(&article.context, a))
        .collect::<AppResult<Vec<Span>>>()?;
    let question_text = input.question.unwrap_or_else(|| s.question.clone());
    if question_text.trim().is_empty() {
        return Err(AppError::bad("問題不能是空的"));
    }
    let question_origin = if question_text.trim() == s.question {
        Origin::Ai
    } else {
        Origin::AiEdited
    };
    let answer_origin = |span: &Span| {
        let unchanged = suggested
            .iter()
            .any(|x| x.start == span.start && x.text == span.text);
        if unchanged {
            Origin::Ai
        } else {
            Origin::AiEdited
        }
    };

    if let Some(qid) = input.question_id {
        if store::question(&state.db, qid).await?.article_id != s.article_id {
            return Err(AppError::bad("這個問題不在建議的那篇文章裡"));
        }
    }

    let now = db::now();
    let mut tx = state.db.begin().await?;
    let question_id = match input.question_id {
        Some(qid) => qid,
        None => {
            store::insert_question(
                &mut tx,
                s.article_id,
                &NewQuestion {
                    text: &question_text,
                    is_impossible: input.is_impossible.unwrap_or(s.is_impossible),
                    reference_answer: input
                        .reference_answer
                        .as_deref()
                        .unwrap_or(&s.reference_answer),
                    origin: question_origin,
                    external_id: None,
                    created_by: annotator,
                    created_at: &now,
                },
            )
            .await?
        }
    };
    for span in &spans {
        store::insert_answer(
            &mut tx,
            question_id,
            span,
            annotator,
            answer_origin(span),
            &now,
        )
        .await?;
    }
    sqlx::query(
        "UPDATE suggestions SET status = 'accepted', question_id = ?, decided_at = ? WHERE id = ?",
    )
    .bind(question_id)
    .bind(&now)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(store::question_view(&state.db, question_id).await?))
}

pub async fn reject(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<Suggestion>> {
    let s = suggestion(&state, id).await?;
    if s.status != SuggestionStatus::Open {
        return Err(AppError::conflict("這個建議已經處理過了"));
    }
    sqlx::query("UPDATE suggestions SET status = 'rejected', decided_at = ? WHERE id = ?")
        .bind(db::now())
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(Json(suggestion(&state, id).await?))
}
