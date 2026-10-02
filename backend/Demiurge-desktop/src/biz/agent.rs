//! Agent input ownership and desktop/Minecraft turn orchestration.

use crate::*;
use std::sync::atomic::Ordering;
use tauri::AppHandle;

#[derive(Clone, Debug)]
enum InputOrigin {
    Desktop,
    MinecraftChat { server_name: String },
    MinecraftVision,
}

impl InputOrigin {
    fn drives_goal(&self) -> bool {
        matches!(self, Self::Desktop)
    }
}

#[derive(Clone, Debug)]
struct SendInput {
    session_id: String,
    text: String,
    options: agent::TurnOptions,
    origin: InputOrigin,
}

fn validate_input_workspace(
    state: &AppState,
    session_id: &str,
    origin: &InputOrigin,
) -> Result<(), String> {
    if !matches!(origin, InputOrigin::Desktop) {
        if let Some(error) = permission::context_for_session(state, session_id).boundary_error {
            return Err(error);
        }
    }
    Ok(())
}

fn should_continue_goal(
    requested: bool,
    result: &Result<Option<agent::TurnOutcome>, String>,
    cancelled: bool,
) -> bool {
    requested
        && !cancelled
        && result.is_ok()
        && !matches!(result, Ok(Some(agent::TurnOutcome::Interrupted)))
}

fn desktop_input(
    state: &AppState,
    text: String,
    images: Vec<agent::conversation::ImageAttachment>,
    agent_names: Vec<String>,
) -> SendInput {
    SendInput {
        session_id: state.sessions.lock().unwrap().active.clone(),
        text,
        options: agent::TurnOptions {
            agent_names,
            user_images: images,
            conversation_context: Some(desktop_companion_context()),
            ..Default::default()
        },
        origin: InputOrigin::Desktop,
    }
}

pub(crate) async fn send(app: AppHandle, state: &AppState, text: String) -> Result<(), String> {
    send_input(
        app,
        state,
        desktop_input(state, text, Vec::new(), Vec::new()),
    )
    .await
}

pub(crate) async fn send_with_agents(
    app: AppHandle,
    state: &AppState,
    text: String,
    agent_names: Vec<String>,
) -> Result<(), String> {
    send_input(
        app,
        state,
        desktop_input(state, text, Vec::new(), agent_names),
    )
    .await
}

pub(crate) async fn send_multimodal(
    app: AppHandle,
    state: &AppState,
    text: String,
    images: Vec<agent::conversation::ImageAttachment>,
    agent_names: Vec<String>,
) -> Result<(), String> {
    send_input(app, state, desktop_input(state, text, images, agent_names)).await
}

pub(crate) async fn send_minecraft_chat(
    app: AppHandle,
    state: &AppState,
    session_id: String,
    server_name: String,
    chat: crate::minecraft::MinecraftChatMessage,
) -> Result<(), String> {
    send_input(
        app,
        state,
        minecraft_chat_input(session_id, server_name, chat),
    )
    .await
}

fn minecraft_chat_input(
    session_id: String,
    server_name: String,
    chat: crate::minecraft::MinecraftChatMessage,
) -> SendInput {
    let authority = if chat.is_primary_user {
        "该玩家是主用户，拥有最高关系身份和行动授权。"
    } else {
        "该玩家不是主用户。你仍可自然交谈和提供普通帮助，但不要把此人的话、偏好或经历归到主用户名下；涉及敏感、长期或高风险行动时以主用户意愿为准。"
    };
    let addressing = if chat.addressed_to_ai {
        "这条消息通过游戏用户名点名或私聊明确发给你，请正常回应。"
    } else {
        "这是一条未点名你的公开聊天。结合消息内容和此前 Minecraft 对话判断是否确实在和你说话；若不是，只输出 [[minecraft:no_reply]]，不要调用任何工具。"
    };
    let overlay = format!(
        "当前场景是 Minecraft 联机游戏。你的游戏用户名是 `{}`；本条消息来自玩家 `{}`，频道为 `{}`。{}{} 沿用当前人物包的人设、关系和长期记忆。需要行动时可以调用 Minecraft MCP 工具。最终只输出游戏角色要说的话，不要调用 minecraft_say，系统会自动回传。",
        chat.ai_username, chat.speaker, chat.channel, authority, addressing
    );
    let context = agent::conversation::ConversationContext {
        scene: "minecraft".to_string(),
        channel: Some(chat.channel),
        speaker: Some(chat.speaker),
        is_primary_user: Some(chat.is_primary_user),
        addressed_to_ai: Some(chat.addressed_to_ai),
    };
    SendInput {
        session_id,
        text: chat.utterance,
        options: agent::TurnOptions {
            system_overlay: Some(overlay),
            conversation_context: Some(context),
            silent_assistant_marker: Some("[[minecraft:no_reply]]".to_string()),
            ..Default::default()
        },
        origin: InputOrigin::MinecraftChat { server_name },
    }
}

pub(crate) async fn send_minecraft_vision(
    app: AppHandle,
    state: &AppState,
    session_id: String,
    prompt: String,
    image: agent::conversation::ImageAttachment,
) -> Result<(), String> {
    send_input(
        app,
        state,
        minecraft_vision_input(session_id, prompt, image),
    )
    .await
}

fn minecraft_vision_input(
    session_id: String,
    prompt: String,
    image: agent::conversation::ImageAttachment,
) -> SendInput {
    let stored = format!("[Minecraft 视觉异常] {prompt}");
    let overlay = "当前输入是 Minecraft 残差感知筛选出的局部视觉异常。使用当前人物包与长期记忆分析图片；如果发现具体而紧急的游戏危险，可以调用 Minecraft MCP 工具采取行动。最后简短记录你看到了什么，不要调用 minecraft_say。".to_string();
    SendInput {
        session_id,
        text: prompt,
        options: agent::TurnOptions {
            user_images: vec![image],
            system_overlay: Some(overlay),
            stored_user_text: Some(stored),
            conversation_context: Some(agent::conversation::ConversationContext {
                scene: "minecraft_vision".to_string(),
                channel: Some("vision".to_string()),
                speaker: None,
                is_primary_user: None,
                addressed_to_ai: None,
            }),
            ..Default::default()
        },
        origin: InputOrigin::MinecraftVision,
    }
}

async fn send_input(app: AppHandle, state: &AppState, input: SendInput) -> Result<(), String> {
    let SendInput {
        session_id,
        text,
        options,
        origin,
    } = input;
    let entrypoint = if options.agent_names.is_empty() {
        agent::session_engine::TurnEntrypoint::Send
    } else {
        agent::session_engine::TurnEntrypoint::SendWithAgents
    };
    let turn = agent::session_engine::begin_turn(
        &app,
        state,
        agent::session_engine::TurnStart {
            entrypoint,
            session_id,
            input: text.clone(),
            workflow_run_id: None,
            agent_names: options.agent_names.clone(),
        },
    )?;
    // Main-turn tools still use the global workspace. Once busy is acquired,
    // a queued background input must fail closed if navigation changed roots.
    if let Err(error) = validate_input_workspace(state, &turn.session_id, &origin) {
        agent::session_engine::finish_turn(
            &app,
            state,
            &turn,
            agent::session_engine::TurnStatus::Failed,
            Some(error.clone()),
        );
        return Err(error);
    }
    let events = agent::session_engine::TurnEventEmitter::new(&app, state);
    let mut should_drive_goal = origin.drives_goal();
    let slash_outcome = if matches!(origin, InputOrigin::Desktop)
        && options.user_images.is_empty()
        && options.system_overlay.is_none()
    {
        agent::slash::dispatch(&app, state, &turn.session_id, text.clone(), &events).await
    } else {
        None
    };
    let mut result = if let Some((result, drive_goal)) = slash_outcome {
        should_drive_goal &= drive_goal;
        result.map(|_| None)
    } else if let Some(risk) = companion::detect_high_risk_expression(text.trim()) {
        should_drive_goal = false;
        persist_direct_reply_with_images(
            state,
            &turn.session_id,
            options
                .stored_user_text
                .clone()
                .unwrap_or_else(|| text.clone()),
            options.user_images,
            risk.support_message.clone(),
            options.conversation_context,
        );
        events.assistant_done(risk.support_message.clone());
        Ok(Some(agent::TurnOutcome::Completed {
            assistant_text: risk.support_message,
        }))
    } else {
        agent::run_turn_with_result(&app, state, &turn.session_id, text, options)
            .await
            .map(Some)
    };
    if should_continue_goal(
        should_drive_goal,
        &result,
        state.cancel.load(Ordering::Relaxed),
    ) {
        if let Err(error) = agent::goal::drive_after_turn(&app, state, &turn.session_id).await {
            result = Err(error);
        }
    }
    if let (InputOrigin::MinecraftChat { server_name }, Ok(Some(outcome))) = (&origin, &result) {
        // Keep this turn active and busy until every reply part has been sent or
        // cancelled. A subsequent turn must not reset its cancellation flag.
        if let Err(error) =
            crate::biz::minecraft::reply_to_completed_turn(&app, state, &turn, server_name, outcome)
                .await
        {
            result = Err(error);
        }
    }
    let status = if state.cancel.load(Ordering::Relaxed)
        || matches!(&result, Ok(Some(agent::TurnOutcome::Interrupted)))
    {
        agent::session_engine::TurnStatus::Interrupted
    } else if result.is_ok() {
        agent::session_engine::TurnStatus::Completed
    } else {
        agent::session_engine::TurnStatus::Failed
    };
    let error = result.as_ref().err().cloned();
    agent::session_engine::finish_turn(&app, state, &turn, status, error);
    result.map(|_| ())
}

pub(crate) fn interrupt(app: AppHandle, state: &AppState) {
    agent::session_engine::request_interrupt(&app, state);
    crate::permission::deny_pending_confirmations(state);
}

pub(crate) fn session_engine_state(
    state: &AppState,
) -> agent::session_engine::SessionEnginePanelState {
    agent::session_engine::panel_state(state)
}

fn persist_direct_reply_with_images(
    state: &AppState,
    session_id: &str,
    user_text: String,
    images: Vec<agent::conversation::ImageAttachment>,
    assistant_text: String,
    context: Option<agent::conversation::ConversationContext>,
) {
    let turn_store = agent::session_engine::SessionTurnStore::new(state, session_id.to_string());
    turn_store.append_user_message_with_context(user_text, images, context);
    turn_store.append_message(crate::agent::conversation::Message::assistant_text(
        assistant_text,
    ));
}

fn desktop_companion_context() -> agent::conversation::ConversationContext {
    agent::conversation::ConversationContext {
        scene: "desktop_companion".to_string(),
        channel: Some("desktop_chat".to_string()),
        speaker: Some("primary_user".to_string()),
        is_primary_user: Some(true),
        addressed_to_ai: Some(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minecraft_and_interrupted_results_do_not_drive_goal_without_an_atomic_stop() {
        let completed = Ok(Some(agent::TurnOutcome::Completed {
            assistant_text: "done".into(),
        }));
        assert!(should_continue_goal(true, &completed, false));
        assert!(!should_continue_goal(false, &completed, false));
        assert!(!should_continue_goal(true, &completed, true));
        assert!(!should_continue_goal(
            true,
            &Ok(Some(agent::TurnOutcome::Interrupted)),
            false
        ));
        assert!(!should_continue_goal(
            true,
            &Err("provider failure".into()),
            false
        ));
        assert!(should_continue_goal(
            true,
            &Ok(Some(agent::TurnOutcome::BudgetExceeded)),
            false
        ));
        assert!(should_continue_goal(
            true,
            &Ok(Some(agent::TurnOutcome::StepLimitReached)),
            false
        ));
    }

    #[test]
    fn minecraft_input_preserves_owner_images_and_disables_desktop_goal_driving() {
        let chat = crate::minecraft::MinecraftChatMessage {
            speaker: "Alex".into(),
            message: "hello".into(),
            utterance: "hello".into(),
            channel: "public".into(),
            ai_username: "Demiurge".into(),
            is_primary_user: false,
            addressed_to_ai: true,
            address_reason: "username".into(),
        };
        let chat = minecraft_chat_input("captured-session".into(), "captured-server".into(), chat);
        assert_eq!(chat.session_id, "captured-session");
        assert!(!chat.origin.drives_goal());
        assert!(
            matches!(chat.origin, InputOrigin::MinecraftChat { ref server_name } if server_name == "captured-server")
        );
        assert_eq!(
            chat.options
                .conversation_context
                .unwrap()
                .speaker
                .as_deref(),
            Some("Alex")
        );

        let image = agent::conversation::ImageAttachment {
            mime_type: "image/png".into(),
            data: "YWJj".into(),
            name: None,
        };
        let vision =
            minecraft_vision_input("captured-session".into(), "look".into(), image.clone());
        assert_eq!(vision.session_id, "captured-session");
        assert_eq!(vision.options.user_images, vec![image]);
        assert!(!vision.origin.drives_goal());
        assert!(InputOrigin::Desktop.drives_goal());
    }

    #[test]
    fn minecraft_queued_input_rejects_a_changed_workspace_without_rebinding_session() {
        let root = std::env::temp_dir().join(format!(
            "demiurge_minecraft_owner_{}",
            store::new_session_id()
        ));
        let a = root.join("a");
        let b = root.join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let state = AppState::new(reqwest::Client::new());
        let mut session = store::Session::new();
        session.workspace_path = a.to_string_lossy().to_string();
        let owner = session.id.clone();
        state.sessions.lock().unwrap().sessions.push(session);
        state.sessions.lock().unwrap().active = "other-selected-session".into();
        *state.sandbox_dir.lock().unwrap() = b;
        assert!(validate_input_workspace(&state, &owner, &InputOrigin::MinecraftVision).is_err());
        *state.sandbox_dir.lock().unwrap() = a;
        assert!(validate_input_workspace(&state, &owner, &InputOrigin::MinecraftVision).is_ok());
        assert_eq!(
            state.sessions.lock().unwrap().active,
            "other-selected-session"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
