//! LLM provider adapter 入口：统一 Agent 消息结构，按 provider 分发到具体流式客户端。
mod anthropic;
mod gemini;
mod local;
mod openai;

use std::sync::atomic::AtomicBool;

use serde_json::Value;

use crate::agent::conversation::Message;
use crate::store::{ProviderKind, ReasoningEffort, Settings};

/// Normalized token usage returned by a provider.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: Option<usize>,
    pub output_tokens: Option<usize>,
    pub total_tokens: Option<usize>,
}

impl Usage {
    pub fn total_or_sum(self) -> Option<usize> {
        self.total_tokens
            .or_else(|| match (self.input_tokens, self.output_tokens) {
                (Some(input), Some(output)) => Some(input.saturating_add(output)),
                (Some(input), None) => Some(input),
                (None, Some(output)) => Some(output),
                (None, None) => None,
            })
    }

    pub fn merge(self, next: Usage) -> Usage {
        let input_tokens = next.input_tokens.or(self.input_tokens);
        let output_tokens = next.output_tokens.or(self.output_tokens);
        let total_tokens = next
            .total_tokens
            .or_else(|| match (input_tokens, output_tokens) {
                (Some(input), Some(output)) => Some(input.saturating_add(output)),
                _ => self.total_tokens,
            });
        Usage {
            input_tokens,
            output_tokens,
            total_tokens,
        }
    }
}

pub(crate) fn merge_usage(slot: &mut Option<Usage>, next: Usage) {
    *slot = Some(slot.map(|current| current.merge(next)).unwrap_or(next));
}

/// 一次 LLM 调用的结果。
pub struct AssistantTurn {
    pub content: String,
    pub tool_calls: Vec<crate::agent::conversation::ToolCall>,
    /// stop | tool_calls | length | interrupted | ...
    pub finish_reason: String,
    pub usage: Option<Usage>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolSchemaDialect {
    OpenAiCompatible,
    Anthropic,
    Gemini,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderAdapterKind {
    OpenAiCompatible,
    Anthropic,
    Gemini,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuredOutputCapability {
    Unsupported,
    OpenAiJsonSchema,
    AnthropicJsonSchema,
    GeminiSchema,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptCacheCapability {
    Unsupported,
    AnthropicCacheControl,
    OpenAiCompatibleHint,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThinkingCapability {
    Unsupported,
    AnthropicThinking,
    GeminiThinking,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReasoningEffortCapability {
    Unsupported,
    OpenAiChatCompletions,
    AnthropicOutputConfig,
    GeminiThinkingBudget,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParallelToolCallCapability {
    Unsupported,
    OpenAiCompatibleField,
    ProviderManaged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderTokenBudget {
    pub max_input_tokens: usize,
    pub reserved_output_tokens: usize,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructuredOutputRequest {
    pub name: String,
    pub description: Option<String>,
    pub schema: Value,
    pub strict: bool,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct ProviderProfile {
    pub adapter: ProviderAdapterKind,
    pub supports_tools: bool,
    pub supports_streaming: bool,
    pub prompt_cache: PromptCacheCapability,
    pub thinking: ThinkingCapability,
    pub reasoning_effort: ReasoningEffortCapability,
    pub parallel_tool_calls: ParallelToolCallCapability,
    pub requires_api_key: bool,
    pub max_input_tokens: Option<u32>,
    pub max_output_tokens: Option<u32>,
    pub token_budget_multiplier: u32,
    pub tool_schema_dialect: ToolSchemaDialect,
    pub structured_output: StructuredOutputCapability,
}

impl ProviderProfile {
    pub const fn openai_compatible() -> Self {
        ProviderProfile {
            adapter: ProviderAdapterKind::OpenAiCompatible,
            supports_tools: true,
            supports_streaming: true,
            prompt_cache: PromptCacheCapability::Unsupported,
            thinking: ThinkingCapability::Unsupported,
            reasoning_effort: ReasoningEffortCapability::Unsupported,
            parallel_tool_calls: ParallelToolCallCapability::Unsupported,
            requires_api_key: true,
            max_input_tokens: None,
            max_output_tokens: None,
            token_budget_multiplier: 1,
            tool_schema_dialect: ToolSchemaDialect::OpenAiCompatible,
            structured_output: StructuredOutputCapability::OpenAiJsonSchema,
        }
    }

    pub const fn openai() -> Self {
        ProviderProfile {
            reasoning_effort: ReasoningEffortCapability::OpenAiChatCompletions,
            parallel_tool_calls: ParallelToolCallCapability::OpenAiCompatibleField,
            // GPT-5 系列输入窗口 ~272K（总 400K，输出 ~128K）。
            max_input_tokens: Some(272_000),
            max_output_tokens: Some(128_000),
            ..ProviderProfile::openai_compatible()
        }
    }

    pub const fn local_openai_compatible() -> Self {
        ProviderProfile {
            requires_api_key: false,
            ..ProviderProfile::openai_compatible()
        }
    }

    pub const fn anthropic() -> Self {
        ProviderProfile {
            adapter: ProviderAdapterKind::Anthropic,
            supports_tools: true,
            supports_streaming: true,
            prompt_cache: PromptCacheCapability::AnthropicCacheControl,
            thinking: ThinkingCapability::AnthropicThinking,
            reasoning_effort: ReasoningEffortCapability::AnthropicOutputConfig,
            parallel_tool_calls: ParallelToolCallCapability::ProviderManaged,
            requires_api_key: true,
            max_input_tokens: Some(200_000),
            max_output_tokens: Some(64_000),
            token_budget_multiplier: 1,
            tool_schema_dialect: ToolSchemaDialect::Anthropic,
            structured_output: StructuredOutputCapability::AnthropicJsonSchema,
        }
    }

    pub const fn gemini() -> Self {
        ProviderProfile {
            adapter: ProviderAdapterKind::Gemini,
            supports_tools: true,
            supports_streaming: true,
            prompt_cache: PromptCacheCapability::Unsupported,
            thinking: ThinkingCapability::GeminiThinking,
            reasoning_effort: ReasoningEffortCapability::GeminiThinkingBudget,
            parallel_tool_calls: ParallelToolCallCapability::ProviderManaged,
            requires_api_key: true,
            max_input_tokens: Some(1_000_000),
            max_output_tokens: Some(65_536),
            token_budget_multiplier: 1,
            tool_schema_dialect: ToolSchemaDialect::Gemini,
            structured_output: StructuredOutputCapability::GeminiSchema,
        }
    }

    pub const fn for_kind(kind: ProviderKind) -> Self {
        match kind {
            ProviderKind::OpenAi => ProviderProfile::openai(),
            ProviderKind::DeepSeek
            | ProviderKind::DashScope
            | ProviderKind::OpenRouter
            | ProviderKind::Glm
            | ProviderKind::MiniMax
            | ProviderKind::Xai
            | ProviderKind::Groq
            | ProviderKind::Mistral
            | ProviderKind::Moonshot
            | ProviderKind::Perplexity
            | ProviderKind::Doubao
            | ProviderKind::Hunyuan
            | ProviderKind::StepFun
            | ProviderKind::Custom
            | ProviderKind::OpenAiCompatible => ProviderProfile::openai_compatible(),
            ProviderKind::Local => ProviderProfile::local_openai_compatible(),
            ProviderKind::Anthropic => ProviderProfile::anthropic(),
            ProviderKind::Gemini => ProviderProfile::gemini(),
        }
    }

    pub fn supports_non_empty_tools(self, tools: &Value) -> bool {
        self.supports_tools && non_empty_tools(tools)
    }

    pub const fn adapter_kind(self) -> ProviderAdapterKind {
        self.adapter
    }

    pub fn supports_parallel_tool_call_field(self) -> bool {
        matches!(
            self.parallel_tool_calls,
            ParallelToolCallCapability::OpenAiCompatibleField
        )
    }

    pub fn supports_structured_output(self) -> bool {
        !matches!(
            self.structured_output,
            StructuredOutputCapability::Unsupported
        )
    }

    #[allow(dead_code)]
    pub fn supports_prompt_cache(self) -> bool {
        !matches!(self.prompt_cache, PromptCacheCapability::Unsupported)
    }

    #[allow(dead_code)]
    pub fn supports_thinking(self) -> bool {
        !matches!(self.thinking, ThinkingCapability::Unsupported)
    }

    pub fn supports_reasoning_effort(self) -> bool {
        !matches!(
            self.reasoning_effort,
            ReasoningEffortCapability::Unsupported
        )
    }

    pub fn supports_reasoning_effort_for_model(self, model: &str) -> bool {
        if !self.supports_reasoning_effort() {
            return false;
        }
        if env_always_enable_effort() {
            return true;
        }
        let model = model.to_ascii_lowercase();
        match self.reasoning_effort {
            ReasoningEffortCapability::OpenAiChatCompletions => {
                openai_model_supports_reasoning_effort(&model)
            }
            ReasoningEffortCapability::AnthropicOutputConfig => {
                anthropic_model_supports_reasoning_effort(&model)
            }
            ReasoningEffortCapability::GeminiThinkingBudget => {
                gemini_model_supports_thinking_budget(&model)
            }
            ReasoningEffortCapability::Unsupported => false,
        }
    }

    pub fn effective_reasoning_effort(self, settings: &Settings) -> Option<ReasoningEffort> {
        if !self.supports_reasoning_effort_for_model(&settings.model) {
            return None;
        }
        let configured = env_reasoning_effort_override().unwrap_or(settings.reasoning_effort);
        if configured.is_auto() {
            return None;
        }
        Some(configured)
    }

    pub fn openai_chat_reasoning_effort(self, settings: &Settings) -> Option<&'static str> {
        if !matches!(
            self.reasoning_effort,
            ReasoningEffortCapability::OpenAiChatCompletions
        ) {
            return None;
        }
        Some(match self.effective_reasoning_effort(settings)? {
            ReasoningEffort::Auto => return None,
            ReasoningEffort::Low => "low",
            ReasoningEffort::Medium => "medium",
            ReasoningEffort::High => "high",
            ReasoningEffort::Xhigh | ReasoningEffort::Max => {
                if openai_model_supports_xhigh_reasoning_effort(&settings.model) {
                    "xhigh"
                } else {
                    "high"
                }
            }
        })
    }

    pub fn anthropic_output_config_effort(self, settings: &Settings) -> Option<&'static str> {
        if !matches!(
            self.reasoning_effort,
            ReasoningEffortCapability::AnthropicOutputConfig
        ) {
            return None;
        }
        Some(match self.effective_reasoning_effort(settings)? {
            ReasoningEffort::Auto => return None,
            ReasoningEffort::Low => "low",
            ReasoningEffort::Medium => "medium",
            ReasoningEffort::High => "high",
            ReasoningEffort::Xhigh => "xhigh",
            ReasoningEffort::Max => "max",
        })
    }

    pub fn gemini_thinking_budget_tokens(self, settings: &Settings) -> Option<usize> {
        if !matches!(
            self.reasoning_effort,
            ReasoningEffortCapability::GeminiThinkingBudget
        ) {
            return None;
        }
        let effort = self.effective_reasoning_effort(settings)?;
        let max_output = self.effective_reserved_output_tokens(settings);
        if max_output <= 2_048 {
            return None;
        }
        let response_reserve = 1_024;
        let max_budget = max_output.saturating_sub(response_reserve);
        let desired = match effort {
            ReasoningEffort::Auto => return None,
            ReasoningEffort::Low => 1_024,
            ReasoningEffort::Medium => 4_096,
            ReasoningEffort::High => 8_192,
            ReasoningEffort::Xhigh => 16_384,
            ReasoningEffort::Max => 32_768,
        };
        Some(desired.min(max_budget).max(1_024))
    }

    pub fn effective_max_input_tokens(self, settings: &Settings) -> usize {
        self.max_input_tokens
            .map(|limit| settings.max_input_tokens.min(limit as usize))
            .unwrap_or(settings.max_input_tokens)
            .max(1)
    }

    pub fn effective_reserved_output_tokens(self, settings: &Settings) -> usize {
        self.max_output_tokens
            .map(|limit| settings.reserved_output_tokens.min(limit as usize))
            .unwrap_or(settings.reserved_output_tokens)
            .max(1)
    }

    pub fn effective_token_budget(self, settings: &Settings) -> ProviderTokenBudget {
        ProviderTokenBudget {
            max_input_tokens: self.effective_max_input_tokens(settings),
            reserved_output_tokens: self.effective_reserved_output_tokens(settings),
        }
    }

    #[allow(dead_code)]
    pub fn effective_max_output_tokens(self, requested: usize) -> usize {
        self.max_output_tokens
            .map(|limit| requested.min(limit as usize))
            .unwrap_or(requested)
    }

    pub fn structured_output_request<'a>(
        self,
        request: Option<&'a StructuredOutputRequest>,
    ) -> Option<&'a StructuredOutputRequest> {
        request.filter(|_| self.supports_structured_output())
    }

    pub fn empty_tool_schema(self) -> Value {
        match self.tool_schema_dialect {
            ToolSchemaDialect::Gemini => Value::Array(vec![serde_json::json!({
                "function_declarations": []
            })]),
            ToolSchemaDialect::OpenAiCompatible | ToolSchemaDialect::Anthropic => {
                Value::Array(vec![])
            }
        }
    }
}

fn env_reasoning_effort_override() -> Option<ReasoningEffort> {
    #[cfg(test)]
    {
        return None;
    }
    #[cfg(not(test))]
    {
        std::env::var("DEMIURGE_EFFORT_LEVEL")
            .ok()
            .and_then(|value| ReasoningEffort::parse(&value))
    }
}

fn env_always_enable_effort() -> bool {
    #[cfg(test)]
    {
        return false;
    }
    #[cfg(not(test))]
    {
        std::env::var("DEMIURGE_ALWAYS_ENABLE_EFFORT")
            .ok()
            .is_some_and(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
    }
}

fn openai_model_supports_reasoning_effort(model: &str) -> bool {
    let model = model.trim_start_matches("openai/");
    model.starts_with("o1")
        || model.starts_with("o3")
        || model.starts_with("o4")
        || model.starts_with("gpt-5")
        || model.contains("codex")
}

fn openai_model_supports_xhigh_reasoning_effort(model: &str) -> bool {
    let model = model
        .trim()
        .to_ascii_lowercase()
        .trim_start_matches("openai/")
        .to_string();
    if model.starts_with("gpt-5-pro") {
        return false;
    }
    if model.contains("codex") {
        return true;
    }
    openai_gpt5_minor_version(&model).is_some_and(|minor| minor >= 2)
}

fn openai_gpt5_minor_version(model: &str) -> Option<u32> {
    let rest = model.strip_prefix("gpt-5.")?;
    let digits = rest
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    digits.parse().ok()
}

fn anthropic_model_supports_reasoning_effort(model: &str) -> bool {
    model.contains("opus-4-7")
        || model.contains("opus-4.7")
        || model.contains("opus-4-6")
        || model.contains("opus-4.6")
        || model.contains("sonnet-4-6")
        || model.contains("sonnet-4.6")
        || model.contains("deepseek-v4-pro")
}

fn gemini_model_supports_thinking_budget(model: &str) -> bool {
    model.contains("gemini-2.5") || model.contains("gemini-3") || model.contains("thinking")
}

pub(crate) fn normalize_finish_reason(
    adapter: ProviderAdapterKind,
    raw: &str,
    has_tool_calls: bool,
) -> String {
    if has_tool_calls {
        return "tool_calls".to_string();
    }

    let raw = raw.trim();
    if raw.is_empty() {
        return "stop".to_string();
    }

    match adapter {
        ProviderAdapterKind::OpenAiCompatible => match raw {
            "stop" => "stop".to_string(),
            "length" => "length".to_string(),
            "tool_calls" | "function_call" => "tool_calls".to_string(),
            "content_filter" => "content_filter".to_string(),
            "interrupted" => "interrupted".to_string(),
            other => other.to_ascii_lowercase(),
        },
        ProviderAdapterKind::Anthropic => match raw {
            "end_turn" | "stop_sequence" => "stop".to_string(),
            "max_tokens" => "length".to_string(),
            "tool_use" => "tool_calls".to_string(),
            "interrupted" => "interrupted".to_string(),
            other => other.to_ascii_lowercase(),
        },
        ProviderAdapterKind::Gemini => match raw {
            "STOP" | "stop" => "stop".to_string(),
            "MAX_TOKENS" | "max_tokens" => "length".to_string(),
            "SAFETY" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" => "content_filter".to_string(),
            "interrupted" => "interrupted".to_string(),
            other => other.to_ascii_lowercase(),
        },
    }
}

/// 流式增量的类型：`Content` 是要展示给用户的正文；`Reasoning` 是推理型模型
/// （DeepSeek-R1/V4、Kimi、qwen `*-flash` 思考版等）在正文之前输出的思维链
/// （OpenAI 兼容端点的 `delta.reasoning_content`、Anthropic 的 `thinking_delta`、
/// Gemini 的 thought part）。两者分开回调，前端可把推理单独渲染成「思考中」气泡，
/// 避免推理阶段界面长时间无任何反馈。
#[derive(Clone, Copy, Debug)]
pub enum StreamDelta<'a> {
    Content(&'a str),
    Reasoning(&'a str),
}

/// 流式调用当前设置选择的 provider。
/// - `on_delta`：每收到一段增量就回调（`StreamDelta::Content` 为正文，
///   `StreamDelta::Reasoning` 为思维链），用于把 token 实时推给前端。
/// - `cancel`：用户中断标志，置位后尽快结束并返回 finish_reason="interrupted"。
pub async fn stream_completion(
    client: &reqwest::Client,
    cfg: &Settings,
    messages: &[Message],
    tools: &Value,
    on_delta: impl FnMut(StreamDelta<'_>),
    cancel: &AtomicBool,
) -> Result<AssistantTurn, String> {
    let profile = ProviderProfile::for_kind(cfg.provider);
    match profile.adapter_kind() {
        ProviderAdapterKind::OpenAiCompatible if cfg.provider == ProviderKind::Local => {
            local::stream_completion_with_profile(
                client, cfg, messages, tools, on_delta, cancel, profile,
            )
            .await
        }
        ProviderAdapterKind::OpenAiCompatible => {
            openai::stream_completion_with_profile(
                client, cfg, messages, tools, on_delta, cancel, profile,
            )
            .await
        }
        ProviderAdapterKind::Anthropic => {
            anthropic::stream_completion_with_profile(
                client, cfg, messages, tools, on_delta, cancel, profile,
            )
            .await
        }
        ProviderAdapterKind::Gemini => {
            gemini::stream_completion_with_profile(
                client, cfg, messages, tools, on_delta, cancel, profile,
            )
            .await
        }
    }
}

pub(crate) fn require_api_key(
    cfg: &Settings,
    profile: ProviderProfile,
) -> Result<Option<&str>, String> {
    let key = cfg.api_key.trim();
    if key.is_empty() {
        if profile.requires_api_key {
            Err("未配置 API Key，请在设置里填写。".to_string())
        } else {
            Ok(None)
        }
    } else {
        Ok(Some(key))
    }
}

pub(crate) fn non_empty_tools(tools: &Value) -> bool {
    tools.as_array().map(|a| !a.is_empty()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{ProviderKind, ReasoningEffort, Settings};

    #[test]
    fn provider_profile_matches_kind() {
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::DeepSeek).adapter_kind(),
            ProviderAdapterKind::OpenAiCompatible
        );
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::DeepSeek).tool_schema_dialect,
            ToolSchemaDialect::OpenAiCompatible
        );
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::Local).adapter_kind(),
            ProviderAdapterKind::OpenAiCompatible
        );
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::OpenAiCompatible).structured_output,
            StructuredOutputCapability::OpenAiJsonSchema
        );
        assert!(ProviderProfile::for_kind(ProviderKind::OpenAiCompatible).requires_api_key);
        assert!(!ProviderProfile::for_kind(ProviderKind::Local).requires_api_key);
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::OpenAi).parallel_tool_calls,
            ParallelToolCallCapability::OpenAiCompatibleField
        );
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::OpenAi).reasoning_effort,
            ReasoningEffortCapability::OpenAiChatCompletions
        );
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::Anthropic).tool_schema_dialect,
            ToolSchemaDialect::Anthropic
        );
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::Anthropic).adapter_kind(),
            ProviderAdapterKind::Anthropic
        );
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::Anthropic).structured_output,
            StructuredOutputCapability::AnthropicJsonSchema
        );
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::Anthropic).reasoning_effort,
            ReasoningEffortCapability::AnthropicOutputConfig
        );
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::Gemini).tool_schema_dialect,
            ToolSchemaDialect::Gemini
        );
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::Gemini).adapter_kind(),
            ProviderAdapterKind::Gemini
        );
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::Gemini).structured_output,
            StructuredOutputCapability::GeminiSchema
        );
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::Gemini).reasoning_effort,
            ReasoningEffortCapability::GeminiThinkingBudget
        );
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::DeepSeek).reasoning_effort,
            ReasoningEffortCapability::Unsupported
        );
    }

    #[test]
    fn provider_profile_helpers_gate_tools_and_tokens() {
        let mut profile = ProviderProfile::openai_compatible();
        assert!(profile.supports_non_empty_tools(&serde_json::json!([{ "name": "x" }])));
        assert!(!profile.supports_non_empty_tools(&serde_json::json!([])));
        profile.supports_tools = false;
        assert!(!profile.supports_non_empty_tools(&serde_json::json!([{ "name": "x" }])));

        profile.max_output_tokens = Some(100);
        assert_eq!(profile.effective_max_output_tokens(250), 100);
        assert_eq!(profile.effective_max_output_tokens(50), 50);
    }

    #[test]
    fn official_openai_profile_clamps_token_budget() {
        let settings = Settings {
            max_input_tokens: 250_000,
            reserved_output_tokens: 32_000,
            ..Settings::default()
        };
        let profile = ProviderProfile::for_kind(ProviderKind::OpenAi);
        let budget = profile.effective_token_budget(&settings);

        assert_eq!(budget.max_input_tokens, 250_000);
        assert_eq!(budget.reserved_output_tokens, 32_000);
        assert!(profile.supports_parallel_tool_call_field());
    }

    #[test]
    fn openai_compatible_profile_keeps_provider_defined_limits() {
        let settings = Settings {
            max_input_tokens: 250_000,
            reserved_output_tokens: 32_000,
            ..Settings::default()
        };
        let profile = ProviderProfile::for_kind(ProviderKind::OpenAiCompatible);
        let budget = profile.effective_token_budget(&settings);

        assert_eq!(budget.max_input_tokens, 250_000);
        assert_eq!(budget.reserved_output_tokens, 32_000);
        assert!(!profile.supports_parallel_tool_call_field());
    }

    #[test]
    fn effort_resolution_is_gated_by_provider_profile() {
        let settings = Settings {
            provider: ProviderKind::DeepSeek,
            reasoning_effort: ReasoningEffort::High,
            ..Settings::default()
        };
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::DeepSeek).effective_reasoning_effort(&settings),
            None
        );
    }

    #[test]
    fn effort_resolution_is_gated_by_model_support() {
        let unsupported = Settings {
            provider: ProviderKind::OpenAi,
            model: "gpt-4o".to_string(),
            reasoning_effort: ReasoningEffort::High,
            ..Settings::default()
        };
        let supported = Settings {
            provider: ProviderKind::OpenAi,
            model: "o3".to_string(),
            reasoning_effort: ReasoningEffort::High,
            ..Settings::default()
        };
        let profile = ProviderProfile::for_kind(ProviderKind::OpenAi);

        assert_eq!(profile.effective_reasoning_effort(&unsupported), None);
        assert_eq!(
            profile.effective_reasoning_effort(&supported),
            Some(ReasoningEffort::High)
        );
    }

    #[test]
    fn openai_effort_maps_xhigh_and_max_by_model_capability() {
        let mut settings = Settings {
            provider: ProviderKind::OpenAi,
            model: "o3".to_string(),
            reasoning_effort: ReasoningEffort::Xhigh,
            ..Settings::default()
        };
        let profile = ProviderProfile::for_kind(ProviderKind::OpenAi);
        assert_eq!(
            profile.openai_chat_reasoning_effort(&settings),
            Some("high")
        );
        settings.reasoning_effort = ReasoningEffort::Max;
        assert_eq!(
            profile.openai_chat_reasoning_effort(&settings),
            Some("high")
        );

        settings.model = "gpt-5.2".to_string();
        assert_eq!(
            profile.openai_chat_reasoning_effort(&settings),
            Some("xhigh")
        );
        settings.reasoning_effort = ReasoningEffort::Xhigh;
        assert_eq!(
            profile.openai_chat_reasoning_effort(&settings),
            Some("xhigh")
        );
    }

    #[test]
    fn openai_xhigh_support_respects_model_exceptions() {
        assert!(openai_model_supports_xhigh_reasoning_effort("gpt-5.2"));
        assert!(openai_model_supports_xhigh_reasoning_effort(
            "gpt-5.1-codex-max"
        ));
        assert!(!openai_model_supports_xhigh_reasoning_effort("gpt-5.1"));
        assert!(!openai_model_supports_xhigh_reasoning_effort("gpt-5-pro"));
    }

    #[test]
    fn auto_effort_sends_no_provider_parameter() {
        let settings = Settings {
            provider: ProviderKind::OpenAi,
            model: "o3".to_string(),
            reasoning_effort: ReasoningEffort::Auto,
            ..Settings::default()
        };
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::OpenAi).effective_reasoning_effort(&settings),
            None
        );
    }

    #[test]
    fn finish_reason_normalization_matches_adapter_dialects() {
        assert_eq!(
            normalize_finish_reason(
                ProviderAdapterKind::OpenAiCompatible,
                "function_call",
                false
            ),
            "tool_calls"
        );
        assert_eq!(
            normalize_finish_reason(ProviderAdapterKind::Anthropic, "max_tokens", false),
            "length"
        );
        assert_eq!(
            normalize_finish_reason(ProviderAdapterKind::Gemini, "SAFETY", false),
            "content_filter"
        );
        assert_eq!(
            normalize_finish_reason(ProviderAdapterKind::Gemini, "STOP", true),
            "tool_calls"
        );
    }
}
