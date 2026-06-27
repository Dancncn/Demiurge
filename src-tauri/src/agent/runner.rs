//! 组件 3：Agent 循环。整个系统的心脏。
//! 输入 + 上下文 → 调 LLM → 若请求工具则执行 → 把 tool_result 喂回 → 重复，直到给出最终答复。
use std::sync::atomic::Ordering;

use serde_json::json;
use tauri::{AppHandle, Emitter};

use super::conversation::Message;
use super::{context, prompt};
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

pub async fn run_turn(
    app: &AppHandle,
    state: &crate::AppState,
    user_text: String,
) -> Result<(), String> {
    state.cancel.store(false, Ordering::Relaxed);

    let settings = state.settings.lock().unwrap().clone();
    // 捕获本轮的目标会话 id：即便用户中途切换会话，写入也始终落到这一段对话
    let sid = state.sessions.lock().unwrap().active.clone();

    // 取当前角色包人格，拼装分区化 system prompt
    let packs_dir = state.packs_dir.lock().unwrap().clone();
    let persona_text = match pack::load_pack(&packs_dir, &settings.current_pack) {
        Ok(p) => p.persona_text,
        Err(_) => String::new(),
    };
    let system = prompt::build(state, &settings, &persona_text);
    let tools_schema = tools::schemas_json();

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
            s.messages.push(Message::user(user_text));
            if s.title == "新对话" {
                s.title = store::derive_title(&s.messages);
            }
            s.updated_at = store::now_millis();
        }
    }
    state.persist_sessions();

    for _step in 0..MAX_STEPS {
        if state.cancel.load(Ordering::Relaxed) {
            let _ = app.emit("assistant-interrupted", ());
            break;
        }

        // 组装本轮请求消息：system + 裁剪后的历史
        let full: Vec<Message> = {
            let mut storeg = state.sessions.lock().unwrap();
            let msgs = if let Some(s) = storeg.get_mut(&sid) {
                context::trim(&mut s.messages, settings.max_context_chars);
                s.messages.clone()
            } else {
                Vec::new()
            };
            let mut v = Vec::with_capacity(msgs.len() + 1);
            v.push(Message::system(system.clone()));
            v.extend(msgs);
            v
        };

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

        // 被用户中断：保留已生成的部分正文
        if turn.finish_reason == "interrupted" {
            if !turn.content.is_empty() {
                push(Message::assistant_text(turn.content));
            }
            state.persist_sessions();
            let _ = app.emit("assistant-interrupted", ());
            return Ok(());
        }

        // 没有工具调用 → 最终答复
        if turn.tool_calls.is_empty() {
            push(Message::assistant_text(turn.content.clone()));
            state.persist_sessions();
            let _ = app.emit("assistant-done", turn.content);
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
                    let response = permission::confirm(
                        app,
                        state,
                        PermissionRequest {
                            tool: &name,
                            args_pretty: &pretty,
                            description,
                            risk,
                            decision: decision.clone(),
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

            push(Message::tool_result(tc.id.clone(), name, result));
        }

        state.persist_sessions();

        // 工具执行阶段被中断：补齐配对后结束本轮
        if state.cancel.load(Ordering::Relaxed) {
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
    state.persist_sessions();
    Ok(())
}
