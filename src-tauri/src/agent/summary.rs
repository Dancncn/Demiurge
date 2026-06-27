//! Phase 2：会话滚动摘要。把被上下文裁剪移除的旧消息压缩为短期会话状态。
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::json;

use super::conversation::Message;
use crate::llm;
use crate::store::Settings;

const MAX_REMOVED_CHARS: usize = 12_000;
const MAX_SUMMARY_CHARS: usize = 6_000;

pub async fn update_session_summary(
    client: &reqwest::Client,
    settings: &Settings,
    existing_summary: Option<&str>,
    removed_messages: &[Message],
    cancel: &AtomicBool,
) -> Result<Option<String>, String> {
    if removed_messages.is_empty()
        || settings.api_key.trim().is_empty()
        || cancel.load(Ordering::Relaxed)
    {
        return Ok(existing_summary.map(str::to_string));
    }

    let removed_text = compact_messages(removed_messages);
    if removed_text.trim().is_empty() {
        return Ok(existing_summary.map(str::to_string));
    }

    let current = existing_summary.unwrap_or("（暂无）");
    let prompt = format!(
        r#"请维护一段「会话滚动摘要」，用于让助手在早期消息被上下文裁剪后仍理解当前会话。

要求：
- 只保留对后续有用的事实，不要编造。
- 保留用户偏好、明确要求、架构/实现决策、未完成事项、关键文件/命令/错误。
- 删除寒暄、重复过程和已经无关的细节。
- 用中文，简洁分点，最长约 4000 字。

已有摘要：
{current}

本次被裁剪的旧消息：
{removed_text}

请输出更新后的会话摘要，不要添加额外说明。"#
    );

    let messages = vec![
        Message::system("你是 Demiurge 的会话摘要器。你只输出摘要文本。"),
        Message::user(prompt),
    ];
    let tools = json!([]);
    let turn = llm::stream_completion(client, settings, &messages, &tools, |_| {}, cancel).await?;

    if cancel.load(Ordering::Relaxed) || turn.finish_reason == "interrupted" {
        return Ok(existing_summary.map(str::to_string));
    }

    let summary = cap_chars(turn.content.trim(), MAX_SUMMARY_CHARS);
    if summary.trim().is_empty() {
        Ok(existing_summary.map(str::to_string))
    } else {
        Ok(Some(summary))
    }
}

fn compact_messages(messages: &[Message]) -> String {
    let mut out = String::new();
    for m in messages {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&m.role);
        if let Some(name) = &m.name {
            out.push('(');
            out.push_str(name);
            out.push(')');
        }
        out.push_str(": ");

        if let Some(content) = &m.content {
            out.push_str(content.trim());
        } else if let Some(calls) = &m.tool_calls {
            let names = calls
                .iter()
                .map(|tc| tc.function.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str("[tool_calls: ");
            out.push_str(&names);
            out.push(']');
        }

        if out.chars().count() > MAX_REMOVED_CHARS {
            return cap_chars(out, MAX_REMOVED_CHARS);
        }
    }
    out
}

fn cap_chars(s: impl AsRef<str>, max_chars: usize) -> String {
    let s = s.as_ref();
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let head: String = s.chars().take(max_chars).collect();
        format!("{head}\n…[已截断]")
    }
}
