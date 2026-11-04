//! 帳號管理（只有管理員能用，權限見 `crate::auth::required_level`）。帳號只能新增和停用，不能刪除，
//! 因為問題和答案上記著是誰標的。

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::{self, Actor, Role},
    error::{AppError, AppResult},
    state::AppState,
};

#[derive(Serialize, sqlx::FromRow)]
pub struct User {
    id: i64,
    username: String,
    role: Role,
    disabled: bool,
    annotator_id: i64,
    created_at: String,
}

const COLUMNS: &str = "id, username, role, disabled, annotator_id, created_at";

async fn user(state: &AppState, id: i64) -> AppResult<User> {
    sqlx::query_as::<_, User>(&format!("SELECT {COLUMNS} FROM users WHERE id = ?"))
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("找不到這個帳號"))
}

pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<User>>> {
    Ok(Json(
        sqlx::query_as::<_, User>(&format!("SELECT {COLUMNS} FROM users ORDER BY username"))
            .fetch_all(&state.db)
            .await?,
    ))
}

#[derive(Deserialize)]
pub struct NewUser {
    username: String,
    password: String,
    role: Role,
}

pub async fn create(
    State(state): State<AppState>,
    Json(input): Json<NewUser>,
) -> AppResult<(StatusCode, Json<User>)> {
    let id = auth::create_user(&state.db, &input.username, &input.password, input.role).await?;
    Ok((StatusCode::CREATED, Json(user(&state, id).await?)))
}

#[derive(Deserialize)]
pub struct UserUpdate {
    role: Option<Role>,
    disabled: Option<bool>,
    password: Option<String>,
}

/// 改角色、停用或啟用、重設密碼。停用和重設密碼會登出那個人所有的裝置。
/// 不能停用自己或拿掉自己的管理員權限；`ADMIN_USERNAME` 的帳號每次啟動都會恢復成管理員，所以也不讓人改。
pub async fn update(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<i64>,
    Json(input): Json<UserUpdate>,
) -> AppResult<Json<User>> {
    let target = user(&state, id).await?;
    let changes_access =
        input.disabled == Some(true) || input.role.is_some_and(|r| r != Role::Admin);
    if changes_access {
        if actor.0.as_ref().is_some_and(|me| me.id == id) {
            return Err(AppError::bad("不能停用自己，或拿掉自己的管理員權限"));
        }
        let is_env_admin = state.auth.as_ref().is_some_and(|rt| {
            rt.settings
                .admin_username
                .eq_ignore_ascii_case(&target.username)
        });
        if is_env_admin {
            return Err(AppError::bad(
                "這個管理員帳號由 ADMIN_USERNAME 設定，不能停用或改角色",
            ));
        }
    }
    if let Some(pw) = &input.password {
        auth::validate_password(pw)?;
        sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
            .bind(auth::hash_password(pw.clone()).await?)
            .bind(id)
            .execute(&state.db)
            .await?;
    }
    sqlx::query(
        "UPDATE users SET role = COALESCE(?, role), disabled = COALESCE(?, disabled) WHERE id = ?",
    )
    .bind(input.role)
    .bind(input.disabled)
    .bind(id)
    .execute(&state.db)
    .await?;
    if input.password.is_some() || input.disabled == Some(true) {
        auth::delete_sessions_of(&state.db, id).await?;
    }
    Ok(Json(user(&state, id).await?))
}
