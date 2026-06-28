use serde_json::Value;
use std::fs;

use crate::store;

pub fn run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let content = args
        .get("content")
        .and_then(Value::as_str)
        .ok_or("content 必须是字符串")?
        .trim();
    if content.is_empty() {
        return Err("计划内容不能为空".to_string());
    }

    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let plans_dir = sandbox.join(".demiurge").join("plans");
    fs::create_dir_all(&plans_dir).map_err(|e| format!("创建计划目录失败：{e}"))?;

    let active = state.sessions.lock().unwrap().active.clone();
    let safe_session: String = active
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let file_name = format!("plan-{}-{}.md", safe_session, store::now_millis());
    let path = plans_dir.join(file_name);
    fs::write(&path, content).map_err(|e| format!("写入计划文件失败：{e}"))?;

    let rel = path
        .strip_prefix(&sandbox)
        .unwrap_or(&path)
        .to_string_lossy()
        .replace('\\', "/");
    {
        let mut plan = state.plan_state.lock().unwrap();
        plan.active = true;
        plan.approved = false;
        plan.path = Some(rel.clone());
        plan.content = Some(content.to_string());
        plan.created_at = Some(store::now_millis());
        plan.approved_at = None;
    }

    Ok(format!("计划已写入：{rel}"))
}
