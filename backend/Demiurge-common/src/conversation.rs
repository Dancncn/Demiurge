//! 组件 2：会话状态。OpenAI 兼容的消息结构，每轮原样发给 LLM。
use serde::{Deserialize, Serialize};

fn default_kind() -> String {
    "function".to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FunctionCall {
    pub name: String,
    /// 模型生成的参数，是一段 JSON 字符串（OpenAI 规范如此）
    pub arguments: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type", default = "default_kind")]
    pub kind: String,
    pub function: FunctionCall,
}

/// 一条消息。role ∈ {system, user, assistant, tool}。
/// 用 Option + skip_serializing_if 保证发给 API 时不出现多余的 null 字段。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolExecutionStatus {
    Ok,
    Denied,
    Failed,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ToolExecutionRecord {
    pub status: ToolExecutionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affected_paths: Vec<String>,
}

/// A user-provided image kept with the conversation and translated into the
/// provider-specific multimodal content format at the LLM boundary.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ImageAttachment {
    pub mime_type: String,
    pub data: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Local conversation provenance. It is persisted with history but translated
/// into a text marker only at the model boundary, so provider payload schemas
/// never receive Demiurge-specific fields.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ConversationContext {
    pub scene: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_primary_user: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub addressed_to_ai: Option<bool>,
}

impl ConversationContext {
    pub fn model_marker(&self) -> String {
        let mut fields = vec![format!("scene={}", self.scene)];
        if let Some(channel) = self.channel.as_deref() {
            fields.push(format!("channel={channel}"));
        }
        if let Some(speaker) = self.speaker.as_deref() {
            fields.push(format!("speaker={speaker}"));
        }
        if let Some(value) = self.is_primary_user {
            fields.push(format!("primary_user={value}"));
        }
        if let Some(value) = self.addressed_to_ai {
            fields.push(format!("addressed_to_ai={value}"));
        }
        format!("[Conversation context: {}]", fields.join("; "))
    }

    pub fn annotate(&self, text: &str) -> String {
        format!("{}\n{text}", self.model_marker())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Message {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ImageAttachment>,
    /// Local provenance metadata. Provider adapters use `model_content` to
    /// expose it as an explicit context marker without leaking custom fields.
    #[serde(default, skip_serializing)]
    pub context: Option<ConversationContext>,
    /// Local execution metadata. Normal message serialization intentionally
    /// omits it so provider payloads remain protocol-compatible. Session
    /// persistence and history views add it explicitly through `history_value`.
    #[serde(default, skip_serializing)]
    pub tool_execution: Option<ToolExecutionRecord>,
}

#[derive(Serialize, Clone, Debug)]
pub struct HistoryMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ImageAttachment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<ConversationContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_execution: Option<ToolExecutionRecord>,
}

impl From<&Message> for HistoryMessage {
    fn from(message: &Message) -> Self {
        HistoryMessage {
            role: message.role.clone(),
            content: message.content.clone(),
            tool_calls: message.tool_calls.clone(),
            tool_call_id: message.tool_call_id.clone(),
            name: message.name.clone(),
            images: message.images.clone(),
            context: message.context.clone(),
            tool_execution: message.tool_execution.clone(),
        }
    }
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Message {
            role: "user".into(),
            content: Some(text.into()),
            ..Default::default()
        }
    }
    pub fn user_with_images(text: impl Into<String>, images: Vec<ImageAttachment>) -> Self {
        Message {
            role: "user".into(),
            content: Some(text.into()),
            images,
            ..Default::default()
        }
    }
    pub fn user_with_context(
        text: impl Into<String>,
        images: Vec<ImageAttachment>,
        context: ConversationContext,
    ) -> Self {
        Message {
            role: "user".into(),
            content: Some(text.into()),
            images,
            context: Some(context),
            ..Default::default()
        }
    }
    pub fn system(text: impl Into<String>) -> Self {
        Message {
            role: "system".into(),
            content: Some(text.into()),
            ..Default::default()
        }
    }
    pub fn assistant_text(text: impl Into<String>) -> Self {
        Message {
            role: "assistant".into(),
            content: Some(text.into()),
            ..Default::default()
        }
    }
    pub fn assistant_tools(content: Option<String>, calls: Vec<ToolCall>) -> Self {
        Message {
            role: "assistant".into(),
            content,
            tool_calls: Some(calls),
            ..Default::default()
        }
    }
    pub fn tool_result(
        call_id: impl Into<String>,
        name: impl Into<String>,
        result: impl Into<String>,
    ) -> Self {
        Message {
            role: "tool".into(),
            content: Some(result.into()),
            tool_call_id: Some(call_id.into()),
            name: Some(name.into()),
            ..Default::default()
        }
    }

    pub fn tool_result_with_execution(
        call_id: impl Into<String>,
        name: impl Into<String>,
        result: impl Into<String>,
        execution: ToolExecutionRecord,
    ) -> Self {
        let mut message = Self::tool_result(call_id, name, result);
        message.tool_execution = Some(execution);
        message
    }

    pub fn history_value(&self) -> serde_json::Value {
        let mut value = serde_json::to_value(self).unwrap_or(serde_json::Value::Null);
        if let (Some(object), Some(context)) = (value.as_object_mut(), &self.context) {
            if let Ok(context) = serde_json::to_value(context) {
                object.insert("context".to_string(), context);
            }
        }
        if let (Some(object), Some(execution)) = (value.as_object_mut(), &self.tool_execution) {
            if let Ok(execution) = serde_json::to_value(execution) {
                object.insert("tool_execution".to_string(), execution);
            }
        }
        value
    }

    pub fn model_content(&self) -> Option<String> {
        let content = self.content.as_deref()?;
        Some(match &self.context {
            Some(context) if self.role == "user" => context.annotate(content),
            _ => content.to_string(),
        })
    }
}

/// 整段对话（不含 system —— system 每轮由 persona 动态拼装，不持久化）
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Conversation {
    pub messages: Vec<Message>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_execution_metadata_is_local_but_available_to_history_views() {
        let message = Message::tool_result_with_execution(
            "call-1",
            "write_file",
            "done",
            ToolExecutionRecord {
                status: ToolExecutionStatus::Ok,
                error: None,
                duration_ms: 42,
                affected_paths: vec!["notes.md".to_string()],
            },
        );

        let provider_value = serde_json::to_value(&message).unwrap();
        assert!(provider_value.get("tool_execution").is_none());

        let history_value = message.history_value();
        assert_eq!(history_value["tool_execution"]["status"], "ok");
        assert_eq!(history_value["tool_execution"]["duration_ms"], 42);
        assert_eq!(
            history_value["tool_execution"]["affected_paths"][0],
            "notes.md"
        );
    }

    #[test]
    fn legacy_tool_messages_deserialize_without_execution_metadata() {
        let message: Message = serde_json::from_str(
            r#"{"role":"tool","content":"old","tool_call_id":"call-1","name":"grep"}"#,
        )
        .unwrap();
        assert!(message.tool_execution.is_none());
        assert!(message.images.is_empty());
        assert!(message.context.is_none());
    }

    #[test]
    fn image_messages_round_trip_in_session_history() {
        let message = Message::user_with_images(
            "describe this",
            vec![ImageAttachment {
                mime_type: "image/png".to_string(),
                data: "YWJj".to_string(),
                name: Some("sample.png".to_string()),
            }],
        );
        let value = serde_json::to_value(&message).unwrap();
        assert_eq!(value["images"][0]["mime_type"], "image/png");
        let restored: Message = serde_json::from_value(value).unwrap();
        assert_eq!(restored, message);
    }

    #[test]
    fn conversation_context_is_persisted_but_not_sent_as_a_provider_field() {
        let message = Message::user_with_context(
            "你好",
            Vec::new(),
            ConversationContext {
                scene: "minecraft".to_string(),
                channel: Some("public".to_string()),
                speaker: Some("Alex".to_string()),
                is_primary_user: Some(false),
                addressed_to_ai: Some(true),
            },
        );
        let provider_value = serde_json::to_value(&message).unwrap();
        assert!(provider_value.get("context").is_none());
        assert_eq!(message.history_value()["context"]["speaker"], "Alex");
        assert!(message.model_content().unwrap().contains("speaker=Alex"));
    }
}
