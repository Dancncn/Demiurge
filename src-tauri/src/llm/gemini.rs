use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::StreamExt;
use serde_json::{json, Value};

use crate::agent::conversation::{FunctionCall, Message, ToolCall};
use crate::store::Settings;

use super::{
    merge_usage, normalize_finish_reason, require_api_key, AssistantTurn, ProviderAdapterKind,
    ProviderProfile, StructuredOutputRequest, Usage,
};

#[allow(dead_code)]
pub async fn stream_completion(
    client: &reqwest::Client,
    cfg: &Settings,
    messages: &[Message],
    tools: &Value,
    on_delta: impl FnMut(&str),
    cancel: &AtomicBool,
) -> Result<AssistantTurn, String> {
    stream_completion_with_profile(
        client,
        cfg,
        messages,
        tools,
        on_delta,
        cancel,
        ProviderProfile::gemini(),
    )
    .await
}

pub async fn stream_completion_with_profile(
    client: &reqwest::Client,
    cfg: &Settings,
    messages: &[Message],
    tools: &Value,
    mut on_delta: impl FnMut(&str),
    cancel: &AtomicBool,
    profile: ProviderProfile,
) -> Result<AssistantTurn, String> {
    let key = require_api_key(cfg, profile)?
        .ok_or_else(|| "未配置 API Key，请在设置里填写。".to_string())?;
    let url = format!(
        "{}/models/{}:streamGenerateContent?alt=sse&key={}",
        cfg.base_url.trim_end_matches('/'),
        cfg.model,
        key
    );
    let body = build_gemini_body(cfg, messages, tools, profile)?;

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
    profile: ProviderProfile,
) -> Result<Value, String> {
    build_gemini_body_with_structured_output(cfg, messages, tools, profile, None)
}

pub fn build_gemini_body_with_structured_output(
    cfg: &Settings,
    messages: &[Message],
    tools: &Value,
    profile: ProviderProfile,
    structured_output: Option<&StructuredOutputRequest>,
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
            "maxOutputTokens": profile.effective_reserved_output_tokens(cfg)
        }
    });
    if !system_parts.is_empty() {
        body["systemInstruction"] = json!({ "parts": system_parts });
    }
    if profile.supports_non_empty_tools(tools) {
        body["tools"] = tools.clone();
    }
    if let Some(request) = profile.structured_output_request(structured_output) {
        body["generationConfig"]["responseMimeType"] = json!("application/json");
        body["generationConfig"]["responseSchema"] = request.schema.clone();
    }
    Ok(body)
}

#[derive(Default)]
struct GeminiStreamState {
    content: String,
    tool_calls: Vec<ToolCall>,
    finish: String,
    usage: Option<Usage>,
}

impl GeminiStreamState {
    fn finish(self) -> AssistantTurn {
        let finish_reason = normalize_finish_reason(
            ProviderAdapterKind::Gemini,
            &self.finish,
            !self.tool_calls.is_empty(),
        );
        AssistantTurn {
            content: self.content,
            tool_calls: self.tool_calls,
            finish_reason,
            usage: self.usage,
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
    if let Some(usage) = parse_gemini_usage(&v["usageMetadata"]) {
        merge_usage(&mut state.usage, usage);
    }

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

fn parse_gemini_usage(v: &Value) -> Option<Usage> {
    if !v.is_object() {
        return None;
    }
    Some(Usage {
        input_tokens: v["promptTokenCount"].as_u64().map(|n| n as usize),
        output_tokens: v["candidatesTokenCount"].as_u64().map(|n| n as usize),
        total_tokens: v["totalTokenCount"].as_u64().map(|n| n as usize),
    })
    .filter(|usage| usage.total_or_sum().is_some())
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
            ProviderProfile::gemini(),
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
        assert_eq!(
            body["generationConfig"]["maxOutputTokens"],
            cfg().reserved_output_tokens
        );
        assert!(body["tools"].is_array());
    }

    #[test]
    fn gemini_body_omits_tools_when_profile_disables_tools() {
        let mut profile = ProviderProfile::gemini();
        profile.supports_tools = false;
        let body = build_gemini_body(
            &cfg(),
            &[Message::user("hi")],
            &json!([{ "function_declarations": [{ "name": "grep", "parameters": { "type": "object" } }] }]),
            profile,
        )
        .unwrap();
        assert!(body.get("tools").is_none());
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

    #[test]
    fn gemini_stream_parses_usage_metadata() {
        let mut state = GeminiStreamState::default();
        parse_gemini_stream_data(
            r#"{"usageMetadata":{"promptTokenCount":21,"candidatesTokenCount":5,"totalTokenCount":26}}"#,
            &mut state,
            &mut |_| {},
        );
        let usage = state.finish().usage.unwrap();
        assert_eq!(usage.input_tokens, Some(21));
        assert_eq!(usage.output_tokens, Some(5));
        assert_eq!(usage.total_tokens, Some(26));
    }
}
