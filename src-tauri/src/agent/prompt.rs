//! Phase 2：system prompt 分区构建。把角色人格、项目指令、环境与轻量记忆组合成稳定上下文。
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::store::Settings;

const MAX_TEXT_FILE_BYTES: u64 = 32 * 1024;
const MAX_SECTION_CHARS: usize = 12_000;
const GIT_TIMEOUT_SECS: u64 = 5;

pub struct PromptSections {
    pub persona: String,
    pub project: String,
    pub environment: String,
    pub memory: String,
}

pub fn build(state: &crate::AppState, settings: &Settings, persona_text: &str) -> String {
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let packs_dir = state.packs_dir.lock().unwrap().clone();

    let sections = PromptSections {
        persona: persona_section(persona_text),
        project: project_section(&sandbox),
        environment: environment_section(&sandbox, settings),
        memory: memory_section(&sandbox, &packs_dir, &settings.current_pack),
    };

    assemble(sections)
}

pub fn assemble(sections: PromptSections) -> String {
    let mut out = String::new();
    out.push_str(super::persona::engine_base());

    push_section(&mut out, "角色设定", &sections.persona);
    push_section(&mut out, "项目指令", &sections.project);
    push_section(&mut out, "运行环境", &sections.environment);
    push_section(&mut out, "记忆", &sections.memory);

    out
}

fn push_section(out: &mut String, title: &str, body: &str) {
    let body = body.trim();
    if body.is_empty() {
        return;
    }
    out.push_str("\n\n---\n");
    out.push_str(title);
    out.push_str("：\n");
    out.push_str(body);
    out.push('\n');
}

fn persona_section(persona_text: &str) -> String {
    persona_text.trim().to_string()
}

fn project_section(root: &Path) -> String {
    let mut parts = Vec::new();
    for name in ["DEMIURGE.md", "CLAUDE.md"] {
        let path = root.join(name);
        if let Some(text) = read_limited_text(&path) {
            parts.push(format!("# {name}\n{text}"));
        }
    }
    cap_section(parts.join("\n\n"))
}

fn memory_section(root: &Path, packs_dir: &Path, pack_id: &str) -> String {
    let mut parts = Vec::new();
    for (label, path) in [
        ("项目记忆 memory.md", root.join("memory.md")),
        (
            "本地记忆 .demiurge/memory.md",
            root.join(".demiurge").join("memory.md"),
        ),
        (
            "角色包记忆 memory.md",
            packs_dir.join(pack_id).join("memory.md"),
        ),
    ] {
        if let Some(text) = read_limited_text(&path) {
            parts.push(format!("# {label}\n{text}"));
        }
    }
    cap_section(parts.join("\n\n"))
}

fn environment_section(root: &Path, settings: &Settings) -> String {
    let mut lines = Vec::new();
    lines.push(format!("当前 Unix 时间戳（毫秒）：{}", now_millis()));
    lines.push(format!("工作区根目录（沙盒）：{}", root.display()));
    lines.push(format!("当前角色包：{}", settings.current_pack));
    lines.push(format!("Git 快照：\n{}", git_snapshot(root)));
    cap_section(lines.join("\n"))
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn git_snapshot(root: &Path) -> String {
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
        Ok(Err(e)) => return format!("不可用：执行 git status 失败：{e}"),
        Err(_) => return format!("不可用：git status 超时（>{GIT_TIMEOUT_SECS}s）"),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.contains("not a git repository") || stderr.contains("不是 git 仓库") {
            return "沙盒目录当前不是 Git 仓库".to_string();
        }
        return if stderr.is_empty() {
            "不可用：git status 返回非零状态".to_string()
        } else {
            format!("不可用：{stderr}")
        };
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        "Git 工作区干净".to_string()
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

fn cap_section(s: String) -> String {
    if s.chars().count() <= MAX_SECTION_CHARS {
        s
    } else {
        let head: String = s.chars().take(MAX_SECTION_CHARS).collect();
        format!("{head}\n…[该上下文分区已截断]")
    }
}
