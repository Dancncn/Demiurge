//! grep：在沙盒内搜索文本内容。
use regex::{Regex, RegexBuilder};
use serde_json::Value;
use walkdir::WalkDir;

const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 300;
const MAX_FILE_BYTES: u64 = 256 * 1024;
const MAX_LINE_CHARS: usize = 240;

pub fn run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let query = super::args::required_non_empty_str(&args, "query")?;

    let rel = super::args::optional_str(&args, "path").unwrap_or("");
    let case_sensitive = super::args::optional_bool(&args, "case_sensitive", false);
    let regex_mode = super::args::optional_bool(&args, "regex", false);
    let limit = super::args::optional_u64_clamped(
        &args,
        "limit",
        DEFAULT_LIMIT as u64,
        1,
        MAX_LIMIT as u64,
    ) as usize;

    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let root = super::resolve_in_sandbox(&sandbox, rel)?;
    if !root.exists() {
        return Err("path 不存在".to_string());
    }

    let matcher = build_matcher(query, regex_mode, case_sensitive)?;
    let mut out = Vec::new();
    let mut scanned = 0usize;
    let mut skipped = 0usize;
    let mut truncated = false;

    if root.is_file() {
        scanned += 1;
        match search_file(&root, &sandbox, &matcher, limit) {
            Ok(mut rows) => out.append(&mut rows),
            Err(_) => skipped += 1,
        }
        truncated = out.len() >= limit;
    } else if root.is_dir() {
        for entry in WalkDir::new(&root)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if out.len() >= limit {
                truncated = true;
                break;
            }
            if entry.file_type().is_file() {
                scanned += 1;
                match search_file(
                    entry.path(),
                    &sandbox,
                    &matcher,
                    limit.saturating_sub(out.len()),
                ) {
                    Ok(mut rows) => out.append(&mut rows),
                    Err(_) => skipped += 1,
                }
            }
        }
    } else {
        return Err("path 既不是文件也不是目录".to_string());
    }

    if out.is_empty() {
        return Ok(format!(
            "未找到匹配内容（扫描 {scanned} 个文件，跳过 {skipped} 个文件）"
        ));
    }

    let mut text = out.join("\n");
    if truncated {
        text.push_str(&format!("\n…已达到 limit={limit}，结果已截断"));
    }
    if skipped > 0 {
        text.push_str(&format!("\n（跳过 {skipped} 个过大或非 UTF-8 文本文件）"));
    }
    Ok(text)
}

fn build_matcher(query: &str, regex_mode: bool, case_sensitive: bool) -> Result<Regex, String> {
    let pattern = if regex_mode {
        query.to_string()
    } else {
        regex::escape(query)
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|e| format!("非法正则表达式：{e}"))
}

fn search_file(
    path: &std::path::Path,
    sandbox: &std::path::Path,
    matcher: &Regex,
    remaining: usize,
) -> Result<Vec<String>, String> {
    if remaining == 0 {
        return Ok(Vec::new());
    }
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return Err("跳过非文本或过大文件".to_string());
    }
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let rel = path
        .strip_prefix(sandbox)
        .map_err(|_| "路径越界：结果不在沙盒内")?;
    let rel = rel.to_string_lossy().replace('\\', "/");

    let mut rows = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        if matcher.is_match(line) {
            rows.push(format!("{}:{}: {}", rel, idx + 1, trim_line(line)));
            if rows.len() >= remaining {
                break;
            }
        }
    }
    Ok(rows)
}

fn trim_line(line: &str) -> String {
    if line.chars().count() <= MAX_LINE_CHARS {
        line.to_string()
    } else {
        let head: String = line.chars().take(MAX_LINE_CHARS).collect();
        format!("{head}…")
    }
}
