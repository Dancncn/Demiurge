use serde::Deserialize;
use serde_json::Value;

use crate::agent::subagent::{SubagentContextMode, SubagentRequest};

#[derive(Deserialize)]
struct Args {
    prompt: String,
    label: Option<String>,
    agent_type: Option<String>,
    context_mode: Option<String>,
}

pub async fn run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let args: Args = serde_json::from_value(args).map_err(|e| format!("参数错误：{e}"))?;
    crate::agent::subagent::run(
        state,
        SubagentRequest {
            prompt: args.prompt,
            label: args.label,
            agent_type: args.agent_type,
            context_mode: SubagentContextMode::parse(args.context_mode.as_deref()),
        },
    )
    .await
}
