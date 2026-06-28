use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct CollapseArgs {
    keep_recent: Option<usize>,
}

pub fn inspect(state: &crate::AppState) -> Result<String, String> {
    serde_json::to_string_pretty(&crate::agent::collapse::inspect(state)).map_err(|e| e.to_string())
}

pub async fn collapse(state: &crate::AppState, args: Value) -> Result<String, String> {
    let args: CollapseArgs =
        serde_json::from_value(args).unwrap_or(CollapseArgs { keep_recent: None });
    let keep_recent = args.keep_recent.unwrap_or(12).max(2);
    let result = crate::agent::collapse::compact_active_session(state, keep_recent).await?;
    Ok(json!({
        "removed_messages": result.removed_messages,
        "after": result.after,
    })
    .to_string())
}
