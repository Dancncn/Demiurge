//! write_file：在沙盒内创建/覆盖文本文件（confirm 类）。
use serde_json::Value;

pub fn run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let rel = args["path"].as_str().ok_or("缺少参数 path")?;
    let content = args["content"].as_str().ok_or("缺少参数 content")?;
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let path = super::resolve_in_sandbox(&sandbox, rel)?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{e}"))?;
    }
    std::fs::write(&path, content).map_err(|e| format!("写入失败：{e}"))?;
    Ok(format!("已写入 {} 字节到沙盒文件：{}", content.len(), rel))
}
