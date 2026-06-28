//! Phase 2：轻量 token 预算。用启发式估算统一约束 system/tools/history/output reserve。
use serde::{Deserialize, Serialize};
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

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenBudgetState {
    pub total: Option<usize>,
    pub used_exact: usize,
    pub used_estimated: usize,
}

impl TokenBudgetState {
    pub fn new(total: Option<usize>) -> Self {
        TokenBudgetState {
            total,
            used_exact: 0,
            used_estimated: 0,
        }
    }

    pub fn used_total(&self) -> usize {
        self.used_exact.saturating_add(self.used_estimated)
    }

    pub fn remaining(&self) -> Option<usize> {
        self.total
            .map(|total| total.saturating_sub(self.used_total()))
    }

    pub fn is_exhausted(&self) -> bool {
        self.total
            .map(|total| self.used_total() >= total)
            .unwrap_or(false)
    }

    pub fn record_exact(&mut self, tokens: usize) {
        self.used_exact = self.used_exact.saturating_add(tokens);
    }

    pub fn record_estimated(&mut self, tokens: usize) {
        self.used_estimated = self.used_estimated.saturating_add(tokens);
    }

    pub fn record_usage_or_estimate(&mut self, usage: Option<crate::llm::Usage>, estimated: usize) -> bool {
        if let Some(tokens) = usage.and_then(|u| u.total_or_sum()) {
            self.record_exact(tokens);
            true
        } else {
            self.record_estimated(estimated);
            false
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_budget_tracks_exact_and_estimated_usage() {
        let mut budget = TokenBudgetState::new(Some(100));
        assert_eq!(budget.remaining(), Some(100));
        budget.record_estimated(25);
        budget.record_exact(50);
        assert_eq!(budget.used_total(), 75);
        assert_eq!(budget.remaining(), Some(25));
        assert!(!budget.is_exhausted());
        budget.record_estimated(25);
        assert!(budget.is_exhausted());
    }

    #[test]
    fn token_budget_prefers_provider_usage() {
        let mut budget = TokenBudgetState::new(Some(20));
        let exact = crate::llm::Usage {
            input_tokens: Some(4),
            output_tokens: Some(6),
            total_tokens: None,
        };
        assert!(budget.record_usage_or_estimate(Some(exact), 99));
        assert_eq!(budget.used_exact, 10);
        assert_eq!(budget.used_estimated, 0);
    }
}
