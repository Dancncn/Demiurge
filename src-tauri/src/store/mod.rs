//! 组件 9：持久化。设置 / 多会话写入磁盘，下次启动可恢复。
//! 这就是 MVP 的全部「记忆」——不做向量 RAG。
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::agent::conversation::{Conversation, Message};

pub const DEFAULT_MAX_CONTEXT_CHARS: usize = 24_000;
pub const DEFAULT_MAX_INPUT_TOKENS: usize = 32_000;
pub const DEFAULT_RESERVED_OUTPUT_TOKENS: usize = 4_000;
pub const DEFAULT_AUTO_MEMORY_ENABLED: bool = true;
pub const DEFAULT_VOICE_ENABLED: bool = false;
pub const DEFAULT_COMPUTER_USE_ENABLED: bool = false;

fn default_web_search_provider() -> String {
    "auto".to_string()
}

fn default_webdav_path() -> String {
    "Demiurge".to_string()
}

fn default_provider() -> ProviderKind {
    ProviderKind::DeepSeek
}

fn default_permission_mode() -> PermissionMode {
    PermissionMode::Default
}

fn default_max_context_chars() -> usize {
    DEFAULT_MAX_CONTEXT_CHARS
}

fn default_max_input_tokens() -> usize {
    DEFAULT_MAX_INPUT_TOKENS
}

fn default_reserved_output_tokens() -> usize {
    DEFAULT_RESERVED_OUTPUT_TOKENS
}

fn default_auto_memory_enabled() -> bool {
    DEFAULT_AUTO_MEMORY_ENABLED
}

fn default_voice_enabled() -> bool {
    DEFAULT_VOICE_ENABLED
}

fn default_voice_stt_backend() -> String {
    "none".to_string()
}

fn default_voice_tts_backend() -> String {
    "none".to_string()
}

fn default_computer_use_enabled() -> bool {
    DEFAULT_COMPUTER_USE_ENABLED
}

fn default_ocr_model_source() -> String {
    "modelscope".to_string()
}

fn default_media_provider() -> String {
    "dashscope".to_string()
}

fn default_media_base_url() -> String {
    "https://dashscope.aliyuncs.com".to_string()
}

fn default_image_model() -> String {
    "qwen-image-2.0".to_string()
}

fn default_image_size() -> String {
    "1024*1024".to_string()
}

fn default_tts_model() -> String {
    "qwen3-tts-flash".to_string()
}

fn default_tts_voice() -> String {
    "Cherry".to_string()
}

fn default_mcp_servers() -> Vec<crate::mcp::McpServerConfig> {
    Vec::new()
}

fn default_reasoning_effort() -> ReasoningEffort {
    ReasoningEffort::Auto
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    Plan,
    Default,
    Auto,
    Bypass,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    #[serde(rename = "deepseek")]
    DeepSeek,
    #[serde(rename = "dashscope")]
    DashScope,
    #[serde(rename = "openai")]
    OpenAi,
    #[serde(rename = "openrouter")]
    OpenRouter,
    OpenAiCompatible,
    Local,
    Anthropic,
    Gemini,
    #[serde(rename = "glm")]
    Glm,
    #[serde(rename = "minimax")]
    MiniMax,
    #[serde(rename = "custom")]
    Custom,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    Auto,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl ReasoningEffort {
    pub const LEVELS: [ReasoningEffort; 5] = [
        ReasoningEffort::Low,
        ReasoningEffort::Medium,
        ReasoningEffort::High,
        ReasoningEffort::Xhigh,
        ReasoningEffort::Max,
    ];

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" | "unset" | "default" => Some(ReasoningEffort::Auto),
            "low" => Some(ReasoningEffort::Low),
            "medium" | "med" => Some(ReasoningEffort::Medium),
            "high" => Some(ReasoningEffort::High),
            "xhigh" | "extra_high" | "extra-high" => Some(ReasoningEffort::Xhigh),
            "max" => Some(ReasoningEffort::Max),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            ReasoningEffort::Auto => "auto",
            ReasoningEffort::Low => "low",
            ReasoningEffort::Medium => "medium",
            ReasoningEffort::High => "high",
            ReasoningEffort::Xhigh => "xhigh",
            ReasoningEffort::Max => "max",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            ReasoningEffort::Auto => "Use the provider or model default.",
            ReasoningEffort::Low => "Quick, straightforward reasoning with minimal overhead.",
            ReasoningEffort::Medium => {
                "Balanced reasoning for standard implementation and testing."
            }
            ReasoningEffort::High => {
                "Comprehensive reasoning for complex implementation and verification."
            }
            ReasoningEffort::Xhigh => "Extended reasoning beyond high, short of max.",
            ReasoningEffort::Max => "Maximum reasoning depth where the provider supports it.",
        }
    }

    pub const fn is_auto(self) -> bool {
        matches!(self, ReasoningEffort::Auto)
    }
}

/// 运行时设置。`api_key` 只保留在内存和前端表单里，落盘时会被清空；
/// 实际密钥由 `credentials` 模块写入系统凭据管理器。
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Settings {
    #[serde(default = "default_provider")]
    pub provider: ProviderKind,
    #[serde(default = "default_permission_mode")]
    pub permission_mode: PermissionMode,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub current_pack: String,
    #[serde(default = "default_max_context_chars")]
    pub max_context_chars: usize,
    #[serde(default = "default_max_input_tokens")]
    pub max_input_tokens: usize,
    #[serde(default = "default_reserved_output_tokens")]
    pub reserved_output_tokens: usize,
    #[serde(default = "default_reasoning_effort")]
    pub reasoning_effort: ReasoningEffort,
    #[serde(default = "default_auto_memory_enabled")]
    pub auto_memory_enabled: bool,
    #[serde(default = "default_voice_enabled")]
    pub voice_enabled: bool,
    #[serde(default = "default_voice_stt_backend")]
    pub voice_stt_backend: String,
    #[serde(default = "default_voice_tts_backend")]
    pub voice_tts_backend: String,
    #[serde(default)]
    pub voice_id: String,
    #[serde(default = "default_computer_use_enabled")]
    pub computer_use_enabled: bool,
    #[serde(default = "default_ocr_model_source")]
    pub ocr_model_source: String,
    #[serde(default = "default_web_search_provider")]
    pub web_search_provider: String,
    #[serde(default)]
    pub tavily_api_key: String,
    #[serde(default)]
    pub brave_search_api_key: String,
    #[serde(default)]
    pub exa_api_key: String,
    #[serde(default)]
    pub webdav_enabled: bool,
    #[serde(default)]
    pub webdav_url: String,
    #[serde(default)]
    pub webdav_username: String,
    #[serde(default)]
    pub webdav_password: String,
    #[serde(default = "default_webdav_path")]
    pub webdav_path: String,
    #[serde(default = "default_media_provider")]
    pub media_provider: String,
    #[serde(default = "default_media_base_url")]
    pub media_base_url: String,
    #[serde(default)]
    pub media_api_key: String,
    #[serde(default = "default_image_model")]
    pub image_model: String,
    #[serde(default = "default_image_size")]
    pub image_size: String,
    #[serde(default = "default_tts_model")]
    pub tts_model: String,
    #[serde(default = "default_tts_voice")]
    pub tts_voice: String,
    #[serde(default = "default_mcp_servers")]
    pub mcp_servers: Vec<crate::mcp::McpServerConfig>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            provider: ProviderKind::DeepSeek,
            permission_mode: PermissionMode::Default,
            // 默认 DeepSeek（OpenAI 兼容）。换 LM Studio 等本地端点只改 base_url + model。
            base_url: "https://api.deepseek.com/v1".to_string(),
            api_key: String::new(),
            model: "deepseek-chat".to_string(),
            current_pack: "default".to_string(),
            max_context_chars: DEFAULT_MAX_CONTEXT_CHARS,
            max_input_tokens: DEFAULT_MAX_INPUT_TOKENS,
            reserved_output_tokens: DEFAULT_RESERVED_OUTPUT_TOKENS,
            reasoning_effort: default_reasoning_effort(),
            auto_memory_enabled: DEFAULT_AUTO_MEMORY_ENABLED,
            voice_enabled: DEFAULT_VOICE_ENABLED,
            voice_stt_backend: default_voice_stt_backend(),
            voice_tts_backend: default_voice_tts_backend(),
            voice_id: String::new(),
            computer_use_enabled: DEFAULT_COMPUTER_USE_ENABLED,
            ocr_model_source: default_ocr_model_source(),
            web_search_provider: default_web_search_provider(),
            tavily_api_key: String::new(),
            brave_search_api_key: String::new(),
            exa_api_key: String::new(),
            webdav_enabled: false,
            webdav_url: String::new(),
            webdav_username: String::new(),
            webdav_password: String::new(),
            webdav_path: default_webdav_path(),
            media_provider: default_media_provider(),
            media_base_url: default_media_base_url(),
            media_api_key: String::new(),
            image_model: default_image_model(),
            image_size: default_image_size(),
            tts_model: default_tts_model(),
            tts_voice: default_tts_voice(),
            mcp_servers: Vec::new(),
        }
    }
}

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

static SEQ: AtomicU64 = AtomicU64::new(1);

/// 生成全局唯一会话 id（时间戳 + 自增序号，避免同一毫秒碰撞）。
pub fn new_session_id() -> String {
    format!("s_{}_{}", now_millis(), SEQ.fetch_add(1, Ordering::Relaxed))
}

/// 一段会话。
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Session {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<crate::agent::goal::GoalState>,
    pub messages: Vec<Message>,
    pub updated_at: u64,
}

impl Session {
    pub fn new() -> Self {
        Session {
            id: new_session_id(),
            title: "新对话".to_string(),
            summary: None,
            goal: None,
            messages: Vec::new(),
            updated_at: now_millis(),
        }
    }
}

/// 会话集合 + 当前活动会话 id。
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct SessionStore {
    pub active: String,
    pub sessions: Vec<Session>,
}

impl SessionStore {
    /// 保证至少有一个会话且 active 指向有效会话。
    pub fn ensure_one(&mut self) {
        if self.sessions.is_empty() {
            let s = Session::new();
            self.active = s.id.clone();
            self.sessions.push(s);
        }
        if !self.sessions.iter().any(|s| s.id == self.active) {
            self.active = self.sessions[0].id.clone();
        }
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Session> {
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    pub fn get(&self, id: &str) -> Option<&Session> {
        self.sessions.iter().find(|s| s.id == id)
    }
}

/// 会话元信息（给前端列表用，不含完整消息）。
#[derive(Serialize, Clone)]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub updated_at: u64,
}

pub fn load_settings(dir: &Path) -> Settings {
    let p = dir.join("settings.json");
    fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
        .unwrap_or_default()
}

pub fn redacted_settings(s: &Settings) -> Settings {
    let mut safe = s.clone();
    safe.api_key.clear();
    safe.tavily_api_key.clear();
    safe.brave_search_api_key.clear();
    safe.exa_api_key.clear();
    safe.webdav_password.clear();
    safe.media_api_key.clear();
    for server in &mut safe.mcp_servers {
        for env in &mut server.env {
            if env.secret {
                env.value.clear();
            }
        }
    }
    safe
}

pub fn save_settings(dir: &Path, s: &Settings) -> Result<(), String> {
    let p = dir.join("settings.json");
    let safe = redacted_settings(s);
    let json = serde_json::to_string_pretty(&safe).map_err(|e| e.to_string())?;
    fs::write(&p, json).map_err(|e| e.to_string())
}

/// 加载会话集合；若不存在则尝试从旧版单会话 conversation.json 迁移，否则建一个空会话。
pub fn load_sessions(dir: &Path) -> SessionStore {
    let p = dir.join("sessions.json");
    if let Some(store) = fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str::<SessionStore>(&s).ok())
    {
        let mut store = store;
        store.ensure_one();
        return store;
    }

    // 迁移：旧版单会话
    let mut store = SessionStore::default();
    let legacy = dir.join("conversation.json");
    if let Some(conv) = fs::read_to_string(&legacy)
        .ok()
        .and_then(|s| serde_json::from_str::<Conversation>(&s).ok())
    {
        if !conv.messages.is_empty() {
            let title = derive_title(&conv.messages);
            let mut s = Session::new();
            s.title = title;
            s.messages = conv.messages;
            store.active = s.id.clone();
            store.sessions.push(s);
        }
    }
    store.ensure_one();
    store
}

pub fn save_sessions(dir: &Path, store: &SessionStore) -> Result<(), String> {
    let p = dir.join("sessions.json");
    let json = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    fs::write(&p, json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_without_provider_defaults_to_deepseek() {
        let json = r#"{
            "base_url": "https://example.test/v1",
            "api_key": "sk-test",
            "model": "test-model",
            "current_pack": "default",
            "max_context_chars": 24000,
            "max_input_tokens": 32000,
            "reserved_output_tokens": 4000,
            "auto_memory_enabled": true
        }"#;
        let settings = serde_json::from_str::<Settings>(json).unwrap();
        assert_eq!(settings.provider, ProviderKind::DeepSeek);
        assert_eq!(settings.permission_mode, PermissionMode::Default);
        assert!(!settings.voice_enabled);
        assert_eq!(settings.voice_stt_backend, "none");
        assert_eq!(settings.voice_tts_backend, "none");
        assert!(!settings.computer_use_enabled);
        assert_eq!(settings.ocr_model_source, "modelscope");
        assert_eq!(settings.web_search_provider, "auto");
        assert!(settings.tavily_api_key.is_empty());
        assert!(settings.brave_search_api_key.is_empty());
        assert!(settings.exa_api_key.is_empty());
        assert_eq!(settings.media_provider, "dashscope");
        assert_eq!(settings.media_base_url, "https://dashscope.aliyuncs.com");
        assert_eq!(settings.image_model, "qwen-image-2.0");
        assert_eq!(settings.tts_model, "qwen3-tts-flash");
        assert!(settings.mcp_servers.is_empty());
        assert_eq!(settings.reasoning_effort, ReasoningEffort::Auto);
    }

    #[test]
    fn reasoning_effort_parses_command_values() {
        assert_eq!(ReasoningEffort::parse("low"), Some(ReasoningEffort::Low));
        assert_eq!(
            ReasoningEffort::parse("extra-high"),
            Some(ReasoningEffort::Xhigh)
        );
        assert_eq!(ReasoningEffort::parse("unset"), Some(ReasoningEffort::Auto));
        assert_eq!(ReasoningEffort::parse("unknown"), None);
    }

    #[test]
    fn save_settings_does_not_persist_api_key() {
        let dir = std::env::temp_dir().join(format!("demiurge_settings_test_{}", new_session_id()));
        std::fs::create_dir_all(&dir).unwrap();

        let settings = Settings {
            api_key: "sk-secret".to_string(),
            tavily_api_key: "tvly-secret".to_string(),
            brave_search_api_key: "brave-secret".to_string(),
            exa_api_key: "exa-secret".to_string(),
            media_api_key: "media-secret".to_string(),
            ..Settings::default()
        };
        save_settings(&dir, &settings).unwrap();

        let raw = std::fs::read_to_string(dir.join("settings.json")).unwrap();
        assert!(!raw.contains("sk-secret"));
        assert!(!raw.contains("tvly-secret"));
        assert!(!raw.contains("brave-secret"));
        assert!(!raw.contains("exa-secret"));
        assert!(!raw.contains("media-secret"));
        let saved = serde_json::from_str::<Settings>(&raw).unwrap();
        assert!(saved.api_key.is_empty());
        assert!(saved.tavily_api_key.is_empty());
        assert!(saved.brave_search_api_key.is_empty());
        assert!(saved.exa_api_key.is_empty());
        assert!(saved.webdav_password.is_empty());
        assert!(saved.media_api_key.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_settings_does_not_persist_secret_mcp_env_values() {
        let dir =
            std::env::temp_dir().join(format!("demiurge_mcp_settings_test_{}", new_session_id()));
        std::fs::create_dir_all(&dir).unwrap();

        let settings = Settings {
            mcp_servers: vec![crate::mcp::McpServerConfig {
                name: "secret-server".to_string(),
                enabled: true,
                transport: crate::mcp::McpTransportKind::Stdio,
                command: "cmd".to_string(),
                args: vec!["/c".to_string(), "echo".to_string(), "ok".to_string()],
                env: vec![crate::mcp::McpEnvVar {
                    key: "API_TOKEN".to_string(),
                    value: "mcp-secret".to_string(),
                    secret: true,
                }],
            }],
            ..Settings::default()
        };
        save_settings(&dir, &settings).unwrap();

        let raw = std::fs::read_to_string(dir.join("settings.json")).unwrap();
        assert!(!raw.contains("mcp-secret"));
        let saved = serde_json::from_str::<Settings>(&raw).unwrap();
        assert_eq!(saved.mcp_servers[0].env[0].value, "");

        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// 用首条用户消息生成标题（截断）。
pub fn derive_title(messages: &[Message]) -> String {
    let first = messages
        .iter()
        .find(|m| m.role == "user")
        .and_then(|m| m.content.as_deref())
        .unwrap_or("")
        .trim();
    if first.is_empty() {
        return "新对话".to_string();
    }
    let t: String = first.chars().take(24).collect();
    if first.chars().count() > 24 {
        format!("{t}…")
    } else {
        t
    }
}

// ---------------- Session stats (dashboard) ----------------

const HEATMAP_DAYS: i64 = 126; // 18 weeks

#[derive(Serialize, Clone)]
pub struct DayCell {
    pub date: String,
    pub count: u32,
    pub level: u8,
}

#[derive(Serialize, Clone)]
pub struct StatsPanel {
    pub sessions: usize,
    pub messages: usize,
    pub est_tokens: u64,
    pub active_days: usize,
    pub current_streak: usize,
    pub longest_streak: usize,
    pub peak_hour: Option<u32>,
    pub model: String,
    pub heatmap_days: usize,
    pub heatmap: Vec<DayCell>,
}

fn level_for(count: u32) -> u8 {
    match count {
        0 => 0,
        1 => 1,
        2 => 2,
        3 | 4 => 3,
        _ => 4,
    }
}

/// Howard Hinnant civil_from_days: days since 1970-01-01 -> (year, month, day).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Aggregate dashboard stats from all sessions. `offset` is the client timezone
/// offset in minutes (JS `Date.getTimezoneOffset()`), used to bucket by local day/hour.
pub fn compute_stats(store: &SessionStore, offset: i64, model: String) -> StatsPanel {
    let offset_ms = offset * 60_000;
    let now_local = now_millis() as i64 - offset_ms;
    let today = now_local.div_euclid(86_400_000);

    let mut messages = 0usize;
    let mut est_tokens = 0u64;
    let mut day_counts: std::collections::HashMap<i64, u32> = std::collections::HashMap::new();
    let mut hour_counts = [0u32; 24];

    for s in &store.sessions {
        for m in &s.messages {
            if m.role == "user" || m.role == "assistant" {
                messages += 1;
            }
            if let Some(c) = &m.content {
                est_tokens += (c.chars().count() as u64) / 4;
            }
            if let Some(tcs) = &m.tool_calls {
                for tc in tcs {
                    est_tokens += (tc.function.arguments.chars().count() as u64) / 4;
                }
            }
        }
        let local = s.updated_at as i64 - offset_ms;
        let day = local.div_euclid(86_400_000);
        *day_counts.entry(day).or_insert(0) += 1;
        let hour = (local.rem_euclid(86_400_000) / 3_600_000) as usize;
        if hour < 24 {
            hour_counts[hour] += 1;
        }
    }

    let active_days = day_counts.len();

    // current streak: consecutive active days ending exactly at today
    let mut current_streak = 0usize;
    let mut d = today;
    while day_counts.contains_key(&d) {
        current_streak += 1;
        d -= 1;
    }

    // longest streak: longest run of consecutive day indices
    let mut days: Vec<i64> = day_counts.keys().copied().collect();
    days.sort_unstable();
    let mut longest_streak = 0usize;
    let mut run = 0usize;
    let mut prev: Option<i64> = None;
    for &day in &days {
        run = if prev == Some(day - 1) { run + 1 } else { 1 };
        if run > longest_streak {
            longest_streak = run;
        }
        prev = Some(day);
    }

    let peak_hour = if hour_counts.iter().all(|&c| c == 0) {
        None
    } else {
        hour_counts
            .iter()
            .enumerate()
            .max_by_key(|(_, &c)| c)
            .map(|(h, _)| h as u32)
    };

    let mut heatmap = Vec::with_capacity(HEATMAP_DAYS as usize);
    for i in (0..HEATMAP_DAYS).rev() {
        let day = today - i;
        let count = day_counts.get(&day).copied().unwrap_or(0);
        let (y, mo, dd) = civil_from_days(day);
        heatmap.push(DayCell {
            date: format!("{y:04}-{mo:02}-{dd:02}"),
            count,
            level: level_for(count),
        });
    }

    StatsPanel {
        sessions: store.sessions.len(),
        messages,
        est_tokens,
        active_days,
        current_streak,
        longest_streak,
        peak_hour,
        model,
        heatmap_days: HEATMAP_DAYS as usize,
        heatmap,
    }
}
