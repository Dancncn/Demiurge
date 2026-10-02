use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};

use crate::store;

pub const JOURNAL_DIR: &str = ".demiurge/workflow-runs";

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
    static APPEND_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _append = APPEND_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let sandbox = match super::workflow_runtime::registered_storage_root(state, run_id)? {
        Some(root) => root,
        None => {
            // Slash/Ultracode journals belong to the active main turn, never the sidebar selection.
            let session_id = state
                .session_engine
                .lock()
                .unwrap()
                .active_turn
                .as_ref()
                .map(|turn| turn.session_id.clone())
                .ok_or_else(|| "Cannot write an unowned workflow journal.".to_string())?;
            let execution =
                super::execution_context::ExecutionContext::for_session(state, &session_id)?;
            let owner_path = run_dir(&execution.workspace_root, run_id).join("execution.json");
            if owner_path.exists() {
                let owner: super::execution_context::ExecutionContext = serde_json::from_str(
                    &fs::read_to_string(&owner_path).map_err(|e| e.to_string())?,
                )
                .map_err(|e| format!("Cannot verify journal execution identity: {e}"))?;
                if owner != execution {
                    return Err("Journal execution identity does not match this turn.".to_string());
                }
            } else {
                // Never attach a new identity to a legacy journal whose owner is unknown.
                if run_dir(&execution.workspace_root, run_id)
                    .join("journal.jsonl")
                    .exists()
                {
                    return Err(
                        "Legacy journal has no verified owner; start a new run.".to_string()
                    );
                }
                let body = serde_json::to_vec(&execution).map_err(|e| e.to_string())?;
                demiurge_framework::persistence::atomic_write(&owner_path, &body, false)
                    .map_err(|e| e.to_string())?;
            }
            execution.workspace_root
        }
    };
    append_in_root(&sandbox, run_id, event, payload)
}

fn append_in_root(root: &Path, run_id: &str, event: &str, payload: Value) -> Result<(), String> {
    let dir = run_dir(root, run_id);
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
    let sandbox = super::workflow_runtime::registered_storage_root(state, run_id)?
        .unwrap_or_else(|| state.sandbox_dir.lock().unwrap().clone());
    let owner_path = run_dir(&sandbox, run_id).join("execution.json");
    let execution: super::execution_context::ExecutionContext =
        serde_json::from_str(&fs::read_to_string(&owner_path).map_err(|_| {
            "Journal has no verified execution identity; start a new run.".to_string()
        })?)
        .map_err(|e| format!("Cannot read journal execution identity: {e}"))?;
    let caller = super::execution_context::ExecutionContext::for_session(
        state,
        &super::session_engine::execution_session_id(state),
    )?;
    if caller != execution
        || fs::canonicalize(&sandbox).ok().as_ref() != Some(&execution.workspace_root)
    {
        return Err("Resume this journal from its original session and workspace.".to_string());
    }
    resume_overlay_in_root(&sandbox, run_id)
}

pub(super) fn resume_overlay_in_root(root: &Path, run_id: &str) -> Result<String, String> {
    let path = run_dir(root, run_id).join("journal.jsonl");
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

pub fn run_dir(root: &Path, run_id: &str) -> PathBuf {
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
    fn workflow_ownership_slash_journal_remains_bound_to_its_turn_and_rejects_rebinding() {
        use super::super::session_engine::{TurnEntrypoint, TurnRunState, TurnStatus};
        let root = std::env::temp_dir().join(format!(
            "demiurge_journal_owner_{}",
            store::new_session_id()
        ));
        fs::create_dir_all(root.join("other")).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let state = crate::AppState::new(reqwest::Client::new());
        *state.sandbox_dir.lock().unwrap() = root.join("other");
        let mut a = store::Session::new();
        a.id = "a".to_string();
        a.workspace_path = root.to_string_lossy().to_string();
        let mut b = a.clone();
        b.id = "b".to_string();
        *state.sessions.lock().unwrap() = store::SessionStore {
            active: "b".to_string(),
            sessions: vec![a, b],
        };
        state.session_engine.lock().unwrap().active_turn = Some(TurnRunState {
            id: "turn-a".to_string(),
            session_id: "a".to_string(),
            entrypoint: TurnEntrypoint::Send,
            status: TurnStatus::Running,
            input_preview: String::new(),
            workflow_run_id: None,
            agent_names: Vec::new(),
            started_at: 1,
            updated_at: 1,
            completed_at: None,
            error: None,
        });
        append(&state, "wf_slash", "original", json!({})).unwrap();
        let journal = run_dir(&root, "wf_slash").join("journal.jsonl");
        assert!(journal.exists());
        assert!(!root.join("other").join(JOURNAL_DIR).exists());
        *state.sandbox_dir.lock().unwrap() = root.clone();
        assert!(resume_overlay(&state, "wf_slash")
            .unwrap()
            .contains("original"));
        state
            .session_engine
            .lock()
            .unwrap()
            .active_turn
            .as_mut()
            .unwrap()
            .session_id = "b".to_string();
        assert!(append(&state, "wf_slash", "rebound", json!({})).is_err());
        assert!(resume_overlay(&state, "wf_slash").is_err());
        fs::remove_file(run_dir(&root, "wf_slash").join("execution.json")).unwrap();
        assert!(append(&state, "wf_slash", "legacy_rebound", json!({})).is_err());
        assert!(!fs::read_to_string(journal).unwrap().contains("rebound"));
        fs::remove_dir_all(root).unwrap();
    }

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
