//! Long-term memory extraction and manual maintenance.
//!
//! Automatic extraction still appends durable memories to the project scope.
//! The maintenance API exposes user/project/session/pack scopes as editable
//! Markdown files.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
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
    pub scope: String,
    #[serde(rename = "scopeLabel")]
    pub scope_label: String,
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
pub struct MemoryScopeState {
    pub id: String,
    pub label: String,
    pub path: String,
    pub entries: Vec<MemoryEntry>,
    pub duplicates: Vec<MemoryDuplicateGroup>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MemoryPanelState {
    pub path: String,
    pub entries: Vec<MemoryEntry>,
    pub duplicates: Vec<MemoryDuplicateGroup>,
    pub scopes: Vec<MemoryScopeState>,
    /// 当前生效的 memory namespace（来自角色卡 runtime.memory.namespace）。
    /// None 表示走默认共享路径；Some(ns) 表示 user/project 已隔离到带后缀文件。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

#[derive(Clone, Debug)]
struct MemoryScopeFile {
    id: &'static str,
    label: &'static str,
    path: PathBuf,
}

pub fn panel_state(
    data_dir: &Path,
    sandbox_dir: &Path,
    packs_dir: &Path,
    pack_id: &str,
    session_id: &str,
) -> MemoryPanelState {
    let scope_files = scope_files(data_dir, sandbox_dir, packs_dir, pack_id, session_id);
    let scopes = scope_files
        .iter()
        .map(|scope| {
            let raw = fs::read_to_string(&scope.path).unwrap_or_default();
            let entries = parse_entries(scope.id, scope.label, &raw);
            let duplicates = audit_duplicates(&entries);
            MemoryScopeState {
                id: scope.id.to_string(),
                label: scope.label.to_string(),
                path: scope.path.to_string_lossy().to_string(),
                entries,
                duplicates,
            }
        })
        .collect::<Vec<_>>();
    let entries = scopes
        .iter()
        .flat_map(|scope| scope.entries.clone())
        .collect::<Vec<_>>();
    let duplicates = scopes
        .iter()
        .flat_map(|scope| scope.duplicates.clone())
        .collect::<Vec<_>>();
    let path = scope_files
        .iter()
        .find(|scope| scope.id == "project")
        .map(|scope| scope.path.to_string_lossy().to_string())
        .unwrap_or_default();
    MemoryPanelState {
        path,
        entries,
        duplicates,
        scopes,
        namespace: active_namespace(packs_dir, pack_id),
    }
}

pub fn add_entry(
    data_dir: &Path,
    sandbox_dir: &Path,
    packs_dir: &Path,
    pack_id: &str,
    session_id: &str,
    scope_id: &str,
    kind: &str,
    text: &str,
) -> Result<MemoryPanelState, String> {
    let scope = scope_files(data_dir, sandbox_dir, packs_dir, pack_id, session_id)
        .into_iter()
        .find(|scope| scope.id == scope_id)
        .ok_or_else(|| format!("Unknown memory scope: {scope_id}"))?;
    let clean_text = sanitize_text(text);
    if clean_text.is_empty() {
        return Err("Memory text cannot be empty".to_string());
    }
    let clean_kind = normalize_kind(kind);
    let mut lines = fs::read_to_string(&scope.path)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push(format!("# {} memory", scope.label));
    }
    lines.push(format!("- [{clean_kind}] {clean_text}"));
    write_lines(&scope.path, lines)?;
    Ok(panel_state(
        data_dir,
        sandbox_dir,
        packs_dir,
        pack_id,
        session_id,
    ))
}

pub fn update_entry(
    data_dir: &Path,
    sandbox_dir: &Path,
    packs_dir: &Path,
    pack_id: &str,
    session_id: &str,
    id: &str,
    kind: &str,
    text: &str,
) -> Result<MemoryPanelState, String> {
    let scope = find_scope_file(data_dir, sandbox_dir, packs_dir, pack_id, session_id, id)?;
    let path = scope.path;
    let raw = fs::read_to_string(&path).unwrap_or_default();
    let mut lines = raw.lines().map(str::to_string).collect::<Vec<_>>();
    let entries = parse_entries(scope.id, scope.label, &raw);
    let entry = entries
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| "Memory entry does not exist".to_string())?;
    let clean_text = sanitize_text(text);
    if clean_text.is_empty() {
        return Err("Memory text cannot be empty".to_string());
    }
    let clean_kind = normalize_kind(kind);
    if entry.line == 0 || entry.line > lines.len() {
        return Err("Memory line is invalid".to_string());
    }
    lines[entry.line - 1] = format!("- [{clean_kind}] {clean_text}");
    write_lines(&path, lines)?;
    Ok(panel_state(
        data_dir,
        sandbox_dir,
        packs_dir,
        pack_id,
        session_id,
    ))
}

pub fn delete_entry(
    data_dir: &Path,
    sandbox_dir: &Path,
    packs_dir: &Path,
    pack_id: &str,
    session_id: &str,
    id: &str,
) -> Result<MemoryPanelState, String> {
    let scope = find_scope_file(data_dir, sandbox_dir, packs_dir, pack_id, session_id, id)?;
    let path = scope.path;
    let raw = fs::read_to_string(&path).unwrap_or_default();
    let mut lines = raw.lines().map(str::to_string).collect::<Vec<_>>();
    let entries = parse_entries(scope.id, scope.label, &raw);
    let entry = entries
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| "Memory entry does not exist".to_string())?;
    if entry.line == 0 || entry.line > lines.len() {
        return Err("Memory line is invalid".to_string());
    }
    lines.remove(entry.line - 1);
    write_lines(&path, lines)?;
    Ok(panel_state(
        data_dir,
        sandbox_dir,
        packs_dir,
        pack_id,
        session_id,
    ))
}

pub fn apply_dedupe(
    data_dir: &Path,
    sandbox_dir: &Path,
    packs_dir: &Path,
    pack_id: &str,
    session_id: &str,
) -> Result<MemoryPanelState, String> {
    for scope in scope_files(data_dir, sandbox_dir, packs_dir, pack_id, session_id) {
        let raw = fs::read_to_string(&scope.path).unwrap_or_default();
        let entries = parse_entries(scope.id, scope.label, &raw);
        let duplicate_ids = audit_duplicates(&entries)
            .into_iter()
            .flat_map(|group| group.duplicate_ids)
            .collect::<HashSet<_>>();
        if duplicate_ids.is_empty() {
            continue;
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
        write_lines(&scope.path, lines)?;
    }
    Ok(panel_state(
        data_dir,
        sandbox_dir,
        packs_dir,
        pack_id,
        session_id,
    ))
}

/// 把 from_ns 命名空间下的 user/project 记忆文件拷贝到 to_ns。
/// 用于角色包切换 namespace 时迁移已有记忆；源文件保留，目标存在则覆盖。
/// from_ns 可以是 "default"（从默认共享路径迁出），to_ns 不能是 "default"。
pub fn migrate_namespace(
    data_dir: &Path,
    sandbox_dir: &Path,
    from_ns: &str,
    to_ns: &str,
) -> Result<(), String> {
    let from_ns = from_ns.trim();
    let to_ns = to_ns.trim();
    if from_ns.is_empty() || to_ns.is_empty() {
        return Err("namespace 不能为空".to_string());
    }
    if from_ns == to_ns {
        return Ok(());
    }
    if to_ns == "default" {
        return Err("目标 namespace 不能是 default".to_string());
    }
    let user_from = data_dir
        .join("memory")
        .join(namespaced_file_name("user", Some(from_ns)));
    let user_to = data_dir
        .join("memory")
        .join(namespaced_file_name("user", Some(to_ns)));
    let proj_from = memory_path(sandbox_dir, Some(from_ns));
    let proj_to = memory_path(sandbox_dir, Some(to_ns));
    copy_memory_file(&user_from, &user_to)?;
    copy_memory_file(&proj_from, &proj_to)?;
    Ok(())
}

fn copy_memory_file(from: &Path, to: &Path) -> Result<(), String> {
    if !from.exists() {
        return Ok(());
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{e}"))?;
    }
    fs::copy(from, to).map_err(|e| format!("复制记忆文件失败：{e}"))?;
    Ok(())
}

pub async fn extract_and_update(
    client: &reqwest::Client,
    settings: &Settings,
    sandbox_dir: &Path,
    packs_dir: &Path,
    pack_id: &str,
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
        format!("User:\n{user_text}\n\nAssistant:\n{assistant_text}"),
        MAX_INPUT_CHARS,
    );
    let prompt = format!(
        r#"Extract long-term memories worth preserving from this conversation turn.
Keep only stable, durable information:
- user preferences and working style;
- project constraints and architecture decisions;
- facts that should influence future conversations.

Do not record temporary task steps, one-off bugs, ordinary chat, secrets, tokens, full command output, stack traces, or uncertain guesses.
If there is nothing worth remembering, output {{"memories":[]}}.
Return at most 3 items. Each text must be under 240 characters.
Return JSON only:
{{"memories":[{{"kind":"user|project|preference","text":"..."}}]}}

Conversation:
{turn_text}"#
    );

    let messages = vec![
        Message::system("You are Demiurge's long-term memory extractor. Output JSON only."),
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

    let namespace = active_namespace(packs_dir, pack_id);
    append_entries(sandbox_dir, namespace.as_deref(), &entries)
}

pub fn scoped_memory_paths(
    data_dir: &Path,
    sandbox_dir: &Path,
    packs_dir: &Path,
    pack_id: &str,
    session_id: &str,
) -> Vec<(String, String, PathBuf)> {
    scope_files(data_dir, sandbox_dir, packs_dir, pack_id, session_id)
        .into_iter()
        .map(|scope| (scope.id.to_string(), scope.label.to_string(), scope.path))
        .collect()
}

/// 当前 project scope 的记忆文件路径（已按 namespace 隔离）。供 /dream 等直接读写项目记忆的调用方使用。
pub fn project_memory_path(sandbox_dir: &Path, packs_dir: &Path, pack_id: &str) -> PathBuf {
    let ns = active_namespace(packs_dir, pack_id);
    memory_path(sandbox_dir, ns.as_deref())
}

fn scope_files(
    data_dir: &Path,
    sandbox_dir: &Path,
    packs_dir: &Path,
    pack_id: &str,
    session_id: &str,
) -> Vec<MemoryScopeFile> {
    let session_id = sanitize_path_segment(session_id);
    let ns = active_namespace(packs_dir, pack_id);
    let user_name = namespaced_file_name("user", ns.as_deref());
    vec![
        MemoryScopeFile {
            id: "user",
            label: "User",
            path: data_dir.join("memory").join(user_name),
        },
        MemoryScopeFile {
            id: "project",
            label: "Project",
            path: memory_path(sandbox_dir, ns.as_deref()),
        },
        MemoryScopeFile {
            id: "session",
            label: "Session",
            path: sandbox_dir
                .join(".demiurge")
                .join("session-memory")
                .join(format!("{session_id}.md")),
        },
        MemoryScopeFile {
            id: "pack",
            label: "Pack",
            path: packs_dir.join(pack_id).join("memory.md"),
        },
    ]
}

/// 当前角色包声明的 memory namespace；None 表示走默认共享路径。
fn active_namespace(packs_dir: &Path, pack_id: &str) -> Option<String> {
    crate::pack::manifest_namespace(packs_dir, pack_id)
}

/// 按 namespace 生成记忆文件名：默认 `user.md`/`memory.md`，命名空间下 `user.{ns}.md`/`memory.{ns}.md`。
fn namespaced_file_name(base: &str, ns: Option<&str>) -> String {
    match ns.map(str::trim) {
        Some(n) if !n.is_empty() && n != "default" => format!("{base}.{n}.md"),
        _ => format!("{base}.md"),
    }
}

fn find_scope_file(
    data_dir: &Path,
    sandbox_dir: &Path,
    packs_dir: &Path,
    pack_id: &str,
    session_id: &str,
    entry_id: &str,
) -> Result<MemoryScopeFile, String> {
    let scope_id = entry_id
        .split_once(':')
        .map(|(scope, _)| scope)
        .unwrap_or("project");
    scope_files(data_dir, sandbox_dir, packs_dir, pack_id, session_id)
        .into_iter()
        .find(|scope| scope.id == scope_id)
        .ok_or_else(|| format!("Unknown memory scope: {scope_id}"))
}

fn memory_path(sandbox_dir: &Path, namespace: Option<&str>) -> PathBuf {
    let name = namespaced_file_name("memory", namespace);
    sandbox_dir.join(".demiurge").join(name)
}

fn parse_entries(scope: &str, scope_label: &str, raw: &str) -> Vec<MemoryEntry> {
    raw.lines()
        .enumerate()
        .filter_map(|(idx, line)| parse_entry_line(scope, scope_label, idx + 1, line))
        .collect()
}

fn parse_entry_line(
    scope: &str,
    scope_label: &str,
    line_no: usize,
    line: &str,
) -> Option<MemoryEntry> {
    let trimmed = line.trim();
    let body = trimmed.strip_prefix("- ")?.trim();
    let (kind, text) = if let Some(rest) = body.strip_prefix('[') {
        let (kind, text) = rest.split_once(']')?;
        (normalize_kind(kind), text.trim().to_string())
    } else {
        (scope.to_string(), body.to_string())
    };
    let text = sanitize_text(&text);
    if text.is_empty() {
        return None;
    }
    Some(MemoryEntry {
        id: format!("{scope}:mem-{line_no}"),
        scope: scope.to_string(),
        scope_label: scope_label.to_string(),
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
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create memory directory: {e}"))?;
    }
    let mut next = lines.join("\n");
    if !next.is_empty() {
        next.push('\n');
    }
    fs::write(path, next).map_err(|e| format!("Failed to write memory: {e}"))
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
        .map_err(|e| format!("Failed to parse memory extraction result: {e}"))
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

fn append_entries(
    sandbox_dir: &Path,
    namespace: Option<&str>,
    entries: &[(String, String)],
) -> Result<(), String> {
    let memory_dir = sandbox_dir.join(".demiurge");
    let memory_path = memory_path(sandbox_dir, namespace);

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

    fs::create_dir_all(&memory_dir)
        .map_err(|e| format!("Failed to create memory directory: {e}"))?;

    let mut next = existing;
    if next.trim().is_empty() {
        next.push_str("# Automatic memory\n");
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

    fs::write(&memory_path, next).map_err(|e| format!("Failed to write memory: {e}"))
}

fn normalize_kind(kind: &str) -> String {
    match kind.trim().to_ascii_lowercase().as_str() {
        "user" => "user".to_string(),
        "session" => "session".to_string(),
        "pack" => "pack".to_string(),
        "preference" => "preference".to_string(),
        "boundary" => "boundary".to_string(),
        "routine" => "routine".to_string(),
        "stress" => "stress".to_string(),
        "encouragement" => "encouragement".to_string(),
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
        .trim_start_matches("[session]")
        .trim_start_matches("[pack]")
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

fn sanitize_path_segment(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "default".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_audits_duplicate_memory_entries() {
        let raw = "# Automatic memory\n- [project] Prefer Rust tests before commits\n- [project] prefer rust tests before commits\n- [user] Likes concise summaries\n";
        let entries = parse_entries("project", "Project", raw);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].id, "project:mem-2");
        assert_eq!(entries[0].kind, "project");
        let duplicates = audit_duplicates(&entries);
        assert_eq!(duplicates.len(), 1);
        assert_eq!(
            duplicates[0].duplicate_ids,
            vec!["project:mem-3".to_string()]
        );
    }

    #[test]
    fn updates_and_deletes_memory_entries_on_disk() {
        let root =
            std::env::temp_dir().join(format!("demiurge_memory_{}", crate::store::now_millis()));
        let data = root.join("data");
        let sandbox = root.join("sandbox");
        let packs = root.join("packs");
        let memory_dir = sandbox.join(".demiurge");
        std::fs::create_dir_all(&memory_dir).unwrap();
        std::fs::write(
            memory_dir.join("memory.md"),
            "# Automatic memory\n- [project] Old text\n",
        )
        .unwrap();

        let state = update_entry(
            &data,
            &sandbox,
            &packs,
            "default",
            "session_1",
            "project:mem-2",
            "user",
            "New text",
        )
        .unwrap();
        let project = state
            .scopes
            .iter()
            .find(|scope| scope.id == "project")
            .unwrap();
        assert_eq!(project.entries[0].kind, "user");
        assert_eq!(project.entries[0].text, "New text");

        let state = delete_entry(
            &data,
            &sandbox,
            &packs,
            "default",
            "session_1",
            "project:mem-2",
        )
        .unwrap();
        let project = state
            .scopes
            .iter()
            .find(|scope| scope.id == "project")
            .unwrap();
        assert!(project.entries.is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn adds_entries_to_user_session_and_pack_scopes() {
        let root = std::env::temp_dir().join(format!(
            "demiurge_memory_scopes_{}",
            crate::store::now_millis()
        ));
        let data = root.join("data");
        let sandbox = root.join("sandbox");
        let packs = root.join("packs");

        let state = add_entry(
            &data,
            &sandbox,
            &packs,
            "default",
            "session/1",
            "user",
            "preference",
            "Use concise summaries",
        )
        .unwrap();
        assert!(state
            .scopes
            .iter()
            .find(|scope| scope.id == "user")
            .unwrap()
            .entries
            .iter()
            .any(|entry| entry.text == "Use concise summaries"));

        add_entry(
            &data,
            &sandbox,
            &packs,
            "default",
            "session/1",
            "session",
            "session",
            "Current task is memory layering",
        )
        .unwrap();
        add_entry(
            &data,
            &sandbox,
            &packs,
            "default",
            "session/1",
            "pack",
            "pack",
            "Pack-specific tone note",
        )
        .unwrap();
        assert!(sandbox
            .join(".demiurge")
            .join("session-memory")
            .join("session_1.md")
            .is_file());
        assert!(packs.join("default").join("memory.md").is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn namespaced_file_name_formats_paths() {
        assert_eq!(namespaced_file_name("user", None), "user.md");
        assert_eq!(namespaced_file_name("user", Some("")), "user.md");
        assert_eq!(namespaced_file_name("user", Some("default")), "user.md");
        assert_eq!(namespaced_file_name("user", Some("alice")), "user.alice.md");
        assert_eq!(
            namespaced_file_name("memory", Some("alice")),
            "memory.alice.md"
        );
    }

    #[test]
    fn scope_files_isolate_user_project_by_namespace() {
        let root =
            std::env::temp_dir().join(format!("demiurge_memory_ns_{}", crate::store::now_millis()));
        let data = root.join("data");
        let sandbox = root.join("sandbox");
        let packs = root.join("packs");

        // 声明 namespace=alice 的角色包
        let alice_dir = packs.join("alice-pack");
        std::fs::create_dir_all(&alice_dir).unwrap();
        std::fs::write(
            alice_dir.join("manifest.json"),
            r#"{"id":"alice-pack","name":"Alice","persona":"persona.md","runtime":{"memory":{"namespace":"alice"}}}"#,
        )
        .unwrap();
        std::fs::write(alice_dir.join("persona.md"), "persona").unwrap();

        let scopes = scope_files(&data, &sandbox, &packs, "alice-pack", "session/1");
        let user = scopes.iter().find(|s| s.id == "user").unwrap();
        let project = scopes.iter().find(|s| s.id == "project").unwrap();
        let session = scopes.iter().find(|s| s.id == "session").unwrap();
        let pack = scopes.iter().find(|s| s.id == "pack").unwrap();
        let norm = |p: &std::path::PathBuf| p.to_string_lossy().replace('\\', "/");
        assert!(norm(&user.path).ends_with("user.alice.md"));
        assert!(norm(&project.path).ends_with("memory.alice.md"));
        // session 与 pack 层不受 namespace 影响
        assert!(norm(&session.path).ends_with("session_1.md"));
        assert!(norm(&pack.path).ends_with("alice-pack/memory.md"));

        // 未声明 namespace 的包走 legacy 共享路径
        let default_dir = packs.join("default");
        std::fs::create_dir_all(&default_dir).unwrap();
        std::fs::write(
            default_dir.join("manifest.json"),
            r#"{"id":"default","name":"Default","persona":"persona.md"}"#,
        )
        .unwrap();
        std::fs::write(default_dir.join("persona.md"), "persona").unwrap();
        let scopes = scope_files(&data, &sandbox, &packs, "default", "session/1");
        let user = scopes.iter().find(|s| s.id == "user").unwrap();
        let project = scopes.iter().find(|s| s.id == "project").unwrap();
        assert!(norm(&user.path).ends_with("user.md"));
        assert!(norm(&project.path).ends_with("memory.md"));

        let _ = std::fs::remove_dir_all(root);
    }
}
