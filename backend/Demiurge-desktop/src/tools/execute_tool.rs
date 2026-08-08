use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
pub(super) struct DeferredInvocation {
    pub(super) tool_name: String,
    #[serde(default)]
    pub(super) args: Value,
}

pub async fn run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let args = parse_invocation(&args)?;

    match args.tool_name.as_str() {
        "open_path" => super::open_path::run(args.args),
        "screen_list_windows" => super::screen::list_windows(state),
        "screen_capture_region" => super::screen::capture_region(state, args.args),
        "screen_capture_window" => super::screen::capture_window(state, args.args),
        "screen_ocr_region" => super::screen::ocr_region(state, args.args),
        "screen_ocr_window" => super::screen::ocr_window(state, args.args),
        other => Err(format!("execute_tool 尚未支持 deferred tool：{other}")),
    }
}

pub fn preview(args: Value) -> Result<String, String> {
    let args = parse_invocation(&args)?;
    Ok(format!(
        "将通过 execute_tool 执行 deferred tool `{}`，参数：{}",
        args.tool_name,
        serde_json::to_string_pretty(&args.args).unwrap_or_else(|_| json!({}).to_string())
    ))
}

pub(super) fn parse_invocation(args: &Value) -> Result<DeferredInvocation, String> {
    let invocation: DeferredInvocation =
        serde_json::from_value(args.clone()).map_err(|e| format!("参数错误：{e}"))?;
    if !super::is_deferred_tool(&invocation.tool_name) {
        return Err(format!(
            "`{}` 不是 deferred tool。已加载的 core tool 请直接调用。",
            invocation.tool_name
        ));
    }
    Ok(invocation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_registered_deferred_invocations() {
        let invocation = parse_invocation(&json!({
            "tool_name": "open_path",
            "args": {"target": "https://example.com"}
        }))
        .unwrap();
        assert_eq!(invocation.tool_name, "open_path");
        assert_eq!(invocation.args["target"], "https://example.com");

        assert!(parse_invocation(&json!({"tool_name": "shell", "args": {}})).is_err());
        assert!(parse_invocation(&json!({"tool_name": "unknown", "args": {}})).is_err());
    }
}
