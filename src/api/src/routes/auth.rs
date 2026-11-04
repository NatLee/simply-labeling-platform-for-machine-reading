//! 登入、登出、目前登入的人。權限檢查在 `crate::auth::guard`。

use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    auth::{self, Actor},
    error::{AppError, AppResult},
    state::AppState,
};

/// `{"enabled": 有沒有啟用登入, "user": 登入的人或 null}`。前端用它決定要不要顯示登入頁。
pub async fn state(State(state): State<AppState>, actor: Actor) -> Json<Value> {
    Json(json!({ "enabled": state.auth.is_some(), "user": actor.0 }))
}

#[derive(Deserialize)]
pub struct LoginInput {
    username: String,
    password: String,
}

pub async fn login(
    State(state): State<AppState>,
    Json(input): Json<LoginInput>,
) -> AppResult<impl IntoResponse> {
    let rt = state
        .auth
        .as_ref()
        .ok_or_else(|| AppError::not_found("這個平台沒有啟用登入"))?;
    let (user, token) = auth::login(&state.db, rt, &input.username, &input.password).await?;
    let cookie = auth::session_cookie(&token, rt.settings.cookie_secure);
    Ok((
        [(header::SET_COOKIE, cookie)],
        Json(json!({ "user": user })),
    ))
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<impl IntoResponse> {
    let secure = state
        .auth
        .as_ref()
        .is_some_and(|rt| rt.settings.cookie_secure);
    auth::logout(&state.db, &headers).await?;
    Ok((
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, auth::clear_cookie(secure))],
    ))
}
