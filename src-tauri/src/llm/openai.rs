use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::StreamExt;
use serde_json::{json, Value};

use crate::agent::conversation::{FunctionCall, Message, ToolCall};
use crate::store::Settings;

use super::{require_api_key, AssistantTurn, ProviderProfile, StructuredOutputRequest, Usage};

pub async fn stream_completion_with_profile(
    client: &reqwest::Client,
    cfg: &Settings,
    messages: &[Message],
    tools: &Value,
    mut on_delta: impl FnMut(&str),
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
    let mut buf: Vec<u8> = Vec::new();
    let mut state = OpenAiStreamState::default();

    'outer: while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            state.finish = "interrupted".to_string();
            break;
        }
        let bytes = chunk.map_err(|e| format!("读取流失败：{e}"))?;
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
            if data == "[DONE]" {
                break 'outer;
            }
            parse_openai_stream_data(data, &mut state, &mut on_delta);
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
        "max_tokens": profile.effective_reserved_output_tokens(cfg),
    });
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

        let finish_reason = if self.finish.is_empty() {
            if tool_calls.is_empty() {
                "stop"
            } else {
                "tool_calls"
            }
            .to_string()
        } else {
            self.finish
        };

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
    on_delta: &mut impl FnMut(&str),
) {
    let Ok(v) = serde_json::from_str::<Value>(data) else {
        return;
    };
    if let Some(usage) = parse_openai_usage(&v["usage"]) {
        state.usage = Some(
            state
                .usage
                .map(|current| current.merge(usage))
                .unwrap_or(usage),
        );
    }

    let Some(choice) = v["choices"].get(0) else {
        return;
    };

    if let Some(c) = choice["delta"]["content"].as_str() {
        if !c.is_empty() {
            state.content.push_str(c);
            on_delta(c);
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
    use crate::store::{ProviderKind, Settings};

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
    fn openai_stream_captures_usage() {
        let mut state = OpenAiStreamState::default();
        parse_openai_stream_data(
            r#"{"choices":[{"delta":{"content":"hi"},"finish_reason":null}],"usage":{"prompt_tokens":12,"completion_tokens":3,"total_tokens":15}}"#,
            &mut state,
            &mut |_| {},
        );
        let turn = state.finish();
        assert_eq!(turn.usage.unwrap().input_tokens, Some(12));
        assert_eq!(turn.usage.unwrap().output_tokens, Some(3));
        assert_eq!(turn.usage.unwrap().total_tokens, Some(15));
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
