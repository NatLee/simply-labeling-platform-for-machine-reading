//! 機器閱讀理解（MRC）資料集的標注平台：管理文章、標註問題和答案範圍、
//! 多人審核，匯入匯出 SQuAD 等格式，並可以請模型先給標注建議。

pub mod auth;
pub mod config;
pub mod dataset;
pub mod db;
pub mod error;
pub mod legacy;
pub mod llm;
pub mod metrics;
pub mod models;
pub mod routes;
pub mod state;
pub mod store;
pub mod suggest;
pub mod text;

use std::sync::Arc;

use anyhow::Result;
use axum::Router;

pub use config::Config;
pub use state::{AppState, Inner};

/// 開資料庫（必要時建立並套用 migration），準備 AI 用的連線；有設定登入時確認管理員帳號。
pub async fn build_state(config: Config) -> Result<AppState> {
    let db = db::connect(&config.database_path).await?;
    let auth = match &config.auth {
        Some(settings) => {
            auth::bootstrap_admin(&db, settings).await?;
            tracing::info!("內建登入：啟用，管理員帳號 {}", settings.admin_username);
            Some(auth::AuthRuntime::new(settings.clone()).await?)
        }
        None => {
            tracing::info!("內建登入：關閉（沒有設定 ADMIN_PASSWORD），不用登入");
            None
        }
    };
    let state = Arc::new(Inner::new(db, config, auth)?);
    match (&state.llm, state.llm_disabled_reason()) {
        (Some(llm), _) => tracing::info!(
            "AI 輔助標注：{}，模型 {}",
            llm.config.kind.id(),
            llm.config.model
        ),
        (None, reason) => tracing::info!("AI 輔助標注關閉：{}", reason.unwrap_or_default()),
    }
    Ok(state)
}

pub fn app(state: AppState) -> Router {
    routes::router(state)
}
