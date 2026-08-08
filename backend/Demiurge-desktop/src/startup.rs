use std::path::Path;

#[cfg(any(target_os = "macos", all(unix, not(target_os = "macos"))))]
use std::path::PathBuf;

const ENTRY_NAME: &str = "Demiurge";

pub fn apply_launch_on_startup(enabled: bool) -> Result<(), String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("Failed to resolve current executable for startup setting: {e}"))?;
    platform_apply(enabled, &exe)
}

#[cfg(target_os = "windows")]
fn platform_apply(enabled: bool, exe: &Path) -> Result<(), String> {
    use std::process::Command;

    const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    if enabled {
        let value = quoted_windows_path(exe);
        let output = Command::new("reg")
            .args([
                "add", RUN_KEY, "/v", ENTRY_NAME, "/t", "REG_SZ", "/d", &value, "/f",
            ])
            .output()
            .map_err(|e| format!("Failed to run reg.exe for startup setting: {e}"))?;
        if output.status.success() {
            return Ok(());
        }
        return Err(command_error("reg add", &output));
    }

    let output = Command::new("reg")
        .args(["delete", RUN_KEY, "/v", ENTRY_NAME, "/f"])
        .output()
        .map_err(|e| format!("Failed to run reg.exe for startup setting: {e}"))?;
    if output.status.success() {
        return Ok(());
    }

    let query = Command::new("reg")
        .args(["query", RUN_KEY, "/v", ENTRY_NAME])
        .output()
        .map_err(|e| format!("Failed to verify startup registry value: {e}"))?;
    if query.status.success() {
        Err(command_error("reg delete", &output))
    } else {
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn platform_apply(enabled: bool, exe: &Path) -> Result<(), String> {
    let path = macos_launch_agent_path()?;
    if !enabled {
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|e| format!("Failed to remove LaunchAgent {}: {e}", path.display()))?;
        }
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create LaunchAgents directory: {e}"))?;
    }
    let exe = xml_escape(&exe.to_string_lossy());
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.demiurge.app</string>
  <key>ProgramArguments</key>
  <array>
    <string>{exe}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
</dict>
</plist>
"#
    );
    std::fs::write(&path, plist)
        .map_err(|e| format!("Failed to write LaunchAgent {}: {e}", path.display()))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_apply(enabled: bool, exe: &Path) -> Result<(), String> {
    let path = linux_autostart_path()?;
    if !enabled {
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|e| format!("Failed to remove autostart entry {}: {e}", path.display()))?;
        }
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create autostart directory: {e}"))?;
    }
    let desktop = format!(
        "[Desktop Entry]\nType=Application\nName={ENTRY_NAME}\nExec={}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        desktop_exec(exe)
    );
    std::fs::write(&path, desktop)
        .map_err(|e| format!("Failed to write autostart entry {}: {e}", path.display()))
}

#[cfg(not(any(target_os = "windows", unix)))]
fn platform_apply(enabled: bool, _exe: &Path) -> Result<(), String> {
    if enabled {
        Err("Launch on startup is not supported on this platform.".to_string())
    } else {
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn quoted_windows_path(path: &Path) -> String {
    format!("\"{}\"", path.display())
}

#[cfg(target_os = "windows")]
fn command_error(action: &str, output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    format!(
        "{action} failed with status {}: {}{}",
        output.status,
        stdout.trim(),
        stderr.trim()
    )
}

#[cfg(target_os = "macos")]
fn macos_launch_agent_path() -> Result<PathBuf, String> {
    home_dir()
        .map(|home| {
            home.join("Library")
                .join("LaunchAgents")
                .join("com.demiurge.app.plist")
        })
        .ok_or_else(|| "Failed to resolve home directory for LaunchAgent.".to_string())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn linux_autostart_path() -> Result<PathBuf, String> {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        if !dir.trim().is_empty() {
            return Ok(PathBuf::from(dir)
                .join("autostart")
                .join("demiurge.desktop"));
        }
    }
    home_dir()
        .map(|home| {
            home.join(".config")
                .join("autostart")
                .join("demiurge.desktop")
        })
        .ok_or_else(|| "Failed to resolve home directory for autostart entry.".to_string())
}

#[cfg(any(target_os = "macos", all(unix, not(target_os = "macos"))))]
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

#[cfg(target_os = "macos")]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(all(unix, not(target_os = "macos")))]
fn desktop_exec(path: &Path) -> String {
    let raw = path.to_string_lossy();
    if raw
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | '+'))
    {
        raw.to_string()
    } else {
        format!("\"{}\"", raw.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn plist_values_are_xml_escaped() {
        assert_eq!(xml_escape(r#"/tmp/A&B"C.app"#), "/tmp/A&amp;B&quot;C.app");
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn desktop_exec_quotes_paths_with_spaces() {
        assert_eq!(
            desktop_exec(Path::new("/opt/Demiurge App/demiurge")),
            "\"/opt/Demiurge App/demiurge\""
        );
        assert_eq!(
            desktop_exec(Path::new("/opt/demiurge/demiurge")),
            "/opt/demiurge/demiurge"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_run_value_is_quoted() {
        assert_eq!(
            quoted_windows_path(Path::new(r"C:\Program Files\Demiurge\Demiurge.exe")),
            r#""C:\Program Files\Demiurge\Demiurge.exe""#
        );
    }
}
