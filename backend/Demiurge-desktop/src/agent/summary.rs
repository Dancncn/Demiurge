//! Phase 2：会话滚动摘要。把被上下文裁剪移除的旧消息压缩为短期会话状态。
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use serde_json::json;

use super::conversation::Message;
use crate::agent::budget;
use crate::llm;
use crate::store::Settings;
use crate::AppState;

const MAX_REMOVED_CHARS: usize = 12_000;
const MAX_SUMMARY_CHARS: usize = 6_000;

pub async fn update_session_summary(
    state: &AppState,
    session_id: &str,
    client: &reqwest::Client,
    settings: &Settings,
    existing_summary: Option<&str>,
    removed_messages: &[Message],
    cancel: &AtomicBool,
) -> Result<Option<String>, String> {
    let Some((messages, tools)) =
        build_summary_request(settings, existing_summary, removed_messages, cancel)
    else {
        return Ok(existing_summary.map(str::to_string));
    };
    let request_started = Instant::now();
    let turn_result =
        llm::stream_completion(client, settings, &messages, &tools, |_| {}, cancel).await;
    let _ = crate::usage::record(
        state,
        crate::usage::UsageRecordInput {
            session_id,
            provider: &llm::provider_name(settings.provider),
            model: &settings.model,
            purpose: "compaction",
            usage: turn_result.as_ref().ok().and_then(|turn| turn.usage),
            fallback_total_tokens: budget::estimate_messages_tokens(&messages).saturating_add(
                turn_result
                    .as_ref()
                    .map_or(0, |turn| budget::estimate_text_tokens(&turn.content)),
            ) as u64,
            latency_ms: request_started.elapsed().as_millis() as u64,
            status: if turn_result.is_ok() {
                "success"
            } else {
                "failed"
            },
        },
    );
    let turn = turn_result?;

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

/// Build the exact auxiliary request used by compaction. Callers that own a
/// session can record this envelope immediately before invoking
/// `update_session_summary`, keeping summary input reconstructible without
/// moving compaction into the main agent loop.
pub fn build_summary_request(
    settings: &Settings,
    existing_summary: Option<&str>,
    removed_messages: &[Message],
    cancel: &AtomicBool,
) -> Option<(Vec<Message>, serde_json::Value)> {
    let profile = llm::ProviderProfile::for_kind(settings.provider);
    if removed_messages.is_empty()
        || (profile.requires_api_key && settings.api_key.trim().is_empty())
        || cancel.load(Ordering::Relaxed)
    {
        return None;
    }

    let removed_text = compact_messages(removed_messages);
    if removed_text.trim().is_empty() {
        return None;
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

    Some((
        vec![
            Message::system("你是 Demiurge 的会话摘要器。你只输出摘要文本。"),
            Message::user(prompt),
        ],
        json!([]),
    ))
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
