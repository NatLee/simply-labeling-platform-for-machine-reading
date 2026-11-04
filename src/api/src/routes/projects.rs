use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::{
    error::{AppError, AppResult},
    models::{Annotator, Project, ProjectSummary},
    state::AppState,
    store::{self, ProjectStats},
};

#[derive(Deserialize)]
pub struct ProjectInput {
    name: String,
    #[serde(default)]
    description: String,
}

pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<ProjectSummary>>> {
    Ok(Json(store::project_summaries(&state.db).await?))
}

pub async fn create(
    State(state): State<AppState>,
    Json(input): Json<ProjectInput>,
) -> AppResult<(StatusCode, Json<Project>)> {
    let p = store::create_project(&state.db, &input.name, &input.description).await?;
    Ok((StatusCode::CREATED, Json(p)))
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<Project>> {
    Ok(Json(store::project(&state.db, id).await?))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<ProjectInput>,
) -> AppResult<Json<Project>> {
    store::project(&state.db, id).await?;
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::bad("專案名稱不能是空的"));
    }
    let res = sqlx::query("UPDATE projects SET name = ?, description = ? WHERE id = ?")
        .bind(name)
        .bind(input.description.trim())
        .bind(id)
        .execute(&state.db)
        .await;
    if let Err(sqlx::Error::Database(e)) = &res {
        if e.is_unique_violation() {
            return Err(AppError::conflict(format!("已經有叫「{name}」的專案")));
        }
    }
    res?;
    Ok(Json(store::project(&state.db, id).await?))
}

/// 刪除專案，連同裡面的文章、問題、答案和 AI 建議。
pub async fn remove(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<StatusCode> {
    store::project(&state.db, id).await?;
    sqlx::query("DELETE FROM projects WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn stats(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<ProjectStats>> {
    store::project(&state.db, id).await?;
    Ok(Json(store::project_stats(&state.db, id).await?))
}

pub async fn list_annotators(State(state): State<AppState>) -> AppResult<Json<Vec<Annotator>>> {
    Ok(Json(store::annotators(&state.db).await?))
}

#[derive(Deserialize)]
pub struct AnnotatorInput {
    name: String,
}

pub async fn create_annotator(
    State(state): State<AppState>,
    Json(input): Json<AnnotatorInput>,
) -> AppResult<Json<Annotator>> {
    Ok(Json(store::create_annotator(&state.db, &input.name).await?))
}
