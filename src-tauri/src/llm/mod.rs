//! 组件 1：LLM 适配器。OpenAI 兼容的流式客户端——循环调用的「大脑」。
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::StreamExt;
use serde_json::{json, Value};

use crate::agent::conversation::{FunctionCall, Message, ToolCall};
use crate::store::Settings;

/// 一次 LLM 调用的结果。
pub struct AssistantTurn {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    /// stop | tool_calls | length | interrupted | ...
    pub finish_reason: String,
}

/// 流式请求 /chat/completions，解析 SSE。
/// - `on_delta`：每收到一段正文增量就回调（用于把 token 实时推给前端）。
/// - `cancel`：用户中断标志，置位后尽快结束并返回 finish_reason="interrupted"。
pub async fn stream_completion(
    client: &reqwest::Client,
    cfg: &Settings,
    messages: &[Message],
    tools: &Value,
    mut on_delta: impl FnMut(&str),
    cancel: &AtomicBool,
) -> Result<AssistantTurn, String> {
    if cfg.api_key.trim().is_empty() {
        return Err("未配置 API Key，请在设置里填写。".to_string());
    }

    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let mut body = json!({
        "model": cfg.model,
        "messages": messages,
        "stream": true,
    });
    // 没有工具时不要传空数组（部分网关会报错）
    if tools.as_array().map(|a| !a.is_empty()).unwrap_or(false) {
        body["tools"] = tools.clone();
        body["tool_choice"] = json!("auto");
    }

    let resp = client
        .post(&url)
        .bearer_auth(&cfg.api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求 LLM 失败：{e}"))?;

    if !resp.status().is_success() {
        let code = resp.status();
        let txt = resp.text().await.unwrap_or_default();
        return Err(format!("LLM 返回 HTTP {code}：{txt}"));
    }

    let mut stream = resp.bytes_stream();
    // 用字节缓冲按行切分：SSE 行以 \n 结尾，0x0A 绝不会出现在 UTF-8 多字节中间，
    // 因此「整行」转字符串是安全的（中文不会被拦腰截断）。
    let mut buf: Vec<u8> = Vec::new();
    let mut content = String::new();
    // index -> (id, name, arguments)
    let mut tool_accum: BTreeMap<u64, (String, String, String)> = BTreeMap::new();
    let mut finish = String::new();

    'outer: while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            finish = "interrupted".to_string();
            break;
        }
        let bytes = chunk.map_err(|e| format!("读取流失败：{e}"))?;
        buf.extend_from_slice(&bytes);

        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line_bytes);
            let line = line.trim();
            let Some(data) = line.strip_prefix("data:") else { continue };
            let data = data.trim();
            if data.is_empty() {
                continue;
            }
            if data == "[DONE]" {
                break 'outer;
            }
            let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
            let Some(choice) = v["choices"].get(0) else { continue };

            if let Some(c) = choice["delta"]["content"].as_str() {
                if !c.is_empty() {
                    content.push_str(c);
                    on_delta(c);
                }
            }
            if let Some(tcs) = choice["delta"]["tool_calls"].as_array() {
                for tc in tcs {
                    let idx = tc["index"].as_u64().unwrap_or(0);
                    let entry = tool_accum.entry(idx).or_default();
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
                    finish = fr.to_string();
                }
            }
        }
    }

    // 用 into_iter 保留 index：当兼容端点（LM Studio 等）在流式 delta 里省略 id 时，
    // 兜底 id 必须纳入 index 才能全局唯一，否则同一轮内重复同名调用会撞 id，
    // 导致下一轮请求里 assistant.tool_calls 出现重复 tool_call_id 而被判 400。
    let tool_calls: Vec<ToolCall> = tool_accum
        .into_iter()
        .filter(|(_, (_, name, _))| !name.is_empty())
        .map(|(idx, (id, name, args))| ToolCall {
            id: if id.is_empty() { format!("call_{idx}_{name}") } else { id },
            kind: "function".to_string(),
            function: FunctionCall {
                name,
                arguments: if args.is_empty() { "{}".to_string() } else { args },
            },
        })
        .collect();

    if finish.is_empty() {
        finish = if tool_calls.is_empty() { "stop" } else { "tool_calls" }.to_string();
    }

    Ok(AssistantTurn { content, tool_calls, finish_reason: finish })
}
