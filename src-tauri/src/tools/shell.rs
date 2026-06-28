//! shell：在沙盒目录内执行短时 shell 命令（confirm 类）。
use serde_json::Value;
use std::collections::BTreeMap;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

const DEFAULT_TIMEOUT_SECS: u64 = 15;
const STRICT_TIMEOUT_SECS: u64 = 8;
const MAX_TIMEOUT_SECS: u64 = 60;
const OUTPUT_LIMIT: usize = 12_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ShellRiskClass {
    ReadOnly,
    BuildTest,
    FileWrite,
    Network,
    DependencyInstall,
    Destructive,
    Privilege,
    ExternalExecution,
}

impl ShellRiskClass {
    fn label(self) -> &'static str {
        match self {
            ShellRiskClass::ReadOnly => "只读：检查/列出项目状态",
            ShellRiskClass::BuildTest => "构建测试：执行本地脚本或测试",
            ShellRiskClass::FileWrite => "文件写入：可能修改沙盒内文件",
            ShellRiskClass::Network => "联网：访问外部网络或 URL",
            ShellRiskClass::DependencyInstall => "依赖安装：获取并执行外部代码",
            ShellRiskClass::Destructive => "破坏性：删除或清理文件",
            ShellRiskClass::Privilege => "权限：修改权限/所有权或提权",
            ShellRiskClass::ExternalExecution => "外部执行：下载后执行或解释远程脚本",
        }
    }

    fn severity(self) -> &'static str {
        match self {
            ShellRiskClass::ReadOnly => "low",
            ShellRiskClass::BuildTest | ShellRiskClass::FileWrite => "medium",
            ShellRiskClass::Network
            | ShellRiskClass::DependencyInstall
            | ShellRiskClass::Destructive
            | ShellRiskClass::Privilege
            | ShellRiskClass::ExternalExecution => "high",
        }
    }

    fn blocked_in_strict(self) -> bool {
        matches!(
            self,
            ShellRiskClass::Network
                | ShellRiskClass::DependencyInstall
                | ShellRiskClass::Destructive
                | ShellRiskClass::Privilege
                | ShellRiskClass::ExternalExecution
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShellIsolationMode {
    Standard,
    Strict,
}

impl ShellIsolationMode {
    fn label(self) -> &'static str {
        match self {
            ShellIsolationMode::Standard => "standard",
            ShellIsolationMode::Strict => "strict",
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
    isolation: ShellIsolationMode,
}

struct RiskRule {
    class: ShellRiskClass,
    needles: &'static [&'static str],
    reason: &'static str,
}

const RISK_RULES: &[RiskRule] = &[
    RiskRule {
        class: ShellRiskClass::ExternalExecution,
        needles: &[
            "curl ",
            "wget ",
            "irm ",
            "iwr ",
            "| sh",
            "| bash",
            "iex ",
            "invoke-expression",
            "python -c",
            "node -e",
        ],
        reason: "包含远程/内联脚本执行特征",
    },
    RiskRule {
        class: ShellRiskClass::Privilege,
        needles: &["sudo", "runas", "chmod ", "chown ", "setfacl", "takeown"],
        reason: "包含权限/所有权变更关键词",
    },
    RiskRule {
        class: ShellRiskClass::Destructive,
        needles: &[
            "rm ",
            "rm -",
            "del ",
            "rmdir",
            "remove-item",
            "format ",
            "git clean",
            "git reset --hard",
        ],
        reason: "包含删除/破坏性文件操作关键词",
    },
    RiskRule {
        class: ShellRiskClass::DependencyInstall,
        needles: &[
            "npm install",
            "pnpm add",
            "yarn add",
            "cargo install",
            "pip install",
            "uv pip install",
            "go install",
        ],
        reason: "包含依赖安装或可执行代码获取",
    },
    RiskRule {
        class: ShellRiskClass::Network,
        needles: &[
            "curl ",
            "wget ",
            "irm ",
            "iwr ",
            "http://",
            "https://",
            " gh ",
            "git clone",
        ],
        reason: "包含联网访问或外部 URL",
    },
    RiskRule {
        class: ShellRiskClass::FileWrite,
        needles: &[
            ">",
            "tee ",
            "touch ",
            "mkdir ",
            "mv ",
            "cp ",
            "copy ",
            "write-output",
        ],
        reason: "可能写入或移动沙盒内文件",
    },
    RiskRule {
        class: ShellRiskClass::BuildTest,
        needles: &[
            "npm test",
            "npm run",
            "pnpm test",
            "pnpm run",
            "cargo test",
            "cargo check",
            "pytest",
            "vitest",
            "go test",
        ],
        reason: "会执行本地构建/测试脚本",
    },
];

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
    validate_isolation_policy(&req, &profile)?;
    let env_policy = env_policy_label(&req);

    Ok(format!(
        "将在沙盒内执行 shell 命令：\n\n$ {}\n\n工作目录：{}\n超时：{} 秒\n风险分类：{} ({})\n风险原因：{}\n隔离模式：{}\n隔离策略：cwd 限定在沙盒、stdin 关闭、超时终止、输出截断；{}\n注意：strict 模式会清空环境后仅注入白名单变量，并拒绝联网、依赖安装、破坏性、提权或外部执行类命令；这仍不是完整 OS 级系统沙箱。",
        req.command,
        cwd.strip_prefix(&sandbox)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| cwd.to_string_lossy().to_string()),
        req.timeout_secs,
        profile.risk.label(),
        profile.risk.severity(),
        profile.reasons.join("；"),
        req.isolation.label(),
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

    let profile = classify_command(&req.command);
    validate_isolation_policy(&req, &profile)?;

    let mut cmd = shell_command(&req.command);
    if !req.inherit_env || req.isolation == ShellIsolationMode::Strict {
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
    let isolation = match super::args::optional_str(args, "isolation").unwrap_or("standard") {
        "" | "standard" => ShellIsolationMode::Standard,
        "strict" => ShellIsolationMode::Strict,
        other => return Err(format!("未知 shell isolation 模式：{other}")),
    };
    let default_timeout = if isolation == ShellIsolationMode::Strict {
        STRICT_TIMEOUT_SECS
    } else {
        DEFAULT_TIMEOUT_SECS
    };
    let timeout_secs = super::args::optional_u64_clamped(
        args,
        "timeout_secs",
        default_timeout,
        1,
        MAX_TIMEOUT_SECS,
    );

    let inherit_env = args
        .get("inherit_env")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if isolation == ShellIsolationMode::Strict && inherit_env {
        return Err("strict isolation 不允许 inherit_env=true".to_string());
    }

    Ok(ShellRequest {
        command,
        cwd,
        timeout_secs,
        inherit_env,
        isolation,
    })
}

pub fn safety_summary(command: &str) -> String {
    let profile = classify_command(command);
    format!(
        "{} / {}（{}）",
        profile.risk.severity(),
        profile.risk.label(),
        profile.reasons.join("；")
    )
}

fn classify_command(command: &str) -> ShellSafetyProfile {
    let lower = format!(" {} ", command.to_ascii_lowercase());
    let mut risk = ShellRiskClass::ReadOnly;
    let mut reasons = Vec::new();

    for rule in RISK_RULES {
        if contains_any(&lower, rule.needles) {
            risk = risk.max(rule.class);
            if !reasons.contains(&rule.reason) {
                reasons.push(rule.reason);
            }
        }
    }
    if reasons.is_empty() {
        reasons.push("未匹配写入/联网/提权关键词，按只读 shell 命令处理但仍需确认");
    }

    ShellSafetyProfile { risk, reasons }
}

fn validate_isolation_policy(
    req: &ShellRequest,
    profile: &ShellSafetyProfile,
) -> Result<(), String> {
    if req.isolation == ShellIsolationMode::Strict && profile.risk.blocked_in_strict() {
        return Err(format!(
            "strict isolation 拒绝执行 {} 命令：{}",
            profile.risk.label(),
            profile.reasons.join("；")
        ));
    }
    Ok(())
}

fn env_policy_label(req: &ShellRequest) -> &'static str {
    if req.isolation == ShellIsolationMode::Strict {
        "strict 模式强制 env_clear，仅传递最小跨平台环境白名单（PATH/HOME/TEMP/SystemRoot 等）"
    } else if req.inherit_env {
        "继承当前进程环境变量（可能暴露本机凭据环境变量）"
    } else {
        "使用最小跨平台环境白名单（PATH/HOME/TEMP/SystemRoot 等）"
    }
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
    use serde_json::json;

    #[test]
    fn classifies_read_only_and_high_risk_commands() {
        assert_eq!(
            classify_command("git status --short").risk,
            ShellRiskClass::ReadOnly
        );
        assert_eq!(
            classify_command("cargo test").risk,
            ShellRiskClass::BuildTest
        );
        assert_eq!(
            classify_command("touch out.txt").risk,
            ShellRiskClass::FileWrite
        );
        assert_eq!(
            classify_command("curl https://example.com").risk,
            ShellRiskClass::ExternalExecution
        );
        assert_eq!(
            classify_command("npm install").risk,
            ShellRiskClass::DependencyInstall
        );
        assert_eq!(
            classify_command("rm -rf target").risk,
            ShellRiskClass::Destructive
        );
        assert_eq!(
            classify_command("sudo chown a b").risk,
            ShellRiskClass::Privilege
        );
    }

    #[test]
    fn strict_isolation_rejects_inherit_env_and_high_risk() {
        assert!(parse_args(&json!({
            "command": "git status",
            "isolation": "strict",
            "inherit_env": true
        }))
        .is_err());

        let req = parse_args(&json!({ "command": "npm install", "isolation": "strict" })).unwrap();
        let profile = classify_command(&req.command);
        assert!(validate_isolation_policy(&req, &profile).is_err());
    }

    #[test]
    fn strict_isolation_uses_shorter_default_timeout() {
        let req = parse_args(&json!({ "command": "git status", "isolation": "strict" })).unwrap();
        assert_eq!(req.timeout_secs, STRICT_TIMEOUT_SECS);
        assert_eq!(req.isolation, ShellIsolationMode::Strict);
    }

    #[test]
    fn safe_env_excludes_secret_like_variables() {
        std::env::set_var("DEMIURGE_TEST_SECRET_TOKEN", "secret");
        let env = safe_env();
        assert!(!env.contains_key("DEMIURGE_TEST_SECRET_TOKEN"));
    }
}
