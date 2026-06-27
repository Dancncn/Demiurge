//! Phase 2：自动记忆提取。把长期有用的会话事实追加到 sandbox 内的 .demiurge/memory.md。
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Deserialize;
use serde_json::json;

use super::conversation::Message;
use crate::llm;
use crate::store::Settings;

const MAX_MEMORY_FILE_BYTES: u64 = 32 * 1024;
const MAX_INPUT_CHARS: usize = 8_000;
const MAX_MEMORIES_PER_TURN: usize = 3;
const MAX_MEMORY_CHARS: usize = 240;

#[derive(Deserialize)]
struct MemoryExtraction {
    #[serde(default)]
    memories: Vec<MemoryCandidate>,
}

#[derive(Deserialize)]
struct MemoryCandidate {
    kind: Option<String>,
    text: Option<String>,
}

pub async fn extract_and_update(
    client: &reqwest::Client,
    settings: &Settings,
    sandbox_dir: &Path,
    user_text: &str,
    assistant_text: &str,
    cancel: &AtomicBool,
) -> Result<(), String> {
    if !settings.auto_memory_enabled
        || settings.api_key.trim().is_empty()
        || cancel.load(Ordering::Relaxed)
    {
        return Ok(());
    }

    let turn_text = cap_chars(
        format!("用户：\n{user_text}\n\n助手：\n{assistant_text}"),
        MAX_INPUT_CHARS,
    );
    let prompt = format!(
        r#"请从下面这轮对话中提取值得长期保存的记忆。

只记录长期有用且稳定的信息，例如：
- 用户偏好、工作方式偏好、明确要求；
- 项目长期约束、架构决策、持续适用的注意事项；
- 未来多次对话都应该遵守的事实。

不要记录：
- 临时任务步骤、一次性 bug、普通聊天寒暄；
- API Key、密码、token、密钥、隐私敏感内容；
- 完整命令输出、工具输出全文、错误堆栈全文；
- 不确定或推测的信息。

如果没有值得记忆的信息，输出 {{"memories":[]}}。
最多输出 3 条，每条 text 不超过 240 个中文字符。
只输出 JSON，不要 Markdown，不要解释。
格式：{{"memories":[{{"kind":"user|project|preference","text":"..."}}]}}

对话：
{turn_text}"#
    );

    let messages = vec![
        Message::system("你是 Demiurge 的长期记忆提取器。你只输出 JSON。"),
        Message::user(prompt),
    ];
    let turn =
        llm::stream_completion(client, settings, &messages, &json!([]), |_| {}, cancel).await?;
    if cancel.load(Ordering::Relaxed) || turn.finish_reason == "interrupted" {
        return Ok(());
    }

    let extraction = parse_extraction(&turn.content)?;
    let entries = normalize_candidates(extraction.memories);
    if entries.is_empty() {
        return Ok(());
    }

    append_entries(sandbox_dir, &entries)
}

fn parse_extraction(content: &str) -> Result<MemoryExtraction, String> {
    let trimmed = content.trim();
    let json_text = if let Some(inner) = trimmed.strip_prefix("```json") {
        inner.trim().trim_end_matches("```").trim()
    } else if let Some(inner) = trimmed.strip_prefix("```") {
        inner.trim().trim_end_matches("```").trim()
    } else {
        trimmed
    };
    serde_json::from_str::<MemoryExtraction>(json_text)
        .map_err(|e| format!("解析记忆提取结果失败：{e}"))
}

fn normalize_candidates(candidates: Vec<MemoryCandidate>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for candidate in candidates.into_iter().take(MAX_MEMORIES_PER_TURN) {
        let kind = normalize_kind(candidate.kind.as_deref().unwrap_or("project"));
        let text = sanitize_text(candidate.text.as_deref().unwrap_or_default());
        if text.is_empty() {
            continue;
        }
        let key = normalize_for_dedupe(&text);
        if seen.insert(key) {
            out.push((kind, cap_chars(text, MAX_MEMORY_CHARS)));
        }
    }

    out
}

fn append_entries(sandbox_dir: &Path, entries: &[(String, String)]) -> Result<(), String> {
    let memory_dir = sandbox_dir.join(".demiurge");
    let memory_path = memory_dir.join("memory.md");

    if let Ok(meta) = fs::metadata(&memory_path) {
        if meta.len() > MAX_MEMORY_FILE_BYTES {
            return Ok(());
        }
    }

    let existing = fs::read_to_string(&memory_path).unwrap_or_default();
    let mut seen = existing
        .lines()
        .map(normalize_for_dedupe)
        .filter(|s| !s.is_empty())
        .collect::<HashSet<_>>();

    let mut additions = Vec::new();
    for (kind, text) in entries {
        let line = format!("- [{kind}] {text}");
        if seen.insert(normalize_for_dedupe(&line)) && seen.insert(normalize_for_dedupe(text)) {
            additions.push(line);
        }
    }

    if additions.is_empty() {
        return Ok(());
    }

    fs::create_dir_all(&memory_dir).map_err(|e| format!("创建记忆目录失败：{e}"))?;

    let mut next = existing;
    if next.trim().is_empty() {
        next.push_str("# 自动记忆\n");
    }
    if !next.ends_with('\n') {
        next.push('\n');
    }
    next.push('\n');
    next.push_str(&additions.join("\n"));
    next.push('\n');

    if next.len() as u64 > MAX_MEMORY_FILE_BYTES {
        return Ok(());
    }

    fs::write(&memory_path, next).map_err(|e| format!("写入记忆失败：{e}"))
}

fn normalize_kind(kind: &str) -> String {
    match kind.trim().to_ascii_lowercase().as_str() {
        "user" => "user".to_string(),
        "preference" => "preference".to_string(),
        _ => "project".to_string(),
    }
}

fn sanitize_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_for_dedupe(text: impl AsRef<str>) -> String {
    text.as_ref()
        .trim()
        .trim_start_matches('-')
        .trim()
        .trim_start_matches("[user]")
        .trim_start_matches("[project]")
        .trim_start_matches("[preference]")
        .trim()
        .to_ascii_lowercase()
}

fn cap_chars(s: impl AsRef<str>, max_chars: usize) -> String {
    let s = s.as_ref();
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        s.chars().take(max_chars).collect()
    }
}
