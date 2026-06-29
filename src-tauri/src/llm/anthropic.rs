use std::collections::BTreeMap;
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
        ProviderProfile::anthropic(),
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
    let url = format!("{}/messages", cfg.base_url.trim_end_matches('/'));
    let body = build_anthropic_body(cfg, messages, tools, profile)?;

    let mut req = client
        .post(&url)
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01");
    if profile.anthropic_output_config_effort(cfg).is_some() {
        req = req.header("anthropic-beta", "effort-2025-11-24");
    }
    let resp = req
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求 Anthropic 失败：{e}"))?;

    if !resp.status().is_success() {
        let code = resp.status();
        let txt = resp.text().await.unwrap_or_default();
        return Err(format!("Anthropic 返回 HTTP {code}：{txt}"));
    }

    let mut stream = resp.bytes_stream();
    let mut buf = Vec::<u8>::new();
    let mut state = AnthropicStreamState::default();

    'outer: while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            state.finish = "interrupted".to_string();
            break;
        }
        let bytes = chunk.map_err(|e| format!("读取 Anthropic 流失败：{e}"))?;
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
            parse_anthropic_stream_data(data, &mut state, &mut on_delta);
            if state.message_stopped {
                break 'outer;
            }
        }
    }

    Ok(state.finish())
}

pub fn build_anthropic_body(
    cfg: &Settings,
    messages: &[Message],
    tools: &Value,
    profile: ProviderProfile,
) -> Result<Value, String> {
    build_anthropic_body_with_structured_output(cfg, messages, tools, profile, None)
}

pub fn build_anthropic_body_with_structured_output(
    cfg: &Settings,
    messages: &[Message],
    tools: &Value,
    profile: ProviderProfile,
    structured_output: Option<&StructuredOutputRequest>,
) -> Result<Value, String> {
    let mut system_parts = Vec::new();
    let mut out_messages = Vec::new();

    for msg in messages {
        match msg.role.as_str() {
            "system" => {
                if let Some(content) = msg.content.as_deref() {
                    if !content.trim().is_empty() {
                        system_parts.push(content.to_string());
                    }
                }
            }
            "user" => out_messages.push(json!({
                "role": "user",
                "content": [{ "type": "text", "text": msg.content.as_deref().unwrap_or_default() }]
            })),
            "assistant" => {
                let mut content = Vec::new();
                if let Some(text) = msg.content.as_deref() {
                    if !text.is_empty() {
                        content.push(json!({ "type": "text", "text": text }));
                    }
                }
                if let Some(calls) = &msg.tool_calls {
                    for call in calls {
                        let input = serde_json::from_str::<Value>(&call.function.arguments)
                            .unwrap_or_else(|_| json!({}));
                        content.push(json!({
                            "type": "tool_use",
                            "id": call.id,
                            "name": call.function.name,
                            "input": input
                        }));
                    }
                }
                if !content.is_empty() {
                    out_messages.push(json!({ "role": "assistant", "content": content }));
                }
            }
            "tool" => out_messages.push(json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": msg.tool_call_id.as_deref().unwrap_or_default(),
                    "content": msg.content.as_deref().unwrap_or_default()
                }]
            })),
            _ => {}
        }
    }

    let mut body = json!({
        "model": cfg.model,
        "max_tokens": profile.effective_reserved_output_tokens(cfg),
        "stream": profile.supports_streaming,
        "messages": out_messages,
    });
    if !system_parts.is_empty() {
        body["system"] = json!(system_parts.join("\n\n"));
    }
    if profile.supports_non_empty_tools(tools) {
        body["tools"] = tools.clone();
    }
    if let Some(effort) = profile.anthropic_output_config_effort(cfg) {
        body["output_config"] = json!({ "effort": effort });
    }
    if let Some(request) = profile.structured_output_request(structured_output) {
        let mut output_tools = tools.as_array().cloned().unwrap_or_default();
        output_tools.push(json!({
            "name": request.name,
            "description": request.description.as_deref().unwrap_or("Return structured output."),
            "input_schema": request.schema,
        }));
        body["tools"] = Value::Array(output_tools);
        body["tool_choice"] = json!({ "type": "tool", "name": request.name });
    }
    Ok(body)
}

#[derive(Default)]
struct ToolAccum {
    id: String,
    name: String,
    input: String,
}

#[derive(Default)]
struct AnthropicStreamState {
    content: String,
    tools: BTreeMap<u64, ToolAccum>,
    finish: String,
    usage: Option<Usage>,
    message_stopped: bool,
}

impl AnthropicStreamState {
    fn finish(self) -> AssistantTurn {
        let tool_calls = self
            .tools
            .into_iter()
            .filter(|(_, t)| !t.name.is_empty())
            .map(|(idx, t)| ToolCall {
                id: if t.id.is_empty() {
                    format!("call_{idx}_{}", t.name)
                } else {
                    t.id
                },
                kind: "function".to_string(),
                function: FunctionCall {
                    name: t.name,
                    arguments: if t.input.trim().is_empty() {
                        "{}".to_string()
                    } else {
                        t.input
                    },
                },
            })
            .collect::<Vec<_>>();
        let finish_reason = normalize_finish_reason(
            ProviderAdapterKind::Anthropic,
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

fn parse_anthropic_stream_data(
    data: &str,
    state: &mut AnthropicStreamState,
    on_delta: &mut impl FnMut(&str),
) {
    let Ok(v) = serde_json::from_str::<Value>(data) else {
        return;
    };
    match v["type"].as_str().unwrap_or_default() {
        "message_start" => {
            if let Some(usage) = parse_anthropic_usage(&v["message"]["usage"]) {
                merge_usage(&mut state.usage, usage);
            }
        }
        "content_block_start" => {
            let idx = v["index"].as_u64().unwrap_or(0);
            let block = &v["content_block"];
            if block["type"].as_str() == Some("tool_use") {
                let entry = state.tools.entry(idx).or_default();
                entry.id = block["id"].as_str().unwrap_or_default().to_string();
                entry.name = block["name"].as_str().unwrap_or_default().to_string();
                if let Some(input) = block.get("input") {
                    if input.is_object() && !input.as_object().map(|o| o.is_empty()).unwrap_or(true)
                    {
                        entry.input = input.to_string();
                    }
                }
            }
        }
        "content_block_delta" => {
            let idx = v["index"].as_u64().unwrap_or(0);
            let delta = &v["delta"];
            match delta["type"].as_str().unwrap_or_default() {
                "text_delta" => {
                    if let Some(text) = delta["text"].as_str() {
                        if !text.is_empty() {
                            state.content.push_str(text);
                            on_delta(text);
                        }
                    }
                }
                "input_json_delta" => {
                    if let Some(partial) = delta["partial_json"].as_str() {
                        state.tools.entry(idx).or_default().input.push_str(partial);
                    }
                }
                _ => {}
            }
        }
        "message_delta" => {
            if let Some(stop) = v["delta"]["stop_reason"].as_str() {
                state.finish = stop.to_string();
            }
            if let Some(usage) = parse_anthropic_usage(&v["usage"]) {
                merge_usage(&mut state.usage, usage);
            }
        }
        "message_stop" => state.message_stopped = true,
        _ => {}
    }
}

fn parse_anthropic_usage(v: &Value) -> Option<Usage> {
    if !v.is_object() {
        return None;
    }
    let base_input = v["input_tokens"].as_u64().map(|n| n as usize);
    let cache_read = v["cache_read_input_tokens"].as_u64().map(|n| n as usize);
    let cache_creation = v["cache_creation_input_tokens"]
        .as_u64()
        .map(|n| n as usize);
    let input_tokens = [base_input, cache_read, cache_creation]
        .into_iter()
        .flatten()
        .fold(None, |acc: Option<usize>, n| {
            Some(acc.unwrap_or(0).saturating_add(n))
        });
    let output_tokens = v["output_tokens"].as_u64().map(|n| n as usize);
    Some(Usage {
        input_tokens,
        output_tokens,
        total_tokens: match (input_tokens, output_tokens) {
            (Some(input), Some(output)) => Some(input.saturating_add(output)),
            _ => None,
        },
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
            provider: ProviderKind::Anthropic,
            model: "claude-sonnet-4-6".to_string(),
            ..Settings::default()
        }
    }

    #[test]
    fn anthropic_body_converts_tools_and_results() {
        let call = ToolCall {
            id: "toolu_1".to_string(),
            kind: "function".to_string(),
            function: FunctionCall {
                name: "read_file".to_string(),
                arguments: "{\"path\":\"a\"}".to_string(),
            },
        };
        let body = build_anthropic_body(
            &cfg(),
            &[
                Message::system("sys"),
                Message::user("hi"),
                Message::assistant_tools(None, vec![call]),
                Message::tool_result("toolu_1", "read_file", "ok"),
            ],
            &json!([{ "name": "read_file", "input_schema": { "type": "object" } }]),
            ProviderProfile::anthropic(),
        )
        .unwrap();
        assert_eq!(body["system"], "sys");
        assert_eq!(body["messages"][1]["content"][0]["type"], "tool_use");
        assert_eq!(body["messages"][2]["content"][0]["type"], "tool_result");
        assert_eq!(body["max_tokens"], cfg().reserved_output_tokens);
        assert_eq!(body["stream"], true);
        assert!(body["tools"].is_array());
    }

    #[test]
    fn anthropic_body_omits_tools_when_profile_disables_tools() {
        let mut profile = ProviderProfile::anthropic();
        profile.supports_tools = false;
        let body = build_anthropic_body(
            &cfg(),
            &[Message::user("hi")],
            &json!([{ "name": "read_file", "input_schema": { "type": "object" } }]),
            profile,
        )
        .unwrap();
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn anthropic_body_includes_effort_output_config() {
        let mut cfg = cfg();
        cfg.reasoning_effort = ReasoningEffort::Xhigh;
        let body = build_anthropic_body(
            &cfg,
            &[Message::user("hi")],
            &json!([]),
            ProviderProfile::anthropic(),
        )
        .unwrap();

        assert_eq!(body["output_config"]["effort"], "xhigh");
    }

    #[test]
    fn anthropic_stream_parses_text_and_tool() {
        let mut state = AnthropicStreamState::default();
        let mut deltas = String::new();
        parse_anthropic_stream_data(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}}"#,
            &mut state,
            &mut |s| deltas.push_str(s),
        );
        parse_anthropic_stream_data(
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"t1","name":"grep","input":{}}}"#,
            &mut state,
            &mut |_| {},
        );
        parse_anthropic_stream_data(
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"query\":"}}"#,
            &mut state,
            &mut |_| {},
        );
        parse_anthropic_stream_data(
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"\"x\"}"}}"#,
            &mut state,
            &mut |_| {},
        );
        parse_anthropic_stream_data(
            r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"}}"#,
            &mut state,
            &mut |_| {},
        );
        let turn = state.finish();
        assert_eq!(deltas, "hi");
        assert_eq!(turn.finish_reason, "tool_calls");
        assert_eq!(turn.tool_calls[0].function.name, "grep");
        assert_eq!(turn.tool_calls[0].function.arguments, "{\"query\":\"x\"}");
    }

    #[test]
    fn anthropic_stream_captures_usage() {
        let mut state = AnthropicStreamState::default();
        parse_anthropic_stream_data(
            r#"{"type":"message_start","message":{"usage":{"input_tokens":10,"cache_read_input_tokens":2,"cache_creation_input_tokens":3,"output_tokens":1}}}"#,
            &mut state,
            &mut |_| {},
        );
        parse_anthropic_stream_data(
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":7}}"#,
            &mut state,
            &mut |_| {},
        );
        let usage = state.finish().usage.unwrap();
        assert_eq!(usage.input_tokens, Some(15));
        assert_eq!(usage.output_tokens, Some(7));
        assert_eq!(usage.total_tokens, Some(22));
    }
}
