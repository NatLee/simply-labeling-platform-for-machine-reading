//! 和舊版 Flask 平台相容的部分，集中在這裡。
//!
//! 1. 讀舊版的 SQLite 資料庫（`mrc.label`），轉成匯入用的資料集。
//! 2. 舊版的 3 個 HTTP API：`GET /article`、`POST /article`、`POST /question-answer`。
//!    保留它們是為了讓原本寫好的腳本不用改；新程式請用 `/api/...`（見 README）。
//!    之後要拿掉時，刪掉 [`router`] 和 `routes/mod.rs` 裡的 `.merge(legacy::router())` 就好。
//!    啟用內建登入時，這 3 個 API 也要登入（帶 session cookie），權限見 [`crate::auth::required_level`]。

use std::path::Path;

use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Form, Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};

use crate::{
    auth::Actor,
    dataset::{DatasetArticle, DatasetQa, Span},
    db,
    error::AppResult,
    models::{Origin, Project},
    state::AppState,
    store,
    text::OffsetUnit,
};

/// 舊版 API 寫入的專案：最早建立的那一個；一個都沒有時建立「預設專案」。
pub const DEFAULT_PROJECT_NAME: &str = "預設專案";

/// 舊版資料庫的答案位置是瀏覽器 textarea 的 `selectionStart`，單位是 UTF-16。
pub const LEGACY_OFFSET_UNIT: OffsetUnit = OffsetUnit::Utf16;

/// 讀舊版資料庫。只讀不寫，原檔不會被改到。
///
/// 舊版的已知問題在這裡一併處理：
/// - 沒選答案的列會存成答案 `0`、起點 0：這種答案不匯入，問題留著等人補答案。
/// - 問題是空的：舊版前端讀不到問題時會送出空白，略過。
pub async fn read_database(path: &Path) -> Result<Vec<DatasetArticle>> {
    let options = SqliteConnectOptions::new().filename(path).read_only(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .with_context(|| format!("開不了舊版資料庫：{}", path.display()))?;
    let result = read_pool(&pool).await;
    pool.close().await;
    result
}

/// 舊版 `question_answer` 的一列。欄位都可能是 NULL 或型別不對（舊版前端會送 0 當答案），所以全部用 Option。
#[derive(sqlx::FromRow)]
struct LegacyQaRow {
    article_id: i64,
    question: Option<String>,
    answer_start: Option<i64>,
    answer_string: Option<String>,
    timestamp: Option<String>,
}

async fn read_pool(pool: &SqlitePool) -> Result<Vec<DatasetArticle>> {
    let articles: Vec<(i64, String, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT id, context, timestamp, description FROM article ORDER BY id")
            .fetch_all(pool)
            .await
            .context("找不到舊版的 article 資料表，這不是舊版平台的資料庫")?;
    let qas: Vec<LegacyQaRow> = sqlx::query_as(
        "SELECT article_id, question, answer_start, CAST(answer_string AS TEXT) AS answer_string, timestamp
         FROM question_answer ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .context("找不到舊版的 question_answer 資料表")?;

    let mut out: Vec<DatasetArticle> = Vec::with_capacity(articles.len());
    let mut index = std::collections::HashMap::new();
    for (id, context, timestamp, description) in articles {
        index.insert(id, out.len());
        out.push(DatasetArticle {
            title: String::new(),
            context,
            description: description.unwrap_or_default(),
            created_at: Some(db::to_utc(timestamp.as_deref())),
            qas: Vec::new(),
        });
    }
    for LegacyQaRow {
        article_id,
        question,
        answer_start: start,
        answer_string: answer,
        timestamp,
    } in qas
    {
        let Some(&i) = index.get(&article_id) else {
            continue;
        };
        let Some(question) = question.filter(|q| !q.trim().is_empty()) else {
            continue;
        };
        let answer = answer.unwrap_or_default();
        let answers = match start {
            // 舊版沒選答案時存的是 0 / 「0」
            Some(start) if start >= 0 && !answer.is_empty() && answer != "0" => vec![Span {
                start: start as usize,
                text: answer,
            }],
            _ => Vec::new(),
        };
        out[i].qas.push(DatasetQa {
            id: None,
            question,
            is_impossible: false,
            answers,
            reference_answer: String::new(),
            created_at: Some(db::to_utc(timestamp.as_deref())),
        });
    }
    Ok(out)
}

/// 舊版 API 用的專案。
pub async fn default_project(db: &SqlitePool) -> AppResult<Project> {
    let oldest = sqlx::query_as::<_, Project>(
        "SELECT id, name, description, created_at FROM projects ORDER BY id LIMIT 1",
    )
    .fetch_optional(db)
    .await?;
    match oldest {
        Some(p) => Ok(p),
        None => store::project_by_name_or_create(db, DEFAULT_PROJECT_NAME).await,
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/article",
            get(legacy_random_article).post(legacy_insert_article),
        )
        .route("/question-answer", post(legacy_insert_qas))
}

/// 舊版：`{"article_id", "article", "timestamp"}`；沒有文章時兩個欄位都是 null。
async fn legacy_random_article(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let project = default_project(&state.db).await?;
    let article = store::random_article(&state.db, project.id, None, false).await?;
    Ok(Json(json!({
        "article_id": article.as_ref().map(|a| a.id),
        "article": article.as_ref().map(|a| a.context.clone()),
        "timestamp": db::now(),
    })))
}

#[derive(Deserialize)]
struct LegacyArticleForm {
    article: Option<String>,
    description: Option<String>,
}

/// 舊版：表單欄位 `article`（必填）、`description`。
async fn legacy_insert_article(
    State(state): State<AppState>,
    Form(form): Form<LegacyArticleForm>,
) -> AppResult<impl IntoResponse> {
    let Some(article) = form.article.filter(|a| !a.trim().is_empty()) else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"status": "error", "message": "Article or description is missing."})),
        ));
    };
    let project = default_project(&state.db).await?;
    let mut conn = state.db.acquire().await?;
    // 同一篇文章已經在專案裡時不會重複建立，回應和舊版一樣是成功。
    store::insert_article(
        &mut conn,
        project.id,
        "",
        &article,
        form.description.as_deref().unwrap_or(""),
        &db::now(),
    )
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!({"status": "success", "message": "Article inserted successfully."})),
    ))
}

#[derive(Deserialize)]
struct LegacyQa {
    article_id: Option<i64>,
    question: Option<String>,
    answer_start: Option<i64>,
    answer_string: Option<Value>,
}

/// 舊版：JSON 陣列，每筆 `article_id`、`question`、`answer_start`（UTF-16 位置）、`answer_string`。
/// 和舊版一樣略過不完整的筆數；位置對不上時找最近的同樣文字，找不到就只存問題。
async fn legacy_insert_qas(
    State(state): State<AppState>,
    actor: Actor,
    Json(rows): Json<Vec<LegacyQa>>,
) -> AppResult<Json<Value>> {
    let project = default_project(&state.db).await?;
    // 啟用登入時記成登入的人；沒啟用時和舊版一樣是匿名
    let created_by = actor.annotator(&state.db, None).await?;
    for row in rows {
        let (Some(article_id), Some(question), Some(start)) =
            (row.article_id, row.question, row.answer_start)
        else {
            continue;
        };
        let Ok(article) = store::article(&state.db, article_id).await else {
            continue;
        };
        if article.project_id != project.id {
            continue;
        }
        let answer = match row.answer_string {
            Some(Value::String(s)) => s,
            _ => String::new(),
        };
        let qa = DatasetQa {
            question,
            answers: if answer.is_empty() || start < 0 {
                Vec::new()
            } else {
                vec![Span {
                    start: start as usize,
                    text: answer,
                }]
            },
            ..Default::default()
        };
        let dataset = vec![DatasetArticle {
            title: article.title,
            context: article.context,
            qas: vec![qa],
            ..Default::default()
        }];
        store::import_dataset(
            &state.db,
            project.id,
            dataset,
            LEGACY_OFFSET_UNIT,
            Origin::Human,
            created_by,
            "legacy-api",
        )
        .await?;
    }
    Ok(Json(json!({"success": true})))
}
