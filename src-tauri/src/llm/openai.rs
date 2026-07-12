use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::StreamExt;
use serde_json::{json, Value};

use crate::agent::conversation::{FunctionCall, Message, ToolCall};
use crate::store::Settings;

use super::sse::{SseDecoder, SseEvent};
use super::{
    merge_usage, normalize_finish_reason, require_api_key, AssistantTurn, ProviderAdapterKind,
    ProviderProfile, ReasoningEffortCapability, StreamDelta, StructuredOutputRequest, Usage,
};

pub async fn stream_completion_with_profile(
    client: &reqwest::Client,
    cfg: &Settings,
    messages: &[Message],
    tools: &Value,
    mut on_delta: impl FnMut(StreamDelta<'_>),
    cancel: &AtomicBool,
    profile: ProviderProfile,
) -> Result<AssistantTurn, String> {
    let key = require_api_key(cfg, profile)?;
    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let body = build_openai_body(cfg, messages, tools, profile)?;

    let mut req = client.post(&url).json(&body);
    if let Some(key) = key {
        req = req.bearer_auth(key);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("请求 LLM 失败：{e}"))?;

    if !resp.status().is_success() {
        let code = resp.status();
        let txt = resp.text().await.unwrap_or_default();
        return Err(format!("LLM 返回 HTTP {code}：{txt}"));
    }

    let mut stream = resp.bytes_stream();
    let mut decoder = SseDecoder::new();
    let mut state = OpenAiStreamState::default();
    let mut stopped = false;

    'outer: while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            state.finish = "interrupted".to_string();
            stopped = true;
            break;
        }
        let bytes = chunk.map_err(|e| format!("读取流失败：{e}"))?;
        for event in decoder.push(&bytes) {
            if process_openai_sse_event(&event, &mut state, &mut on_delta)? {
                stopped = true;
                break 'outer;
            }
        }
    }

    if !stopped {
        for event in decoder.finish() {
            if process_openai_sse_event(&event, &mut state, &mut on_delta)? {
                break;
            }
        }
    }

    Ok(state.finish())
}

pub fn build_openai_body(
    cfg: &Settings,
    messages: &[Message],
    tools: &Value,
    profile: ProviderProfile,
) -> Result<Value, String> {
    build_openai_body_with_structured_output(cfg, messages, tools, profile, None)
}

pub fn build_openai_body_with_structured_output(
    cfg: &Settings,
    messages: &[Message],
    tools: &Value,
    profile: ProviderProfile,
    structured_output: Option<&StructuredOutputRequest>,
) -> Result<Value, String> {
    let mut body = json!({
        "model": cfg.model,
        "messages": messages,
        "stream": profile.supports_streaming,
    });
    let max_output_tokens = profile.effective_reserved_output_tokens(cfg);
    if matches!(
        profile.reasoning_effort,
        ReasoningEffortCapability::OpenAiChatCompletions
    ) {
        body["max_completion_tokens"] = json!(max_output_tokens);
    } else {
        body["max_tokens"] = json!(max_output_tokens);
    }
    if let Some(effort) = profile.openai_chat_reasoning_effort(cfg) {
        body["reasoning_effort"] = json!(effort);
    }
    if profile.supports_non_empty_tools(tools) {
        body["tools"] = tools.clone();
        body["tool_choice"] = json!("auto");
        if profile.supports_parallel_tool_call_field() {
            body["parallel_tool_calls"] = json!(true);
        }
    }
    if let Some(request) = profile.structured_output_request(structured_output) {
        body["response_format"] = json!({
            "type": "json_schema",
            "json_schema": {
                "name": request.name,
                "description": request.description,
                "schema": request.schema,
                "strict": request.strict,
            }
        });
    }
    Ok(body)
}

#[derive(Default)]
struct OpenAiStreamState {
    content: String,
    tool_accum: BTreeMap<u64, (String, String, String)>,
    finish: String,
    usage: Option<Usage>,
}

impl OpenAiStreamState {
    fn finish(self) -> AssistantTurn {
        let tool_calls: Vec<ToolCall> = self
            .tool_accum
            .into_iter()
            .filter(|(_, (_, name, _))| !name.is_empty())
            .map(|(idx, (id, name, args))| ToolCall {
                id: if id.is_empty() {
                    format!("call_{idx}_{name}")
                } else {
                    id
                },
                kind: "function".to_string(),
                function: FunctionCall {
                    name,
                    arguments: if args.is_empty() {
                        "{}".to_string()
                    } else {
                        args
                    },
                },
            })
            .collect();

        let finish_reason = normalize_finish_reason(
            ProviderAdapterKind::OpenAiCompatible,
            &self.finish,
            !tool_calls.is_empty(),
        );

        AssistantTurn {
            content: self.content,
            tool_calls,
            finish_reason,
            usage: self.usage,
        }
    }
}

fn parse_openai_stream_data(
    data: &str,
    state: &mut OpenAiStreamState,
    on_delta: &mut impl FnMut(StreamDelta<'_>),
) -> Result<(), String> {
    let v = serde_json::from_str::<Value>(data)
        .map_err(|error| format!("Invalid OpenAI stream event: {error}; data={data}"))?;
    if let Some(error) = openai_stream_error(&v) {
        return Err(error);
    }
    if let Some(usage) = parse_openai_usage(&v["usage"]) {
        merge_usage(&mut state.usage, usage);
    }

    let Some(choice) = v["choices"].get(0) else {
        return Ok(());
    };

    // 推理型模型（reasoning_content）会先于正文输出思维链；单独作为 Reasoning 推给前端，
    // 不混入 state.content，避免污染最终答复，同时消除推理阶段的「界面静默」。
    if let Some(r) = choice["delta"]["reasoning_content"].as_str() {
        if !r.is_empty() {
            on_delta(StreamDelta::Reasoning(r));
        }
    }
    if let Some(c) = choice["delta"]["content"].as_str() {
        if !c.is_empty() {
            state.content.push_str(c);
            on_delta(StreamDelta::Content(c));
        }
    }
    if let Some(tcs) = choice["delta"]["tool_calls"].as_array() {
        for tc in tcs {
            let idx = tc["index"].as_u64().unwrap_or(0);
            let entry = state.tool_accum.entry(idx).or_default();
            if let Some(id) = tc["id"].as_str() {
                if !id.is_empty() {
                    entry.0 = id.to_string();
                }
            }
            if let Some(n) = tc["function"]["name"].as_str() {
                entry.1.push_str(n);
            }
            if let Some(a) = tc["function"]["arguments"].as_str() {
                entry.2.push_str(a);
            }
        }
    }
    if let Some(fr) = choice["finish_reason"].as_str() {
        if !fr.is_empty() {
            state.finish = fr.to_string();
        }
    }
    Ok(())
}

fn process_openai_sse_event(
    event: &SseEvent,
    state: &mut OpenAiStreamState,
    on_delta: &mut impl FnMut(StreamDelta<'_>),
) -> Result<bool, String> {
    let data = event.data.trim();
    if data.is_empty() {
        if event.event.as_deref() == Some("error") {
            return Err("OpenAI stream error without details".to_string());
        }
        return Ok(false);
    }
    if event.event.as_deref() == Some("error") {
        if let Ok(value) = serde_json::from_str::<Value>(data) {
            if let Some(error) = openai_stream_error(&value) {
                return Err(error);
            }
            return Err(format!("OpenAI stream error: {value}"));
        }
        return Err(format!("OpenAI stream error: {data}"));
    }
    if data == "[DONE]" {
        return Ok(true);
    }

    parse_openai_stream_data(data, state, on_delta)?;
    Ok(false)
}

fn openai_stream_error(value: &Value) -> Option<String> {
    let error = value
        .get("error")
        .filter(|error| !error.is_null())
        .or_else(|| (value["type"].as_str() == Some("error")).then_some(value))?;
    let message = error["message"]
        .as_str()
        .or_else(|| value["message"].as_str());
    let kind = error["type"].as_str().or_else(|| value["type"].as_str());
    let code = error.get("code").filter(|code| !code.is_null());

    let mut details = Vec::new();
    if let Some(kind) = kind {
        details.push(kind.to_string());
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
    Some(format!("OpenAI stream error: {}", details.join(": ")))
}

fn parse_openai_usage(v: &Value) -> Option<Usage> {
    if !v.is_object() {
        return None;
    }
    Some(Usage {
        input_tokens: v["prompt_tokens"].as_u64().map(|n| n as usize),
        output_tokens: v["completion_tokens"].as_u64().map(|n| n as usize),
        total_tokens: v["total_tokens"].as_u64().map(|n| n as usize),
    })
    .filter(|usage| usage.total_or_sum().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{ProviderKind, ReasoningEffort, Settings};

    fn settings(provider: ProviderKind, api_key: &str) -> Settings {
        Settings {
            provider,
            api_key: api_key.to_string(),
            ..Settings::default()
        }
    }

    #[test]
    fn openai_body_includes_tools_when_present() {
        let cfg = settings(ProviderKind::OpenAiCompatible, "sk-test");
        let body = build_openai_body(
            &cfg,
            &[Message::user("hi")],
            &json!([{ "type": "function", "function": { "name": "x" } }]),
            ProviderProfile::openai_compatible(),
        )
        .unwrap();
        assert_eq!(body["stream"], true);
        assert!(body["tools"].is_array());
        assert_eq!(body["tool_choice"], "auto");
    }

    #[test]
    fn official_openai_body_uses_profile_specific_fields() {
        let mut cfg = settings(ProviderKind::OpenAi, "sk-test");
        cfg.reserved_output_tokens = 32_000;
        let body = build_openai_body(
            &cfg,
            &[Message::user("hi")],
            &json!([{ "type": "function", "function": { "name": "x" } }]),
            ProviderProfile::for_kind(ProviderKind::OpenAi),
        )
        .unwrap();

        assert_eq!(body["max_completion_tokens"], 32_000);
        assert!(body.get("max_tokens").is_none());
        assert_eq!(body["parallel_tool_calls"], true);
    }

    #[test]
    fn openai_compatible_body_omits_openai_only_fields() {
        let mut cfg = settings(ProviderKind::OpenAiCompatible, "sk-test");
        cfg.reserved_output_tokens = 32_000;
        let body = build_openai_body(
            &cfg,
            &[Message::user("hi")],
            &json!([{ "type": "function", "function": { "name": "x" } }]),
            ProviderProfile::for_kind(ProviderKind::OpenAiCompatible),
        )
        .unwrap();

        assert_eq!(body["max_tokens"], 32_000);
        assert!(body.get("max_completion_tokens").is_none());
        assert!(body.get("parallel_tool_calls").is_none());
    }

    #[test]
    fn official_openai_body_includes_xhigh_reasoning_effort_when_supported() {
        let mut cfg = settings(ProviderKind::OpenAi, "sk-test");
        cfg.model = "gpt-5.2".to_string();
        cfg.reasoning_effort = ReasoningEffort::Max;
        let body = build_openai_body(
            &cfg,
            &[Message::user("hi")],
            &json!([]),
            ProviderProfile::for_kind(ProviderKind::OpenAi),
        )
        .unwrap();

        assert_eq!(body["reasoning_effort"], "xhigh");
    }

    #[test]
    fn official_openai_body_downgrades_xhigh_when_model_lacks_support() {
        let mut cfg = settings(ProviderKind::OpenAi, "sk-test");
        cfg.model = "o3".to_string();
        cfg.reasoning_effort = ReasoningEffort::Max;
        let body = build_openai_body(
            &cfg,
            &[Message::user("hi")],
            &json!([]),
            ProviderProfile::for_kind(ProviderKind::OpenAi),
        )
        .unwrap();

        assert_eq!(body["reasoning_effort"], "high");
    }

    #[test]
    fn official_openai_body_omits_reasoning_effort_for_non_reasoning_models() {
        let mut cfg = settings(ProviderKind::OpenAi, "sk-test");
        cfg.model = "gpt-4o".to_string();
        cfg.reasoning_effort = ReasoningEffort::High;
        let body = build_openai_body(
            &cfg,
            &[Message::user("hi")],
            &json!([]),
            ProviderProfile::for_kind(ProviderKind::OpenAi),
        )
        .unwrap();

        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn openai_stream_captures_usage() {
        let mut state = OpenAiStreamState::default();
        parse_openai_stream_data(
            r#"{"choices":[{"delta":{"content":"hi"},"finish_reason":null}],"usage":{"prompt_tokens":12,"completion_tokens":3,"total_tokens":15}}"#,
            &mut state,
            &mut |_| {},
        )
        .unwrap();
        let turn = state.finish();
        assert_eq!(turn.usage.unwrap().input_tokens, Some(12));
        assert_eq!(turn.usage.unwrap().output_tokens, Some(3));
        assert_eq!(turn.usage.unwrap().total_tokens, Some(15));
    }

    #[test]
    fn openai_stream_routes_reasoning_separately_from_content() {
        // reasoning_content（DeepSeek-V4/R1、Kimi、qwen *-flash 思考版等推理模型）应作为
        // Reasoning 单独回调，且不混入最终正文 content。
        let mut state = OpenAiStreamState::default();
        let mut content = String::new();
        let mut reasoning = String::new();
        parse_openai_stream_data(
            r#"{"choices":[{"delta":{"reasoning_content":"先想一下"},"finish_reason":null}]}"#,
            &mut state,
            &mut |d| match d {
                StreamDelta::Content(c) => content.push_str(c),
                StreamDelta::Reasoning(r) => reasoning.push_str(r),
            },
        )
        .unwrap();
        parse_openai_stream_data(
            r#"{"choices":[{"delta":{"content":"答案"},"finish_reason":"stop"}]}"#,
            &mut state,
            &mut |d| match d {
                StreamDelta::Content(c) => content.push_str(c),
                StreamDelta::Reasoning(r) => reasoning.push_str(r),
            },
        )
        .unwrap();
        assert_eq!(reasoning, "先想一下");
        assert_eq!(content, "答案");
        // 思维链不污染最终答复
        assert_eq!(state.finish().content, "答案");
    }

    #[test]
    fn openai_stream_keeps_fragmented_tool_call_normalization() {
        let mut state = OpenAiStreamState::default();
        parse_openai_stream_data(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"read_","arguments":"{\"path\":"}}]},"finish_reason":null}]}"#,
            &mut state,
            &mut |_| {},
        )
        .unwrap();
        parse_openai_stream_data(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"name":"file","arguments":"\"a.rs\"}"}}]},"finish_reason":"tool_calls"}]}"#,
            &mut state,
            &mut |_| {},
        )
        .unwrap();

        let turn = state.finish();
        assert_eq!(turn.finish_reason, "tool_calls");
        assert_eq!(turn.tool_calls[0].id, "call_1");
        assert_eq!(turn.tool_calls[0].function.name, "read_file");
        assert_eq!(turn.tool_calls[0].function.arguments, r#"{"path":"a.rs"}"#);
    }

    #[test]
    fn openai_sse_handles_byte_fragmentation_multiline_data_and_done_tail() {
        let input = concat!(
            ": keep-alive\r\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"你\"},\r\n",
            "data: \"finish_reason\":\"stop\"}]}\r\n\r\n",
            "data: [DONE]"
        );
        let mut decoder = SseDecoder::new();
        let mut state = OpenAiStreamState::default();
        let mut rendered = String::new();
        let mut stopped = false;

        for byte in input.as_bytes() {
            for event in decoder.push(std::slice::from_ref(byte)) {
                stopped = process_openai_sse_event(&event, &mut state, &mut |delta| {
                    if let StreamDelta::Content(text) = delta {
                        rendered.push_str(text);
                    }
                })
                .unwrap();
                assert!(!stopped, "[DONE] has no event delimiter and belongs to EOF");
            }
        }
        for event in decoder.finish() {
            stopped = process_openai_sse_event(&event, &mut state, &mut |_| {}).unwrap();
        }

        assert!(stopped);
        assert_eq!(rendered, "你");
        let turn = state.finish();
        assert_eq!(turn.content, "你");
        assert_eq!(turn.finish_reason, "stop");
    }

    #[test]
    fn openai_sse_returns_embedded_and_named_errors() {
        let embedded = SseEvent {
            event: None,
            data:
                r#"{"error":{"type":"rate_limit_error","code":"rate_limit","message":"slow down"}}"#
                    .to_string(),
        };
        let named = SseEvent {
            event: Some("error".to_string()),
            data: "upstream disconnected".to_string(),
        };

        let embedded_error =
            process_openai_sse_event(&embedded, &mut OpenAiStreamState::default(), &mut |_| {})
                .unwrap_err();
        let named_error =
            process_openai_sse_event(&named, &mut OpenAiStreamState::default(), &mut |_| {})
                .unwrap_err();

        assert!(embedded_error.contains("rate_limit_error"));
        assert!(embedded_error.contains("slow down"));
        assert!(named_error.contains("upstream disconnected"));
    }

    #[test]
    fn openai_stream_rejects_malformed_json_event() {
        let error =
            parse_openai_stream_data("{not-json}", &mut OpenAiStreamState::default(), &mut |_| {})
                .unwrap_err();
        assert!(error.contains("Invalid OpenAI stream event"));
    }

    #[test]
    fn local_profile_allows_empty_key() {
        let cfg = settings(ProviderKind::Local, "");
        assert!(
            require_api_key(&cfg, ProviderProfile::local_openai_compatible())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn openai_profile_requires_key() {
        let cfg = settings(ProviderKind::OpenAiCompatible, "");
        assert!(require_api_key(&cfg, ProviderProfile::openai_compatible()).is_err());
    }
}
