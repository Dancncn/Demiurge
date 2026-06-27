//! read_file：读取沙盒内文本文件。
use serde_json::Value;

const MAX_READ: u64 = 256 * 1024; // 256 KB 上限，避免把超大文件灌进上下文

pub fn run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let rel = args["path"].as_str().ok_or("缺少参数 path")?;
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let path = super::resolve_in_sandbox(&sandbox, rel)?;

    let meta = std::fs::metadata(&path).map_err(|e| format!("无法访问文件：{e}"))?;
    if !meta.is_file() {
        return Err("目标不是文件".to_string());
    }
    if meta.len() > MAX_READ {
        return Err(format!("文件过大（{} 字节），超过 {} 字节上限", meta.len(), MAX_READ));
    }

    let content = std::fs::read_to_string(&path).map_err(|e| format!("读取失败（可能不是 UTF-8 文本）：{e}"))?;
    Ok(content)
}
