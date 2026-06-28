//! edit_file：在沙盒内对已有 UTF-8 文本文件做精确替换（confirm 类）。
use std::collections::HashMap;

use serde_json::Value;

const MAX_EDIT: u64 = 256 * 1024;
const MAX_PREVIEW_CHARS: usize = 12_000;
const MAX_UNDO_ENTRIES: usize = 20;
const MAX_MULTI_EDITS: usize = 20;
const MAX_PATCH_HUNKS: usize = 20;

#[derive(Clone, Debug)]
pub struct EditUndoEntry {
    pub id: String,
    pub path: String,
    pub before: String,
    pub after: String,
    pub created_at: u64,
    pub replacements: usize,
}

#[derive(Clone, Debug)]
struct EditRequest {
    rel: String,
    old_string: String,
    new_string: String,
    replace_all: bool,
}

#[derive(Clone, Debug)]
struct PlannedEdit {
    rel: String,
    before: String,
    after: String,
    replacements: usize,
}

#[derive(Clone, Debug)]
struct PatchHunk {
    rel: String,
    start_line: usize,
    old_lines: Vec<String>,
    new_lines: Vec<String>,
}

pub fn preview(state: &crate::AppState, args: Value) -> Result<String, String> {
    let req = parse_edit_args(&args)?;
    let planned = plan_edit(state, &req)?;
    Ok(build_preview(
        &planned.rel,
        &planned.before,
        &planned.after,
        planned.replacements,
    ))
}

pub fn run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let req = parse_edit_args(&args)?;
    let planned = plan_edit(state, &req)?;
    let undo_id = write_planned_edit(state, &planned)?;
    Ok(format!(
        "Edit applied: {} ({} replacement(s)). undo_record: {}. Run undo_edit to roll it back.",
        planned.rel, planned.replacements, undo_id
    ))
}

pub fn multi_preview(state: &crate::AppState, args: Value) -> Result<String, String> {
    let edits_count = edits_count(&args)?;
    let planned = plan_multi_edit(state, &args)?;
    Ok(build_multi_preview(&planned, edits_count))
}

pub fn multi_run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let edits_count = edits_count(&args)?;
    let planned = plan_multi_edit(state, &args)?;
    let total_replacements = planned.iter().map(|p| p.replacements).sum::<usize>();

    let mut undo_ids = Vec::new();
    for edit in &planned {
        undo_ids.push(write_planned_edit(state, edit)?);
    }

    Ok(format!(
        "Batch edit applied: {} file(s), {} edit(s), {} replacement(s). undo_records: {}. Run undo_edit to roll back the latest record.",
        planned.len(),
        edits_count,
        total_replacements,
        undo_ids.join(", ")
    ))
}

pub fn patch_preview(state: &crate::AppState, args: Value) -> Result<String, String> {
    let hunk_count = hunks_count(&args)?;
    let planned = plan_patch(state, &args)?;
    Ok(build_patch_preview(&planned, hunk_count))
}

pub fn patch_run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let hunk_count = hunks_count(&args)?;
    let planned = plan_patch(state, &args)?;

    let mut undo_ids = Vec::new();
    for edit in &planned {
        undo_ids.push(write_planned_edit(state, edit)?);
    }

    Ok(format!(
        "Structured patch applied: {} file(s), {} hunk(s). undo_records: {}. Run undo_edit to roll back the latest record.",
        planned.len(),
        hunk_count,
        undo_ids.join(", ")
    ))
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

fn parse_edit_args(args: &Value) -> Result<EditRequest, String> {
    let rel = super::args::required_non_empty_str(args, "path")?.to_string();
    let old_string = super::args::required_str(args, "old_string")?.to_string();
    let new_string = super::args::required_str(args, "new_string")?.to_string();
    let replace_all = super::args::optional_bool(args, "replace_all", false);
    validate_edit_args(rel, old_string, new_string, replace_all)
}

fn validate_edit_args(
    rel: String,
    old_string: String,
    new_string: String,
    replace_all: bool,
) -> Result<EditRequest, String> {
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

fn parse_multi_args(args: &Value) -> Result<Vec<EditRequest>, String> {
    let edits = args
        .get("edits")
        .and_then(Value::as_array)
        .ok_or_else(|| "edits 必须是数组".to_string())?;
    if edits.is_empty() {
        return Err("edits 不能为空".to_string());
    }
    if edits.len() > MAX_MULTI_EDITS {
        return Err(format!(
            "一次最多允许 {MAX_MULTI_EDITS} 个 edit，当前为 {} 个",
            edits.len()
        ));
    }

    edits
        .iter()
        .enumerate()
        .map(|(idx, edit)| {
            let rel = super::args::required_non_empty_str(edit, "path")?.to_string();
            let old_string = super::args::required_str(edit, "old_string")?.to_string();
            let new_string = super::args::required_str(edit, "new_string")?.to_string();
            let replace_all = super::args::optional_bool(edit, "replace_all", false);
            validate_edit_args(rel, old_string, new_string, replace_all)
                .map_err(|e| format!("第 {} 个 edit 无效：{e}", idx + 1))
        })
        .collect()
}

fn edits_count(args: &Value) -> Result<usize, String> {
    Ok(args
        .get("edits")
        .and_then(Value::as_array)
        .ok_or_else(|| "edits 必须是数组".to_string())?
        .len())
}

fn parse_patch_args(args: &Value) -> Result<Vec<PatchHunk>, String> {
    let hunks = args
        .get("hunks")
        .and_then(Value::as_array)
        .ok_or_else(|| "hunks 必须是数组".to_string())?;
    if hunks.is_empty() {
        return Err("hunks 不能为空".to_string());
    }
    if hunks.len() > MAX_PATCH_HUNKS {
        return Err(format!(
            "一次最多允许 {MAX_PATCH_HUNKS} 个 hunk，当前为 {} 个",
            hunks.len()
        ));
    }

    hunks
        .iter()
        .enumerate()
        .map(|(_idx, hunk)| {
            let rel = super::args::required_non_empty_str(hunk, "path")?.to_string();
            let start_line =
                hunk.get("start_line")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| "start_line 必须是正整数".to_string())? as usize;
            if start_line == 0 {
                return Err("start_line 必须从 1 开始".to_string());
            }
            let old_lines = required_string_array(hunk, "old_lines")?;
            let new_lines = required_string_array(hunk, "new_lines")?;
            if old_lines.is_empty() {
                return Err("old_lines 不能为空".to_string());
            }
            if old_lines == new_lines {
                return Err("old_lines 与 new_lines 相同，无需 patch".to_string());
            }
            Ok(PatchHunk {
                rel,
                start_line,
                old_lines,
                new_lines,
            })
        })
        .collect::<Result<Vec<_>, String>>()
        .map_err(|e| format!("patch hunk 无效：{e}"))
}

fn required_string_array(args: &Value, key: &str) -> Result<Vec<String>, String> {
    args.get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{key} 必须是字符串数组"))?
        .iter()
        .map(|line| {
            line.as_str()
                .map(ToString::to_string)
                .ok_or_else(|| format!("{key} 只能包含字符串"))
        })
        .collect()
}

fn hunks_count(args: &Value) -> Result<usize, String> {
    Ok(args
        .get("hunks")
        .and_then(Value::as_array)
        .ok_or_else(|| "hunks 必须是数组".to_string())?
        .len())
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

fn plan_edit(state: &crate::AppState, req: &EditRequest) -> Result<PlannedEdit, String> {
    let before = read_target(state, &req.rel)?;
    let (after, replacements) = apply_edit(&before, req)?;
    Ok(PlannedEdit {
        rel: req.rel.clone(),
        before,
        after,
        replacements,
    })
}

fn plan_multi_edit(state: &crate::AppState, args: &Value) -> Result<Vec<PlannedEdit>, String> {
    let requests = parse_multi_args(args)?;
    let mut planned = Vec::<PlannedEdit>::new();
    let mut by_path = HashMap::<String, usize>::new();

    for req in requests {
        if let Some(idx) = by_path.get(&req.rel).copied() {
            let (after, replacements) = apply_edit(&planned[idx].after, &req)
                .map_err(|e| format!("{} 后续 edit 失败：{e}", req.rel))?;
            planned[idx].after = after;
            planned[idx].replacements += replacements;
        } else {
            let edit = plan_edit(state, &req)?;
            by_path.insert(req.rel.clone(), planned.len());
            planned.push(edit);
        }
    }

    Ok(planned)
}

fn plan_patch(state: &crate::AppState, args: &Value) -> Result<Vec<PlannedEdit>, String> {
    let hunks = parse_patch_args(args)?;
    let mut planned = Vec::<PlannedEdit>::new();
    let mut by_path = HashMap::<String, usize>::new();

    for hunk in hunks {
        if let Some(idx) = by_path.get(&hunk.rel).copied() {
            let after = apply_hunk_to_text(&planned[idx].after, &hunk)?;
            planned[idx].after = after;
            planned[idx].replacements += 1;
        } else {
            let before = read_target(state, &hunk.rel)?;
            let after = apply_hunk_to_text(&before, &hunk)?;
            by_path.insert(hunk.rel.clone(), planned.len());
            planned.push(PlannedEdit {
                rel: hunk.rel.clone(),
                before,
                after,
                replacements: 1,
            });
        }
    }

    Ok(planned)
}

fn apply_hunk_to_text(original: &str, hunk: &PatchHunk) -> Result<String, String> {
    let had_trailing_newline = original.ends_with('\n');
    let mut lines = original
        .lines()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let start = hunk.start_line - 1;
    let end = start + hunk.old_lines.len();

    if start > lines.len() || end > lines.len() {
        return Err(format!(
            "{}:{} hunk 越界，文件只有 {} 行",
            hunk.rel,
            hunk.start_line,
            lines.len()
        ));
    }

    if lines[start..end] != hunk.old_lines[..] {
        return Err(format!(
            "{}:{} hunk 不匹配，当前内容与 old_lines 不一致",
            hunk.rel, hunk.start_line
        ));
    }

    lines.splice(start..end, hunk.new_lines.clone());
    let mut next = lines.join("\n");
    if had_trailing_newline {
        next.push('\n');
    }
    Ok(next)
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

fn write_planned_edit(state: &crate::AppState, edit: &PlannedEdit) -> Result<String, String> {
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let path = super::resolve_in_sandbox(&sandbox, &edit.rel)?;
    std::fs::write(&path, &edit.after).map_err(|e| format!("写入失败：{e}"))?;
    let undo_id = push_undo_entry(
        state,
        edit.rel.clone(),
        edit.before.clone(),
        edit.after.clone(),
        edit.replacements,
    );
    Ok(undo_id)
}

fn push_undo_entry(
    state: &crate::AppState,
    path: String,
    before: String,
    after: String,
    replacements: usize,
) -> String {
    let created_at = crate::store::now_millis();
    let mut stack = state.edit_undo_stack.lock().unwrap();
    let entry_id = format!("edit_{created_at}_{}", stack.len() + 1);
    let entry = EditUndoEntry {
        id: entry_id.clone(),
        path,
        before,
        after,
        created_at,
        replacements,
    };

    stack.push(entry);
    if stack.len() > MAX_UNDO_ENTRIES {
        let overflow = stack.len() - MAX_UNDO_ENTRIES;
        stack.drain(0..overflow);
    }
    entry_id
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

fn build_multi_preview(planned: &[PlannedEdit], edits_count: usize) -> String {
    let total_replacements = planned.iter().map(|p| p.replacements).sum::<usize>();
    let mut out = format!(
        "批量编辑预览：{} 个文件，{} 个 edit，替换 {} 处\n\n",
        planned.len(),
        edits_count,
        total_replacements
    );

    for edit in planned {
        out.push_str(&build_preview(
            &edit.rel,
            &edit.before,
            &edit.after,
            edit.replacements,
        ));
        out.push('\n');
        if out.chars().count() > MAX_PREVIEW_CHARS {
            out.push_str("…multi_edit preview 已截断\n");
            break;
        }
    }

    out
}

fn build_patch_preview(planned: &[PlannedEdit], hunk_count: usize) -> String {
    let mut out = format!(
        "结构化 patch 预览：{} 个文件，{} 个 hunk\n\n",
        planned.len(),
        hunk_count
    );

    for edit in planned {
        out.push_str(&build_preview(
            &edit.rel,
            &edit.before,
            &edit.after,
            edit.replacements,
        ));
        out.push('\n');
        if out.chars().count() > MAX_PREVIEW_CHARS {
            out.push_str("…apply_patch preview 已截断\n");
            break;
        }
    }

    out
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

    use super::{multi_preview, multi_run, patch_preview, patch_run, run, undo, undo_preview};
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
            workflow_runs: Mutex::new(Vec::new()),
            workflow_cancels: Mutex::new(HashMap::new()),
            cancel: AtomicBool::new(false),
            busy: AtomicBool::new(false),
            data_dir: Mutex::new(sandbox.clone()),
            sandbox_dir: Mutex::new(sandbox),
            packs_dir: Mutex::new(PathBuf::new()),
            ocr: crate::ocr::OcrState::default(),
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
        assert!(result.contains("Edit applied: note.txt (1 replacement(s))."));
        assert!(result.contains("undo_record: edit_"));
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

    #[test]
    fn multi_edit_updates_two_files_and_records_undo_entries() {
        let sandbox = temp_sandbox("multi_two_files");
        let a = sandbox.join("a.txt");
        let b = sandbox.join("b.txt");
        std::fs::write(&a, "alpha\n").unwrap();
        std::fs::write(&b, "beta\n").unwrap();
        let state = test_state(sandbox.clone());

        let result = multi_run(
            &state,
            json!({
                "edits": [
                    { "path": "a.txt", "old_string": "alpha", "new_string": "ALPHA" },
                    { "path": "b.txt", "old_string": "beta", "new_string": "BETA" }
                ]
            }),
        )
        .unwrap();

        assert!(result.contains("Batch edit applied: 2 file(s), 2 edit(s), 2 replacement(s)."));
        assert!(result.contains("undo_records: edit_"));
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "ALPHA\n");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "BETA\n");
        assert_eq!(state.edit_undo_stack.lock().unwrap().len(), 2);

        let _ = std::fs::remove_dir_all(&sandbox);
    }

    #[test]
    fn multi_edit_applies_multiple_edits_to_same_file_as_one_plan() {
        let sandbox = temp_sandbox("multi_same_file");
        let file = sandbox.join("note.txt");
        std::fs::write(&file, "one two three\n").unwrap();
        let state = test_state(sandbox.clone());

        multi_run(
            &state,
            json!({
                "edits": [
                    { "path": "note.txt", "old_string": "one", "new_string": "1" },
                    { "path": "note.txt", "old_string": "two", "new_string": "2" }
                ]
            }),
        )
        .unwrap();

        assert_eq!(std::fs::read_to_string(&file).unwrap(), "1 2 three\n");
        let stack = state.edit_undo_stack.lock().unwrap();
        assert_eq!(stack.len(), 1);
        assert_eq!(stack[0].replacements, 2);

        let _ = std::fs::remove_dir_all(&sandbox);
    }

    #[test]
    fn multi_edit_failure_writes_nothing() {
        let sandbox = temp_sandbox("multi_failure");
        let a = sandbox.join("a.txt");
        let b = sandbox.join("b.txt");
        std::fs::write(&a, "alpha\n").unwrap();
        std::fs::write(&b, "beta\n").unwrap();
        let state = test_state(sandbox.clone());

        let err = multi_run(
            &state,
            json!({
                "edits": [
                    { "path": "a.txt", "old_string": "alpha", "new_string": "ALPHA" },
                    { "path": "b.txt", "old_string": "missing", "new_string": "BETA" }
                ]
            }),
        )
        .unwrap_err();

        assert!(err.contains("未找到 old_string"));
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "alpha\n");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "beta\n");
        assert!(state.edit_undo_stack.lock().unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&sandbox);
    }

    #[test]
    fn multi_preview_contains_each_file_diff() {
        let sandbox = temp_sandbox("multi_preview");
        std::fs::write(sandbox.join("a.txt"), "alpha\n").unwrap();
        std::fs::write(sandbox.join("b.txt"), "beta\n").unwrap();
        let state = test_state(sandbox.clone());

        let preview = multi_preview(
            &state,
            json!({
                "edits": [
                    { "path": "a.txt", "old_string": "alpha", "new_string": "ALPHA" },
                    { "path": "b.txt", "old_string": "beta", "new_string": "BETA" }
                ]
            }),
        )
        .unwrap();

        assert!(preview.contains("批量编辑预览"));
        assert!(preview.contains("--- a.txt"));
        assert!(preview.contains("--- b.txt"));
        assert!(preview.contains("+ ALPHA"));
        assert!(preview.contains("+ BETA"));

        let _ = std::fs::remove_dir_all(&sandbox);
    }

    #[test]
    fn multi_edit_rejects_empty_edits() {
        let sandbox = temp_sandbox("multi_empty");
        let state = test_state(sandbox.clone());

        let err = multi_run(&state, json!({ "edits": [] })).unwrap_err();
        assert!(err.contains("edits 不能为空"));

        let _ = std::fs::remove_dir_all(&sandbox);
    }

    #[test]
    fn patch_applies_single_hunk() {
        let sandbox = temp_sandbox("patch_single");
        let file = sandbox.join("note.txt");
        std::fs::write(&file, "one\ntwo\nthree\n").unwrap();
        let state = test_state(sandbox.clone());

        let result = patch_run(
            &state,
            json!({
                "hunks": [{
                    "path": "note.txt",
                    "start_line": 2,
                    "old_lines": ["two"],
                    "new_lines": ["TWO", "two-point-five"]
                }]
            }),
        )
        .unwrap();

        assert!(result.contains("Structured patch applied: 1 file(s), 1 hunk(s)."));
        assert!(result.contains("undo_records: edit_"));
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "one\nTWO\ntwo-point-five\nthree\n"
        );
        assert_eq!(state.edit_undo_stack.lock().unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(&sandbox);
    }

    #[test]
    fn patch_updates_multiple_files_and_records_undo() {
        let sandbox = temp_sandbox("patch_multi_files");
        let a = sandbox.join("a.txt");
        let b = sandbox.join("b.txt");
        std::fs::write(&a, "alpha\nold\n").unwrap();
        std::fs::write(&b, "beta\nold\n").unwrap();
        let state = test_state(sandbox.clone());

        patch_run(
            &state,
            json!({
                "hunks": [
                    { "path": "a.txt", "start_line": 2, "old_lines": ["old"], "new_lines": ["new-a"] },
                    { "path": "b.txt", "start_line": 2, "old_lines": ["old"], "new_lines": ["new-b"] }
                ]
            }),
        )
        .unwrap();

        assert_eq!(std::fs::read_to_string(&a).unwrap(), "alpha\nnew-a\n");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "beta\nnew-b\n");
        assert_eq!(state.edit_undo_stack.lock().unwrap().len(), 2);

        let _ = std::fs::remove_dir_all(&sandbox);
    }

    #[test]
    fn patch_applies_multiple_hunks_to_same_file_in_order() {
        let sandbox = temp_sandbox("patch_same_file");
        let file = sandbox.join("note.txt");
        std::fs::write(&file, "a\nb\nc\n").unwrap();
        let state = test_state(sandbox.clone());

        patch_run(
            &state,
            json!({
                "hunks": [
                    { "path": "note.txt", "start_line": 2, "old_lines": ["b"], "new_lines": ["B", "inserted"] },
                    { "path": "note.txt", "start_line": 4, "old_lines": ["c"], "new_lines": ["C"] }
                ]
            }),
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "a\nB\ninserted\nC\n"
        );
        let stack = state.edit_undo_stack.lock().unwrap();
        assert_eq!(stack.len(), 1);
        assert_eq!(stack[0].replacements, 2);

        let _ = std::fs::remove_dir_all(&sandbox);
    }

    #[test]
    fn patch_mismatch_writes_nothing() {
        let sandbox = temp_sandbox("patch_mismatch");
        let a = sandbox.join("a.txt");
        let b = sandbox.join("b.txt");
        std::fs::write(&a, "alpha\n").unwrap();
        std::fs::write(&b, "beta\n").unwrap();
        let state = test_state(sandbox.clone());

        let err = patch_run(
            &state,
            json!({
                "hunks": [
                    { "path": "a.txt", "start_line": 1, "old_lines": ["alpha"], "new_lines": ["ALPHA"] },
                    { "path": "b.txt", "start_line": 1, "old_lines": ["missing"], "new_lines": ["BETA"] }
                ]
            }),
        )
        .unwrap_err();

        assert!(err.contains("hunk 不匹配"));
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "alpha\n");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "beta\n");
        assert!(state.edit_undo_stack.lock().unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&sandbox);
    }

    #[test]
    fn patch_rejects_out_of_range_start_line() {
        let sandbox = temp_sandbox("patch_range");
        std::fs::write(sandbox.join("note.txt"), "one\n").unwrap();
        let state = test_state(sandbox.clone());

        let err = patch_run(
            &state,
            json!({
                "hunks": [{
                    "path": "note.txt",
                    "start_line": 3,
                    "old_lines": ["missing"],
                    "new_lines": ["new"]
                }]
            }),
        )
        .unwrap_err();

        assert!(err.contains("hunk 越界"));

        let _ = std::fs::remove_dir_all(&sandbox);
    }

    #[test]
    fn patch_preview_contains_diff() {
        let sandbox = temp_sandbox("patch_preview");
        std::fs::write(sandbox.join("note.txt"), "one\ntwo\n").unwrap();
        let state = test_state(sandbox.clone());

        let preview = patch_preview(
            &state,
            json!({
                "hunks": [{
                    "path": "note.txt",
                    "start_line": 2,
                    "old_lines": ["two"],
                    "new_lines": ["TWO"]
                }]
            }),
        )
        .unwrap();

        assert!(preview.contains("结构化 patch 预览"));
        assert!(preview.contains("--- note.txt"));
        assert!(preview.contains("- two"));
        assert!(preview.contains("+ TWO"));

        let _ = std::fs::remove_dir_all(&sandbox);
    }
}
