//! 组件 3：Agent 循环。整个系统的心脏。
//! 输入 + 上下文 → 调 LLM → 若请求工具则执行 → 把 tool_result 喂回 → 重复，直到给出最终答复。
use std::sync::atomic::Ordering;

use serde_json::json;
use tauri::AppHandle;

use super::conversation::{ImageAttachment, Message};
use super::{
    budget, context, custom, goal, memory, prompt, session_engine, summary, workflow_journal,
};
use crate::{companion, llm, pack, permission, store, tools};

/// 一轮内最多的工具往返次数，防止模型陷入死循环
const MAX_STEPS: usize = 16;

fn assistant_error_payload(err: &str) -> session_engine::AssistantErrorEvent {
    let lower = err.to_ascii_lowercase();
    let (kind, hint) = if lower.contains("401")
        || lower.contains("403")
        || lower.contains("api key")
        || lower.contains("unauthorized")
    {
        (
            "llm",
            "Provider authentication failed. Re-save the API key in Settings and retry.",
        )
    } else if lower.contains("timeout") || lower.contains("timed out") {
        (
            "network",
            "The provider timed out. Retry once; if it repeats, lower context size or switch endpoint.",
        )
    } else if lower.contains("network")
        || lower.contains("connection")
        || lower.contains("dns")
        || lower.contains("econn")
    {
        (
            "network",
            "The app could not reach the provider. Check base URL, proxy, and local network access.",
        )
    } else {
        (
            "llm",
            "Verify the provider, base URL, model name, API key, and tool capability settings.",
        )
    };
    session_engine::AssistantErrorEvent {
        kind: kind.to_string(),
        message: err.to_string(),
        hint: hint.to_string(),
        retryable: true,
    }
}

fn empty_assistant_response_message(finish_reason: &str, language: &str) -> String {
    let reason = finish_reason.trim();
    let reason = if reason.is_empty() { "stop" } else { reason };
    let is_zh =
        language.eq_ignore_ascii_case("zh") || language.to_ascii_lowercase().starts_with("zh-");

    if is_zh {
        match reason {
            "content_filter" => "模型这次没有返回可见内容，响应可能被提供商的安全策略过滤了。请换一种问法，或检查当前模型/端点设置。".to_string(),
            "length" => "模型这次在输出 token 上限耗尽前没有返回可见内容。请提高预留输出 token，或缩短上下文后重试。".to_string(),
            "stop" => "模型这次没有返回可见内容。请重试；如果持续出现，请检查当前模型、端点和流式输出设置。".to_string(),
            other => format!(
                "模型这次没有返回可见内容（finish_reason: {other}）。请重试；如果持续出现，请检查当前模型、端点和能力设置。"
            ),
        }
    } else {
        match reason {
            "content_filter" => "The model returned no visible content. The provider may have filtered the response. Try rephrasing, or check the current model and endpoint settings.".to_string(),
            "length" => "The model returned no visible content before the output token limit was reached. Increase reserved output tokens or shorten the context, then retry.".to_string(),
            "stop" => "The model returned no visible content. Retry once; if it keeps happening, check the current model, endpoint, and streaming settings.".to_string(),
            other => format!(
                "The model returned no visible content (finish_reason: {other}). Retry once; if it keeps happening, check the current model, endpoint, and capability settings."
            ),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct TurnOptions {
    pub system_overlay: Option<String>,
    pub stored_user_text: Option<String>,
    pub workflow_run_id: Option<String>,
    pub agent_names: Vec<String>,
    pub token_budget: Option<budget::TokenBudgetState>,
    pub user_images: Vec<ImageAttachment>,
    pub conversation_context: Option<super::conversation::ConversationContext>,
    /// A model-only control reply that should be persisted for continuity but
    /// represented as an empty completion in the desktop projection.
    pub silent_assistant_marker: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TurnOutcome {
    Completed { assistant_text: String },
    Interrupted,
    BudgetExceeded,
    StepLimitReached,
}

pub async fn run_turn_with_options(
    app: &AppHandle,
    state: &crate::AppState,
    session_id: &str,
    user_text: String,
    options: TurnOptions,
) -> Result<(), String> {
    run_turn_with_result(app, state, session_id, user_text, options)
        .await
        .map(|_| ())
}

pub(crate) async fn run_turn_with_result(
    app: &AppHandle,
    state: &crate::AppState,
    session_id: &str,
    user_text: String,
    options: TurnOptions,
) -> Result<TurnOutcome, String> {
    if state.sessions.lock().unwrap().get(session_id).is_none() {
        return Err("The target session no longer exists.".to_string());
    }
    let permission_context = permission::context_for_session(state, session_id);
    // Only begin_turn resets cancellation. A Goal continuation or delayed
    // initialization must never clear a stop requested for its owning turn.
    let events = session_engine::TurnEventEmitter::new(app, state);

    let mut settings = state.settings.lock().unwrap().clone();
    crate::mcp::ensure_initialized(state).await;
    let selected_agents = custom::resolve_selected(state, &options.agent_names)?;
    if let Some(max_input_tokens) = selected_agents.max_input_tokens {
        settings.max_input_tokens = settings.max_input_tokens.min(max_input_tokens);
    }
    if let Some(reserved_output_tokens) = selected_agents.reserved_output_tokens {
        settings.reserved_output_tokens = settings
            .reserved_output_tokens
            .min(reserved_output_tokens)
            .min(settings.max_input_tokens.saturating_sub(512));
    }
    let max_steps = selected_agents
        .max_steps
        .unwrap_or(MAX_STEPS)
        .min(MAX_STEPS);
    let mut turn_budget = options.token_budget.clone().or_else(|| {
        selected_agents
            .max_total_tokens
            .map(|total| budget::TokenBudgetState::new(Some(total)))
    });
    custom::record_runtime_start(state, &selected_agents.definitions);
    // The caller captures this immutable target before any initialization
    // await. Never derive the destination from the mutable active session.
    let sid = session_id.to_string();
    let session_store = session_engine::SessionTurnStore::new(state, sid.clone());

    // 取当前角色包人格，后续每次请求会结合最新会话摘要拼装 system prompt
    let packs_dir = state.packs_dir.lock().unwrap().clone();
    let persona_text = match pack::load_pack(&packs_dir, &settings.current_pack) {
        Ok(p) => p.persona_text,
        Err(_) => String::new(),
    };
    let profile = llm::ProviderProfile::for_kind(settings.provider);
    let allowed_tool_names = selected_agents
        .allowed_tools
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let tools_schema = if !profile.supports_tools {
        profile.empty_tool_schema()
    } else if allowed_tool_names.is_empty() {
        tools::main_schemas_json_for_state(state, profile.tool_schema_dialect)
    } else {
        tools::schemas_json_for_names_state(state, profile.tool_schema_dialect, &allowed_tool_names)
    };
    let stored_user_text = options
        .stored_user_text
        .clone()
        .unwrap_or_else(|| user_text.clone());
    let original_user_text = stored_user_text.clone();
    let memory_user_text = options
        .conversation_context
        .as_ref()
        .map(|context| context.annotate(&original_user_text))
        .unwrap_or_else(|| original_user_text.clone());
    if let Some(run_id) = &options.workflow_run_id {
        let _ = workflow_journal::append(
            state,
            run_id,
            "run_started",
            json!({
                "user_text": stored_user_text.clone(),
                "agents": selected_agents
                    .definitions
                    .iter()
                    .map(|agent| agent.name.clone())
                    .collect::<Vec<_>>()
            }),
        );
    }

    // 向目标会话追加一条消息（并刷新 updated_at）
    let push = |msg: Message| session_store.append_message(msg);

    // 追加用户消息；若标题仍是默认值，用首条用户消息生成标题
    session_store.append_user_message_with_context(
        stored_user_text.clone(),
        options.user_images.clone(),
        options.conversation_context.clone(),
    );

    for _step in 0..max_steps {
        if state.cancel.load(Ordering::Relaxed) {
            events.assistant_interrupted();
            return Ok(TurnOutcome::Interrupted);
        }

        // 组装本轮请求消息：system + token-aware 裁剪后的历史。若裁剪掉旧消息，先滚动更新会话摘要。
        let (original_msgs, original_summary) = session_store.snapshot();
        let mut msgs = original_msgs.clone();
        let mut session_summary = original_summary.clone();
        let build_system = |summary: Option<&str>| {
            let mut system = prompt::build_for_session_input(
                state,
                &sid,
                &settings,
                &persona_text,
                summary,
                &memory_user_text,
            );
            if settings.permission_mode == store::PermissionMode::Plan {
                apply_system_overlay(&mut system, Some(plan_mode_overlay()));
            }
            apply_system_overlay(&mut system, Some(&selected_agents.prompt_overlay));
            apply_system_overlay(&mut system, options.system_overlay.as_deref());
            system
        };
        let mut system = build_system(session_summary.as_deref());
        let current_budget =
            budget::history_budget_for_profile(&settings, profile, &system, &tools_schema, &msgs);
        let mut removed_messages = context::trim_collect_removed_by_tokens(
            &mut msgs,
            current_budget.history_budget_tokens,
        );
        let mut compaction_ready = !removed_messages.is_empty();

        while !removed_messages.is_empty() {
            if state.cancel.load(Ordering::Relaxed) {
                compaction_ready = false;
                break;
            }
            if let Some((summary_messages, summary_tools)) = summary::build_summary_request(
                &settings,
                session_summary.as_deref(),
                &removed_messages,
                &state.cancel,
            ) {
                session_store.append_model_request(
                    &summary_messages,
                    &summary_tools,
                    &llm::provider_name(settings.provider),
                    &settings.model,
                    "compaction",
                );
            }
            match summary::update_session_summary(
                state,
                &sid,
                &state.http,
                &settings,
                session_summary.as_deref(),
                &removed_messages,
                &state.cancel,
            )
            .await
            {
                Ok(next_summary) => {
                    if state.cancel.load(Ordering::Relaxed) || next_summary == session_summary {
                        eprintln!(
                            "Context summary did not advance; persisted history was kept unchanged."
                        );
                        compaction_ready = false;
                        break;
                    }
                    session_summary = next_summary;
                    system = build_system(session_summary.as_deref());
                    let next_budget = budget::history_budget_for_profile(
                        &settings,
                        profile,
                        &system,
                        &tools_schema,
                        &msgs,
                    );
                    removed_messages = context::trim_collect_removed_by_tokens(
                        &mut msgs,
                        next_budget.history_budget_tokens,
                    );
                }
                Err(error) => {
                    eprintln!(
                        "Context summary update failed; persisted history was kept unchanged: {error}"
                    );
                    compaction_ready = false;
                    break;
                }
            }
        }

        // A cancelled summary may return unchanged history. Do not turn that
        // normal stop into another provider request or an assistant error.
        if state.cancel.load(Ordering::Relaxed) {
            events.assistant_interrupted();
            return Ok(TurnOutcome::Interrupted);
        }

        if compaction_ready {
            let _ = session_store.commit_compaction(
                &original_msgs,
                &original_summary,
                msgs.clone(),
                session_summary.clone(),
            )?;
        }

        let full: Vec<Message> = {
            let mut v = Vec::with_capacity(msgs.len() + 1);
            v.push(Message::system(system));
            v.extend(msgs);
            v
        };
        let contains_images = full.iter().any(|message| !message.images.is_empty());
        let primary_model = crate::model_routing::resolve_model_for_messages(&settings, &full);
        // A provider-local text fallback may not understand images. Fail clearly
        // instead of silently retrying the same multimodal payload on one.
        let fallback_models: &[String] = if contains_images {
            &[]
        } else {
            &settings.model_routing.fallback_models
        };

        if turn_budget
            .as_ref()
            .is_some_and(|budget| budget.is_exhausted())
        {
            let message = "（已达到本轮 token 硬预算，已停止继续调用模型）".to_string();
            push(Message::assistant_text(message.clone()));
            if let Some(run_id) = &options.workflow_run_id {
                let _ = workflow_journal::append(
                    state,
                    run_id,
                    "token_budget_exhausted",
                    json!({ "used": turn_budget.as_ref().map(|b| b.used_total()), "total": turn_budget.as_ref().and_then(|b| b.total) }),
                );
            }
            events.assistant_done(message);
            return Ok(TurnOutcome::BudgetExceeded);
        }

        events.assistant_start();

        let delta_events = events.clone();
        let retry_events = events.clone();
        let routed = match crate::model_routing::stream_with_failover(
            crate::model_routing::FailoverRequest {
                state,
                settings: &settings,
                messages: &full,
                tools: &tools_schema,
                primary_model: &primary_model,
                fallback_models,
                cancel: &state.cancel,
                request_cancel: None,
                session_id: &sid,
                purpose: "agent_turn",
            },
            |model| {
                // Every model-visible retry has its own durable envelope.
                session_store.append_model_request(
                    &full,
                    &tools_schema,
                    &llm::provider_name(settings.provider),
                    model,
                    "agent_turn",
                );
            },
            |_failed_model, _next_model| {
                // Reset the active UI projection before a fallback stream so
                // partial tokens from the failed provider are not duplicated.
                retry_events.assistant_start();
            },
            |delta| match delta {
                llm::StreamChunk::Content(text) => delta_events.assistant_delta(text),
                llm::StreamChunk::Reasoning(text) => delta_events.assistant_reasoning(text),
            },
        )
        .await
        {
            Ok(routed) => routed,
            Err(err) => {
                if state.cancel.load(Ordering::Relaxed) {
                    events.assistant_interrupted();
                    return Ok(TurnOutcome::Interrupted);
                }
                custom::record_runtime_error(state, &selected_agents.definitions, &err);
                events.assistant_error(assistant_error_payload(&err));
                return Err(err);
            }
        };
        let crate::model_routing::RoutedTurn { turn, .. } = routed;

        let estimated_usage = budget::estimate_messages_tokens(&full)
            .saturating_add(budget::estimate_text_tokens(&turn.content));
        let agent_usage_tokens = turn
            .usage
            .and_then(|usage| usage.total_or_sum())
            .unwrap_or(estimated_usage);
        custom::record_runtime_usage(state, &selected_agents.definitions, agent_usage_tokens);

        if let Some(budget_state) = &mut turn_budget {
            budget_state.record_usage_or_estimate(turn.usage, estimated_usage);
            if let Some(run_id) = &options.workflow_run_id {
                let _ = workflow_journal::append(
                    state,
                    run_id,
                    "token_budget_used",
                    json!({
                        "used": budget_state.used_total(),
                        "used_exact": budget_state.used_exact,
                        "used_estimated": budget_state.used_estimated,
                        "total": budget_state.total,
                        "remaining": budget_state.remaining(),
                    }),
                );
            }
        }

        let exact_usage_recorded = goal::add_provider_usage(state, &sid, turn.usage.as_ref());

        // 被用户中断：保留已生成的部分正文
        if turn.finish_reason == "interrupted" || state.cancel.load(Ordering::Relaxed) {
            if !turn.content.is_empty() {
                push(Message::assistant_text(turn.content));
            }
            if let Some(run_id) = &options.workflow_run_id {
                let _ = workflow_journal::append(
                    state,
                    run_id,
                    "run_interrupted",
                    json!({ "reason": "model_interrupted" }),
                );
            }
            events.assistant_interrupted();
            return Ok(TurnOutcome::Interrupted);
        }

        // 没有工具调用 → 最终答复
        if turn.tool_calls.is_empty() {
            let assistant_text = if turn.content.trim().is_empty() {
                empty_assistant_response_message(&turn.finish_reason, &settings.language)
            } else {
                turn.content.clone()
            };
            push(Message::assistant_text(assistant_text.clone()));
            if !exact_usage_recorded {
                goal::add_estimated_tokens(state, &sid, &original_user_text);
                goal::add_estimated_tokens(state, &sid, &assistant_text);
            }
            if let Some(run_id) = &options.workflow_run_id {
                let _ = workflow_journal::append(
                    state,
                    run_id,
                    "run_done",
                    json!({ "assistant_text": assistant_text.clone() }),
                );
            }
            events.assistant_done(assistant_completion_event_text(
                &assistant_text,
                options.silent_assistant_marker.as_deref(),
            ));

            let sandbox_dir = state.sandbox_dir.lock().unwrap().clone();
            let packs_dir = state.packs_dir.lock().unwrap().clone();
            if let Err(error) = memory::extract_and_update(
                state,
                &sid,
                &state.http,
                &settings,
                &sandbox_dir,
                &packs_dir,
                &settings.current_pack,
                &memory_user_text,
                &assistant_text,
                options.conversation_context.as_ref(),
                &state.cancel,
            )
            .await
            {
                eprintln!("Automatic memory extraction was not persisted: {error}");
            }
            let data_dir = state.data_dir.lock().unwrap().clone();
            let _ = companion::extract_memory_to_queue(
                &state.http,
                &settings,
                state,
                &data_dir,
                &sid,
                &memory_user_text,
                &assistant_text,
                options.conversation_context.as_ref(),
                &state.cancel,
            )
            .await;
            return Ok(TurnOutcome::Completed { assistant_text });
        }

        // 有工具调用 → 先把带 tool_calls 的 assistant 消息入历史
        let content_opt = if turn.content.is_empty() {
            None
        } else {
            Some(turn.content.clone())
        };
        push(Message::assistant_tools(
            content_opt,
            turn.tool_calls.clone(),
        ));

        // 逐个执行工具。注意：带 tool_calls 的 assistant 消息已入历史，
        // 因此必须为「每一个」tool_call 都补一条 tool 结果，否则下一轮请求会因配对缺失而被判 400。
        for tc in &turn.tool_calls {
            let outcome = super::tool_call::execute(
                app,
                state,
                &permission_context,
                &events,
                tc,
                options.workflow_run_id.as_deref(),
            )
            .await;
            if !exact_usage_recorded {
                goal::add_estimated_tokens(state, &sid, &tc.function.arguments);
                goal::add_estimated_tokens(state, &sid, &outcome.ui_result);
            }
            push(outcome.message);
        }

        // 工具执行阶段被中断：补齐配对后结束本轮
        if state.cancel.load(Ordering::Relaxed) {
            if let Some(run_id) = &options.workflow_run_id {
                let _ = workflow_journal::append(
                    state,
                    run_id,
                    "run_interrupted",
                    json!({ "reason": "user_cancelled_during_tools" }),
                );
            }
            events.assistant_interrupted();
            return Ok(TurnOutcome::Interrupted);
        }
        // 继续下一轮，让模型基于工具结果作答
    }

    // 达到步数上限
    events.assistant_done("（已达到本轮工具调用次数上限）".to_string());
    if let Some(run_id) = &options.workflow_run_id {
        let _ = workflow_journal::append(
            state,
            run_id,
            "run_stopped",
            json!({ "reason": "max_steps" }),
        );
    }
    Ok(TurnOutcome::StepLimitReached)
}

fn plan_mode_overlay() -> &'static str {
    "当前处于 Plan Mode。你必须先探索和制定计划，不能请求写文件、shell、外部发布或系统能力工具。只允许使用只读工具，以及在计划完成时调用 write_plan 写入一份 Markdown 实施计划。计划应包含背景、推荐做法、关键文件、范围边界和验证步骤。用户批准计划前不要执行实现。"
}

fn apply_system_overlay(system: &mut String, overlay: Option<&str>) {
    let Some(overlay) = overlay else {
        return;
    };
    if overlay.trim().is_empty() {
        return;
    }
    system.push_str("\n\n---\n临时任务指令：\n");
    system.push_str(overlay.trim());
}

fn assistant_completion_event_text(assistant_text: &str, silent_marker: Option<&str>) -> String {
    if silent_marker.is_some_and(|marker| assistant_text.trim().eq_ignore_ascii_case(marker.trim()))
    {
        String::new()
    } else {
        assistant_text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_assistant_response_message_explains_empty_turns() {
        let zh = empty_assistant_response_message("stop", "zh");
        assert!(zh.contains("没有返回可见内容"));

        let en = empty_assistant_response_message("length", "en");
        assert!(en.contains("output token limit"));

        let unknown = empty_assistant_response_message("provider_blank", "en");
        assert!(unknown.contains("finish_reason: provider_blank"));
    }

    #[test]
    fn minecraft_no_reply_marker_stays_out_of_desktop_projection() {
        assert_eq!(
            assistant_completion_event_text(
                " [[MINECRAFT:NO_REPLY]] ",
                Some("[[minecraft:no_reply]]")
            ),
            ""
        );
        assert_eq!(
            assistant_completion_event_text("我来帮你。", Some("[[minecraft:no_reply]]")),
            "我来帮你。"
        );
    }
}
