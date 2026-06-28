//! web_search：多 adapter 的联网搜索工具。
//! 默认使用 Bing HTML 结果页，DuckDuckGo Instant Answer 作为 fallback。
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Value};

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
    livecrawl: Option<String>,
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
    Tavily,
    Brave,
    Exa,
}

impl Adapter {
    fn parse(value: Option<&str>) -> Self {
        match value.unwrap_or("").trim().to_ascii_lowercase().as_str() {
            "bing" => Adapter::Bing,
            "duckduckgo" | "ddg" => Adapter::DuckDuckGo,
            "tavily" => Adapter::Tavily,
            "brave" => Adapter::Brave,
            "exa" => Adapter::Exa,
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
    let settings_provider = state
        .settings
        .lock()
        .unwrap()
        .web_search_provider
        .trim()
        .to_string();
    let adapter = Adapter::parse(
        args.source
            .as_deref()
            .or_else(|| non_empty(settings_provider.as_str()))
            .or(env_adapter.as_deref()),
    );
    let allowed = args.allowed_domains.as_deref().unwrap_or(&[]);
    let blocked = args.blocked_domains.as_deref().unwrap_or(&[]);

    let mut results = match adapter {
        Adapter::Bing => search_bing(state, query).await?,
        Adapter::DuckDuckGo => search_duckduckgo(state, query).await?,
        Adapter::Tavily => search_tavily(state, query, limit, allowed, blocked).await?,
        Adapter::Brave => search_brave(state, query).await?,
        Adapter::Exa => {
            search_exa(
                state,
                query,
                limit,
                args.livecrawl.as_deref(),
                args.search_type.as_deref(),
                context_max,
            )
            .await?
        }
        Adapter::Auto => match search_bing(state, query).await {
            Ok(results) if !results.is_empty() => results,
            Ok(_) | Err(_) => search_duckduckgo(state, query).await?,
        },
    };

    results = filter_domains(results, allowed, blocked);
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

async fn search_tavily(
    state: &crate::AppState,
    query: &str,
    limit: usize,
    allowed: &[String],
    blocked: &[String],
) -> Result<Vec<SearchResult>, String> {
    let endpoint = env_first(&["TAVILY_SEARCH_URL", "TAVILY_ENDPOINT_URL"])
        .unwrap_or_else(|| "https://tavily.claude-code-best.win/search".to_string());
    let mut body = json!({
        "query": query,
        "search_depth": "basic",
        "max_results": limit,
    });
    if !allowed.is_empty() {
        body["include_domains"] = json!(normalize_domain_list(allowed));
    }
    if !blocked.is_empty() {
        body["exclude_domains"] = json!(normalize_domain_list(blocked));
    }

    let mut req = state
        .http
        .post(endpoint)
        .header("User-Agent", "Demiurge WebSearch")
        .json(&body);
    if let Some(key) = tavily_api_key(state) {
        req = req.bearer_auth(key.clone()).header("x-api-key", key);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Tavily 搜索请求失败：{e}"))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取 Tavily 搜索结果失败：{e}"))?;
    if !status.is_success() {
        return Err(format!(
            "Tavily 搜索返回 HTTP {status}: {}",
            cap_chars(text, 500)
        ));
    }
    let v: Value =
        serde_json::from_str(&text).map_err(|e| format!("解析 Tavily 搜索结果失败：{e}"))?;
    Ok(extract_tavily_results(&v))
}

async fn search_brave(state: &crate::AppState, query: &str) -> Result<Vec<SearchResult>, String> {
    let key = brave_search_api_key(state).ok_or_else(|| {
        "Brave 搜索需要在设置中保存 Brave API Key，或设置 BRAVE_SEARCH_API_KEY / BRAVE_API_KEY"
            .to_string()
    })?;
    let resp = state
        .http
        .get("https://api.search.brave.com/res/v1/llm/context")
        .query(&[("q", query)])
        .header("Accept", "application/json")
        .header("X-Subscription-Token", key)
        .send()
        .await
        .map_err(|e| format!("Brave 搜索请求失败：{e}"))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取 Brave 搜索结果失败：{e}"))?;
    if !status.is_success() {
        return Err(format!(
            "Brave 搜索返回 HTTP {status}: {}",
            cap_chars(text, 500)
        ));
    }
    let v: Value =
        serde_json::from_str(&text).map_err(|e| format!("解析 Brave 搜索结果失败：{e}"))?;
    Ok(extract_brave_results(&v))
}

async fn search_exa(
    state: &crate::AppState,
    query: &str,
    limit: usize,
    livecrawl: Option<&str>,
    search_type: Option<&str>,
    context_max: usize,
) -> Result<Vec<SearchResult>, String> {
    let endpoint =
        env_first(&["EXA_MCP_URL"]).unwrap_or_else(|| "https://mcp.exa.ai/mcp".to_string());
    let body = json!({
        "jsonrpc": "2.0",
        "id": "demiurge-web-search",
        "method": "tools/call",
        "params": {
            "name": "web_search_exa",
            "arguments": {
                "query": query,
                "type": search_type.unwrap_or("auto"),
                "numResults": limit,
                "livecrawl": livecrawl.unwrap_or("fallback"),
                "contextMaxCharacters": context_max,
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
        .map_err(|e| format!("Exa 搜索请求失败：{e}"))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取 Exa 搜索结果失败：{e}"))?;
    if !status.is_success() {
        return Err(format!(
            "Exa 搜索返回 HTTP {status}: {}",
            cap_chars(text, 500)
        ));
    }
    Ok(extract_exa_results(&text)?)
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

fn extract_tavily_results(v: &Value) -> Vec<SearchResult> {
    let mut results = Vec::new();
    if let Some(items) = v["results"].as_array().or_else(|| v["data"].as_array()) {
        for it in items {
            push_result_from_value(it, &mut results);
        }
    }
    results
}

fn extract_brave_results(v: &Value) -> Vec<SearchResult> {
    let mut results = Vec::new();
    for path in [
        "/grounding/generic",
        "/grounding/map",
        "/grounding/poi",
        "/web/results",
        "/results",
    ] {
        if let Some(node) = v.pointer(path) {
            collect_result_values(node, &mut results, 0);
        }
    }
    if results.is_empty() {
        collect_result_values(v, &mut results, 0);
    }
    results
}

fn extract_exa_results(raw: &str) -> Result<Vec<SearchResult>, String> {
    let mut results = Vec::new();
    let payloads = parse_json_payloads(raw);
    if payloads.is_empty() {
        results.extend(extract_results_from_text(raw));
    } else {
        for payload in &payloads {
            collect_result_values(payload, &mut results, 0);
            for text in collect_text_segments(payload) {
                results.extend(extract_results_from_text(&text));
            }
        }
    }
    dedupe_by_url(&mut results);
    Ok(results)
}

fn split_duckduckgo_text(text: &str) -> (String, Option<String>) {
    if let Some((title, rest)) = text.split_once(" - ") {
        (title.trim().to_string(), Some(rest.trim().to_string()))
    } else {
        let title: String = text.chars().take(80).collect();
        (title, Some(text.to_string()))
    }
}

fn collect_result_values(v: &Value, out: &mut Vec<SearchResult>, depth: usize) {
    if depth > 8 {
        return;
    }
    match v {
        Value::Array(items) => {
            for item in items {
                collect_result_values(item, out, depth + 1);
            }
        }
        Value::Object(map) => {
            push_result_from_value(v, out);
            for value in map.values() {
                collect_result_values(value, out, depth + 1);
            }
        }
        _ => {}
    }
}

fn push_result_from_value(v: &Value, out: &mut Vec<SearchResult>) {
    let url = first_str(
        v,
        &[
            "url",
            "link",
            "href",
            "website",
            "sourceUrl",
            "source_url",
            "resolved_url",
        ],
    );
    let title = first_str(v, &["title", "name", "heading", "source"]);
    let snippet = first_text(
        v,
        &[
            "content",
            "snippet",
            "description",
            "summary",
            "text",
            "raw_content",
        ],
    );
    push_result(out, title, url, snippet);
}

fn push_result(
    out: &mut Vec<SearchResult>,
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
    out.push(SearchResult {
        title,
        url,
        snippet,
    });
}

fn first_str<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .filter_map(|key| v.get(*key).and_then(Value::as_str))
        .find(|s| !s.trim().is_empty())
}

fn first_text(v: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        let Some(value) = v.get(*key) else {
            continue;
        };
        if let Some(s) = value.as_str().filter(|s| !s.trim().is_empty()) {
            return Some(s.to_string());
        }
        if let Some(items) = value.as_array() {
            let joined = items
                .iter()
                .filter_map(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            if !joined.is_empty() {
                return Some(joined);
            }
        }
    }
    None
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

fn collect_text_segments(v: &Value) -> Vec<String> {
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

fn extract_results_from_text(text: &str) -> Vec<SearchResult> {
    let mut results = Vec::new();
    let md_re =
        Regex::new(r#"\[([^\]\n]{1,200})\]\((https?://[^\s\)]+)\)(?::\s*([^\n]+))?"#).unwrap();
    for cap in md_re.captures_iter(text) {
        push_result(
            &mut results,
            cap.get(1).map(|m| m.as_str()),
            cap.get(2).map(|m| m.as_str()),
            cap.get(3).map(|m| m.as_str().to_string()),
        );
    }

    let mut title: Option<String> = None;
    let mut url: Option<String> = None;
    let mut snippet = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(value) = strip_label(trimmed, &["title"]) {
            if url.is_some() {
                push_result(
                    &mut results,
                    title.as_deref(),
                    url.as_deref(),
                    Some(snippet.join(" ")),
                );
                url = None;
                snippet.clear();
            }
            title = Some(value.to_string());
        } else if let Some(value) = strip_label(trimmed, &["url", "link"]) {
            if url.is_some() {
                push_result(
                    &mut results,
                    title.as_deref(),
                    url.as_deref(),
                    Some(snippet.join(" ")),
                );
                snippet.clear();
            }
            url = Some(value.to_string());
        } else if let Some(value) = strip_label(trimmed, &["content", "snippet", "text"]) {
            snippet.push(value.to_string());
        } else if url.is_some() && !trimmed.is_empty() {
            snippet.push(trimmed.to_string());
        }
    }
    if url.is_some() {
        push_result(
            &mut results,
            title.as_deref(),
            url.as_deref(),
            Some(snippet.join(" ")),
        );
    }

    let url_re = Regex::new(r#"https?://[^\s\)\]\}>,]+"#).unwrap();
    for cap in url_re.captures_iter(text) {
        push_result(&mut results, None, cap.get(0).map(|m| m.as_str()), None);
    }
    dedupe_by_url(&mut results);
    results
}

fn strip_label<'a>(line: &'a str, labels: &[&str]) -> Option<&'a str> {
    let (label, value) = line.split_once(':')?;
    let label = label.trim().to_ascii_lowercase();
    if labels.iter().any(|candidate| *candidate == label) {
        Some(value.trim()).filter(|s| !s.is_empty())
    } else {
        None
    }
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("auto") {
        None
    } else {
        Some(value)
    }
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

fn tavily_api_key(state: &crate::AppState) -> Option<String> {
    settings_secret(state, |s| &s.tavily_api_key).or_else(|| env_first(&["TAVILY_API_KEY"]))
}

fn brave_search_api_key(state: &crate::AppState) -> Option<String> {
    settings_secret(state, |s| &s.brave_search_api_key)
        .or_else(|| env_first(&["BRAVE_SEARCH_API_KEY", "BRAVE_API_KEY"]))
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

fn normalize_domain_list(domains: &[String]) -> Vec<String> {
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

fn clean_plain_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clean_extracted_url(url: &str) -> String {
    url.trim()
        .trim_matches(|c| matches!(c, ')' | ']' | '}' | '>' | '"' | '\'' | ',' | ';' | '.'))
        .to_string()
}

fn is_http_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
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
    fn parses_configured_adapter_names() {
        assert_eq!(Adapter::parse(Some("tavily")), Adapter::Tavily);
        assert_eq!(Adapter::parse(Some("brave")), Adapter::Brave);
        assert_eq!(Adapter::parse(Some("exa")), Adapter::Exa);
        assert_eq!(Adapter::parse(Some("ddg")), Adapter::DuckDuckGo);
    }

    #[test]
    fn extracts_tavily_results() {
        let value = serde_json::json!({
            "results": [{
                "title": "Tavily Result",
                "url": "https://example.com/tavily",
                "content": "A Tavily summary."
            }]
        });
        let results = extract_tavily_results(&value);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Tavily Result");
        assert_eq!(results[0].snippet.as_deref(), Some("A Tavily summary."));
    }

    #[test]
    fn extracts_brave_grounding_results() {
        let value = serde_json::json!({
            "grounding": {
                "generic": [{
                    "title": "Brave Result",
                    "url": "https://example.com/brave",
                    "description": "A Brave grounding snippet."
                }]
            }
        });
        let results = extract_brave_results(&value);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].url, "https://example.com/brave");
        assert_eq!(
            results[0].snippet.as_deref(),
            Some("A Brave grounding snippet.")
        );
    }

    #[test]
    fn extracts_exa_sse_text_results() {
        let raw = r#"event: message
data: {"result":{"content":[{"type":"text","text":"Title: Exa Result\nURL: https://example.com/exa\nContent: An Exa summary."}]}}

data: [DONE]
"#;
        let results = extract_exa_results(raw).unwrap();
        assert!(results.iter().any(|r| r.title == "Exa Result"
            && r.url == "https://example.com/exa"
            && r.snippet.as_deref() == Some("An Exa summary.")));
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
