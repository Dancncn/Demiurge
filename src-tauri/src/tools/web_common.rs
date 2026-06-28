use regex::Regex;
use serde_json::Value;

pub(super) const SOURCE_REMINDER_EN: &str =
    "REMINDER: You MUST include relevant sources above in your response using markdown hyperlinks.";
pub(super) const SOURCE_REMINDER_ZH: &str =
    "REMINDER: 如果你回答用户问题时使用了联网信息，必须在回答末尾用 markdown 链接列出 Sources。";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WebSource {
    pub title: String,
    pub url: String,
    pub snippet: Option<String>,
}

impl WebSource {
    pub(super) fn new(title: String, url: String, snippet: Option<String>) -> Self {
        Self {
            title,
            url,
            snippet,
        }
    }
}

pub(super) fn push_web_source(
    out: &mut Vec<WebSource>,
    title: Option<&str>,
    url: Option<&str>,
    snippet: Option<String>,
) {
    let Some(url) = url.map(clean_extracted_url).filter(|s| is_http_url(s)) else {
        return;
    };
    let title = title
        .map(clean_plain_text)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| title_from_url(&url));
    let snippet = snippet
        .map(|s| clean_plain_text(&s))
        .filter(|s| !s.is_empty());
    out.push(WebSource::new(title, url, snippet));
}

pub(super) fn dedupe_sources_by_url(sources: &mut Vec<WebSource>) {
    let mut seen = std::collections::HashSet::new();
    sources.retain(|source| seen.insert(source.url.clone()));
}

pub(super) fn append_source_lines(
    out: &mut String,
    sources: &[WebSource],
    numbered: bool,
    max_chars: Option<usize>,
) {
    for (idx, source) in sources.iter().enumerate() {
        if numbered {
            out.push_str(&format!("{}. [{}]({})", idx + 1, source.title, source.url));
        } else {
            out.push_str(&format!("- [{}]({})", source.title, source.url));
        }
        if let Some(snippet) = &source.snippet {
            out.push_str(": ");
            out.push_str(snippet);
        }
        out.push('\n');
        if max_chars.is_some_and(|max| out.chars().count() >= max) {
            break;
        }
    }
}

pub(crate) fn source_link_count(output: &str) -> usize {
    let mut in_source_block = false;
    let mut count = 0;
    for line in output.lines() {
        let trimmed = line.trim();
        if matches!(trimmed.to_ascii_lowercase().as_str(), "links:" | "sources:") {
            in_source_block = true;
            continue;
        }
        if !in_source_block {
            continue;
        }
        if trimmed.starts_with("REMINDER:") {
            break;
        }
        if trimmed.is_empty() {
            continue;
        }
        if is_markdown_http_link_line(trimmed) {
            count += 1;
        }
    }
    count
}

fn is_markdown_http_link_line(line: &str) -> bool {
    line.contains("](") && (line.contains("](http://") || line.contains("](https://"))
}

pub(super) fn parse_choice<'a>(
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

pub(super) fn parse_optional_choice<'a>(
    value: Option<&str>,
    field: &str,
    allowed: &'a [&'a str],
) -> Result<Option<&'a str>, String> {
    let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    parse_choice(Some(value), field, allowed, allowed[0]).map(Some)
}

pub(super) fn parse_json_payloads(raw: &str) -> Vec<Value> {
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

pub(super) fn collect_text_segments(v: &Value) -> Vec<String> {
    let mut out = Vec::new();
    collect_text_segments_inner(v, &mut out, 0);
    out
}

fn collect_text_segments_inner(v: &Value, out: &mut Vec<String>, depth: usize) {
    if depth > 8 {
        return;
    }
    match v {
        Value::Array(items) => {
            for item in items {
                collect_text_segments_inner(item, out, depth + 1);
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                let key = key.to_ascii_lowercase();
                if matches!(
                    key.as_str(),
                    "text" | "content" | "markdown" | "answer" | "result"
                ) {
                    if let Some(s) = value.as_str().filter(|s| s.contains("http")) {
                        out.push(s.to_string());
                    }
                }
                collect_text_segments_inner(value, out, depth + 1);
            }
        }
        Value::String(s) if s.contains("http") && s.len() > 20 => out.push(s.to_string()),
        _ => {}
    }
}

pub(super) fn looks_like_html(text: &str) -> bool {
    let lower = text
        .chars()
        .take(300)
        .collect::<String>()
        .to_ascii_lowercase();
    lower.contains("<html") || lower.contains("<!doctype html") || lower.contains("<body")
}

pub(super) fn extract_title(html: &str) -> Option<String> {
    let re = Regex::new(r"(?is)<title[^>]*>(.*?)</title>").unwrap();
    re.captures(html)
        .and_then(|cap| cap.get(1))
        .map(|m| clean_html_text(m.as_str()))
        .filter(|s| !s.is_empty())
}

pub(super) fn html_to_text(html: &str) -> String {
    let script_re =
        Regex::new(r"(?is)<(script|style|noscript)[^>]*>.*?</(script|style|noscript)>").unwrap();
    let html = script_re.replace_all(html, " ");
    let block_re =
        Regex::new(r"(?i)</?(p|div|section|article|header|footer|main|br|li|h[1-6]|tr)[^>]*>")
            .unwrap();
    let html = block_re.replace_all(&html, "\n");
    clean_html_text(&html)
}

pub(super) fn clean_html_text(html: &str) -> String {
    let tag_re = Regex::new(r"(?is)<[^>]+>").unwrap();
    let text = tag_re.replace_all(html, " ");
    decode_html_entities(&text)
        .lines()
        .map(clean_plain_text)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn clean_html_inline(html: &str) -> String {
    let tag_re = Regex::new(r"(?is)<[^>]+>").unwrap();
    let text = tag_re.replace_all(html, "");
    clean_plain_text(&decode_html_entities(&text))
}

pub(super) fn clean_plain_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn clean_plain_text_preserve_lines(text: &str) -> String {
    text.lines()
        .map(clean_plain_text)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn decode_html_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

pub(super) fn clean_extracted_url(url: &str) -> String {
    url.trim()
        .trim_matches(|c| matches!(c, ')' | ']' | '}' | '>' | '"' | '\'' | ',' | ';' | '.'))
        .to_string()
}

pub(super) fn is_http_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

pub(super) fn title_from_url(url: &str) -> String {
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

pub(super) fn normalize_domain_list(domains: &[String]) -> Vec<String> {
    domains
        .iter()
        .map(|d| {
            d.trim()
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .trim_start_matches("www.")
                .trim_matches('/')
                .to_ascii_lowercase()
        })
        .filter(|d| !d.is_empty())
        .collect()
}

pub(super) fn domain_matches(host: &str, domain: &str) -> bool {
    let domain = domain
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.")
        .trim_matches('/')
        .to_ascii_lowercase();
    let host = host.trim_start_matches("www.");
    host == domain || host.ends_with(&format!(".{domain}"))
}

pub(super) fn env_first(keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| std::env::var(key).ok())
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
}

pub(super) fn cap_chars_with_marker(s: String, max: usize, marker: &str) -> String {
    if s.chars().count() <= max {
        s
    } else {
        let head: String = s.chars().take(max).collect();
        format!("{head}\n{marker}")
    }
}

pub(super) fn cap_chars_with_flag(s: String, max: usize, marker: &str) -> (String, bool) {
    if s.chars().count() <= max {
        (s, false)
    } else {
        (cap_chars_with_marker(s, max, marker), true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_json_and_sse_payloads() {
        let plain = parse_json_payloads(r#"{"ok":true}"#);
        assert_eq!(plain.len(), 1);
        assert_eq!(plain[0]["ok"], true);

        let sse = parse_json_payloads(
            r#"event: message
data: {"result":{"text":"hello"}}
data: [DONE]
"#,
        );
        assert_eq!(sse.len(), 1);
        assert_eq!(sse[0]["result"]["text"], "hello");
    }

    #[test]
    fn cleans_html_and_preserves_document_lines() {
        let html = "<h1>A &amp; B</h1><p>Hello&nbsp;world</p>";
        assert_eq!(clean_html_inline(html), "A & BHello world");
        assert_eq!(clean_html_text(html), "A & B Hello world");
        assert_eq!(html_to_text(html), "A & B\nHello world");
    }

    #[test]
    fn matches_domains_after_normalization() {
        let domains = normalize_domain_list(&["https://www.example.com/docs".to_string()]);
        assert_eq!(domains, vec!["example.com/docs"]);
        assert!(domain_matches("docs.example.com", "example.com"));
        assert!(domain_matches("www.example.com", "https://example.com/"));
        assert!(!domain_matches("badexample.com", "example.com"));
    }

    #[test]
    fn formats_source_lines_consistently() {
        let sources = vec![WebSource::new(
            "Example".into(),
            "https://example.com".into(),
            Some("Snippet".into()),
        )];
        let mut numbered = String::new();
        append_source_lines(&mut numbered, &sources, true, None);
        assert_eq!(numbered, "1. [Example](https://example.com): Snippet\n");

        let mut bullets = String::new();
        append_source_lines(&mut bullets, &sources, false, None);
        assert_eq!(bullets, "- [Example](https://example.com): Snippet\n");
    }

    #[test]
    fn counts_only_links_from_source_blocks() {
        let output = "\
Web fetch result

Content:
[Inline](https://inline.example) should not count.

Sources:
- [A](https://a.example)
- [B](http://b.example): snippet

REMINDER: cite sources";
        assert_eq!(source_link_count(output), 2);

        let search_output = "\
Web search results

Links:
1. [A](https://a.example)
2. [B](https://b.example)
3. [C](https://c.example)
";
        assert_eq!(source_link_count(search_output), 3);
        assert_eq!(source_link_count("No search results found."), 0);
    }
}
