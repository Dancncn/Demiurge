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

/// 运行时设置。MVP 直接以 JSON 落盘（含 api_key 明文）。
/// 注：更安全的做法是把 api_key 存进系统凭据管理器（如 Windows 凭据管理器 / keyring），后续可平滑替换。
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Settings {
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
    #[serde(default = "default_auto_memory_enabled")]
    pub auto_memory_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            // 默认 DeepSeek（OpenAI 兼容）。换 LM Studio 等本地端点只改 base_url + model。
            base_url: "https://api.deepseek.com/v1".to_string(),
            api_key: String::new(),
            model: "deepseek-chat".to_string(),
            current_pack: "default".to_string(),
            max_context_chars: DEFAULT_MAX_CONTEXT_CHARS,
            max_input_tokens: DEFAULT_MAX_INPUT_TOKENS,
            reserved_output_tokens: DEFAULT_RESERVED_OUTPUT_TOKENS,
            auto_memory_enabled: DEFAULT_AUTO_MEMORY_ENABLED,
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
    pub messages: Vec<Message>,
    pub updated_at: u64,
}

impl Session {
    pub fn new() -> Self {
        Session {
            id: new_session_id(),
            title: "新对话".to_string(),
            summary: None,
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

pub fn save_settings(dir: &Path, s: &Settings) -> Result<(), String> {
    let p = dir.join("settings.json");
    let json = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
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
