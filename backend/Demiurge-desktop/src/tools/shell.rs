//! shell：在沙盒目录内执行短时 shell 命令（confirm 类）。
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

const DEFAULT_TIMEOUT_SECS: u64 = 15;
const STRICT_TIMEOUT_SECS: u64 = 8;
const MAX_TIMEOUT_SECS: u64 = 60;
const OUTPUT_LIMIT: usize = 12_000;
const ENV_ALLOWLIST: &[&str] = &[
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
    fn id(self) -> &'static str {
        match self {
            ShellRiskClass::ReadOnly => "read_only",
            ShellRiskClass::BuildTest => "build_test",
            ShellRiskClass::FileWrite => "file_write",
            ShellRiskClass::Network => "network",
            ShellRiskClass::DependencyInstall => "dependency_install",
            ShellRiskClass::Destructive => "destructive",
            ShellRiskClass::Privilege => "privilege",
            ShellRiskClass::ExternalExecution => "external_execution",
        }
    }

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

#[derive(Clone, Debug, Serialize)]
pub struct ShellPolicyState {
    pub platform: &'static str,
    pub default_isolation: &'static str,
    pub strict_timeout_secs: u64,
    pub max_timeout_secs: u64,
    pub env_allowlist: Vec<&'static str>,
    pub strict_blocked_risks: Vec<ShellRiskView>,
    pub risk_rules: Vec<ShellRiskRuleView>,
    pub containment: ShellContainmentView,
}

#[derive(Clone, Debug, Serialize)]
pub struct ShellRiskView {
    pub id: &'static str,
    pub label: &'static str,
    pub severity: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct ShellRiskRuleView {
    pub class: ShellRiskView,
    pub reason: &'static str,
    pub patterns: Vec<&'static str>,
    pub blocked_in_strict: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ShellContainmentView {
    pub process_group: bool,
    pub kill_process_tree_on_timeout: bool,
    pub filesystem_sandbox: &'static str,
    pub network_sandbox: &'static str,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShellIsolationMode {
    Standard,
    Strict,
    Sandboxed,
}

impl ShellIsolationMode {
    fn label(self) -> &'static str {
        match self {
            ShellIsolationMode::Standard => "standard",
            ShellIsolationMode::Strict => "strict",
            ShellIsolationMode::Sandboxed => "sandboxed",
        }
    }

    fn blocks_high_risk(self) -> bool {
        matches!(
            self,
            ShellIsolationMode::Strict | ShellIsolationMode::Sandboxed
        )
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct ShellCommandSpec {
    program: String,
    args: Vec<String>,
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

pub fn policy_state() -> ShellPolicyState {
    let strict_blocked_risks = [
        ShellRiskClass::Network,
        ShellRiskClass::DependencyInstall,
        ShellRiskClass::Destructive,
        ShellRiskClass::Privilege,
        ShellRiskClass::ExternalExecution,
    ]
    .into_iter()
    .map(risk_view)
    .collect();

    let risk_rules = RISK_RULES
        .iter()
        .map(|rule| ShellRiskRuleView {
            class: risk_view(rule.class),
            reason: rule.reason,
            patterns: rule.needles.to_vec(),
            blocked_in_strict: rule.class.blocked_in_strict(),
        })
        .collect();

    ShellPolicyState {
        platform: std::env::consts::OS,
        default_isolation: ShellIsolationMode::Standard.label(),
        strict_timeout_secs: STRICT_TIMEOUT_SECS,
        max_timeout_secs: MAX_TIMEOUT_SECS,
        env_allowlist: ENV_ALLOWLIST.to_vec(),
        strict_blocked_risks,
        risk_rules,
        containment: containment_view(),
    }
}

fn risk_view(class: ShellRiskClass) -> ShellRiskView {
    ShellRiskView {
        id: class.id(),
        label: class.label(),
        severity: class.severity(),
    }
}

fn containment_view() -> ShellContainmentView {
    ShellContainmentView {
        process_group: true,
        kill_process_tree_on_timeout: true,
        filesystem_sandbox: platform_filesystem_sandbox(),
        network_sandbox: platform_network_sandbox(),
        notes: vec![
            "所有 shell 子进程都会以独立进程组/进程树启动，并在超时时终止整棵进程树",
            "strict 会拒绝联网、依赖安装、破坏性、提权和外部执行类命令，并强制最小环境",
            "sandboxed 模式在 macOS 使用 sandbox-exec，在 Linux/WSL 使用 bubblewrap；运行时不可用会 fail closed",
        ],
    }
}

fn platform_filesystem_sandbox() -> &'static str {
    match std::env::consts::OS {
        "macos" => "sandbox-exec in sandboxed mode",
        "linux" => "bubblewrap in sandboxed mode",
        "windows" => "unsupported on native Windows; process-tree containment only",
        _ => "unsupported on this platform; process-tree containment only",
    }
}

fn platform_network_sandbox() -> &'static str {
    match std::env::consts::OS {
        "macos" => "sandbox-exec denies network in sandboxed mode",
        "linux" => "bubblewrap unshares network in sandboxed mode",
        _ => "policy_only",
    }
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
    validate_isolation_policy(&req, &profile)?;
    let env_policy = env_policy_label(&req);

    Ok(format!(
        "将在沙盒内执行 shell 命令：\n\n$ {}\n\n工作目录：{}\n超时：{} 秒\n风险分类：{} ({})\n风险原因：{}\n隔离模式：{}\n隔离策略：cwd 限定在沙盒、stdin 关闭、独立进程组/进程树、超时终止整棵进程树、输出截断；{}\n平台 containment：文件系统={}；网络={}\n注意：strict 模式会清空环境后仅注入白名单变量，并拒绝联网、依赖安装、破坏性、提权或外部执行类命令；sandboxed 模式还会要求平台 OS sandbox wrapper 可用。",
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
        platform_filesystem_sandbox(),
        platform_network_sandbox(),
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

    let mut cmd = build_shell_command(&req, &sandbox, &cwd)?;
    if !req.inherit_env || req.isolation.blocks_high_risk() {
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
            terminate_process_tree(&mut child);
            let _ = child.wait();
            return Err(format!(
                "shell 命令超时（>{}s），已尝试终止进程树",
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
        "sandboxed" => ShellIsolationMode::Sandboxed,
        other => return Err(format!("未知 shell isolation 模式：{other}")),
    };
    let default_timeout = if isolation.blocks_high_risk() {
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
    if isolation.blocks_high_risk() && inherit_env {
        return Err(format!(
            "{} isolation 不允许 inherit_env=true",
            isolation.label()
        ));
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
    if req.isolation.blocks_high_risk() && profile.risk.blocked_in_strict() {
        return Err(format!(
            "{} isolation 拒绝执行 {} 命令：{}",
            req.isolation.label(),
            profile.risk.label(),
            profile.reasons.join("；")
        ));
    }
    if req.isolation == ShellIsolationMode::Sandboxed {
        ensure_sandbox_runtime_available()?;
    }
    Ok(())
}

fn ensure_sandbox_runtime_available() -> Result<(), String> {
    let runtime = platform_sandbox_runtime()
        .ok_or_else(|| "当前平台不支持 shell sandboxed isolation".to_string())?;
    if !command_available(runtime) {
        return Err(format!(
            "shell sandboxed isolation 需要 `{runtime}`，但当前 PATH 中不可用"
        ));
    }
    Ok(())
}

fn env_policy_label(req: &ShellRequest) -> &'static str {
    if req.isolation == ShellIsolationMode::Strict {
        "strict 模式强制 env_clear，仅传递最小跨平台环境白名单（PATH/HOME/TEMP/SystemRoot 等）"
    } else if req.isolation == ShellIsolationMode::Sandboxed {
        "sandboxed 模式强制 env_clear，并要求平台 OS sandbox wrapper 可用"
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
    ENV_ALLOWLIST
        .iter()
        .filter_map(|key| {
            std::env::var(key)
                .ok()
                .map(|value| ((*key).to_string(), value))
        })
        .collect()
}

fn build_shell_command(req: &ShellRequest, sandbox: &Path, cwd: &Path) -> Result<Command, String> {
    let base = shell_command_spec(&req.command);
    let spec = if req.isolation == ShellIsolationMode::Sandboxed {
        sandboxed_shell_spec(&base, sandbox, cwd)?
    } else {
        base
    };
    let mut cmd = command_from_spec(spec);
    apply_process_containment(&mut cmd);
    Ok(cmd)
}

fn command_from_spec(spec: ShellCommandSpec) -> Command {
    let mut cmd = Command::new(spec.program);
    cmd.args(spec.args);
    cmd
}

#[cfg(windows)]
fn shell_command_spec(command: &str) -> ShellCommandSpec {
    ShellCommandSpec {
        program: "bash".to_string(),
        args: vec!["-lc".to_string(), command.to_string()],
    }
}

#[cfg(not(windows))]
fn shell_command_spec(command: &str) -> ShellCommandSpec {
    ShellCommandSpec {
        program: "sh".to_string(),
        args: vec!["-lc".to_string(), command.to_string()],
    }
}

fn sandboxed_shell_spec(
    base: &ShellCommandSpec,
    sandbox: &Path,
    cwd: &Path,
) -> Result<ShellCommandSpec, String> {
    ensure_sandbox_runtime_available()?;
    let runtime = platform_sandbox_runtime()
        .ok_or_else(|| "当前平台不支持 shell sandboxed isolation".to_string())?;
    Ok(sandboxed_shell_spec_for_runtime(
        runtime, base, sandbox, cwd,
    ))
}

fn sandboxed_shell_spec_for_runtime(
    runtime: &str,
    base: &ShellCommandSpec,
    sandbox: &Path,
    cwd: &Path,
) -> ShellCommandSpec {
    match runtime {
        "bwrap" => bubblewrap_spec(base, sandbox, cwd),
        "sandbox-exec" => sandbox_exec_spec(base, sandbox),
        _ => base.clone(),
    }
}

fn platform_sandbox_runtime() -> Option<&'static str> {
    match std::env::consts::OS {
        "linux" => Some("bwrap"),
        "macos" => Some("sandbox-exec"),
        _ => None,
    }
}

fn bubblewrap_spec(base: &ShellCommandSpec, sandbox: &Path, cwd: &Path) -> ShellCommandSpec {
    let sandbox = sandbox.to_string_lossy().to_string();
    let cwd = cwd.to_string_lossy().to_string();
    let temp = std::env::temp_dir().to_string_lossy().to_string();
    let mut args = vec![
        "--die-with-parent".to_string(),
        "--unshare-net".to_string(),
        "--ro-bind".to_string(),
        "/".to_string(),
        "/".to_string(),
        "--bind".to_string(),
        sandbox.clone(),
        sandbox,
        "--bind".to_string(),
        temp.clone(),
        temp,
        "--dev".to_string(),
        "/dev".to_string(),
        "--proc".to_string(),
        "/proc".to_string(),
        "--chdir".to_string(),
        cwd,
        base.program.clone(),
    ];
    args.extend(base.args.clone());
    ShellCommandSpec {
        program: "bwrap".to_string(),
        args,
    }
}

fn sandbox_exec_spec(base: &ShellCommandSpec, sandbox: &Path) -> ShellCommandSpec {
    let temp = std::env::temp_dir();
    let profile = format!(
        "(version 1)\n\
         (deny default)\n\
         (allow process*)\n\
         (allow file-read*)\n\
         (allow file-write* (subpath \"{}\") (subpath \"{}\"))\n\
         (deny network*)",
        sandbox_profile_path(sandbox),
        sandbox_profile_path(&temp)
    );
    let mut args = vec!["-p".to_string(), profile, base.program.clone()];
    args.extend(base.args.clone());
    ShellCommandSpec {
        program: "sandbox-exec".to_string(),
        args,
    }
}

fn sandbox_profile_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn command_available(program: &str) -> bool {
    if program.contains(std::path::MAIN_SEPARATOR) {
        return Path::new(program).exists();
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| {
        let candidate = dir.join(program);
        if candidate.exists() {
            return true;
        }
        #[cfg(windows)]
        {
            if dir.join(format!("{program}.exe")).exists() {
                return true;
            }
        }
        false
    })
}

fn apply_process_containment(cmd: &mut Command) {
    apply_platform_process_containment(cmd);
}

#[cfg(windows)]
fn apply_platform_process_containment(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
}

#[cfg(unix)]
fn apply_platform_process_containment(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    cmd.process_group(0);
}

#[cfg(not(any(unix, windows)))]
fn apply_platform_process_containment(_cmd: &mut Command) {}

fn terminate_process_tree(child: &mut Child) {
    terminate_platform_process_tree(child);
}

#[cfg(windows)]
fn terminate_platform_process_tree(child: &mut Child) {
    let pid = child.id().to_string();
    let _ = Command::new("taskkill")
        .args(["/PID", &pid, "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.kill();
}

#[cfg(unix)]
fn terminate_platform_process_tree(child: &mut Child) {
    let pgid = format!("-{}", child.id());
    let _ = Command::new("kill")
        .args(["-TERM", &pgid])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    sleep(Duration::from_millis(200));
    if matches!(child.try_wait(), Ok(None)) {
        let _ = Command::new("kill")
            .args(["-KILL", &pgid])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
}

#[cfg(not(any(unix, windows)))]
fn terminate_platform_process_tree(child: &mut Child) {
    let _ = child.kill();
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
    fn sandboxed_isolation_reuses_strict_policy() {
        assert!(parse_args(&json!({
            "command": "git status",
            "isolation": "sandboxed",
            "inherit_env": true
        }))
        .is_err());

        let req =
            parse_args(&json!({ "command": "curl https://example.com", "isolation": "sandboxed" }))
                .unwrap();
        let profile = classify_command(&req.command);
        let err = validate_isolation_policy(&req, &profile).unwrap_err();
        assert!(err.contains("sandboxed isolation 拒绝执行"));
    }

    #[test]
    fn strict_isolation_uses_shorter_default_timeout() {
        let req = parse_args(&json!({ "command": "git status", "isolation": "strict" })).unwrap();
        assert_eq!(req.timeout_secs, STRICT_TIMEOUT_SECS);
        assert_eq!(req.isolation, ShellIsolationMode::Strict);

        let sandboxed =
            parse_args(&json!({ "command": "git status", "isolation": "sandboxed" })).unwrap();
        assert_eq!(sandboxed.timeout_secs, STRICT_TIMEOUT_SECS);
        assert_eq!(sandboxed.isolation, ShellIsolationMode::Sandboxed);
    }

    #[test]
    fn safe_env_excludes_secret_like_variables() {
        std::env::set_var("DEMIURGE_TEST_SECRET_TOKEN", "secret");
        let env = safe_env();
        assert!(!env.contains_key("DEMIURGE_TEST_SECRET_TOKEN"));
    }

    #[test]
    fn policy_state_exposes_strict_shell_guardrails() {
        let state = policy_state();
        assert_eq!(state.default_isolation, "standard");
        assert_eq!(state.strict_timeout_secs, STRICT_TIMEOUT_SECS);
        assert!(state.env_allowlist.contains(&"PATH"));
        assert!(state
            .strict_blocked_risks
            .iter()
            .any(|risk| risk.id == "network"));
        assert!(state.risk_rules.iter().any(|rule| {
            rule.class.id == "dependency_install"
                && rule.blocked_in_strict
                && rule.patterns.contains(&"npm install")
        }));
        assert!(state.containment.process_group);
        assert!(state.containment.kill_process_tree_on_timeout);
        assert_ne!(state.containment.filesystem_sandbox, "not_configured");
    }

    #[test]
    fn bubblewrap_spec_unshares_network_and_binds_sandbox() {
        let base = ShellCommandSpec {
            program: "sh".to_string(),
            args: vec!["-lc".to_string(), "npm test".to_string()],
        };
        let spec = sandboxed_shell_spec_for_runtime(
            "bwrap",
            &base,
            Path::new("/tmp/demiurge-sandbox"),
            Path::new("/tmp/demiurge-sandbox/app"),
        );

        assert_eq!(spec.program, "bwrap");
        assert!(spec.args.contains(&"--unshare-net".to_string()));
        assert!(spec.args.contains(&"--die-with-parent".to_string()));
        assert!(spec.args.contains(&"/tmp/demiurge-sandbox".to_string()));
        assert_eq!(spec.args.last().map(String::as_str), Some("npm test"));
    }

    #[test]
    fn sandbox_exec_spec_denies_network_and_limits_writes() {
        let base = ShellCommandSpec {
            program: "sh".to_string(),
            args: vec!["-lc".to_string(), "cargo test".to_string()],
        };
        let spec = sandboxed_shell_spec_for_runtime(
            "sandbox-exec",
            &base,
            Path::new("/tmp/demiurge-sandbox"),
            Path::new("/tmp/demiurge-sandbox"),
        );

        assert_eq!(spec.program, "sandbox-exec");
        let profile = &spec.args[1];
        assert!(profile.contains("(deny network*)"));
        assert!(profile.contains("(allow file-write*"));
        assert!(profile.contains("/tmp/demiurge-sandbox"));
    }
}
