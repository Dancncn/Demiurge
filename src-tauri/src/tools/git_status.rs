//! git_status：读取沙盒目录中的 Git 状态摘要（只读）。
use serde_json::Value;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

const TIMEOUT_SECS: u64 = 5;

pub fn run(state: &crate::AppState, _args: Value) -> Result<String, String> {
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let result = Command::new("git")
            .args(["status", "--short", "--branch"])
            .current_dir(&sandbox)
            .output();
        let _ = tx.send(result);
    });

    let output = rx
        .recv_timeout(Duration::from_secs(TIMEOUT_SECS))
        .map_err(|_| format!("git status 超时（>{TIMEOUT_SECS}s）"))?
        .map_err(|e| format!("执行 git status 失败：{e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.contains("not a git repository") || stderr.contains("不是 git 仓库") {
            return Ok("沙盒目录当前不是 Git 仓库".to_string());
        }
        return Err(if stderr.is_empty() { "git status 返回非零状态".to_string() } else { stderr });
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        Ok("Git 工作区干净".to_string())
    } else {
        Ok(stdout)
    }
}
