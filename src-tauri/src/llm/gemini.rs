use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::StreamExt;
use serde_json::{json, Value};

use crate::agent::conversation::{FunctionCall, Message, ToolCall};
use crate::store::Settings;

use super::{require_api_key, AssistantTurn, ProviderProfile};

pub async fn stream_completion(
    client: &reqwest::Client,
    cfg: &Settings,
    messages: &[Message],
    tools: &Value,
    mut on_delta: impl FnMut(&str),
    cancel: &AtomicBool,
) -> Result<AssistantTurn, String> {
    let key = require_api_key(cfg, ProviderProfile::gemini())?
        .ok_or_else(|| "未配置 API Key，请在设置里填写。".to_string())?;
    let url = format!(
        "{}/models/{}:streamGenerateContent?alt=sse&key={}",
        cfg.base_url.trim_end_matches('/'),
        cfg.model,
        key
    );
    let body = build_gemini_body(cfg, messages, tools)?;

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求 Gemini 失败：{e}"))?;

    if !resp.status().is_success() {
        let code = resp.status();
        let txt = resp.text().await.unwrap_or_default();
        return Err(format!("Gemini 返回 HTTP {code}：{txt}"));
    }

    let mut stream = resp.bytes_stream();
    let mut buf = Vec::<u8>::new();
    let mut state = GeminiStreamState::default();

    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            state.finish = "interrupted".to_string();
            break;
        }
        let bytes = chunk.map_err(|e| format!("读取 Gemini 流失败：{e}"))?;
        buf.extend_from_slice(&bytes);
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line_bytes);
            let line = line.trim();
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data.is_empty() {
                continue;
            }
            parse_gemini_stream_data(data, &mut state, &mut on_delta);
        }
    }

    Ok(state.finish())
}

pub fn build_gemini_body(
    cfg: &Settings,
    messages: &[Message],
    tools: &Value,
) -> Result<Value, String> {
    let mut system_parts = Vec::new();
    let mut contents = Vec::new();

    for msg in messages {
        match msg.role.as_str() {
            "system" => {
                if let Some(content) = msg.content.as_deref() {
                    if !content.trim().is_empty() {
                        system_parts.push(json!({ "text": content }));
                    }
                }
            }
            "user" => contents.push(json!({
                "role": "user",
                "parts": [{ "text": msg.content.as_deref().unwrap_or_default() }]
            })),
            "assistant" => {
                let mut parts = Vec::new();
                if let Some(text) = msg.content.as_deref() {
                    if !text.is_empty() {
                        parts.push(json!({ "text": text }));
                    }
                }
                if let Some(calls) = &msg.tool_calls {
                    for call in calls {
                        let args = serde_json::from_str::<Value>(&call.function.arguments)
                            .unwrap_or_else(|_| json!({}));
                        parts.push(json!({
                            "functionCall": {
                                "name": call.function.name,
                                "args": args
                            }
                        }));
                    }
                }
                if !parts.is_empty() {
                    contents.push(json!({ "role": "model", "parts": parts }));
                }
            }
            "tool" => {
                let response = msg
                    .content
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                    .filter(Value::is_object)
                    .unwrap_or_else(
                        || json!({ "content": msg.content.as_deref().unwrap_or_default() }),
                    );
                contents.push(json!({
                    "role": "function",
                    "parts": [{
                        "functionResponse": {
                            "name": msg.name.as_deref().unwrap_or_default(),
                            "response": response
                        }
                    }]
                }));
            }
            _ => {}
        }
    }

    let mut body = json!({
        "contents": contents,
        "generationConfig": {
            "maxOutputTokens": cfg.reserved_output_tokens
        }
    });
    if !system_parts.is_empty() {
        body["systemInstruction"] = json!({ "parts": system_parts });
    }
    if super::non_empty_tools(tools) {
        body["tools"] = tools.clone();
    }
    Ok(body)
}

#[derive(Default)]
struct GeminiStreamState {
    content: String,
    tool_calls: Vec<ToolCall>,
    finish: String,
}

impl GeminiStreamState {
    fn finish(self) -> AssistantTurn {
        let finish_reason = if self.finish.is_empty() {
            if self.tool_calls.is_empty() {
                "stop"
            } else {
                "tool_calls"
            }
            .to_string()
        } else if !self.tool_calls.is_empty() {
            "tool_calls".to_string()
        } else {
            match self.finish.as_str() {
                "STOP" => "stop".to_string(),
                "MAX_TOKENS" => "length".to_string(),
                other => other.to_ascii_lowercase(),
            }
        };
        AssistantTurn {
            content: self.content,
            tool_calls: self.tool_calls,
            finish_reason,
        }
    }
}

fn parse_gemini_stream_data(
    data: &str,
    state: &mut GeminiStreamState,
    on_delta: &mut impl FnMut(&str),
) {
    let Ok(v) = serde_json::from_str::<Value>(data) else {
        return;
    };
    let Some(candidate) = v["candidates"].get(0) else {
        return;
    };
    if let Some(parts) = candidate["content"]["parts"].as_array() {
        for part in parts {
            if let Some(text) = part["text"].as_str() {
                if !text.is_empty() {
                    state.content.push_str(text);
                    on_delta(text);
                }
            }
            if part.get("functionCall").is_some() {
                let fc = &part["functionCall"];
                let name = fc["name"].as_str().unwrap_or_default().to_string();
                if !name.is_empty() {
                    let idx = state.tool_calls.len();
                    let args = fc.get("args").cloned().unwrap_or_else(|| json!({}));
                    state.tool_calls.push(ToolCall {
                        id: format!("call_{idx}_{name}"),
                        kind: "function".to_string(),
                        function: FunctionCall {
                            name,
                            arguments: args.to_string(),
                        },
                    });
                }
            }
        }
    }
    if let Some(finish) = candidate["finishReason"].as_str() {
        state.finish = finish.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::conversation::Message;
    use crate::store::{ProviderKind, Settings};

    fn cfg() -> Settings {
        Settings {
            provider: ProviderKind::Gemini,
            model: "gemini-test".to_string(),
            ..Settings::default()
        }
    }

    #[test]
    fn gemini_body_converts_function_call_and_response() {
        let call = ToolCall {
            id: "call_1".to_string(),
            kind: "function".to_string(),
            function: FunctionCall {
                name: "grep".to_string(),
                arguments: "{\"query\":\"x\"}".to_string(),
            },
        };
        let body = build_gemini_body(
            &cfg(),
            &[
                Message::system("sys"),
                Message::user("hi"),
                Message::assistant_tools(None, vec![call]),
                Message::tool_result("call_1", "grep", "done"),
            ],
            &json!([{ "function_declarations": [{ "name": "grep", "parameters": { "type": "object" } }] }]),
        )
        .unwrap();
        assert_eq!(body["systemInstruction"]["parts"][0]["text"], "sys");
        assert_eq!(
            body["contents"][1]["parts"][0]["functionCall"]["name"],
            "grep"
        );
        assert_eq!(
            body["contents"][2]["parts"][0]["functionResponse"]["name"],
            "grep"
        );
        assert!(body["tools"].is_array());
    }

    #[test]
    fn gemini_stream_parses_text_and_function_call() {
        let mut state = GeminiStreamState::default();
        let mut deltas = String::new();
        parse_gemini_stream_data(
            r#"{"candidates":[{"content":{"parts":[{"text":"hi"},{"functionCall":{"name":"read_file","args":{"path":"a"}}}]},"finishReason":"STOP"}]}"#,
            &mut state,
            &mut |s| deltas.push_str(s),
        );
        let turn = state.finish();
        assert_eq!(deltas, "hi");
        assert_eq!(turn.finish_reason, "tool_calls");
        assert_eq!(turn.tool_calls[0].function.name, "read_file");
        assert_eq!(turn.tool_calls[0].function.arguments, "{\"path\":\"a\"}");
    }
}
