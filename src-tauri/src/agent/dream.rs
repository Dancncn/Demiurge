//! Manual `/dream` memory consolidation.
//!
//! This is intentionally smaller than a full background Auto Dream system:
//! a user-triggered command reads the current local memory, asks the model to
//! consolidate it, then confirms before overwriting `.demiurge/memory.md`.
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use serde_json::json;
use tauri::{AppHandle, Emitter};

use super::conversation::Message;
use crate::permission::{self, PermissionDecision, PermissionRequest};
use crate::store::{self, Settings};
use crate::{llm, tools};

const MAX_INPUT_CHARS: usize = 18_000;
const MAX_OUTPUT_BYTES: usize = 32 * 1024;
const PREVIEW_CHARS: usize = 8_000;

pub async fn run_manual_dream(
    app: &AppHandle,
    state: &crate::AppState,
    command_text: String,
) -> Result<(), String> {
    state.cancel.store(false, Ordering::Relaxed);

    let settings = state.settings.lock().unwrap().clone();
    let sid = state.sessions.lock().unwrap().active.clone();
    push_message(state, &sid, Message::user(command_text));
    state.persist_sessions();

    let mut visible = String::new();
    emit_delta(app, &mut visible, "开始整理长期记忆...\n\n");

    let sandbox_dir = state.sandbox_dir.lock().unwrap().clone();
    let packs_dir = state.packs_dir.lock().unwrap().clone();
    let memory_path = sandbox_dir.join(".demiurge").join("memory.md");
    let current_memory = fs::read_to_string(&memory_path).unwrap_or_default();
    let session_snapshot = current_session_snapshot(state, &sid);
    let source = build_source_bundle(
        &sandbox_dir,
        &packs_dir,
        &settings,
        &current_memory,
        &session_snapshot,
    );

    if source.trim().is_empty() {
        emit_delta(app, &mut visible, "没有找到可整理的记忆材料。");
        finish(app, state, &sid, visible);
        return Ok(());
    }

    let prompt = format!(
        r#"请整理 Demiurge 的长期记忆文件。

目标：
- 输出新的 `.demiurge/memory.md` 完整内容，而不是补丁。
- 保留长期有用、稳定、可复用的信息。
- 合并重复项，删除过时、矛盾、一次性任务过程、普通寒暄。
- 不保存 API Key、密码、token、密钥、隐私敏感内容。
- 将相对时间（例如“昨天”“上周”）改写为明确事实；不确定就删除。
- 用 Markdown，保持短小清晰。建议分为：用户偏好、项目约束、工作方式、参考信息。
- 如果没有值得保存的信息，输出：
# 自动记忆

（暂无稳定长期记忆）

只输出 Markdown 正文，不要解释，不要代码围栏。

待整理材料：
{source}"#
    );

    let messages = vec![
        Message::system("你是 Demiurge 的长期记忆整理器。你只输出整理后的 Markdown 记忆文件。"),
        Message::user(prompt),
    ];

    let turn = llm::stream_completion(
        &state.http,
        &settings,
        &messages,
        &json!([]),
        |_| {},
        &state.cancel,
    )
    .await?;

    if state.cancel.load(Ordering::Relaxed) || turn.finish_reason == "interrupted" {
        let _ = app.emit("assistant-interrupted", ());
        return Ok(());
    }

    let next_memory = normalize_memory_output(&turn.content);
    if next_memory.trim().is_empty() {
        emit_delta(
            app,
            &mut visible,
            "模型没有输出可用的记忆内容，已跳过写入。",
        );
        finish(app, state, &sid, visible);
        return Ok(());
    }
    if next_memory.len() > MAX_OUTPUT_BYTES {
        emit_delta(
            app,
            &mut visible,
            "整理后的记忆超过 32 KiB，已跳过写入，避免污染上下文。",
        );
        finish(app, state, &sid, visible);
        return Ok(());
    }

    if normalize_for_compare(&current_memory) == normalize_for_compare(&next_memory) {
        emit_delta(app, &mut visible, "记忆已经足够干净，没有需要写入的变化。");
        finish(app, state, &sid, visible);
        return Ok(());
    }

    let preview = build_preview(&memory_path, &current_memory, &next_memory);
    let decision = PermissionDecision::from_policy(tools::PermissionPolicy::ask(
        "会整理并覆盖沙盒内的 .demiurge/memory.md。",
    ));
    permission::audit(state, "dream", &decision);
    let response = permission::confirm(
        app,
        state,
        PermissionRequest {
            tool: "dream",
            args_pretty: "{}",
            description: "整理长期记忆文件",
            risk: tools::ToolRisk::Mutating,
            decision: decision.clone(),
            preview: Some(preview),
        },
    )
    .await;
    let _ = permission::remember_response(state, "dream", &response);

    if !response.allow {
        emit_delta(app, &mut visible, "已取消写入，记忆文件保持不变。");
        finish(app, state, &sid, visible);
        return Ok(());
    }

    if let Some(parent) = memory_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建记忆目录失败：{e}"))?;
    }
    fs::write(&memory_path, next_memory).map_err(|e| format!("写入记忆失败：{e}"))?;

    emit_delta(
        app,
        &mut visible,
        "记忆整理完成，已更新沙盒 `.demiurge/memory.md`。",
    );
    finish(app, state, &sid, visible);
    Ok(())
}

fn push_message(state: &crate::AppState, sid: &str, msg: Message) {
    let mut storeg = state.sessions.lock().unwrap();
    if let Some(s) = storeg.get_mut(sid) {
        s.messages.push(msg);
        if s.title == "新对话" {
            s.title = store::derive_title(&s.messages);
        }
        s.updated_at = store::now_millis();
    }
}

fn emit_delta(app: &AppHandle, visible: &mut String, text: &str) {
    if visible.is_empty() {
        let _ = app.emit("assistant-start", ());
    }
    visible.push_str(text);
    let _ = app.emit("assistant-delta", text);
}

fn finish(app: &AppHandle, state: &crate::AppState, sid: &str, visible: String) {
    push_message(state, sid, Message::assistant_text(visible.clone()));
    state.persist_sessions();
    let _ = app.emit("assistant-done", visible);
}

fn current_session_snapshot(state: &crate::AppState, sid: &str) -> String {
    let storeg = state.sessions.lock().unwrap();
    let Some(s) = storeg.get(sid) else {
        return String::new();
    };

    let mut out = String::new();
    if let Some(summary) = &s.summary {
        out.push_str("# 会话摘要\n");
        out.push_str(summary.trim());
        out.push_str("\n\n");
    }

    out.push_str("# 最近消息\n");
    let start = s.messages.len().saturating_sub(12);
    for msg in s.messages.iter().skip(start) {
        if msg.role == "tool" {
            continue;
        }
        let content = msg.content.as_deref().unwrap_or_default().trim();
        if content.is_empty() {
            continue;
        }
        out.push_str("- ");
        out.push_str(&msg.role);
        out.push_str(": ");
        out.push_str(content);
        out.push('\n');
    }

    cap_chars(out, MAX_INPUT_CHARS / 3)
}

fn build_source_bundle(
    sandbox_dir: &Path,
    packs_dir: &Path,
    settings: &Settings,
    current_memory: &str,
    session_snapshot: &str,
) -> String {
    let mut parts = Vec::new();

    if !current_memory.trim().is_empty() {
        parts.push(format!(
            "# 当前 .demiurge/memory.md\n{}",
            current_memory.trim()
        ));
    }

    for (label, path) in [
        ("项目 memory.md", sandbox_dir.join("memory.md")),
        (
            "角色包 memory.md",
            packs_dir.join(&settings.current_pack).join("memory.md"),
        ),
        ("项目 DEMIURGE.md", sandbox_dir.join("DEMIURGE.md")),
        ("项目 CLAUDE.md", sandbox_dir.join("CLAUDE.md")),
    ] {
        if let Some(text) = read_limited_text(&path) {
            parts.push(format!("# {label}\n{}", text.trim()));
        }
    }

    if !session_snapshot.trim().is_empty() {
        parts.push(session_snapshot.trim().to_string());
    }

    cap_chars(parts.join("\n\n---\n\n"), MAX_INPUT_CHARS)
}

fn read_limited_text(path: &Path) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > 32 * 1024 {
        return None;
    }
    fs::read_to_string(path).ok()
}

fn normalize_memory_output(raw: &str) -> String {
    let mut text = raw.trim().to_string();
    if let Some(inner) = text.strip_prefix("```markdown") {
        text = inner.trim().trim_end_matches("```").trim().to_string();
    } else if let Some(inner) = text.strip_prefix("```md") {
        text = inner.trim().trim_end_matches("```").trim().to_string();
    } else if let Some(inner) = text.strip_prefix("```") {
        text = inner.trim().trim_end_matches("```").trim().to_string();
    }

    if text.trim().is_empty() {
        return String::new();
    }

    if !text.trim_start().starts_with('#') {
        text = format!("# 自动记忆\n\n{text}");
    }
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn normalize_for_compare(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn build_preview(path: &PathBuf, current: &str, next: &str) -> String {
    let current_lines = current.lines().count();
    let next_lines = next.lines().count();
    let body = cap_chars(next, PREVIEW_CHARS);
    format!(
        "目标文件：{}\n当前：{} 行，{} 字节\n整理后：{} 行，{} 字节\n\n整理后内容预览：\n\n{}",
        path.display(),
        current_lines,
        current.len(),
        next_lines,
        next.len(),
        body
    )
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
