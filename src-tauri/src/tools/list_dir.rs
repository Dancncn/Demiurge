//! list_dir：列出沙盒内目录的直接子项。
use serde_json::Value;
use std::fs;
use std::path::Path;

const DEFAULT_LIMIT: usize = 200;
const MAX_LIMIT: usize = 500;

#[derive(Clone, Debug)]
struct DirEntryView {
    name: String,
    kind: &'static str,
    size: Option<u64>,
}

pub fn run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let rel = super::args::optional_str(&args, "path").unwrap_or("");
    let include_hidden = super::args::optional_bool(&args, "include_hidden", false);
    let limit = super::args::optional_u64_clamped(
        &args,
        "limit",
        DEFAULT_LIMIT as u64,
        1,
        MAX_LIMIT as u64,
    ) as usize;

    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let path = super::resolve_in_sandbox(&sandbox, rel)?;
    list_dir_path(&path, rel, include_hidden, limit)
}

fn list_dir_path(
    path: &Path,
    rel: &str,
    include_hidden: bool,
    limit: usize,
) -> Result<String, String> {
    let meta = fs::metadata(path).map_err(|e| format!("无法访问目录：{e}"))?;
    if !meta.is_dir() {
        return Err("path 必须是目录".to_string());
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(path).map_err(|e| format!("读取目录失败：{e}"))? {
        let entry = entry.map_err(|e| format!("读取目录项失败：{e}"))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !include_hidden && name.starts_with('.') {
            continue;
        }
        let meta = entry
            .metadata()
            .map_err(|e| format!("读取目录项元数据失败：{e}"))?;
        let kind = if meta.is_dir() {
            "dir"
        } else if meta.is_file() {
            "file"
        } else {
            "other"
        };
        entries.push(DirEntryView {
            name,
            kind,
            size: meta.is_file().then_some(meta.len()),
        });
    }

    entries.sort_by(|a, b| {
        let kind_order = |kind: &str| match kind {
            "dir" => 0,
            "file" => 1,
            _ => 2,
        };
        kind_order(a.kind).cmp(&kind_order(b.kind)).then_with(|| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        })
    });

    let total = entries.len();
    let truncated = total > limit;
    entries.truncate(limit);

    let shown_path = if rel.trim().is_empty() {
        "."
    } else {
        rel.trim()
    };
    let mut out = format!("Directory listing for `{shown_path}` ({total} entries):\n");
    if entries.is_empty() {
        out.push_str("(empty)");
    } else {
        for entry in entries {
            match entry.size {
                Some(size) => out.push_str(&format!(
                    "- [{}] {} ({} bytes)\n",
                    entry.kind, entry.name, size
                )),
                None => out.push_str(&format!("- [{}] {}\n", entry.kind, entry.name)),
            }
        }
        if truncated {
            out.push_str(&format!("…已达到 limit={limit}，结果已截断\n"));
        }
    }
    Ok(out.trim_end().to_string())
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
        std::env::temp_dir().join(format!("demiurge-list-dir-{label}-{nonce}"))
    }

    #[test]
    fn lists_direct_children_sorted_by_kind_and_name() {
        let root = temp_dir("sorted");
        fs::create_dir_all(root.join("z_dir")).unwrap();
        fs::create_dir_all(root.join("a_dir")).unwrap();
        fs::write(root.join("b.txt"), "hello").unwrap();
        fs::write(root.join(".hidden"), "secret").unwrap();

        let out = list_dir_path(&root, "", false, 20).unwrap();
        assert!(out.contains("[dir] a_dir"));
        assert!(out.contains("[dir] z_dir"));
        assert!(out.contains("[file] b.txt (5 bytes)"));
        assert!(!out.contains(".hidden"));
        assert!(out.find("[dir] a_dir").unwrap() < out.find("[file] b.txt").unwrap());

        let out = list_dir_path(&root, "", true, 20).unwrap();
        assert!(out.contains(".hidden"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reports_truncation() {
        let root = temp_dir("limit");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.txt"), "a").unwrap();
        fs::write(root.join("b.txt"), "b").unwrap();

        let out = list_dir_path(&root, "", false, 1).unwrap();
        assert!(out.contains("limit=1"));
        let _ = fs::remove_dir_all(root);
    }
}
