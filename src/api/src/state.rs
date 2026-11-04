use std::sync::Arc;

use sqlx::SqlitePool;

use crate::{
    auth::AuthRuntime,
    config::Config,
    llm::{Llm, LlmSettings},
};

pub type AppState = Arc<Inner>;

pub struct Inner {
    pub db: SqlitePool,
    pub config: Config,
    /// AI 輔助標注；沒設定時是 `None`，相關 API 回 503 並說明原因。
    pub llm: Option<Llm>,
    /// 內建登入；沒設定 `ADMIN_PASSWORD` 時是 `None`。
    pub auth: Option<AuthRuntime>,
}

impl Inner {
    pub fn new(db: SqlitePool, config: Config, auth: Option<AuthRuntime>) -> anyhow::Result<Self> {
        let llm = match &config.llm {
            LlmSettings::Enabled(cfg) => Some(Llm::new(cfg.clone())?),
            LlmSettings::Disabled { .. } => None,
        };
        Ok(Self {
            db,
            config,
            llm,
            auth,
        })
    }

    /// AI 功能關閉的原因；開啟時是 `None`。
    pub fn llm_disabled_reason(&self) -> Option<&str> {
        match &self.config.llm {
            LlmSettings::Enabled(_) => None,
            LlmSettings::Disabled { reason } => Some(reason),
        }
    }
}
