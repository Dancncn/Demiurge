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
use base64::{engine::general_purpose, Engine as _};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use super::manifest::{
    read_manifest_no_avatar, read_manifest_with_avatar, resolve_pack_dir, validate_manifest_paths,
    validate_pack_files, PackManifest, MAX_LIVE2D_IMPORT_BYTES, MAX_LIVE2D_IMPORT_FILES,
};

static LIVE2D_IMPORT_SEQ: AtomicU64 = AtomicU64::new(1);
static LIVE2D_MUTATION_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Serialize)]
pub struct Live2DAsset {
    pub path: String,
    pub mime: String,
    pub data: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Live2DBundle {
    pub model_json: String,
    pub assets: Vec<Live2DAsset>,
}

/// 导入 Live2D 模型文件夹到 <pack>/live2d/。源目录顶层须有且仅有一个 .model3.json。
/// 复制全部文件后更新 manifest.live2d 指向该 .model3.json（相对路径）。
pub fn import_live2d_folder(
    packs_dir: &Path,
    pack_id: &str,
    src_dir: &str,
) -> Result<PackManifest, String> {
    let _mutation_guard = LIVE2D_MUTATION_LOCK
        .lock()
        .map_err(|_| "Live2D 导入锁已损坏，已拒绝修改".to_string())?;
    let pack_path = resolve_pack_dir(packs_dir, pack_id)?;
    let pack_base = pack_path
        .canonicalize()
        .map_err(|e| format!("角色包路径校验失败：{e}"))?;
    let src = Path::new(src_dir);
    if !src.is_dir() {
        return Err("Live2D 模型源目录不存在".to_string());
    }
    let source_base = src
        .canonicalize()
        .map_err(|e| format!("Live2D 模型源目录校验失败：{e}"))?;
    if pack_base.starts_with(&source_base) {
        return Err("Live2D 源目录不能包含目标角色包目录".to_string());
    }
    let mut manifest = read_manifest_no_avatar(&pack_path)?;

    // The old model and manifest remain untouched while the entire candidate
    // tree is copied, normalized, and validated in a same-parent staging dir.
    let staging = create_unique_import_dir(&pack_path)?;
    let prepared = prepare_live2d_import(&source_base, &staging);
    let model3_name = match prepared {
        Ok(name) => name,
        Err(error) => {
            let _ = remove_any_path(&staging);
            return Err(error);
        }
    };

    manifest.live2d = Some(format!("live2d/{model3_name}"));
    manifest.avatar_data_url = None;
    if let Err(error) = validate_manifest_paths(&manifest) {
        let _ = remove_any_path(&staging);
        return Err(error);
    }
    let text = match serde_json::to_string_pretty(&manifest) {
        Ok(text) => text,
        Err(error) => {
            let _ = remove_any_path(&staging);
            return Err(format!("序列化角色卡清单失败：{error}"));
        }
    };
    let manifest_temp =
        match write_unique_import_file(&pack_path, "manifest", format!("{text}\n").as_bytes()) {
            Ok(path) => path,
            Err(error) => {
                let _ = remove_any_path(&staging);
                return Err(error);
            }
        };

    install_prepared_live2d(
        &pack_path,
        &staging,
        &manifest_temp,
        &manifest,
        &model3_name,
    )
}

fn prepare_live2d_import(source_base: &Path, staging: &Path) -> Result<String, String> {
    let mut total_bytes = 0u64;
    let mut file_count = 0usize;
    let mut visited_directories = BTreeSet::from([source_base.to_path_buf()]);
    copy_live2d_dir_recursive(
        source_base,
        source_base,
        staging,
        &mut total_bytes,
        &mut file_count,
        &mut visited_directories,
    )?;
    let model3_name = find_top_level_model3(staging)?;
    let model3_name = normalize_live2d_model_files(staging, &model3_name)?;
    validate_live2d_model_tree(staging, &model3_name)?;
    Ok(model3_name)
}

fn copy_live2d_dir_recursive(
    source_base: &Path,
    src: &Path,
    dest: &Path,
    total_bytes: &mut u64,
    file_count: &mut usize,
    visited_directories: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    for entry in fs::read_dir(src).map_err(|e| format!("读取目录失败：{e}"))? {
        let entry = entry.map_err(|e| format!("读取条目失败：{e}"))?;
        let source_path = entry.path();
        let metadata = fs::symlink_metadata(&source_path)
            .map_err(|e| format!("读取 Live2D 源条目元数据失败：{e}"))?;
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            return Err(format!(
                "Live2D 源目录不允许符号链接或 junction：{}",
                source_path.display()
            ));
        }
        let canonical_source = source_path
            .canonicalize()
            .map_err(|e| format!("校验 Live2D 源条目失败：{e}"))?;
        if !canonical_source.starts_with(source_base) {
            return Err(format!(
                "Live2D 源条目经链接解析后逃逸源目录：{}",
                source_path.display()
            ));
        }
        let dest_path = dest.join(entry.file_name());
        if file_type.is_dir() {
            if !visited_directories.insert(canonical_source.clone()) {
                return Err(format!(
                    "Live2D 源目录包含目录链接循环或重复目标：{}",
                    source_path.display()
                ));
            }
            fs::create_dir_all(&dest_path).map_err(|e| format!("创建子目录失败：{e}"))?;
            copy_live2d_dir_recursive(
                source_base,
                &canonical_source,
                &dest_path,
                total_bytes,
                file_count,
                visited_directories,
            )?;
        } else if file_type.is_file() {
            *file_count += 1;
            if *file_count > MAX_LIVE2D_IMPORT_FILES {
                return Err(format!(
                    "Live2D 文件过多：最多 {MAX_LIVE2D_IMPORT_FILES} 个文件"
                ));
            }
            let meta =
                fs::metadata(&canonical_source).map_err(|e| format!("读取文件元数据失败：{e}"))?;
            *total_bytes = total_bytes.saturating_add(meta.len());
            if *total_bytes > MAX_LIVE2D_IMPORT_BYTES {
                return Err(format!(
                    "Live2D 模型过大：最大允许 {} MB",
                    MAX_LIVE2D_IMPORT_BYTES / 1024 / 1024
                ));
            }
            fs::copy(&canonical_source, &dest_path).map_err(|e| format!("复制文件失败：{e}"))?;
        } else {
            return Err(format!(
                "Live2D 源目录包含不支持的特殊文件：{}",
                source_path.display()
            ));
        }
    }
    Ok(())
}

fn find_top_level_model3(dir: &Path) -> Result<String, String> {
    let mut model3_files = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| format!("读取候选目录失败：{e}"))? {
        let entry = entry.map_err(|e| format!("读取候选目录条目失败：{e}"))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|e| format!("读取候选文件元数据失败：{e}"))?;
        if !metadata.is_file() {
            continue;
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "Live2D model3.json 文件名必须是有效 UTF-8".to_string())?;
        if name.ends_with(".model3.json") {
            model3_files.push(name);
        }
    }
    model3_files.sort();
    match model3_files.len() {
        0 => Err("源目录顶层没有 .model3.json 文件".to_string()),
        1 => Ok(model3_files.remove(0)),
        _ => Err(format!(
            "源目录顶层有多个 .model3.json 文件：{}",
            model3_files.join(", ")
        )),
    }
}

/// Validates every model resource reference before moving anything, then moves
/// non-ASCII referenced files into one generated ASCII directory and rewrites
/// every supported FileReferences location (including motions/expressions).
fn normalize_live2d_model_files(dest: &Path, original_model3_name: &str) -> Result<String, String> {
    let model3_path = dest.join(original_model3_name);
    let root = dest
        .canonicalize()
        .map_err(|e| format!("校验 Live2D 候选目录失败：{e}"))?;
    let canonical_model = model3_path
        .canonicalize()
        .map_err(|e| format!("读取 model3.json 失败：{e}"))?;
    if canonical_model.parent() != Some(root.as_path()) || !canonical_model.is_file() {
        return Err("Live2D model3.json 必须是候选目录顶层的普通文件".to_string());
    }
    let raw =
        fs::read_to_string(&canonical_model).map_err(|e| format!("读取 model3.json 失败：{e}"))?;
    let mut json: Value =
        serde_json::from_str(&raw).map_err(|e| format!("解析 model3.json 失败：{e}"))?;
    let references = collect_live2d_refs_checked(&json)?;
    let normalized_dir = choose_normalized_asset_dir(dest);
    let mut mapping = HashMap::new();
    let mut moves = Vec::new();

    for raw_ref in references {
        let normalized = normalize_live2d_reference(&raw_ref)?;
        if mapping.contains_key(&normalized) {
            continue;
        }
        let source = resolve_model_relative_file(&root, &root, &normalized)?;
        if normalized.is_ascii() {
            mapping.insert(normalized.clone(), normalized);
            continue;
        }
        let extension = safe_live2d_extension(&normalized);
        let target_rel = format!("{normalized_dir}/resource_{:04}.{extension}", moves.len());
        let target = dest.join(&target_rel);
        if path_exists_no_follow(&target) {
            return Err(format!("Live2D 规范化目标已存在：{target_rel}"));
        }
        mapping.insert(normalized, target_rel.clone());
        moves.push((source, target, target_rel));
    }

    if !moves.is_empty() {
        fs::create_dir(dest.join(&normalized_dir))
            .map_err(|e| format!("创建 Live2D ASCII 资源目录失败：{e}"))?;
    }
    for (source, target, target_rel) in &moves {
        fs::rename(source, target)
            .map_err(|e| format!("规范化 Live2D 资源 `{target_rel}` 失败：{e}"))?;
    }
    rewrite_live2d_refs(&mut json, &mapping)?;

    let new_name = "model.model3.json";
    let new_path = dest.join(new_name);
    let text =
        serde_json::to_string_pretty(&json).map_err(|e| format!("序列化 model3.json 失败：{e}"))?;
    fs::write(&new_path, format!("{text}\n")).map_err(|e| format!("写入 model3.json 失败：{e}"))?;
    if original_model3_name != new_name {
        remove_any_path(&model3_path)?;
    }
    validate_live2d_model_tree(dest, new_name)?;
    Ok(new_name.to_string())
}

fn choose_normalized_asset_dir(dest: &Path) -> String {
    for index in 0usize.. {
        let name = if index == 0 {
            "normalized_assets".to_string()
        } else {
            format!("normalized_assets_{index}")
        };
        if !path_exists_no_follow(&dest.join(&name)) {
            return name;
        }
    }
    unreachable!()
}

fn safe_live2d_extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| {
            !extension.is_empty()
                && extension.len() <= 16
                && extension.chars().all(|c| c.is_ascii_alphanumeric())
        })
        .map(|extension| extension.to_ascii_lowercase())
        .unwrap_or_else(|| "bin".to_string())
}

/// 解析当前角色包 Live2D 模型的绝对路径（供前端 convertFileSrc）。未配置或缺失则报错。
pub fn resolve_live2d_model_path(packs_dir: &Path, pack_id: &str) -> Result<String, String> {
    let _mutation_guard = LIVE2D_MUTATION_LOCK
        .lock()
        .map_err(|_| "Live2D 修改锁已损坏，已拒绝读取".to_string())?;
    resolve_live2d_model_path_inner(packs_dir, pack_id)
}

fn resolve_live2d_model_path_inner(packs_dir: &Path, pack_id: &str) -> Result<String, String> {
    let dir = resolve_pack_dir(packs_dir, pack_id)?;
    let manifest = read_manifest_no_avatar(&dir)?;
    let live2d = manifest
        .live2d
        .as_deref()
        .ok_or_else(|| "当前角色包未配置 Live2D 模型".to_string())?;
    let path = super::manifest::resolve_pack_file(&dir, live2d, "live2d")?;
    let pack_base = dir
        .canonicalize()
        .map_err(|e| format!("角色包路径校验失败：{e}"))?;
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("Live2D 模型文件不存在或不可访问：{live2d} ({e})"))?;
    if !canonical.starts_with(&pack_base) || !canonical.is_file() {
        return Err(format!("Live2D 模型文件越界或不是普通文件：{live2d}"));
    }
    Ok(canonical.to_string_lossy().to_string())
}

/// 读取当前 Live2D 模型及其初始依赖，供前端生成 blob: URL。
///
/// `untitled-pixi-live2d-engine` 内部使用 XMLHttpRequest 加载 model/moc/texture。
/// 在 Tauri WebView 中，XHR 对 asset:// 本地协议的兼容性不稳定，因此这里把资源
/// 作为 base64 bundle 交给前端，再由前端改写 model3.json 引用到 blob: URL。
pub fn live2d_bundle(packs_dir: &Path, pack_id: &str) -> Result<Live2DBundle, String> {
    let _mutation_guard = LIVE2D_MUTATION_LOCK
        .lock()
        .map_err(|_| "Live2D 修改锁已损坏，已拒绝读取".to_string())?;
    let model_path = PathBuf::from(resolve_live2d_model_path_inner(packs_dir, pack_id)?);
    let model_dir = model_path
        .parent()
        .ok_or_else(|| "Live2D 模型路径缺少父目录".to_string())?;
    let model_root = model_dir
        .canonicalize()
        .map_err(|e| format!("Live2D 模型目录校验失败：{e}"))?;
    let raw = fs::read_to_string(&model_path)
        .map_err(|e| format!("读取 Live2D model3.json 失败：{e}"))?;
    let json: Value =
        serde_json::from_str(&raw).map_err(|e| format!("解析 Live2D model3.json 失败：{e}"))?;
    let refs = collect_live2d_refs_checked(&json)?;

    let mut assets = Vec::new();
    for rel in refs {
        let path = resolve_model_relative_file(&model_root, &model_root, &rel)?;
        let bytes = fs::read(&path).map_err(|e| format!("读取 Live2D 资源 {rel} 失败：{e}"))?;
        assets.push(Live2DAsset {
            path: rel,
            mime: live2d_mime(&path),
            data: general_purpose::STANDARD.encode(bytes),
        });
    }

    Ok(Live2DBundle {
        model_json: raw,
        assets,
    })
}

fn collect_live2d_refs_checked(json: &Value) -> Result<Vec<String>, String> {
    let mut refs = BTreeSet::new();
    let file_refs = json
        .get("FileReferences")
        .and_then(Value::as_object)
        .ok_or_else(|| "Live2D model3.json 缺少 FileReferences 对象".to_string())?;

    let moc = required_live2d_ref(file_refs.get("Moc"), "FileReferences.Moc")?;
    refs.insert(normalize_live2d_reference(moc)?);

    for key in ["Physics", "Pose", "DisplayInfo", "UserData"] {
        if let Some(value) = file_refs.get(key) {
            let path = required_live2d_ref(Some(value), &format!("FileReferences.{key}"))?;
            refs.insert(normalize_live2d_reference(path)?);
        }
    }

    if let Some(value) = file_refs.get("Textures") {
        let textures = value
            .as_array()
            .ok_or_else(|| "FileReferences.Textures 必须是字符串数组".to_string())?;
        for (index, value) in textures.iter().enumerate() {
            let path =
                required_live2d_ref(Some(value), &format!("FileReferences.Textures[{index}]"))?;
            refs.insert(normalize_live2d_reference(path)?);
        }
    }

    if let Some(value) = file_refs.get("Expressions") {
        let expressions = value
            .as_array()
            .ok_or_else(|| "FileReferences.Expressions 必须是数组".to_string())?;
        for (index, expression) in expressions.iter().enumerate() {
            let expression = expression
                .as_object()
                .ok_or_else(|| format!("FileReferences.Expressions[{index}] 必须是对象"))?;
            let path = required_live2d_ref(
                expression.get("File"),
                &format!("FileReferences.Expressions[{index}].File"),
            )?;
            refs.insert(normalize_live2d_reference(path)?);
        }
    }

    if let Some(value) = file_refs.get("Motions") {
        let motions = value
            .as_object()
            .ok_or_else(|| "FileReferences.Motions 必须是对象".to_string())?;
        for (group_name, group) in motions {
            let group = group
                .as_array()
                .ok_or_else(|| format!("FileReferences.Motions.{group_name} 必须是数组"))?;
            for (index, motion) in group.iter().enumerate() {
                let motion = motion.as_object().ok_or_else(|| {
                    format!("FileReferences.Motions.{group_name}[{index}] 必须是对象")
                })?;
                let file = required_live2d_ref(
                    motion.get("File"),
                    &format!("FileReferences.Motions.{group_name}[{index}].File"),
                )?;
                refs.insert(normalize_live2d_reference(file)?);
                if let Some(sound) = motion.get("Sound") {
                    let sound = required_live2d_ref(
                        Some(sound),
                        &format!("FileReferences.Motions.{group_name}[{index}].Sound"),
                    )?;
                    refs.insert(normalize_live2d_reference(sound)?);
                }
            }
        }
    }

    Ok(refs.into_iter().collect())
}

fn required_live2d_ref<'a>(value: Option<&'a Value>, label: &str) -> Result<&'a str, String> {
    value
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| format!("{label} 必须是非空字符串"))
}

fn normalize_live2d_reference(rel: &str) -> Result<String, String> {
    if rel.is_empty() || rel.contains('\0') {
        return Err("Live2D 资源路径不能为空或包含 NUL".to_string());
    }
    let normalized = rel.replace('\\', "/");
    if normalized.starts_with('/') {
        return Err(format!("Live2D 资源路径不能是绝对路径：{rel}"));
    }
    let mut parts = Vec::new();
    for part in normalized.split('/') {
        if part.is_empty() || matches!(part, "." | "..") {
            return Err(format!("Live2D 资源路径包含非法组件：{rel}"));
        }
        if part.contains(':') {
            return Err(format!("Live2D 资源路径包含 Windows prefix/盘符：{rel}"));
        }
        if part.ends_with(' ')
            || part.ends_with('.')
            || part.chars().any(|c| c.is_control())
            || is_windows_reserved_component(part)
        {
            return Err(format!("Live2D 资源路径包含不可移植组件：{rel}"));
        }
        parts.push(part);
    }
    let normalized = parts.join("/");
    let path = Path::new(&normalized);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("Live2D 资源路径包含非法组件：{rel}"));
    }
    Ok(normalized)
}

fn is_windows_reserved_component(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem.strip_prefix("COM").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
        || stem.strip_prefix("LPT").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
}

fn rewrite_live2d_refs(json: &mut Value, mapping: &HashMap<String, String>) -> Result<(), String> {
    let refs = json
        .get_mut("FileReferences")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "Live2D model3.json 缺少 FileReferences 对象".to_string())?;

    for key in ["Moc", "Physics", "Pose", "DisplayInfo", "UserData"] {
        if let Some(value) = refs.get_mut(key) {
            rewrite_live2d_ref_value(value, mapping, &format!("FileReferences.{key}"))?;
        }
    }
    if let Some(textures) = refs.get_mut("Textures").and_then(Value::as_array_mut) {
        for (index, value) in textures.iter_mut().enumerate() {
            rewrite_live2d_ref_value(value, mapping, &format!("FileReferences.Textures[{index}]"))?;
        }
    }
    if let Some(expressions) = refs.get_mut("Expressions").and_then(Value::as_array_mut) {
        for (index, expression) in expressions.iter_mut().enumerate() {
            let file = expression
                .as_object_mut()
                .and_then(|entry| entry.get_mut("File"))
                .ok_or_else(|| format!("FileReferences.Expressions[{index}].File 缺失"))?;
            rewrite_live2d_ref_value(
                file,
                mapping,
                &format!("FileReferences.Expressions[{index}].File"),
            )?;
        }
    }
    if let Some(motions) = refs.get_mut("Motions").and_then(Value::as_object_mut) {
        for (group_name, group) in motions {
            let group = group
                .as_array_mut()
                .ok_or_else(|| format!("FileReferences.Motions.{group_name} 必须是数组"))?;
            for (index, motion) in group.iter_mut().enumerate() {
                let motion = motion.as_object_mut().ok_or_else(|| {
                    format!("FileReferences.Motions.{group_name}[{index}] 必须是对象")
                })?;
                let file = motion.get_mut("File").ok_or_else(|| {
                    format!("FileReferences.Motions.{group_name}[{index}].File 缺失")
                })?;
                rewrite_live2d_ref_value(
                    file,
                    mapping,
                    &format!("FileReferences.Motions.{group_name}[{index}].File"),
                )?;
                if let Some(sound) = motion.get_mut("Sound") {
                    rewrite_live2d_ref_value(
                        sound,
                        mapping,
                        &format!("FileReferences.Motions.{group_name}[{index}].Sound"),
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn rewrite_live2d_ref_value(
    value: &mut Value,
    mapping: &HashMap<String, String>,
    label: &str,
) -> Result<(), String> {
    let raw = value
        .as_str()
        .ok_or_else(|| format!("{label} 必须是字符串"))?;
    let normalized = normalize_live2d_reference(raw)?;
    let rewritten = mapping
        .get(&normalized)
        .ok_or_else(|| format!("{label} 缺少已校验的资源映射"))?;
    *value = Value::String(rewritten.clone());
    Ok(())
}

fn validate_live2d_model_tree(model_root: &Path, model3_name: &str) -> Result<(), String> {
    let model3_name = normalize_live2d_reference(model3_name)?;
    if model3_name.contains('/') {
        return Err("Live2D model3.json 必须位于模型目录顶层".to_string());
    }
    let root = model_root
        .canonicalize()
        .map_err(|e| format!("校验 Live2D 模型目录失败：{e}"))?;
    let model_path = root.join(&model3_name);
    let canonical_model = model_path
        .canonicalize()
        .map_err(|e| format!("Live2D model3.json 不存在或不可访问：{e}"))?;
    if canonical_model.parent() != Some(root.as_path()) || !canonical_model.is_file() {
        return Err("Live2D model3.json 越界或不是普通文件".to_string());
    }
    let raw = fs::read_to_string(&canonical_model)
        .map_err(|e| format!("读取 Live2D model3.json 失败：{e}"))?;
    let json: Value =
        serde_json::from_str(&raw).map_err(|e| format!("解析 Live2D model3.json 失败：{e}"))?;
    for rel in collect_live2d_refs_checked(&json)? {
        resolve_model_relative_file(&root, &root, &rel)?;
    }
    Ok(())
}

fn resolve_model_relative_file(
    model_dir: &Path,
    model_root: &Path,
    rel: &str,
) -> Result<PathBuf, String> {
    let normalized = normalize_live2d_reference(rel)?;
    let root = model_root
        .canonicalize()
        .map_err(|e| format!("Live2D 模型根目录不可访问：{e}"))?;
    let base = model_dir
        .canonicalize()
        .map_err(|e| format!("Live2D 模型引用目录不可访问：{e}"))?;
    if !base.starts_with(&root) {
        return Err("Live2D 模型引用目录越界".to_string());
    }
    let joined = base.join(Path::new(&normalized));
    let canon = joined
        .canonicalize()
        .map_err(|e| format!("Live2D 资源不存在或不可访问：{rel} ({e})"))?;
    if !canon.starts_with(&root) {
        return Err(format!("Live2D 资源路径越界：{rel}"));
    }
    if !canon.is_file() {
        return Err(format!("Live2D 资源不是文件：{rel}"));
    }
    Ok(canon)
}

fn create_unique_import_dir(pack_path: &Path) -> Result<PathBuf, String> {
    loop {
        let candidate = next_import_path(pack_path, "candidate", "tmp");
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "创建 Live2D 导入临时目录 `{}` 失败：{error}",
                    candidate.display()
                ))
            }
        }
    }
}

fn write_unique_import_file(
    pack_path: &Path,
    label: &str,
    bytes: &[u8],
) -> Result<PathBuf, String> {
    loop {
        let candidate = next_import_path(pack_path, label, "tmp");
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "创建 Live2D 导入临时文件 `{}` 失败：{error}",
                    candidate.display()
                ))
            }
        };
        let result = (|| -> Result<(), String> {
            file.write_all(bytes).map_err(|e| e.to_string())?;
            file.flush().map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            Ok(())
        })();
        if let Err(error) = result {
            drop(file);
            let _ = remove_any_path(&candidate);
            return Err(format!(
                "写入 Live2D 导入临时文件 `{}` 失败：{error}",
                candidate.display()
            ));
        }
        return Ok(candidate);
    }
}

fn next_import_path(pack_path: &Path, label: &str, suffix: &str) -> PathBuf {
    let sequence = LIVE2D_IMPORT_SEQ.fetch_add(1, Ordering::Relaxed);
    pack_path.join(format!(
        ".live2d-{label}-{}-{sequence}.{suffix}",
        std::process::id()
    ))
}

fn unique_unused_import_path(pack_path: &Path, label: &str) -> PathBuf {
    loop {
        let candidate = next_import_path(pack_path, label, "bak");
        if !path_exists_no_follow(&candidate) {
            return candidate;
        }
    }
}

fn install_prepared_live2d(
    pack_path: &Path,
    staging: &Path,
    manifest_temp: &Path,
    manifest: &PackManifest,
    model3_name: &str,
) -> Result<PackManifest, String> {
    let destination = pack_path.join("live2d");
    let manifest_path = pack_path.join("manifest.json");
    let live2d_backup = unique_unused_import_path(pack_path, "previous-model");
    let manifest_backup = unique_unused_import_path(pack_path, "previous-manifest");
    let had_live2d = path_exists_no_follow(&destination);
    let mut old_live2d_moved = false;
    let mut new_live2d_installed = false;
    let mut manifest_backup_created = false;
    let mut new_manifest_installed = false;

    let install_result = (|| -> Result<PackManifest, String> {
        if had_live2d {
            fs::rename(&destination, &live2d_backup)
                .map_err(|e| format!("备份旧 Live2D 目录失败：{e}"))?;
            old_live2d_moved = true;
        }
        fs::rename(staging, &destination).map_err(|e| format!("提交新 Live2D 目录失败：{e}"))?;
        new_live2d_installed = true;

        fs::copy(&manifest_path, &manifest_backup)
            .map_err(|e| format!("备份角色包清单失败：{e}"))?;
        manifest_backup_created = true;
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&manifest_backup)
            .and_then(|file| file.sync_all())
            .map_err(|e| format!("同步角色包清单备份失败：{e}"))?;
        fs::rename(manifest_temp, &manifest_path)
            .map_err(|e| format!("提交新角色包清单失败：{e}"))?;
        new_manifest_installed = true;

        validate_pack_files(pack_path, manifest)?;
        validate_live2d_model_tree(&destination, model3_name)?;
        read_manifest_with_avatar(pack_path)
    })();

    match install_result {
        Ok(installed) => {
            if old_live2d_moved {
                let _ = remove_any_path(&live2d_backup);
            }
            if manifest_backup_created {
                let _ = remove_any_path(&manifest_backup);
            }
            Ok(installed)
        }
        Err(error) => {
            let mut rollback_errors = Vec::new();
            if manifest_backup_created && new_manifest_installed {
                if let Err(rollback) = fs::rename(&manifest_backup, &manifest_path) {
                    rollback_errors.push(format!(
                        "恢复旧清单失败：{rollback}；备份保留在 {}",
                        manifest_backup.display()
                    ));
                }
            } else if manifest_backup_created {
                let _ = remove_any_path(&manifest_backup);
            } else {
                // `fs::copy` may have created a partial destination before
                // returning an error. It is never a trusted recovery copy.
                let _ = remove_any_path(&manifest_backup);
            }
            if new_live2d_installed {
                if let Err(rollback) = remove_any_path(&destination) {
                    rollback_errors.push(format!("移除新模型失败：{rollback}"));
                }
            }
            if old_live2d_moved {
                if let Err(rollback) = fs::rename(&live2d_backup, &destination) {
                    rollback_errors.push(format!(
                        "恢复旧模型失败：{rollback}；备份保留在 {}",
                        live2d_backup.display()
                    ));
                }
            }
            let _ = remove_any_path(staging);
            let _ = remove_any_path(manifest_temp);
            if rollback_errors.is_empty() {
                Err(format!("Live2D 导入失败，旧模型已恢复：{error}"))
            } else {
                Err(format!(
                    "Live2D 导入失败：{error}；回滚异常：{}",
                    rollback_errors.join("；")
                ))
            }
        }
    }
}

fn path_exists_no_follow(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn remove_any_path(path: &Path) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("读取 `{}` 失败：{error}", path.display())),
    };
    let file_type = metadata.file_type();
    let result = if file_type.is_symlink() {
        fs::remove_file(path).or_else(|_| fs::remove_dir(path))
    } else if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    result.map_err(|error| format!("删除 `{}` 失败：{error}", path.display()))
}

fn live2d_mime(path: &Path) -> String {
    match path
        .extension()
        .and_then(|v| v.to_str())
        .map(|v| v.to_ascii_lowercase())
        .as_deref()
    {
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("wav") => "audio/wav",
        Some("mp3") => "audio/mpeg",
        Some("m4a") => "audio/mp4",
        Some("ogg") => "audio/ogg",
        _ => "application/octet-stream",
    }
    .to_string()
}

/// 移除角色包的 Live2D 模型：删 <pack>/live2d/ 目录并清空 manifest.live2d。
pub fn remove_live2d(packs_dir: &Path, pack_id: &str) -> Result<PackManifest, String> {
    let _mutation_guard = LIVE2D_MUTATION_LOCK
        .lock()
        .map_err(|_| "Live2D 修改锁已损坏，已拒绝修改".to_string())?;
    let dir = resolve_pack_dir(packs_dir, pack_id)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct TestRoot(PathBuf);

    impl TestRoot {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "demiurge_live2d_{label}_{}",
                crate::store::new_session_id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_json(path: &Path, value: &Value) {
        fs::write(
            path,
            format!("{}\n", serde_json::to_string_pretty(value).unwrap()),
        )
        .unwrap();
    }

    fn create_pack(root: &Path) -> (PathBuf, PathBuf) {
        let packs = root.join("packs");
        let pack = packs.join("test-pack");
        let live2d = pack.join("live2d");
        fs::create_dir_all(&live2d).unwrap();
        fs::write(pack.join("persona.md"), "# Persona\n").unwrap();
        fs::write(live2d.join("old.moc3"), b"old-model").unwrap();
        write_json(
            &live2d.join("old.model3.json"),
            &json!({"FileReferences": {"Moc": "old.moc3", "Textures": []}}),
        );
        write_json(
            &pack.join("manifest.json"),
            &json!({
                "schema_version": "2.0",
                "id": "test-pack",
                "name": "Test Pack",
                "persona": "persona.md",
                "live2d": "live2d/old.model3.json"
            }),
        );
        (packs, pack)
    }

    fn assert_no_import_artifacts(pack: &Path) {
        let artifacts = fs::read_dir(pack)
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| name.starts_with(".live2d-"))
            .collect::<Vec<_>>();
        assert!(
            artifacts.is_empty(),
            "leftover import artifacts: {artifacts:?}"
        );
    }

    #[test]
    fn portable_reference_validation_rejects_absolute_prefix_and_parent_paths() {
        for path in [
            "/etc/passwd",
            "\\\\server\\share\\model.moc3",
            "C:\\models\\model.moc3",
            "C:model.moc3",
            "../model.moc3",
            "textures/../../model.moc3",
            "textures/.. /model.moc3",
            "./model.moc3",
            "textures//model.png",
            "textures./model.png",
            "NUL.moc3",
            "COM1/model.moc3",
        ] {
            assert!(
                normalize_live2d_reference(path).is_err(),
                "unsafe reference must be rejected: {path}"
            );
        }
        assert_eq!(
            normalize_live2d_reference("textures\\model.png").unwrap(),
            "textures/model.png"
        );
    }

    #[test]
    fn invalid_import_preserves_old_model_manifest_and_external_file() {
        let root = TestRoot::new("preserve_old");
        let (packs, pack) = create_pack(root.path());
        let old_manifest = fs::read(pack.join("manifest.json")).unwrap();
        let old_model = fs::read(pack.join("live2d/old.moc3")).unwrap();
        let outside = root.path().join("外部模型.moc3");
        fs::write(&outside, b"outside-secret").unwrap();

        for (label, reference) in [
            ("absolute", outside.to_string_lossy().into_owned()),
            ("parent", "../outside.moc3".to_string()),
            ("windows", "C:\\outside\\模型.moc3".to_string()),
        ] {
            let source = root.path().join(format!("source-{label}"));
            fs::create_dir_all(&source).unwrap();
            write_json(
                &source.join("candidate.model3.json"),
                &json!({"FileReferences": {"Moc": reference, "Textures": []}}),
            );
            let error =
                import_live2d_folder(&packs, "test-pack", source.to_string_lossy().as_ref())
                    .unwrap_err();
            assert!(error.contains("Live2D"));
            assert_eq!(fs::read(pack.join("manifest.json")).unwrap(), old_manifest);
            assert_eq!(fs::read(pack.join("live2d/old.moc3")).unwrap(), old_model);
            assert_eq!(fs::read(&outside).unwrap(), b"outside-secret");
            assert_no_import_artifacts(&pack);
        }
    }

    #[test]
    fn imports_non_ascii_references_and_rewrites_every_resource_kind() {
        let root = TestRoot::new("non_ascii");
        let (packs, pack) = create_pack(root.path());
        let source = root.path().join("模型源");
        for directory in ["纹理", "表情", "动作", "声音"] {
            fs::create_dir_all(source.join(directory)).unwrap();
        }
        fs::write(source.join("模型.moc3"), b"new-model").unwrap();
        fs::write(source.join("物理.physics3.json"), b"{}").unwrap();
        fs::write(source.join("纹理/贴图.png"), b"png").unwrap();
        fs::write(source.join("表情/微笑.exp3.json"), b"{}").unwrap();
        fs::write(source.join("动作/待机.motion3.json"), b"{}").unwrap();
        fs::write(source.join("声音/问候.mp3"), b"mp3").unwrap();
        write_json(
            &source.join("角色.model3.json"),
            &json!({
                "FileReferences": {
                    "Moc": "模型.moc3",
                    "Textures": ["纹理/贴图.png"],
                    "Physics": "物理.physics3.json",
                    "Expressions": [{"Name": "smile", "File": "表情/微笑.exp3.json"}],
                    "Motions": {
                        "Idle": [{"File": "动作/待机.motion3.json", "Sound": "声音/问候.mp3"}]
                    }
                }
            }),
        );

        let manifest =
            import_live2d_folder(&packs, "test-pack", source.to_string_lossy().as_ref()).unwrap();
        assert_eq!(manifest.live2d.as_deref(), Some("live2d/model.model3.json"));
        let model_text = fs::read_to_string(pack.join("live2d/model.model3.json")).unwrap();
        let model_json: Value = serde_json::from_str(&model_text).unwrap();
        let references = collect_live2d_refs_checked(&model_json).unwrap();
        assert_eq!(references.len(), 6);
        assert!(references.iter().all(|reference| reference.is_ascii()));
        assert!(references
            .iter()
            .all(|reference| reference.starts_with("normalized_assets/")));
        validate_live2d_model_tree(&pack.join("live2d"), "model.model3.json").unwrap();
        let bundle = live2d_bundle(&packs, "test-pack").unwrap();
        assert_eq!(bundle.assets.len(), 6);
        assert_eq!(fs::read(source.join("模型.moc3")).unwrap(), b"new-model");
        assert!(!pack.join("live2d/old.moc3").exists());
        assert_no_import_artifacts(&pack);
    }

    #[test]
    fn commit_failure_rolls_back_model_and_manifest() {
        let root = TestRoot::new("commit_rollback");
        let (_packs, pack) = create_pack(root.path());
        let old_manifest = fs::read(pack.join("manifest.json")).unwrap();
        let old_model = fs::read(pack.join("live2d/old.moc3")).unwrap();
        let staging = pack.join("manual-candidate");
        fs::create_dir(&staging).unwrap();
        fs::write(staging.join("new.moc3"), b"new-model").unwrap();
        write_json(
            &staging.join("model.model3.json"),
            &json!({"FileReferences": {"Moc": "new.moc3", "Textures": []}}),
        );
        let mut manifest = read_manifest_no_avatar(&pack).unwrap();
        manifest.live2d = Some("live2d/model.model3.json".to_string());
        let missing_manifest_temp = pack.join("missing-manifest.tmp");

        let error = install_prepared_live2d(
            &pack,
            &staging,
            &missing_manifest_temp,
            &manifest,
            "model.model3.json",
        )
        .unwrap_err();
        assert!(error.contains("旧模型已恢复"));
        assert_eq!(fs::read(pack.join("manifest.json")).unwrap(), old_manifest);
        assert_eq!(fs::read(pack.join("live2d/old.moc3")).unwrap(), old_model);
        assert!(!pack.join("live2d/new.moc3").exists());
        assert_no_import_artifacts(&pack);
    }

    #[test]
    fn rejects_symlinked_source_and_model_reference_escape() {
        let root = TestRoot::new("symlink_escape");
        let (packs, pack) = create_pack(root.path());
        let outside = root.path().join("outside.moc3");
        fs::write(&outside, b"outside").unwrap();
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        let source_link = source.join("linked.moc3");
        if let Err(error) = create_file_symlink(&outside, &source_link) {
            eprintln!("skipping symlink test on this platform: {error}");
            return;
        }
        write_json(
            &source.join("linked.model3.json"),
            &json!({"FileReferences": {"Moc": "linked.moc3", "Textures": []}}),
        );
        assert!(
            import_live2d_folder(&packs, "test-pack", source.to_string_lossy().as_ref(),).is_err()
        );
        assert_eq!(fs::read(&outside).unwrap(), b"outside");
        assert!(pack.join("live2d/old.moc3").exists());

        let model_root = pack.join("manual-model");
        fs::create_dir(&model_root).unwrap();
        let bundle_link = model_root.join("escape.moc3");
        create_file_symlink(&outside, &bundle_link).unwrap();
        assert!(resolve_model_relative_file(&model_root, &model_root, "escape.moc3").is_err());
        assert_no_import_artifacts(&pack);
    }

    #[cfg(unix)]
    fn create_file_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }

    #[cfg(windows)]
    fn create_file_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
        std::os::windows::fs::symlink_file(target, link)
    }
}
