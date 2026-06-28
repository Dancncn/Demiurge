//! web_fetch：单 URL 抓取/深抓取工具，输出与 web_search 一致的来源提醒。
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Value};

const DEFAULT_CONTEXT_MAX_CHARS: usize = 20_000;
const MAX_CONTEXT_MAX_CHARS: usize = 80_000;
const LIVECRAWL_STRATEGIES: &[&str] = &["fallback", "always", "never"];
const FETCH_SOURCES: &[&str] = &["direct", "exa"];

#[derive(Deserialize)]
struct Args {
    url: String,
    context_max_characters: Option<usize>,
    source: Option<String>,
    livecrawl: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FetchDocument {
    url: String,
    title: String,
    content: String,
    source: String,
    truncated: bool,
}

pub async fn run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let args: Args = serde_json::from_value(args).map_err(|e| format!("参数错误：{e}"))?;
    let url = normalize_url(&args.url)?;
    let context_max = args
        .context_max_characters
        .unwrap_or(DEFAULT_CONTEXT_MAX_CHARS)
        .clamp(1_000, MAX_CONTEXT_MAX_CHARS);
    let source = parse_choice(args.source.as_deref(), "source", FETCH_SOURCES, "direct")?;
    let livecrawl =
        parse_optional_choice(args.livecrawl.as_deref(), "livecrawl", LIVECRAWL_STRATEGIES)?;

    let doc = if source == "exa" || livecrawl.is_some() {
        fetch_exa(state, &url, livecrawl, context_max).await?
    } else {
        fetch_direct(state, &url, context_max).await?
    };
    Ok(format_document(&doc, context_max))
}

async fn fetch_direct(
    state: &crate::AppState,
    url: &str,
    context_max: usize,
) -> Result<FetchDocument, String> {
    let resp = state
        .http
        .get(url)
        .header("User-Agent", "Demiurge WebFetch")
        .header(
            "Accept",
            "text/html,application/xhtml+xml,application/json,text/plain,*/*;q=0.8",
        )
        .send()
        .await
        .map_err(|e| format!("WebFetch 请求失败：{e}"))?;
    let status = resp.status();
    let final_url = resp.url().to_string();
    if !status.is_success() {
        return Err(format!("WebFetch 返回 HTTP {status}"));
    }
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取 WebFetch 响应失败：{e}"))?;
    let (title, content) = if content_type.contains("html") || looks_like_html(&text) {
        (
            extract_title(&text).unwrap_or_else(|| title_from_url(&final_url)),
            html_to_text(&text),
        )
    } else if content_type.contains("json") {
        let pretty = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| serde_json::to_string_pretty(&v).ok())
            .unwrap_or(text);
        (title_from_url(&final_url), pretty)
    } else {
        (
            title_from_url(&final_url),
            clean_plain_text_preserve_lines(&text),
        )
    };

    let (content, truncated) = cap_chars_with_flag(content, context_max);
    Ok(FetchDocument {
        url: final_url,
        title,
        content,
        source: "direct".to_string(),
        truncated,
    })
}

async fn fetch_exa(
    state: &crate::AppState,
    url: &str,
    livecrawl: Option<&str>,
    context_max: usize,
) -> Result<FetchDocument, String> {
    let endpoint =
        env_first(&["EXA_MCP_URL"]).unwrap_or_else(|| "https://mcp.exa.ai/mcp".to_string());
    let body = json!({
        "jsonrpc": "2.0",
        "id": "demiurge-web-fetch",
        "method": "tools/call",
        "params": {
            "name": "get_contents",
            "arguments": {
                "ids": [url],
                "livecrawl": livecrawl.unwrap_or("fallback"),
                "contextMaxCharacters": context_max
            }
        }
    });
    let mut req = state
        .http
        .post(endpoint)
        .header("Accept", "application/json, text/event-stream")
        .json(&body);
    if let Some(key) = exa_api_key(state) {
        req = req.bearer_auth(key);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("Exa WebFetch 请求失败：{e}"))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取 Exa WebFetch 结果失败：{e}"))?;
    if !status.is_success() {
        return Err(format!(
            "Exa WebFetch 返回 HTTP {status}: {}",
            cap_chars(text, 500)
        ));
    }
    let (title, content) = extract_exa_document(&text, url);
    let (content, truncated) = cap_chars_with_flag(content, context_max);
    Ok(FetchDocument {
        url: url.to_string(),
        title,
        content,
        source: "exa-livecrawl".to_string(),
        truncated,
    })
}

fn extract_exa_document(raw: &str, fallback_url: &str) -> (String, String) {
    let payloads = parse_json_payloads(raw);
    let mut title = None;
    let mut content = Vec::new();
    for payload in payloads {
        collect_document_fields(&payload, &mut title, &mut content, 0);
    }
    if content.is_empty() {
        content.push(clean_plain_text_preserve_lines(raw));
    }
    (
        title.unwrap_or_else(|| title_from_url(fallback_url)),
        content.join("\n\n"),
    )
}

fn collect_document_fields(
    v: &Value,
    title: &mut Option<String>,
    content: &mut Vec<String>,
    depth: usize,
) {
    if depth > 8 {
        return;
    }
    match v {
        Value::Array(items) => {
            for item in items {
                collect_document_fields(item, title, content, depth + 1);
            }
        }
        Value::Object(map) => {
            if title.is_none() {
                for key in ["title", "name"] {
                    if let Some(value) = map
                        .get(key)
                        .and_then(Value::as_str)
                        .filter(|s| !s.trim().is_empty())
                    {
                        *title = Some(clean_plain_text(value));
                        break;
                    }
                }
            }
            for key in ["markdown", "content", "text", "summary", "raw_content"] {
                if let Some(value) = map
                    .get(key)
                    .and_then(Value::as_str)
                    .filter(|s| !s.trim().is_empty())
                {
                    content.push(clean_plain_text_preserve_lines(value));
                }
            }
            for value in map.values() {
                collect_document_fields(value, title, content, depth + 1);
            }
        }
        Value::String(s) if s.len() > 80 => content.push(clean_plain_text_preserve_lines(s)),
        _ => {}
    }
}

fn format_document(doc: &FetchDocument, context_max: usize) -> String {
    let mut out = format!(
        "Web fetch result\n\nTitle: {}\nURL: {}\nSource adapter: {}\nTruncated: {}\n\nContent:\n{}\n\nSources:\n- [{}]({})\n\nREMINDER: You MUST include relevant sources above in your response using markdown hyperlinks.",
        doc.title,
        doc.url,
        doc.source,
        doc.truncated,
        doc.content,
        doc.title,
        doc.url,
    );
    out = cap_chars(out, context_max.saturating_add(600));
    out
}

fn normalize_url(url: &str) -> Result<String, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("url 不能为空".to_string());
    }
    let url = if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else if url.contains("://") {
        return Err("WebFetch 只支持 http/https URL；需要登录或本地文件的地址不会抓取".to_string());
    } else {
        format!("https://{url}")
    };
    let parsed = reqwest::Url::parse(&url).map_err(|e| format!("URL 无效：{e}"))?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed.to_string()),
        _ => Err("WebFetch 只支持 http/https URL；需要登录或本地文件的地址不会抓取".to_string()),
    }
}

fn parse_choice<'a>(
    value: Option<&str>,
    field: &str,
    allowed: &'a [&'a str],
    default: &'a str,
) -> Result<&'a str, String> {
    let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(default);
    };
    let value = value.to_ascii_lowercase();
    allowed
        .iter()
        .copied()
        .find(|candidate| *candidate == value)
        .ok_or_else(|| format!("{field} 只支持：{}", allowed.join(", ")))
}

fn parse_optional_choice<'a>(
    value: Option<&str>,
    field: &str,
    allowed: &'a [&'a str],
) -> Result<Option<&'a str>, String> {
    let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    parse_choice(Some(value), field, allowed, allowed[0]).map(Some)
}

fn parse_json_payloads(raw: &str) -> Vec<Value> {
    let trimmed = raw.trim();
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        return vec![v];
    }
    let mut values = Vec::new();
    for line in raw.lines() {
        let line = line.trim_start();
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(data) {
            values.push(v);
        }
    }
    values
}

fn looks_like_html(text: &str) -> bool {
    let lower = text
        .chars()
        .take(300)
        .collect::<String>()
        .to_ascii_lowercase();
    lower.contains("<html") || lower.contains("<!doctype html") || lower.contains("<body")
}

fn extract_title(html: &str) -> Option<String> {
    let re = Regex::new(r"(?is)<title[^>]*>(.*?)</title>").unwrap();
    re.captures(html)
        .and_then(|cap| cap.get(1))
        .map(|m| clean_html_text(m.as_str()))
        .filter(|s| !s.is_empty())
}

fn html_to_text(html: &str) -> String {
    let script_re =
        Regex::new(r"(?is)<(script|style|noscript)[^>]*>.*?</(script|style|noscript)>").unwrap();
    let html = script_re.replace_all(html, " ");
    let block_re =
        Regex::new(r"(?i)</?(p|div|section|article|header|footer|main|br|li|h[1-6]|tr)[^>]*>")
            .unwrap();
    let html = block_re.replace_all(&html, "\n");
    clean_html_text(&html)
}

fn clean_html_text(html: &str) -> String {
    let tag_re = Regex::new(r"(?is)<[^>]+>").unwrap();
    let text = tag_re.replace_all(html, " ");
    decode_html_entities(&text)
        .lines()
        .map(clean_plain_text)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn clean_plain_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clean_plain_text_preserve_lines(text: &str) -> String {
    text.lines()
        .map(clean_plain_text)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn decode_html_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

fn title_from_url(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.host_str().map(|host| {
                let path = u.path().trim_matches('/');
                if path.is_empty() {
                    host.to_string()
                } else {
                    format!("{host}/{path}")
                }
            })
        })
        .unwrap_or_else(|| url.to_string())
}

fn settings_secret(
    state: &crate::AppState,
    key: fn(&crate::store::Settings) -> &String,
) -> Option<String> {
    let settings = state.settings.lock().unwrap();
    let value = key(&settings).trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn exa_api_key(state: &crate::AppState) -> Option<String> {
    settings_secret(state, |s| &s.exa_api_key).or_else(|| env_first(&["EXA_API_KEY"]))
}

fn env_first(keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| std::env::var(key).ok())
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
}

fn cap_chars_with_flag(s: String, max: usize) -> (String, bool) {
    if s.chars().count() <= max {
        (s, false)
    } else {
        let head: String = s.chars().take(max).collect();
        (
            format!("{head}\n…[web_fetch 输出已按 context_max_characters 截断]"),
            true,
        )
    }
}

fn cap_chars(s: String, max: usize) -> String {
    if s.chars().count() <= max {
        s
    } else {
        let head: String = s.chars().take(max).collect();
        format!("{head}\n…[web_fetch 输出已截断]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_urls_and_rejects_non_http() {
        assert_eq!(
            normalize_url("example.com/path").unwrap(),
            "https://example.com/path"
        );
        assert!(normalize_url("file:///tmp/a").is_err());
    }

    #[test]
    fn validates_fetch_source_and_livecrawl_strategy() {
        assert_eq!(
            parse_choice(Some(" Exa "), "source", FETCH_SOURCES, "direct").unwrap(),
            "exa"
        );
        assert_eq!(
            parse_optional_choice(Some("always"), "livecrawl", LIVECRAWL_STRATEGIES).unwrap(),
            Some("always")
        );
        assert!(parse_choice(Some("browser"), "source", FETCH_SOURCES, "direct").is_err());
        assert!(
            parse_optional_choice(Some("aggressive"), "livecrawl", LIVECRAWL_STRATEGIES).is_err()
        );
    }

    #[test]
    fn extracts_html_title_and_markdownish_text() {
        let html = r#"<html><head><title>A &amp; B</title><script>x()</script></head><body><h1>Hello</h1><p>World &lt;ok&gt;</p></body></html>"#;
        assert_eq!(extract_title(html).as_deref(), Some("A & B"));
        let text = html_to_text(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("World <ok>"));
        assert!(!text.contains("x()"));
    }

    #[test]
    fn parses_exa_sse_document() {
        let raw = r##"event: message
data: {"result":{"content":[{"title":"Fetched","url":"https://example.com","markdown":"# Body\nUseful text"}]}}

data: [DONE]
"##;
        let (title, content) = extract_exa_document(raw, "https://example.com");
        assert_eq!(title, "Fetched");
        assert!(content.contains("Useful text"));
    }

    #[test]
    fn formats_sources_reminder() {
        let doc = FetchDocument {
            url: "https://example.com".into(),
            title: "Example".into(),
            content: "Body".into(),
            source: "direct".into(),
            truncated: false,
        };
        let out = format_document(&doc, 1_000);
        assert!(out.contains("[Example](https://example.com)"));
        assert!(out.contains("Sources"));
    }
}
