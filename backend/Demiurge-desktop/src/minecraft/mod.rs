//! Minecraft bridge event contracts.
//!
//! Parses notifications emitted by the independent Minecraft MCP process.
//! It does not own Agent turns, memory persistence, or MCP lifecycle.

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use serde::Deserialize;
use serde_json::Value;

use crate::mcp::McpIncomingNotification;

#[derive(Clone, Debug, Deserialize)]
pub struct MinecraftEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub timestamp: u64,
    pub priority: String,
    #[serde(default)]
    pub data: Value,
}

#[derive(Clone, Debug)]
pub struct MinecraftNotification {
    pub server_name: String,
    pub event: MinecraftEvent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MinecraftChatMessage {
    pub speaker: String,
    pub message: String,
    pub utterance: String,
    pub channel: String,
    pub ai_username: String,
    pub is_primary_user: bool,
    pub addressed_to_ai: bool,
    pub address_reason: String,
}

#[derive(Clone, Debug)]
pub struct MinecraftVisionRequest {
    pub prompt: String,
    pub mime_type: String,
    pub data: String,
}

pub fn parse_notification(notification: McpIncomingNotification) -> Option<MinecraftNotification> {
    if notification.method != "notifications/message"
        || notification.params.get("logger").and_then(Value::as_str) != Some("minecraft.event")
    {
        return None;
    }
    let data = notification.params.get("data")?;
    if data.get("source").and_then(Value::as_str) != Some("minecraft-ai-player") {
        return None;
    }
    let event = serde_json::from_value(data.get("event")?.clone()).ok()?;
    Some(MinecraftNotification {
        server_name: notification.server_name,
        event,
    })
}

pub fn chat_message(event: &MinecraftEvent) -> Result<Option<MinecraftChatMessage>, String> {
    if !matches!(event.event_type.as_str(), "chat" | "whisper") {
        return Ok(None);
    }
    let string = |key: &str| {
        event
            .data
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| format!("Minecraft chat event omitted `{key}`."))
    };
    let message = string("message")?;
    Ok(Some(MinecraftChatMessage {
        speaker: string("username")?,
        utterance: event
            .data
            .get("utterance")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&message)
            .to_string(),
        channel: event
            .data
            .get("channel")
            .and_then(Value::as_str)
            .unwrap_or(if event.event_type == "whisper" {
                "whisper"
            } else {
                "public"
            })
            .to_string(),
        ai_username: string("aiUsername")?,
        is_primary_user: event
            .data
            .get("isPrimaryUser")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        addressed_to_ai: event
            .data
            .get("addressedToAi")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        address_reason: event
            .data
            .get("addressReason")
            .and_then(Value::as_str)
            .unwrap_or("implicit")
            .to_string(),
        message,
    }))
}

pub fn experience_memory(event: &MinecraftEvent) -> Option<String> {
    let label = match event.event_type.as_str() {
        "death" => "角色在 Minecraft 中死亡",
        "dimension_changed" => "角色切换了 Minecraft 维度",
        "skill_completed" => "角色完成了一项 Minecraft 行动",
        "skill_failed" => "角色的一项 Minecraft 行动失败",
        "vision_anomaly" => "角色在 Minecraft 中发现了意外视觉事件",
        _ => return None,
    };
    Some(format!(
        "{label}（时间戳 {}，优先级 {}）：{}",
        event.timestamp,
        event.priority,
        compact_json(&event.data, 1_200)
    ))
}

pub fn vision_request(event: &MinecraftEvent) -> Result<Option<MinecraftVisionRequest>, String> {
    if event.event_type != "vision_vlm_request" {
        return Ok(None);
    }
    let mime_type = event
        .data
        .get("mimeType")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if !matches!(
        mime_type.as_str(),
        "image/jpeg" | "image/png" | "image/gif" | "image/webp"
    ) {
        return Err("Minecraft vision request used an unsupported MIME type.".to_string());
    }
    let data = event
        .data
        .get("imageBase64")
        .and_then(Value::as_str)
        .ok_or_else(|| "Minecraft vision request omitted image data.".to_string())?;
    let decoded = BASE64_STANDARD
        .decode(data.as_bytes())
        .map_err(|_| "Minecraft vision request contained invalid base64 data.".to_string())?;
    if decoded.is_empty() || decoded.len() > 10 * 1024 * 1024 {
        return Err("Minecraft vision request exceeded the 10 MB image limit.".to_string());
    }
    let prompt = event
        .data
        .get("prompt")
        .and_then(Value::as_str)
        .unwrap_or("识别 Minecraft 画面中的意外对象、危险或环境变化。")
        .trim()
        .chars()
        .take(1_000)
        .collect::<String>();
    Ok(Some(MinecraftVisionRequest {
        prompt,
        mime_type,
        data: data.to_string(),
    }))
}

fn compact_json(value: &Value, max_chars: usize) -> String {
    let raw = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    if raw.chars().count() <= max_chars {
        raw
    } else {
        format!("{}…", raw.chars().take(max_chars).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_only_tagged_minecraft_notifications() {
        let parsed = parse_notification(McpIncomingNotification {
            server_name: "minecraft_ai".to_string(),
            method: "notifications/message".to_string(),
            params: json!({
                "logger": "minecraft.event",
                "data": {
                    "source": "minecraft-ai-player",
                    "event": {
                        "type": "chat",
                        "timestamp": 10,
                        "priority": "normal",
                        "data": {
                            "username": "Alex",
                            "message": "DemiurgeBot，跟着我",
                            "utterance": "跟着我",
                            "channel": "public",
                            "aiUsername": "DemiurgeBot",
                            "isPrimaryUser": false,
                            "addressedToAi": true,
                            "addressReason": "username"
                        }
                    }
                }
            }),
            received_at: 11,
        })
        .unwrap();

        assert_eq!(parsed.server_name, "minecraft_ai");
        assert_eq!(parsed.event.event_type, "chat");
        let chat = chat_message(&parsed.event).unwrap().unwrap();
        assert_eq!(chat.speaker, "Alex");
        assert!(!chat.is_primary_user);
        assert!(chat.addressed_to_ai);
        assert_eq!(chat.utterance, "跟着我");
    }

    #[test]
    fn keeps_key_experiences_out_of_chat_memory_noise() {
        let event = MinecraftEvent {
            event_type: "skill_completed".to_string(),
            timestamp: 10,
            priority: "normal".to_string(),
            data: json!({"skill": "collect_blocks"}),
        };
        assert!(experience_memory(&event)
            .unwrap()
            .contains("collect_blocks"));

        let chat = MinecraftEvent {
            event_type: "chat".to_string(),
            ..event
        };
        assert!(experience_memory(&chat).is_none());
    }

    #[test]
    fn validates_external_vision_requests() {
        let event = MinecraftEvent {
            event_type: "vision_vlm_request".to_string(),
            timestamp: 10,
            priority: "high".to_string(),
            data: json!({
                "prompt": "look",
                "mimeType": "image/png",
                "imageBase64": "YWJj"
            }),
        };
        let request = vision_request(&event).unwrap().unwrap();
        assert_eq!(request.prompt, "look");
        assert_eq!(request.data, "YWJj");
    }
}
