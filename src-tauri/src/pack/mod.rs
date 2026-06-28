//! 角色包加载 + 清单。MVP 文本版清单，格式预留可成长字段（Live2D / TTS / 表情等）。
use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

const MAX_IMPORT_FILES: usize = 100;
const MAX_IMPORT_BYTES: u64 = 25 * 1024 * 1024;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PackManifest {
    pub id: String,
    pub name: String,
    /// persona 文件名（相对包目录）
    pub persona: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(
        default,
        rename = "avatarDataUrl",
        skip_serializing_if = "Option::is_none"
    )]
    pub avatar_data_url: Option<String>,
}

pub struct Pack {
    #[allow(dead_code)]
    pub manifest: PackManifest,
    pub persona_text: String,
}

/// 内置的通用人格（通用、不绑定任何特定角色）。首启动时落地为 packs/default。
const DEFAULT_MANIFEST: &str = r#"{
  "id": "default",
  "name": "Demiurge",
  "persona": "persona.md"
}
"#;
const DEFAULT_PERSONA: &str = r#"你是用户的桌面伴侣。性格温和、好奇、乐于助人。
你会用自然、口语化的方式陪用户聊天，也能在需要时调用工具帮用户查信息、整理文件、打开网页等。
说话简洁、不绕弯、不过度客套。遇到不确定的事会如实说不知道，而不是编造。
"#;

/// 确保 packs 目录存在，且至少有一个可用的 default 包。
pub fn ensure_default(packs_dir: &Path) -> Result<(), String> {
    let dir = packs_dir.join("default");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let manifest = dir.join("manifest.json");
    if !manifest.exists() {
        fs::write(&manifest, DEFAULT_MANIFEST).map_err(|e| e.to_string())?;
    }
    let persona = dir.join("persona.md");
    if !persona.exists() {
        fs::write(&persona, DEFAULT_PERSONA).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 列出 packs 目录下所有含 manifest.json 的子目录。
pub fn list_packs(packs_dir: &Path) -> Vec<PackManifest> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(packs_dir) else {
        return out;
    };
    for e in entries.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        if let Ok(m) = read_manifest_with_avatar(&p) {
            out.push(m);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    out
}

fn pack_dir(packs_dir: &Path, id: &str) -> PathBuf {
    packs_dir.join(id)
}

/// 按 id 加载角色包（读 manifest + persona 正文）。
pub fn load_pack(packs_dir: &Path, id: &str) -> Result<Pack, String> {
    let dir = pack_dir(packs_dir, id);
    let manifest = read_manifest_with_avatar(&dir)?;
    let persona_path = resolve_pack_file(&dir, &manifest.persona, "persona")?;
    let persona_text =
        fs::read_to_string(&persona_path).map_err(|e| format!("读取 persona 失败：{e}"))?;
    Ok(Pack {
        manifest,
        persona_text,
    })
}

pub fn import_zip(
    packs_dir: &Path,
    file_name: &str,
    bytes: Vec<u8>,
) -> Result<PackManifest, String> {
    if bytes.is_empty() {
        return Err("角色包 zip 为空".to_string());
    }
    if bytes.len() as u64 > MAX_IMPORT_BYTES {
        return Err(format!(
            "角色包 zip 过大：最大允许 {} MB",
            MAX_IMPORT_BYTES / 1024 / 1024
        ));
    }
    if !file_name.to_ascii_lowercase().ends_with(".zip") {
        return Err("角色包导入只支持 .zip 文件".to_string());
    }

    fs::create_dir_all(packs_dir).map_err(|e| format!("创建 packs 目录失败：{e}"))?;
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("读取 zip 失败：{e}"))?;
    let manifest_path = find_manifest_entry(&mut archive)?;
    let prefix = manifest_path
        .strip_suffix("manifest.json")
        .unwrap_or("")
        .to_string();
    let manifest_text = read_zip_text(&mut archive, &manifest_path)?;
    let manifest = parse_manifest(&manifest_text)?;
    validate_manifest_paths(&manifest)?;

    let dest = packs_dir.join(&manifest.id);
    if dest.exists() {
        return Err(format!("角色包 `{}` 已存在，导入已取消。", manifest.id));
    }
    let temp = packs_dir.join(format!(
        ".import-{}-{}",
        manifest.id,
        crate::store::new_session_id()
    ));
    if temp.exists() {
        fs::remove_dir_all(&temp).map_err(|e| format!("清理临时导入目录失败：{e}"))?;
    }
    fs::create_dir_all(&temp).map_err(|e| format!("创建临时导入目录失败：{e}"))?;

    let result = extract_archive(&mut archive, &prefix, &temp)
        .and_then(|_| validate_extracted_pack(&temp, &manifest))
        .and_then(|_| {
            fs::rename(&temp, &dest).map_err(|e| format!("保存角色包失败：{e}"))?;
            read_manifest_with_avatar(&dest)
        });
    if result.is_err() {
        let _ = fs::remove_dir_all(&temp);
    }
    result
}

fn read_manifest_with_avatar(dir: &Path) -> Result<PackManifest, String> {
    let mf = dir.join("manifest.json");
    let txt = fs::read_to_string(&mf).map_err(|e| format!("读取角色包清单失败：{e}"))?;
    let mut manifest = parse_manifest(&txt)?;
    validate_manifest_paths(&manifest)?;
    if let Some(avatar) = manifest.avatar.as_deref() {
        let avatar_path = resolve_pack_file(dir, avatar, "avatar")?;
        if avatar_path.exists() {
            manifest.avatar_data_url = Some(avatar_data_url(&avatar_path)?);
        }
    }
    Ok(manifest)
}

fn parse_manifest(text: &str) -> Result<PackManifest, String> {
    let manifest: PackManifest =
        serde_json::from_str(text).map_err(|e| format!("解析角色包清单失败：{e}"))?;
    validate_manifest_identity(&manifest)?;
    Ok(manifest)
}

fn validate_manifest_identity(manifest: &PackManifest) -> Result<(), String> {
    let id = manifest.id.trim();
    if id.is_empty() || id != manifest.id {
        return Err("manifest.id 不能为空或包含首尾空白".to_string());
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err("manifest.id 只能包含 ASCII 字母、数字、- 和 _".to_string());
    }
    if manifest.name.trim().is_empty() {
        return Err("manifest.name 不能为空".to_string());
    }
    Ok(())
}

fn validate_manifest_paths(manifest: &PackManifest) -> Result<(), String> {
    validate_relative_file(&manifest.persona, "persona")?;
    if let Some(avatar) = manifest.avatar.as_deref() {
        validate_relative_file(avatar, "avatar")?;
        if avatar_mime(avatar).is_none() {
            return Err("manifest.avatar 只支持 png、jpg、jpeg、webp 或 gif".to_string());
        }
    }
    Ok(())
}

fn validate_relative_file(value: &str, field: &str) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed != value {
        return Err(format!("manifest.{field} 不能为空或包含首尾空白"));
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(format!("manifest.{field} 必须是相对路径"));
    }
    for comp in path.components() {
        match comp {
            std::path::Component::Normal(_) | std::path::Component::CurDir => {}
            _ => return Err(format!("manifest.{field} 包含非法路径组件")),
        }
    }
    Ok(())
}

fn resolve_pack_file(dir: &Path, rel: &str, field: &str) -> Result<PathBuf, String> {
    validate_relative_file(rel, field)?;
    Ok(dir.join(rel))
}

fn avatar_data_url(path: &Path) -> Result<String, String> {
    let mime = avatar_mime(&path.to_string_lossy())
        .ok_or_else(|| "manifest.avatar 只支持 png、jpg、jpeg、webp 或 gif".to_string())?;
    let bytes = fs::read(path).map_err(|e| format!("读取 avatar 失败：{e}"))?;
    if bytes.is_empty() {
        return Err("avatar 文件为空".to_string());
    }
    Ok(format!(
        "data:{mime};base64,{}",
        general_purpose::STANDARD.encode(bytes)
    ))
}

fn avatar_mime(path: &str) -> Option<&'static str> {
    match Path::new(path)
        .extension()
        .and_then(|v| v.to_str())
        .map(|v| v.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => Some("image/png"),
        Some("jpg") | Some("jpeg") => Some("image/jpeg"),
        Some("webp") => Some("image/webp"),
        Some("gif") => Some("image/gif"),
        _ => None,
    }
}

fn find_manifest_entry<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<String, String> {
    let mut manifests = Vec::new();
    for idx in 0..archive.len() {
        let file = archive
            .by_index(idx)
            .map_err(|e| format!("读取 zip 条目失败：{e}"))?;
        let name = normalized_zip_name(file.name())?;
        if name == "manifest.json" || name.ends_with("/manifest.json") {
            manifests.push(name);
        }
    }
    match manifests.len() {
        0 => Err("zip 中缺少 manifest.json".to_string()),
        1 => Ok(manifests.remove(0)),
        _ => Err("zip 中只能包含一个 manifest.json".to_string()),
    }
}

fn read_zip_text<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Result<String, String> {
    let mut file = archive
        .by_name(name)
        .map_err(|e| format!("读取 {name} 失败：{e}"))?;
    if file.size() > 256 * 1024 {
        return Err("manifest.json 过大".to_string());
    }
    let mut text = String::new();
    file.read_to_string(&mut text)
        .map_err(|e| format!("读取 manifest.json 文本失败：{e}"))?;
    Ok(text)
}

fn extract_archive<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    prefix: &str,
    dest: &Path,
) -> Result<(), String> {
    let mut total = 0u64;
    let mut files = 0usize;
    for idx in 0..archive.len() {
        let mut file = archive
            .by_index(idx)
            .map_err(|e| format!("读取 zip 条目失败：{e}"))?;
        if file.is_dir() {
            continue;
        }
        let name = normalized_zip_name(file.name())?;
        if !name.starts_with(prefix) {
            continue;
        }
        let rel = name[prefix.len()..].trim_start_matches('/');
        if rel.is_empty() {
            continue;
        }
        validate_relative_file(rel, "zip entry")?;
        files += 1;
        if files > MAX_IMPORT_FILES {
            return Err(format!("角色包文件过多：最多 {MAX_IMPORT_FILES} 个文件"));
        }
        total = total.saturating_add(file.size());
        if total > MAX_IMPORT_BYTES {
            return Err(format!(
                "角色包解压后过大：最大允许 {} MB",
                MAX_IMPORT_BYTES / 1024 / 1024
            ));
        }
        let out_path = dest.join(rel);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建导入目录失败：{e}"))?;
        }
        let mut out = fs::File::create(&out_path).map_err(|e| format!("创建导入文件失败：{e}"))?;
        std::io::copy(&mut file, &mut out).map_err(|e| format!("写入导入文件失败：{e}"))?;
    }
    Ok(())
}

fn validate_extracted_pack(dir: &Path, manifest: &PackManifest) -> Result<(), String> {
    let manifest_path = dir.join("manifest.json");
    if !manifest_path.exists() {
        return Err("导入后缺少 manifest.json".to_string());
    }
    let persona_path = resolve_pack_file(dir, &manifest.persona, "persona")?;
    if !persona_path.exists() {
        return Err(format!("导入后缺少 persona 文件：{}", manifest.persona));
    }
    if let Some(avatar) = manifest.avatar.as_deref() {
        let avatar_path = resolve_pack_file(dir, avatar, "avatar")?;
        if !avatar_path.exists() {
            return Err(format!("导入后缺少 avatar 文件：{avatar}"));
        }
    }
    Ok(())
}

fn normalized_zip_name(name: &str) -> Result<String, String> {
    let normalized = name.replace('\\', "/");
    validate_relative_file(normalized.trim_end_matches('/'), "zip entry")?;
    Ok(normalized.trim_start_matches("./").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "demiurge-pack-{label}-{}",
            crate::store::new_session_id()
        ))
    }

    fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        let options = SimpleFileOptions::default();
        for (name, bytes) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn validates_manifest_identity_and_paths() {
        let manifest = PackManifest {
            id: "valid_pack-1".to_string(),
            name: "Valid".to_string(),
            persona: "persona.md".to_string(),
            avatar: Some("avatar.png".to_string()),
            avatar_data_url: None,
        };
        validate_manifest_identity(&manifest).unwrap();
        validate_manifest_paths(&manifest).unwrap();

        let mut invalid = manifest.clone();
        invalid.id = "../bad".to_string();
        assert!(validate_manifest_identity(&invalid).is_err());

        let mut invalid_path = manifest;
        invalid_path.avatar = Some("avatar.svg".to_string());
        assert!(validate_manifest_paths(&invalid_path).is_err());
    }

    #[test]
    fn imports_zip_pack_and_exposes_avatar_data_url() {
        let packs = temp_dir("import");
        fs::create_dir_all(&packs).unwrap();
        let manifest = br#"{
  "id": "demo",
  "name": "Demo",
  "persona": "persona.md",
  "avatar": "avatar.png"
}"#;
        let bytes = zip_bytes(&[
            ("demo/manifest.json", manifest),
            ("demo/persona.md", b"hello"),
            ("demo/avatar.png", b"png-bytes"),
        ]);
        let imported = import_zip(&packs, "demo.zip", bytes).unwrap();
        assert_eq!(imported.id, "demo");
        assert!(imported
            .avatar_data_url
            .unwrap()
            .starts_with("data:image/png;base64,"));
        assert!(packs.join("demo").join("persona.md").exists());
        let _ = fs::remove_dir_all(packs);
    }

    #[test]
    fn rejects_zip_slip_entries() {
        let packs = temp_dir("slip");
        fs::create_dir_all(&packs).unwrap();
        let manifest = br#"{"id":"bad","name":"Bad","persona":"persona.md"}"#;
        let bytes = zip_bytes(&[("manifest.json", manifest), ("../persona.md", b"escape")]);
        assert!(import_zip(&packs, "bad.zip", bytes).is_err());
        let _ = fs::remove_dir_all(packs);
    }
}
