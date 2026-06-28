use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Args {
    tool_name: String,
    #[serde(default)]
    args: Value,
}

pub async fn run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let args: Args = serde_json::from_value(args).map_err(|e| format!("参数错误：{e}"))?;
    if !super::is_deferred_tool(&args.tool_name) {
        return Err(format!(
            "`{}` 不是 deferred tool。已加载的 core tool 请直接调用。",
            args.tool_name
        ));
    }

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
    let args: Args = serde_json::from_value(args).map_err(|e| format!("参数错误：{e}"))?;
    Ok(format!(
        "将通过 execute_tool 执行 deferred tool `{}`，参数：{}",
        args.tool_name,
        serde_json::to_string_pretty(&args.args).unwrap_or_else(|_| json!({}).to_string())
    ))
}
