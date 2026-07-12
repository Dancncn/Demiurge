//! open_path：用系统默认处理器打开文件/应用/URL。
//! 该工具被标为 Permission::Confirm（执行前必须用户确认），并在此对 target 做硬性校验：
//! 拒绝 UNC/网络路径与危险 URL 协议——即便用户点了确认，也不放行这些高危目标。
use serde_json::Value;

#[cfg(target_os = "windows")]
use std::{ffi::c_void, io, ptr};

/// 仅放行的安全 URL 协议；其余带 scheme 的目标（ms-msdt: / search-ms: / 自定义协议等）一律拒绝。
const ALLOWED_SCHEMES: [&str; 4] = ["http", "https", "file", "mailto"];

/// Windows 上直接交给 ShellExecuteW，避免经过 cmd.exe 解释 target 中的元字符。
#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowsOpenBackend {
    ShellExecuteW,
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, PartialEq, Eq)]
struct WindowsShellExecuteRequest {
    verb: Vec<u16>,
    target: Vec<u16>,
}

#[cfg(any(target_os = "windows", test))]
impl WindowsShellExecuteRequest {
    const BACKEND: WindowsOpenBackend = WindowsOpenBackend::ShellExecuteW;

    fn new(target: &str) -> Result<Self, String> {
        Ok(Self {
            verb: nul_terminated_utf16("open")?,
            target: nul_terminated_utf16(target)?,
        })
    }

    #[cfg(target_os = "windows")]
    fn execute(&self) -> io::Result<()> {
        const SW_SHOWNORMAL: i32 = 1;
        debug_assert_eq!(Self::BACKEND, WindowsOpenBackend::ShellExecuteW);

        // SAFETY: `verb` 与 `target` 都是有效、以 NUL 结尾且在调用期间保持存活的
        // UTF-16 缓冲区；其余可选指针按 ShellExecuteW 契约传入 null。
        let result = unsafe {
            shell_execute_w(
                ptr::null_mut(),
                self.verb.as_ptr(),
                self.target.as_ptr(),
                ptr::null(),
                ptr::null(),
                SW_SHOWNORMAL,
            )
        } as isize;

        // ShellExecuteW 的返回值大于 32 表示成功；失败值是 ShellExecute 定义的错误码，
        // 不保证可由 GetLastError 获取，因此把原始码保留在错误消息中。
        if result > 32 {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                format!("ShellExecuteW 失败，错误码 {result}"),
            ))
        }
    }
}

#[cfg(any(target_os = "windows", test))]
fn nul_terminated_utf16(value: &str) -> Result<Vec<u16>, String> {
    if value.contains('\0') {
        return Err("target 不能包含 NUL 字符".to_string());
    }
    let mut encoded: Vec<u16> = value.encode_utf16().collect();
    encoded.push(0);
    Ok(encoded)
}

#[cfg(target_os = "windows")]
#[link(name = "shell32")]
extern "system" {
    #[link_name = "ShellExecuteW"]
    fn shell_execute_w(
        hwnd: *mut c_void,
        operation: *const u16,
        file: *const u16,
        parameters: *const u16,
        directory: *const u16,
        show_command: i32,
    ) -> *mut c_void;
}

pub fn run(args: Value) -> Result<String, String> {
    let target = args["target"].as_str().ok_or("缺少参数 target")?.trim();
    if target.is_empty() {
        return Err("target 不能为空".to_string());
    }
    validate(target)?;

    #[cfg(target_os = "windows")]
    let result = WindowsShellExecuteRequest::new(target).and_then(|request| {
        request
            .execute()
            .map_err(|error| format!("打开失败：{error}"))
    });
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open")
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("打开失败：{error}"));
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open")
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("打开失败：{error}"));

    match result {
        Ok(_) => Ok(format!("已请求用系统默认程序打开：{target}")),
        Err(error) => Err(error),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_nul_terminated(value: &[u16]) -> String {
        assert_eq!(value.last(), Some(&0));
        assert!(!value[..value.len() - 1].contains(&0));
        String::from_utf16(&value[..value.len() - 1]).expect("测试字符串应为有效 UTF-16")
    }

    #[test]
    fn windows_request_uses_native_shell_execute_backend() {
        let request = WindowsShellExecuteRequest::new("C:\\Program Files\\Demiurge\\notes.txt")
            .expect("本地路径应能构造打开请求");

        assert_eq!(
            WindowsShellExecuteRequest::BACKEND,
            WindowsOpenBackend::ShellExecuteW
        );
        assert_eq!(decode_nul_terminated(&request.verb), "open");
    }

    #[test]
    fn windows_request_preserves_url_query_metacharacters_as_data() {
        let target = r#"https://example.com/search?q="alpha beta"&next=(x)|<tag>^100%!"#;
        validate(target).expect("HTTPS URL 应通过校验");

        let request =
            WindowsShellExecuteRequest::new(target).expect("URL 应能构造 ShellExecuteW 请求");

        assert_eq!(decode_nul_terminated(&request.target), target);
    }

    #[test]
    fn windows_request_preserves_local_path_spaces_and_metacharacters_as_data() {
        let target = r#"C:\Program Files\Demiurge & test (draft) ^ 100% !\notes.txt"#;
        validate(target).expect("带空格和元字符的本地路径应通过校验");

        let request =
            WindowsShellExecuteRequest::new(target).expect("本地路径应能构造 ShellExecuteW 请求");

        assert_eq!(decode_nul_terminated(&request.target), target);
    }

    #[test]
    fn windows_request_rejects_interior_nul_instead_of_truncating_target() {
        let error = WindowsShellExecuteRequest::new("https://example.com\0malicious")
            .expect_err("NUL 字符不能传给 ShellExecuteW");

        assert!(error.contains("NUL"));
    }
}
