//! shell：在沙盒目录内执行短时 shell 命令（confirm 类）。
use serde_json::Value;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

const DEFAULT_TIMEOUT_SECS: u64 = 15;
const MAX_TIMEOUT_SECS: u64 = 60;
const OUTPUT_LIMIT: usize = 12_000;

struct ShellRequest {
    command: String,
    cwd: String,
    timeout_secs: u64,
}

pub fn preview(state: &crate::AppState, args: Value) -> Result<String, String> {
    let req = parse_args(&args)?;
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let cwd = super::resolve_in_sandbox(&sandbox, &req.cwd)?;
    if !cwd.exists() {
        return Err("cwd 路径不存在".to_string());
    }
    if !cwd.is_dir() {
        return Err("cwd 必须是目录".to_string());
    }

    Ok(format!(
        "将在沙盒内执行 shell 命令：\n\n$ {}\n\n工作目录：{}\n超时：{} 秒",
        req.command,
        cwd.strip_prefix(&sandbox)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| cwd.to_string_lossy().to_string()),
        req.timeout_secs
    ))
}

pub fn run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let req = parse_args(&args)?;
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let cwd = super::resolve_in_sandbox(&sandbox, &req.cwd)?;
    if !cwd.exists() {
        return Err("cwd 路径不存在".to_string());
    }
    if !cwd.is_dir() {
        return Err("cwd 必须是目录".to_string());
    }

    let mut child = shell_command(&req.command)
        .current_dir(&cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 shell 失败：{e}"))?;

    let deadline = Instant::now() + Duration::from_secs(req.timeout_secs);
    loop {
        if let Some(_status) = child.try_wait().map_err(|e| format!("等待命令失败：{e}"))? {
            let output = child
                .wait_with_output()
                .map_err(|e| format!("读取命令输出失败：{e}"))?;
            return Ok(format_output(
                &req.command,
                output.status.code(),
                &output.stdout,
                &output.stderr,
            ));
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "shell 命令超时（>{}s），已尝试终止进程",
                req.timeout_secs
            ));
        }
        sleep(Duration::from_millis(50));
    }
}

fn parse_args(args: &Value) -> Result<ShellRequest, String> {
    let command = super::args::required_non_empty_str(args, "command")?.to_string();
    let cwd = super::args::optional_str(args, "cwd")
        .unwrap_or("")
        .trim()
        .to_string();
    let timeout_secs = super::args::optional_u64_clamped(
        args,
        "timeout_secs",
        DEFAULT_TIMEOUT_SECS,
        1,
        MAX_TIMEOUT_SECS,
    );

    Ok(ShellRequest {
        command,
        cwd,
        timeout_secs,
    })
}

#[cfg(windows)]
fn shell_command(command: &str) -> Command {
    let mut cmd = Command::new("bash");
    cmd.args(["-lc", command]);
    cmd
}

#[cfg(not(windows))]
fn shell_command(command: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.args(["-lc", command]);
    cmd
}

fn format_output(command: &str, code: Option<i32>, stdout: &[u8], stderr: &[u8]) -> String {
    let stdout = String::from_utf8_lossy(stdout).to_string();
    let stderr = String::from_utf8_lossy(stderr).to_string();
    let mut out = String::new();
    out.push_str(&format!(
        "$ {command}\n退出码：{}\n",
        code.map(|c| c.to_string())
            .unwrap_or_else(|| "未知".to_string())
    ));
    if !stdout.trim().is_empty() {
        out.push_str("\nstdout:\n");
        out.push_str(&truncate(&stdout));
    }
    if !stderr.trim().is_empty() {
        out.push_str("\nstderr:\n");
        out.push_str(&truncate(&stderr));
    }
    if stdout.trim().is_empty() && stderr.trim().is_empty() {
        out.push_str("\n（无输出）");
    }
    out
}

fn truncate(s: &str) -> String {
    if s.chars().count() <= OUTPUT_LIMIT {
        s.to_string()
    } else {
        let head: String = s.chars().take(OUTPUT_LIMIT).collect();
        format!("{head}\n…输出已截断")
    }
}
