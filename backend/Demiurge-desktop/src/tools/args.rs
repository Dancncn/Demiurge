//! 工具入参轻量校验辅助。
use serde_json::Value;

pub(crate) fn required_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("缺少参数 {key}"))
}

pub(crate) fn required_non_empty_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    let value = required_str(args, key)?.trim();
    if value.is_empty() {
        Err(format!("{key} 不能为空"))
    } else {
        Ok(value)
    }
}

pub(crate) fn optional_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

pub(crate) fn optional_bool(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(default)
}

pub(crate) fn optional_u64_clamped(
    args: &Value,
    key: &str,
    default: u64,
    min: u64,
    max: u64,
) -> u64 {
    args.get(key)
        .and_then(Value::as_u64)
        .unwrap_or(default)
        .clamp(min, max)
}
