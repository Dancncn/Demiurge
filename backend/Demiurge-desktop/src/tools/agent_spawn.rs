use serde::Deserialize;
use serde_json::Value;

use crate::agent::subagent::{
    SubagentContextMode, SubagentOutputFormat, SubagentRequest, SubagentScope,
};

#[derive(Deserialize)]
struct Args {
    prompt: String,
    label: Option<String>,
    agent_type: Option<String>,
    agent_name: Option<String>,
    model: Option<String>,
    model_tier: Option<String>,
    scope: Option<String>,
    context_mode: Option<String>,
    max_total_tokens: Option<usize>,
    output_format: Option<String>,
    reviewer_count: Option<usize>,
}

pub async fn run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let args: Args = serde_json::from_value(args).map_err(|e| format!("参数错误：{e}"))?;
    let output_format = SubagentOutputFormat::parse(args.output_format.as_deref())?;
    let model_tier = match args.model_tier.as_deref() {
        None => None,
        Some(value) => Some(
            crate::store::ModelTier::parse(value)
                .ok_or_else(|| "model_tier 不支持；可选 haiku、sonnet、opus".to_string())?,
        ),
    };
    let reviewer_count = args.reviewer_count.unwrap_or(1).clamp(1, 5);
    crate::agent::subagent::run(
        state,
        SubagentRequest {
            prompt: args.prompt,
            label: args.label,
            agent_type: args.agent_type,
            agent_name: args.agent_name,
            model: args.model,
            model_tier,
            scope: SubagentScope::parse(args.scope.as_deref()),
            context_mode: SubagentContextMode::parse(args.context_mode.as_deref()),
            max_total_tokens: args.max_total_tokens,
            output_format,
            reviewer_count,
            cancel: None,
        },
    )
    .await
}
