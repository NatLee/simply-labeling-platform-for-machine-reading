//! 設定都來自環境變數，全部都有預設值。AI 相關的變數見 [`crate::llm::LlmSettings`]。

use std::{env, path::PathBuf};

use anyhow::{Context, Result};

use crate::llm::LlmSettings;

/// 舊版 Flask 平台用的 port，沿用當預設值。
pub const DEFAULT_BIND: &str = "0.0.0.0:8787";

#[derive(Clone, Debug)]
pub struct Config {
    pub bind: String,
    pub data_dir: PathBuf,
    pub database_path: PathBuf,
    pub static_dir: PathBuf,
    /// 匯入檔案的大小上限。SQuAD 2.0 的訓練集約 40 MB。
    pub max_upload_bytes: usize,
    pub llm: LlmSettings,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let data_dir = PathBuf::from(var("MRC_DATA_DIR").unwrap_or_else(|| "data".into()));
        let database_path = var("MRC_DATABASE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| data_dir.join("mrc-label.db"));
        let max_upload_mb: usize = parse_var("MRC_MAX_UPLOAD_MB", 200)?;
        Ok(Self {
            bind: var("MRC_BIND").unwrap_or_else(|| DEFAULT_BIND.into()),
            database_path,
            static_dir: PathBuf::from(
                var("MRC_STATIC_DIR").unwrap_or_else(|| "src/web/dist".into()),
            ),
            max_upload_bytes: max_upload_mb * 1024 * 1024,
            llm: LlmSettings::from_env(var),
            data_dir,
        })
    }
}

/// 讀環境變數；沒設定或只有空白都當成沒設定。
pub fn var(name: &str) -> Option<String> {
    env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn parse_var<T: std::str::FromStr>(name: &str, default: T) -> Result<T>
where
    T::Err: std::error::Error + Send + Sync + 'static,
{
    match var(name) {
        Some(v) => v
            .trim()
            .parse()
            .with_context(|| format!("{name} 的值不合法：{v}")),
        None => Ok(default),
    }
}
