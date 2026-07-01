//! 角色包加载 + 清单。MVP 文本版清单，格式预留可成长字段（Live2D / TTS / 表情等）。
use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use zip::ZipArchive;

const MAX_IMPORT_FILES: usize = 100;
const MAX_IMPORT_BYTES: u64 = 25 * 1024 * 1024;
const LORE_INDEX_VERSION: u32 = 1;
const MAX_LORE_FILE_BYTES: u64 = 256 * 1024;
const MAX_LORE_INDEX_FILES: usize = 500;
const MAX_LORE_CHUNK_CHARS: usize = 1_200;
const MAX_LORE_CONTEXT_CHARS: usize = 4_000;
const MAX_LORE_CONTEXT_CHUNKS: usize = 4;
const DEFAULT_LORE_EXTENSIONS: &[&str] = &["md", "markdown", "txt"];

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PackManifest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<String>,
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character: Option<CharacterCard>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<CharacterRuntime>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lorebook: Vec<LoreEntry>,
}

pub struct Pack {
    #[allow(dead_code)]
    pub manifest: PackManifest,
    pub persona_text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CharacterCard {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub personality: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speech_style: Option<SpeechStyle>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub habits: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationship: Option<RelationshipStyle>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub opening_messages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub example_dialogues: Vec<ExampleDialogue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ooc_rules: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SpeechStyle {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tone: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_person: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address_user_as: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub catchphrases: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub taboo_phrases: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sentence_patterns: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct RelationshipStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progression: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ExampleDialogue {
    pub user: String,
    pub assistant: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct LoreEntry {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<f32>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub recursive: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CharacterRuntime {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<SkillBindingPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<MemoryPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<VoicePreference>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub permissions: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct SkillBindingPolicy {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recommended: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auto_activate: Vec<AutoSkillBinding>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct AutoSkillBinding {
    pub skill: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub when: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct MemoryPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_policy: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preferred_facts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub must_remember: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub avoid_remembering: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct VoicePreference {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tts_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackSkillPolicy {
    pub recommended: Vec<String>,
    pub disabled: Vec<String>,
    pub auto_activate: Vec<AutoSkillBinding>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
struct LoreFileSignature {
    path: String,
    len: u64,
    modified_ms: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct LoreIndexCache {
    version: u32,
    pack_id: String,
    files: Vec<LoreFileSignature>,
    chunks: Vec<LoreChunk>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct LoreChunk {
    source: String,
    title: String,
    heading: Option<String>,
    tags: Vec<String>,
    keywords: Vec<String>,
    priority: f32,
    chunk_index: usize,
    text: String,
}

#[derive(Clone, Debug)]
struct LoreSource {
    rel_path: String,
    title: Option<String>,
    tags: Vec<String>,
    priority: Option<f32>,
}

#[derive(Clone, Debug)]
struct MarkdownMeta {
    title: Option<String>,
    tags: Vec<String>,
    keywords: Vec<String>,
    priority: Option<f32>,
    body: String,
}

#[derive(Clone, Debug)]
struct LoreHit {
    score: f32,
    chunk: LoreChunk,
}

struct LoreSearchStats {
    document_count: usize,
    average_len: f32,
    document_frequency: HashMap<String, usize>,
}

/// 内置的通用人格（通用、不绑定任何特定角色）。首启动时落地为 packs/default。
const DEFAULT_MANIFEST: &str = r#"{
  "schema_version": "2.0",
  "id": "default",
  "name": "Demiurge",
  "description": "通用桌面伴侣角色卡示例。",
  "persona": "persona.md",
  "character": {
    "identity": "用户的桌面伴侣与本地 Agent 协作者。",
    "personality": ["温和", "好奇", "可靠", "克制"],
    "speech_style": {
      "tone": ["自然", "简洁", "口语化"],
      "first_person": "我",
      "address_user_as": "你",
      "taboo_phrases": ["过度客服腔", "假装已经完成未执行的动作"]
    },
    "habits": ["不确定时会说明不确定", "需要工具结果时会基于真实输出回答"],
    "relationship": {
      "default": "熟悉但保留专业边界的长期陪伴关系",
      "progression": "随用户记忆和会话习惯逐渐调整称呼、节奏和提醒方式"
    },
    "opening_messages": ["我在。今天想先聊聊，还是直接开工？"],
    "example_dialogues": [
      {
        "user": "我有点累，但是还想把事情做完。",
        "assistant": "那我们别硬冲。先把任务切小一点，做一个能收尾的版本，再决定要不要继续。"
      }
    ],
    "ooc_rules": ["不要编造工具结果", "不要越过用户明确拒绝的权限边界"]
  },
  "runtime": {
    "skills": {
      "recommended": ["pack-tone-guard"],
      "auto_activate": [
        { "skill": "pack-tone-guard", "when": ["*", "陪伴", "语气", "角色"] }
      ]
    },
    "memory": {
      "namespace": "default",
      "write_policy": "ask_before_sensitive",
      "preferred_facts": ["称呼偏好", "工作节奏", "提醒偏好", "语气偏好"],
      "avoid_remembering": ["敏感隐私，除非用户明确要求"]
    },
    "voice": {
      "tts_profile": "default",
      "speed": 1.0
    },
    "permissions": {
      "weather": "ask_once",
      "screen_ocr": "ask_every_time"
    }
  },
  "lorebook": [
    {
      "path": "lore",
      "title": "角色扩展设定",
      "tags": ["lore", "companion"],
      "recursive": true,
      "extensions": ["md", "txt"],
      "priority": 0.5
    }
  ]
}
"#;
const DEFAULT_PERSONA: &str = r#"你是用户的桌面伴侣。性格温和、好奇、乐于助人。
你会用自然、口语化的方式陪用户聊天，也能在需要时调用工具帮用户查信息、整理文件、打开网页等。
说话简洁、不绕弯、不过度客套。遇到不确定的事会如实说不知道，而不是编造。
"#;
const DEFAULT_LORE_README: &str = r#"# Demiurge 默认角色扩展设定

这里可以放角色背景、世界观、长期陪伴设定、台词样例和可检索 lore。
核心人格、说话风格和 OOC 规则应优先写入 manifest.json 或 persona.md；长篇剧情文本适合放在 lore/ 中，后续由检索系统按需注入上下文。
"#;
const DEFAULT_PACK_TONE_SKILL: &str = r#"---
name: Pack Tone Guard
description: Keep the active role card's persona, speech style, boundaries, and OOC rules stable.
triggers: [tone, persona, role, 陪伴, 角色, 语气, ooc]
always_include: true
---
Before replying, preserve the active role card's identity, relationship, speech style, habits, and OOC rules.
If the user asks for a capability outside the role card's permission or safety boundaries, stay in character while explaining the boundary and offering a safer alternative.
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
    let lore_dir = dir.join("lore");
    fs::create_dir_all(&lore_dir).map_err(|e| e.to_string())?;
    let lore = lore_dir.join("README.md");
    if !lore.exists() {
        fs::write(&lore, DEFAULT_LORE_README).map_err(|e| e.to_string())?;
    }
    let skill_dir = dir.join("skills").join("pack-tone-guard");
    fs::create_dir_all(&skill_dir).map_err(|e| e.to_string())?;
    let skill = skill_dir.join("SKILL.md");
    if !skill.exists() {
        fs::write(&skill, DEFAULT_PACK_TONE_SKILL).map_err(|e| e.to_string())?;
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
    let persona_text = render_persona_context(&dir, &manifest, &persona_text);
    Ok(Pack {
        manifest,
        persona_text,
    })
}

pub fn skill_policy(packs_dir: &Path, id: &str) -> PackSkillPolicy {
    let Ok(manifest) = read_manifest_no_avatar(&pack_dir(packs_dir, id)) else {
        return PackSkillPolicy::default();
    };
    manifest
        .runtime
        .and_then(|runtime| runtime.skills)
        .map(|skills| PackSkillPolicy {
            recommended: skills.recommended,
            disabled: skills.disabled,
            auto_activate: skills.auto_activate,
        })
        .unwrap_or_default()
}

pub fn lorebook_context(
    packs_dir: &Path,
    data_dir: &Path,
    id: &str,
    query: Option<&str>,
) -> String {
    let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) else {
        return String::new();
    };
    let Ok(chunks) = load_lore_index(packs_dir, data_dir, id) else {
        return String::new();
    };
    render_lore_hits(select_lore_hits(&chunks, query))
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

pub fn read_manifest_json(packs_dir: &Path, id: &str) -> Result<String, String> {
    let dir = pack_dir(packs_dir, id);
    let manifest = read_manifest_no_avatar(&dir)?;
    serde_json::to_string_pretty(&manifest).map_err(|e| format!("序列化角色卡清单失败：{e}"))
}

pub fn save_manifest_json(
    packs_dir: &Path,
    current_id: &str,
    raw_json: &str,
) -> Result<PackManifest, String> {
    let dir = pack_dir(packs_dir, current_id);
    if !dir.is_dir() {
        return Err(format!("角色包 `{current_id}` 不存在"));
    }
    let mut manifest = parse_manifest(raw_json)?;
    if manifest.id != current_id {
        return Err("暂不支持通过编辑 manifest.id 重命名角色包".to_string());
    }
    manifest.avatar_data_url = None;
    validate_manifest_paths(&manifest)?;
    validate_pack_files(&dir, &manifest)?;
    let text = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("序列化角色卡清单失败：{e}"))?;
    fs::write(dir.join("manifest.json"), format!("{text}\n"))
        .map_err(|e| format!("保存角色卡清单失败：{e}"))?;
    read_manifest_with_avatar(&dir)
}

fn read_manifest_with_avatar(dir: &Path) -> Result<PackManifest, String> {
    let mut manifest = read_manifest_no_avatar(dir)?;
    if let Some(avatar) = manifest.avatar.as_deref() {
        let avatar_path = resolve_pack_file(dir, avatar, "avatar")?;
        if avatar_path.exists() {
            manifest.avatar_data_url = Some(avatar_data_url(&avatar_path)?);
        }
    }
    Ok(manifest)
}

fn read_manifest_no_avatar(dir: &Path) -> Result<PackManifest, String> {
    let mf = dir.join("manifest.json");
    let txt = fs::read_to_string(&mf).map_err(|e| format!("读取角色包清单失败：{e}"))?;
    let manifest = parse_manifest(&txt)?;
    validate_manifest_paths(&manifest)?;
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
    if let Some(version) = manifest.schema_version.as_deref() {
        if !matches!(version, "1.0" | "2.0") {
            return Err("manifest.schema_version 只支持 1.0 或 2.0".to_string());
        }
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
    for lore in &manifest.lorebook {
        validate_relative_file(&lore.path, "lorebook.path")?;
        for extension in &lore.extensions {
            validate_lore_extension(extension)?;
        }
    }
    validate_runtime(&manifest.runtime)?;
    Ok(())
}

fn validate_runtime(runtime: &Option<CharacterRuntime>) -> Result<(), String> {
    let Some(runtime) = runtime else {
        return Ok(());
    };
    if let Some(skills) = &runtime.skills {
        for skill in skills
            .recommended
            .iter()
            .chain(skills.disabled.iter())
            .chain(skills.auto_activate.iter().map(|item| &item.skill))
        {
            validate_non_empty_trimmed(skill, "runtime.skills")?;
        }
    }
    if let Some(memory) = &runtime.memory {
        if let Some(namespace) = memory.namespace.as_deref() {
            validate_non_empty_trimmed(namespace, "runtime.memory.namespace")?;
            if !namespace
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            {
                return Err(
                    "runtime.memory.namespace 只能包含 ASCII 字母、数字、-、_ 和 .".to_string(),
                );
            }
        }
    }
    Ok(())
}

fn validate_non_empty_trimmed(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.trim() != value {
        return Err(format!("manifest.{field} 不能为空或包含首尾空白"));
    }
    Ok(())
}

fn validate_lore_extension(extension: &str) -> Result<(), String> {
    let extension = extension.trim().trim_start_matches('.');
    if extension.is_empty() || extension.len() > 16 {
        return Err("manifest.lorebook.extensions 包含非法扩展名".to_string());
    }
    if !extension
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err("manifest.lorebook.extensions 只能包含字母、数字和 _".to_string());
    }
    Ok(())
}

fn validate_lore_file_extension(path: &str, lore: &LoreEntry) -> Result<(), String> {
    let allowed = lore_extensions(lore);
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    if allowed.iter().any(|value| value == &extension) {
        Ok(())
    } else {
        Err(format!(
            "lore 文件扩展名不在允许列表中：{}",
            normalize_rel_path(path)
        ))
    }
}

fn lore_extensions(lore: &LoreEntry) -> Vec<String> {
    let values = if lore.extensions.is_empty() {
        DEFAULT_LORE_EXTENSIONS
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
    } else {
        lore.extensions
            .iter()
            .map(|value| value.trim().trim_start_matches('.').to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
    };
    dedupe_strings(values)
        .into_iter()
        .map(|value| value.to_ascii_lowercase())
        .collect()
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

fn normalize_rel_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_string()
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
    validate_pack_files(dir, manifest)
}

fn validate_pack_files(dir: &Path, manifest: &PackManifest) -> Result<(), String> {
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
    for lore in &manifest.lorebook {
        let lore_path = resolve_pack_file(dir, &lore.path, "lorebook.path")?;
        if !lore_path.exists() {
            return Err(format!("导入后缺少 lore 路径：{}", lore.path));
        }
        if lore_path.is_file() {
            validate_lore_file_extension(&lore.path, lore)?;
        } else if lore_path.is_dir() {
            if collect_lore_sources_from_entry(dir, lore)?.is_empty() {
                return Err(format!("lore 目录中缺少可索引文本：{}", lore.path));
            }
        } else {
            return Err(format!("lore 路径不是文件或目录：{}", lore.path));
        }
    }
    Ok(())
}

fn render_persona_context(dir: &Path, manifest: &PackManifest, persona_text: &str) -> String {
    let mut sections = Vec::new();
    sections.push(format!("# Persona\n{}", persona_text.trim()));
    let card = render_character_card(manifest);
    if !card.is_empty() {
        sections.push(format!("# Character Card\n{card}"));
    }
    let runtime = render_runtime_policy(manifest);
    if !runtime.is_empty() {
        sections.push(format!("# Runtime Policy\n{runtime}"));
    }
    let lore = render_lorebook_index(dir, manifest);
    if !lore.is_empty() {
        sections.push(format!("# Lorebook Index\n{lore}"));
    }
    sections.join("\n\n")
}

fn render_character_card(manifest: &PackManifest) -> String {
    let Some(character) = &manifest.character else {
        return String::new();
    };
    let mut lines = Vec::new();
    push_optional_line(&mut lines, "Identity", character.identity.as_deref());
    push_optional_line(&mut lines, "Background", character.background.as_deref());
    push_list_line(&mut lines, "Personality", &character.personality);
    if let Some(style) = &character.speech_style {
        push_list_line(&mut lines, "Tone", &style.tone);
        push_optional_line(&mut lines, "First person", style.first_person.as_deref());
        push_optional_line(
            &mut lines,
            "Address user as",
            style.address_user_as.as_deref(),
        );
        push_list_line(&mut lines, "Catchphrases", &style.catchphrases);
        push_list_line(&mut lines, "Taboo phrases", &style.taboo_phrases);
        push_list_line(&mut lines, "Sentence patterns", &style.sentence_patterns);
    }
    push_list_line(&mut lines, "Habits", &character.habits);
    if let Some(relationship) = &character.relationship {
        push_optional_line(
            &mut lines,
            "Default relationship",
            relationship.default.as_deref(),
        );
        push_optional_line(
            &mut lines,
            "Relationship progression",
            relationship.progression.as_deref(),
        );
    }
    push_list_line(&mut lines, "Opening messages", &character.opening_messages);
    if !character.example_dialogues.is_empty() {
        lines.push("Example dialogues:".to_string());
        for item in &character.example_dialogues {
            lines.push(format!("- User: {}", item.user.trim()));
            lines.push(format!("  Assistant: {}", item.assistant.trim()));
        }
    }
    push_list_line(&mut lines, "OOC rules", &character.ooc_rules);
    lines.join("\n")
}

fn render_runtime_policy(manifest: &PackManifest) -> String {
    let Some(runtime) = &manifest.runtime else {
        return String::new();
    };
    let mut lines = Vec::new();
    if let Some(skills) = &runtime.skills {
        push_list_line(&mut lines, "Recommended skills", &skills.recommended);
        push_list_line(&mut lines, "Disabled skills", &skills.disabled);
        if !skills.auto_activate.is_empty() {
            lines.push("Auto-activated skills:".to_string());
            for item in &skills.auto_activate {
                if item.when.is_empty() {
                    lines.push(format!("- {}", item.skill));
                } else {
                    lines.push(format!("- {} when {}", item.skill, item.when.join(", ")));
                }
            }
        }
    }
    if let Some(memory) = &runtime.memory {
        push_optional_line(
            &mut lines,
            "Memory namespace hint",
            memory.namespace.as_deref(),
        );
        push_optional_line(
            &mut lines,
            "Memory write policy hint",
            memory.write_policy.as_deref(),
        );
        push_list_line(&mut lines, "Preferred facts", &memory.preferred_facts);
        push_list_line(&mut lines, "Must remember", &memory.must_remember);
        push_list_line(&mut lines, "Avoid remembering", &memory.avoid_remembering);
    }
    if let Some(voice) = &runtime.voice {
        push_optional_line(
            &mut lines,
            "Voice TTS profile hint",
            voice.tts_profile.as_deref(),
        );
        if let Some(speed) = voice.speed {
            lines.push(format!("Voice speed: {speed}"));
        }
    }
    if !runtime.permissions.is_empty() {
        lines.push(
            "Tool permission preferences (advisory; system permission rules still apply):"
                .to_string(),
        );
        for (tool, policy) in &runtime.permissions {
            lines.push(format!("- {tool}: {policy}"));
        }
    }
    lines.join("\n")
}

fn render_lorebook_index(dir: &Path, manifest: &PackManifest) -> String {
    if manifest.lorebook.is_empty() {
        return String::new();
    }
    let mut lines = Vec::new();
    lines.push("Long lore and plot text are indexed here; keep core personality in the Character Card and retrieve lore only when relevant.".to_string());
    for lore in &manifest.lorebook {
        let title = lore.title.as_deref().unwrap_or(&lore.path);
        let exists = dir.join(&lore.path).exists();
        let kind = if dir.join(&lore.path).is_dir() {
            if lore.recursive {
                "directory recursive"
            } else {
                "directory"
            }
        } else {
            "file"
        };
        let tags = if lore.tags.is_empty() {
            "".to_string()
        } else {
            format!(" tags={}", lore.tags.join(","))
        };
        let extensions = if lore.extensions.is_empty() {
            "".to_string()
        } else {
            format!(" extensions={}", lore.extensions.join(","))
        };
        let priority = lore
            .priority
            .map(|value| format!(" priority={value}"))
            .unwrap_or_default();
        let status = if exists { "available" } else { "missing" };
        lines.push(format!(
            "- {title} ({}) [{status}; {kind}]{tags}{extensions}{priority}",
            lore.path
        ));
    }
    lines.join("\n")
}

fn load_lore_index(packs_dir: &Path, data_dir: &Path, id: &str) -> Result<Vec<LoreChunk>, String> {
    let dir = pack_dir(packs_dir, id);
    let manifest = read_manifest_no_avatar(&dir)?;
    if manifest.lorebook.is_empty() {
        return Ok(Vec::new());
    }
    let files = lore_file_signatures(&dir, &manifest)?;
    let cache_path = lore_index_cache_path(data_dir, id);
    if let Ok(text) = fs::read_to_string(&cache_path) {
        if let Ok(cache) = serde_json::from_str::<LoreIndexCache>(&text) {
            if cache.version == LORE_INDEX_VERSION && cache.pack_id == id && cache.files == files {
                return Ok(cache.chunks);
            }
        }
    }

    let chunks = build_lore_chunks(&dir, &manifest)?;
    let cache = LoreIndexCache {
        version: LORE_INDEX_VERSION,
        pack_id: id.to_string(),
        files,
        chunks: chunks.clone(),
    };
    if let Some(parent) = cache_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(&cache) {
        let _ = fs::write(cache_path, format!("{text}\n"));
    }
    Ok(chunks)
}

fn lore_index_cache_path(data_dir: &Path, id: &str) -> PathBuf {
    let safe_id = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    data_dir
        .join("lorebook-index")
        .join(format!("{safe_id}.json"))
}

fn lore_file_signatures(
    dir: &Path,
    manifest: &PackManifest,
) -> Result<Vec<LoreFileSignature>, String> {
    let mut files = Vec::new();
    for source in collect_lore_sources(dir, manifest)? {
        let path = resolve_pack_file(dir, &source.rel_path, "lorebook.path")?;
        let meta = fs::metadata(&path).map_err(|e| format!("读取 lore 元数据失败：{e}"))?;
        if !meta.is_file() {
            return Err(format!("lore 不是文件：{}", source.rel_path));
        }
        if meta.len() > MAX_LORE_FILE_BYTES {
            return Err(format!("lore 文件过大：{}", source.rel_path));
        }
        let modified_ms = meta
            .modified()
            .ok()
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .map(|value| value.as_millis().min(u128::from(u64::MAX)) as u64)
            .unwrap_or(0);
        files.push(LoreFileSignature {
            path: source.rel_path,
            len: meta.len(),
            modified_ms,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn build_lore_chunks(dir: &Path, manifest: &PackManifest) -> Result<Vec<LoreChunk>, String> {
    let mut chunks = Vec::new();
    for source in collect_lore_sources(dir, manifest)? {
        let path = resolve_pack_file(dir, &source.rel_path, "lorebook.path")?;
        let text = fs::read_to_string(&path).map_err(|e| format!("读取 lore 失败：{e}"))?;
        let meta = parse_markdown_meta(&text);
        let title = meta
            .title
            .clone()
            .or_else(|| source.title.clone())
            .unwrap_or_else(|| source.rel_path.clone());
        let tags = merge_unique(source.tags.clone(), meta.tags.clone());
        let keywords = merge_unique(tags.clone(), meta.keywords.clone());
        let priority = meta.priority.or(source.priority).unwrap_or(0.0);
        split_lore_markdown(
            &mut chunks,
            &source.rel_path,
            title,
            tags,
            keywords,
            priority,
            &meta.body,
        );
    }
    Ok(chunks)
}

fn collect_lore_sources(dir: &Path, manifest: &PackManifest) -> Result<Vec<LoreSource>, String> {
    let mut sources = Vec::new();
    for lore in &manifest.lorebook {
        sources.extend(collect_lore_sources_from_entry(dir, lore)?);
        if sources.len() > MAX_LORE_INDEX_FILES {
            return Err(format!(
                "lore 文件过多：最多索引 {MAX_LORE_INDEX_FILES} 个文本文件"
            ));
        }
    }
    sources.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(sources)
}

fn collect_lore_sources_from_entry(
    dir: &Path,
    lore: &LoreEntry,
) -> Result<Vec<LoreSource>, String> {
    let path = resolve_pack_file(dir, &lore.path, "lorebook.path")?;
    if path.is_file() {
        validate_lore_file_extension(&lore.path, lore)?;
        return Ok(vec![LoreSource {
            rel_path: normalize_rel_path(&lore.path),
            title: lore.title.clone(),
            tags: lore.tags.clone(),
            priority: lore.priority,
        }]);
    }
    if !path.is_dir() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    collect_lore_dir(dir, &path, lore, &mut out)?;
    out.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(out)
}

fn collect_lore_dir(
    pack_dir: &Path,
    dir: &Path,
    lore: &LoreEntry,
    out: &mut Vec<LoreSource>,
) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("读取 lore 目录失败：{e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            if lore.recursive {
                collect_lore_dir(pack_dir, &path, lore, out)?;
            }
            continue;
        }
        if !meta.is_file() {
            continue;
        }
        let rel = path
            .strip_prefix(pack_dir)
            .map_err(|_| "lore 文件不在角色包目录内".to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if validate_lore_file_extension(&rel, lore).is_ok() {
            out.push(LoreSource {
                rel_path: rel,
                title: lore.title.clone(),
                tags: lore.tags.clone(),
                priority: lore.priority,
            });
        }
    }
    Ok(())
}

fn parse_markdown_meta(text: &str) -> MarkdownMeta {
    let normalized = text.strip_prefix('\u{feff}').unwrap_or(text);
    if !normalized.starts_with("---\n") && !normalized.starts_with("---\r\n") {
        return MarkdownMeta {
            title: None,
            tags: Vec::new(),
            keywords: Vec::new(),
            priority: None,
            body: normalized.trim().to_string(),
        };
    }

    let body_start = if normalized.starts_with("---\r\n") {
        5
    } else {
        4
    };
    let rest = &normalized[body_start..];
    let Some(end) = rest.find("\n---") else {
        return MarkdownMeta {
            title: None,
            tags: Vec::new(),
            keywords: Vec::new(),
            priority: None,
            body: normalized.trim().to_string(),
        };
    };
    let frontmatter = &rest[..end];
    let after_marker = &rest[end + "\n---".len()..];
    let body = after_marker
        .strip_prefix("\r\n")
        .or_else(|| after_marker.strip_prefix('\n'))
        .unwrap_or(after_marker)
        .trim()
        .to_string();
    let mut meta = MarkdownMeta {
        title: None,
        tags: Vec::new(),
        keywords: Vec::new(),
        priority: None,
        body,
    };
    for line in frontmatter.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim().trim_matches('"').trim_matches('\'');
        match key.as_str() {
            "title" => meta.title = non_empty_string(value),
            "tags" => meta.tags = parse_list_value(value),
            "keywords" => meta.keywords = parse_list_value(value),
            "priority" => meta.priority = value.parse::<f32>().ok(),
            _ => {}
        }
    }
    meta
}

fn split_lore_markdown(
    chunks: &mut Vec<LoreChunk>,
    source: &str,
    title: String,
    tags: Vec<String>,
    keywords: Vec<String>,
    priority: f32,
    text: &str,
) {
    let mut heading: Option<String> = None;
    let mut buffer = Vec::new();
    for line in text.lines() {
        if let Some(next_heading) = markdown_heading(line) {
            flush_lore_segment(
                chunks,
                source,
                &title,
                heading.as_deref(),
                &tags,
                &keywords,
                priority,
                &buffer.join("\n"),
            );
            buffer.clear();
            heading = Some(next_heading);
        } else {
            buffer.push(line.to_string());
        }
    }
    flush_lore_segment(
        chunks,
        source,
        &title,
        heading.as_deref(),
        &tags,
        &keywords,
        priority,
        &buffer.join("\n"),
    );
}

fn flush_lore_segment(
    chunks: &mut Vec<LoreChunk>,
    source: &str,
    title: &str,
    heading: Option<&str>,
    tags: &[String],
    keywords: &[String],
    priority: f32,
    text: &str,
) {
    let paragraphs = text
        .split("\n\n")
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mut current = String::new();
    for paragraph in paragraphs {
        if char_count(paragraph) > MAX_LORE_CHUNK_CHARS {
            flush_lore_text(
                chunks, source, title, heading, tags, keywords, priority, &current,
            );
            current.clear();
            for part in split_by_chars(paragraph, MAX_LORE_CHUNK_CHARS) {
                flush_lore_text(
                    chunks, source, title, heading, tags, keywords, priority, &part,
                );
            }
            continue;
        }
        let next_len = char_count(&current)
            .saturating_add(char_count(paragraph))
            .saturating_add(2);
        if !current.is_empty() && next_len > MAX_LORE_CHUNK_CHARS {
            flush_lore_text(
                chunks, source, title, heading, tags, keywords, priority, &current,
            );
            current.clear();
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(paragraph);
    }
    flush_lore_text(
        chunks, source, title, heading, tags, keywords, priority, &current,
    );
}

fn flush_lore_text(
    chunks: &mut Vec<LoreChunk>,
    source: &str,
    title: &str,
    heading: Option<&str>,
    tags: &[String],
    keywords: &[String],
    priority: f32,
    text: &str,
) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let chunk_index = chunks.iter().filter(|chunk| chunk.source == source).count();
    chunks.push(LoreChunk {
        source: source.to_string(),
        title: title.to_string(),
        heading: heading.map(str::to_string),
        tags: tags.to_vec(),
        keywords: keywords.to_vec(),
        priority,
        chunk_index,
        text: text.to_string(),
    });
}

fn select_lore_hits(chunks: &[LoreChunk], query: &str) -> Vec<LoreHit> {
    let terms = query_terms(query);
    if terms.is_empty() {
        return Vec::new();
    }
    let query = normalize_search_text(query);
    let stats = lore_search_stats(chunks);
    let mut hits = chunks
        .iter()
        .filter_map(|chunk| {
            let score = score_lore_chunk(chunk, &query, &terms, &stats);
            (score > 0.0).then(|| LoreHit {
                score,
                chunk: chunk.clone(),
            })
        })
        .collect::<Vec<_>>();
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                b.chunk
                    .priority
                    .partial_cmp(&a.chunk.priority)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.chunk.source.cmp(&b.chunk.source))
            .then_with(|| a.chunk.chunk_index.cmp(&b.chunk.chunk_index))
    });
    hits.truncate(MAX_LORE_CONTEXT_CHUNKS);
    hits
}

fn score_lore_chunk(
    chunk: &LoreChunk,
    query: &str,
    terms: &[String],
    stats: &LoreSearchStats,
) -> f32 {
    let title = normalize_search_text(&chunk.title);
    let heading = normalize_search_text(chunk.heading.as_deref().unwrap_or_default());
    let tags = normalize_search_text(&chunk.tags.join(" "));
    let keywords = normalize_search_text(&chunk.keywords.join(" "));
    let text = normalize_search_text(&chunk.text);
    let mut metadata_score = 0.0;
    let mut chunk_score = 0.0;
    if !query.is_empty() && text.contains(query) {
        chunk_score += 12.0;
    }
    for term in terms {
        if title.contains(term) {
            metadata_score += 8.0;
        }
        if heading.contains(term) {
            chunk_score += 6.0;
        }
        if tags.contains(term) || keywords.contains(term) {
            metadata_score += 5.0;
        }
        if text.contains(term) {
            chunk_score += 2.0;
        }
    }
    if chunk_score > 0.0 {
        chunk_score
            + metadata_score
            + bm25_score(chunk, terms, stats)
            + chunk.priority.max(0.0) * 4.0
    } else if metadata_score > 0.0 && chunk.chunk_index == 0 {
        metadata_score + chunk.priority.max(0.0) * 4.0
    } else {
        0.0
    }
}

fn lore_search_stats(chunks: &[LoreChunk]) -> LoreSearchStats {
    let mut document_frequency = HashMap::new();
    let mut total_len = 0usize;
    for chunk in chunks {
        let terms = chunk_search_terms(chunk);
        total_len = total_len.saturating_add(terms.len());
        let unique = terms.into_iter().collect::<HashSet<_>>();
        for term in unique {
            *document_frequency.entry(term).or_insert(0) += 1;
        }
    }
    let document_count = chunks.len();
    let average_len = if document_count == 0 {
        1.0
    } else {
        (total_len as f32 / document_count as f32).max(1.0)
    };
    LoreSearchStats {
        document_count,
        average_len,
        document_frequency,
    }
}

fn bm25_score(chunk: &LoreChunk, query_terms: &[String], stats: &LoreSearchStats) -> f32 {
    if stats.document_count == 0 {
        return 0.0;
    }
    let terms = chunk_search_terms(chunk);
    if terms.is_empty() {
        return 0.0;
    }
    let mut tf = HashMap::new();
    for term in &terms {
        *tf.entry(term.as_str()).or_insert(0usize) += 1;
    }
    let doc_len = terms.len() as f32;
    let k1 = 1.2f32;
    let b = 0.75f32;
    let mut score = 0.0;
    for term in query_terms {
        let Some(freq) = tf.get(term.as_str()).copied() else {
            continue;
        };
        let df = stats.document_frequency.get(term).copied().unwrap_or(0) as f32;
        let n = stats.document_count as f32;
        let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln().max(0.0);
        let freq = freq as f32;
        let denom = freq + k1 * (1.0 - b + b * doc_len / stats.average_len);
        score += idf * (freq * (k1 + 1.0)) / denom;
    }
    score * 3.0
}

fn chunk_search_terms(chunk: &LoreChunk) -> Vec<String> {
    let haystack = [
        chunk.title.as_str(),
        chunk.heading.as_deref().unwrap_or_default(),
        &chunk.tags.join(" "),
        &chunk.keywords.join(" "),
        chunk.text.as_str(),
    ]
    .join(" ");
    let normalized = normalize_search_text(&haystack);
    let mut terms = search_tokens(&normalized);
    for token in search_tokens(&normalized) {
        if token.chars().any(is_cjk) {
            let chars = token.chars().collect::<Vec<_>>();
            for n in [2usize, 3] {
                if chars.len() >= n {
                    for window in chars.windows(n) {
                        terms.push(window.iter().collect::<String>());
                    }
                }
            }
        }
    }
    terms
}

fn render_lore_hits(hits: Vec<LoreHit>) -> String {
    if hits.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "Use these retrieved lore snippets as supporting facts only. Character Card, persona, speech style, and OOC rules remain authoritative.\n",
    );
    for hit in hits {
        let chunk = hit.chunk;
        let heading = chunk
            .heading
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&chunk.title);
        let tags = if chunk.tags.is_empty() {
            String::new()
        } else {
            format!("; tags={}", chunk.tags.join(", "))
        };
        let block = format!(
            "\n## {} / {}\nsource: {}#{}; score={:.1}{}\n{}\n",
            chunk.title,
            heading,
            chunk.source,
            chunk.chunk_index,
            hit.score,
            tags,
            chunk.text.trim()
        );
        if char_count(&out).saturating_add(char_count(&block)) > MAX_LORE_CONTEXT_CHARS {
            break;
        }
        out.push_str(&block);
    }
    out.trim().to_string()
}

fn markdown_heading(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|c| *c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = trimmed[level..].trim();
    (!rest.is_empty()).then(|| rest.to_string())
}

fn query_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let normalized = normalize_search_text(query);
    if normalized.chars().count() >= 2 {
        terms.push(normalized.clone());
    }
    for token in search_tokens(&normalized) {
        if token.chars().count() >= 2 {
            terms.push(token.clone());
        }
        if token.chars().any(is_cjk) {
            let chars = token.chars().collect::<Vec<_>>();
            for n in [2usize, 3] {
                if chars.len() >= n {
                    for window in chars.windows(n) {
                        terms.push(window.iter().collect::<String>());
                    }
                }
            }
        }
    }
    dedupe_strings(terms)
}

fn search_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || is_cjk(c) {
            current.push(c);
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn normalize_search_text(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| {
            if c.is_ascii_alphanumeric() || is_cjk(c) {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_cjk(c: char) -> bool {
    matches!(
        c as u32,
        0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0x20000..=0x2A6DF
            | 0x2A700..=0x2B73F
            | 0x2B740..=0x2B81F
            | 0x2B820..=0x2CEAF
    )
}

fn split_by_chars(text: &str, max_chars: usize) -> Vec<String> {
    let chars = text.chars().collect::<Vec<_>>();
    chars
        .chunks(max_chars.max(1))
        .map(|chunk| chunk.iter().collect::<String>())
        .collect()
}

fn char_count(text: &str) -> usize {
    text.chars().count()
}

fn parse_list_value(value: &str) -> Vec<String> {
    let trimmed = value.trim();
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(trimmed);
    inner
        .split(',')
        .filter_map(|item| non_empty_string(item.trim().trim_matches('"').trim_matches('\'')))
        .collect()
}

fn non_empty_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn merge_unique(first: Vec<String>, second: Vec<String>) -> Vec<String> {
    dedupe_strings(first.into_iter().chain(second).collect())
}

fn dedupe_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for value in values {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let key = value.to_ascii_lowercase();
        if seen.insert(key) {
            out.push(value.to_string());
        }
    }
    out
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn push_optional_line(lines: &mut Vec<String>, label: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        lines.push(format!("{label}: {value}"));
    }
}

fn push_list_line(lines: &mut Vec<String>, label: &str, values: &[String]) {
    let values = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if !values.is_empty() {
        lines.push(format!("{label}: {}", values.join(", ")));
    }
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
            schema_version: None,
            id: "valid_pack-1".to_string(),
            name: "Valid".to_string(),
            description: None,
            persona: "persona.md".to_string(),
            avatar: Some("avatar.png".to_string()),
            avatar_data_url: None,
            character: None,
            runtime: None,
            lorebook: Vec::new(),
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
    fn loads_manifest_2_character_card_into_persona_context() {
        let packs = temp_dir("card");
        let dir = packs.join("demo");
        fs::create_dir_all(dir.join("lore")).unwrap();
        fs::write(dir.join("persona.md"), "Base persona.").unwrap();
        fs::write(dir.join("lore").join("story.md"), "Long story.").unwrap();
        fs::write(
            dir.join("manifest.json"),
            r#"{
  "schema_version": "2.0",
  "id": "demo",
  "name": "Demo",
  "persona": "persona.md",
  "character": {
    "identity": "A careful companion.",
    "speech_style": {
      "tone": ["quiet", "warm"],
      "taboo_phrases": ["customer support voice"]
    },
    "ooc_rules": ["Never break character."]
  },
  "runtime": {
    "skills": { "recommended": ["Pack Tone Guard"] },
    "memory": { "namespace": "demo", "must_remember": ["relationship state"] }
  },
  "lorebook": [
    { "path": "lore/story.md", "title": "Main Story", "tags": ["plot"] }
  ]
}"#,
        )
        .unwrap();

        let pack = load_pack(&packs, "demo").unwrap();
        assert!(pack.persona_text.contains("Base persona."));
        assert!(pack.persona_text.contains("Identity: A careful companion."));
        assert!(pack.persona_text.contains("Tone: quiet, warm"));
        assert!(pack
            .persona_text
            .contains("OOC rules: Never break character."));
        assert!(pack
            .persona_text
            .contains("Recommended skills: Pack Tone Guard"));
        assert!(pack
            .persona_text
            .contains("Main Story (lore/story.md) [available; file]"));

        let policy = skill_policy(&packs, "demo");
        assert_eq!(policy.recommended, vec!["Pack Tone Guard"]);
        let _ = fs::remove_dir_all(packs);
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
    fn import_validates_manifest_2_lore_files() {
        let packs = temp_dir("import_lore");
        fs::create_dir_all(&packs).unwrap();
        let manifest = br#"{
  "schema_version":"2.0",
  "id":"demo",
  "name":"Demo",
  "persona":"persona.md",
  "lorebook":[{"path":"lore/missing.md"}]
}"#;
        let bytes = zip_bytes(&[("manifest.json", manifest), ("persona.md", b"hello")]);
        assert!(import_zip(&packs, "demo.zip", bytes).is_err());
        let _ = fs::remove_dir_all(packs);
    }

    #[test]
    fn lorebook_context_retrieves_markdown_chunks_and_caches_index() {
        let packs = temp_dir("lorebook");
        let data = temp_dir("lorebook_data");
        let dir = packs.join("demo");
        fs::create_dir_all(dir.join("lore").join("arc")).unwrap();
        fs::create_dir_all(&data).unwrap();
        fs::write(dir.join("persona.md"), "Base persona.").unwrap();
        fs::write(
            dir.join("manifest.json"),
            r#"{
  "schema_version": "2.0",
  "id": "demo",
  "name": "Demo",
  "persona": "persona.md",
  "lorebook": [
    { "path": "lore", "title": "故事设定", "tags": ["月亮城"], "recursive": true, "extensions": ["md", "txt"], "priority": 0.5 }
  ]
}"#,
        )
        .unwrap();
        fs::write(
            dir.join("lore").join("story.md"),
            r#"---
title: 月亮城年表
tags: [剧情, 城市]
keywords: [银钟塔, 夜巡]
priority: 0.8
---

# 银钟塔事件

月亮城的银钟塔在雨夜停摆，夜巡队随后封锁了旧桥。

# 海边集市

这里记录的是海边集市和甜点摊的日常。"#,
        )
        .unwrap();
        fs::write(
            dir.join("lore").join("arc").join("battle.txt"),
            "赤桥战役发生在第三章，夜巡队在这里第一次公开协助主角。",
        )
        .unwrap();

        let context = lorebook_context(&packs, &data, "demo", Some("银钟塔后来怎么了"));
        assert!(context.contains("retrieved lore snippets"));
        assert!(context.contains("月亮城年表"));
        assert!(context.contains("银钟塔在雨夜停摆"));
        assert!(!context.contains("甜点摊"));
        assert!(lore_index_cache_path(&data, "demo").exists());

        let nested = lorebook_context(&packs, &data, "demo", Some("赤桥战役在哪一章"));
        assert!(nested.contains("赤桥战役发生在第三章"));

        let _ = fs::remove_dir_all(packs);
        let _ = fs::remove_dir_all(data);
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
