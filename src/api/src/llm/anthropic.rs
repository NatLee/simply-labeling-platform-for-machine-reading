//! Claude 的 Messages API（`POST /v1/messages`）。Rust 沒有官方 SDK，直接送 HTTP。
//!
//! - 用 structured outputs（`output_config.format`）要求回答符合 JSON Schema。
//! - `claude-opus-5-5` 一定會思考（adaptive thinking），用 `effort` 控制深度；
//!   產生標注建議是有人會再確認的工作，用 `medium`（這個模型的預設值，這裡寫明）。
//! - 開啟 server-side fallback（`fallbacks: "default"`）：模型的安全分類器拒絕時，
//!   API 會自動改用 Anthropic 建議的備援模型重跑，不用自己維護模型清單。
//! - 回應先看 `stop_reason`：`refusal` 和 `max_tokens` 都不能當成正常答案。

use serde_json::{json, Value};

use super::{JsonTask, Llm, LlmError};

pub const DEFAULT_MODEL: &str = "claude-opus-5-5";
pub const API_VERSION: &str = "2026-10-21";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
/// 非串流請求的建議上限；思考的 token 也算在裡面。
const MAX_TOKENS: u32 = 16_000;
const EFFORT: &str = "medium";

pub fn request_body(model: &str, task: &JsonTask<'_>) -> Value {
    json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "system": task.system,
        "messages": [{ "role": "user", "content": task.user }],
        "thinking": { "type": "adaptive" },
        "output_config": {
            "effort": EFFORT,
            "format": { "type": "json_schema", "schema": task.schema },
        },
        "fallbacks": "default",
    })
}

pub async fn complete(llm: &Llm, task: &JsonTask<'_>) -> Result<String, LlmError> {
    let key = llm
        .config
        .api_key
        .as_ref()
        .map(|k| k.expose())
        .unwrap_or_default();
    let req = llm
        .http
        .post(llm.endpoint("v1/messages"))
        .header("x-api-key", key)
        .header("anthropic-version", API_VERSION)
        .header("anthropic-beta", FALLBACK_BETA)
        .json(&request_body(&llm.config.model, task));
    extract_text(&llm.send(req).await?)
}

/// 取出回答的文字。思考區塊（`thinking`）和備援紀錄（`fallback`）都略過，只拿 `text`。
pub fn extract_text(resp: &Value) -> Result<String, LlmError> {
    match resp["stop_reason"].as_str() {
        Some("refusal") => {
            return Err(LlmError::Refused(
                resp["stop_details"]["category"].as_str().map(String::from),
            ))
        }
        Some("max_tokens") => return Err(LlmError::Truncated),
        _ => {}
    }
    let text: String = resp["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str())
        .collect();
    if text.trim().is_empty() {
        return Err(LlmError::BadOutput("回應裡沒有文字".into()));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_uses_structured_outputs_adaptive_thinking_and_fallbacks() {
        let schema = json!({"type": "object"});
        let task = JsonTask {
            system: "sys",
            user: "hi",
            schema_name: "x",
            schema: &schema,
        };
        let body = request_body("claude-opus-5-5", &task);
        assert_eq!(body["model"], "claude-opus-5-5");
        assert_eq!(body["system"], "sys");
        assert_eq!(body["output_config"]["format"]["type"], "json_schema");
        assert_eq!(body["output_config"]["format"]["schema"], schema);
        assert_eq!(body["output_config"]["effort"], "medium");
        assert_eq!(body["thinking"]["type"], "adaptive");
        assert_eq!(body["fallbacks"], "default");
        // 不送 temperature 這類取樣參數（新版 API 會拒絕）
        assert!(body.get("temperature").is_none());
    }

    #[test]
    fn text_blocks_are_joined_and_thinking_is_skipped() {
        let resp = json!({
            "stop_reason": "end_turn",
            "content": [
                {"type": "thinking", "thinking": ""},
                {"type": "text", "text": "{\"a\":"},
                {"type": "text", "text": "1}"}
            ]
        });
        assert_eq!(extract_text(&resp).unwrap(), "{\"a\":1}");
    }

    #[test]
    fn refusals_and_truncation_are_errors() {
        let refusal = json!({
            "stop_reason": "refusal",
            "stop_details": {"type": "refusal", "category": "cyber"},
            "content": []
        });
        assert_eq!(
            extract_text(&refusal),
            Err(LlmError::Refused(Some("cyber".into())))
        );
        let cut = json!({"stop_reason": "max_tokens", "content": [{"type": "text", "text": "{"}]});
        assert_eq!(extract_text(&cut), Err(LlmError::Truncated));
        let empty = json!({"stop_reason": "end_turn", "content": []});
        assert!(matches!(extract_text(&empty), Err(LlmError::BadOutput(_))));
    }
}
