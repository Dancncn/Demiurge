//! shell：在沙盒目录内执行短时 shell 命令（confirm 类）。
use serde_json::Value;
use std::collections::BTreeMap;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

const DEFAULT_TIMEOUT_SECS: u64 = 15;
const MAX_TIMEOUT_SECS: u64 = 60;
const OUTPUT_LIMIT: usize = 12_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ShellRiskClass {
    Low,
    Medium,
    High,
}

impl ShellRiskClass {
    fn label(self) -> &'static str {
        match self {
            ShellRiskClass::Low => "低：只读/检查类命令",
            ShellRiskClass::Medium => "中：可能写入沙盒或启动本地任务",
            ShellRiskClass::High => "高：包含下载、删除、权限或外部执行特征",
        }
    }
}

#[derive(Clone, Debug)]
struct ShellSafetyProfile {
    risk: ShellRiskClass,
    reasons: Vec<&'static str>,
}

struct ShellRequest {
    command: String,
    cwd: String,
    timeout_secs: u64,
    inherit_env: bool,
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

    let profile = classify_command(&req.command);
    let env_policy = if req.inherit_env {
        "继承当前进程环境变量（可能暴露本机凭据环境变量）"
    } else {
        "使用最小跨平台环境白名单（PATH/HOME/TEMP/SystemRoot 等）"
    };

    Ok(format!(
        "将在沙盒内执行 shell 命令：\n\n$ {}\n\n工作目录：{}\n超时：{} 秒\n风险分类：{}\n风险原因：{}\n隔离策略：cwd 限定在沙盒、stdin 关闭、超时终止、输出截断；{}\n注意：这不是完整 OS 级系统沙箱，macOS/Linux/Windows 进程隔离能力仍按平台逐步增强。",
        req.command,
        cwd.strip_prefix(&sandbox)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| cwd.to_string_lossy().to_string()),
        req.timeout_secs,
        profile.risk.label(),
        profile.reasons.join("；"),
        env_policy,
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

    let mut cmd = shell_command(&req.command);
    if !req.inherit_env {
        cmd.env_clear().envs(safe_env());
    }
    let mut child = cmd
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

    let inherit_env = args
        .get("inherit_env")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    Ok(ShellRequest {
        command,
        cwd,
        timeout_secs,
        inherit_env,
    })
}

pub fn safety_summary(command: &str) -> String {
    let profile = classify_command(command);
    format!("{}（{}）", profile.risk.label(), profile.reasons.join("；"))
}

fn classify_command(command: &str) -> ShellSafetyProfile {
    let lower = command.to_ascii_lowercase();
    let mut risk = ShellRiskClass::Low;
    let mut reasons = Vec::new();

    if contains_any(&lower, &["rm ", "del ", "rmdir", "remove-item", "format "]) {
        risk = ShellRiskClass::High;
        reasons.push("包含删除/破坏性文件操作关键词");
    }
    if contains_any(
        &lower,
        &["curl ", "wget ", "irm ", "iwr ", "http://", "https://"],
    ) {
        risk = risk.max(ShellRiskClass::High);
        reasons.push("包含联网下载或外部 URL");
    }
    if contains_any(
        &lower,
        &["sudo", "runas", "chmod ", "chown ", "setfacl", "takeown"],
    ) {
        risk = risk.max(ShellRiskClass::High);
        reasons.push("包含权限/所有权变更关键词");
    }
    if contains_any(
        &lower,
        &[
            "npm install",
            "pnpm add",
            "yarn add",
            "cargo install",
            "pip install",
        ],
    ) {
        risk = risk.max(ShellRiskClass::High);
        reasons.push("包含依赖安装或可执行代码获取");
    }
    if contains_any(
        &lower,
        &[">", "tee ", "touch ", "mkdir ", "mv ", "cp ", "copy "],
    ) {
        risk = risk.max(ShellRiskClass::Medium);
        reasons.push("可能写入或移动沙盒内文件");
    }
    if contains_any(
        &lower,
        &[
            "npm test",
            "npm run",
            "cargo test",
            "cargo check",
            "pytest",
            "vitest",
        ],
    ) {
        risk = risk.max(ShellRiskClass::Medium);
        reasons.push("会执行本地构建/测试脚本");
    }
    if reasons.is_empty() {
        reasons.push("未匹配高风险关键词，仍会按 shell 进程执行请求确认");
    }

    ShellSafetyProfile { risk, reasons }
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn safe_env() -> BTreeMap<String, String> {
    const ALLOW: &[&str] = &[
        "PATH",
        "Path",
        "HOME",
        "USERPROFILE",
        "TEMP",
        "TMP",
        "SystemRoot",
        "WINDIR",
        "COMSPEC",
        "SHELL",
        "LANG",
        "LC_ALL",
    ];
    ALLOW
        .iter()
        .filter_map(|key| {
            std::env::var(key)
                .ok()
                .map(|value| ((*key).to_string(), value))
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_read_only_and_destructive_commands() {
        assert_eq!(
            classify_command("git status --short").risk,
            ShellRiskClass::Low
        );
        assert_eq!(classify_command("rm -rf target").risk, ShellRiskClass::High);
        assert_eq!(classify_command("npm test").risk, ShellRiskClass::Medium);
    }

    #[test]
    fn safe_env_excludes_secret_like_variables() {
        std::env::set_var("DEMIURGE_TEST_SECRET_TOKEN", "secret");
        let env = safe_env();
        assert!(!env.contains_key("DEMIURGE_TEST_SECRET_TOKEN"));
    }
}
