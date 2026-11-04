use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::paginate;
use crate::{
    db,
    error::{AppError, AppResult},
    models::{Article, ArticleDetail},
    state::AppState,
    store::{self, ArticleFilter, ArticlePage, Inserted},
};

#[derive(Deserialize)]
pub struct ListQuery {
    q: Option<String>,
    filter: Option<ArticleFilter>,
    limit: Option<i64>,
    offset: Option<i64>,
}

pub async fn list(
    State(state): State<AppState>,
    Path(project_id): Path<i64>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<ArticlePage>> {
    store::project(&state.db, project_id).await?;
    let (limit, offset) = paginate(query.limit, query.offset);
    Ok(Json(
        store::list_articles(
            &state.db,
            project_id,
            query.q.as_deref(),
            query.filter,
            limit,
            offset,
        )
        .await?,
    ))
}

#[derive(Deserialize)]
pub struct ArticleInput {
    #[serde(default)]
    title: String,
    context: String,
    #[serde(default)]
    description: String,
}

/// 新增文章。專案裡已經有一樣的文章時不重複建立，回 200 和那篇的 id（`created: false`）。
pub async fn create(
    State(state): State<AppState>,
    Path(project_id): Path<i64>,
    Json(input): Json<ArticleInput>,
) -> AppResult<(StatusCode, Json<Value>)> {
    store::project(&state.db, project_id).await?;
    let mut conn = state.db.acquire().await?;
    let inserted = store::insert_article(
        &mut conn,
        project_id,
        &input.title,
        &input.context,
        &input.description,
        &db::now(),
    )
    .await?;
    Ok(match inserted {
        Inserted::Created(id) => (
            StatusCode::CREATED,
            Json(json!({ "id": id, "created": true })),
        ),
        Inserted::Existing(id) => (StatusCode::OK, Json(json!({ "id": id, "created": false }))),
    })
}

#[derive(Deserialize)]
pub struct RandomQuery {
    exclude: Option<i64>,
    #[serde(default)]
    unlabeled: bool,
}

pub async fn random(
    State(state): State<AppState>,
    Path(project_id): Path<i64>,
    Query(query): Query<RandomQuery>,
) -> AppResult<Json<Article>> {
    store::project(&state.db, project_id).await?;
    store::random_article(&state.db, project_id, query.exclude, query.unlabeled)
        .await?
        .map(Json)
        .ok_or_else(|| {
            AppError::not_found(if query.unlabeled {
                "沒有還沒標註的文章了"
            } else {
                "這個專案還沒有文章"
            })
        })
}

pub async fn detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<ArticleDetail>> {
    Ok(Json(store::article_detail(&state.db, id).await?))
}

#[derive(Deserialize)]
pub struct ArticleUpdate {
    title: Option<String>,
    description: Option<String>,
}

/// 只能改標題和說明。文章內容改了，答案的位置就會對不上，所以不開放修改。
pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<ArticleUpdate>,
) -> AppResult<Json<ArticleDetail>> {
    let current = store::article(&state.db, id).await?;
    sqlx::query("UPDATE articles SET title = ?, description = ? WHERE id = ?")
        .bind(input.title.as_deref().unwrap_or(&current.title).trim())
        .bind(
            input
                .description
                .as_deref()
                .unwrap_or(&current.description)
                .trim(),
        )
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(Json(store::article_detail(&state.db, id).await?))
}

pub async fn remove(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<StatusCode> {
    store::article(&state.db, id).await?;
    sqlx::query("DELETE FROM articles WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
