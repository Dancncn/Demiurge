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
#[derive(Clone, Copy, Debug)]
pub struct ProviderProfile {
    pub supports_tools: bool,
    pub supports_streaming: bool,
    pub supports_prompt_cache: bool,
    pub supports_thinking: bool,
    pub supports_parallel_tool_calls: bool,
    pub requires_api_key: bool,
    pub max_input_tokens: Option<u32>,
    pub max_output_tokens: Option<u32>,
    pub tool_schema_dialect: ToolSchemaDialect,
}

impl ProviderProfile {
    pub const fn openai_compatible() -> Self {
        ProviderProfile {
            supports_tools: true,
            supports_streaming: true,
            supports_prompt_cache: false,
            supports_thinking: false,
            supports_parallel_tool_calls: false,
            requires_api_key: true,
            max_input_tokens: None,
            max_output_tokens: None,
            tool_schema_dialect: ToolSchemaDialect::OpenAiCompatible,
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
            supports_prompt_cache: false,
            supports_thinking: false,
            supports_parallel_tool_calls: false,
            requires_api_key: true,
            max_input_tokens: None,
            max_output_tokens: None,
            tool_schema_dialect: ToolSchemaDialect::Anthropic,
        }
    }

    pub const fn gemini() -> Self {
        ProviderProfile {
            supports_tools: true,
            supports_streaming: true,
            supports_prompt_cache: false,
            supports_thinking: false,
            supports_parallel_tool_calls: false,
            requires_api_key: true,
            max_input_tokens: None,
            max_output_tokens: None,
            tool_schema_dialect: ToolSchemaDialect::Gemini,
        }
    }

    pub const fn for_kind(kind: ProviderKind) -> Self {
        match kind {
            ProviderKind::DeepSeek
            | ProviderKind::DashScope
            | ProviderKind::OpenAi
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
        assert!(ProviderProfile::for_kind(ProviderKind::OpenAiCompatible).requires_api_key);
        assert!(!ProviderProfile::for_kind(ProviderKind::Local).requires_api_key);
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::Anthropic).tool_schema_dialect,
            ToolSchemaDialect::Anthropic
        );
        assert_eq!(
            ProviderProfile::for_kind(ProviderKind::Gemini).tool_schema_dialect,
            ToolSchemaDialect::Gemini
        );
    }
}
