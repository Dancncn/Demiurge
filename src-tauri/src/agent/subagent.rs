//! 子 Agent：给主 Agent 提供只读、多视角的探索/审查 worker。
use std::sync::atomic::Ordering;

use serde_json::{json, Value};

use super::conversation::Message;
use super::{context, prompt};
use crate::{llm, pack, store, tools};

const MAX_SUBAGENT_STEPS: usize = 6;
const MAX_PARENT_CONTEXT_CHARS: usize = 10_000;

const READ_ONLY_TOOLS: &[&str] = &[
    "read_file",
    "glob",
    "grep",
    "git_status",
    "system_info",
    "web_search",
];

#[derive(Clone, Debug)]
pub struct SubagentRequest {
    pub prompt: String,
    pub label: Option<String>,
    pub agent_type: Option<String>,
    pub context_mode: SubagentContextMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubagentContextMode {
    Brief,
    Recent,
}

impl SubagentContextMode {
    pub fn parse(value: Option<&str>) -> Self {
        match value.unwrap_or("").trim().to_ascii_lowercase().as_str() {
            "recent" | "fork" | "full" => SubagentContextMode::Recent,
            _ => SubagentContextMode::Brief,
        }
    }
}

pub async fn run(state: &crate::AppState, req: SubagentRequest) -> Result<String, String> {
    if req.prompt.trim().is_empty() {
        return Err("子 Agent prompt 不能为空".to_string());
    }

    let settings = state.settings.lock().unwrap().clone();
    let sid = state.sessions.lock().unwrap().active.clone();
    let packs_dir = state.packs_dir.lock().unwrap().clone();
    let persona_text = match pack::load_pack(&packs_dir, &settings.current_pack) {
        Ok(p) => p.persona_text,
        Err(_) => String::new(),
    };
    let session = {
        let storeg = state.sessions.lock().unwrap();
        storeg.get(&sid).cloned()
    };
    let session_summary = session.as_ref().and_then(|s| s.summary.as_deref());
    let mut system = prompt::build(state, &settings, &persona_text, session_summary);
    system.push_str("\n\n---\n子 Agent 运行约束：\n");
    system.push_str(
        "你是 Demiurge 的只读子 Agent。你帮助主 Agent 独立探索、审查、验证或反驳一个明确子任务。\n\
         你可以使用只读工具收集证据，但不能修改文件、运行 shell、截图或再次派生子 Agent。\n\
         你的最终输出会返回给主 Agent，而不是直接给用户；请输出结构清晰、可引用的发现。\n",
    );

    let parent_context = parent_context_block(session.as_ref(), req.context_mode);
    let label = req.label.as_deref().unwrap_or("subagent");
    let agent_type = req.agent_type.as_deref().unwrap_or("general");
    let user = format!(
        "# 子 Agent 任务\n\
         label: {label}\n\
         agent_type: {agent_type}\n\n\
         ## 父会话上下文\n\
         {parent_context}\n\n\
         ## 指令\n\
         {}\n\n\
         ## 输出要求\n\
         - 先给出一句结论。\n\
         - 列出关键发现、证据路径或行号。\n\
         - 标注不确定点和建议主 Agent 下一步做什么。\n\
         - 不要声称已经修改文件。",
        req.prompt.trim()
    );

    let profile = llm::ProviderProfile::for_kind(settings.provider);
    let tool_schema = tools::schemas_json_for_names(profile.tool_schema_dialect, READ_ONLY_TOOLS);
    let mut msgs = vec![Message::system(system), Message::user(user)];

    for _ in 0..MAX_SUBAGENT_STEPS {
        if state.cancel.load(Ordering::Relaxed) {
            return Ok("[子 Agent 已被用户中断]".to_string());
        }

        let turn = llm::stream_completion(
            &state.http,
            &settings,
            &msgs,
            &tool_schema,
            |_| {},
            &state.cancel,
        )
        .await?;

        if turn.finish_reason == "interrupted" {
            return Ok(if turn.content.trim().is_empty() {
                "[子 Agent 已被用户中断]".to_string()
            } else {
                turn.content
            });
        }

        if turn.tool_calls.is_empty() {
            return Ok(turn.content);
        }

        let content_opt = if turn.content.is_empty() {
            None
        } else {
            Some(turn.content.clone())
        };
        msgs.push(Message::assistant_tools(
            content_opt,
            turn.tool_calls.clone(),
        ));

        for tc in turn.tool_calls {
            let name = tc.function.name;
            let args: Value =
                serde_json::from_str(&tc.function.arguments).unwrap_or_else(|_| json!({}));
            let result = if READ_ONLY_TOOLS.contains(&name.as_str()) {
                match tools::execute(state, &name, args).await {
                    Ok(s) => s,
                    Err(e) => format!("错误：{e}"),
                }
            } else {
                format!("错误：子 Agent 不允许使用工具 {name}")
            };
            msgs.push(Message::tool_result(tc.id, name, result));
        }
    }

    Ok("子 Agent 达到内部工具轮次上限，未形成最终回答。".to_string())
}

fn parent_context_block(session: Option<&store::Session>, mode: SubagentContextMode) -> String {
    let Some(session) = session else {
        return "（无父会话上下文）".to_string();
    };

    let mut out = String::new();
    if let Some(summary) = &session.summary {
        if !summary.trim().is_empty() {
            out.push_str("### 会话摘要\n");
            out.push_str(summary.trim());
            out.push_str("\n\n");
        }
    }

    let keep = match mode {
        SubagentContextMode::Brief => 8,
        SubagentContextMode::Recent => 18,
    };
    out.push_str("### 最近消息摘录\n");
    let start = session.messages.len().saturating_sub(keep);
    for msg in session.messages.iter().skip(start) {
        out.push_str(&compact_message(msg));
        out.push('\n');
    }
    cap_chars(out, MAX_PARENT_CONTEXT_CHARS)
}

fn compact_message(msg: &Message) -> String {
    let mut body = msg.content.as_deref().unwrap_or("").trim().to_string();
    if let Some(calls) = &msg.tool_calls {
        let names = calls
            .iter()
            .map(|c| c.function.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        if !names.is_empty() {
            if !body.is_empty() {
                body.push(' ');
            }
            body.push_str(&format!("[tool_calls: {names}]"));
        }
    }
    let body = cap_chars(body, 900);
    format!("- {}: {}", msg.role, body)
}

fn cap_chars(s: String, max: usize) -> String {
    if s.chars().count() <= max {
        s
    } else {
        let head: String = s.chars().take(max).collect();
        format!("{head}\n…[已截断]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_context_mode_aliases() {
        assert_eq!(SubagentContextMode::parse(Some("recent")), SubagentContextMode::Recent);
        assert_eq!(SubagentContextMode::parse(Some("fork")), SubagentContextMode::Recent);
        assert_eq!(SubagentContextMode::parse(None), SubagentContextMode::Brief);
    }

    #[test]
    fn caps_parent_context() {
        let capped = cap_chars("x".repeat(12), 5);
        assert!(capped.contains("已截断"));
    }
}
