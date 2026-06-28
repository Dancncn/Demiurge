//! http_get：轻量 HTTP GET typed tool。
use serde::Deserialize;
use serde_json::Value;

use super::web_common::{
    cap_chars_with_flag as cap_chars_with_flag_common, clean_plain_text_preserve_lines,
    html_to_text, looks_like_html, title_from_url,
};

const DEFAULT_CONTEXT_MAX_CHARS: usize = 12_000;
const MAX_CONTEXT_MAX_CHARS: usize = 50_000;

#[derive(Deserialize)]
struct Args {
    url: String,
    context_max_characters: Option<usize>,
    accept: Option<String>,
}

pub async fn run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let args: Args = serde_json::from_value(args).map_err(|e| format!("参数错误：{e}"))?;
    let url = normalize_url(&args.url)?;
    let context_max = args
        .context_max_characters
        .unwrap_or(DEFAULT_CONTEXT_MAX_CHARS)
        .clamp(1_000, MAX_CONTEXT_MAX_CHARS);
    let accept = args
        .accept
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("application/json,text/plain,text/html,*/*;q=0.8");

    let resp = state
        .http
        .get(&url)
        .header("User-Agent", "Demiurge HttpGet")
        .header("Accept", accept)
        .send()
        .await
        .map_err(|e| format!("HTTP GET 请求失败：{e}"))?;

    let status = resp.status();
    let final_url = resp.url().to_string();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取 HTTP GET 响应失败：{e}"))?;
    let body = normalize_body(&content_type, &text);
    let (body, truncated) = cap_chars_with_flag(body, context_max);

    Ok(format!(
        "HTTP GET result\n\nURL: {final_url}\nStatus: {status}\nContent-Type: {}\nTruncated: {truncated}\nTitle: {}\n\nBody:\n{}",
        if content_type.is_empty() { "unknown" } else { content_type.as_str() },
        title_from_url(&final_url),
        body
    ))
}

fn normalize_url(url: &str) -> Result<String, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("url 不能为空".to_string());
    }
    let url = if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else if url.contains("://") {
        return Err("http_get 只支持公开 http/https URL".to_string());
    } else {
        format!("https://{url}")
    };
    let parsed = reqwest::Url::parse(&url).map_err(|e| format!("URL 无效：{e}"))?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed.to_string()),
        _ => Err("http_get 只支持公开 http/https URL".to_string()),
    }
}

fn normalize_body(content_type: &str, text: &str) -> String {
    let lower = content_type.to_ascii_lowercase();
    if lower.contains("json") {
        serde_json::from_str::<Value>(text)
            .ok()
            .and_then(|v| serde_json::to_string_pretty(&v).ok())
            .unwrap_or_else(|| clean_plain_text_preserve_lines(text))
    } else if lower.contains("html") || looks_like_html(text) {
        html_to_text(text)
    } else {
        clean_plain_text_preserve_lines(text)
    }
}

fn cap_chars_with_flag(s: String, max: usize) -> (String, bool) {
    cap_chars_with_flag_common(s, max, "…[http_get 输出已按 context_max_characters 截断]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_urls_and_rejects_non_http() {
        assert_eq!(
            normalize_url("example.com/a").unwrap(),
            "https://example.com/a"
        );
        assert!(normalize_url("file:///tmp/a").is_err());
    }

    #[test]
    fn normalizes_json_html_and_text_bodies() {
        let json = normalize_body("application/json", r#"{"b":2,"a":1}"#);
        assert!(json.contains("\"a\": 1"));

        let html = normalize_body(
            "text/html",
            "<html><body><h1>Hello</h1><p>World</p></body></html>",
        );
        assert!(html.contains("Hello"));
        assert!(html.contains("World"));

        let text = normalize_body("text/plain", " hello   world\n\n ok ");
        assert_eq!(text, "hello world\nok");
    }

    #[test]
    fn caps_with_http_marker() {
        let (body, truncated) = cap_chars_with_flag("abcdef".to_string(), 3);
        assert!(truncated);
        assert!(body.contains("http_get"));
    }
}
