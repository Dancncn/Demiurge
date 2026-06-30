//! Prompt context builder.
//!
//! The runner still consumes a single system prompt string, but this module now
//! builds it from ordered sections with explicit priorities and a character
//! budget. The same builder can return a report for the UI.
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;

use crate::store::Settings;

const MAX_TEXT_FILE_BYTES: u64 = 32 * 1024;
const MAX_PROJECT_CHARS: usize = 14_000;
const MAX_MEMORY_CHARS: usize = 8_000;
const MAX_DIRECTORY_ENTRIES: usize = 90;
const MAX_DIRECTORY_DEPTH: usize = 2;
const MIN_TRUNCATED_SECTION_CHARS: usize = 600;
const GIT_TIMEOUT_SECS: u64 = 5;
/// Git 状态快照的复用窗口：一轮工具回合内（runner 每个 step 都会重建 prompt）复用
/// 同一结果，过期才重新执行 `git status`，避免每步都付一次子进程开销。
const GIT_SNAPSHOT_TTL: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, Serialize)]
pub struct PromptSectionReport {
    pub id: String,
    pub title: String,
    pub priority: u8,
    pub chars: usize,
    pub original_chars: usize,
    pub tokens: usize,
    pub included: bool,
    pub truncated: bool,
}

#[derive(Clone, Debug)]
pub struct PromptBuild {
    pub text: String,
    pub sections: Vec<PromptSectionReport>,
    pub prompt_chars: usize,
}

#[derive(Clone, Debug)]
struct SectionDraft {
    id: &'static str,
    title: &'static str,
    priority: u8,
    body: String,
}

#[derive(Clone, Debug)]
struct SectionDecision {
    body: String,
    truncated: bool,
}

pub fn build_for_input(
    state: &crate::AppState,
    settings: &Settings,
    persona_text: &str,
    session_summary: Option<&str>,
    user_text: &str,
) -> String {
    build_with_report_for_input(
        state,
        settings,
        persona_text,
        session_summary,
        Some(user_text),
    )
    .text
}

pub fn build_with_report(
    state: &crate::AppState,
    settings: &Settings,
    persona_text: &str,
    session_summary: Option<&str>,
) -> PromptBuild {
    build_with_report_for_input(state, settings, persona_text, session_summary, None)
}

pub fn build_with_report_for_input(
    state: &crate::AppState,
    settings: &Settings,
    persona_text: &str,
    session_summary: Option<&str>,
    user_text: Option<&str>,
) -> PromptBuild {
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let data_dir = state.data_dir.lock().unwrap().clone();
    let packs_dir = state.packs_dir.lock().unwrap().clone();
    let session_id = state.sessions.lock().unwrap().active.clone();
    let goal_block = super::goal::build_goal_context_block(state);
    let drafts = build_ordered_sections(
        &sandbox,
        &data_dir,
        &packs_dir,
        &session_id,
        settings,
        persona_text,
        session_summary,
        goal_block,
        user_text,
    );
    assemble_drafts(
        super::persona::engine_base(),
        drafts,
        settings.max_context_chars,
    )
}

fn build_ordered_sections(
    root: &Path,
    data_dir: &Path,
    packs_dir: &Path,
    session_id: &str,
    settings: &Settings,
    persona_text: &str,
    session_summary: Option<&str>,
    goal_block: String,
    user_text: Option<&str>,
) -> Vec<SectionDraft> {
    vec![
        section(
            "pack_persona",
            "Pack Persona",
            90,
            persona_section(persona_text),
        ),
        section(
            "skills",
            "Skills",
            60,
            skills_section(root, data_dir, packs_dir, &settings.current_pack, user_text),
        ),
        section(
            "lorebook",
            "Retrieved Lorebook",
            78,
            lorebook_section(data_dir, packs_dir, &settings.current_pack, user_text),
        ),
        section(
            "project_instructions",
            "Project Instructions",
            80,
            project_section(root),
        ),
        section(
            "memories",
            "Memories",
            75,
            memory_section(
                root,
                data_dir,
                packs_dir,
                &settings.current_pack,
                session_id,
            ),
        ),
        section(
            "conversation_summary",
            "Conversation Summary",
            65,
            session_summary_section(session_summary),
        ),
        section("current_goal", "Current Goal", 70, goal_block),
        section(
            "environment",
            "Environment",
            55,
            environment_section(root, settings),
        ),
        section("tools", "Tools", 85, tools_section()),
        section("safety_rules", "Safety Rules", 95, safety_section()),
    ]
}

fn section(id: &'static str, title: &'static str, priority: u8, body: String) -> SectionDraft {
    SectionDraft {
        id,
        title,
        priority,
        body,
    }
}

fn assemble_drafts(base: &str, drafts: Vec<SectionDraft>, max_context_chars: usize) -> PromptBuild {
    let base = base.trim_end();
    let base_chars = char_count(base);
    let mut remaining = max_context_chars.saturating_sub(base_chars);
    let mut decisions: Vec<Option<SectionDecision>> = vec![None; drafts.len()];

    let mut by_priority = (0..drafts.len()).collect::<Vec<_>>();
    by_priority.sort_by(|a, b| {
        drafts[*b]
            .priority
            .cmp(&drafts[*a].priority)
            .then_with(|| a.cmp(b))
    });

    for idx in by_priority {
        let draft = &drafts[idx];
        let body = draft.body.trim();
        if body.is_empty() {
            continue;
        }

        let overhead = section_overhead_chars(draft.title);
        let body_chars = char_count(body);
        let full_cost = overhead.saturating_add(body_chars);
        if full_cost <= remaining {
            decisions[idx] = Some(SectionDecision {
                body: body.to_string(),
                truncated: false,
            });
            remaining = remaining.saturating_sub(full_cost);
            continue;
        }

        if remaining > overhead.saturating_add(MIN_TRUNCATED_SECTION_CHARS) {
            let allowed_body_chars = remaining.saturating_sub(overhead);
            decisions[idx] = Some(SectionDecision {
                body: truncate_with_note(body, allowed_body_chars),
                truncated: true,
            });
            remaining = 0;
        }
    }

    let mut out = String::new();
    out.push_str(base);
    let mut reports = Vec::with_capacity(drafts.len());
    for (idx, draft) in drafts.iter().enumerate() {
        if let Some(decision) = &decisions[idx] {
            let chars = char_count(&decision.body);
            push_section(&mut out, draft.title, &decision.body);
            reports.push(PromptSectionReport {
                id: draft.id.to_string(),
                title: draft.title.to_string(),
                priority: draft.priority,
                chars,
                original_chars: char_count(draft.body.trim()),
                tokens: super::budget::estimate_text_tokens(&decision.body),
                included: true,
                truncated: decision.truncated,
            });
        } else {
            reports.push(PromptSectionReport {
                id: draft.id.to_string(),
                title: draft.title.to_string(),
                priority: draft.priority,
                chars: 0,
                original_chars: char_count(draft.body.trim()),
                tokens: 0,
                included: false,
                truncated: false,
            });
        }
    }

    let prompt_chars = char_count(&out);
    PromptBuild {
        text: out,
        sections: reports,
        prompt_chars,
    }
}

fn push_section(out: &mut String, title: &str, body: &str) {
    let body = body.trim();
    if body.is_empty() {
        return;
    }
    out.push_str("\n\n---\n");
    out.push_str(title);
    out.push_str(":\n");
    out.push_str(body);
    out.push('\n');
}

fn section_overhead_chars(title: &str) -> usize {
    "\n\n---\n:\n\n".chars().count() + char_count(title)
}

fn persona_section(persona_text: &str) -> String {
    persona_text.trim().to_string()
}

fn session_summary_section(summary: Option<&str>) -> String {
    summary.unwrap_or_default().trim().to_string()
}

fn skills_section(
    root: &Path,
    data_dir: &Path,
    packs_dir: &Path,
    pack_id: &str,
    user_text: Option<&str>,
) -> String {
    super::skills::context_for_turn(root, data_dir, packs_dir, pack_id, user_text).text
}

fn lorebook_section(
    data_dir: &Path,
    packs_dir: &Path,
    pack_id: &str,
    user_text: Option<&str>,
) -> String {
    crate::pack::lorebook_context(packs_dir, data_dir, pack_id, user_text)
}

fn project_section(root: &Path) -> String {
    let mut parts = Vec::new();

    let mut instruction_docs = Vec::new();
    // DEMIURGE.md / SYSTEM.md 为本项目自有的中性指令文件约定；AGENTS.md 兼容跨工具的 agents.md 通用约定。
    for name in ["DEMIURGE.md", "SYSTEM.md", "AGENTS.md"] {
        let path = root.join(name);
        if let Some(text) = read_limited_text(&path) {
            instruction_docs.push(format!("## {name}\n{}", text.trim()));
        }
    }
    if !instruction_docs.is_empty() {
        parts.push(format!(
            "# Instruction Files\n{}",
            instruction_docs.join("\n\n")
        ));
    }

    if let Some(readme) = read_limited_text(&root.join("README.md")) {
        parts.push(format!("# README.md\n{}", readme.trim()));
    }

    let detected = package_detection(root);
    if !detected.trim().is_empty() {
        parts.push(format!("# Package / Framework Detection\n{detected}"));
    }

    let tree = directory_snapshot(root);
    if !tree.trim().is_empty() {
        parts.push(format!("# Directory Snapshot\n{tree}"));
    }

    cap_chars(parts.join("\n\n"), MAX_PROJECT_CHARS)
}

fn memory_section(
    root: &Path,
    data_dir: &Path,
    packs_dir: &Path,
    pack_id: &str,
    session_id: &str,
) -> String {
    let mut parts = Vec::new();
    for (scope, label, path) in
        super::memory::scoped_memory_paths(data_dir, root, packs_dir, pack_id, session_id)
    {
        if let Some(text) = read_limited_text(&path) {
            parts.push(format!("# {label} memory ({scope})\n{}", text.trim()));
        }
    }
    if let Some(text) = read_limited_text(&root.join("memory.md")) {
        parts.push(format!("# Project legacy memory\n{}", text.trim()));
    }
    cap_chars(parts.join("\n\n"), MAX_MEMORY_CHARS)
}

fn environment_section(root: &Path, settings: &Settings) -> String {
    let mut lines = Vec::new();
    lines.push(format!("Unix time (ms): {}", now_millis()));
    lines.push(format!(
        "OS/arch: {}/{}",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    lines.push(format!("Workspace sandbox: {}", root.display()));
    lines.push(format!("Current pack: {}", settings.current_pack));
    lines.push(format!(
        "Provider/model: {:?} / {}",
        settings.provider, settings.model
    ));
    lines.push(format!("Git status:\n{}", git_snapshot(root)));
    cap_chars(lines.join("\n"), MAX_PROJECT_CHARS)
}

fn tools_section() -> String {
    [
        "Tool schemas are supplied to the model out-of-band and are the canonical interface.",
        "Prefer typed tools over shell commands when a typed tool exists.",
        "Mutating tools may require user confirmation; respect rejected tool calls and report what was not done.",
        "Use previewable edit tools for file changes so writes remain explainable and undoable.",
    ]
    .join("\n")
}

fn safety_section() -> String {
    [
        "Never reveal, persist, or commit API keys, tokens, credentials, or private user secrets.",
        "Treat the sandbox/workspace boundary as authoritative. Do not claim to have accessed files outside allowed paths.",
        "When tool output conflicts with assumptions, trust the tool output and explain any uncertainty.",
        "For high-risk actions, make the intended effect clear before proceeding.",
    ]
    .join("\n")
}

fn package_detection(root: &Path) -> String {
    let mut lines = Vec::new();

    if let Some(package_text) = read_limited_text(&root.join("package.json")) {
        if let Ok(package) = serde_json::from_str::<Value>(&package_text) {
            if let Some(name) = package.get("name").and_then(Value::as_str) {
                lines.push(format!("npm package: {name}"));
            }
            let deps = package_dependencies(&package);
            let mut frameworks = Vec::new();
            for (dep, label) in [
                ("@tauri-apps/api", "Tauri frontend API"),
                ("@tauri-apps/cli", "Tauri CLI"),
                ("react", "React"),
                ("vite", "Vite"),
                ("typescript", "TypeScript"),
                ("tailwindcss", "Tailwind CSS"),
                ("pdfjs-dist", "PDF.js"),
                ("mermaid", "Mermaid"),
            ] {
                if deps.iter().any(|d| d == dep) {
                    frameworks.push(label);
                }
            }
            if !frameworks.is_empty() {
                lines.push(format!("frontend stack: {}", frameworks.join(", ")));
            }
            if let Some(scripts) = package.get("scripts").and_then(Value::as_object) {
                let mut names = scripts.keys().cloned().collect::<Vec<_>>();
                names.sort();
                if !names.is_empty() {
                    lines.push(format!("npm scripts: {}", names.join(", ")));
                }
            }
        }
    }

    if let Some(cargo_text) = read_limited_text(&root.join("src-tauri").join("Cargo.toml")) {
        if let Some(name) = cargo_package_name(&cargo_text) {
            lines.push(format!("Rust crate: {name}"));
        }
        let mut rust_stack = Vec::new();
        for (needle, label) in [
            ("tauri", "Tauri backend"),
            ("reqwest", "HTTP client"),
            ("tokio", "async runtime"),
            ("keyring", "system credential storage"),
            ("xcap", "screen capture"),
            ("oar-ocr", "OCR"),
        ] {
            if cargo_text.contains(needle) {
                rust_stack.push(label);
            }
        }
        if !rust_stack.is_empty() {
            lines.push(format!("Rust stack: {}", rust_stack.join(", ")));
        }
    }

    for (path, label) in [
        ("vite.config.ts", "Vite config present"),
        ("tsconfig.json", "TypeScript config present"),
        ("src-tauri/tauri.conf.json", "Tauri config present"),
    ] {
        if root.join(path).exists() {
            lines.push(label.to_string());
        }
    }

    lines.join("\n")
}

fn package_dependencies(package: &Value) -> Vec<String> {
    let mut deps = Vec::new();
    for key in ["dependencies", "devDependencies", "peerDependencies"] {
        if let Some(map) = package.get(key).and_then(Value::as_object) {
            deps.extend(map.keys().cloned());
        }
    }
    deps.sort();
    deps.dedup();
    deps
}

fn cargo_package_name(cargo: &str) -> Option<String> {
    let mut in_package = false;
    for line in cargo.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if in_package && trimmed.starts_with("name") {
            let value = trimmed.split_once('=')?.1.trim().trim_matches('"');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn directory_snapshot(root: &Path) -> String {
    let mut lines = Vec::new();
    let mut omitted = 0usize;
    collect_directory(root, 0, &mut lines, &mut omitted);
    if omitted > 0 {
        lines.push(format!("... {omitted} entries omitted"));
    }
    lines.join("\n")
}

fn collect_directory(dir: &Path, depth: usize, lines: &mut Vec<String>, omitted: &mut usize) {
    if depth > MAX_DIRECTORY_DEPTH || lines.len() >= MAX_DIRECTORY_ENTRIES {
        *omitted += 1;
        return;
    }

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries = entries.flatten().collect::<Vec<_>>();
    entries.sort_by(|a, b| {
        let a_path = a.path();
        let b_path = b.path();
        b_path
            .is_file()
            .cmp(&a_path.is_file())
            .then_with(|| a.file_name().cmp(&b.file_name()))
    });

    for entry in entries {
        if lines.len() >= MAX_DIRECTORY_ENTRIES {
            *omitted += 1;
            continue;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if should_skip_entry(&name) {
            continue;
        }
        let indent = "  ".repeat(depth);
        if path.is_dir() {
            lines.push(format!("{indent}- {name}/"));
            collect_directory(&path, depth + 1, lines, omitted);
        } else {
            lines.push(format!("{indent}- {name}"));
        }
    }
}

fn should_skip_entry(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "node_modules"
            | "target"
            | "dist"
            | ".tauri-dev"
            | ".tmp"
            | ".playwright-mcp"
            | "package-lock.json"
            | "Cargo.lock"
    )
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Git 状态快照带短 TTL 进程内缓存：system prompt 在一轮工具循环里会被多次重建
/// （runner 每个 step 调一次 build_for_input），而 `git status` 子进程实测约 130ms，
/// 没必要每步都跑。同一沙盒目录在 GIT_SNAPSHOT_TTL 内复用上次结果，过期再刷新；
/// 这点延迟对 prompt 上下文完全可接受，却能显著降低多步回合的本地开销。
fn git_snapshot(root: &Path) -> String {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, (Instant, String)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = root.to_path_buf();
    if let Ok(map) = cache.lock() {
        if let Some((at, value)) = map.get(&key) {
            if at.elapsed() < GIT_SNAPSHOT_TTL {
                return value.clone();
            }
        }
    }
    let value = git_snapshot_uncached(root);
    if let Ok(mut map) = cache.lock() {
        map.insert(key, (Instant::now(), value.clone()));
    }
    value
}

fn git_snapshot_uncached(root: &Path) -> String {
    let cwd = root.to_path_buf();
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let result = Command::new("git")
            .args(["status", "--short", "--branch"])
            .current_dir(&cwd)
            .output();
        let _ = tx.send(result);
    });

    let output = match rx.recv_timeout(Duration::from_secs(GIT_TIMEOUT_SECS)) {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => return format!("unavailable: git status failed: {e}"),
        Err(_) => return format!("unavailable: git status timed out (>{GIT_TIMEOUT_SECS}s)"),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.contains("not a git repository") {
            return "sandbox is not a Git repository".to_string();
        }
        return if stderr.is_empty() {
            "unavailable: git status returned a non-zero exit code".to_string()
        } else {
            format!("unavailable: {stderr}")
        };
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        "Git working tree is clean".to_string()
    } else {
        stdout
    }
}

fn read_limited_text(path: &Path) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_TEXT_FILE_BYTES {
        return None;
    }
    fs::read_to_string(path).ok()
}

fn cap_chars(s: String, max_chars: usize) -> String {
    if char_count(&s) <= max_chars {
        s
    } else {
        truncate_with_note(&s, max_chars)
    }
}

fn truncate_with_note(s: &str, max_chars: usize) -> String {
    let note = "\n[section truncated by prompt budget]";
    let note_chars = char_count(note);
    let take = max_chars.saturating_sub(note_chars).max(1);
    let head: String = s.chars().take(take).collect();
    format!("{head}{note}")
}

fn char_count(s: &str) -> usize {
    s.chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "demiurge_prompt_builder_test_{}_{}",
            name,
            crate::store::now_millis()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn project_section_discovers_readme_package_and_tree() {
        let root = temp_root("project_section");
        fs::write(root.join("README.md"), "# Demiurge\nDesktop agent").unwrap();
        fs::write(
            root.join("package.json"),
            r#"{"name":"demo","dependencies":{"react":"1"},"devDependencies":{"vite":"1","typescript":"1"},"scripts":{"build":"vite build"}}"#,
        )
        .unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src").join("App.tsx"), "export default null;").unwrap();

        let section = project_section(&root);
        assert!(section.contains("# README.md"));
        assert!(section.contains("npm package: demo"));
        assert!(section.contains("frontend stack: React, Vite, TypeScript"));
        assert!(section.contains("- src/"));
        assert!(section.contains("- App.tsx"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn prompt_budget_keeps_high_priority_sections_first() {
        let low = "low ".repeat(2_000);
        let high = "never leak secrets".to_string();
        let build = assemble_drafts(
            "# Base",
            vec![
                section("low", "Low", 10, low),
                section("safety", "Safety", 95, high),
            ],
            900,
        );

        assert!(build.text.contains("Safety:"));
        assert!(build.text.contains("never leak secrets"));
        let low_report = build.sections.iter().find(|s| s.id == "low").unwrap();
        assert!(!low_report.included || low_report.truncated);
        assert!(build.prompt_chars <= 900);
    }

    #[test]
    fn directory_snapshot_ignores_heavy_dirs() {
        let root = temp_root("directory_snapshot");
        fs::create_dir_all(root.join("node_modules").join("pkg")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src").join("main.rs"), "fn main() {}").unwrap();

        let snapshot = directory_snapshot(&root);
        assert!(snapshot.contains("- src/"));
        assert!(snapshot.contains("- main.rs"));
        assert!(!snapshot.contains("node_modules"));

        let _ = fs::remove_dir_all(&root);
    }
}
