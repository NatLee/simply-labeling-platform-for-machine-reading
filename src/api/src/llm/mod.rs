//! AI 輔助標注用的模型服務。
//!
//! Claude API（原生的 Messages API）和 OpenAI 相容 API（OpenAI、Ollama、vLLM、LM Studio…）
//! 放在同一個介面後面：呼叫端只用 [`Llm::generate_json`]，不需要知道是哪一家。
//! 用 `LLM_PROVIDER` 選；沒設定或設定不完整時 AI 功能關閉，其他功能照常。
//! API key 只從環境變數讀，不寫進資料庫，也不回傳給前端。

mod anthropic;
mod openai;

use std::{fmt, time::Duration};

use reqwest::{header::RETRY_AFTER, RequestBuilder, StatusCode};
use serde::Serialize;
use serde_json::Value;

pub use crate::config::Secret;

pub use anthropic::{
    API_VERSION as ANTHROPIC_API_VERSION, DEFAULT_MODEL as ANTHROPIC_DEFAULT_MODEL,
};

pub const ANTHROPIC_DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
pub const OPENAI_DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
/// 一次請求最多等多久。模型會先思考再回答，長文章可能要一兩分鐘。
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(180);
/// 遇到 429、5xx 或連線失敗時最多試幾次（含第一次）。
const MAX_ATTEMPTS: u32 = 3;
/// 服務要求等待（`retry-after`）時最多等幾秒。
const MAX_RETRY_AFTER_SECS: u64 = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Anthropic,
    OpenAi,
}

impl ProviderKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAi => "openai",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub model: String,
    pub base_url: String,
    pub api_key: Option<Secret>,
}

#[derive(Clone, Debug)]
pub enum LlmSettings {
    Enabled(ProviderConfig),
    Disabled { reason: String },
}

impl LlmSettings {
    /// 從環境變數讀設定。`get` 通常是 [`crate::config::var`]，測試時可以換成假的。
    ///
    /// | 變數 | 說明 |
    /// |---|---|
    /// | `LLM_PROVIDER` | `anthropic` 或 `openai`；沒設定就關閉 AI 功能 |
    /// | `ANTHROPIC_API_KEY` | 用 Claude 時必填 |
    /// | `ANTHROPIC_MODEL` | 預設 `claude-opus-5-5` |
    /// | `ANTHROPIC_BASE_URL` | 預設 `https://api.anthropic.com`（測試或代理用） |
    /// | `OPENAI_BASE_URL` | 預設 `https://api.openai.com/v1`；Ollama 是 `http://<主機>:11434/v1` |
    /// | `OPENAI_API_KEY` | 本機的 Ollama、vLLM 通常不需要 |
    /// | `OPENAI_MODEL` | 用 OpenAI 相容 API 時必填 |
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Self {
        let disabled = |reason: &str| Self::Disabled {
            reason: reason.to_string(),
        };
        let Some(provider) = get("LLM_PROVIDER") else {
            return disabled("沒有設定 LLM_PROVIDER");
        };
        match provider.trim().to_ascii_lowercase().as_str() {
            "anthropic" => {
                let Some(key) = get("ANTHROPIC_API_KEY") else {
                    return disabled("LLM_PROVIDER=anthropic，但沒有設定 ANTHROPIC_API_KEY");
                };
                Self::Enabled(ProviderConfig {
                    kind: ProviderKind::Anthropic,
                    model: get("ANTHROPIC_MODEL").unwrap_or_else(|| ANTHROPIC_DEFAULT_MODEL.into()),
                    base_url: get("ANTHROPIC_BASE_URL")
                        .unwrap_or_else(|| ANTHROPIC_DEFAULT_BASE_URL.into()),
                    api_key: Some(Secret::new(key)),
                })
            }
            "openai" => {
                let Some(model) = get("OPENAI_MODEL") else {
                    return disabled("LLM_PROVIDER=openai，但沒有設定 OPENAI_MODEL");
                };
                Self::Enabled(ProviderConfig {
                    kind: ProviderKind::OpenAi,
                    model,
                    base_url: get("OPENAI_BASE_URL")
                        .unwrap_or_else(|| OPENAI_DEFAULT_BASE_URL.into()),
                    api_key: get("OPENAI_API_KEY").map(Secret::new),
                })
            }
            other => Self::Disabled {
                reason: format!("LLM_PROVIDER 只能是 anthropic 或 openai，現在是「{other}」"),
            },
        }
    }
}

/// 要模型回一段符合 JSON Schema 的資料。
pub struct JsonTask<'a> {
    pub system: &'a str,
    pub user: &'a str,
    pub schema_name: &'a str,
    pub schema: &'a Value,
}

#[derive(Debug, PartialEq)]
pub enum LlmError {
    Auth(u16),
    Status {
        status: u16,
        message: String,
    },
    /// 模型拒絕回答（Claude 的 `stop_reason: "refusal"`，或 OpenAI 的 `refusal`）。
    Refused(Option<String>),
    /// 回答超過長度上限。
    Truncated,
    BadOutput(String),
    Network(String),
}

impl fmt::Display for LlmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auth(s) => write!(f, "模型服務拒絕了 API key（HTTP {s}），請檢查金鑰和權限"),
            Self::Status { status, message } => write!(f, "模型服務回應 HTTP {status}：{message}"),
            Self::Refused(Some(c)) => write!(f, "模型拒絕回答這個請求（類別：{c}）"),
            Self::Refused(None) => write!(f, "模型拒絕回答這個請求"),
            Self::Truncated => write!(f, "模型的回答超過長度上限，被截斷了"),
            Self::BadOutput(m) => write!(f, "模型的回答不能用：{m}"),
            Self::Network(m) => write!(f, "連不到模型服務：{m}"),
        }
    }
}

impl std::error::Error for LlmError {}

pub struct Llm {
    pub config: ProviderConfig,
    http: reqwest::Client,
}

impl Llm {
    pub fn new(config: ProviderConfig) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(concat!("mrc-label/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { config, http })
    }

    /// 送出請求，回傳模型給的 JSON。
    pub async fn generate_json(&self, task: &JsonTask<'_>) -> Result<Value, LlmError> {
        let text = match self.config.kind {
            ProviderKind::Anthropic => anthropic::complete(self, task).await?,
            ProviderKind::OpenAi => openai::complete(self, task).await?,
        };
        parse_json_text(&text)
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}/{path}", self.config.base_url.trim_end_matches('/'))
    }

    /// 送出 JSON 請求；429、5xx 和連線失敗會等一下再試。
    async fn send(&self, req: RequestBuilder) -> Result<Value, LlmError> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            let req = req.try_clone().expect("JSON request bodies can be cloned");
            let wait = match req.send().await {
                Ok(resp) if resp.status().is_success() => {
                    return resp
                        .json::<Value>()
                        .await
                        .map_err(|e| LlmError::BadOutput(format!("回應不是 JSON：{e}")));
                }
                Ok(resp) => {
                    let status = resp.status();
                    let retry_after = resp
                        .headers()
                        .get(RETRY_AFTER)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.trim().parse::<u64>().ok());
                    let body = resp.text().await.unwrap_or_default();
                    if !retryable(status) || attempt >= MAX_ATTEMPTS {
                        return Err(status_error(status, &body));
                    }
                    backoff(attempt, retry_after)
                }
                Err(e) if e.is_connect() && attempt < MAX_ATTEMPTS => backoff(attempt, None),
                Err(e) if e.is_timeout() => {
                    return Err(LlmError::Network(format!(
                        "等了 {} 秒還沒有回應",
                        REQUEST_TIMEOUT.as_secs()
                    )))
                }
                Err(e) => return Err(LlmError::Network(e.to_string())),
            };
            tracing::warn!(
                "模型服務暫時失敗，{} 毫秒後重試（第 {attempt} 次）",
                wait.as_millis()
            );
            tokio::time::sleep(wait).await;
        }
    }
}

fn retryable(status: StatusCode) -> bool {
    matches!(status.as_u16(), 408 | 409 | 429) || status.is_server_error()
}

fn backoff(attempt: u32, retry_after: Option<u64>) -> Duration {
    match retry_after {
        Some(secs) => Duration::from_secs(secs.min(MAX_RETRY_AFTER_SECS)),
        None => Duration::from_millis(500 * 2u64.pow(attempt - 1)),
    }
}

/// Anthropic 和 OpenAI 的錯誤都是 `{"error": {"message": "..."}}`；Ollama 有時是 `{"error": "..."}`。
fn status_error(status: StatusCode, body: &str) -> LlmError {
    if matches!(status.as_u16(), 401 | 403) {
        return LlmError::Auth(status.as_u16());
    }
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let message = parsed
        .as_ref()
        .and_then(|v| {
            v["error"]["message"]
                .as_str()
                .or_else(|| v["error"].as_str())
                .map(String::from)
        })
        .unwrap_or_else(|| body.chars().take(300).collect());
    LlmError::Status {
        status: status.as_u16(),
        message,
    }
}

/// 從模型的文字回答取出 JSON。多數服務會照 schema 回純 JSON；
/// 小模型偶爾會包在 ```json 區塊裡，或前後多一句話，這裡一併處理。
pub fn parse_json_text(text: &str) -> Result<Value, LlmError> {
    let t = text.trim();
    let t = t
        .strip_prefix("```json")
        .or_else(|| t.strip_prefix("```"))
        .map(|rest| rest.trim_end().trim_end_matches("```"))
        .unwrap_or(t)
        .trim();
    if let Ok(v) = serde_json::from_str(t) {
        return Ok(v);
    }
    if let (Some(s), Some(e)) = (t.find('{'), t.rfind('}')) {
        if s < e {
            if let Ok(v) = serde_json::from_str(&t[s..=e]) {
                return Ok(v);
            }
        }
    }
    let preview: String = t.chars().take(200).collect();
    Err(LlmError::BadOutput(format!("不是 JSON：{preview}")))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::*;

    fn settings(vars: &[(&str, &str)]) -> LlmSettings {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        LlmSettings::from_env(|k| map.get(k).cloned())
    }

    #[test]
    fn ai_is_off_without_a_provider() {
        assert!(matches!(settings(&[]), LlmSettings::Disabled { .. }));
        // 只有 key 不夠，要明確選 provider
        assert!(matches!(
            settings(&[("ANTHROPIC_API_KEY", "k")]),
            LlmSettings::Disabled { .. }
        ));
    }

    #[test]
    fn anthropic_needs_a_key_and_defaults_the_model() {
        let LlmSettings::Disabled { reason } = settings(&[("LLM_PROVIDER", "anthropic")]) else {
            panic!("should be disabled");
        };
        assert!(reason.contains("ANTHROPIC_API_KEY"));

        let LlmSettings::Enabled(cfg) = settings(&[
            ("LLM_PROVIDER", "Anthropic"),
            ("ANTHROPIC_API_KEY", "sk-test"),
        ]) else {
            panic!("should be enabled");
        };
        assert_eq!(cfg.kind, ProviderKind::Anthropic);
        assert_eq!(cfg.model, "claude-opus-5-5");
        assert_eq!(cfg.base_url, ANTHROPIC_DEFAULT_BASE_URL);
        // key 不會出現在 Debug 輸出（例如 log）裡
        assert!(!format!("{cfg:?}").contains("sk-test"));
    }

    #[test]
    fn openai_needs_a_model_but_not_a_key() {
        assert!(matches!(
            settings(&[("LLM_PROVIDER", "openai")]),
            LlmSettings::Disabled { .. }
        ));
        let LlmSettings::Enabled(cfg) = settings(&[
            ("LLM_PROVIDER", "openai"),
            ("OPENAI_MODEL", "qwen2.5:0.5b"),
            ("OPENAI_BASE_URL", "http://ollama:11434/v1"),
        ]) else {
            panic!("should be enabled");
        };
        assert_eq!(cfg.kind, ProviderKind::OpenAi);
        assert!(cfg.api_key.is_none());
        assert_eq!(cfg.base_url, "http://ollama:11434/v1");
    }

    #[test]
    fn unknown_provider_is_reported() {
        let LlmSettings::Disabled { reason } = settings(&[("LLM_PROVIDER", "gemini")]) else {
            panic!("should be disabled");
        };
        assert!(reason.contains("gemini"));
    }

    #[test]
    fn json_is_extracted_from_fenced_or_chatty_answers() {
        assert_eq!(parse_json_text(r#"{"a":1}"#), Ok(json!({"a": 1})));
        assert_eq!(
            parse_json_text("```json\n{\"a\":1}\n```"),
            Ok(json!({"a": 1}))
        );
        assert_eq!(
            parse_json_text("Sure! {\"a\":1} Hope it helps."),
            Ok(json!({"a": 1}))
        );
        assert!(matches!(
            parse_json_text("no json"),
            Err(LlmError::BadOutput(_))
        ));
    }

    #[test]
    fn error_bodies_become_readable_messages() {
        let body = r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        assert_eq!(
            status_error(StatusCode::from_u16(529).unwrap(), body),
            LlmError::Status {
                status: 529,
                message: "Overloaded".into()
            }
        );
        assert_eq!(
            status_error(StatusCode::NOT_FOUND, r#"{"error":"model 'x' not found"}"#),
            LlmError::Status {
                status: 404,
                message: "model 'x' not found".into()
            }
        );
        assert_eq!(
            status_error(StatusCode::UNAUTHORIZED, ""),
            LlmError::Auth(401)
        );
    }

    #[test]
    fn retry_waits_follow_retry_after_with_a_cap() {
        assert_eq!(backoff(1, Some(3)), Duration::from_secs(3));
        assert_eq!(
            backoff(1, Some(999)),
            Duration::from_secs(MAX_RETRY_AFTER_SECS)
        );
        assert_eq!(backoff(2, None), Duration::from_millis(1000));
        assert!(retryable(StatusCode::TOO_MANY_REQUESTS));
        assert!(retryable(StatusCode::from_u16(529).unwrap()));
        assert!(!retryable(StatusCode::BAD_REQUEST));
    }
}
