use std::fmt::Display;
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::{pin_mut, Stream, StreamExt};
use serde_json::{json, Value};

use crate::agent::conversation::{FunctionCall, Message, ToolCall};
use crate::store::Settings;

use super::sse::{SseDecoder, SseEvent};
use super::{
    merge_usage, normalize_finish_reason, require_api_key, AssistantTurn, ProviderAdapterKind,
    ProviderProfile, StreamDelta, StructuredOutputRequest, Usage,
};

#[allow(dead_code)]
pub async fn stream_completion(
    client: &reqwest::Client,
    cfg: &Settings,
    messages: &[Message],
    tools: &Value,
    on_delta: impl FnMut(StreamDelta<'_>),
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
    on_delta: impl FnMut(StreamDelta<'_>),
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

    consume_gemini_stream(resp.bytes_stream(), on_delta, cancel).await
}

async fn consume_gemini_stream<S, B, E>(
    stream: S,
    mut on_delta: impl FnMut(StreamDelta<'_>),
    cancel: &AtomicBool,
) -> Result<AssistantTurn, String>
where
    S: Stream<Item = Result<B, E>>,
    B: AsRef<[u8]>,
    E: Display,
{
    pin_mut!(stream);
    let mut decoder = SseDecoder::new();
    let mut state = GeminiStreamState::default();
    let mut terminated = false;

    'outer: while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            state.finish = "interrupted".to_string();
            return Ok(state.finish());
        }
        let bytes = chunk.map_err(|e| format!("读取 Gemini 流失败：{e}"))?;
        for event in decoder
            .push(bytes.as_ref())
            .map_err(|error| format!("Invalid Gemini SSE framing: {error}"))?
        {
            if process_gemini_sse_event(&event, &mut state, &mut on_delta)? {
                terminated = true;
                break 'outer;
            }
        }
    }

    if cancel.load(Ordering::Relaxed) {
        state.finish = "interrupted".to_string();
        return Ok(state.finish());
    }

    if !terminated {
        for event in decoder
            .finish()
            .map_err(|error| format!("Invalid Gemini SSE framing: {error}"))?
        {
            if process_gemini_sse_event(&event, &mut state, &mut on_delta)? {
                terminated = true;
                break;
            }
        }
    }

    if !terminated {
        return Err("Gemini stream ended before protocol terminator finishReason".to_string());
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
    if let Some(budget) = profile.gemini_thinking_budget_tokens(cfg) {
        body["generationConfig"]["thinkingConfig"] = json!({
            "includeThoughts": true,
            "thinkingBudget": budget
        });
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
    terminated: bool,
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
    on_delta: &mut impl FnMut(StreamDelta<'_>),
) -> Result<(), String> {
    let v = serde_json::from_str::<Value>(data)
        .map_err(|error| format!("Invalid Gemini stream event: {error}; data={data}"))?;
    if let Some(error) = v.get("error").filter(|error| !error.is_null()) {
        let code = error.get("code").filter(|code| !code.is_null());
        let status = error.get("status").and_then(Value::as_str);
        let message = error.get("message").and_then(Value::as_str);
        let mut details = Vec::new();
        if let Some(status) = status {
            details.push(status.to_string());
        }
        if let Some(code) = code {
            details.push(format!("code={code}"));
        }
        if let Some(message) = message {
            details.push(message.to_string());
        }
        if details.is_empty() {
            details.push(error.to_string());
        }
        return Err(format!("Gemini stream error: {}", details.join(": ")));
    }
    if let Some(usage) = parse_gemini_usage(&v["usageMetadata"]) {
        merge_usage(&mut state.usage, usage);
    }

    let Some(candidate) = v["candidates"].get(0) else {
        return Ok(());
    };
    if let Some(parts) = candidate["content"]["parts"].as_array() {
        for part in parts {
            let is_thought = part["thought"].as_bool().unwrap_or(false);
            if let Some(text) = part["text"].as_str() {
                if !text.is_empty() {
                    if is_thought {
                        // thought part 是思维链：单独推给前端做「思考中」气泡，不进正文。
                        on_delta(StreamDelta::Reasoning(text));
                    } else {
                        state.content.push_str(text);
                        on_delta(StreamDelta::Content(text));
                    }
                }
            }
            if is_thought {
                continue;
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
        if !finish.is_empty() {
            state.finish = finish.to_string();
            state.terminated = true;
        }
    }
    Ok(())
}

fn process_gemini_sse_event(
    event: &SseEvent,
    state: &mut GeminiStreamState,
    on_delta: &mut impl FnMut(StreamDelta<'_>),
) -> Result<bool, String> {
    let data = event.data.trim();
    if data.is_empty() {
        if event.event.as_deref() == Some("error") {
            return Err("Gemini stream error without details".to_string());
        }
        return Ok(false);
    }
    if event.event.as_deref() == Some("error") {
        return Err(format!("Gemini stream error: {data}"));
    }

    parse_gemini_stream_data(data, state, on_delta)?;
    Ok(state.terminated)
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
    use crate::store::{ProviderKind, ReasoningEffort, Settings};

    fn cfg() -> Settings {
        Settings {
            provider: ProviderKind::Gemini,
            model: "gemini-2.5-pro".to_string(),
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
    fn gemini_body_includes_thinking_budget_for_effort() {
        let mut cfg = cfg();
        cfg.reasoning_effort = ReasoningEffort::High;
        cfg.reserved_output_tokens = 12_000;
        let body = build_gemini_body(
            &cfg,
            &[Message::user("hi")],
            &json!([]),
            ProviderProfile::gemini(),
        )
        .unwrap();

        assert_eq!(
            body["generationConfig"]["thinkingConfig"]["thinkingBudget"],
            8_192
        );
        assert_eq!(
            body["generationConfig"]["thinkingConfig"]["includeThoughts"],
            true
        );
    }

    #[test]
    fn gemini_stream_parses_text_and_function_call() {
        let mut state = GeminiStreamState::default();
        let mut deltas = String::new();
        parse_gemini_stream_data(
            r#"{"candidates":[{"content":{"parts":[{"text":"hi"},{"functionCall":{"name":"read_file","args":{"path":"a"}}}]},"finishReason":"STOP"}]}"#,
            &mut state,
            &mut |d| {
                if let crate::llm::StreamDelta::Content(s) = d {
                    deltas.push_str(s);
                }
            },
        )
        .unwrap();
        let turn = state.finish();
        assert_eq!(deltas, "hi");
        assert_eq!(turn.finish_reason, "tool_calls");
        assert_eq!(turn.tool_calls[0].function.name, "read_file");
        assert_eq!(turn.tool_calls[0].function.arguments, "{\"path\":\"a\"}");
    }

    #[test]
    fn gemini_stream_omits_thought_parts_from_visible_text() {
        let mut state = GeminiStreamState::default();
        let mut deltas = String::new();
        parse_gemini_stream_data(
            r#"{"candidates":[{"content":{"parts":[{"text":"hidden","thought":true},{"text":"visible"}]},"finishReason":"STOP"}]}"#,
            &mut state,
            &mut |d| {
                if let crate::llm::StreamDelta::Content(s) = d {
                    deltas.push_str(s);
                }
            },
        )
        .unwrap();
        let turn = state.finish();
        assert_eq!(deltas, "visible");
        assert_eq!(turn.content, "visible");
    }

    #[test]
    fn gemini_stream_parses_usage_metadata() {
        let mut state = GeminiStreamState::default();
        parse_gemini_stream_data(
            r#"{"usageMetadata":{"promptTokenCount":21,"candidatesTokenCount":5,"totalTokenCount":26}}"#,
            &mut state,
            &mut |_| {},
        )
        .unwrap();
        let usage = state.finish().usage.unwrap();
        assert_eq!(usage.input_tokens, Some(21));
        assert_eq!(usage.output_tokens, Some(5));
        assert_eq!(usage.total_tokens, Some(26));
    }

    #[tokio::test]
    async fn gemini_sse_handles_byte_fragmentation_multiline_data_and_finish_tail() {
        let input = concat!(
            ": keep-alive\r\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"你\"}]},\r\n",
            "data: \"finishReason\":\"STOP\"}]}"
        );
        let chunks = input
            .as_bytes()
            .iter()
            .map(|byte| Ok::<_, &'static str>(vec![*byte]))
            .collect::<Vec<_>>();
        let cancel = AtomicBool::new(false);
        let mut rendered = String::new();

        let turn = consume_gemini_stream(
            futures_util::stream::iter(chunks),
            |delta| {
                if let StreamDelta::Content(text) = delta {
                    rendered.push_str(text);
                }
            },
            &cancel,
        )
        .await
        .unwrap();

        assert_eq!(rendered, "你");
        assert_eq!(turn.content, "你");
        assert_eq!(turn.finish_reason, "stop");
    }

    #[tokio::test]
    async fn gemini_stream_rejects_clean_eof_without_finish_reason() {
        let stream = futures_util::stream::iter([Ok::<_, &'static str>(
            b"data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"partial\"}]}}]}\n\n"
                .to_vec(),
        )]);
        let cancel = AtomicBool::new(false);

        let error = match consume_gemini_stream(stream, |_| {}, &cancel).await {
            Ok(_) => panic!("clean EOF without finishReason must fail"),
            Err(error) => error,
        };

        assert!(error.contains("finishReason"));
    }

    #[tokio::test]
    async fn gemini_stream_rejects_malformed_json_with_diagnostics() {
        let stream =
            futures_util::stream::iter([Ok::<_, &'static str>(b"data: {not-json}\n\n".to_vec())]);
        let cancel = AtomicBool::new(false);

        let error = match consume_gemini_stream(stream, |_| {}, &cancel).await {
            Ok(_) => panic!("malformed Gemini event must fail"),
            Err(error) => error,
        };

        assert!(error.contains("Invalid Gemini stream event"));
        assert!(error.contains("{not-json}"));
    }

    #[tokio::test]
    async fn gemini_stream_allows_user_cancellation_without_finish_reason() {
        let stream = futures_util::stream::empty::<Result<Vec<u8>, &'static str>>();
        let cancel = AtomicBool::new(true);

        let turn = consume_gemini_stream(stream, |_| {}, &cancel)
            .await
            .unwrap();

        assert_eq!(turn.finish_reason, "interrupted");
    }
}
