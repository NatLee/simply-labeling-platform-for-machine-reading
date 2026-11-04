use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use super::paginate;
use crate::{
    auth::Actor,
    db,
    error::{AppError, AppResult},
    models::{Origin, QuestionView, ReviewStatus, SpanInput},
    state::AppState,
    store::{self, NewQuestion, ReviewPage},
};

#[derive(Deserialize)]
pub struct QuestionInput {
    text: String,
    #[serde(default)]
    is_impossible: bool,
    #[serde(default)]
    reference_answer: String,
    #[serde(default)]
    answers: Vec<SpanInput>,
    annotator_id: Option<i64>,
}

/// 新增問題和它的答案。每個答案的位置都會檢查，有一個對不上就整題不存。
pub async fn create(
    State(state): State<AppState>,
    actor: Actor,
    Path(article_id): Path<i64>,
    Json(input): Json<QuestionInput>,
) -> AppResult<(StatusCode, Json<QuestionView>)> {
    let article = store::article(&state.db, article_id).await?;
    let annotator = actor.annotator(&state.db, input.annotator_id).await?;
    if input.text.trim().is_empty() {
        return Err(AppError::bad("問題不能是空的"));
    }
    let spans = input
        .answers
        .iter()
        .map(|s| store::check_span(&article.context, s))
        .collect::<AppResult<Vec<_>>>()?;
    let now = db::now();
    let mut tx = state.db.begin().await?;
    let id = store::insert_question(
        &mut tx,
        article_id,
        &NewQuestion {
            text: &input.text,
            is_impossible: input.is_impossible,
            reference_answer: &input.reference_answer,
            origin: Origin::Human,
            external_id: None,
            created_by: annotator,
            created_at: &now,
        },
    )
    .await?;
    for span in &spans {
        store::insert_answer(&mut tx, id, span, annotator, Origin::Human, &now).await?;
    }
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(store::question_view(&state.db, id).await?),
    ))
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<QuestionView>> {
    Ok(Json(store::question_view(&state.db, id).await?))
}

#[derive(Deserialize)]
pub struct QuestionUpdate {
    text: Option<String>,
    is_impossible: Option<bool>,
    reference_answer: Option<String>,
}

/// 改問題文字、無法回答、參考答案。答案用 `/questions/{id}/answers` 和 `/answers/{id}` 增刪，
/// 這樣不會動到別人標的答案。
pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<QuestionUpdate>,
) -> AppResult<Json<QuestionView>> {
    let current = store::question(&state.db, id).await?;
    let text = input.text.unwrap_or(current.text);
    if text.trim().is_empty() {
        return Err(AppError::bad("問題不能是空的"));
    }
    sqlx::query(
        "UPDATE questions SET text = ?, is_impossible = ?, reference_answer = ?, updated_at = ? WHERE id = ?",
    )
    .bind(text.trim())
    .bind(input.is_impossible.unwrap_or(current.is_impossible))
    .bind(input.reference_answer.unwrap_or(current.reference_answer).trim())
    .bind(db::now())
    .bind(id)
    .execute(&state.db)
    .await?;
    Ok(Json(store::question_view(&state.db, id).await?))
}

pub async fn remove(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<StatusCode> {
    store::question(&state.db, id).await?;
    sqlx::query("DELETE FROM questions WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct AnswerInput {
    #[serde(flatten)]
    span: SpanInput,
    annotator_id: Option<i64>,
}

/// 加一個答案（例如第二位標註者對同一題作答）。同一個人標一樣的範圍兩次只會留一筆。
pub async fn add_answer(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<i64>,
    Json(input): Json<AnswerInput>,
) -> AppResult<Json<QuestionView>> {
    let q = store::question(&state.db, id).await?;
    let article = store::article(&state.db, q.article_id).await?;
    let annotator = actor.annotator(&state.db, input.annotator_id).await?;
    let span = store::check_span(&article.context, &input.span)?;
    let mut conn = state.db.acquire().await?;
    store::insert_answer(&mut conn, id, &span, annotator, Origin::Human, &db::now()).await?;
    sqlx::query("UPDATE questions SET updated_at = ? WHERE id = ?")
        .bind(db::now())
        .bind(id)
        .execute(&mut *conn)
        .await?;
    drop(conn);
    Ok(Json(store::question_view(&state.db, id).await?))
}

pub async fn remove_answer(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    let res = sqlx::query("DELETE FROM answers WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("找不到這個答案"));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct ReviewInput {
    status: ReviewStatus,
    annotator_id: Option<i64>,
    #[serde(default)]
    note: String,
}

/// 審核：通過、退回，或改回待審核。
pub async fn review(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<i64>,
    Json(input): Json<ReviewInput>,
) -> AppResult<Json<QuestionView>> {
    store::question(&state.db, id).await?;
    let reviewer = actor.annotator(&state.db, input.annotator_id).await?;
    let (reviewer, reviewed_at) = match input.status {
        ReviewStatus::Pending => (None, None),
        _ => (reviewer, Some(db::now())),
    };
    sqlx::query(
        "UPDATE questions SET status = ?, reviewed_by = ?, reviewed_at = ?, review_note = ? WHERE id = ?",
    )
    .bind(input.status)
    .bind(reviewer)
    .bind(reviewed_at)
    .bind(input.note.trim())
    .bind(id)
    .execute(&state.db)
    .await?;
    Ok(Json(store::question_view(&state.db, id).await?))
}

#[derive(Deserialize)]
pub struct ReviewQuery {
    status: Option<ReviewStatus>,
    limit: Option<i64>,
    offset: Option<i64>,
}

pub async fn review_queue(
    State(state): State<AppState>,
    Path(project_id): Path<i64>,
    Query(query): Query<ReviewQuery>,
) -> AppResult<Json<ReviewPage>> {
    store::project(&state.db, project_id).await?;
    let (limit, offset) = paginate(query.limit, query.offset);
    Ok(Json(
        store::review_page(&state.db, project_id, query.status, limit, offset).await?,
    ))
}
