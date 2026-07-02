//! 角色包加载 + 清单。MVP 文本版清单，格式预留可成长字段（Live2D / TTS / 表情等）。
use crate::embed;
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
const MAX_PACK_READ_BYTES: u64 = 512 * 1024;
const MAX_PACK_LIST_ENTRIES: usize = 1000;
const MAX_LIVE2D_IMPORT_BYTES: u64 = 200 * 1024 * 1024;
const MAX_LIVE2D_IMPORT_FILES: usize = 200;

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
    pub live2d: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character: Option<CharacterCard>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<CharacterRuntime>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lorebook: Vec<LoreEntry>,
    /// 素材授权/出处清单（avatar、persona、lore、voice 等）。导入时缺失会产出提示。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credits: Vec<AssetCredit>,
    /// 整包 license 声明（如 MIT、CC-BY-4.0 等）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
}

/// 单条素材授权记录：asset 是包内相对路径或字段名，author/source/license 可选。
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetCredit {
    pub asset: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
}

/// 包内文件浏览条目。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PackFileEntry {
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_ms: u64,
}

/// 包内文件读取结果：文本文件返 text，图片返 base64 data URL。
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PackFileContent {
    pub path: String,
    pub text: Option<String>,
    pub data_url: Option<String>,
    pub truncated: bool,
}

/// 批量 lore 导入入参。
#[derive(Deserialize, Clone, Debug)]
pub struct PackLoreFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// zip 导入结果：manifest + 授权缺失等非阻塞警告。
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PackImportResult {
    pub manifest: PackManifest,
    pub warnings: Vec<String>,
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
    /// 生成 chunks.embeddings 所用的 provider+model 标识；切换 model 时失效重算。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    embedding_model: Option<String>,
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
    /// 向量召回用的稠密向量缓存（按 embedding_model 失效）。None 表示尚未计算。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    embedding: Option<Vec<f32>>,
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
    /// 向量召回的余弦相似度（provider 可用时填充），供召回面板展示。
    #[allow(dead_code)]
    dense_score: Option<f32>,
}

struct LoreSearchStats {
    document_count: usize,
    average_len: f32,
    document_frequency: HashMap<String, usize>,
}

/// Lorebook 索引状态（供前端召回可视化面板）。
#[derive(Serialize, Clone, Debug)]
pub struct LoreIndexStatus {
    pub pack_id: String,
    pub cache_exists: bool,
    pub version: Option<u32>,
    pub file_count: usize,
    pub chunk_count: usize,
    /// 当前磁盘文件签名与缓存是否不一致（需要重建）。
    pub files_stale: bool,
    pub last_built_ms: u64,
}

/// 单条召回详情（含命中关键词）。
#[derive(Serialize, Clone, Debug)]
pub struct LoreHitDetail {
    pub score: f32,
    pub source: String,
    pub title: String,
    pub heading: Option<String>,
    pub chunk_index: usize,
    pub text: String,
    pub tags: Vec<String>,
    pub keywords: Vec<String>,
    pub priority: f32,
    pub matched_terms: Vec<String>,
    /// 向量召回余弦相似度（provider 可用时填充）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dense_score: Option<f32>,
}

/// 召回详情结果。
#[derive(Serialize, Clone, Debug)]
pub struct LoreRecallDetail {
    pub query: String,
    pub total_chunks: usize,
    pub hits: Vec<LoreHitDetail>,
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

/// 读取角色包声明的 memory namespace；返回 None 表示走默认（共享）路径。
/// 非 default 的 namespace 会让 memory 模块把 user/project 记忆文件隔离到带后缀的路径。
pub fn manifest_namespace(packs_dir: &Path, id: &str) -> Option<String> {
    let manifest = read_manifest_no_avatar(&pack_dir(packs_dir, id)).ok()?;
    let ns = manifest
        .runtime
        .and_then(|runtime| runtime.memory)
        .and_then(|memory| memory.namespace)?;
    let ns = ns.trim();
    if ns.is_empty() || ns == "default" {
        None
    } else {
        Some(ns.to_string())
    }
}

/// 读取角色卡 runtime.permissions 偏好（tool → 偏好字符串，如 "allow"/"deny"/"ask_once"/"ask_every_time"）。
/// 供 permission overlay 把角色卡偏好升级为可执行覆盖，而非仅提示词建议。
pub fn permission_preferences(packs_dir: &Path, id: &str) -> BTreeMap<String, String> {
    let Ok(manifest) = read_manifest_no_avatar(&pack_dir(packs_dir, id)) else {
        return BTreeMap::new();
    };
    manifest
        .runtime
        .map(|runtime| runtime.permissions)
        .unwrap_or_default()
}

pub fn lorebook_context(
    packs_dir: &Path,
    data_dir: &Path,
    id: &str,
    query: Option<&str>,
    provider: Option<&dyn embed::EmbeddingProvider>,
    hybrid_weight: f32,
) -> String {
    let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) else {
        return String::new();
    };
    let Ok((mut chunks, cache_path)) = load_lore_index_with_cache(packs_dir, data_dir, id) else {
        return String::new();
    };
    if let Some(p) = provider {
        let model_key = format!("{}:{}", p.name(), p.dims());
        ensure_chunk_embeddings(&mut chunks, p, &model_key, &cache_path);
        render_lore_hits(select_lore_hits(&chunks, query, Some(p), hybrid_weight))
    } else {
        render_lore_hits(select_lore_hits(&chunks, query, None, hybrid_weight))
    }
}

/// Lorebook 索引状态：缓存是否存在、文件数、chunk 数、是否过期。
pub fn lorebook_index_status(
    packs_dir: &Path,
    data_dir: &Path,
    id: &str,
) -> Result<LoreIndexStatus, String> {
    let dir = pack_dir(packs_dir, id);
    let manifest = read_manifest_no_avatar(&dir)?;
    let cache_path = lore_index_cache_path(data_dir, id);
    let mut status = LoreIndexStatus {
        pack_id: id.to_string(),
        cache_exists: cache_path.exists(),
        version: None,
        file_count: 0,
        chunk_count: 0,
        files_stale: false,
        last_built_ms: 0,
    };
    if manifest.lorebook.is_empty() {
        return Ok(status);
    }
    let current_files = lore_file_signatures(&dir, &manifest)?;
    status.file_count = current_files.len();
    let last_built_ms = || {
        fs::metadata(&cache_path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    };
    if let Ok(text) = fs::read_to_string(&cache_path) {
        if let Ok(cache) = serde_json::from_str::<LoreIndexCache>(&text) {
            status.version = Some(cache.version);
            status.chunk_count = cache.chunks.len();
            status.files_stale = cache.version != LORE_INDEX_VERSION
                || cache.pack_id != id
                || cache.files != current_files;
            status.last_built_ms = last_built_ms();
        }
    } else {
        status.files_stale = !current_files.is_empty();
    }
    Ok(status)
}

/// 召回详情：全量打分（按 score 降序，截断到 limit），每条含命中关键词。
pub fn lorebook_recall_detail(
    packs_dir: &Path,
    data_dir: &Path,
    id: &str,
    query: &str,
    limit: usize,
    provider: Option<&dyn embed::EmbeddingProvider>,
    hybrid_weight: f32,
) -> Result<LoreRecallDetail, String> {
    let (mut chunks, cache_path) = load_lore_index_with_cache(packs_dir, data_dir, id)?;
    let total = chunks.len();
    if let Some(p) = provider {
        let model_key = format!("{}:{}", p.name(), p.dims());
        ensure_chunk_embeddings(&mut chunks, p, &model_key, &cache_path);
    }
    let terms = query_terms(query);
    let norm = normalize_search_text(query);
    let mut hits = score_all_lore_hits(&chunks, query, provider, hybrid_weight);
    if limit > 0 {
        hits.truncate(limit);
    }
    let details = hits
        .iter()
        .map(|hit| {
            let matched = matched_terms_for(&hit.chunk, &norm, &terms);
            LoreHitDetail {
                score: hit.score,
                source: hit.chunk.source.clone(),
                title: hit.chunk.title.clone(),
                heading: hit.chunk.heading.clone(),
                chunk_index: hit.chunk.chunk_index,
                text: hit.chunk.text.clone(),
                tags: hit.chunk.tags.clone(),
                keywords: hit.chunk.keywords.clone(),
                priority: hit.chunk.priority,
                matched_terms: matched,
                dense_score: hit.dense_score,
            }
        })
        .collect();
    Ok(LoreRecallDetail {
        query: query.to_string(),
        total_chunks: total,
        hits: details,
    })
}

/// 删除缓存并重建索引，返回新状态。
pub fn lorebook_rebuild_index(
    packs_dir: &Path,
    data_dir: &Path,
    id: &str,
) -> Result<LoreIndexStatus, String> {
    let cache_path = lore_index_cache_path(data_dir, id);
    let _ = fs::remove_file(&cache_path);
    let _ = load_lore_index(packs_dir, data_dir, id)?;
    lorebook_index_status(packs_dir, data_dir, id)
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
fn normalize_live2d_model_files(dest: &Path, original_model3_name: &str) -> Result<String, String> {
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
    let path = resolve_pack_file(&dir, live2d, "live2d")?;
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
    if let Some(live2d) = manifest.live2d.as_deref() {
        validate_relative_file(live2d, "live2d")?;
        if !live2d.ends_with(".model3.json") {
            return Err("manifest.live2d 必须指向 .model3.json 文件".to_string());
        }
    }
    for lore in &manifest.lorebook {
        validate_relative_file(&lore.path, "lorebook.path")?;
        for extension in &lore.extensions {
            validate_lore_extension(extension)?;
        }
    }
    for credit in &manifest.credits {
        if !credit.asset.trim().is_empty() {
            validate_relative_file(&credit.asset, "credits.asset")?;
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
    if let Some(live2d) = manifest.live2d.as_deref() {
        let live2d_path = resolve_pack_file(dir, live2d, "live2d")?;
        if !live2d_path.exists() {
            return Err(format!("导入后缺少 live2d 文件：{live2d}"));
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
            "Tool permission preferences (enforced as an overlay between user rules and tool defaults; explicit user rules still override):"
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
    Ok(load_lore_index_with_cache(packs_dir, data_dir, id)?.0)
}

/// 加载 lorebook 索引，返回 chunks 与缓存文件路径（供 embedding 持久化使用）。
fn load_lore_index_with_cache(
    packs_dir: &Path,
    data_dir: &Path,
    id: &str,
) -> Result<(Vec<LoreChunk>, PathBuf), String> {
    let dir = pack_dir(packs_dir, id);
    let manifest = read_manifest_no_avatar(&dir)?;
    let cache_path = lore_index_cache_path(data_dir, id);
    if manifest.lorebook.is_empty() {
        return Ok((Vec::new(), cache_path));
    }
    let files = lore_file_signatures(&dir, &manifest)?;
    if let Ok(text) = fs::read_to_string(&cache_path) {
        if let Ok(cache) = serde_json::from_str::<LoreIndexCache>(&text) {
            if cache.version == LORE_INDEX_VERSION && cache.pack_id == id && cache.files == files {
                return Ok((cache.chunks, cache_path));
            }
        }
    }

    let chunks = build_lore_chunks(&dir, &manifest)?;
    let cache = LoreIndexCache {
        version: LORE_INDEX_VERSION,
        pack_id: id.to_string(),
        files,
        chunks: chunks.clone(),
        embedding_model: None,
    };
    if let Some(parent) = cache_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(&cache) {
        let _ = fs::write(&cache_path, format!("{text}\n"));
    }
    Ok((chunks, cache_path))
}

/// 为缺 embedding 的 chunk 批量计算向量并回写缓存。provider/model 变化时整体重算。
fn ensure_chunk_embeddings(
    chunks: &mut [LoreChunk],
    provider: &dyn embed::EmbeddingProvider,
    model_key: &str,
    cache_path: &Path,
) {
    let existing: Option<LoreIndexCache> = fs::read_to_string(cache_path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    let model_changed =
        existing.as_ref().and_then(|c| c.embedding_model.as_deref()) != Some(model_key);
    if model_changed {
        for chunk in chunks.iter_mut() {
            chunk.embedding = None;
        }
    }
    let to_embed: Vec<usize> = chunks
        .iter()
        .enumerate()
        .filter(|(_, c)| c.embedding.is_none())
        .map(|(i, _)| i)
        .collect();
    if to_embed.is_empty() {
        return;
    }
    let texts: Vec<&str> = to_embed.iter().map(|&i| chunks[i].text.as_str()).collect();
    if let Ok(vectors) = provider.embed(&texts) {
        for (idx, vec) in to_embed.into_iter().zip(vectors.into_iter()) {
            chunks[idx].embedding = Some(vec);
        }
    }
    // 回写缓存（保留原 metadata，更新 chunks 与 embedding_model）
    let (version, pack_id, files) = existing
        .as_ref()
        .map(|c| (c.version, c.pack_id.clone(), c.files.clone()))
        .unwrap_or((LORE_INDEX_VERSION, String::new(), Vec::new()));
    let cache = LoreIndexCache {
        version,
        pack_id,
        files,
        chunks: chunks.to_vec(),
        embedding_model: Some(model_key.to_string()),
    };
    if let Some(parent) = cache_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(&cache) {
        let _ = fs::write(cache_path, format!("{text}\n"));
    }
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
        embedding: None,
    });
}

fn score_all_lore_hits(
    chunks: &[LoreChunk],
    query: &str,
    provider: Option<&dyn embed::EmbeddingProvider>,
    hybrid_weight: f32,
) -> Vec<LoreHit> {
    let terms = query_terms(query);
    if terms.is_empty() {
        return Vec::new();
    }
    let norm = normalize_search_text(query);
    let stats = lore_search_stats(chunks);

    // 稀疏分（BM25 + 短语/元数据加权）
    let sparse: Vec<f32> = chunks
        .iter()
        .map(|chunk| score_lore_chunk(chunk, &norm, &terms, &stats))
        .collect();

    // 稠密分（余弦相似度）。provider 不可用或 chunk 缺 embedding 时为 None。
    let dense: Vec<Option<f32>> = if let Some(p) = provider {
        match p.embed(&[query]) {
            Ok(vectors) => {
                let q = vectors.into_iter().next().unwrap_or_default();
                chunks
                    .iter()
                    .map(|chunk| chunk.embedding.as_ref().map(|e| embed::cosine(e, &q)))
                    .collect()
            }
            Err(_) => chunks.iter().map(|_| None).collect(),
        }
    } else {
        chunks.iter().map(|_| None).collect()
    };

    let has_dense = dense.iter().any(|d| d.is_some());
    if !has_dense {
        // 纯稀疏路径：保持原语义，仅 score>0 入选
        let mut hits: Vec<LoreHit> = chunks
            .iter()
            .zip(sparse.iter())
            .filter_map(|(chunk, &score)| {
                (score > 0.0).then(|| LoreHit {
                    score,
                    chunk: chunk.clone(),
                    dense_score: None,
                })
            })
            .collect();
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
        return hits;
    }

    // 混合路径：RRF 融合 sparse/dense 排名，按 hybrid_weight 加权
    let n = chunks.len();
    let mut sparse_order = (0..n).collect::<Vec<_>>();
    sparse_order.sort_by(|&a, &b| {
        sparse[b]
            .partial_cmp(&sparse[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut dense_order = (0..n).collect::<Vec<_>>();
    dense_order.sort_by(|&a, &b| {
        dense[b]
            .unwrap_or(-1.0)
            .partial_cmp(&dense[a].unwrap_or(-1.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let sparse_rank = {
        let mut r = vec![usize::MAX; n];
        for (i, &idx) in sparse_order.iter().enumerate() {
            r[idx] = i;
        }
        r
    };
    let dense_rank = {
        let mut r = vec![usize::MAX; n];
        for (i, &idx) in dense_order.iter().enumerate() {
            r[idx] = i;
        }
        r
    };
    let w = hybrid_weight.clamp(0.0, 1.0);
    let fused = embed::rrf_fuse(&sparse_rank, &dense_rank, 60, w);
    let mut hits: Vec<LoreHit> = chunks
        .iter()
        .enumerate()
        .filter_map(|(i, chunk)| {
            // 至少一路上榜，或稀疏分>0，才算命中
            let has_signal =
                sparse_rank[i] != usize::MAX || dense_rank[i] != usize::MAX || sparse[i] > 0.0;
            if !has_signal {
                return None;
            }
            Some(LoreHit {
                score: fused[i],
                chunk: chunk.clone(),
                dense_score: dense[i],
            })
        })
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                b.dense_score
                    .unwrap_or(-1.0)
                    .partial_cmp(&a.dense_score.unwrap_or(-1.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.chunk.source.cmp(&b.chunk.source))
            .then_with(|| a.chunk.chunk_index.cmp(&b.chunk.chunk_index))
    });
    hits
}

fn matched_terms_for(chunk: &LoreChunk, query: &str, terms: &[String]) -> Vec<String> {
    let title = normalize_search_text(&chunk.title);
    let heading = normalize_search_text(chunk.heading.as_deref().unwrap_or_default());
    let tags = normalize_search_text(&chunk.tags.join(" "));
    let keywords = normalize_search_text(&chunk.keywords.join(" "));
    let text = normalize_search_text(&chunk.text);
    let mut matched = Vec::new();
    if !query.is_empty() && text.contains(query) {
        matched.push(query.to_string());
    }
    for term in terms {
        if title.contains(term)
            || heading.contains(term)
            || tags.contains(term)
            || keywords.contains(term)
            || text.contains(term)
        {
            matched.push(term.clone());
        }
    }
    matched
}

fn select_lore_hits(
    chunks: &[LoreChunk],
    query: &str,
    provider: Option<&dyn embed::EmbeddingProvider>,
    hybrid_weight: f32,
) -> Vec<LoreHit> {
    let mut hits = score_all_lore_hits(chunks, query, provider, hybrid_weight);
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
            live2d: None,
            character: None,
            runtime: None,
            lorebook: Vec::new(),
            credits: Vec::new(),
            license: None,
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

        let context = lorebook_context(&packs, &data, "demo", Some("银钟塔后来怎么了"), None, 0.5);
        assert!(context.contains("retrieved lore snippets"));
        assert!(context.contains("月亮城年表"));
        assert!(context.contains("银钟塔在雨夜停摆"));
        assert!(!context.contains("甜点摊"));
        assert!(lore_index_cache_path(&data, "demo").exists());

        let nested = lorebook_context(&packs, &data, "demo", Some("赤桥战役在哪一章"), None, 0.5);
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
