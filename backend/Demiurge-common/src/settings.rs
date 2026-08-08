use serde::{Deserialize, Serialize};

pub const DEFAULT_MAX_CONTEXT_CHARS: usize = 24_000;
pub const DEFAULT_MAX_INPUT_TOKENS: usize = 32_000;
pub const DEFAULT_RESERVED_OUTPUT_TOKENS: usize = 4_000;
pub const DEFAULT_AUTO_MEMORY_ENABLED: bool = true;
pub const DEFAULT_VOICE_ENABLED: bool = false;
pub const DEFAULT_COMPUTER_USE_ENABLED: bool = false;
pub const DEFAULT_COMPANION_ENABLED: bool = true;
pub const DEFAULT_WEATHER_ENABLED: bool = false;

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

fn default_context_budget_auto() -> bool {
    true
}

fn default_language() -> String {
    "zh".to_string()
}

fn default_theme() -> String {
    "system".to_string()
}

fn default_appearance() -> String {
    "material_bloom".to_string()
}

fn default_auto_memory_enabled() -> bool {
    DEFAULT_AUTO_MEMORY_ENABLED
}

fn default_embedding_provider() -> String {
    "none".to_string()
}

fn default_embedding_dims() -> usize {
    1024
}

fn default_hybrid_weight() -> f32 {
    0.5
}

fn default_companion_enabled() -> bool {
    DEFAULT_COMPANION_ENABLED
}

fn default_companion_memory_extraction_enabled() -> bool {
    false
}

fn default_companion_memory_extraction_scope() -> String {
    "recent_turn".to_string()
}

fn default_companion_tone() -> String {
    "gentle".to_string()
}

fn default_companion_mood() -> String {
    "neutral".to_string()
}

fn default_companion_energy() -> String {
    "normal".to_string()
}

fn default_companion_focus() -> String {
    "available".to_string()
}

fn default_desktop_companion_always_on_top() -> bool {
    true
}

fn default_weather_enabled() -> bool {
    DEFAULT_WEATHER_ENABLED
}

fn default_weather_location_mode() -> String {
    "manual".to_string()
}

fn default_weather_provider() -> String {
    "open_meteo".to_string()
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

fn default_voice_speed() -> f32 {
    1.0
}

fn default_voice_streaming() -> bool {
    false
}

fn default_voice_tts_fallback() -> bool {
    true
}

fn default_voice_hotkey_enabled() -> bool {
    true
}

fn default_voice_hotkey() -> String {
    "Ctrl+Shift+Space".to_string()
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
    #[serde(rename = "xai")]
    Xai,
    #[serde(rename = "groq")]
    Groq,
    #[serde(rename = "mistral")]
    Mistral,
    #[serde(rename = "moonshot")]
    Moonshot,
    #[serde(rename = "perplexity")]
    Perplexity,
    #[serde(rename = "doubao")]
    Doubao,
    #[serde(rename = "hunyuan")]
    Hunyuan,
    #[serde(rename = "stepfun")]
    StepFun,
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
    #[serde(default)]
    pub current_pet: String,
    #[serde(default = "default_max_context_chars")]
    pub max_context_chars: usize,
    #[serde(default = "default_max_input_tokens")]
    pub max_input_tokens: usize,
    #[serde(default = "default_reserved_output_tokens")]
    pub reserved_output_tokens: usize,
    /// 当为 true 时，输入预算自动跟随所选模型的上下文窗口，忽略手填的 max_input_tokens。
    #[serde(default = "default_context_budget_auto")]
    pub context_budget_auto: bool,
    /// 界面语言：zh（默认）/ en。
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_appearance")]
    pub appearance: String,
    #[serde(default)]
    pub launch_on_startup: bool,
    #[serde(default = "default_reasoning_effort")]
    pub reasoning_effort: ReasoningEffort,
    #[serde(default = "default_auto_memory_enabled")]
    pub auto_memory_enabled: bool,
    /// 是否启用 Lorebook 向量召回（embedding 混合 RRF）。
    #[serde(default)]
    pub embedding_enabled: bool,
    /// embedding provider 类型："none"（默认，纯 BM25）或 "remote"。
    #[serde(default = "default_embedding_provider")]
    pub embedding_provider: String,
    /// 远程 embedding base URL（OpenAI 兼容，如 DashScope / OpenAI）。
    #[serde(default)]
    pub embedding_base_url: String,
    /// 远程 embedding API Key。运行时由 `credentials::hydrate_or_migrate_settings` 从系统凭据管理器填充；
    /// 落盘时由 `redacted_settings` 清空，明文不写入 `settings.json`。
    #[serde(default)]
    pub embedding_api_key: String,
    /// embedding 模型名（如 text-embedding-v3 / text-embedding-3-small）。
    #[serde(default)]
    pub embedding_model: String,
    /// 输出向量维度，用于缓存失效判断。默认 1024。
    #[serde(default = "default_embedding_dims")]
    pub embedding_dims: usize,
    /// 混合召回权重：0.0=纯稀疏，1.0=纯稠密，默认 0.5（RRF 融合）。
    #[serde(default = "default_hybrid_weight")]
    pub hybrid_weight: f32,
    #[serde(default = "default_companion_enabled")]
    pub companion_enabled: bool,
    #[serde(default = "default_companion_memory_extraction_enabled")]
    pub companion_memory_extraction_enabled: bool,
    #[serde(default = "default_companion_memory_extraction_scope")]
    pub companion_memory_extraction_scope: String,
    #[serde(default = "default_companion_tone")]
    pub companion_tone: String,
    #[serde(default = "default_companion_mood")]
    pub companion_mood: String,
    #[serde(default = "default_companion_energy")]
    pub companion_energy: String,
    #[serde(default = "default_companion_focus")]
    pub companion_focus: String,
    #[serde(default)]
    pub companion_do_not_disturb: String,
    #[serde(default)]
    pub desktop_companion_enabled: bool,
    #[serde(default = "default_desktop_companion_always_on_top")]
    pub desktop_companion_always_on_top: bool,
    #[serde(default)]
    pub desktop_companion_click_through: bool,
    #[serde(default)]
    pub desktop_companion_collapsed: bool,
    #[serde(default = "default_weather_enabled")]
    pub weather_enabled: bool,
    #[serde(default = "default_weather_location_mode")]
    pub weather_location_mode: String,
    #[serde(default)]
    pub weather_city: String,
    #[serde(default = "default_weather_provider")]
    pub weather_provider: String,
    #[serde(default = "default_voice_enabled")]
    pub voice_enabled: bool,
    #[serde(default = "default_voice_stt_backend")]
    pub voice_stt_backend: String,
    #[serde(default = "default_voice_tts_backend")]
    pub voice_tts_backend: String,
    #[serde(default)]
    pub voice_id: String,
    #[serde(default = "default_voice_speed")]
    pub voice_speed: f32,
    #[serde(default)]
    pub voice_emotion: String,
    #[serde(default = "default_voice_streaming")]
    pub voice_streaming: bool,
    #[serde(default = "default_voice_tts_fallback")]
    pub voice_tts_fallback: bool,
    #[serde(default = "default_voice_hotkey_enabled")]
    pub voice_hotkey_enabled: bool,
    #[serde(default = "default_voice_hotkey")]
    pub voice_hotkey: String,
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
            current_pet: String::new(),
            max_context_chars: DEFAULT_MAX_CONTEXT_CHARS,
            max_input_tokens: DEFAULT_MAX_INPUT_TOKENS,
            reserved_output_tokens: DEFAULT_RESERVED_OUTPUT_TOKENS,
            context_budget_auto: true,
            language: default_language(),
            theme: default_theme(),
            appearance: default_appearance(),
            launch_on_startup: false,
            reasoning_effort: default_reasoning_effort(),
            auto_memory_enabled: DEFAULT_AUTO_MEMORY_ENABLED,
            embedding_enabled: false,
            embedding_provider: default_embedding_provider(),
            embedding_base_url: String::new(),
            embedding_api_key: String::new(),
            embedding_model: String::new(),
            embedding_dims: default_embedding_dims(),
            hybrid_weight: default_hybrid_weight(),
            companion_enabled: DEFAULT_COMPANION_ENABLED,
            companion_memory_extraction_enabled: false,
            companion_memory_extraction_scope: default_companion_memory_extraction_scope(),
            companion_tone: default_companion_tone(),
            companion_mood: default_companion_mood(),
            companion_energy: default_companion_energy(),
            companion_focus: default_companion_focus(),
            companion_do_not_disturb: String::new(),
            desktop_companion_enabled: false,
            desktop_companion_always_on_top: default_desktop_companion_always_on_top(),
            desktop_companion_click_through: false,
            desktop_companion_collapsed: false,
            weather_enabled: DEFAULT_WEATHER_ENABLED,
            weather_location_mode: default_weather_location_mode(),
            weather_city: String::new(),
            weather_provider: default_weather_provider(),
            voice_enabled: DEFAULT_VOICE_ENABLED,
            voice_stt_backend: default_voice_stt_backend(),
            voice_tts_backend: default_voice_tts_backend(),
            voice_id: String::new(),
            voice_speed: default_voice_speed(),
            voice_emotion: String::new(),
            voice_streaming: default_voice_streaming(),
            voice_tts_fallback: default_voice_tts_fallback(),
            voice_hotkey_enabled: default_voice_hotkey_enabled(),
            voice_hotkey: default_voice_hotkey(),
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

#[cfg(test)]
mod tests {
    use super::{PermissionMode, ProviderKind, ReasoningEffort, Settings};

    #[test]
    fn legacy_settings_keep_compatible_defaults() {
        let settings = serde_json::from_str::<Settings>(
            r#"{
                "base_url": "https://example.test/v1",
                "api_key": "sk-test",
                "model": "test-model",
                "current_pack": "default",
                "max_context_chars": 24000,
                "max_input_tokens": 32000,
                "reserved_output_tokens": 4000,
                "auto_memory_enabled": true
            }"#,
        )
        .unwrap();

        assert_eq!(settings.provider, ProviderKind::DeepSeek);
        assert_eq!(settings.permission_mode, PermissionMode::Default);
        assert_eq!(settings.reasoning_effort, ReasoningEffort::Auto);
        assert_eq!(settings.theme, "system");
        assert_eq!(settings.appearance, "material_bloom");
        assert_eq!(settings.web_search_provider, "auto");
        assert_eq!(settings.voice_speed, 1.0);
        assert!(settings.voice_tts_fallback);
        assert!(settings.mcp_servers.is_empty());
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
}
