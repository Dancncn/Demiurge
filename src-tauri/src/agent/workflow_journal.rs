use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};

use crate::store;

const JOURNAL_DIR: &str = ".demiurge/workflow-runs";

#[derive(Clone, Debug, Serialize)]
pub struct WorkflowRunInfo {
    pub run_id: String,
    pub updated_at: u64,
    pub journal_path: String,
}

pub fn new_run_id() -> String {
    format!("wf_{}", store::new_session_id().trim_start_matches("s_"))
}

pub fn append(
    state: &crate::AppState,
    run_id: &str,
    event: &str,
    payload: Value,
) -> Result<(), String> {
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    append_in_root(&sandbox, run_id, event, payload)
}

fn append_in_root(root: &Path, run_id: &str, event: &str, payload: Value) -> Result<(), String> {
    let dir = journal_dir(root, run_id);
    fs::create_dir_all(&dir).map_err(|e| format!("创建 workflow journal 目录失败：{e}"))?;
    let path = dir.join("journal.jsonl");
    let line = json!({
        "ts": store::now_millis(),
        "run_id": run_id,
        "event": event,
        "payload": payload,
    });
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("打开 workflow journal 失败：{e}"))?;
    writeln!(file, "{line}").map_err(|e| format!("写入 workflow journal 失败：{e}"))
}

pub fn list(state: &crate::AppState) -> Vec<WorkflowRunInfo> {
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let root = sandbox.join(JOURNAL_DIR);
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut runs = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path().join("journal.jsonl");
            let meta = fs::metadata(&path).ok()?;
            Some(WorkflowRunInfo {
                run_id: entry.file_name().to_string_lossy().to_string(),
                updated_at: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0),
                journal_path: path.to_string_lossy().to_string(),
            })
        })
        .collect::<Vec<_>>();
    runs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    runs
}

pub fn resume_overlay(state: &crate::AppState, run_id: &str) -> Result<String, String> {
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let path = journal_dir(&sandbox, run_id).join("journal.jsonl");
    let raw = fs::read_to_string(&path).map_err(|e| format!("读取 workflow journal 失败：{e}"))?;
    let tail = raw
        .lines()
        .rev()
        .take(40)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!(
        "你正在恢复 Ultracode workflow run `{run_id}`。\n\
         下面是该 run journal 的最近事件。请先根据 journal 复盘已完成事项、未完成事项和下一步，然后继续执行；不要重复已经完成的安全操作。\n\n\
         ```jsonl\n{tail}\n```"
    ))
}

fn journal_dir(root: &Path, run_id: &str) -> PathBuf {
    root.join(JOURNAL_DIR).join(sanitize_run_id(run_id))
}

fn sanitize_run_id(run_id: &str) -> String {
    run_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_run_ids_for_paths() {
        assert_eq!(sanitize_run_id("wf_1/../x"), "wf_1____x");
    }

    #[test]
    fn appends_jsonl() {
        let root =
            std::env::temp_dir().join(format!("demiurge_journal_{}", store::new_session_id()));
        append_in_root(&root, "wf_test", "run_started", json!({"ok": true})).unwrap();
        let raw =
            std::fs::read_to_string(root.join(JOURNAL_DIR).join("wf_test").join("journal.jsonl"))
                .unwrap();
        assert!(raw.contains("run_started"));
        let _ = std::fs::remove_dir_all(root);
    }
}
