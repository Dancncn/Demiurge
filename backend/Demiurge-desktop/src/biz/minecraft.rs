//! Minecraft companion event orchestration.
//!
//! Routes attributed multiplayer chat through the normal Demiurge Agent turn
//! so the active character pack, scene-aware history and memory extraction
//! remain authoritative. Important game events use the same memory namespace.

use std::sync::atomic::Ordering;
use std::time::Duration;

use serde_json::json;
use tauri::{AppHandle, Manager};

use crate::minecraft::{
    chat_message, experience_memory, parse_notification, vision_request, MinecraftChatMessage,
    MinecraftNotification,
};
use crate::{agent, mcp, AppState};

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
    if let Some(chat) = chat_message(&notification.event)? {
        return handle_game_chat(app, &notification.server_name, chat).await;
    }
    if let Some(request) = vision_request(&notification.event)? {
        while app.state::<AppState>().busy.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        return crate::biz::agent::send_minecraft_vision(
            app.clone(),
            app.state::<AppState>().inner(),
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
    server_name: &str,
    chat: MinecraftChatMessage,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    while state.busy.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    let session_id = state.sessions.lock().unwrap().active.clone();
    let prior_messages = state
        .sessions
        .lock()
        .unwrap()
        .get(&session_id)
        .map(|session| session.messages.len())
        .unwrap_or(0);
    crate::biz::agent::send_minecraft_chat(app.clone(), state.inner(), chat).await?;

    let reply = state
        .sessions
        .lock()
        .unwrap()
        .get(&session_id)
        .and_then(|session| {
            session
                .messages
                .iter()
                .skip(prior_messages)
                .rev()
                .find(|message| message.role == "assistant")
        })
        .and_then(|message| message.content.clone())
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| "Minecraft chat turn completed without an assistant reply.".to_string())?;

    if reply.trim().eq_ignore_ascii_case(NO_MINECRAFT_REPLY) {
        return Ok(());
    }

    for part in split_minecraft_chat(&reply, 220) {
        mcp::call_original_tool(
            state.inner(),
            server_name,
            "minecraft_say",
            json!({ "message": part }),
        )
        .await?;
    }
    Ok(())
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
