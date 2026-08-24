//! agent IPC Adapter.

use crate::permission::PermissionResponse;
use crate::*;
use std::sync::atomic::Ordering;
use tauri::AppHandle;

pub(crate) async fn send(app: AppHandle, state: &AppState, text: String) -> Result<(), String> {
    send_input(
        app,
        state,
        text,
        Vec::new(),
        Vec::new(),
        None,
        None,
        Some(desktop_companion_context()),
        None,
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
        text,
        Vec::new(),
        agent_names,
        None,
        None,
        Some(desktop_companion_context()),
        None,
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
    send_input(
        app,
        state,
        text,
        images,
        agent_names,
        None,
        None,
        Some(desktop_companion_context()),
        None,
    )
    .await
}

pub(crate) async fn send_minecraft_chat(
    app: AppHandle,
    state: &AppState,
    chat: crate::minecraft::MinecraftChatMessage,
) -> Result<(), String> {
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
    send_input(
        app,
        state,
        chat.utterance,
        Vec::new(),
        Vec::new(),
        Some(overlay),
        None,
        Some(context),
        Some("[[minecraft:no_reply]]".to_string()),
    )
    .await
}

pub(crate) async fn send_minecraft_vision(
    app: AppHandle,
    state: &AppState,
    prompt: String,
    image: agent::conversation::ImageAttachment,
) -> Result<(), String> {
    let stored = format!("[Minecraft 视觉异常] {prompt}");
    let overlay = "当前输入是 Minecraft 残差感知筛选出的局部视觉异常。使用当前人物包与长期记忆分析图片；如果发现具体而紧急的游戏危险，可以调用 Minecraft MCP 工具采取行动。最后简短记录你看到了什么，不要调用 minecraft_say。".to_string();
    send_input(
        app,
        state,
        prompt,
        vec![image],
        Vec::new(),
        Some(overlay),
        Some(stored),
        Some(agent::conversation::ConversationContext {
            scene: "minecraft_vision".to_string(),
            channel: Some("vision".to_string()),
            speaker: None,
            is_primary_user: None,
            addressed_to_ai: None,
        }),
        None,
    )
    .await
}

async fn send_input(
    app: AppHandle,
    state: &AppState,
    text: String,
    images: Vec<agent::conversation::ImageAttachment>,
    agent_names: Vec<String>,
    system_overlay: Option<String>,
    stored_user_text: Option<String>,
    conversation_context: Option<agent::conversation::ConversationContext>,
    silent_assistant_marker: Option<String>,
) -> Result<(), String> {
    let st = state;
    let session_id = st.sessions.lock().unwrap().active.clone();
    let entrypoint = if agent_names.is_empty() {
        agent::session_engine::TurnEntrypoint::Send
    } else {
        agent::session_engine::TurnEntrypoint::SendWithAgents
    };
    let turn = agent::session_engine::begin_turn(
        &app,
        st,
        agent::session_engine::TurnStart {
            entrypoint,
            session_id: session_id.clone(),
            input: text.clone(),
            workflow_run_id: None,
            agent_names: agent_names.clone(),
        },
    )?;
    let events = agent::session_engine::TurnEventEmitter::new(&app, st);
    let mut should_drive_goal = true;
    let slash_outcome = if images.is_empty() && system_overlay.is_none() {
        agent::slash::dispatch(&app, st, &turn.session_id, text.clone(), &events).await
    } else {
        None
    };
    let res = if let Some(outcome) = slash_outcome {
        should_drive_goal = outcome.1;
        outcome.0
    } else if let Some(risk) = companion::detect_high_risk_expression(text.trim()) {
        should_drive_goal = false;
        persist_direct_reply_with_images(
            st,
            &turn.session_id,
            stored_user_text.clone().unwrap_or_else(|| text.clone()),
            images,
            risk.support_message.clone(),
            conversation_context.clone(),
        );
        events.assistant_done(risk.support_message);
        Ok(())
    } else {
        agent::run_turn_with_options(
            &app,
            st,
            &turn.session_id,
            text,
            agent::TurnOptions {
                agent_names,
                user_images: images,
                system_overlay,
                stored_user_text,
                conversation_context,
                silent_assistant_marker,
                ..agent::TurnOptions::default()
            },
        )
        .await
    };
    let res = if res.is_ok() && should_drive_goal && !st.cancel.load(Ordering::Relaxed) {
        agent::goal::drive_after_turn(&app, st, &turn.session_id).await
    } else {
        res
    };
    let status = if st.cancel.load(Ordering::Relaxed) {
        agent::session_engine::TurnStatus::Interrupted
    } else if res.is_ok() {
        agent::session_engine::TurnStatus::Completed
    } else {
        agent::session_engine::TurnStatus::Failed
    };
    let error = res.as_ref().err().cloned();
    agent::session_engine::finish_turn(&app, st, &turn, status, error);
    res
}

pub(crate) fn interrupt(app: AppHandle, state: &AppState) {
    agent::session_engine::request_interrupt(&app, state);
    // 立即唤醒所有正在等待的确认（按「中断」处理），否则确认弹窗的 await 会把整轮卡住最长 5 分钟
    let mut pending = state.pending_confirms.lock().unwrap();
    for (_, tx) in pending.drain() {
        let _ = tx.send(PermissionResponse::deny_once());
    }
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
