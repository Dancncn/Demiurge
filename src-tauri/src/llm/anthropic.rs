use std::collections::BTreeMap;
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
    let key = require_api_key(cfg, ProviderProfile::anthropic())?
        .ok_or_else(|| "未配置 API Key，请在设置里填写。".to_string())?;
    let url = format!("{}/messages", cfg.base_url.trim_end_matches('/'));
    let body = build_anthropic_body(cfg, messages, tools)?;

    let resp = client
        .post(&url)
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
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
        "max_tokens": cfg.reserved_output_tokens,
        "stream": true,
        "messages": out_messages,
    });
    if !system_parts.is_empty() {
        body["system"] = json!(system_parts.join("\n\n"));
    }
    if super::non_empty_tools(tools) {
        body["tools"] = tools.clone();
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
        let finish_reason = if self.finish.is_empty() {
            if tool_calls.is_empty() {
                "stop"
            } else {
                "tool_calls"
            }
            .to_string()
        } else if self.finish == "tool_use" {
            "tool_calls".to_string()
        } else if self.finish == "end_turn" {
            "stop".to_string()
        } else if self.finish == "max_tokens" {
            "length".to_string()
        } else {
            self.finish
        };
        AssistantTurn {
            content: self.content,
            tool_calls,
            finish_reason,
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
        }
        "message_stop" => state.message_stopped = true,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::conversation::Message;
    use crate::store::{ProviderKind, Settings};

    fn cfg() -> Settings {
        Settings {
            provider: ProviderKind::Anthropic,
            model: "claude-test".to_string(),
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
        )
        .unwrap();
        assert_eq!(body["system"], "sys");
        assert_eq!(body["messages"][1]["content"][0]["type"], "tool_use");
        assert_eq!(body["messages"][2]["content"][0]["type"], "tool_result");
        assert!(body["tools"].is_array());
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
}
