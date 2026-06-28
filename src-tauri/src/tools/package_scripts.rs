//! package_scripts：读取 package.json scripts，生成建议执行命令但不直接执行。
use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let rel = super::args::optional_str(&args, "path").unwrap_or("package.json");
    let script = super::args::optional_str(&args, "script")
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let path = resolve_package_path(&sandbox, rel)?;
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("读取 package.json 失败：{e}"))?;
    let value: Value =
        serde_json::from_str(&raw).map_err(|e| format!("解析 package.json 失败：{e}"))?;
    let scripts = extract_scripts(&value);
    if scripts.is_empty() {
        return Ok(format!(
            "Package scripts for `{}`\n\nNo scripts found.",
            relative_display(&sandbox, &path)
        ));
    }

    let manager = detect_package_manager(path.parent().unwrap_or(&sandbox));
    let mut out = format!(
        "Package scripts for `{}`\nPackage manager: {manager}\n\n",
        relative_display(&sandbox, &path)
    );

    if let Some(script) = script {
        let Some((_, command)) = scripts.iter().find(|(name, _)| name == script) else {
            out.push_str(&format!(
                "Script `{script}` not found.\n\nAvailable scripts:\n"
            ));
            append_scripts(&mut out, &scripts);
            return Ok(out.trim_end().to_string());
        };
        out.push_str(&format!("Script: {script}\nCommand: {command}\n"));
        out.push_str(&format!(
            "Suggested shell command: {}\n",
            suggested_command(&manager, script)
        ));
        out.push_str("\nNote: package_scripts does not execute scripts. Use shell if you want to run the suggested command.");
    } else {
        out.push_str("Available scripts:\n");
        append_scripts(&mut out, &scripts);
        out.push_str("\nUse `script` to get the suggested shell command for one script.");
    }
    Ok(out.trim_end().to_string())
}

fn resolve_package_path(sandbox: &Path, rel: &str) -> Result<PathBuf, String> {
    let path = super::resolve_in_sandbox(sandbox, rel)?;
    if path.is_dir() {
        Ok(path.join("package.json"))
    } else {
        Ok(path)
    }
}

fn extract_scripts(value: &Value) -> Vec<(String, String)> {
    let Some(map) = value.get("scripts").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut scripts = map
        .iter()
        .filter_map(|(name, command)| {
            Some((
                name.trim().to_string(),
                command.as_str()?.trim().to_string(),
            ))
            .filter(|(name, command)| !name.is_empty() && !command.is_empty())
        })
        .collect::<Vec<_>>();
    scripts.sort_by(|a, b| a.0.cmp(&b.0));
    scripts
}

fn append_scripts(out: &mut String, scripts: &[(String, String)]) {
    for (name, command) in scripts {
        out.push_str(&format!("- {name}: {command}\n"));
    }
}

fn detect_package_manager(dir: &Path) -> String {
    if dir.join("pnpm-lock.yaml").exists() {
        "pnpm".to_string()
    } else if dir.join("yarn.lock").exists() {
        "yarn".to_string()
    } else if dir.join("bun.lockb").exists() || dir.join("bun.lock").exists() {
        "bun".to_string()
    } else {
        "npm".to_string()
    }
}

fn suggested_command(manager: &str, script: &str) -> String {
    let script = quote_script_name(script);
    match manager {
        "pnpm" => format!("pnpm run {script}"),
        "yarn" => format!("yarn {script}"),
        "bun" => format!("bun run {script}"),
        _ => format!("npm run {script}"),
    }
}

fn quote_script_name(script: &str) -> String {
    if script
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-'))
    {
        script.to_string()
    } else {
        format!("'{}'", script.replace('\'', "'\"'\"'"))
    }
}

fn relative_display(sandbox: &Path, path: &Path) -> String {
    path.strip_prefix(sandbox)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("demiurge-package-scripts-{label}-{nonce}"))
    }

    #[test]
    fn extracts_sorted_scripts_and_commands() {
        let value: Value = serde_json::json!({
            "scripts": {
                "test": "vitest",
                "build": "vite build",
                "empty": ""
            }
        });
        let scripts = extract_scripts(&value);
        assert_eq!(scripts[0].0, "build");
        assert_eq!(scripts[1].0, "test");
        assert_eq!(scripts.len(), 2);
    }

    #[test]
    fn detects_package_manager_from_lockfiles() {
        let root = temp_dir("manager");
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(detect_package_manager(&root), "npm");
        std::fs::write(root.join("pnpm-lock.yaml"), "").unwrap();
        assert_eq!(detect_package_manager(&root), "pnpm");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn quotes_unusual_script_names() {
        assert_eq!(suggested_command("npm", "build:web"), "npm run build:web");
        assert_eq!(suggested_command("bun", "my script"), "bun run 'my script'");
    }
}
