//! OpenAI 相容的 Chat Completions API（`POST {OPENAI_BASE_URL}/chat/completions`）。
//! OpenAI、Ollama、vLLM、LM Studio 都提供這個介面。
//!
//! 用 `response_format: json_schema` 要求照 schema 回答。有些服務不完全遵守，
//! 所以解析時也接受包在 ```json 區塊裡的回答（見 [`super::parse_json_text`]）。

use serde_json::{json, Value};

use super::{JsonTask, Llm, LlmError};

pub fn request_body(model: &str, task: &JsonTask<'_>) -> Value {
    json!({
        "model": model,
        "messages": [
            { "role": "system", "content": task.system },
            { "role": "user", "content": task.user },
        ],
        "response_format": {
            "type": "json_schema",
            "json_schema": { "name": task.schema_name, "strict": true, "schema": task.schema },
        },
    })
}

pub async fn complete(llm: &Llm, task: &JsonTask<'_>) -> Result<String, LlmError> {
    let mut req = llm
        .http
        .post(llm.endpoint("chat/completions"))
        .json(&request_body(&llm.config.model, task));
    if let Some(key) = &llm.config.api_key {
        req = req.bearer_auth(key.expose());
    }
    extract_text(&llm.send(req).await?)
}

pub fn extract_text(resp: &Value) -> Result<String, LlmError> {
    let choice = &resp["choices"][0];
    if choice["finish_reason"] == "length" {
        return Err(LlmError::Truncated);
    }
    let message = &choice["message"];
    if message["refusal"].as_str().is_some_and(|r| !r.is_empty()) {
        return Err(LlmError::Refused(None));
    }
    match message["content"].as_str() {
        Some(text) if !text.trim().is_empty() => Ok(text.to_string()),
        _ => Err(LlmError::BadOutput("回應裡沒有文字".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_asks_for_the_json_schema() {
        let schema = json!({"type": "object"});
        let task = JsonTask {
            system: "sys",
            user: "hi",
            schema_name: "mrc_suggestions",
            schema: &schema,
        };
        let body = request_body("qwen2.5:0.5b", &task);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["content"], "hi");
        assert_eq!(
            body["response_format"]["json_schema"]["name"],
            "mrc_suggestions"
        );
        assert_eq!(body["response_format"]["json_schema"]["schema"], schema);
    }

    #[test]
    fn content_refusal_and_length_are_handled() {
        let ok = json!({"choices": [{"finish_reason": "stop", "message": {"content": "{}"}}]});
        assert_eq!(extract_text(&ok).unwrap(), "{}");
        let cut = json!({"choices": [{"finish_reason": "length", "message": {"content": "{"}}]});
        assert_eq!(extract_text(&cut), Err(LlmError::Truncated));
        let no = json!({"choices": [{"finish_reason": "stop",
                         "message": {"content": null, "refusal": "I can't help"}}]});
        assert_eq!(extract_text(&no), Err(LlmError::Refused(None)));
        assert!(matches!(
            extract_text(&json!({})),
            Err(LlmError::BadOutput(_))
        ));
    }
}
