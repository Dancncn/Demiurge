//! glob：在沙盒内按 glob pattern 搜索文件路径。
use globset::{Glob, GlobSetBuilder};
use serde_json::Value;
use std::path::{Component, Path};
use walkdir::WalkDir;

const DEFAULT_LIMIT: usize = 200;
const MAX_LIMIT: usize = 500;

pub fn run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let pattern = args["pattern"].as_str().ok_or("缺少参数 pattern")?.trim();
    if pattern.is_empty() {
        return Err("pattern 不能为空".to_string());
    }
    validate_pattern(pattern)?;

    let base = args["base"].as_str().unwrap_or("");
    let limit = args["limit"].as_u64().map(|n| n as usize).unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);

    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let base_path = super::resolve_in_sandbox(&sandbox, base)?;
    if !base_path.exists() {
        return Err("base 路径不存在".to_string());
    }
    if !base_path.is_dir() {
        return Err("base 必须是目录".to_string());
    }

    let mut builder = GlobSetBuilder::new();
    builder.add(Glob::new(pattern).map_err(|e| format!("非法 glob pattern：{e}"))?);
    let set = builder.build().map_err(|e| format!("构建 glob matcher 失败：{e}"))?;

    let mut matches = Vec::new();
    let mut truncated = false;
    for entry in WalkDir::new(&base_path).follow_links(false).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let rel = path.strip_prefix(&sandbox).map_err(|_| "路径越界：结果不在沙盒内")?;
        if set.is_match(rel) {
            matches.push(rel.to_string_lossy().replace('\\', "/"));
            if matches.len() >= limit {
                truncated = true;
                break;
            }
        }
    }

    if matches.is_empty() {
        return Ok("未找到匹配文件".to_string());
    }

    let mut out = matches.join("\n");
    if truncated {
        out.push_str(&format!("\n…已达到 limit={limit}，结果已截断"));
    }
    Ok(out)
}

fn validate_pattern(pattern: &str) -> Result<(), String> {
    let path = Path::new(pattern);
    if path.is_absolute() {
        return Err("pattern 不能是绝对路径".to_string());
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("pattern 不允许包含 ..".to_string());
    }
    Ok(())
}
