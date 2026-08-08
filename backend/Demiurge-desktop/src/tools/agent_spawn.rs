use serde::Deserialize;
use serde_json::Value;

use crate::agent::subagent::{SubagentContextMode, SubagentOutputFormat, SubagentRequest};

#[derive(Deserialize)]
struct Args {
    prompt: String,
    label: Option<String>,
    agent_type: Option<String>,
    agent_name: Option<String>,
    context_mode: Option<String>,
    max_total_tokens: Option<usize>,
    output_format: Option<String>,
    reviewer_count: Option<usize>,
}

pub async fn run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let args: Args = serde_json::from_value(args).map_err(|e| format!("参数错误：{e}"))?;
    let output_format = SubagentOutputFormat::parse(args.output_format.as_deref())?;
    let reviewer_count = args.reviewer_count.unwrap_or(1).clamp(1, 5);
    crate::agent::subagent::run(
        state,
        SubagentRequest {
            prompt: args.prompt,
            label: args.label,
            agent_type: args.agent_type,
            agent_name: args.agent_name,
            context_mode: SubagentContextMode::parse(args.context_mode.as_deref()),
            max_total_tokens: args.max_total_tokens,
            output_format,
            reviewer_count,
            cancel: None,
        },
    )
    .await
}
