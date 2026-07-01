use std::process::Command;

use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Args {
    label: String,
    branch: Option<String>,
}

pub fn create(state: &crate::AppState, args: Value) -> Result<String, String> {
    let args: Args = serde_json::from_value(args).map_err(|e| format!("参数错误：{e}"))?;
    let label = sanitize_label(&args.label);
    if label.is_empty() {
        return Err("label 不能为空".to_string());
    }

    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let wt_root = sandbox.join(".demiurge").join("worktrees");
    std::fs::create_dir_all(&wt_root).map_err(|e| format!("创建 worktree 根目录失败：{e}"))?;
    let path = wt_root.join(&label);
    if path.exists() {
        return Err(format!("worktree 已存在：{}", path.display()));
    }

    let branch = args.branch.unwrap_or_else(|| format!("demiurge/{label}"));
    let output = Command::new("git")
        .args(["worktree", "add", "-b", &branch, &path.to_string_lossy()])
        .current_dir(&sandbox)
        .output()
        .map_err(|e| format!("执行 git worktree add 失败：{e}"))?;

    if !output.status.success() {
        return Err(format!(
            "git worktree add 失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(json!({
        "worktree_path": path,
        "branch": branch,
        "notice": "这是独立 git worktree。子任务在这里操作前应重新读取文件；路径与主沙盒不同。"
    })
    .to_string())
}

pub fn preview(args: Value) -> Result<String, String> {
    let args: Args = serde_json::from_value(args).map_err(|e| format!("参数错误：{e}"))?;
    let label = sanitize_label(&args.label);
    let branch = args.branch.unwrap_or_else(|| format!("demiurge/{label}"));
    Ok(format!(
        "将在沙盒 Git 仓库下创建 worktree label=`{label}` branch=`{branch}`。"
    ))
}

pub(crate) fn sanitize_label(label: &str) -> String {
    label
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_label() {
        assert_eq!(sanitize_label(" fix auth! "), "fix-auth");
    }
}
