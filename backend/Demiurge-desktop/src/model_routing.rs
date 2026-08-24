//! Model-role routing and bounded, provider-local failover.
//!
//! This is an adapter seam around `llm::stream_completion`, not a second agent
//! loop. The main turn remains a direct loop; this module only resolves a
//! model id and retries a transient provider failure with an explicitly
//! configured sibling model.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::agent::budget;
use crate::agent::conversation::Message;
use crate::store::{ModelTier, ProviderKind, Settings};
use crate::{llm, AppState};

const MAX_FAILOVER_ATTEMPTS: usize = 5;
const DEEPSEEK_DEFAULT_VISION_MODEL: &str = "deepseek-v4-flash-vision-exp";

pub struct RoutedTurn {
    pub turn: llm::AssistantTurn,
}

pub struct FailoverRequest<'a> {
    pub state: &'a AppState,
    pub settings: &'a Settings,
    pub messages: &'a [Message],
    pub tools: &'a serde_json::Value,
    pub primary_model: &'a str,
    pub fallback_models: &'a [String],
    pub cancel: &'a AtomicBool,
    pub request_cancel: Option<&'a AtomicBool>,
    pub session_id: &'a str,
    pub purpose: &'a str,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct RouteHealth {
    pub consecutive_failures: usize,
    pub opened_until_unix_ms: Option<u64>,
    pub last_error: Option<String>,
    #[serde(skip)]
    opened_until: Option<Instant>,
}

pub fn default_agent_tier(
    agent_type: &str,
    config: &crate::store::ModelRoutingConfig,
) -> ModelTier {
    match agent_type.trim().to_ascii_lowercase().as_str() {
        "planner" | "architect" | "strategist" => ModelTier::Opus,
        "documenter" | "doc-editor" | "doc_editor" | "writer" | "docs" => config.docs_agent_tier,
        "researcher" | "reader" | "explorer" | "reviewer" | "verifier" | "general" => {
            config.subagent_tier
        }
        _ => config.subagent_tier,
    }
}

pub fn resolve_model_for_tier(
    settings: &Settings,
    tier: Option<ModelTier>,
    explicit_model: Option<&str>,
) -> String {
    if let Some(model) = explicit_model
        .map(str::trim)
        .filter(|model| !model.is_empty())
    {
        return model.to_string();
    }

    let Some(tier) = tier else {
        return settings.model.clone();
    };
    if !settings.model_routing.enabled {
        return settings.model.clone();
    }

    let configured = match tier {
        ModelTier::Haiku => &settings.model_routing.haiku_model,
        ModelTier::Sonnet => &settings.model_routing.sonnet_model,
        ModelTier::Opus => &settings.model_routing.opus_model,
    };
    if !configured.trim().is_empty() {
        return configured.trim().to_string();
    }

    // Anthropic has stable capability aliases in its own API family. Other
    // providers deliberately fall back to the selected model until the user
    // binds a role in Settings; guessing a cross-provider model would mix
    // pricing, capabilities, and credentials silently.
    if settings.provider == ProviderKind::Anthropic {
        return match tier {
            ModelTier::Haiku => "claude-haiku-4-5-20251001".to_string(),
            ModelTier::Sonnet => "claude-sonnet-4-6".to_string(),
            ModelTier::Opus => "claude-opus-4-6".to_string(),
        };
    }

    let current = settings.model.trim();
    if current.to_ascii_lowercase().contains(tier.as_str()) {
        current.to_string()
    } else {
        settings.model.clone()
    }
}

pub fn resolve_model_for_messages(settings: &Settings, messages: &[Message]) -> String {
    if messages.iter().all(|message| message.images.is_empty()) {
        return settings.model.clone();
    }

    let configured = settings.vision_model.trim();
    if !configured.is_empty() {
        return configured.to_string();
    }
    if settings.provider == ProviderKind::DeepSeek {
        return DEEPSEEK_DEFAULT_VISION_MODEL.to_string();
    }
    settings.model.clone()
}

pub fn route_candidates(
    primary: &str,
    fallback_models: &[String],
    max_additional: usize,
) -> Vec<String> {
    let max_additional = max_additional.min(MAX_FAILOVER_ATTEMPTS);
    let mut candidates = Vec::with_capacity(max_additional.saturating_add(1));
    for model in std::iter::once(primary).chain(fallback_models.iter().map(String::as_str)) {
        let model = model.trim();
        if model.is_empty() || candidates.iter().any(|candidate| candidate == model) {
            continue;
        }
        candidates.push(model.to_string());
        if candidates.len() >= max_additional.saturating_add(1) {
            break;
        }
    }
    candidates
}

pub fn is_retryable_failure(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    if [
        "401",
        "403",
        "unauthorized",
        "forbidden",
        "invalid api",
        "invalid request",
        "permission",
        "cancel",
        "interrupted",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    {
        return false;
    }
    [
        "429",
        "500",
        "502",
        "503",
        "504",
        "529",
        "timeout",
        "timed out",
        "connection reset",
        "connection refused",
        "overloaded",
        "rate limit",
        "capacity",
        "temporarily unavailable",
        "service unavailable",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn route_key(settings: &Settings, model: &str) -> String {
    format!("{}/{}", llm::provider_name(settings.provider), model.trim())
}

fn route_is_available(health: &RouteHealth, now: Instant) -> bool {
    health.opened_until.is_none_or(|until| until <= now)
}

fn record_success(state: &AppState, key: &str) {
    if let Ok(mut health) = state.model_route_health.lock() {
        health.insert(key.to_string(), RouteHealth::default());
    }
}

fn record_failure(state: &AppState, key: &str, error: &str, threshold: usize, cooldown: Duration) {
    if let Ok(mut routes) = state.model_route_health.lock() {
        let entry = routes.entry(key.to_string()).or_default();
        entry.consecutive_failures = entry.consecutive_failures.saturating_add(1);
        entry.last_error = Some(error.chars().take(400).collect());
        if entry.consecutive_failures >= threshold.max(1) {
            let until = Instant::now() + cooldown;
            entry.opened_until = Some(until);
            entry.opened_until_unix_ms = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64
                    + cooldown.as_millis() as u64,
            );
        }
    }
}

pub async fn stream_with_failover<FD, FA, FR>(
    request: FailoverRequest<'_>,
    mut on_attempt_start: FA,
    mut on_retry: FR,
    mut on_delta: FD,
) -> Result<RoutedTurn, String>
where
    FD: FnMut(llm::StreamDelta<'_>),
    FA: FnMut(&str),
    FR: FnMut(&str, &str),
{
    let FailoverRequest {
        state,
        settings,
        messages,
        tools,
        primary_model,
        fallback_models,
        cancel,
        request_cancel,
        session_id,
        purpose,
    } = request;
    let routing = &settings.model_routing;
    let max_additional = if routing.enabled && routing.auto_failover {
        routing.max_failover_attempts.min(MAX_FAILOVER_ATTEMPTS)
    } else {
        0
    };
    let candidates = route_candidates(primary_model, fallback_models, max_additional);
    let mut last_error = None;
    let mut available_candidates = Vec::new();
    let now = Instant::now();
    if let Ok(routes) = state.model_route_health.lock() {
        for model in &candidates {
            let key = route_key(settings, model);
            let available = routes
                .get(&key)
                .is_none_or(|entry| route_is_available(entry, now));
            if available {
                available_candidates.push(model.clone());
            }
        }
    } else {
        available_candidates = candidates.clone();
    }
    // If every circuit is open, probe the primary once rather than turning a
    // temporary outage into a permanent dead-end.
    if available_candidates.is_empty() {
        available_candidates.push(
            candidates
                .first()
                .cloned()
                .ok_or_else(|| "没有可用的模型路由候选。".to_string())?,
        );
    }

    for (index, model) in available_candidates.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err("请求已被用户中断。".to_string());
        }
        on_attempt_start(model);
        let mut attempt_settings = settings.clone();
        attempt_settings.model = model.clone();
        let request_started = Instant::now();
        let result = match request_cancel {
            Some(request_cancel) => {
                let combined_cancel = AtomicBool::new(
                    cancel.load(Ordering::Relaxed) || request_cancel.load(Ordering::Relaxed),
                );
                tokio::select! {
                    result = llm::stream_completion(
                        &state.http,
                        &attempt_settings,
                        messages,
                        tools,
                        &mut on_delta,
                        &combined_cancel,
                    ) => result,
                    _ = relay_cancel(cancel, request_cancel, &combined_cancel) => {
                        Ok(interrupted_turn())
                    }
                }
            }
            None => {
                llm::stream_completion(
                    &state.http,
                    &attempt_settings,
                    messages,
                    tools,
                    &mut on_delta,
                    cancel,
                )
                .await
            }
        };
        let _ = crate::usage::record(
            state,
            crate::usage::UsageRecordInput {
                session_id,
                provider: &llm::provider_name(settings.provider),
                model,
                purpose,
                usage: result.as_ref().ok().and_then(|turn| turn.usage),
                fallback_total_tokens: budget::estimate_messages_tokens(messages).saturating_add(
                    result
                        .as_ref()
                        .map_or(0, |turn| budget::estimate_text_tokens(&turn.content)),
                ) as u64,
                latency_ms: request_started.elapsed().as_millis() as u64,
                status: usage_status(&result),
            },
        );
        match result {
            Ok(turn) => {
                record_success(state, &route_key(settings, model));
                return Ok(RoutedTurn { turn });
            }
            Err(error) => {
                let retryable = is_retryable_failure(&error);
                record_failure(
                    state,
                    &route_key(settings, model),
                    &error,
                    routing.failure_threshold,
                    Duration::from_secs(routing.cooldown_seconds),
                );
                last_error = Some(error.clone());
                let next = available_candidates.get(index + 1);
                if !retryable || next.is_none() {
                    return Err(error);
                }
                on_retry(model, next.expect("checked above"));
            }
        }
    }

    Err(last_error.unwrap_or_else(|| "模型路由失败。".to_string()))
}

async fn relay_cancel(
    global_cancel: &AtomicBool,
    request_cancel: &AtomicBool,
    combined_cancel: &AtomicBool,
) {
    loop {
        if global_cancel.load(Ordering::Relaxed) || request_cancel.load(Ordering::Relaxed) {
            combined_cancel.store(true, Ordering::Relaxed);
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn interrupted_turn() -> llm::AssistantTurn {
    llm::AssistantTurn {
        content: String::new(),
        tool_calls: Vec::new(),
        finish_reason: "interrupted".to_string(),
        usage: None,
    }
}

fn usage_status(result: &Result<llm::AssistantTurn, String>) -> &'static str {
    match result {
        Ok(turn) if turn.finish_reason == "interrupted" => "interrupted",
        Ok(_) => "success",
        Err(_) => "failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::conversation::ImageAttachment;
    use crate::store::{ModelRoutingConfig, Settings};

    #[test]
    fn candidates_are_deduplicated_and_bounded() {
        let models = vec!["fallback".into(), "fallback".into(), "third".into()];
        assert_eq!(
            route_candidates("primary", &models, 1),
            vec!["primary", "fallback"]
        );
    }

    #[test]
    fn candidate_limit_is_bounded_even_for_untrusted_configuration() {
        let models = (0..20)
            .map(|idx| format!("fallback-{idx}"))
            .collect::<Vec<_>>();
        assert_eq!(route_candidates("primary", &models, usize::MAX).len(), 6);
    }

    #[test]
    fn interrupted_turn_is_not_counted_as_success() {
        assert_eq!(usage_status(&Ok(interrupted_turn())), "interrupted");
        assert_eq!(
            usage_status(&Ok(llm::AssistantTurn {
                finish_reason: "stop".to_string(),
                ..interrupted_turn()
            })),
            "success"
        );
        assert_eq!(usage_status(&Err("provider failed".to_string())), "failed");
    }

    #[test]
    fn retry_classifier_is_fail_closed_for_auth_and_cancel() {
        assert!(is_retryable_failure("HTTP 503 overloaded"));
        assert!(is_retryable_failure("request timed out"));
        assert!(!is_retryable_failure("HTTP 401 unauthorized"));
        assert!(!is_retryable_failure("cancelled by user"));
    }

    #[test]
    fn anthropic_roles_resolve_without_changing_other_providers() {
        let mut settings = Settings::default();
        settings.model_routing.enabled = true;
        settings.provider = ProviderKind::Anthropic;
        assert_eq!(
            resolve_model_for_tier(&settings, Some(ModelTier::Haiku), None),
            "claude-haiku-4-5-20251001"
        );
        settings.provider = ProviderKind::OpenRouter;
        assert_eq!(
            resolve_model_for_tier(&settings, Some(ModelTier::Haiku), None),
            settings.model
        );
        let _ = ModelRoutingConfig::default();
    }

    #[test]
    fn image_turns_use_the_configured_vision_model() {
        let mut settings = Settings::default();
        settings.model = "deepseek-v4-flash".to_string();
        settings.vision_model = "deepseek-v4-flash-vision-exp".to_string();
        let text_only = vec![Message::user("hello")];
        let with_image = vec![Message::user_with_images(
            "what is this?",
            vec![ImageAttachment {
                mime_type: "image/png".to_string(),
                data: "YWJj".to_string(),
                name: Some("frame.png".to_string()),
            }],
        )];

        assert_eq!(resolve_model_for_messages(&settings, &text_only), settings.model);
        assert_eq!(
            resolve_model_for_messages(&settings, &with_image),
            "deepseek-v4-flash-vision-exp"
        );

        settings.vision_model.clear();
        assert_eq!(
            resolve_model_for_messages(&settings, &with_image),
            "deepseek-v4-flash-vision-exp"
        );
    }
}
