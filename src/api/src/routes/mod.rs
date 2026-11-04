//! HTTP API（規格見 `docs/api.md`）、舊版相容的 API 和前端靜態檔。

mod ai;
mod articles;
mod auth;
mod data;
mod projects;
mod questions;
mod users;

pub use data::import_file;

use axum::{
    extract::DefaultBodyLimit,
    http::{header, HeaderValue},
    middleware,
    routing::{delete, get, post},
    Json, Router,
};
use serde_json::{json, Value};
use tower_http::{
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

use crate::{auth::guard, dataset::EXPORT_FORMATS, error::AppError, legacy, state::AppState};

/// 列表 API 一頁最多幾筆；前端要更多就用 offset 往後翻。
pub const MAX_PAGE_SIZE: i64 = 200;
const DEFAULT_PAGE_SIZE: i64 = 50;

/// 把 `limit`、`offset` 查詢參數限制在合理範圍。
pub fn paginate(limit: Option<i64>, offset: Option<i64>) -> (i64, i64) {
    (
        limit.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE),
        offset.unwrap_or(0).max(0),
    )
}

pub fn router(state: AppState) -> Router {
    let upload_limit = state.config.max_upload_bytes;
    let api = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/auth/state", get(auth::state))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/users", get(users::list).post(users::create))
        .route("/users/{id}", axum::routing::put(users::update))
        .route("/meta", get(meta))
        .route(
            "/annotators",
            get(projects::list_annotators).post(projects::create_annotator),
        )
        .route("/projects", get(projects::list).post(projects::create))
        .route(
            "/projects/{id}",
            get(projects::get_one)
                .put(projects::update)
                .delete(projects::remove),
        )
        .route("/projects/{id}/stats", get(projects::stats))
        .route(
            "/projects/{id}/articles",
            get(articles::list).post(articles::create),
        )
        .route("/projects/{id}/articles/random", get(articles::random))
        .route("/projects/{id}/review", get(questions::review_queue))
        .route(
            "/projects/{id}/import",
            post(data::import).layer(DefaultBodyLimit::max(upload_limit)),
        )
        .route("/projects/{id}/export", get(data::export))
        .route(
            "/articles/{id}",
            get(articles::detail)
                .put(articles::update)
                .delete(articles::remove),
        )
        .route("/articles/{id}/questions", post(questions::create))
        .route(
            "/articles/{id}/suggestions",
            get(ai::list).post(ai::generate),
        )
        .route("/articles/{id}/suggestions/answer", post(ai::answer))
        .route("/suggestions/{id}/accept", post(ai::accept))
        .route("/suggestions/{id}/reject", post(ai::reject))
        .route(
            "/questions/{id}",
            get(questions::get_one)
                .put(questions::update)
                .delete(questions::remove),
        )
        .route("/questions/{id}/answers", post(questions::add_answer))
        .route("/questions/{id}/review", post(questions::review))
        .route("/answers/{id}", delete(questions::remove_answer))
        .fallback(|| async { AppError::not_found("找不到這個 API") });

    // 前端是單頁應用：找不到的路徑一律回 index.html，交給前端路由。
    let static_dir = state.config.static_dir.clone();
    let spa = ServeDir::new(&static_dir).fallback(ServeFile::new(static_dir.join("index.html")));

    Router::new()
        .nest("/api", api)
        .merge(legacy::router())
        // 登入、角色和同源檢查（沒啟用登入時不做事）。route_layer 只套在 API 上，前端檔案不用登入就拿得到。
        .route_layer(middleware::from_fn_with_state(state.clone(), guard))
        .fallback_service(spa)
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::REFERRER_POLICY,
            HeaderValue::from_static("same-origin"),
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// 版本、AI 狀態（只有 provider 和模型名稱，不含 key）和支援的匯出格式。
async fn meta(axum::extract::State(state): axum::extract::State<AppState>) -> Json<Value> {
    let ai = match &state.llm {
        Some(llm) => json!({
            "enabled": true,
            "provider": llm.config.kind.id(),
            "model": llm.config.model,
        }),
        None => json!({ "enabled": false, "reason": state.llm_disabled_reason() }),
    };
    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "ai": ai,
        "auth": { "enabled": state.auth.is_some() },
        "export_formats": EXPORT_FORMATS.iter().map(|f| f.id()).collect::<Vec<_>>(),
        "max_upload_mb": state.config.max_upload_bytes / 1024 / 1024,
    }))
}
