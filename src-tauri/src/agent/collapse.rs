use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::{budget, conversation::Message, summary};
use crate::{llm, store};

const MANUAL_KEEP_RECENT: usize = 12;

#[derive(Clone, Debug, Serialize)]
pub struct ContextStats {
    pub session_id: String,
    pub message_count: usize,
    pub summary_chars: usize,
    pub estimated_history_tokens: usize,
    pub compactable_messages: usize,
}

pub fn inspect(state: &crate::AppState) -> ContextStats {
    let storeg = state.sessions.lock().unwrap();
    let sid = storeg.active.clone();
    let session = storeg.get(&sid);
    let messages = session.map(|s| s.messages.as_slice()).unwrap_or(&[]);
    let summary_chars = session
        .and_then(|s| s.summary.as_deref())
        .map(|s| s.chars().count())
        .unwrap_or(0);

    ContextStats {
        session_id: sid,
        message_count: messages.len(),
        summary_chars,
        estimated_history_tokens: budget::estimate_messages_tokens(messages),
        compactable_messages: messages.len().saturating_sub(MANUAL_KEEP_RECENT),
    }
}

pub async fn run_manual_compact(
    app: &AppHandle,
    state: &crate::AppState,
    raw_text: String,
) -> Result<(), String> {
    let keep_recent = parse_keep_recent(&raw_text).unwrap_or(MANUAL_KEEP_RECENT);
    let result = compact_active_session(state, keep_recent).await?;
    let text = if result.removed_messages == 0 {
        format!(
            "当前上下文无需折叠。消息数：{}，估算历史 token：{}。",
            result.after.message_count, result.after.estimated_history_tokens
        )
    } else {
        format!(
            "已折叠上下文：压缩 {} 条旧消息，保留最近 {} 条。当前消息数：{}，摘要约 {} 字。",
            result.removed_messages,
            keep_recent,
            result.after.message_count,
            result.after.summary_chars
        )
    };
    let _ = app.emit("assistant-done", text.clone());
    Ok(())
}

#[derive(Clone, Debug)]
pub struct CompactResult {
    pub removed_messages: usize,
    pub after: ContextStats,
}

pub async fn compact_active_session(
    state: &crate::AppState,
    keep_recent: usize,
) -> Result<CompactResult, String> {
    let settings = state.settings.lock().unwrap().clone();
    let profile = llm::ProviderProfile::for_kind(settings.provider);
    if profile.requires_api_key && settings.api_key.trim().is_empty() {
        return Err("当前 provider 需要 API Key，无法调用摘要模型折叠上下文。".to_string());
    }

    let sid = state.sessions.lock().unwrap().active.clone();
    let (removed, existing_summary) = {
        let mut storeg = state.sessions.lock().unwrap();
        let Some(session) = storeg.get_mut(&sid) else {
            return Err("当前会话不存在".to_string());
        };
        let split_at = session.messages.len().saturating_sub(keep_recent);
        if split_at == 0 {
            return Ok(CompactResult {
                removed_messages: 0,
                after: inspect(state),
            });
        }
        let removed = drain_prefix_preserving_pairs(&mut session.messages, split_at);
        session.updated_at = store::now_millis();
        (removed, session.summary.clone())
    };

    if removed.is_empty() {
        return Ok(CompactResult {
            removed_messages: 0,
            after: inspect(state),
        });
    }

    let next_summary = summary::update_session_summary(
        &state.http,
        &settings,
        existing_summary.as_deref(),
        &removed,
        &state.cancel,
    )
    .await?;

    {
        let mut storeg = state.sessions.lock().unwrap();
        if let Some(session) = storeg.get_mut(&sid) {
            session.summary = next_summary;
            session.updated_at = store::now_millis();
        }
    }
    state.persist_sessions();

    Ok(CompactResult {
        removed_messages: removed.len(),
        after: inspect(state),
    })
}

fn drain_prefix_preserving_pairs(messages: &mut Vec<Message>, split_at: usize) -> Vec<Message> {
    let split_at = split_at.min(messages.len());
    let mut removed = messages.drain(0..split_at).collect::<Vec<_>>();
    while matches!(messages.first(), Some(m) if m.role == "tool") {
        removed.push(messages.remove(0));
    }
    removed
}

fn parse_keep_recent(raw: &str) -> Option<usize> {
    raw.split_whitespace()
        .find_map(|part| part.strip_prefix("keep="))
        .and_then(|n| n.parse::<usize>().ok())
        .filter(|n| *n >= 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_keep_recent_option() {
        assert_eq!(parse_keep_recent("/compact keep=20"), Some(20));
        assert_eq!(parse_keep_recent("/compact keep=1"), None);
    }

    #[test]
    fn drains_orphan_tool_results_after_prefix() {
        let mut messages = vec![
            Message::user("a"),
            Message::assistant_text("b"),
            Message::tool_result("c", "read_file", "result"),
            Message::user("d"),
        ];
        let removed = drain_prefix_preserving_pairs(&mut messages, 2);
        assert_eq!(removed.len(), 3);
        assert_eq!(messages.first().unwrap().role, "user");
    }
}
