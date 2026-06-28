//! Phase 2：自动记忆提取。把长期有用的会话事实追加到 sandbox 内的 .demiurge/memory.md。
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
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

#[derive(Clone, Debug, Serialize)]
pub struct MemoryEntry {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub line: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct MemoryDuplicateGroup {
    pub canonical_id: String,
    pub duplicate_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MemoryPanelState {
    pub path: String,
    pub entries: Vec<MemoryEntry>,
    pub duplicates: Vec<MemoryDuplicateGroup>,
}

pub fn panel_state(sandbox_dir: &Path) -> MemoryPanelState {
    let path = memory_path(sandbox_dir);
    let raw = fs::read_to_string(&path).unwrap_or_default();
    let entries = parse_entries(&raw);
    let duplicates = audit_duplicates(&entries);
    MemoryPanelState {
        path: path.to_string_lossy().to_string(),
        entries,
        duplicates,
    }
}

pub fn update_entry(
    sandbox_dir: &Path,
    id: &str,
    kind: &str,
    text: &str,
) -> Result<MemoryPanelState, String> {
    let path = memory_path(sandbox_dir);
    let raw = fs::read_to_string(&path).unwrap_or_default();
    let mut lines = raw.lines().map(str::to_string).collect::<Vec<_>>();
    let entries = parse_entries(&raw);
    let entry = entries
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| "记忆条目不存在".to_string())?;
    let clean_text = sanitize_text(text);
    if clean_text.is_empty() {
        return Err("记忆内容不能为空".to_string());
    }
    let clean_kind = normalize_kind(kind);
    if entry.line == 0 || entry.line > lines.len() {
        return Err("记忆行号无效".to_string());
    }
    lines[entry.line - 1] = format!("- [{clean_kind}] {clean_text}");
    write_lines(&path, lines)?;
    Ok(panel_state(sandbox_dir))
}

pub fn delete_entry(sandbox_dir: &Path, id: &str) -> Result<MemoryPanelState, String> {
    let path = memory_path(sandbox_dir);
    let raw = fs::read_to_string(&path).unwrap_or_default();
    let mut lines = raw.lines().map(str::to_string).collect::<Vec<_>>();
    let entries = parse_entries(&raw);
    let entry = entries
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| "记忆条目不存在".to_string())?;
    if entry.line == 0 || entry.line > lines.len() {
        return Err("记忆行号无效".to_string());
    }
    lines.remove(entry.line - 1);
    write_lines(&path, lines)?;
    Ok(panel_state(sandbox_dir))
}

pub fn apply_dedupe(sandbox_dir: &Path) -> Result<MemoryPanelState, String> {
    let path = memory_path(sandbox_dir);
    let raw = fs::read_to_string(&path).unwrap_or_default();
    let entries = parse_entries(&raw);
    let duplicate_ids = audit_duplicates(&entries)
        .into_iter()
        .flat_map(|group| group.duplicate_ids)
        .collect::<HashSet<_>>();
    if duplicate_ids.is_empty() {
        return Ok(panel_state(sandbox_dir));
    }
    let lines = raw
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let line_no = idx + 1;
            let remove = entries
                .iter()
                .any(|entry| entry.line == line_no && duplicate_ids.contains(&entry.id));
            (!remove).then(|| line.to_string())
        })
        .collect::<Vec<_>>();
    write_lines(&path, lines)?;
    Ok(panel_state(sandbox_dir))
}
pub async fn extract_and_update(
    client: &reqwest::Client,
    settings: &Settings,
    sandbox_dir: &Path,
    user_text: &str,
    assistant_text: &str,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let profile = llm::ProviderProfile::for_kind(settings.provider);
    if !settings.auto_memory_enabled
        || (profile.requires_api_key && settings.api_key.trim().is_empty())
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

fn memory_path(sandbox_dir: &Path) -> std::path::PathBuf {
    sandbox_dir.join(".demiurge").join("memory.md")
}

fn parse_entries(raw: &str) -> Vec<MemoryEntry> {
    raw.lines()
        .enumerate()
        .filter_map(|(idx, line)| parse_entry_line(idx + 1, line))
        .collect()
}

fn parse_entry_line(line_no: usize, line: &str) -> Option<MemoryEntry> {
    let trimmed = line.trim();
    let body = trimmed.strip_prefix("- ")?.trim();
    let (kind, text) = if let Some(rest) = body.strip_prefix('[') {
        let (kind, text) = rest.split_once(']')?;
        (normalize_kind(kind), text.trim().to_string())
    } else {
        ("project".to_string(), body.to_string())
    };
    let text = sanitize_text(&text);
    if text.is_empty() {
        return None;
    }
    Some(MemoryEntry {
        id: format!("mem-{line_no}"),
        kind,
        text,
        line: line_no,
    })
}

fn audit_duplicates(entries: &[MemoryEntry]) -> Vec<MemoryDuplicateGroup> {
    let mut groups: Vec<MemoryDuplicateGroup> = Vec::new();
    for entry in entries {
        let key = normalize_for_dedupe(&entry.text);
        if key.is_empty() {
            continue;
        }
        if let Some(group) = groups.iter_mut().find(|group| {
            entries
                .iter()
                .find(|candidate| candidate.id == group.canonical_id)
                .map(|candidate| is_duplicate_key(&normalize_for_dedupe(&candidate.text), &key))
                .unwrap_or(false)
        }) {
            group.duplicate_ids.push(entry.id.clone());
        } else {
            groups.push(MemoryDuplicateGroup {
                canonical_id: entry.id.clone(),
                duplicate_ids: Vec::new(),
            });
        }
    }
    groups
        .into_iter()
        .filter(|group| !group.duplicate_ids.is_empty())
        .collect()
}

fn is_duplicate_key(a: &str, b: &str) -> bool {
    a == b || (a.len() > 16 && b.contains(a)) || (b.len() > 16 && a.contains(b))
}

fn write_lines(path: &Path, lines: Vec<String>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建记忆目录失败：{e}"))?;
    }
    let mut next = lines.join("\n");
    if !next.is_empty() {
        next.push('\n');
    }
    fs::write(path, next).map_err(|e| format!("写入记忆失败：{e}"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_audits_duplicate_memory_entries() {
        let raw = "# 自动记忆\n- [project] Prefer Rust tests before commits\n- [project] prefer rust tests before commits\n- [user] Likes concise summaries\n";
        let entries = parse_entries(raw);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].kind, "project");
        let duplicates = audit_duplicates(&entries);
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].duplicate_ids, vec!["mem-3".to_string()]);
    }

    #[test]
    fn updates_and_deletes_memory_entries_on_disk() {
        let dir =
            std::env::temp_dir().join(format!("demiurge_memory_{}", crate::store::now_millis()));
        let memory_dir = dir.join(".demiurge");
        std::fs::create_dir_all(&memory_dir).unwrap();
        std::fs::write(
            memory_dir.join("memory.md"),
            "# 自动记忆\n- [project] Old text\n",
        )
        .unwrap();

        let state = update_entry(&dir, "mem-2", "user", "New text").unwrap();
        assert_eq!(state.entries[0].kind, "user");
        assert_eq!(state.entries[0].text, "New text");

        let state = delete_entry(&dir, "mem-2").unwrap();
        assert!(state.entries.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
