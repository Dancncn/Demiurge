//! open_path：用系统默认处理器打开文件/应用/URL。
//! 该工具被标为 Permission::Confirm（执行前必须用户确认），并在此对 target 做硬性校验：
//! 拒绝 UNC/网络路径与危险 URL 协议——即便用户点了确认，也不放行这些高危目标。
use serde_json::Value;

/// 仅放行的安全 URL 协议；其余带 scheme 的目标（ms-msdt: / search-ms: / 自定义协议等）一律拒绝。
const ALLOWED_SCHEMES: [&str; 4] = ["http", "https", "file", "mailto"];

pub fn run(args: Value) -> Result<String, String> {
    let target = args["target"].as_str().ok_or("缺少参数 target")?.trim();
    if target.is_empty() {
        return Err("target 不能为空".to_string());
    }
    validate(target)?;

    #[cfg(target_os = "windows")]
    let result = {
        // start "" <target>：第一个空引号是窗口标题占位，避免把带空格的目标当标题
        std::process::Command::new("cmd")
            .args(["/C", "start", "", target])
            .spawn()
    };
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(target).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(target).spawn();

    match result {
        Ok(_) => Ok(format!("已请求用系统默认程序打开：{target}")),
        Err(e) => Err(format!("打开失败：{e}")),
    }
}

fn validate(target: &str) -> Result<(), String> {
    // UNC / 网络路径：可触发远端可执行，直接拒绝
    if target.starts_with("\\\\") || target.starts_with("//") {
        return Err("出于安全考虑，拒绝打开 UNC/网络路径".to_string());
    }
    // 带 scheme 的 URL 只允许安全协议
    if let Some(scheme) = url_scheme(target) {
        let s = scheme.to_ascii_lowercase();
        if !ALLOWED_SCHEMES.contains(&s.as_str()) {
            return Err(format!(
                "出于安全考虑，拒绝打开协议 {s}:（仅允许 http/https/file/mailto）"
            ));
        }
    }
    Ok(())
}

/// 提取 URL scheme（若有）。注意区分 Windows 盘符："C:\..." 的单字母不算 scheme。
fn url_scheme(t: &str) -> Option<&str> {
    let idx = t.find(':')?;
    let scheme = &t[..idx];
    // 单字母 + ':' 视为盘符（C:\...），不是协议
    if scheme.len() <= 1 {
        return None;
    }
    let mut chars = scheme.chars();
    let first_ok = chars
        .next()
        .map(|c| c.is_ascii_alphabetic())
        .unwrap_or(false);
    let rest_ok = scheme
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if first_ok && rest_ok {
        Some(scheme)
    } else {
        None
    }
}
