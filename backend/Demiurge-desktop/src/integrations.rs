//! 外部 Agent 资源接入：Skill、会话历史和安全配置。
//!
//! 这个模块是一个受限的适配层，不改变 Agent 主循环。外部目录只读扫描，
//! 导入时复制到 Demiurge 自己的目录；不会执行 Skill 内容，也不会把 API key、
//! hooks 或权限配置带进 Demiurge。

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Cursor};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_SKILL_FILE_BYTES: u64 = 64 * 1024;
const MAX_IMPORT_BYTES: u64 = 50 * 1024 * 1024;
const MAX_IMPORT_FILES: usize = 512;
const MAX_SESSION_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_SESSION_MESSAGES: usize = 5_000;
const MAX_SESSION_FILES: usize = 4_096;
const MAX_SESSION_SCAN_DEPTH: usize = 8;
const MAX_MESSAGE_CHARS: usize = 200_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExternalProvider {
    Codex,
    Claude,
}

impl ExternalProvider {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SkillCandidate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub path: String,
    pub managed: bool,
    pub enabled: bool,
    pub imported_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct SkillRegistry {
    version: u32,
    #[serde(default)]
    skills: Vec<ManagedSkill>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ManagedSkill {
    id: String,
    name: String,
    description: String,
    directory: String,
    source: String,
    source_path: String,
    imported_at: u64,
    #[serde(default = "default_true")]
    enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExternalSessionMeta {
    pub id: String,
    pub provider: ExternalProvider,
    pub title: String,
    pub summary: Option<String>,
    pub project_dir: Option<String>,
    pub created_at: Option<u64>,
    pub last_active_at: Option<u64>,
    pub source_path: String,
    pub message_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExternalSessionMessage {
    pub role: String,
    pub content: String,
    pub timestamp: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExternalConfigSummary {
    pub provider: ExternalProvider,
    pub kind: String,
    pub path: String,
    pub available: bool,
    pub safe_keys: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportedConfig {
    pub provider: ExternalProvider,
    pub kind: String,
    pub source_path: String,
    pub destination_path: String,
    pub safe_values: BTreeMap<String, String>,
    pub imported_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IntegrationSnapshot {
    pub skills: Vec<SkillCandidate>,
    pub sessions: Vec<ExternalSessionMeta>,
    pub configs: Vec<ExternalConfigSummary>,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarketSkill {
    pub key: String,
    pub name: String,
    pub directory: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub repo_branch: String,
    pub installs: u64,
    pub readme_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarketSearchResult {
    pub skills: Vec<MarketSkill>,
    pub total_count: usize,
    pub query: String,
}

#[derive(Clone, Debug, Deserialize)]
struct MarketApiResponse {
    query: String,
    #[serde(default)]
    skills: Vec<MarketApiSkill>,
    #[serde(default)]
    count: usize,
}

#[derive(Clone, Debug, Deserialize)]
struct MarketApiSkill {
    id: String,
    #[serde(rename = "skillId")]
    skill_id: String,
    name: String,
    #[serde(default)]
    installs: u64,
    source: String,
}

pub fn scan(sandbox: &Path, data_dir: &Path) -> IntegrationSnapshot {
    let mut diagnostics = Vec::new();
    let skills = scan_skills(sandbox, data_dir, &mut diagnostics);
    let sessions = scan_sessions(&mut diagnostics);
    let configs = scan_configs();
    IntegrationSnapshot {
        skills,
        sessions,
        configs,
        diagnostics,
    }
}

pub fn scan_skills(
    sandbox: &Path,
    data_dir: &Path,
    diagnostics: &mut Vec<String>,
) -> Vec<SkillCandidate> {
    let registry = load_registry(data_dir, diagnostics);
    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for managed in &registry.skills {
        let path = data_dir.join("skills").join(&managed.directory);
        if !is_real_directory(&path) || !is_real_file(&path.join("SKILL.md")) {
            diagnostics.push(format!("已登记的 Skill 不存在：{}", path.display()));
            continue;
        }
        let key = canonical_key(&path);
        seen.insert(key);
        result.push(SkillCandidate {
            id: managed.id.clone(),
            name: managed.name.clone(),
            description: managed.description.clone(),
            source: managed.source.clone(),
            path: path.display().to_string(),
            managed: true,
            enabled: managed.enabled,
            imported_at: Some(managed.imported_at),
        });
    }

    for (source, root) in skill_roots(sandbox, data_dir) {
        for path in collect_skill_dirs(&root) {
            let key = canonical_key(&path);
            if !seen.insert(key) {
                continue;
            }
            let Some((name, description)) = read_skill_metadata(&path) else {
                diagnostics.push(format!("跳过无效 Skill：{}", path.display()));
                continue;
            };
            result.push(SkillCandidate {
                id: format!("{source}:{}", slug(&name)),
                name,
                description,
                source: source.clone(),
                path: path.display().to_string(),
                managed: false,
                enabled: true,
                imported_at: None,
            });
        }
    }

    result.sort_by_key(|candidate| candidate.name.to_lowercase());
    result
}

pub fn import_skill(
    data_dir: &Path,
    source_path: &Path,
    source: Option<String>,
) -> Result<SkillCandidate, String> {
    let source_path = fs::canonicalize(source_path)
        .map_err(|e| format!("无法读取 Skill 目录 {}：{e}", source_path.display()))?;
    if !is_real_directory(&source_path) || !is_real_file(&source_path.join("SKILL.md")) {
        return Err("导入目标必须是包含 SKILL.md 的目录".to_string());
    }
    let (name, description) = read_skill_metadata(&source_path)
        .ok_or_else(|| "SKILL.md 无法读取或 frontmatter 无效".to_string())?;
    let mut registry = load_registry(data_dir, &mut Vec::new());
    let base_directory = safe_directory_name(&name);
    let source_key = canonical_key(&source_path);
    let same_source = registry
        .skills
        .iter()
        .find(|item| canonical_key(Path::new(&item.source_path)) == source_key)
        .map(|item| item.directory.clone());
    let directory = if let Some(directory) = same_source.as_ref() {
        directory.clone()
    } else if data_dir.join("skills").join(&base_directory).exists()
        || registry
            .skills
            .iter()
            .any(|item| item.directory == base_directory)
    {
        format!("{}-{}", base_directory, short_hash(&source_key))
    } else {
        base_directory
    };
    let managed_root = data_dir.join("skills");
    let target = managed_root.join(&directory);
    if !is_safe_child(&target, &managed_root) {
        return Err("Skill 目录名不安全".to_string());
    }
    if target.exists() && same_source.is_none() {
        return Err("Skill 目录冲突，拒绝覆盖；请先重命名来源或移除旧条目".to_string());
    }

    let temp = managed_root.join(format!(".import-{}-{}", directory, now_millis()));
    if temp.exists() {
        fs::remove_dir_all(&temp).map_err(|e| format!("清理导入临时目录失败：{e}"))?;
    }
    if let Err(error) = copy_tree(&source_path, &temp) {
        let _ = fs::remove_dir_all(&temp);
        return Err(error);
    }
    if target.exists() {
        fs::remove_dir_all(&target).map_err(|e| format!("替换旧 Skill 失败：{e}"))?;
    }
    fs::create_dir_all(&managed_root).map_err(|e| format!("创建 Skill 目录失败：{e}"))?;
    fs::rename(&temp, &target).map_err(|e| format!("提交 Skill 导入失败：{e}"))?;

    let record = ManagedSkill {
        id: format!("local:{directory}"),
        name: name.clone(),
        description: description.clone(),
        directory: directory.clone(),
        source: source.unwrap_or_else(|| "external".to_string()),
        source_path: source_path.display().to_string(),
        imported_at: now_millis(),
        enabled: true,
    };
    registry.skills.retain(|item| item.id != record.id);
    registry.skills.push(record.clone());
    save_registry(data_dir, &registry)?;
    Ok(SkillCandidate {
        id: record.id,
        name,
        description,
        source: record.source,
        path: target.display().to_string(),
        managed: true,
        enabled: true,
        imported_at: Some(record.imported_at),
    })
}

pub fn set_skill_enabled(data_dir: &Path, id: &str, enabled: bool) -> Result<(), String> {
    let mut registry = load_registry(data_dir, &mut Vec::new());
    let record = registry
        .skills
        .iter_mut()
        .find(|item| item.id == id)
        .ok_or_else(|| "只能管理已导入的 Skill".to_string())?;
    record.enabled = enabled;
    save_registry(data_dir, &registry)
}

/// Agent 上下文选择使用的轻量开关。未进入注册表的旧 Skill 保持兼容并默认启用。
pub fn skill_enabled(data_dir: &Path, skill_dir: &Path) -> bool {
    let mut diagnostics = Vec::new();
    let registry = load_registry(data_dir, &mut diagnostics);
    let canonical = canonical_key(skill_dir);
    registry
        .skills
        .iter()
        .find_map(|item| {
            let path = data_dir.join("skills").join(&item.directory);
            (canonical_key(&path) == canonical).then_some(item.enabled)
        })
        .unwrap_or(true)
}

pub fn remove_skill(data_dir: &Path, id: &str) -> Result<(), String> {
    let mut registry = load_registry(data_dir, &mut Vec::new());
    let record = registry
        .skills
        .iter()
        .find(|item| item.id == id)
        .cloned()
        .ok_or_else(|| "Skill 不在 Demiurge 管理目录中".to_string())?;
    let root = data_dir.join("skills");
    let target = root.join(&record.directory);
    if !is_safe_child(&target, &root) {
        return Err("拒绝删除目录外的 Skill".to_string());
    }
    if target.exists() {
        fs::remove_dir_all(&target).map_err(|e| format!("删除 Skill 失败：{e}"))?;
    }
    registry.skills.retain(|item| item.id != id);
    save_registry(data_dir, &registry)
}

pub fn scan_sessions(diagnostics: &mut Vec<String>) -> Vec<ExternalSessionMeta> {
    let mut sessions = Vec::new();
    for (provider, root) in session_roots() {
        let mut files = Vec::new();
        collect_jsonl_files(&root, &mut files, 0);
        for path in files {
            match parse_session(&provider, &path) {
                Ok(Some(meta)) => sessions.push(meta),
                Ok(None) => {}
                Err(error) => diagnostics.push(error),
            }
        }
    }
    sessions.sort_by_key(|session| Reverse(session.last_active_at));
    sessions
}

pub fn load_session_messages(
    provider: &ExternalProvider,
    source_path: &Path,
) -> Result<Vec<ExternalSessionMessage>, String> {
    let roots = session_roots()
        .into_iter()
        .filter(|(candidate, _)| candidate == provider)
        .map(|(_, root)| root)
        .collect::<Vec<_>>();
    validate_under_roots(source_path, &roots)?;
    let metadata = fs::metadata(source_path).map_err(|e| format!("无法读取会话文件：{e}"))?;
    if metadata.len() > MAX_SESSION_FILE_BYTES {
        return Err("会话文件超过 32 MB，拒绝一次性导入".to_string());
    }
    let file = File::open(source_path).map_err(|e| format!("打开会话文件失败：{e}"))?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();
    for (line_number, line) in reader.lines().enumerate() {
        let line = line.map_err(|e| format!("读取会话文件失败：{e}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let value = serde_json::from_str::<Value>(&line)
            .map_err(|e| format!("会话文件第 {} 行损坏，拒绝导入：{e}", line_number + 1))?;
        if let Some(message) = parse_message(provider, &value) {
            messages.push(message);
            if messages.len() >= MAX_SESSION_MESSAGES {
                break;
            }
        }
    }
    Ok(messages)
}

pub fn import_session(
    provider: &ExternalProvider,
    source_path: &Path,
) -> Result<(ExternalSessionMeta, Vec<ExternalSessionMessage>), String> {
    let messages = load_session_messages(provider, source_path)?;
    if messages.is_empty() {
        return Err("会话没有可导入的文本消息".to_string());
    }
    let meta = parse_session(provider, source_path)?
        .ok_or_else(|| "会话文件不是可导入的主会话".to_string())?;
    Ok((meta, messages))
}

pub fn scan_configs() -> Vec<ExternalConfigSummary> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    let candidates = [
        (
            ExternalProvider::Codex,
            "codex-config",
            home.join(".codex/config.toml"),
        ),
        (
            ExternalProvider::Claude,
            "claude-settings",
            home.join(".claude/settings.json"),
        ),
        (
            ExternalProvider::Claude,
            "claude-config",
            home.join(".claude.json"),
        ),
    ];
    candidates
        .into_iter()
        .map(|(provider, kind, path)| {
            let safe_keys = read_safe_config_values(&provider, &path)
                .map(|values| values.keys().cloned().collect())
                .unwrap_or_default();
            ExternalConfigSummary {
                provider,
                kind: kind.to_string(),
                available: path.is_file(),
                path: path.display().to_string(),
                safe_keys,
            }
        })
        .collect()
}

pub fn import_config(
    data_dir: &Path,
    provider: ExternalProvider,
    kind: &str,
) -> Result<ImportedConfig, String> {
    let summary = scan_configs()
        .into_iter()
        .find(|item| item.provider == provider && item.kind == kind)
        .ok_or_else(|| "未找到该外部配置".to_string())?;
    if !summary.available {
        return Err("外部配置文件不存在".to_string());
    }
    let values = read_safe_config_values(&provider, Path::new(&summary.path))
        .ok_or_else(|| "配置无法解析，或只包含敏感字段".to_string())?;
    let imported = ImportedConfig {
        provider,
        kind: kind.to_string(),
        source_path: summary.path,
        destination_path: data_dir
            .join("integrations")
            .join(format!("{kind}.json"))
            .display()
            .to_string(),
        safe_values: values,
        imported_at: now_millis(),
    };
    let path = PathBuf::from(&imported.destination_path);
    let text = serde_json::to_string_pretty(&imported).map_err(|e| e.to_string())?;
    crate::store::atomic_write_text(&path, &text, true)?;
    Ok(imported)
}

pub async fn search_market(
    client: &Client,
    query: &str,
    limit: usize,
) -> Result<MarketSearchResult, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("市场搜索关键词不能为空".to_string());
    }
    let limit = limit.clamp(1, 50);
    let response = client
        .get("https://skills.sh/api/search")
        .query(&[("q", query), ("limit", &limit.to_string()), ("offset", "0")])
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Skill 市场请求失败：{e}"))?
        .error_for_status()
        .map_err(|e| format!("Skill 市场返回错误：{e}"))?
        .json::<MarketApiResponse>()
        .await
        .map_err(|e| format!("Skill 市场响应无法解析：{e}"))?;
    let skills = response
        .skills
        .into_iter()
        .filter_map(|item| {
            let (owner, repo) = item.source.split_once('/')?;
            if !valid_repo_part(owner) || !valid_repo_part(repo) {
                return None;
            }
            Some(MarketSkill {
                key: item.id,
                name: item.name,
                directory: item.skill_id,
                repo_owner: owner.to_string(),
                repo_name: repo.to_string(),
                repo_branch: "main".to_string(),
                installs: item.installs,
                readme_url: Some(format!("https://github.com/{owner}/{repo}")),
            })
        })
        .collect();
    Ok(MarketSearchResult {
        skills,
        total_count: response.count,
        query: response.query,
    })
}

pub async fn install_market_skill(
    client: &Client,
    data_dir: &Path,
    skill: &MarketSkill,
) -> Result<SkillCandidate, String> {
    if !valid_repo_part(&skill.repo_owner)
        || !valid_repo_part(&skill.repo_name)
        || !valid_repo_part(&skill.repo_branch)
        || !is_safe_relative(Path::new(&skill.directory))
    {
        return Err("市场 Skill 坐标不安全".to_string());
    }
    let url = format!(
        "https://codeload.github.com/{}/{}/zip/refs/heads/{}",
        skill.repo_owner, skill.repo_name, skill.repo_branch
    );
    let bytes = client
        .get(url)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("下载市场 Skill 失败：{e}"))?
        .error_for_status()
        .map_err(|e| format!("市场 Skill 下载返回错误：{e}"))?
        .bytes()
        .await
        .map_err(|e| format!("读取市场 Skill 压缩包失败：{e}"))?;
    if bytes.len() as u64 > MAX_IMPORT_BYTES {
        return Err("市场 Skill 压缩包超过 50 MB".to_string());
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("市场 Skill 压缩包无效：{e}"))?;
    let temp_root = data_dir.join(".skill-market-import");
    if temp_root.exists() {
        fs::remove_dir_all(&temp_root).map_err(|e| format!("清理市场临时目录失败：{e}"))?;
    }
    fs::create_dir_all(&temp_root).map_err(|e| format!("创建市场临时目录失败：{e}"))?;
    let suffix = format!("/{}/SKILL.md", skill.directory.replace('\\', "/"));
    let prefix = (0..archive.len())
        .find_map(|index| {
            let file = archive.by_index(index).ok()?;
            let name = file.name().replace('\\', "/");
            name.ends_with(&suffix)
                .then(|| name[..name.len() - suffix.len()].to_string())
        })
        .ok_or_else(|| "市场压缩包中找不到目标 SKILL.md".to_string())?;
    let source = temp_root.join("skill");
    fs::create_dir_all(&source).map_err(|e| format!("创建市场 Skill 临时目录失败：{e}"))?;
    let wanted_prefix = format!("{}{}/", prefix, skill.directory.replace('\\', "/"));
    let mut total = 0u64;
    let mut files = 0usize;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|e| format!("读取市场压缩包条目失败：{e}"))?;
        let name = file.name().replace('\\', "/");
        let Some(relative) = name.strip_prefix(&wanted_prefix) else {
            continue;
        };
        let relative_path = Path::new(relative);
        if relative.is_empty() || !is_safe_relative(relative_path) {
            continue;
        }
        if file.is_dir() {
            continue;
        }
        files += 1;
        total = total.saturating_add(file.size());
        if files > MAX_IMPORT_FILES || total > MAX_IMPORT_BYTES {
            return Err("市场 Skill 文件数量或体积超限".to_string());
        }
        let target = source.join(relative_path);
        if !is_safe_child(&target, &source) {
            return Err("市场压缩包路径不安全".to_string());
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut output = File::create(&target).map_err(|e| format!("写入市场 Skill 失败：{e}"))?;
        std::io::copy(&mut file, &mut output).map_err(|e| format!("解压市场 Skill 失败：{e}"))?;
    }
    let result = import_skill(data_dir, &source, Some("skills.sh".to_string()));
    let _ = fs::remove_dir_all(&temp_root);
    result
}

fn skill_roots(sandbox: &Path, data_dir: &Path) -> Vec<(String, PathBuf)> {
    let mut roots = vec![("demiurge".to_string(), data_dir.join("skills"))];
    roots.push(("project-claude".to_string(), sandbox.join(".claude/skills")));
    roots.push(("project-codex".to_string(), sandbox.join(".codex/skills")));
    roots.push(("project-agents".to_string(), sandbox.join(".agents/skills")));
    if let Some(home) = home_dir() {
        roots.push(("claude".to_string(), home.join(".claude/skills")));
        roots.push(("codex".to_string(), home.join(".codex/skills")));
        roots.push(("agents".to_string(), home.join(".agents/skills")));
    }
    roots
}

fn session_roots() -> Vec<(ExternalProvider, PathBuf)> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    vec![
        (ExternalProvider::Claude, home.join(".claude/projects")),
        (ExternalProvider::Codex, home.join(".codex/sessions")),
        (
            ExternalProvider::Codex,
            home.join(".codex/archived_sessions"),
        ),
    ]
}

fn parse_session(
    provider: &ExternalProvider,
    path: &Path,
) -> Result<Option<ExternalSessionMeta>, String> {
    let metadata =
        fs::metadata(path).map_err(|e| format!("读取会话元数据失败 {}：{e}", path.display()))?;
    if metadata.len() > MAX_SESSION_FILE_BYTES {
        return Ok(None);
    }
    let (head, tail) = head_tail_lines(path)?;
    let mut id = None;
    let mut project_dir = None;
    let mut created_at = None;
    let mut first_user = None;
    let mut last_active = None;
    let mut summary = None;
    let mut message_count = 0;
    for line in head.iter().chain(tail.iter()) {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if let Some(ts) = timestamp(&value) {
            created_at.get_or_insert(ts);
            last_active = Some(last_active.unwrap_or(ts).max(ts));
        }
        match provider {
            ExternalProvider::Claude => {
                if value.get("isMeta").and_then(Value::as_bool) == Some(true) {
                    continue;
                }
                if id.is_none() {
                    id = value
                        .get("sessionId")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                }
                if project_dir.is_none() {
                    project_dir = value.get("cwd").and_then(Value::as_str).map(str::to_string);
                }
                if value.get("type").and_then(Value::as_str) == Some("custom-title")
                    && summary.is_none()
                {
                    summary = value
                        .get("customTitle")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                }
            }
            ExternalProvider::Codex => {
                if value.get("type").and_then(Value::as_str) == Some("session_meta") {
                    let payload = value.get("payload").unwrap_or(&Value::Null);
                    if payload
                        .get("source")
                        .and_then(|v| v.get("subagent"))
                        .is_some()
                    {
                        return Ok(None);
                    }
                    id = id.or_else(|| {
                        payload
                            .get("id")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    });
                    project_dir = project_dir.or_else(|| {
                        payload
                            .get("cwd")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    });
                    created_at =
                        created_at.or_else(|| payload.get("timestamp").and_then(parse_timestamp));
                }
            }
        }
        if let Some(message) = parse_message(provider, &value) {
            message_count += 1;
            if first_user.is_none() && message.role == "user" && !message.content.trim().is_empty()
            {
                first_user = Some(message.content.clone());
            }
            if message.role == "assistant" && !message.content.trim().is_empty() {
                summary = Some(message.content);
            }
        }
    }
    let id = id.or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_string)
    });
    let Some(id) = id else { return Ok(None) };
    let title = first_user
        .as_deref()
        .map(|s| truncate(s, 80))
        .or_else(|| {
            project_dir
                .as_deref()
                .and_then(|s| Path::new(s).file_name())
                .and_then(|s| s.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| id.clone());
    Ok(Some(ExternalSessionMeta {
        id,
        provider: provider.clone(),
        title,
        summary: summary.map(|s| truncate(&s, 160)),
        project_dir,
        created_at,
        last_active_at: last_active,
        source_path: path.display().to_string(),
        message_count,
    }))
}

fn parse_message(provider: &ExternalProvider, value: &Value) -> Option<ExternalSessionMessage> {
    let (role, content) = match provider {
        ExternalProvider::Claude => {
            if value.get("isMeta").and_then(Value::as_bool) == Some(true) {
                return None;
            }
            let message = value.get("message")?;
            let mut role = message
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let content_value = message.get("content")?;
            if role == "user"
                && content_value.as_array().is_some_and(|items| {
                    !items.is_empty()
                        && items.iter().all(|item| {
                            item.get("type").and_then(Value::as_str) == Some("tool_result")
                        })
                })
            {
                role = "tool".to_string();
            }
            (role, extract_text(content_value))
        }
        ExternalProvider::Codex => {
            if value.get("type").and_then(Value::as_str) != Some("response_item") {
                return None;
            }
            let payload = value.get("payload")?;
            match payload.get("type").and_then(Value::as_str).unwrap_or("") {
                "message" => (
                    payload
                        .get("role")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .to_string(),
                    extract_text(payload.get("content")?),
                ),
                "function_call" => (
                    "assistant".to_string(),
                    format!(
                        "[Tool: {}]",
                        payload
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown")
                    ),
                ),
                "function_call_output" => (
                    "tool".to_string(),
                    payload
                        .get("output")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                ),
                _ => return None,
            }
        }
    };
    let content = truncate(&content, MAX_MESSAGE_CHARS);
    (!content.trim().is_empty()).then_some(ExternalSessionMessage {
        role,
        content,
        timestamp: timestamp(value),
    })
}

fn read_safe_config_values(
    provider: &ExternalProvider,
    path: &Path,
) -> Option<BTreeMap<String, String>> {
    let raw = fs::read_to_string(path).ok()?;
    if raw.len() > 512 * 1024 {
        return None;
    }
    let allowed = [
        "model",
        "model_provider",
        "approval_policy",
        "sandbox_mode",
        "reasoning_effort",
        "alwaysThinkingEnabled",
    ];
    let mut values = BTreeMap::new();
    if matches!(provider, ExternalProvider::Claude) {
        let value = serde_json::from_str::<Value>(&raw).ok()?;
        for key in allowed {
            if let Some(value) = value.get(key).and_then(safe_scalar) {
                values.insert(key.to_string(), value);
            }
        }
    } else {
        for line in raw.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            if allowed.contains(&key) {
                let value = value.trim().trim_matches(['"', '\'']);
                if !value.is_empty() && !value.to_ascii_lowercase().contains("key") {
                    values.insert(key.to_string(), value.to_string());
                }
            }
        }
    }
    (!values.is_empty()).then_some(values)
}

fn safe_scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(v) if !v.trim().is_empty() => Some(v.trim().to_string()),
        Value::Bool(v) => Some(v.to_string()),
        Value::Number(v) => Some(v.to_string()),
        _ => None,
    }
}

fn load_registry(data_dir: &Path, diagnostics: &mut Vec<String>) -> SkillRegistry {
    let path = data_dir.join("skill_registry.json");
    match fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
    {
        Some(registry) => registry,
        None => {
            if path.is_file() {
                diagnostics.push(format!(
                    "Skill 注册表损坏，使用空注册表：{}",
                    path.display()
                ));
            }
            SkillRegistry {
                version: 1,
                skills: Vec::new(),
            }
        }
    }
}

fn save_registry(data_dir: &Path, registry: &SkillRegistry) -> Result<(), String> {
    let text = serde_json::to_string_pretty(registry).map_err(|e| e.to_string())?;
    crate::store::atomic_write_text(&data_dir.join("skill_registry.json"), &text, true)
}

fn collect_skill_dirs(root: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if !is_real_directory(root) {
        return result;
    }
    if is_real_file(&root.join("SKILL.md")) {
        result.push(root.to_path_buf());
    }
    let Ok(entries) = fs::read_dir(root) else {
        return result;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if is_real_directory(&path) && is_real_file(&path.join("SKILL.md")) {
            result.push(path);
        }
    }
    result
}

fn read_skill_metadata(dir: &Path) -> Option<(String, String)> {
    let raw = fs::read_to_string(dir.join("SKILL.md")).ok()?;
    if raw.len() as u64 > MAX_SKILL_FILE_BYTES {
        return None;
    }
    let fallback = dir.file_name()?.to_string_lossy().to_string();
    let (name, description) = if let Some(rest) = raw
        .strip_prefix("---\n")
        .or_else(|| raw.strip_prefix("---\r\n"))
    {
        let (front, _) = rest.split_once("\n---")?;
        let parsed = serde_yaml::from_str::<SkillFrontMatter>(front).ok()?;
        (
            parsed.name.unwrap_or(fallback),
            parsed.description.unwrap_or_default(),
        )
    } else {
        (fallback, String::new())
    };
    let name = name.trim().to_string();
    (!name.is_empty()).then_some((name, description.trim().to_string()))
}

#[derive(Default, Deserialize)]
struct SkillFrontMatter {
    name: Option<String>,
    description: Option<String>,
}

fn copy_tree(source: &Path, target: &Path) -> Result<(), String> {
    let mut total = 0u64;
    let mut files = 0usize;
    fn walk(
        source: &Path,
        target: &Path,
        total: &mut u64,
        files: &mut usize,
    ) -> Result<(), String> {
        fs::create_dir_all(target).map_err(|e| format!("创建导入目录失败：{e}"))?;
        for entry in fs::read_dir(source).map_err(|e| format!("读取 Skill 目录失败：{e}"))? {
            let entry = entry.map_err(|e| format!("读取 Skill 条目失败：{e}"))?;
            let file_type = entry
                .file_type()
                .map_err(|e| format!("读取 Skill 类型失败：{e}"))?;
            if file_type.is_symlink() {
                continue;
            }
            let from = entry.path();
            let to = target.join(entry.file_name());
            if !is_safe_child(&to, target) {
                return Err("Skill 文件路径不安全".to_string());
            }
            if file_type.is_dir() {
                walk(&from, &to, total, files)?;
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let size = fs::metadata(&from).map_err(|e| e.to_string())?.len();
            *total = total.saturating_add(size);
            *files += 1;
            if *total > MAX_IMPORT_BYTES || *files > MAX_IMPORT_FILES {
                return Err("Skill 文件数量或体积超限".to_string());
            }
            fs::copy(&from, &to).map_err(|e| format!("复制 Skill 文件失败：{e}"))?;
        }
        Ok(())
    }
    walk(source, target, &mut total, &mut files)
}

fn head_tail_lines(path: &Path) -> Result<(Vec<String>, Vec<String>), String> {
    let file = File::open(path).map_err(|e| format!("打开会话文件失败 {}：{e}", path.display()))?;
    let reader = BufReader::new(file);
    let mut head = Vec::new();
    let mut tail = VecDeque::with_capacity(32);
    for line in reader.lines() {
        let line = line.map_err(|e| format!("读取会话文件失败：{e}"))?;
        if head.len() < 12 {
            head.push(line.clone());
        }
        if tail.len() == 32 {
            tail.pop_front();
        }
        tail.push_back(line);
    }
    Ok((head, tail.into_iter().collect()))
}

fn collect_jsonl_files(root: &Path, files: &mut Vec<PathBuf>, depth: usize) {
    if depth > MAX_SESSION_SCAN_DEPTH
        || files.len() >= MAX_SESSION_FILES
        || !is_real_directory(root)
    {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        if files.len() >= MAX_SESSION_FILES {
            return;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            collect_jsonl_files(&path, files, depth + 1);
        } else if file_type.is_file() && path.extension().and_then(|v| v.to_str()) == Some("jsonl")
        {
            files.push(path);
        }
    }
}

fn is_real_directory(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_dir() && !metadata.file_type().is_symlink())
        .unwrap_or(false)
}

fn is_real_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_file() && !metadata.file_type().is_symlink())
        .unwrap_or(false)
}

fn validate_under_roots(path: &Path, roots: &[PathBuf]) -> Result<(), String> {
    let canonical = fs::canonicalize(path).map_err(|e| format!("外部路径无法解析：{e}"))?;
    let canonical_path_key = canonical_key(&canonical);
    if roots
        .iter()
        .filter_map(|root| fs::canonicalize(root).ok())
        .any(|root| {
            let root_key = canonical_key(&root);
            let prefix = format!("{root_key}{}", std::path::MAIN_SEPARATOR);
            canonical.is_file()
                && (canonical_path_key == root_key || canonical_path_key.starts_with(&prefix))
        })
    {
        Ok(())
    } else {
        Err("拒绝读取不在 Codex/Claude 会话目录下的路径".to_string())
    }
}

fn is_safe_child(path: &Path, root: &Path) -> bool {
    let relative = path.strip_prefix(root).ok();
    relative.is_some_and(is_safe_relative)
}

fn is_safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn valid_repo_part(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn safe_directory_name(value: &str) -> String {
    slug(value).chars().take(80).collect()
}

fn short_hash(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}").chars().take(10).collect()
}

fn slug(value: &str) -> String {
    let mut output = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            output.push(ch.to_ascii_lowercase());
        } else if !output.ends_with('-') {
            output.push('-');
        }
    }
    let output = output.trim_matches('-').to_string();
    if output.is_empty() {
        "imported-skill".to_string()
    } else {
        output
    }
}

fn canonical_key(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
        .to_ascii_lowercase()
}

fn extract_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(extract_text)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(map) => match map.get("type").and_then(Value::as_str) {
            Some("text") => map
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            Some("tool_use") => format!(
                "[Tool: {}]",
                map.get("name").and_then(Value::as_str).unwrap_or("unknown")
            ),
            Some("tool_result") => map.get("content").map(extract_text).unwrap_or_default(),
            _ => map
                .get("text")
                .map(extract_text)
                .or_else(|| map.get("content").map(extract_text))
                .unwrap_or_default(),
        },
        _ => String::new(),
    }
}

fn parse_timestamp(value: &Value) -> Option<u64> {
    value
        .as_i64()
        .map(|v| v.max(0) as u64)
        .or_else(|| value.as_u64())
        .or_else(|| {
            value.as_str().and_then(|text| {
                text.parse::<u64>()
                    .ok()
                    .or_else(|| chrono_like_timestamp(text))
            })
        })
}

fn timestamp(value: &Value) -> Option<u64> {
    value.get("timestamp").and_then(parse_timestamp)
}

fn chrono_like_timestamp(value: &str) -> Option<u64> {
    let (date, time) = value.split_once('T')?;
    let mut parts = date.split('-').filter_map(|v| v.parse::<u64>().ok());
    let year = parts.next()?;
    let month = parts.next()?;
    let day = parts.next()?;
    let time = time
        .trim_end_matches('Z')
        .split([':', '.'])
        .filter_map(|v| v.parse::<u64>().ok())
        .collect::<Vec<_>>();
    if time.len() < 3 {
        return None;
    }
    let days = year.saturating_sub(1970) * 365 + month.saturating_sub(1) * 30 + day;
    Some((days * 86_400 + time[0] * 3_600 + time[1] * 60 + time[2]) * 1000)
}

fn truncate(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_messages_parse_claude_tool_results_and_codex_items() {
        let claude = serde_json::json!({"timestamp":"2026-01-01T00:00:00Z","message":{"role":"user","content":[{"type":"tool_result","content":"done"}]}});
        let codex = serde_json::json!({"timestamp":42,"type":"response_item","payload":{"type":"function_call","name":"shell"}});
        assert_eq!(
            parse_message(&ExternalProvider::Claude, &claude)
                .unwrap()
                .role,
            "tool"
        );
        assert_eq!(
            parse_message(&ExternalProvider::Codex, &codex)
                .unwrap()
                .content,
            "[Tool: shell]"
        );
    }

    #[test]
    fn unsafe_relative_paths_are_rejected() {
        assert!(!is_safe_relative(Path::new("../outside")));
        assert!(!is_safe_relative(Path::new("a/../../outside")));
        assert!(is_safe_relative(Path::new("a/b/SKILL.md")));
    }

    #[test]
    fn safe_config_values_never_include_secrets() {
        let root = std::env::temp_dir().join(format!("demiurge_config_{}", now_millis()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("settings.json");
        fs::write(
            &path,
            r#"{"model":"claude-3","apiKey":"secret","alwaysThinkingEnabled":true}"#,
        )
        .unwrap();
        let values = read_safe_config_values(&ExternalProvider::Claude, &path).unwrap();
        assert_eq!(values.get("model").map(String::as_str), Some("claude-3"));
        assert!(!values.contains_key("apiKey"));
        let _ = fs::remove_dir_all(root);
    }
}
