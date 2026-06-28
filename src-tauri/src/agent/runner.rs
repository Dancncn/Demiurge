//! 组件 3：Agent 循环。整个系统的心脏。
//! 输入 + 上下文 → 调 LLM → 若请求工具则执行 → 把 tool_result 喂回 → 重复，直到给出最终答复。
use std::sync::atomic::Ordering;

use serde_json::json;
use tauri::{AppHandle, Emitter};

use super::conversation::Message;
use super::{budget, context, custom, goal, memory, prompt, summary, workflow_journal};
use crate::{llm, pack, permission, store, tools};
use permission::PermissionRequest;

/// 单工具结果回传给前端时的最大展示长度（完整结果仍会进上下文喂回模型）
const UI_RESULT_CAP: usize = 2000;
/// 一轮内最多的工具往返次数，防止模型陷入死循环
const MAX_STEPS: usize = 16;

fn truncate_ui(s: &str) -> String {
    if s.chars().count() <= UI_RESULT_CAP {
        s.to_string()
    } else {
        let head: String = s.chars().take(UI_RESULT_CAP).collect();
        format!("{head}…（已截断，共 {} 字）", s.chars().count())
    }
}

#[derive(Clone, Debug, Default)]
pub struct TurnOptions {
    pub system_overlay: Option<String>,
    pub stored_user_text: Option<String>,
    pub workflow_run_id: Option<String>,
    pub agent_names: Vec<String>,
    pub token_budget: Option<budget::TokenBudgetState>,
}

pub async fn run_turn(
    app: &AppHandle,
    state: &crate::AppState,
    user_text: String,
) -> Result<(), String> {
    run_turn_with_options(app, state, user_text, TurnOptions::default()).await
}

pub async fn run_turn_with_options(
    app: &AppHandle,
    state: &crate::AppState,
    user_text: String,
    options: TurnOptions,
) -> Result<(), String> {
    state.cancel.store(false, Ordering::Relaxed);

    let mut settings = state.settings.lock().unwrap().clone();
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
    // 捕获本轮的目标会话 id：即便用户中途切换会话，写入也始终落到这一段对话
    let sid = state.sessions.lock().unwrap().active.clone();

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
        tools::main_schemas_json_for(profile.tool_schema_dialect)
    } else {
        tools::schemas_json_for_names(profile.tool_schema_dialect, &allowed_tool_names)
    };
    let stored_user_text = options
        .stored_user_text
        .clone()
        .unwrap_or_else(|| user_text.clone());
    let original_user_text = stored_user_text.clone();
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
    let push = |msg: Message| {
        let mut storeg = state.sessions.lock().unwrap();
        if let Some(s) = storeg.get_mut(&sid) {
            s.messages.push(msg);
            s.updated_at = store::now_millis();
        }
    };

    // 追加用户消息；若标题仍是默认值，用首条用户消息生成标题
    {
        let mut storeg = state.sessions.lock().unwrap();
        if let Some(s) = storeg.get_mut(&sid) {
            s.messages.push(Message::user(stored_user_text.clone()));
            if s.title == "新对话" {
                s.title = store::derive_title(&s.messages);
            }
            s.updated_at = store::now_millis();
        }
    }
    state.persist_sessions();

    for _step in 0..max_steps {
        if state.cancel.load(Ordering::Relaxed) {
            let _ = app.emit("assistant-interrupted", ());
            break;
        }

        // 组装本轮请求消息：system + token-aware 裁剪后的历史。若裁剪掉旧消息，先滚动更新会话摘要。
        let (mut msgs, mut session_summary) = {
            let storeg = state.sessions.lock().unwrap();
            if let Some(s) = storeg.get(&sid) {
                (s.messages.clone(), s.summary.clone())
            } else {
                (Vec::new(), None)
            }
        };

        let mut system = prompt::build(state, &settings, &persona_text, session_summary.as_deref());
        apply_system_overlay(&mut system, Some(&selected_agents.prompt_overlay));
        apply_system_overlay(&mut system, options.system_overlay.as_deref());
        let mut current_budget = budget::history_budget(&settings, &system, &tools_schema, &msgs);
        let mut removed_messages = context::trim_collect_removed_by_tokens(
            &mut msgs,
            current_budget.history_budget_tokens,
        );
        let mut should_persist_trim = !removed_messages.is_empty();

        if !removed_messages.is_empty() && !state.cancel.load(Ordering::Relaxed) {
            if let Ok(next_summary) = summary::update_session_summary(
                &state.http,
                &settings,
                session_summary.as_deref(),
                &removed_messages,
                &state.cancel,
            )
            .await
            {
                session_summary = next_summary;
                let mut storeg = state.sessions.lock().unwrap();
                if let Some(s) = storeg.get_mut(&sid) {
                    s.messages = msgs.clone();
                    s.summary = session_summary.clone();
                    s.updated_at = store::now_millis();
                }
                drop(storeg);
                state.persist_sessions();

                system = prompt::build(state, &settings, &persona_text, session_summary.as_deref());
                apply_system_overlay(&mut system, options.system_overlay.as_deref());
                current_budget = budget::history_budget(&settings, &system, &tools_schema, &msgs);
                removed_messages = context::trim_collect_removed_by_tokens(
                    &mut msgs,
                    current_budget.history_budget_tokens,
                );
                should_persist_trim = should_persist_trim || !removed_messages.is_empty();
                if !removed_messages.is_empty() {
                    let mut storeg = state.sessions.lock().unwrap();
                    if let Some(s) = storeg.get_mut(&sid) {
                        s.messages = msgs.clone();
                        s.updated_at = store::now_millis();
                    }
                    drop(storeg);
                    state.persist_sessions();
                }
            }
        }

        if should_persist_trim {
            let mut storeg = state.sessions.lock().unwrap();
            if let Some(s) = storeg.get_mut(&sid) {
                s.messages = msgs.clone();
                s.updated_at = store::now_millis();
            }
            drop(storeg);
            state.persist_sessions();
        }

        let full: Vec<Message> = {
            let mut v = Vec::with_capacity(msgs.len() + 1);
            v.push(Message::system(system));
            v.extend(msgs);
            v
        };

        if turn_budget
            .as_ref()
            .is_some_and(|budget| budget.is_exhausted())
        {
            let message = "（已达到本轮 token 硬预算，已停止继续调用模型）".to_string();
            push(Message::assistant_text(message.clone()));
            state.persist_sessions();
            if let Some(run_id) = &options.workflow_run_id {
                let _ = workflow_journal::append(
                    state,
                    run_id,
                    "token_budget_exhausted",
                    json!({ "used": turn_budget.as_ref().map(|b| b.used_total()), "total": turn_budget.as_ref().and_then(|b| b.total) }),
                );
            }
            let _ = app.emit("assistant-done", message);
            return Ok(());
        }

        let _ = app.emit("assistant-start", ());

        let turn = llm::stream_completion(
            &state.http,
            &settings,
            &full,
            &tools_schema,
            |delta| {
                let _ = app.emit("assistant-delta", delta);
            },
            &state.cancel,
        )
        .await?;

        if let Some(budget_state) = &mut turn_budget {
            let estimated = budget::estimate_messages_tokens(&full)
                .saturating_add(budget::estimate_text_tokens(&turn.content));
            budget_state.record_usage_or_estimate(turn.usage, estimated);
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
        if turn.finish_reason == "interrupted" {
            if !turn.content.is_empty() {
                push(Message::assistant_text(turn.content));
            }
            state.persist_sessions();
            if let Some(run_id) = &options.workflow_run_id {
                let _ = workflow_journal::append(
                    state,
                    run_id,
                    "run_interrupted",
                    json!({ "reason": "model_interrupted" }),
                );
            }
            let _ = app.emit("assistant-interrupted", ());
            return Ok(());
        }

        // 没有工具调用 → 最终答复
        if turn.tool_calls.is_empty() {
            let assistant_text = turn.content.clone();
            push(Message::assistant_text(assistant_text.clone()));
            if !exact_usage_recorded {
                goal::add_estimated_tokens(state, &sid, &original_user_text);
                goal::add_estimated_tokens(state, &sid, &assistant_text);
            }
            state.persist_sessions();
            if let Some(run_id) = &options.workflow_run_id {
                let _ = workflow_journal::append(
                    state,
                    run_id,
                    "run_done",
                    json!({ "assistant_text": assistant_text.clone() }),
                );
            }
            let _ = app.emit("assistant-done", assistant_text.clone());

            let sandbox_dir = state.sandbox_dir.lock().unwrap().clone();
            let _ = memory::extract_and_update(
                &state.http,
                &settings,
                &sandbox_dir,
                &original_user_text,
                &assistant_text,
                &state.cancel,
            )
            .await;
            return Ok(());
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
            let name = tc.function.name.clone();

            // 已被用户中断：不再执行后续工具，但仍补一条结果以保持 tool_calls/结果配对
            if state.cancel.load(Ordering::Relaxed) {
                push(Message::tool_result(
                    tc.id.clone(),
                    name,
                    "[已被用户中断，未执行]",
                ));
                continue;
            }

            let args: serde_json::Value =
                serde_json::from_str(&tc.function.arguments).unwrap_or_else(|_| json!({}));
            let tool_def = tools::definition_for(&name);

            let _ = app.emit(
                "tool-start",
                json!({
                    "tool_call_id": tc.id,
                    "name": name,
                    "args": args,
                    "description": tool_def.as_ref().map(|t| t.description),
                    "risk": tool_def.as_ref().map(|t| t.risk),
                    "permission_effect": tool_def.as_ref().map(|t| t.permission.effect),
                    "concurrency": tool_def.as_ref().map(|t| t.concurrency),
                    "output_policy": tool_def.as_ref().map(|t| t.output_policy),
                }),
            );
            if let Some(run_id) = &options.workflow_run_id {
                let _ = workflow_journal::append(
                    state,
                    run_id,
                    "tool_started",
                    json!({ "tool_call_id": tc.id.clone(), "name": name.clone(), "args": args.clone() }),
                );
            }

            // 权限门（confirm 等待期间若用户点「停止」，interrupt 会立即唤醒并返回 deny-once）
            let default_policy = tools::permission_policy_for(&name);
            let mut decision = permission::decide(state, &name, default_policy);
            permission::audit(state, &name, &decision);
            let allowed = match decision.effect {
                tools::PermissionEffect::Allow => true,
                tools::PermissionEffect::Deny => false,
                tools::PermissionEffect::Ask => {
                    let pretty = serde_json::to_string_pretty(&args).unwrap_or_default();
                    let description = tool_def
                        .as_ref()
                        .map(|t| t.description)
                        .unwrap_or("未知工具");
                    let risk = tool_def
                        .as_ref()
                        .map(|t| t.risk)
                        .unwrap_or(tools::ToolRisk::Privileged);
                    let preview = tools::confirmation_preview(state, &name, args.clone());
                    let summary = tools::permission_summary(&name, &args);
                    let response = permission::confirm(
                        app,
                        state,
                        PermissionRequest {
                            tool: &name,
                            args_pretty: &pretty,
                            description,
                            risk,
                            decision: decision.clone(),
                            summary,
                            preview,
                        },
                    )
                    .await;
                    let _ = permission::remember_response(state, &name, &response);
                    decision.effect = if response.allow {
                        tools::PermissionEffect::Allow
                    } else {
                        tools::PermissionEffect::Deny
                    };
                    decision.scope = response.scope;
                    decision.source = permission::PermissionDecisionSource::UserOverride;
                    decision.reason = if response.allow {
                        "用户在确认弹窗中允许本次操作。".to_string()
                    } else {
                        "用户在确认弹窗中拒绝本次操作。".to_string()
                    };
                    permission::audit(state, &name, &decision);
                    response.allow
                }
            };

            let interrupted = state.cancel.load(Ordering::Relaxed);
            let result = if !allowed && interrupted {
                "[已被用户中断]".to_string()
            } else if !allowed {
                "[用户拒绝了该操作]".to_string()
            } else {
                match tools::execute(state, &name, args.clone()).await {
                    Ok(s) => s,
                    Err(e) => format!("错误：{e}"),
                }
            };

            let _ = app.emit(
                "tool-end",
                json!({ "tool_call_id": tc.id, "name": name, "ok": allowed, "result": truncate_ui(&result) }),
            );
            if let Some(run_id) = &options.workflow_run_id {
                let _ = workflow_journal::append(
                    state,
                    run_id,
                    "tool_done",
                    json!({
                        "tool_call_id": tc.id.clone(),
                        "name": name.clone(),
                        "ok": allowed,
                        "result": truncate_ui(&result),
                    }),
                );
            }

            if !exact_usage_recorded {
                goal::add_estimated_tokens(state, &sid, &tc.function.arguments);
                goal::add_estimated_tokens(state, &sid, &truncate_ui(&result));
            }
            push(Message::tool_result(tc.id.clone(), name, result));
        }

        state.persist_sessions();

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
            let _ = app.emit("assistant-interrupted", ());
            return Ok(());
        }
        // 继续下一轮，让模型基于工具结果作答
    }

    // 达到步数上限
    let _ = app.emit(
        "assistant-done",
        "（已达到本轮工具调用次数上限）".to_string(),
    );
    if let Some(run_id) = &options.workflow_run_id {
        let _ = workflow_journal::append(
            state,
            run_id,
            "run_stopped",
            json!({ "reason": "max_steps" }),
        );
    }
    state.persist_sessions();
    Ok(())
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
