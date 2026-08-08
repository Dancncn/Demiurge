//! clipboard：读取系统剪贴板文本。
use serde::Deserialize;
use serde_json::Value;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

const DEFAULT_MAX_CHARS: usize = 4_000;
const MAX_CHARS: usize = 20_000;
const TIMEOUT_SECS: u64 = 3;

#[derive(Deserialize)]
struct Args {
    action: Option<String>,
    max_characters: Option<usize>,
}

struct ClipboardCommand {
    program: &'static str,
    args: &'static [&'static str],
}

pub fn run(args: Value) -> Result<String, String> {
    let args: Args = serde_json::from_value(args).map_err(|e| format!("参数错误：{e}"))?;
    let action = args
        .action
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("read")
        .to_ascii_lowercase();
    if action != "read" {
        return Err("clipboard 目前只支持 action=read".to_string());
    }
    let max_chars = args
        .max_characters
        .unwrap_or(DEFAULT_MAX_CHARS)
        .clamp(1, MAX_CHARS);
    let text = read_clipboard_text()?;
    let truncated = text.chars().count() > max_chars;
    let text = if truncated {
        let head: String = text.chars().take(max_chars).collect();
        format!("{head}\n…[clipboard 输出已按 max_characters 截断]")
    } else {
        text
    };
    Ok(format!(
        "Clipboard text\n\nTruncated: {truncated}\nCharacters: {}\n\n{}",
        text.chars().count(),
        text
    ))
}

fn read_clipboard_text() -> Result<String, String> {
    let mut errors = Vec::new();
    for candidate in clipboard_commands() {
        match run_clipboard_command(candidate) {
            Ok(text) => return Ok(text),
            Err(err) => errors.push(err),
        }
    }
    Err(format!(
        "无法读取剪贴板文本。{}",
        if errors.is_empty() {
            "当前平台没有可用的剪贴板读取命令。".to_string()
        } else {
            errors.join("; ")
        }
    ))
}

fn run_clipboard_command(candidate: ClipboardCommand) -> Result<String, String> {
    let (tx, rx) = mpsc::channel();
    let program = candidate.program;
    std::thread::spawn(move || {
        let result = Command::new(candidate.program)
            .args(candidate.args)
            .output();
        let _ = tx.send((candidate.program, result));
    });

    let (program, output) = rx
        .recv_timeout(Duration::from_secs(TIMEOUT_SECS))
        .map_err(|_| format!("{program} 读取剪贴板超时（>{TIMEOUT_SECS}s）"))?;
    let output = output.map_err(|e| format!("{program} 不可用：{e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("{program} 返回非零状态")
        } else {
            format!("{program} 返回非零状态：{stderr}")
        });
    }
    String::from_utf8(output.stdout).map_err(|e| format!("{program} 输出不是 UTF-8 文本：{e}"))
}

fn clipboard_commands() -> Vec<ClipboardCommand> {
    platform_clipboard_commands(std::env::consts::OS)
}

fn platform_clipboard_commands(os: &str) -> Vec<ClipboardCommand> {
    match os {
        "windows" => vec![ClipboardCommand {
            program: "powershell",
            args: &["-NoProfile", "-Command", "Get-Clipboard -Raw"],
        }],
        "macos" => vec![ClipboardCommand {
            program: "pbpaste",
            args: &[],
        }],
        "linux" => vec![
            ClipboardCommand {
                program: "wl-paste",
                args: &["-n"],
            },
            ClipboardCommand {
                program: "xclip",
                args: &["-selection", "clipboard", "-out"],
            },
            ClipboardCommand {
                program: "xsel",
                args: &["--clipboard", "--output"],
            },
        ],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_platform_clipboard_commands() {
        assert_eq!(
            platform_clipboard_commands("windows")[0].program,
            "powershell"
        );
        assert_eq!(platform_clipboard_commands("macos")[0].program, "pbpaste");
        let linux = platform_clipboard_commands("linux");
        assert_eq!(linux.len(), 3);
        assert!(platform_clipboard_commands("unknown").is_empty());
    }

    #[test]
    fn rejects_unsupported_actions() {
        let result = run(serde_json::json!({ "action": "write" }));
        assert!(result.unwrap_err().contains("action=read"));
    }
}
