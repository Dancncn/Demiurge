//! web_search：多 adapter 的联网搜索工具。
//! 默认使用 Bing HTML 结果页，DuckDuckGo Instant Answer 作为 fallback。
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

const DEFAULT_NUM_RESULTS: usize = 8;
const MAX_NUM_RESULTS: usize = 20;
const DEFAULT_CONTEXT_MAX_CHARS: usize = 10_000;
const MAX_CONTEXT_MAX_CHARS: usize = 50_000;

#[derive(Deserialize)]
struct Args {
    query: String,
    allowed_domains: Option<Vec<String>>,
    blocked_domains: Option<Vec<String>>,
    num_results: Option<usize>,
    context_max_characters: Option<usize>,
    source: Option<String>,
    #[allow(dead_code)]
    livecrawl: Option<String>,
    #[allow(dead_code)]
    search_type: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SearchResult {
    title: String,
    url: String,
    snippet: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Adapter {
    Auto,
    Bing,
    DuckDuckGo,
}

impl Adapter {
    fn parse(value: Option<&str>) -> Self {
        match value.unwrap_or("").trim().to_ascii_lowercase().as_str() {
            "bing" => Adapter::Bing,
            "duckduckgo" | "ddg" => Adapter::DuckDuckGo,
            _ => Adapter::Auto,
        }
    }
}

pub async fn run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let args: Args = serde_json::from_value(args).map_err(|e| format!("参数错误：{e}"))?;
    let query = args.query.trim();
    if query.len() < 2 {
        return Err("query 至少需要 2 个字符".to_string());
    }
    if args.allowed_domains.as_ref().is_some_and(|v| !v.is_empty())
        && args.blocked_domains.as_ref().is_some_and(|v| !v.is_empty())
    {
        return Err("不能同时指定 allowed_domains 和 blocked_domains".to_string());
    }

    let limit = args
        .num_results
        .unwrap_or(DEFAULT_NUM_RESULTS)
        .clamp(1, MAX_NUM_RESULTS);
    let context_max = args
        .context_max_characters
        .unwrap_or(DEFAULT_CONTEXT_MAX_CHARS)
        .clamp(1_000, MAX_CONTEXT_MAX_CHARS);
    let env_adapter = std::env::var("WEB_SEARCH_ADAPTER").ok();
    let adapter = Adapter::parse(args.source.as_deref().or(env_adapter.as_deref()));

    let mut results = match adapter {
        Adapter::Bing => search_bing(state, query).await?,
        Adapter::DuckDuckGo => search_duckduckgo(state, query).await?,
        Adapter::Auto => match search_bing(state, query).await {
            Ok(results) if !results.is_empty() => results,
            Ok(_) | Err(_) => search_duckduckgo(state, query).await?,
        },
    };

    results = filter_domains(
        results,
        args.allowed_domains.as_deref().unwrap_or(&[]),
        args.blocked_domains.as_deref().unwrap_or(&[]),
    );
    dedupe_by_url(&mut results);
    results.truncate(limit);

    Ok(format_results(query, &results, context_max))
}

async fn search_bing(state: &crate::AppState, query: &str) -> Result<Vec<SearchResult>, String> {
    let resp = state
        .http
        .get("https://www.bing.com/search")
        .query(&[("q", query), ("setmkt", "en-US")])
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0",
        )
        .header(
            "Accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        )
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .await
        .map_err(|e| format!("Bing 搜索请求失败：{e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Bing 搜索返回 HTTP {}", resp.status()));
    }
    let html = resp
        .text()
        .await
        .map_err(|e| format!("读取 Bing 搜索结果失败：{e}"))?;
    Ok(extract_bing_results(&html))
}

async fn search_duckduckgo(
    state: &crate::AppState,
    query: &str,
) -> Result<Vec<SearchResult>, String> {
    let resp = state
        .http
        .get("https://api.duckduckgo.com/")
        .query(&[
            ("q", query),
            ("format", "json"),
            ("no_html", "1"),
            ("no_redirect", "1"),
            ("skip_disambig", "1"),
        ])
        .send()
        .await
        .map_err(|e| format!("DuckDuckGo 搜索请求失败：{e}"))?;

    if !resp.status().is_success() {
        return Err(format!("DuckDuckGo 搜索返回 HTTP {}", resp.status()));
    }
    let v: Value = resp
        .json()
        .await
        .map_err(|e| format!("解析 DuckDuckGo 搜索结果失败：{e}"))?;
    Ok(extract_duckduckgo_results(&v))
}

fn extract_bing_results(html: &str) -> Vec<SearchResult> {
    let block_re = Regex::new(r#"(?is)<li\s+class="b_algo"[^>]*>(.*?)</li>"#).unwrap();
    let link_re = Regex::new(r#"(?is)<h2[^>]*>\s*<a[^>]+href="([^"]+)"[^>]*>(.*?)</a>"#).unwrap();
    let snippet_res = [
        Regex::new(r#"(?is)<p[^>]*class="b_lineclamp[^"]*"[^>]*>(.*?)</p>"#).unwrap(),
        Regex::new(r#"(?is)<div[^>]*class="b_caption[^"]*"[^>]*>.*?<p[^>]*>(.*?)</p>"#).unwrap(),
        Regex::new(r#"(?is)<div[^>]*class="b_caption[^"]*"[^>]*>(.*?)</div>"#).unwrap(),
    ];

    let mut results = Vec::new();
    for block in block_re.captures_iter(html) {
        let Some(block) = block.get(1).map(|m| m.as_str()) else {
            continue;
        };
        let Some(link) = link_re.captures(block) else {
            continue;
        };
        let Some(raw_url) = link.get(1).map(|m| decode_html_entities(m.as_str())) else {
            continue;
        };
        let Some(url) = resolve_bing_url(&raw_url) else {
            continue;
        };
        let title = link
            .get(2)
            .map(|m| clean_html_text(m.as_str()))
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let snippet = snippet_res
            .iter()
            .find_map(|re| re.captures(block))
            .and_then(|cap| cap.get(1))
            .map(|m| clean_html_text(m.as_str()))
            .filter(|s| !s.is_empty());

        results.push(SearchResult {
            title,
            url,
            snippet,
        });
    }
    results
}

fn extract_duckduckgo_results(v: &Value) -> Vec<SearchResult> {
    let mut results = Vec::new();
    if let (Some(title), Some(url)) = (v["Heading"].as_str(), v["AbstractURL"].as_str()) {
        if !title.is_empty() && !url.is_empty() {
            results.push(SearchResult {
                title: title.to_string(),
                url: url.to_string(),
                snippet: v["AbstractText"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
            });
        }
    }
    if let Some(items) = v["RelatedTopics"].as_array() {
        for it in items {
            collect_duckduckgo_topic(it, &mut results);
        }
    }
    results
}

fn collect_duckduckgo_topic(it: &Value, out: &mut Vec<SearchResult>) {
    if let (Some(text), Some(url)) = (it["Text"].as_str(), it["FirstURL"].as_str()) {
        if !text.is_empty() && !url.is_empty() {
            let (title, snippet) = split_duckduckgo_text(text);
            out.push(SearchResult {
                title,
                url: url.to_string(),
                snippet,
            });
            return;
        }
    }
    if let Some(sub) = it["Topics"].as_array() {
        for t in sub {
            collect_duckduckgo_topic(t, out);
        }
    }
}

fn split_duckduckgo_text(text: &str) -> (String, Option<String>) {
    if let Some((title, rest)) = text.split_once(" - ") {
        (title.trim().to_string(), Some(rest.trim().to_string()))
    } else {
        let title: String = text.chars().take(80).collect();
        (title, Some(text.to_string()))
    }
}

fn filter_domains(
    results: Vec<SearchResult>,
    allowed: &[String],
    blocked: &[String],
) -> Vec<SearchResult> {
    results
        .into_iter()
        .filter(|r| {
            let Ok(url) = reqwest::Url::parse(&r.url) else {
                return false;
            };
            let Some(host) = url.host_str().map(|h| h.to_ascii_lowercase()) else {
                return false;
            };
            if !allowed.is_empty() && !allowed.iter().any(|d| domain_matches(&host, d)) {
                return false;
            }
            if !blocked.is_empty() && blocked.iter().any(|d| domain_matches(&host, d)) {
                return false;
            }
            true
        })
        .collect()
}

fn domain_matches(host: &str, domain: &str) -> bool {
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

fn dedupe_by_url(results: &mut Vec<SearchResult>) {
    let mut seen = std::collections::HashSet::new();
    results.retain(|r| seen.insert(r.url.clone()));
}

fn format_results(query: &str, results: &[SearchResult], context_max: usize) -> String {
    if results.is_empty() {
        return format!(
            "Web search results for query: \"{query}\"\n\nNo search results found.\n\n\
             REMINDER: 如果你回答用户问题时使用了联网信息，必须在回答末尾用 markdown 链接列出 Sources。"
        );
    }

    let mut out = format!("Web search results for query: \"{query}\"\n\nLinks:\n");
    for (idx, result) in results.iter().enumerate() {
        out.push_str(&format!("{}. [{}]({})", idx + 1, result.title, result.url));
        if let Some(snippet) = &result.snippet {
            out.push_str(": ");
            out.push_str(snippet);
        }
        out.push('\n');
        if out.chars().count() >= context_max {
            break;
        }
    }
    out.push_str(
        "\nREMINDER: You MUST include relevant sources above in your response using markdown hyperlinks.",
    );
    cap_chars(out, context_max)
}

fn resolve_bing_url(raw_url: &str) -> Option<String> {
    if raw_url.starts_with('#') {
        return None;
    }
    if raw_url.starts_with("http") && !raw_url.contains("bing.com/ck/") {
        return Some(raw_url.to_string());
    }

    let parsed = if raw_url.starts_with('/') {
        reqwest::Url::parse(&format!("https://www.bing.com{raw_url}")).ok()?
    } else {
        reqwest::Url::parse(raw_url).ok()?
    };

    if !parsed.host_str().is_some_and(|h| h.contains("bing.com")) {
        return Some(parsed.to_string());
    }

    let encoded = parsed
        .query_pairs()
        .find(|(k, _)| k == "u")
        .map(|(_, v)| v.to_string())?;
    decode_bing_redirect(&encoded)
}

fn decode_bing_redirect(encoded: &str) -> Option<String> {
    if encoded.len() < 3 {
        return None;
    }
    let b64 = encoded.get(2..)?;
    let decoded = decode_base64_url(b64)?;
    if decoded.starts_with("http") {
        Some(decoded)
    } else {
        None
    }
}

fn decode_base64_url(input: &str) -> Option<String> {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bits = 0u32;
    let mut bit_count = 0u8;
    let mut out = Vec::new();

    for ch in input.bytes() {
        if ch == b'=' {
            break;
        }
        let normalized = match ch {
            b'-' => b'+',
            b'_' => b'/',
            other => other,
        };
        let value = TABLE.iter().position(|b| *b == normalized)? as u32;
        bits = (bits << 6) | value;
        bit_count += 6;
        if bit_count >= 8 {
            bit_count -= 8;
            out.push(((bits >> bit_count) & 0xff) as u8);
        }
    }
    String::from_utf8(out).ok()
}

fn clean_html_text(html: &str) -> String {
    let tag_re = Regex::new(r"(?is)<[^>]+>").unwrap();
    let text = tag_re.replace_all(html, "");
    decode_html_entities(&text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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

fn cap_chars(s: String, max: usize) -> String {
    if s.chars().count() <= max {
        s
    } else {
        let head: String = s.chars().take(max).collect();
        format!("{head}\n…[web_search 输出已按 context_max_characters 截断]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_bing_results_and_decodes_entities() {
        let html = r#"
        <ol id="b_results">
          <li class="b_algo">
            <h2><a href="https://example.com/a?x=1&amp;y=2">Example &amp; Result</a></h2>
            <div class="b_caption"><p>A useful &lt;snippet&gt;.</p></div>
          </li>
        </ol>
        "#;
        let results = extract_bing_results(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Example & Result");
        assert_eq!(results[0].url, "https://example.com/a?x=1&y=2");
        assert_eq!(results[0].snippet.as_deref(), Some("A useful <snippet>."));
    }

    #[test]
    fn filters_allowed_and_blocked_domains() {
        let results = vec![
            SearchResult {
                title: "A".into(),
                url: "https://docs.rs/foo".into(),
                snippet: None,
            },
            SearchResult {
                title: "B".into(),
                url: "https://example.com/foo".into(),
                snippet: None,
            },
        ];
        let filtered = filter_domains(results.clone(), &[String::from("docs.rs")], &[]);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].title, "A");

        let filtered = filter_domains(results, &[], &[String::from("example.com")]);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].title, "A");
    }

    #[test]
    fn formats_with_source_reminder_and_cap() {
        let result = SearchResult {
            title: "A".into(),
            url: "https://example.com".into(),
            snippet: Some("snippet".into()),
        };
        let out = format_results("query", &[result], 180);
        assert!(out.contains("[A](https://example.com)"));
        assert!(out.contains("Sources") || out.contains("sources"));
    }
}
