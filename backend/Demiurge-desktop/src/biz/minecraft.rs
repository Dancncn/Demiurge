//! Minecraft companion event orchestration.
//!
//! Routes attributed multiplayer chat through the normal Demiurge Agent turn
//! so the active character pack, scene-aware history and memory extraction
//! remain authoritative. Important game events use the same memory namespace.

use std::future::Future;
use std::sync::atomic::Ordering;
use std::time::Duration;

use serde_json::json;
use tauri::{AppHandle, Manager};

use crate::minecraft::{
    chat_message, experience_memory, parse_notification, vision_request, MinecraftChatMessage,
    MinecraftNotification,
};
use crate::{agent, mcp, permission, tools, AppState};

const NO_MINECRAFT_REPLY: &str = "[[minecraft:no_reply]]";

pub(crate) fn start_event_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let notification = app.state::<AppState>().mcp.next_notification().await;
            let Some(notification) = parse_notification(notification) else {
                continue;
            };
            if let Err(error) = handle_notification(&app, notification).await {
                eprintln!("Minecraft integration event failed: {error}");
            }
        }
    });
}

async fn handle_notification(
    app: &AppHandle,
    notification: MinecraftNotification,
) -> Result<(), String> {
    let session_id = app
        .state::<AppState>()
        .sessions
        .lock()
        .unwrap()
        .active
        .clone();
    if let Some(chat) = chat_message(&notification.event)? {
        return handle_game_chat(app, session_id, notification.server_name, chat).await;
    }
    if let Some(request) = vision_request(&notification.event)? {
        while app.state::<AppState>().busy.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        return crate::biz::agent::send_minecraft_vision(
            app.clone(),
            app.state::<AppState>().inner(),
            session_id,
            request.prompt,
            agent::conversation::ImageAttachment {
                mime_type: request.mime_type,
                data: request.data,
                name: Some("minecraft-vision-anomaly".to_string()),
            },
        )
        .await;
    }
    if let Some(memory) = experience_memory(&notification.event) {
        remember_experience(app.state::<AppState>().inner(), &memory)?;
    }
    Ok(())
}

async fn handle_game_chat(
    app: &AppHandle,
    session_id: String,
    server_name: String,
    chat: MinecraftChatMessage,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    while state.busy.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    crate::biz::agent::send_minecraft_chat(
        app.clone(),
        state.inner(),
        session_id,
        server_name,
        chat,
    )
    .await
}

pub(super) async fn reply_to_completed_turn(
    app: &AppHandle,
    state: &AppState,
    turn: &agent::session_engine::TurnHandle,
    server_name: &str,
    outcome: &agent::TurnOutcome,
) -> Result<(), String> {
    let agent::TurnOutcome::Completed { assistant_text } = outcome else {
        return Ok(());
    };
    if assistant_text.trim().is_empty()
        || assistant_text
            .trim()
            .eq_ignore_ascii_case(NO_MINECRAFT_REPLY)
        || !owns_running_turn(state, turn)
    {
        return Ok(());
    }
    let exposed_name = mcp::original_tool_name(state, server_name, "minecraft_say")?;
    let definition = tools::definition_for_state(state, &exposed_name)
        .ok_or_else(|| "Minecraft reply tool is no longer available.".to_string())?;
    let context = permission::context_for_session(state, &turn.session_id);
    let args = json!({ "message": assistant_text });
    let pretty = serde_json::to_string_pretty(&args).map_err(|error| error.to_string())?;
    let allowed = authorize_reply(state, &context, &definition, |decision| {
        permission::confirm(
            app,
            state,
            permission::PermissionRequest {
                session_id: &turn.session_id,
                tool: &exposed_name,
                args_pretty: &pretty,
                description: definition.description,
                risk: definition.risk,
                decision,
                summary: format!("Send this completed reply to Minecraft server `{server_name}`."),
                preview: Some(assistant_text.clone()),
                affected_paths: Vec::new(),
            },
        )
    })
    .await;
    if !allowed {
        return Ok(());
    }
    let exposed_name = &exposed_name;
    send_completed_reply(state, turn, outcome, |part| async move {
        mcp::call_tool(state, exposed_name, json!({ "message": part }))
            .await
            .map(|_| ())
    })
    .await
}

async fn authorize_reply<F, Fut>(
    state: &AppState,
    context: &permission::PermissionContext,
    definition: &tools::ToolDefinition,
    confirm: F,
) -> bool
where
    F: FnOnce(permission::PermissionDecision) -> Fut,
    Fut: Future<Output = permission::PermissionResponse>,
{
    let mut decision = permission::decide_for_mode(
        state,
        context,
        definition.name,
        definition.permission,
        definition.risk,
    );
    permission::audit(state, context, definition.name, &decision);
    match decision.effect {
        tools::PermissionEffect::Allow => true,
        tools::PermissionEffect::Deny => false,
        tools::PermissionEffect::Ask => {
            let response = confirm(decision.clone()).await;
            let remembered =
                permission::remember_response(state, context, definition.name, &response);
            decision.effect = if response.allow {
                tools::PermissionEffect::Allow
            } else {
                tools::PermissionEffect::Deny
            };
            decision.scope = remembered
                .as_ref()
                .copied()
                .unwrap_or(tools::PermissionScope::Once);
            decision.source = permission::PermissionDecisionSource::UserOverride;
            decision.reason = if response.allow {
                "用户允许发送 Minecraft 回复。"
            } else {
                "用户拒绝发送 Minecraft 回复。"
            }
            .to_string();
            if let Err(error) = remembered {
                decision.reason.push_str(&format!(" 规则未持久化：{error}"));
            }
            permission::audit(state, context, definition.name, &decision);
            response.allow
        }
    }
}

async fn send_completed_reply<F, Fut>(
    state: &AppState,
    turn: &agent::session_engine::TurnHandle,
    outcome: &agent::TurnOutcome,
    mut send: F,
) -> Result<(), String>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<(), String>>,
{
    let agent::TurnOutcome::Completed { assistant_text } = outcome else {
        return Ok(());
    };
    if assistant_text
        .trim()
        .eq_ignore_ascii_case(NO_MINECRAFT_REPLY)
    {
        return Ok(());
    }
    for part in split_minecraft_chat(assistant_text, 220) {
        // A captured result is not authority to act after cancellation, turn
        // replacement or completion. Check again for every external write.
        if !owns_running_turn(state, turn) {
            return Ok(());
        }
        send(part).await?;
    }
    Ok(())
}

fn owns_running_turn(state: &AppState, turn: &agent::session_engine::TurnHandle) -> bool {
    let runtime = state.session_engine.lock().unwrap();
    !state.cancel.load(Ordering::Acquire)
        && runtime.active_turn.as_ref().is_some_and(|active| {
            active.id == turn.id
                && active.session_id == turn.session_id
                && active.status == agent::session_engine::TurnStatus::Running
        })
}

fn remember_experience(state: &AppState, text: &str) -> Result<(), String> {
    let (data, sandbox, packs, pack_id, session_id) = crate::biz::memory::memory_context(state);
    agent::memory::add_entry(
        &data,
        &sandbox,
        &packs,
        &pack_id,
        &session_id,
        "user",
        "minecraft_experience",
        &format!("[scene=minecraft] {text}"),
    )?;
    Ok(())
}

fn split_minecraft_chat(value: &str, max_chars: usize) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    for ch in value.trim().chars() {
        if current.chars().count() >= max_chars {
            parts.push(current);
            current = String::new();
        }
        current.push(ch);
    }
    if !current.trim().is_empty() {
        parts.push(current);
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply_fixture() -> (AppState, agent::session_engine::TurnHandle) {
        use agent::session_engine::{TurnEntrypoint, TurnHandle, TurnRunState, TurnStatus};
        let state = AppState::new(reqwest::Client::new());
        let turn = TurnHandle {
            id: "game-turn".into(),
            session_id: "game-session".into(),
        };
        state.busy.store(true, Ordering::Release);
        state.session_engine.lock().unwrap().active_turn = Some(TurnRunState {
            id: turn.id.clone(),
            session_id: turn.session_id.clone(),
            entrypoint: TurnEntrypoint::Send,
            status: TurnStatus::Running,
            input_preview: String::new(),
            workflow_run_id: None,
            agent_names: Vec::new(),
            started_at: 0,
            updated_at: 0,
            completed_at: None,
            error: None,
        });
        (state, turn)
    }

    #[tokio::test]
    async fn minecraft_relay_only_sends_completed_reply_from_its_running_turn() {
        let (state, turn) = reply_fixture();
        let mut sent = Vec::new();
        for outcome in [
            agent::TurnOutcome::Interrupted,
            agent::TurnOutcome::BudgetExceeded,
            agent::TurnOutcome::StepLimitReached,
            agent::TurnOutcome::Completed {
                assistant_text: NO_MINECRAFT_REPLY.into(),
            },
        ] {
            send_completed_reply(&state, &turn, &outcome, |part| {
                sent.push(part);
                std::future::ready(Ok(()))
            })
            .await
            .unwrap();
        }
        assert!(sent.is_empty());
        // Sidebar selection is not the reply owner.
        state.sessions.lock().unwrap().active = "other-session".into();
        let complete = agent::TurnOutcome::Completed {
            assistant_text: "the actual game reply".into(),
        };
        send_completed_reply(&state, &turn, &complete, |part| {
            assert!(state.busy.load(Ordering::Acquire));
            sent.push(part);
            std::future::ready(Ok(()))
        })
        .await
        .unwrap();
        assert_eq!(sent, vec!["the actual game reply"]);
        state
            .session_engine
            .lock()
            .unwrap()
            .active_turn
            .as_mut()
            .unwrap()
            .id = "new-turn".into();
        send_completed_reply(&state, &turn, &complete, |part| {
            sent.push(part);
            std::future::ready(Ok(()))
        })
        .await
        .unwrap();
        assert_eq!(sent.len(), 1, "a newer turn cannot authorize an old reply");
    }

    #[tokio::test]
    async fn minecraft_relay_rechecks_cancel_before_every_part_and_propagates_send_failure() {
        let (state, turn) = reply_fixture();
        let complete = agent::TurnOutcome::Completed {
            assistant_text: "你".repeat(450),
        };
        let mut sent = Vec::new();
        state.cancel.store(true, Ordering::Release);
        send_completed_reply(&state, &turn, &complete, |part| {
            sent.push(part);
            std::future::ready(Ok(()))
        })
        .await
        .unwrap();
        assert!(
            sent.is_empty(),
            "cancelled partial model output must not be sent"
        );
        state.cancel.store(false, Ordering::Release);
        send_completed_reply(&state, &turn, &complete, |part| {
            sent.push(part);
            state.cancel.store(true, Ordering::Release);
            std::future::ready(Ok(()))
        })
        .await
        .unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].chars().count(), 220);
        state.cancel.store(false, Ordering::Release);
        let mut attempts = 0;
        let error = send_completed_reply(&state, &turn, &complete, |_| {
            attempts += 1;
            std::future::ready(Err("bridge disconnected".into()))
        })
        .await
        .unwrap_err();
        assert_eq!(error, "bridge disconnected");
        assert_eq!(attempts, 1);
    }

    #[tokio::test]
    async fn minecraft_reply_respects_plan_deny_and_cancellation_after_approval() {
        let (state, turn) = reply_fixture();
        let root = std::env::temp_dir().join(format!(
            "demiurge_minecraft_permission_{}",
            crate::store::new_session_id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        *state.data_dir.lock().unwrap() = root.clone();
        let mut context = permission::PermissionContext::default();
        context.session_id = Some(turn.session_id.clone());
        let mut definition = tools::definition_for("http_get").unwrap();
        definition.name = "mcp__minecraft__minecraft_say";
        definition.risk = tools::ToolRisk::External;
        definition.permission = tools::PermissionPolicy {
            effect: tools::PermissionEffect::Ask,
            scope: tools::PermissionScope::Once,
            reason: "External Minecraft reply",
        };
        state.settings.lock().unwrap().permission_mode = crate::store::PermissionMode::Plan;
        let allowed = authorize_reply(&state, &context, &definition, |_| async {
            panic!("Plan mode must reject an external write before confirmation")
        })
        .await;
        assert!(!allowed);
        state.settings.lock().unwrap().permission_mode = crate::store::PermissionMode::Default;
        state
            .session_permission_rules
            .lock()
            .unwrap()
            .entry(turn.session_id.clone())
            .or_default()
            .insert(
                definition.name.into(),
                permission::PermissionRule {
                    tool: definition.name.into(),
                    effect: tools::PermissionEffect::Deny,
                    scope: tools::PermissionScope::Session,
                    reason: "test denial".into(),
                    updated_at: 0,
                },
            );
        assert!(
            !authorize_reply(&state, &context, &definition, |_| async {
                panic!("The remembered deny must not request confirmation")
            })
            .await
        );
        state.session_permission_rules.lock().unwrap().clear();
        let allowed = authorize_reply(&state, &context, &definition, |_| async {
            state.cancel.store(true, Ordering::Release);
            permission::PermissionResponse {
                allow: true,
                scope: tools::PermissionScope::Once,
            }
        })
        .await;
        assert!(allowed);
        let mut sent = 0;
        send_completed_reply(
            &state,
            &turn,
            &agent::TurnOutcome::Completed {
                assistant_text: "approved then stopped".into(),
            },
            |_| {
                sent += 1;
                std::future::ready(Ok(()))
            },
        )
        .await
        .unwrap();
        assert_eq!(sent, 0, "approval does not override a later stop");
        let audit = std::fs::read_to_string(root.join("permission_audit.jsonl")).unwrap();
        let entries = audit
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0]["effect"], "deny");
        assert_eq!(entries[1]["effect"], "deny");
        assert_eq!(entries[2]["effect"], "ask");
        assert_eq!(entries[3]["effect"], "allow");
        assert!(entries.iter().all(
            |entry| entry["session_id"] == turn.session_id && entry["tool"] == definition.name
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn splits_long_replies_for_minecraft_chat_limit() {
        let parts = split_minecraft_chat(&"你".repeat(450), 220);
        assert_eq!(parts.len(), 3);
        assert!(parts.iter().all(|part| part.chars().count() <= 220));
    }

    #[test]
    fn recognizes_the_private_no_reply_marker() {
        assert!(NO_MINECRAFT_REPLY.eq_ignore_ascii_case("[[MINECRAFT:NO_REPLY]]"));
    }
}
