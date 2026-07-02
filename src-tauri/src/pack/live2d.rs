//! Live2D 模型导入与文件名归一化。
//!
//! 跨模块依赖：
//! - `use super::manifest::{...}` 复用 manifest 读写、路径校验。
//!
//! 公开 API（通过 `mod.rs` 的 `pub use` 重导出）：
//! `import_live2d_folder` / `normalize_live2d_model_files` / `resolve_live2d_model_path` / `remove_live2d`。
//!
//! 规范化文件名为 ASCII 并重写 model3.json 的 FileReferences，规避 Tauri asset 协议对 CJK
//! 路径的编码 bug（convertFileSrc 不正确 percent-encode 非 ASCII）。
use std::fs;
use std::path::Path;

use super::manifest::{
    pack_dir, read_manifest_no_avatar, read_manifest_with_avatar, validate_manifest_paths,
    validate_pack_files, PackManifest, MAX_LIVE2D_IMPORT_BYTES, MAX_LIVE2D_IMPORT_FILES,
};

/// 导入 Live2D 模型文件夹到 <pack>/live2d/。源目录顶层须有且仅有一个 .model3.json。
/// 复制全部文件后更新 manifest.live2d 指向该 .model3.json（相对路径）。
pub fn import_live2d_folder(
    packs_dir: &Path,
    pack_id: &str,
    src_dir: &str,
) -> Result<PackManifest, String> {
    let pack_path = pack_dir(packs_dir, pack_id);
    if !pack_path.is_dir() {
        return Err(format!("角色包 `{pack_id}` 不存在"));
    }
    let src = Path::new(src_dir);
    if !src.is_dir() {
        return Err("Live2D 模型源目录不存在".to_string());
    }

    // 源目录顶层必须有且仅有一个 .model3.json
    let mut model3_files: Vec<String> = Vec::new();
    for entry in fs::read_dir(src).map_err(|e| format!("读取源目录失败：{e}"))? {
        let entry = entry.map_err(|e| format!("读取目录条目失败：{e}"))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".model3.json") {
            model3_files.push(name);
        }
    }
    match model3_files.len() {
        0 => return Err("源目录中没有 .model3.json 文件".to_string()),
        1 => {}
        _ => {
            return Err(format!(
                "源目录中有多个 .model3.json 文件：{}",
                model3_files.join(", ")
            ))
        }
    }
    let model3_name = model3_files[0].clone();

    // 清空并重建 <pack>/live2d/
    let dest = pack_path.join("live2d");
    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(|e| format!("清理旧 Live2D 目录失败：{e}"))?;
    }
    fs::create_dir_all(&dest).map_err(|e| format!("创建 Live2D 目录失败：{e}"))?;

    let mut total_bytes = 0u64;
    let mut file_count = 0usize;
    copy_live2d_dir_recursive(src, &dest, &mut total_bytes, &mut file_count)?;

    // 规范化文件名为 ASCII 并重写 model3.json 的 FileReferences。
    // 规避 Tauri asset 协议对 CJK 路径的编码 bug（convertFileSrc 不正确 percent-encode 非 ASCII，
    // 引擎 fetch sibling 时报 Network error）。
    let model3_name = normalize_live2d_model_files(&dest, &model3_name)?;

    // 更新 manifest
    let mut manifest = read_manifest_no_avatar(&pack_path)?;
    manifest.live2d = Some(format!("live2d/{model3_name}"));
    manifest.avatar_data_url = None;
    validate_manifest_paths(&manifest)?;
    validate_pack_files(&pack_path, &manifest)?;
    let text = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("序列化角色卡清单失败：{e}"))?;
    fs::write(pack_path.join("manifest.json"), format!("{text}\n"))
        .map_err(|e| format!("保存角色卡清单失败：{e}"))?;
    read_manifest_with_avatar(&pack_path)
}

fn copy_live2d_dir_recursive(
    src: &Path,
    dest: &Path,
    total_bytes: &mut u64,
    file_count: &mut usize,
) -> Result<(), String> {
    for entry in fs::read_dir(src).map_err(|e| format!("读取目录失败：{e}"))? {
        let entry = entry.map_err(|e| format!("读取条目失败：{e}"))?;
        let file_type = entry
            .file_type()
            .map_err(|e| format!("读取文件类型失败：{e}"))?;
        let dest_path = dest.join(entry.file_name());
        if file_type.is_dir() {
            fs::create_dir_all(&dest_path).map_err(|e| format!("创建子目录失败：{e}"))?;
            copy_live2d_dir_recursive(&entry.path(), &dest_path, total_bytes, file_count)?;
        } else if file_type.is_file() {
            *file_count += 1;
            if *file_count > MAX_LIVE2D_IMPORT_FILES {
                return Err(format!(
                    "Live2D 文件过多：最多 {MAX_LIVE2D_IMPORT_FILES} 个文件"
                ));
            }
            let meta = entry
                .metadata()
                .map_err(|e| format!("读取文件元数据失败：{e}"))?;
            *total_bytes = total_bytes.saturating_add(meta.len());
            if *total_bytes > MAX_LIVE2D_IMPORT_BYTES {
                return Err(format!(
                    "Live2D 模型过大：最大允许 {} MB",
                    MAX_LIVE2D_IMPORT_BYTES / 1024 / 1024
                ));
            }
            fs::copy(entry.path(), &dest_path).map_err(|e| format!("复制文件失败：{e}"))?;
        }
    }
    Ok(())
}

/// 把 Live2D 模型文件名规范化为 ASCII，并重写 model3.json 的 FileReferences。
///
/// 规避 Tauri asset 协议对 CJK 路径的编码 bug：`convertFileSrc` 不正确 percent-encode
/// 非 ASCII 路径，导致 webview fetch sibling（.moc3 / 纹理 / 物理 / cdi）时报 Network error。
/// 把 `三月七.moc3` 这类文件名改成 `model.moc3`，CJK 子目录改成 `textures_<n>`，
/// 并同步重写 model3.json 里的引用，让 asset URL 全 ASCII。
///
/// 当前处理 FileReferences 的 Moc / Textures / Physics / DisplayInfo / UserData；
/// Motions / Expressions 里的 CJK 文件名暂未处理（本模型无此情况，后续按需扩展）。
/// 返回新的 model3.json 文件名（相对 dest）。
fn normalize_live2d_model_files(
    dest: &Path,
    original_model3_name: &str,
) -> Result<String, String> {
    let model3_path = dest.join(original_model3_name);
    let raw = fs::read_to_string(&model3_path)
        .map_err(|e| format!("读取 model3.json 失败：{e}"))?;
    let mut json: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("解析 model3.json 失败：{e}"))?;

    if let Some(refs) = json.get_mut("FileReferences").and_then(|v| v.as_object_mut()) {
        // 单文件引用：Moc / Physics / DisplayInfo / UserData
        for (key, ascii) in [
            ("Moc", "model.moc3"),
            ("Physics", "model.physics3.json"),
            ("DisplayInfo", "model.cdi3.json"),
            ("UserData", "model.userdata3.json"),
        ] {
            if let Some(v) = refs.get_mut(key) {
                if let Some(p) = v.as_str().map(|s| s.to_string()) {
                    let new = rename_file_ascii(dest, &p, ascii)?;
                    *v = serde_json::Value::String(new);
                }
            }
        }
        // 纹理数组：路径可能含 CJK 子目录（如 "三月七.4096/texture_00.png"）
        if let Some(texs) = refs.get_mut("Textures").and_then(|v| v.as_array_mut()) {
            let mut tex_idx = 0usize;
            for tex in texs.iter_mut() {
                if let Some(p) = tex.as_str().map(|s| s.to_string()) {
                    let new = rename_texture_path_ascii(dest, &p, &mut tex_idx)?;
                    *tex = serde_json::Value::String(new);
                }
            }
        }
        // TODO: Motions / Expressions 里的 CJK 文件名未处理
    }

    // 重写 model3.json 到 ASCII 名 model.model3.json，删原文件
    let new_name = "model.model3.json";
    let new_path = dest.join(new_name);
    let text = serde_json::to_string_pretty(&json)
        .map_err(|e| format!("序列化 model3.json 失败：{e}"))?;
    fs::write(&new_path, format!("{text}\n"))
        .map_err(|e| format!("写入 model3.json 失败：{e}"))?;
    if original_model3_name != new_name {
        let _ = fs::remove_file(&model3_path);
    }
    Ok(new_name.to_string())
}

/// 把 dest 下的 rel 文件重命名为 ascii_name（仅当 rel 含非 ASCII）。返回新相对路径。
fn rename_file_ascii(dest: &Path, rel: &str, ascii_name: &str) -> Result<String, String> {
    if !rel.chars().any(|c| !c.is_ascii()) {
        return Ok(rel.to_string());
    }
    let old_path = dest.join(rel);
    let new_path = dest.join(ascii_name);
    if old_path.exists() {
        fs::rename(&old_path, &new_path).map_err(|e| format!("重命名 {rel} 失败：{e}"))?;
    }
    Ok(ascii_name.to_string())
}

/// 规范化纹理路径：CJK 目录段 → textures_<idx>，CJK 文件名 → texture_<idx>.<ext>。
fn rename_texture_path_ascii(dest: &Path, rel: &str, idx: &mut usize) -> Result<String, String> {
    if !rel.chars().any(|c| !c.is_ascii()) {
        return Ok(rel.to_string());
    }
    let parts: Vec<&str> = rel.split('/').collect();
    let mut new_parts: Vec<String> = Vec::new();
    let mut cur = dest.to_path_buf();
    for (i, part) in parts.iter().enumerate() {
        if part.chars().any(|c| !c.is_ascii()) {
            if i == parts.len() - 1 {
                // 文件名 CJK
                let ext = Path::new(part)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("png");
                let new = format!("texture_{idx}.{ext}");
                let old_f = cur.join(part);
                let new_f = cur.join(&new);
                if old_f.exists() {
                    fs::rename(&old_f, &new_f)
                        .map_err(|e| format!("重命名纹理 {rel} 失败：{e}"))?;
                }
                new_parts.push(new);
            } else {
                // 目录段 CJK
                let new = format!("textures_{idx}");
                let old_d = cur.join(part);
                let new_d = cur.join(&new);
                if old_d.exists() {
                    fs::rename(&old_d, &new_d)
                        .map_err(|e| format!("重命名目录 {part} 失败：{e}"))?;
                }
                cur = new_d;
                new_parts.push(new);
            }
        } else {
            cur = cur.join(part);
            new_parts.push(part.to_string());
        }
    }
    *idx += 1;
    Ok(new_parts.join("/"))
}

/// 解析当前角色包 Live2D 模型的绝对路径（供前端 convertFileSrc）。未配置或缺失则报错。
pub fn resolve_live2d_model_path(packs_dir: &Path, pack_id: &str) -> Result<String, String> {
    let dir = pack_dir(packs_dir, pack_id);
    if !dir.is_dir() {
        return Err(format!("角色包 `{pack_id}` 不存在"));
    }
    let manifest = read_manifest_no_avatar(&dir)?;
    let live2d = manifest
        .live2d
        .as_deref()
        .ok_or_else(|| "当前角色包未配置 Live2D 模型".to_string())?;
    let path = super::manifest::resolve_pack_file(&dir, live2d, "live2d")?;
    if !path.exists() {
        return Err(format!("Live2D 模型文件不存在：{live2d}"));
    }
    Ok(path.to_string_lossy().to_string())
}

/// 移除角色包的 Live2D 模型：删 <pack>/live2d/ 目录并清空 manifest.live2d。
pub fn remove_live2d(packs_dir: &Path, pack_id: &str) -> Result<PackManifest, String> {
    let dir = pack_dir(packs_dir, pack_id);
    if !dir.is_dir() {
        return Err(format!("角色包 `{pack_id}` 不存在"));
    }
    let mut manifest = read_manifest_no_avatar(&dir)?;
    if manifest.live2d.is_none() {
        return read_manifest_with_avatar(&dir);
    }
    let live2d_dir = dir.join("live2d");
    if live2d_dir.exists() {
        fs::remove_dir_all(&live2d_dir).map_err(|e| format!("删除 Live2D 目录失败：{e}"))?;
    }
    manifest.live2d = None;
    manifest.avatar_data_url = None;
    validate_manifest_paths(&manifest)?;
    let text = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("序列化角色卡清单失败：{e}"))?;
    fs::write(dir.join("manifest.json"), format!("{text}\n"))
        .map_err(|e| format!("保存角色卡清单失败：{e}"))?;
    read_manifest_with_avatar(&dir)
}
