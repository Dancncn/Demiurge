//! edit_file：在沙盒内对已有 UTF-8 文本文件做精确替换（confirm 类）。
use serde_json::Value;

const MAX_EDIT: u64 = 256 * 1024;
const MAX_PREVIEW_CHARS: usize = 12_000;
const MAX_UNDO_ENTRIES: usize = 20;

#[derive(Clone, Debug)]
pub struct EditUndoEntry {
    pub id: String,
    pub path: String,
    pub before: String,
    pub after: String,
    pub created_at: u64,
    pub replacements: usize,
}

struct EditRequest {
    rel: String,
    old_string: String,
    new_string: String,
    replace_all: bool,
}

pub fn preview(state: &crate::AppState, args: Value) -> Result<String, String> {
    let req = parse_args(&args)?;
    let original = read_target(state, &req.rel)?;
    let (updated, count) = apply_edit(&original, &req)?;
    Ok(build_preview(&req.rel, &original, &updated, count))
}

pub fn run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let req = parse_args(&args)?;
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let path = super::resolve_in_sandbox(&sandbox, &req.rel)?;
    let original = read_target(state, &req.rel)?;
    let (updated, count) = apply_edit(&original, &req)?;

    std::fs::write(&path, &updated).map_err(|e| format!("写入失败：{e}"))?;
    push_undo_entry(state, req.rel.clone(), original, updated, count);
    Ok(format!("已编辑沙盒文件：{}（替换 {} 处）", req.rel, count))
}

pub fn undo_preview(state: &crate::AppState, _args: Value) -> Result<String, String> {
    let entry = latest_undo_entry(state)?;
    let current = read_target(state, &entry.path)?;
    ensure_undo_safe(&current, &entry)?;

    let mut preview = format!(
        "撤销最近编辑：{}（原替换 {} 处，记录 {}）\n",
        entry.path, entry.replacements, entry.id
    );
    preview.push_str(&build_preview(&entry.path, &entry.after, &entry.before, 1));
    Ok(preview)
}

pub fn undo(state: &crate::AppState, _args: Value) -> Result<String, String> {
    let entry = latest_undo_entry(state)?;
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let path = super::resolve_in_sandbox(&sandbox, &entry.path)?;
    let current = read_target(state, &entry.path)?;
    ensure_undo_safe(&current, &entry)?;

    std::fs::write(&path, &entry.before).map_err(|e| format!("撤销写入失败：{e}"))?;
    let mut stack = state.edit_undo_stack.lock().unwrap();
    let popped = stack.pop();
    match popped {
        Some(last) if last.id == entry.id => Ok(format!(
            "已撤销最近编辑：{}（记录 {}）",
            entry.path, entry.id
        )),
        Some(last) => {
            stack.push(last);
            Err("undo 栈在撤销过程中发生变化，未移除记录".to_string())
        }
        None => Err("undo 栈为空，无法撤销".to_string()),
    }
}

fn parse_args(args: &Value) -> Result<EditRequest, String> {
    let rel = super::args::required_non_empty_str(args, "path")?.to_string();
    let old_string = super::args::required_str(args, "old_string")?.to_string();
    let new_string = super::args::required_str(args, "new_string")?.to_string();
    let replace_all = super::args::optional_bool(args, "replace_all", false);

    if old_string.is_empty() {
        return Err("old_string 不能为空".to_string());
    }
    if old_string == new_string {
        return Err("old_string 与 new_string 相同，无需编辑".to_string());
    }

    Ok(EditRequest {
        rel,
        old_string,
        new_string,
        replace_all,
    })
}

fn read_target(state: &crate::AppState, rel: &str) -> Result<String, String> {
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let path = super::resolve_in_sandbox(&sandbox, rel)?;
    let meta = std::fs::metadata(&path).map_err(|e| format!("无法访问文件：{e}"))?;
    if !meta.is_file() {
        return Err("目标不是文件".to_string());
    }
    if meta.len() > MAX_EDIT {
        return Err(format!(
            "文件过大（{} 字节），超过 {} 字节上限",
            meta.len(),
            MAX_EDIT
        ));
    }
    std::fs::read_to_string(&path).map_err(|e| format!("读取失败（可能不是 UTF-8 文本）：{e}"))
}

fn apply_edit(original: &str, req: &EditRequest) -> Result<(String, usize), String> {
    let count = original.matches(&req.old_string).count();
    if count == 0 {
        return Err("未找到 old_string，未做任何修改".to_string());
    }
    if !req.replace_all && count > 1 {
        return Err(format!(
            "old_string 出现 {count} 次，不唯一；请提供更具体上下文或设置 replace_all=true"
        ));
    }

    let updated = if req.replace_all {
        original.replace(&req.old_string, &req.new_string)
    } else {
        original.replacen(&req.old_string, &req.new_string, 1)
    };
    Ok((updated, if req.replace_all { count } else { 1 }))
}

fn push_undo_entry(
    state: &crate::AppState,
    path: String,
    before: String,
    after: String,
    replacements: usize,
) {
    let created_at = crate::store::now_millis();
    let entry = EditUndoEntry {
        id: format!("edit_{created_at}"),
        path,
        before,
        after,
        created_at,
        replacements,
    };

    let mut stack = state.edit_undo_stack.lock().unwrap();
    stack.push(entry);
    if stack.len() > MAX_UNDO_ENTRIES {
        let overflow = stack.len() - MAX_UNDO_ENTRIES;
        stack.drain(0..overflow);
    }
}

fn latest_undo_entry(state: &crate::AppState) -> Result<EditUndoEntry, String> {
    state
        .edit_undo_stack
        .lock()
        .unwrap()
        .last()
        .cloned()
        .ok_or_else(|| "undo 栈为空，无法撤销最近编辑".to_string())
}

fn ensure_undo_safe(current: &str, entry: &EditUndoEntry) -> Result<(), String> {
    if current != entry.after {
        return Err(format!(
            "无法安全撤销：{} 已在编辑后发生变化，当前内容与 undo 记录不匹配",
            entry.path
        ));
    }
    Ok(())
}

fn build_preview(path: &str, original: &str, updated: &str, count: usize) -> String {
    let mut out = String::new();
    out.push_str(&format!("--- {path}\n+++ {path}\n@@ 替换 {count} 处 @@\n"));

    let old_lines: Vec<&str> = original.lines().collect();
    let new_lines: Vec<&str> = updated.lines().collect();
    let max = old_lines.len().max(new_lines.len());
    let mut changed = 0usize;

    for idx in 0..max {
        let old = old_lines.get(idx).copied();
        let new = new_lines.get(idx).copied();
        if old == new {
            continue;
        }
        changed += 1;
        if let Some(line) = old {
            out.push_str("- ");
            out.push_str(line);
            out.push('\n');
        }
        if let Some(line) = new {
            out.push_str("+ ");
            out.push_str(line);
            out.push('\n');
        }
        if out.chars().count() > MAX_PREVIEW_CHARS {
            out.push_str("…diff preview 已截断\n");
            break;
        }
    }

    if changed == 0 {
        out.push_str("（内容会变化，但按行 diff 未发现整行差异；可能是行尾或空白字符变化）\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::Mutex;

    use serde_json::json;
    use tokio::sync::oneshot;

    use super::{run, undo, undo_preview};
    use crate::permission::{PermissionResponse, PermissionRule};
    use crate::store::{SessionStore, Settings};

    fn temp_sandbox(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "demiurge_edit_undo_test_{}_{}",
            name,
            crate::store::now_millis()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn test_state(sandbox: PathBuf) -> crate::AppState {
        crate::AppState {
            http: reqwest::Client::new(),
            settings: Mutex::new(Settings::default()),
            sessions: Mutex::new(SessionStore::default()),
            pending_confirms: Mutex::new(
                HashMap::<String, oneshot::Sender<PermissionResponse>>::new(),
            ),
            session_permission_rules: Mutex::new(HashMap::<String, PermissionRule>::new()),
            edit_undo_stack: Mutex::new(Vec::new()),
            cancel: AtomicBool::new(false),
            busy: AtomicBool::new(false),
            data_dir: Mutex::new(sandbox.clone()),
            sandbox_dir: Mutex::new(sandbox),
            packs_dir: Mutex::new(PathBuf::new()),
        }
    }

    #[test]
    fn edit_records_undo_and_undo_restores_file() {
        let sandbox = temp_sandbox("restore");
        let file = sandbox.join("note.txt");
        std::fs::write(&file, "hello\nworld\n").unwrap();
        let state = test_state(sandbox.clone());

        let result = run(
            &state,
            json!({ "path": "note.txt", "old_string": "world", "new_string": "Demiurge" }),
        )
        .unwrap();
        assert!(result.contains("替换 1 处"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello\nDemiurge\n");
        assert_eq!(state.edit_undo_stack.lock().unwrap().len(), 1);

        let preview = undo_preview(&state, json!({})).unwrap();
        assert!(preview.contains("撤销最近编辑"));
        assert!(preview.contains("- Demiurge"));
        assert!(preview.contains("+ world"));

        undo(&state, json!({})).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello\nworld\n");
        assert!(state.edit_undo_stack.lock().unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&sandbox);
    }

    #[test]
    fn undo_refuses_when_file_drifted() {
        let sandbox = temp_sandbox("drift");
        let file = sandbox.join("note.txt");
        std::fs::write(&file, "before\n").unwrap();
        let state = test_state(sandbox.clone());

        run(
            &state,
            json!({ "path": "note.txt", "old_string": "before", "new_string": "after" }),
        )
        .unwrap();
        std::fs::write(&file, "external change\n").unwrap();

        let err = undo(&state, json!({})).unwrap_err();
        assert!(err.contains("无法安全撤销"));
        assert_eq!(state.edit_undo_stack.lock().unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(&sandbox);
    }

    #[test]
    fn undo_empty_stack_has_clear_error() {
        let sandbox = temp_sandbox("empty");
        let state = test_state(sandbox.clone());

        let err = undo(&state, json!({})).unwrap_err();
        assert!(err.contains("undo 栈为空"));

        let _ = std::fs::remove_dir_all(&sandbox);
    }
}
