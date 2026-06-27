//! 组件 2：会话状态。OpenAI 兼容的消息结构，每轮原样发给 LLM。
use serde::{Deserialize, Serialize};

fn default_kind() -> String {
    "function".to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FunctionCall {
    pub name: String,
    /// 模型生成的参数，是一段 JSON 字符串（OpenAI 规范如此）
    pub arguments: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type", default = "default_kind")]
    pub kind: String,
    pub function: FunctionCall,
}

/// 一条消息。role ∈ {system, user, assistant, tool}。
/// 用 Option + skip_serializing_if 保证发给 API 时不出现多余的 null 字段。
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
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
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Message { role: "user".into(), content: Some(text.into()), ..Default::default() }
    }
    pub fn system(text: impl Into<String>) -> Self {
        Message { role: "system".into(), content: Some(text.into()), ..Default::default() }
    }
    pub fn assistant_text(text: impl Into<String>) -> Self {
        Message { role: "assistant".into(), content: Some(text.into()), ..Default::default() }
    }
    pub fn assistant_tools(content: Option<String>, calls: Vec<ToolCall>) -> Self {
        Message {
            role: "assistant".into(),
            content,
            tool_calls: Some(calls),
            ..Default::default()
        }
    }
    pub fn tool_result(call_id: impl Into<String>, name: impl Into<String>, result: impl Into<String>) -> Self {
        Message {
            role: "tool".into(),
            content: Some(result.into()),
            tool_call_id: Some(call_id.into()),
            name: Some(name.into()),
            ..Default::default()
        }
    }
}

/// 整段对话（不含 system —— system 每轮由 persona 动态拼装，不持久化）
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Conversation {
    pub messages: Vec<Message>,
}
