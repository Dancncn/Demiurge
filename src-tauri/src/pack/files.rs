//! Pack 内文件浏览、lore 批量导入与素材授权警告。
//!
//! 跨模块依赖：
//! - `use super::manifest::{...}` 复用 manifest 读写、路径校验、avatar mime。
//!
//! 公开 API（通过 `mod.rs` 的 `pub use` 重导出）：
//! `list_pack_files` / `read_pack_file` / `import_pack_lore_files` / `credit_warnings`。
use base64::{engine::general_purpose, Engine as _};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use super::manifest::{
    avatar_mime, pack_dir, read_manifest_no_avatar, read_manifest_with_avatar, resolve_pack_file,
    validate_lore_file_extension, validate_relative_file, PackFileContent, PackFileEntry,
    PackLoreFile, PackManifest, DEFAULT_LORE_EXTENSIONS, MAX_LORE_FILE_BYTES,
    MAX_LORE_INDEX_FILES, MAX_PACK_LIST_ENTRIES, MAX_PACK_READ_BYTES,
};

/// 计算 manifest 的授权缺失警告（非阻塞）。avatar/persona/lore 无 credit 记录时提示。
pub fn credit_warnings(manifest: &PackManifest) -> Vec<String> {
    let mut warnings = Vec::new();
    if manifest.credits.is_empty() && manifest.license.is_none() {
        warnings
            .push("角色包未声明任何素材 credits / license，请确认导入素材已获得授权。".to_string());
    }
    let credited: HashSet<&str> = manifest.credits.iter().map(|c| c.asset.as_str()).collect();
    let mut check = |asset: Option<&str>, label: &str| {
        if let Some(asset) = asset {
            if !credited.contains(asset) {
                warnings.push(format!("{label}（{asset}）缺少 credits 记录"));
            }
        }
    };
    check(manifest.avatar.as_deref(), "avatar");
    check(Some(manifest.persona.as_str()), "persona");
    for lore in &manifest.lorebook {
        check(Some(&lore.path), "lorebook");
    }
    warnings
}

/// 批量导入 lore 文件到 <pack>/<lore_root>/。文件名取 bare file_name，扩展名按 lore entry 或默认。
pub fn import_pack_lore_files(
    packs_dir: &Path,
    id: &str,
    files: Vec<PackLoreFile>,
) -> Result<PackManifest, String> {
    let dir = pack_dir(packs_dir, id);
    if !dir.is_dir() {
        return Err(format!("角色包 `{id}` 不存在"));
    }
    let manifest = read_manifest_no_avatar(&dir)?;
    if files.is_empty() {
        return Err("没有可导入的 lore 文件".to_string());
    }
    if files.len() > MAX_LORE_INDEX_FILES {
        return Err(format!("lore 文件过多：最多 {MAX_LORE_INDEX_FILES} 个"));
    }
    let lore_root = manifest
        .lorebook
        .first()
        .and_then(|e| {
            Path::new(&e.path)
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map(Path::to_path_buf)
        })
        .unwrap_or_else(|| PathBuf::from("lore"));
    let dest_root = resolve_pack_file(&dir, &lore_root.to_string_lossy(), "lorebook.path")?;
    fs::create_dir_all(&dest_root).map_err(|e| format!("创建 lore 目录失败：{e}"))?;
    for file in &files {
        let name = Path::new(&file.name)
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| format!("lore 文件名非法：{}", file.name))?
            .to_string();
        let ext_ok = manifest
            .lorebook
            .iter()
            .any(|e| validate_lore_file_extension(&name, e).is_ok())
            || {
                let ext = Path::new(&name)
                    .extension()
                    .and_then(|v| v.to_str())
                    .map(|v| v.to_ascii_lowercase())
                    .unwrap_or_default();
                DEFAULT_LORE_EXTENSIONS.iter().any(|d| *d == ext)
            };
        if !ext_ok {
            return Err(format!("lore 文件扩展名不在允许列表：{name}"));
        }
        if file.bytes.len() as u64 > MAX_LORE_FILE_BYTES {
            return Err(format!(
                "lore 文件过大（>{MAX_LORE_FILE_BYTES} 字节）：{name}"
            ));
        }
        fs::write(dest_root.join(&name), &file.bytes)
            .map_err(|e| format!("写入 lore 文件 {name} 失败：{e}"))?;
    }
    read_manifest_with_avatar(&dir)
}

/// 列出包内某子目录的文件树（一层）。sub_dir 为 None 时列包根。canonicalize 前缀校验防逃逸。
pub fn list_pack_files(
    packs_dir: &Path,
    id: &str,
    sub_dir: Option<&str>,
) -> Result<Vec<PackFileEntry>, String> {
    let dir = pack_dir(packs_dir, id);
    if !dir.is_dir() {
        return Err(format!("角色包 `{id}` 不存在"));
    }
    let base = dir
        .canonicalize()
        .map_err(|e| format!("路径校验失败：{e}"))?;
    let target = match sub_dir {
        Some(sub) => {
            validate_relative_file(sub, "sub_dir")?;
            let joined = dir.join(sub);
            let canon = joined
                .canonicalize()
                .map_err(|e| format!("路径校验失败：{e}"))?;
            if !canon.starts_with(&base) {
                return Err("子目录路径越界".to_string());
            }
            canon
        }
        None => base,
    };
    let entries = fs::read_dir(&target).map_err(|e| format!("读取目录失败：{e}"))?;
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let p = entry.path();
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let rel = p
            .strip_prefix(&dir)
            .ok()
            .and_then(|r| r.to_str())
            .map(|s| s.replace('\\', "/"))
            .unwrap_or_default();
        if rel.is_empty() {
            continue;
        }
        let modified_ms = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        out.push(PackFileEntry {
            path: rel,
            is_dir: meta.is_dir(),
            size: meta.len(),
            modified_ms,
        });
        if out.len() >= MAX_PACK_LIST_ENTRIES {
            break;
        }
    }
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.path.cmp(&b.path)));
    Ok(out)
}

/// 读取包内单个文件：md/txt/json 返 text，图片返 base64 data URL，其余仅返大小。
pub fn read_pack_file(packs_dir: &Path, id: &str, path: &str) -> Result<PackFileContent, String> {
    let dir = pack_dir(packs_dir, id);
    if !dir.is_dir() {
        return Err(format!("角色包 `{id}` 不存在"));
    }
    validate_relative_file(path, "path")?;
    let joined = dir.join(path);
    let canon = joined
        .canonicalize()
        .map_err(|e| format!("路径校验失败：{e}"))?;
    let base = dir
        .canonicalize()
        .map_err(|e| format!("路径校验失败：{e}"))?;
    if !canon.starts_with(&base) {
        return Err("文件路径越界".to_string());
    }
    if !canon.is_file() {
        return Err(format!("不是文件：{path}"));
    }
    let meta = fs::metadata(&canon).map_err(|e| format!("读取文件信息失败：{e}"))?;
    let size = meta.len();
    let truncated = size > MAX_PACK_READ_BYTES;
    let ext = canon
        .extension()
        .and_then(|v| v.to_str())
        .map(|v| v.to_ascii_lowercase())
        .unwrap_or_default();
    let text_ext = matches!(ext.as_str(), "md" | "markdown" | "txt" | "json");
    let image_mime = avatar_mime(&canon.to_string_lossy());
    if text_ext {
        let mut bytes = fs::read(&canon).map_err(|e| format!("读取文件失败：{e}"))?;
        if truncated {
            bytes.truncate(MAX_PACK_READ_BYTES as usize);
        }
        let text = String::from_utf8_lossy(&bytes).into_owned();
        Ok(PackFileContent {
            path: path.to_string(),
            text: Some(text),
            data_url: None,
            truncated,
        })
    } else if let Some(mime) = image_mime {
        if size > MAX_PACK_READ_BYTES {
            return Err(format!("图片过大（>{MAX_PACK_READ_BYTES} 字节）"));
        }
        let bytes = fs::read(&canon).map_err(|e| format!("读取图片失败：{e}"))?;
        let data_url = format!(
            "data:{mime};base64,{}",
            general_purpose::STANDARD.encode(&bytes)
        );
        Ok(PackFileContent {
            path: path.to_string(),
            text: None,
            data_url: Some(data_url),
            truncated: false,
        })
    } else {
        Ok(PackFileContent {
            path: path.to_string(),
            text: None,
            data_url: None,
            truncated,
        })
    }
}
