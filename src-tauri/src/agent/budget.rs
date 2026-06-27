//! Phase 2：轻量 token 预算。用启发式估算统一约束 system/tools/history/output reserve。
use serde_json::Value;

use super::conversation::Message;
use crate::store::Settings;

const MESSAGE_OVERHEAD_TOKENS: usize = 8;
const TOOL_CALL_OVERHEAD_TOKENS: usize = 12;
const MIN_HISTORY_BUDGET_TOKENS: usize = 512;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct ContextBudget {
    pub max_input_tokens: usize,
    pub reserved_output_tokens: usize,
    pub system_tokens: usize,
    pub tools_tokens: usize,
    pub history_tokens: usize,
    pub history_budget_tokens: usize,
}

pub fn estimate_text_tokens(text: &str) -> usize {
    let mut ascii = 0usize;
    let mut non_ascii = 0usize;
    for ch in text.chars() {
        if ch.is_ascii() {
            ascii += 1;
        } else {
            non_ascii += 1;
        }
    }

    ascii.div_ceil(4) + non_ascii.max(1)
}

pub fn estimate_message_tokens(message: &Message) -> usize {
    let mut total = MESSAGE_OVERHEAD_TOKENS + estimate_text_tokens(&message.role);

    if let Some(content) = &message.content {
        total += estimate_text_tokens(content);
    }
    if let Some(tool_call_id) = &message.tool_call_id {
        total += estimate_text_tokens(tool_call_id);
    }
    if let Some(name) = &message.name {
        total += estimate_text_tokens(name);
    }
    if let Some(tool_calls) = &message.tool_calls {
        for tc in tool_calls {
            total += TOOL_CALL_OVERHEAD_TOKENS;
            total += estimate_text_tokens(&tc.id);
            total += estimate_text_tokens(&tc.function.name);
            total += estimate_text_tokens(&tc.function.arguments);
        }
    }

    total
}

pub fn estimate_messages_tokens(messages: &[Message]) -> usize {
    messages.iter().map(estimate_message_tokens).sum()
}

pub fn estimate_tools_tokens(tools: &Value) -> usize {
    estimate_text_tokens(&tools.to_string())
}

pub fn history_budget(
    settings: &Settings,
    system: &str,
    tools: &Value,
    history: &[Message],
) -> ContextBudget {
    let max_input_tokens = settings.max_input_tokens.max(MIN_HISTORY_BUDGET_TOKENS * 2);
    let reserved_output_tokens = settings
        .reserved_output_tokens
        .min(max_input_tokens.saturating_sub(MIN_HISTORY_BUDGET_TOKENS))
        .max(1);
    let system_tokens = estimate_text_tokens(system);
    let tools_tokens = estimate_tools_tokens(tools);
    let history_tokens = estimate_messages_tokens(history);
    let occupied = reserved_output_tokens
        .saturating_add(system_tokens)
        .saturating_add(tools_tokens);
    let history_budget_tokens = max_input_tokens
        .saturating_sub(occupied)
        .max(MIN_HISTORY_BUDGET_TOKENS);

    ContextBudget {
        max_input_tokens,
        reserved_output_tokens,
        system_tokens,
        tools_tokens,
        history_tokens,
        history_budget_tokens,
    }
}
