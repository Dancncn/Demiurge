//! LLM provider adapter 入口：统一 Agent 消息结构，按 provider 分发到具体流式客户端。
mod anthropic;
mod gemini;
mod local;
mod openai;

use std::sync::atomic::AtomicBool;

use serde_json::Value;

use crate::agent::conversation::Message;
use crate::store::{ProviderKind, Settings};

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
    pub supports_tools: bool,
    pub supports_streaming: bool,
    pub prompt_cache: PromptCacheCapability,
    pub thinking: ThinkingCapability,
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
            supports_tools: true,
            supports_streaming: true,
            prompt_cache: PromptCacheCapability::Unsupported,
            thinking: ThinkingCapability::Unsupported,
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
            parallel_tool_calls: ParallelToolCallCapability::OpenAiCompatibleField,
            max_input_tokens: Some(128_000),
            max_output_tokens: Some(16_384),
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
            supports_tools: true,
            supports_streaming: true,
            prompt_cache: PromptCacheCapability::AnthropicCacheControl,
            thinking: ThinkingCapability::AnthropicThinking,
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
            supports_tools: true,
            supports_streaming: true,
            prompt_cache: PromptCacheCapability::Unsupported,
            thinking: ThinkingCapability::GeminiThinking,
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

    pub fn supports_parallel_tool_call_field(self) -> bool {
        matches!(self.parallel_tool_calls, ParallelToolCallCapability::OpenAiCompatibleField)
    }

    pub fn supports_structured_output(self) -> bool {
        !matches!(self.structured_output, StructuredOutputCapability::Unsupported)
    }

    #[allow(dead_code)]
    pub fn supports_prompt_cache(self) -> bool {
        !matches!(self.prompt_cache, PromptCacheCapability::Unsupported)
    }

    #[allow(dead_code)]
    pub fn supports_thinking(self) -> bool {
        !matches!(self.thinking, ThinkingCapability::Unsupported)
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
            ToolSchemaDialect::OpenAiCompatible | ToolSchemaDialect::Anthropic => Value::Array(vec![]),
        }
    }
}

/// 流式调用当前设置选择的 provider。
/// - `on_delta`：每收到一段正文增量就回调（用于把 token 实时推给前端）。
/// - `cancel`：用户中断标志，置位后尽快结束并返回 finish_reason="interrupted"。
pub async fn stream_completion(
    client: &reqwest::Client,
    cfg: &Settings,
    messages: &[Message],
    tools: &Value,
    on_delta: impl FnMut(&str),
    cancel: &AtomicBool,
) -> Result<AssistantTurn, String> {
    match cfg.provider {
        ProviderKind::DeepSeek
        | ProviderKind::DashScope
        | ProviderKind::OpenAi
        | ProviderKind::OpenRouter
        | ProviderKind::Glm
        | ProviderKind::MiniMax
        | ProviderKind::Custom
        | ProviderKind::OpenAiCompatible => {
            openai::stream_completion_with_profile(
                client,
                cfg,
                messages,
                tools,
                on_delta,
                cancel,
                ProviderProfile::openai_compatible(),
            )
            .await
        }
        ProviderKind::Local => {
            local::stream_completion(client, cfg, messages, tools, on_delta, cancel).await
        }
        ProviderKind::Anthropic => {
            anthropic::stream_completion(client, cfg, messages, tools, on_delta, cancel).await
        }
        ProviderKind::Gemini => {
            gemini::stream_completion(client, cfg, messages, tools, on_delta, cancel).await
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
    use crate::store::ProviderKind;

    #[test]
    fn provider_profile_matches_kind() {
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::DeepSeek).tool_schema_dialect,
            ToolSchemaDialect::OpenAiCompatible
        );
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::OpenAiCompatible).structured_output,
            StructuredOutputCapability::OpenAiJsonSchema
        );
        assert!(ProviderProfile::for_kind(ProviderKind::OpenAiCompatible).requires_api_key);
        assert!(!ProviderProfile::for_kind(ProviderKind::Local).requires_api_key);
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::Anthropic).tool_schema_dialect,
            ToolSchemaDialect::Anthropic
        );
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::Anthropic).structured_output,
            StructuredOutputCapability::AnthropicJsonSchema
        );
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::Gemini).tool_schema_dialect,
            ToolSchemaDialect::Gemini
        );
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::Gemini).structured_output,
            StructuredOutputCapability::GeminiSchema
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
}
