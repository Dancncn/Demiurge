use serde::Deserialize;
use serde_json::Value;

use crate::agent::subagent::{SubagentContextMode, SubagentRequest};

#[derive(Deserialize)]
struct Args {
    prompt: String,
    label: Option<String>,
    agent_type: Option<String>,
    agent_name: Option<String>,
    context_mode: Option<String>,
    max_total_tokens: Option<usize>,
}

pub async fn run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let args: Args = serde_json::from_value(args).map_err(|e| format!("参数错误：{e}"))?;
    crate::agent::subagent::run(
        state,
        SubagentRequest {
            prompt: args.prompt,
            label: args.label,
            agent_type: args.agent_type,
            agent_name: args.agent_name,
            context_mode: SubagentContextMode::parse(args.context_mode.as_deref()),
            max_total_tokens: args.max_total_tokens,
        },
    )
    .await
}
