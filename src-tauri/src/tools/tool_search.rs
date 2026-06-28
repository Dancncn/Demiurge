use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Args {
    query: String,
    limit: Option<usize>,
}

pub fn run(args: Value) -> Result<String, String> {
    let args: Args = serde_json::from_value(args).map_err(|e| format!("参数错误：{e}"))?;
    let query = args.query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return Err("query 不能为空".to_string());
    }
    let limit = args.limit.unwrap_or(8).clamp(1, 20);
    let mut scored = super::deferred_definitions()
        .into_iter()
        .filter_map(|tool| {
            let haystack = format!("{} {} {}", tool.name, tool.description, tool.parameters)
                .to_ascii_lowercase();
            let mut score = 0usize;
            for term in query.split_whitespace() {
                if tool.name.to_ascii_lowercase().contains(term) {
                    score += 5;
                }
                if haystack.contains(term) {
                    score += 1;
                }
            }
            (score > 0).then_some((score, tool))
        })
        .collect::<Vec<_>>();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(b.1.name)));

    if scored.is_empty() {
        return Ok(format!(
            "没有找到匹配 `{}` 的 deferred tools。",
            args.query.trim()
        ));
    }

    let mut out = String::from("Deferred tools discovered:\n");
    for (_, tool) in scored.into_iter().take(limit) {
        out.push_str(&format!(
            "- `{}`: {}\n  params: {}\n",
            tool.name, tool.description, tool.parameters
        ));
    }
    out.push_str("\nUse execute_tool with tool_name and args to invoke one of these tools.");
    Ok(out)
}
