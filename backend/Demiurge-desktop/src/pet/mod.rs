//! 声明式桌宠包的校验、事务安装和资源解析。
//!
//! 桌宠包不允许执行自带代码；所有资源引用必须留在安装目录内。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use zip::ZipArchive;

const MAX_PACKAGE_BYTES: u64 = 50 * 1024 * 1024;
const MAX_PACKAGE_FILES: usize = 200;
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const BUNDLED_MARKER: &str = ".demiurge-bundled-version";

struct BundledPet {
    id: &'static str,
    file_name: &'static str,
    version: &'static str,
    legacy_versions: &'static [&'static str],
    bytes: &'static [u8],
}

const BUNDLED_PETS: &[BundledPet] = &[BundledPet {
    id: "nailong",
    file_name: "nailong.demipet",
    version: "1.0.1",
    legacy_versions: &["1.0.0"],
    bytes: include_bytes!("../../../../resources/pets/nailong.demipet"),
}];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    pub renderer: SpriteSheetRenderer,
    pub actions: BTreeMap<String, PetAction>,
    pub bindings: BTreeMap<String, PetBinding>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sounds: BTreeMap<String, PetSound>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpriteSheetRenderer {
    #[serde(rename = "type")]
    pub renderer_type: String,
    pub sheet: String,
    pub frame_width: u32,
    pub frame_height: u32,
    pub columns: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetAction {
    pub frames: PetFrameRange,
    pub frame_duration_ms: u64,
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_to: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetFrameRange {
    pub row: u32,
    pub start: u32,
    pub count: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PetBinding {
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PetSound {
    pub source: String,
    #[serde(default = "default_volume")]
    pub volume: f32,
    #[serde(default, rename = "loop")]
    pub loop_audio: bool,
}

fn default_volume() -> f32 {
    1.0
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPet {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub thumbnail_path: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedPet {
    pub manifest: PetManifest,
    pub sheet_path: String,
    pub sound_paths: BTreeMap<String, String>,
}

pub fn ensure_bundled(pets_dir: &Path) -> Result<(), String> {
    for bundled in BUNDLED_PETS {
        let installed_dir = pets_dir.join(bundled.id);
        let marker = installed_dir.join(BUNDLED_MARKER);
        let marker_version = fs::symlink_metadata(&marker)
            .ok()
            .filter(|metadata| metadata.file_type().is_file() && !metadata.file_type().is_symlink())
            .and_then(|_| fs::read_to_string(&marker).ok());
        let legacy_bundled = marker_version.is_none()
            && read_manifest(&installed_dir)
                .map(|manifest| bundled.legacy_versions.contains(&manifest.version.as_str()))
                .unwrap_or(false);
        let should_install = !installed_dir.exists()
            || marker_version.as_deref().map(str::trim) != Some(bundled.version)
                && (marker_version.is_some() || legacy_bundled);
        if should_install {
            import(pets_dir, bundled.file_name, bundled.bytes.to_vec())?;
            fs::write(installed_dir.join(BUNDLED_MARKER), bundled.version)
                .map_err(|e| format!("记录内置桌宠版本失败：{e}"))?;
        }
    }
    Ok(())
}

pub fn list(pets_dir: &Path) -> Vec<InstalledPet> {
    let Ok(entries) = fs::read_dir(pets_dir) else {
        return Vec::new();
    };
    let mut pets = entries
        .flatten()
        .filter_map(|entry| {
            let id = entry.file_name().to_string_lossy().to_string();
            let dir = resolve_dir(pets_dir, &id).ok()?;
            let manifest = read_manifest(&dir).ok()?;
            validate_installed(&dir, &manifest).ok()?;
            Some(InstalledPet {
                id: manifest.id,
                name: manifest.name,
                version: manifest.version,
                description: manifest.description,
                thumbnail_path: manifest
                    .thumbnail
                    .as_deref()
                    .and_then(|path| resolve_file(&dir, path, "thumbnail").ok())
                    .map(|path| path.to_string_lossy().to_string()),
            })
        })
        .collect::<Vec<_>>();
    pets.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    pets
}

pub fn import(pets_dir: &Path, file_name: &str, bytes: Vec<u8>) -> Result<InstalledPet, String> {
    if bytes.is_empty() {
        return Err("桌宠包为空".to_string());
    }
    if bytes.len() as u64 > MAX_PACKAGE_BYTES {
        return Err(format!(
            "桌宠包过大：最大允许 {} MB",
            MAX_PACKAGE_BYTES / 1024 / 1024
        ));
    }
    let lower = file_name.to_ascii_lowercase();
    if !lower.ends_with(".demipet") && !lower.ends_with(".zip") {
        return Err("桌宠导入只支持 .demipet 文件".to_string());
    }

    fs::create_dir_all(pets_dir).map_err(|e| format!("创建桌宠目录失败：{e}"))?;
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("读取桌宠包失败：{e}"))?;
    let manifest_path = find_manifest(&mut archive)?;
    let prefix = manifest_path
        .strip_suffix("pet.json")
        .unwrap_or("")
        .to_string();
    let manifest_text = read_manifest_from_zip(&mut archive, &manifest_path)?;
    let manifest: PetManifest =
        serde_json::from_str(&manifest_text).map_err(|e| format!("解析 pet.json 失败：{e}"))?;
    validate_manifest(&manifest)?;

    let root = canonical_root(pets_dir)?;
    let staging = root.join(format!(
        ".import-{}-{}",
        manifest.id,
        crate::store::new_session_id()
    ));
    if fs::symlink_metadata(&staging).is_ok() {
        return Err("桌宠临时导入目录已存在，请重试".to_string());
    }
    fs::create_dir(&staging).map_err(|e| format!("创建桌宠临时目录失败：{e}"))?;

    let result = extract(&mut archive, &prefix, &staging)
        .and_then(|_| validate_installed(&staging, &manifest))
        .and_then(|_| install_staging(&root, &staging, &manifest.id))
        .and_then(|dir| resolved_summary(&dir));
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

pub fn remove(pets_dir: &Path, id: &str) -> Result<(), String> {
    let dir = resolve_dir(pets_dir, id)?;
    fs::remove_dir_all(&dir).map_err(|e| format!("删除桌宠失败：{e}"))
}

pub fn resolve(pets_dir: &Path, id: &str) -> Result<ResolvedPet, String> {
    let dir = resolve_dir(pets_dir, id)?;
    let manifest = read_manifest(&dir)?;
    validate_installed(&dir, &manifest)?;
    let sheet = resolve_file(&dir, &manifest.renderer.sheet, "renderer.sheet")?;
    let mut sound_paths = BTreeMap::new();
    for (name, sound) in &manifest.sounds {
        let path = resolve_file(&dir, &sound.source, "sound.source")?;
        sound_paths.insert(name.clone(), path.to_string_lossy().to_string());
    }
    Ok(ResolvedPet {
        manifest,
        sheet_path: sheet.to_string_lossy().to_string(),
        sound_paths,
    })
}

pub fn resolve_dir(pets_dir: &Path, id: &str) -> Result<PathBuf, String> {
    validate_id(id)?;
    let root = canonical_root(pets_dir)?;
    let candidate = root.join(id);
    let metadata = fs::symlink_metadata(&candidate).map_err(|_| format!("桌宠不存在：{id}"))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(format!("桌宠目录无效：{id}"));
    }
    let canonical = candidate
        .canonicalize()
        .map_err(|e| format!("解析桌宠目录失败：{e}"))?;
    if canonical.parent() != Some(root.as_path()) {
        return Err("桌宠目录越过可信根目录".to_string());
    }
    Ok(canonical)
}

fn canonical_root(pets_dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(pets_dir).map_err(|e| format!("创建桌宠目录失败：{e}"))?;
    pets_dir
        .canonicalize()
        .map_err(|e| format!("解析桌宠根目录失败：{e}"))
}

fn validate_manifest(manifest: &PetManifest) -> Result<(), String> {
    if manifest.schema_version != 1 {
        return Err(format!("不支持的桌宠包版本：{}", manifest.schema_version));
    }
    validate_id(&manifest.id)?;
    if manifest.name.trim().is_empty() || manifest.version.trim().is_empty() {
        return Err("桌宠名称和版本不能为空".to_string());
    }
    if manifest.renderer.renderer_type != "sprite-sheet" {
        return Err(format!(
            "不支持的桌宠渲染类型：{}",
            manifest.renderer.renderer_type
        ));
    }
    if manifest.renderer.frame_width == 0
        || manifest.renderer.frame_height == 0
        || manifest.renderer.columns == 0
    {
        return Err("桌宠帧尺寸和列数必须大于 0".to_string());
    }
    validate_relative(&manifest.renderer.sheet, "renderer.sheet")?;
    if let Some(path) = manifest.thumbnail.as_deref() {
        validate_relative(path, "thumbnail")?;
    }
    if manifest.actions.is_empty() {
        return Err("桌宠至少需要一个动作".to_string());
    }
    for (name, action) in &manifest.actions {
        if name.trim().is_empty() || action.frames.count == 0 || action.frame_duration_ms == 0 {
            return Err(format!("桌宠动作无效：{name}"));
        }
        if !matches!(action.mode.as_str(), "loop" | "once" | "hold") {
            return Err(format!("桌宠动作模式无效：{name}"));
        }
        if let Some(target) = action.return_to.as_deref() {
            if target != "previous" && target != "idle" && !manifest.actions.contains_key(target) {
                return Err(format!("桌宠动作回退目标不存在：{target}"));
            }
        }
    }
    let idle = manifest
        .bindings
        .get("system.idle")
        .ok_or_else(|| "桌宠缺少 system.idle 绑定".to_string())?;
    if !manifest.actions.contains_key(&idle.action) {
        return Err("system.idle 绑定的动作不存在".to_string());
    }
    for (event, binding) in &manifest.bindings {
        if !manifest.actions.contains_key(&binding.action) {
            return Err(format!("事件 {event} 绑定的动作不存在：{}", binding.action));
        }
        if let Some(sound) = binding.sound.as_deref() {
            if !manifest.sounds.contains_key(sound) {
                return Err(format!("事件 {event} 绑定的声音不存在：{sound}"));
            }
        }
    }
    for (name, sound) in &manifest.sounds {
        validate_relative(&sound.source, "sound.source")?;
        if !(0.0..=1.0).contains(&sound.volume) {
            return Err(format!("声音音量必须在 0 到 1 之间：{name}"));
        }
    }
    Ok(())
}

fn validate_installed(dir: &Path, manifest: &PetManifest) -> Result<(), String> {
    validate_manifest(manifest)?;
    let sheet = resolve_file(dir, &manifest.renderer.sheet, "renderer.sheet")?;
    let (width, height) =
        image::image_dimensions(&sheet).map_err(|e| format!("读取桌宠精灵图失败：{e}"))?;
    let renderer = &manifest.renderer;
    if renderer.frame_width.saturating_mul(renderer.columns) > width {
        return Err("桌宠精灵图宽度不足以容纳声明的列数".to_string());
    }
    for (name, action) in &manifest.actions {
        if action.frames.start.saturating_add(action.frames.count) > renderer.columns {
            return Err(format!("桌宠动作帧越过精灵图列范围：{name}"));
        }
        if action
            .frames
            .row
            .saturating_add(1)
            .saturating_mul(renderer.frame_height)
            > height
        {
            return Err(format!("桌宠动作帧越过精灵图行范围：{name}"));
        }
    }
    if let Some(path) = manifest.thumbnail.as_deref() {
        let _ = resolve_file(dir, path, "thumbnail")?;
    }
    for sound in manifest.sounds.values() {
        let path = resolve_file(dir, &sound.source, "sound.source")?;
        if fs::metadata(path).map(|meta| meta.len()).unwrap_or(0) == 0 {
            return Err("桌宠声音文件为空".to_string());
        }
    }
    Ok(())
}

fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 128
        || id.trim() != id
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("桌宠 id 只能包含 ASCII 字母、数字、- 和 _".to_string());
    }
    Ok(())
}

fn validate_relative(path: &str, label: &str) -> Result<(), String> {
    if path.trim().is_empty() || Path::new(path).is_absolute() || path.contains('\\') {
        return Err(format!("{label} 必须是包内相对路径"));
    }
    if Path::new(path)
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!("{label} 包含非法路径组件"));
    }
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(
        extension.as_str(),
        "json" | "png" | "jpg" | "jpeg" | "webp" | "wav" | "mp3" | "ogg"
    ) {
        return Err(format!("{label} 使用了不允许的文件类型"));
    }
    Ok(())
}

fn resolve_file(dir: &Path, relative: &str, label: &str) -> Result<PathBuf, String> {
    validate_relative(relative, label)?;
    let root = dir
        .canonicalize()
        .map_err(|e| format!("解析桌宠目录失败：{e}"))?;
    let path = root.join(relative);
    let metadata = fs::symlink_metadata(&path).map_err(|_| format!("桌宠缺少资源：{relative}"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(format!("桌宠资源不是普通文件：{relative}"));
    }
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("解析桌宠资源失败：{e}"))?;
    if !canonical.starts_with(&root) {
        return Err(format!("桌宠资源越过安装目录：{relative}"));
    }
    Ok(canonical)
}

fn read_manifest(dir: &Path) -> Result<PetManifest, String> {
    let text =
        fs::read_to_string(dir.join("pet.json")).map_err(|e| format!("读取 pet.json 失败：{e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("解析 pet.json 失败：{e}"))
}

fn resolved_summary(dir: &Path) -> Result<InstalledPet, String> {
    let manifest = read_manifest(dir)?;
    validate_installed(dir, &manifest)?;
    Ok(InstalledPet {
        id: manifest.id,
        name: manifest.name,
        version: manifest.version,
        description: manifest.description,
        thumbnail_path: manifest
            .thumbnail
            .as_deref()
            .and_then(|path| resolve_file(dir, path, "thumbnail").ok())
            .map(|path| path.to_string_lossy().to_string()),
    })
}

fn find_manifest<R: Read + std::io::Seek>(archive: &mut ZipArchive<R>) -> Result<String, String> {
    let mut manifests = Vec::new();
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|e| format!("读取桌宠包条目失败：{e}"))?;
        let name = normalize_zip_name(file.name())?;
        if name == "pet.json" || name.ends_with("/pet.json") {
            manifests.push(name);
        }
    }
    match manifests.len() {
        0 => Err("桌宠包中缺少 pet.json".to_string()),
        1 => Ok(manifests.remove(0)),
        _ => Err("桌宠包中只能包含一个 pet.json".to_string()),
    }
}

fn read_manifest_from_zip<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Result<String, String> {
    let mut file = archive
        .by_name(name)
        .map_err(|e| format!("读取 pet.json 失败：{e}"))?;
    if file.size() > MAX_MANIFEST_BYTES {
        return Err("pet.json 过大".to_string());
    }
    let mut text = String::new();
    file.read_to_string(&mut text)
        .map_err(|e| format!("读取 pet.json 文本失败：{e}"))?;
    Ok(text)
}

fn extract<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    prefix: &str,
    destination: &Path,
) -> Result<(), String> {
    let mut total = 0u64;
    let mut count = 0usize;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|e| format!("读取桌宠包条目失败：{e}"))?;
        if file.is_dir() {
            continue;
        }
        let name = normalize_zip_name(file.name())?;
        if !name.starts_with(prefix) {
            continue;
        }
        let relative = name[prefix.len()..].trim_start_matches('/');
        if relative.is_empty() {
            continue;
        }
        validate_relative(relative, "桌宠包条目")?;
        count += 1;
        total = total.saturating_add(file.size());
        if count > MAX_PACKAGE_FILES || total > MAX_PACKAGE_BYTES {
            return Err("桌宠包解压后超出资源限制".to_string());
        }
        let output = destination.join(relative);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建桌宠资源目录失败：{e}"))?;
        }
        let mut writer = fs::File::create(&output).map_err(|e| format!("创建桌宠资源失败：{e}"))?;
        std::io::copy(&mut file, &mut writer).map_err(|e| format!("写入桌宠资源失败：{e}"))?;
    }
    Ok(())
}

fn normalize_zip_name(name: &str) -> Result<String, String> {
    let normalized = name.replace('\\', "/").trim_end_matches('/').to_string();
    if normalized.is_empty() {
        return Ok(normalized);
    }
    for component in normalized.split('/') {
        if component.is_empty() || component == "." || component == ".." || component.contains(':')
        {
            return Err("桌宠包包含非法路径".to_string());
        }
    }
    Ok(normalized)
}

fn install_staging(root: &Path, staging: &Path, id: &str) -> Result<PathBuf, String> {
    let destination = root.join(id);
    let backup = root.join(format!(".backup-{id}-{}", crate::store::new_session_id()));
    let had_existing = fs::symlink_metadata(&destination).is_ok();
    if had_existing {
        let existing = resolve_dir(root, id)?;
        fs::rename(&existing, &backup).map_err(|e| format!("备份旧桌宠失败：{e}"))?;
    }
    if let Err(error) = fs::rename(staging, &destination) {
        if had_existing {
            let _ = fs::rename(&backup, &destination);
        }
        return Err(format!("安装桌宠失败：{error}"));
    }
    if had_existing {
        let _ = fs::remove_dir_all(&backup);
    }
    resolve_dir(root, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn manifest(sheet: &str) -> PetManifest {
        serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": "demo",
            "name": "Demo",
            "version": "1.0.0",
            "renderer": { "type": "sprite-sheet", "sheet": sheet, "frameWidth": 16, "frameHeight": 16, "columns": 2 },
            "actions": { "idle": { "frames": { "row": 0, "start": 0, "count": 2 }, "frameDurationMs": 100, "mode": "loop" } },
            "bindings": { "system.idle": { "action": "idle" } }
        })).unwrap()
    }

    #[test]
    fn rejects_missing_idle_binding_and_path_traversal() {
        let mut missing = manifest("sheet.png");
        missing.bindings.clear();
        assert!(validate_manifest(&missing).is_err());
        assert!(validate_manifest(&manifest("../sheet.png")).is_err());
    }

    #[test]
    fn imports_and_resolves_a_sprite_pet() {
        let root = std::env::temp_dir().join(format!(
            "demiurge_pet_test_{}",
            crate::store::new_session_id()
        ));
        fs::create_dir_all(&root).unwrap();
        let mut png = Vec::new();
        image::DynamicImage::new_rgba8(32, 16)
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let manifest_text = serde_json::to_vec(&manifest("sheet.png")).unwrap();
        let cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        writer
            .start_file("pet.json", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&manifest_text).unwrap();
        writer
            .start_file("sheet.png", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&png).unwrap();
        let bytes = writer.finish().unwrap().into_inner();

        let installed = import(&root, "demo.demipet", bytes).unwrap();
        assert_eq!(installed.id, "demo");
        let resolved = resolve(&root, "demo").unwrap();
        assert!(resolved.sheet_path.ends_with("sheet.png"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn installs_and_resolves_the_bundled_pet_registry() {
        let root = std::env::temp_dir().join(format!(
            "demiurge_bundled_pet_test_{}",
            crate::store::new_session_id()
        ));
        fs::create_dir_all(&root).unwrap();

        ensure_bundled(&root).unwrap();
        let installed = list(&root);
        assert!(installed.iter().any(|pet| pet.id == "nailong"));
        let resolved = resolve(&root, "nailong").unwrap();
        assert_eq!(resolved.manifest.version, "1.0.1");
        assert_eq!(
            resolved.manifest.bindings["agent.working"].action,
            "running-suit-coding"
        );
        assert_eq!(
            resolved.manifest.bindings["interaction.hover"].action,
            "hover-laugh"
        );
        assert!(resolved.manifest.sounds["laugh-loop"].loop_audio);
        assert!(resolved.sound_paths.contains_key("laugh"));
        assert_eq!(
            fs::read_to_string(root.join("nailong").join(BUNDLED_MARKER)).unwrap(),
            "1.0.1"
        );

        let _ = fs::remove_dir_all(root);
    }
}
