//! 子 Agent：给主 Agent 提供只读、多视角的探索/审查 worker。
use std::sync::atomic::Ordering;

use serde_json::{json, Value};

use super::conversation::Message;
use super::custom;
use super::{budget, prompt};
use crate::{llm, pack, store, tools};

const MAX_SUBAGENT_STEPS: usize = 6;
const MAX_PARENT_CONTEXT_CHARS: usize = 10_000;
const FORK_PLACEHOLDER_RESULT: &str = "Fork started - processing in background";

const READ_ONLY_TOOLS: &[&str] = &[
    "read_file",
    "glob",
    "grep",
    "git_status",
    "system_info",
    "web_fetch",
    "web_search",
    "context_inspect",
];

#[derive(Clone, Debug)]
pub struct SubagentRequest {
    pub prompt: String,
    pub label: Option<String>,
    pub agent_type: Option<String>,
    pub agent_name: Option<String>,
    pub context_mode: SubagentContextMode,
    pub max_total_tokens: Option<usize>,
    pub output_format: SubagentOutputFormat,
    pub reviewer_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubagentOutputFormat {
    Plain,
    EvidencePacket,
}

impl SubagentOutputFormat {
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        match value.unwrap_or("").trim().to_ascii_lowercase().as_str() {
            "" | "plain" | "text" => Ok(SubagentOutputFormat::Plain),
            "evidence" | "evidence_packet" | "structured" | "json" => {
                Ok(SubagentOutputFormat::EvidencePacket)
            }
            other => Err(format!(
                "output_format 不支持 {other:?}；可选：plain, evidence_packet"
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubagentContextMode {
    Brief,
    Recent,
    Fork,
}

impl SubagentContextMode {
    pub fn parse(value: Option<&str>) -> Self {
        match value.unwrap_or("").trim().to_ascii_lowercase().as_str() {
            "fork" | "full" => SubagentContextMode::Fork,
            "recent" => SubagentContextMode::Recent,
            _ => SubagentContextMode::Brief,
        }
    }
}

pub async fn run(state: &crate::AppState, req: SubagentRequest) -> Result<String, String> {
    if req.prompt.trim().is_empty() {
        return Err("子 Agent prompt 不能为空".to_string());
    }

    let reviewer_count = req.reviewer_count.clamp(1, 5);
    if reviewer_count > 1 {
        return run_reviewer_panel(state, req, reviewer_count).await;
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
    let label = req.label.as_deref().unwrap_or("subagent");
    let agent_type = req.agent_type.as_deref().unwrap_or("general");
    let template = req
        .agent_name
        .as_deref()
        .and_then(|name| custom::load_agent(state, name).ok())
        .or_else(|| custom::load_agent(state, agent_type).ok());
    let mut token_budget = req
        .max_total_tokens
        .or_else(|| {
            template
                .as_ref()
                .and_then(|agent| agent.budget.as_ref()?.max_total_tokens)
        })
        .map(|total| budget::TokenBudgetState::new(Some(total)));
    let template_block = template
        .as_ref()
        .map(|agent| {
            format!(
                "## 自定义 Agent 模板\nname: {}\nkind: {:?}\nallowed_tools: {}\n\n### prompt\n{}\n\n### handoff_format\n{}\n",
                agent.name,
                agent.kind,
                if agent.allowed_tools.is_empty() {
                    "只读默认工具".to_string()
                } else {
                    agent.allowed_tools.join(", ")
                },
                agent.prompt.trim(),
                agent.handoff_format.trim()
            )
        })
        .unwrap_or_default();
    let output_contract = output_contract(req.output_format);
    let user = format!(
        "# 子 Agent 任务\n\
         label: {label}\n\
         agent_type: {agent_type}\n\n\
         ## 指令\n\
         {template_block}\n\
         {}\n\n\
         ## 子 Agent 运行约束\n\
         - 你是 Demiurge 的只读子 Agent，最终输出会返回给主 Agent。\n\
         - 你可以使用只读工具收集证据，但不能修改文件、运行 shell、截图或再次派生子 Agent。\n\
         - 如果工具 schema 中出现非只读工具，不要调用；即使调用也会被拒绝。\n\n\
         ## 输出要求\n\
         {output_contract}\n\
         - 不要声称已经修改文件。",
        req.prompt.trim()
    );

    let profile = llm::ProviderProfile::for_kind(settings.provider);
    let template_tool_names = template
        .as_ref()
        .map(|agent| {
            agent
                .allowed_tools
                .iter()
                .filter(|tool| READ_ONLY_TOOLS.contains(&tool.as_str()))
                .map(String::as_str)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let readonly_tool_names: &[&str] = if template_tool_names.is_empty() {
        READ_ONLY_TOOLS
    } else {
        &template_tool_names
    };
    let (tool_schema, mut msgs) = match req.context_mode {
        SubagentContextMode::Fork => {
            let system = prompt::build(state, &settings, &persona_text, session_summary);
            let mut msgs = vec![Message::system(system)];
            if let Some(session) = &session {
                let mut parent = session.messages.clone();
                repair_unpaired_tool_calls(&mut parent);
                msgs.extend(parent);
            }
            msgs.push(Message::user(user));
            (
                if profile.supports_tools {
                    tools::schemas_json_for_names(profile.tool_schema_dialect, readonly_tool_names)
                } else {
                    profile.empty_tool_schema()
                },
                msgs,
            )
        }
        SubagentContextMode::Brief | SubagentContextMode::Recent => {
            let mut system = prompt::build(state, &settings, &persona_text, session_summary);
            system.push_str("\n\n---\n子 Agent 运行约束：\n");
            system.push_str(
                "你是 Demiurge 的只读子 Agent。你帮助主 Agent 独立探索、审查、验证或反驳一个明确子任务。\n\
                 你可以使用只读工具收集证据，但不能修改文件、运行 shell、截图或再次派生子 Agent。\n\
                 你的最终输出会返回给主 Agent，而不是直接给用户；请输出结构清晰、可引用的发现。\n",
            );
            let parent_context = parent_context_block(session.as_ref(), req.context_mode);
            let user = user.replace(
                "## 指令",
                &format!("## 父会话上下文\n{parent_context}\n\n## 指令"),
            );
            (
                if profile.supports_tools {
                    tools::schemas_json_for_names(profile.tool_schema_dialect, readonly_tool_names)
                } else {
                    profile.empty_tool_schema()
                },
                vec![Message::system(system), Message::user(user)],
            )
        }
    };

    for _ in 0..MAX_SUBAGENT_STEPS {
        if state.cancel.load(Ordering::Relaxed) {
            return Ok("[子 Agent 已被用户中断]".to_string());
        }

        if token_budget
            .as_ref()
            .is_some_and(|budget| budget.is_exhausted())
        {
            return Ok("[子 Agent 已达到 token 硬预算，停止继续调用模型]".to_string());
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

        if let Some(budget_state) = &mut token_budget {
            let estimated = budget::estimate_messages_tokens(&msgs)
                .saturating_add(budget::estimate_text_tokens(&turn.content));
            budget_state.record_usage_or_estimate(turn.usage, estimated);
        }

        if turn.finish_reason == "interrupted" {
            return Ok(if turn.content.trim().is_empty() {
                "[子 Agent 已被用户中断]".to_string()
            } else {
                turn.content
            });
        }

        if turn.tool_calls.is_empty() {
            return Ok(with_budget_footer(turn.content, token_budget.as_ref()));
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
                match tools::execute_subagent_readonly(state, &name, args).await {
                    Ok(s) => s,
                    Err(e) => format!("错误：{e}"),
                }
            } else {
                format!("错误：子 Agent 不允许使用工具 {name}")
            };
            if let Some(budget_state) = &mut token_budget {
                budget_state.record_estimated(
                    budget::estimate_text_tokens(&tc.function.arguments)
                        .saturating_add(budget::estimate_text_tokens(&result)),
                );
            }
            msgs.push(Message::tool_result(tc.id, name, result));
        }
    }

    Ok("子 Agent 达到内部工具轮次上限，未形成最终回答。".to_string())
}

async fn run_reviewer_panel(
    state: &crate::AppState,
    req: SubagentRequest,
    reviewer_count: usize,
) -> Result<String, String> {
    let per_reviewer_budget = req
        .max_total_tokens
        .map(|total| (total / reviewer_count).max(1));
    let lenses = [
        "correctness",
        "evidence",
        "risk",
        "completeness",
        "simplicity",
    ];
    let mut outputs = Vec::new();

    for idx in 0..reviewer_count {
        if state.cancel.load(Ordering::Relaxed) {
            outputs.push(format!("## Reviewer {}\n[子 Agent 已被用户中断]", idx + 1));
            break;
        }
        let lens = lenses.get(idx).copied().unwrap_or("review");
        let mut child = req.clone();
        child.reviewer_count = 1;
        child.max_total_tokens = per_reviewer_budget;
        child.output_format = SubagentOutputFormat::EvidencePacket;
        child.label = Some(format!(
            "{}-{}",
            req.label.as_deref().unwrap_or("reviewer"),
            idx + 1
        ));
        child.prompt = format!(
            "{}\n\n你是 reviewer {} / {}，审查视角：{}。请独立判断，不要假设其他 reviewer 的结论正确。给出 0-100 confidence_score，并用证据支撑。",
            req.prompt.trim(),
            idx + 1,
            reviewer_count,
            lens
        );
        let output = Box::pin(run(state, child)).await?;
        outputs.push(format!("## Reviewer {} ({lens})\n{}", idx + 1, output));
    }

    Ok(synthesize_reviewer_outputs(&outputs))
}

fn output_contract(format: SubagentOutputFormat) -> &'static str {
    match format {
        SubagentOutputFormat::Plain => {
            "- 先给出一句结论。\n\
             - 列出关键发现、证据路径或行号。\n\
             - 标注不确定点和建议主 Agent 下一步做什么。"
        }
        SubagentOutputFormat::EvidencePacket => {
            "请输出一个结构化 evidence packet，使用以下 Markdown 小节且字段名不要改：\n\
             1. `verdict`: 一句话结论。\n\
             2. `confidence_score`: 0-100 整数。\n\
             3. `findings`: 列表，每项包含 `claim`、`evidence`（文件路径/行号/工具来源）、`reasoning`、`severity`。\n\
             4. `uncertainties`: 尚未验证或证据不足的点。\n\
             5. `next_actions`: 建议主 Agent 做的最小下一步。"
        }
    }
}

fn synthesize_reviewer_outputs(outputs: &[String]) -> String {
    let mut out = String::from("Multi-reviewer synthesis\n\n");
    out.push_str(&format!("Reviewers completed: {}\n\n", outputs.len()));
    out.push_str(
        "Synthesis guidance for main Agent: compare the reviewer evidence packets, prefer claims with concrete file/path evidence, and treat score disagreements as uncertainty rather than majority truth.\n\n",
    );
    for output in outputs {
        out.push_str(output.trim());
        out.push_str("\n\n");
    }
    out
}

fn with_budget_footer(content: String, budget: Option<&budget::TokenBudgetState>) -> String {
    let Some(budget) = budget else {
        return content;
    };
    let remaining = budget
        .remaining()
        .map(|n| n.to_string())
        .unwrap_or_else(|| "unbounded".to_string());
    format!(
        "{}\n\n---\nSubagent token budget: used={}, remaining={}",
        content.trim_end(),
        budget.used_total(),
        remaining
    )
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
        SubagentContextMode::Recent | SubagentContextMode::Fork => 18,
    };
    out.push_str("### 最近消息摘录\n");
    let start = session.messages.len().saturating_sub(keep);
    for msg in session.messages.iter().skip(start) {
        out.push_str(&compact_message(msg));
        out.push('\n');
    }
    cap_chars(out, MAX_PARENT_CONTEXT_CHARS)
}

fn repair_unpaired_tool_calls(messages: &mut Vec<Message>) {
    let existing_results = messages
        .iter()
        .filter(|m| m.role == "tool")
        .filter_map(|m| m.tool_call_id.as_deref())
        .map(str::to_string)
        .collect::<std::collections::HashSet<_>>();

    let mut repaired = Vec::with_capacity(messages.len());
    for msg in messages.drain(..) {
        let missing = msg
            .tool_calls
            .as_ref()
            .map(|calls| {
                calls
                    .iter()
                    .filter(|tc| !existing_results.contains(&tc.id))
                    .map(|tc| (tc.id.clone(), tc.function.name.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        repaired.push(msg);
        for (id, name) in missing {
            repaired.push(Message::tool_result(id, name, FORK_PLACEHOLDER_RESULT));
        }
    }
    *messages = repaired;
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
        assert_eq!(
            SubagentContextMode::parse(Some("recent")),
            SubagentContextMode::Recent
        );
        assert_eq!(
            SubagentContextMode::parse(Some("fork")),
            SubagentContextMode::Fork
        );
        assert_eq!(SubagentContextMode::parse(None), SubagentContextMode::Brief);
    }

    #[test]
    fn parses_output_format_aliases() {
        assert_eq!(
            SubagentOutputFormat::parse(Some("evidence_packet")).unwrap(),
            SubagentOutputFormat::EvidencePacket
        );
        assert_eq!(
            SubagentOutputFormat::parse(Some("plain")).unwrap(),
            SubagentOutputFormat::Plain
        );
        assert!(SubagentOutputFormat::parse(Some("xml")).is_err());
    }

    #[test]
    fn formats_budget_footer() {
        let mut budget = budget::TokenBudgetState::new(Some(100));
        budget.record_estimated(40);
        let out = with_budget_footer("done".to_string(), Some(&budget));
        assert!(out.contains("used=40"));
        assert!(out.contains("remaining=60"));
    }

    #[test]
    fn synthesizes_reviewer_outputs() {
        let out = synthesize_reviewer_outputs(&["a".into(), "b".into()]);
        assert!(out.contains("Reviewers completed: 2"));
        assert!(out.contains("a"));
        assert!(out.contains("b"));
    }

    #[test]
    fn caps_parent_context() {
        let capped = cap_chars("x".repeat(12), 5);
        assert!(capped.contains("已截断"));
    }

    #[test]
    fn repairs_unpaired_tool_calls_with_placeholder() {
        let mut messages = vec![Message::assistant_tools(
            None,
            vec![super::super::conversation::ToolCall {
                id: "tc1".to_string(),
                kind: "function".to_string(),
                function: super::super::conversation::FunctionCall {
                    name: "agent_spawn".to_string(),
                    arguments: "{}".to_string(),
                },
            }],
        )];
        repair_unpaired_tool_calls(&mut messages);
        assert_eq!(messages.len(), 2);
        assert_eq!(
            messages[1].content.as_deref(),
            Some(FORK_PLACEHOLDER_RESULT)
        );
    }
}
