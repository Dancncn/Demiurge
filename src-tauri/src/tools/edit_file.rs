//! edit_file：在沙盒内对已有 UTF-8 文本文件做精确替换（confirm 类）。
use serde_json::Value;

const MAX_EDIT: u64 = 256 * 1024;
const MAX_PREVIEW_CHARS: usize = 12_000;

struct EditRequest {
    rel: String,
    old_string: String,
    new_string: String,
    replace_all: bool,
}

pub fn preview(state: &crate::AppState, args: Value) -> Result<String, String> {
    let req = parse_args(&args)?;
    let original = read_target(state, &req)?;
    let (updated, count) = apply_edit(&original, &req)?;
    Ok(build_preview(&req.rel, &original, &updated, count))
}

pub fn run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let req = parse_args(&args)?;
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let path = super::resolve_in_sandbox(&sandbox, &req.rel)?;
    let original = read_target(state, &req)?;
    let (updated, count) = apply_edit(&original, &req)?;

    std::fs::write(&path, updated).map_err(|e| format!("写入失败：{e}"))?;
    Ok(format!("已编辑沙盒文件：{}（替换 {} 处）", req.rel, count))
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

fn read_target(state: &crate::AppState, req: &EditRequest) -> Result<String, String> {
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let path = super::resolve_in_sandbox(&sandbox, &req.rel)?;
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
