use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};

use crate::llm::{self, ProviderAdapterKind, ProviderProfile};
use crate::store::{ProviderKind, Settings};

const CONNECTION_TEST_TIMEOUT_SECS: u64 = 20;
const CONNECTION_TEST_MAX_ERROR_CHARS: usize = 600;
const WEB_SEARCH_TEST_QUERY: &str = "Demiurge connection test";

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ConnectionTestResult {
    pub ok: bool,
    pub target: String,
    pub detail: String,
    pub latency_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ProviderTestKind {
    OpenAiCompatible,
    Anthropic,
    Gemini,
}

impl ProviderTestKind {
    fn from_adapter(adapter: ProviderAdapterKind) -> Self {
        match adapter {
            ProviderAdapterKind::OpenAiCompatible => ProviderTestKind::OpenAiCompatible,
            ProviderAdapterKind::Anthropic => ProviderTestKind::Anthropic,
            ProviderAdapterKind::Gemini => ProviderTestKind::Gemini,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProviderTestRequest {
    provider: ProviderKind,
    kind: ProviderTestKind,
    base_url: String,
    model: String,
    api_key: Option<String>,
}

pub async fn test_provider(
    client: &reqwest::Client,
    settings: Settings,
) -> Result<ConnectionTestResult, String> {
    let request = ProviderTestRequest::from_settings(settings)?;
    let target = request.target();
    let started = Instant::now();

    let resp = match request.kind {
        ProviderTestKind::OpenAiCompatible => {
            let body = build_openai_provider_test_body(&request.model);
            let mut req = client
                .post(format!("{}/chat/completions", request.base_url))
                .timeout(Duration::from_secs(CONNECTION_TEST_TIMEOUT_SECS))
                .json(&body);
            if let Some(key) = request.api_key.as_deref() {
                req = req.bearer_auth(key);
            }
            req.send()
                .await
                .map_err(|e| request_error(request.provider_label(), e))?
        }
        ProviderTestKind::Anthropic => {
            let key = request
                .api_key
                .as_deref()
                .ok_or_else(|| "未配置 API Key，请在设置里填写。".to_string())?;
            client
                .post(format!("{}/messages", request.base_url))
                .timeout(Duration::from_secs(CONNECTION_TEST_TIMEOUT_SECS))
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01")
                .json(&build_anthropic_provider_test_body(&request.model))
                .send()
                .await
                .map_err(|e| request_error(request.provider_label(), e))?
        }
        ProviderTestKind::Gemini => {
            let key = request
                .api_key
                .as_deref()
                .ok_or_else(|| "未配置 API Key，请在设置里填写。".to_string())?;
            client
                .post(format!(
                    "{}/models/{}:generateContent",
                    request.base_url, request.model
                ))
                .timeout(Duration::from_secs(CONNECTION_TEST_TIMEOUT_SECS))
                .header("x-goog-api-key", key)
                .json(&build_gemini_provider_test_body())
                .send()
                .await
                .map_err(|e| request_error(request.provider_label(), e))?
        }
    };

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!(
            "{} 连接测试返回 HTTP {status}：{}",
            request.provider_label(),
            cap_chars(body.trim(), CONNECTION_TEST_MAX_ERROR_CHARS)
        ));
    }

    Ok(ConnectionTestResult {
        ok: true,
        target,
        detail: format!(
            "{} accepted model `{}` with a minimal request.",
            request.provider_label(),
            request.model
        ),
        latency_ms: elapsed_ms(started),
    })
}

pub async fn test_web_search(
    client: &reqwest::Client,
    settings: Settings,
    provider: Option<String>,
) -> Result<ConnectionTestResult, String> {
    let request = WebSearchTestRequest::from_settings(&settings, provider.as_deref())?;
    let started = Instant::now();
    let (target, detail) = match request.adapter {
        WebSearchAdapter::Auto => match check_bing_search(client).await {
            Ok(target) => (target, "Bing search connected for auto Web Search.".to_string()),
            Err(bing_err) => match check_duckduckgo_search(client).await {
                Ok(target) => (
                    target,
                    format!(
                        "DuckDuckGo fallback connected for auto Web Search. Bing check failed: {bing_err}"
                    ),
                ),
                Err(duck_err) => {
                    return Err(format!(
                        "Auto Web Search connection test failed. Bing: {bing_err}; DuckDuckGo: {duck_err}"
                    ));
                }
            },
        },
        WebSearchAdapter::Bing => (
            check_bing_search(client).await?,
            "Bing search connected.".to_string(),
        ),
        WebSearchAdapter::DuckDuckGo => (
            check_duckduckgo_search(client).await?,
            "DuckDuckGo search connected.".to_string(),
        ),
        WebSearchAdapter::Tavily => (
            check_tavily_search(client, &request).await?,
            "Tavily key accepted a minimal search request.".to_string(),
        ),
        WebSearchAdapter::Brave => (
            check_brave_search(client, &request).await?,
            "Brave Search key accepted a minimal search request.".to_string(),
        ),
        WebSearchAdapter::Exa => (
            check_exa_search(client, &request).await?,
            "Exa key accepted a minimal search request.".to_string(),
        ),
    };

    Ok(ConnectionTestResult {
        ok: true,
        target,
        detail,
        latency_ms: elapsed_ms(started),
    })
}

impl ProviderTestRequest {
    fn from_settings(settings: Settings) -> Result<Self, String> {
        let profile = ProviderProfile::for_kind(settings.provider);
        let api_key = llm::require_api_key(&settings, profile)?.map(str::to_string);
        let base_url = normalize_base_url(&settings.base_url)?;
        let model = settings.model.trim().to_string();
        if model.is_empty() {
            return Err("Model is required for the provider connection test.".to_string());
        }
        let kind = ProviderTestKind::from_adapter(profile.adapter_kind());
        Ok(Self {
            provider: settings.provider,
            kind,
            base_url,
            model,
            api_key,
        })
    }

    fn target(&self) -> String {
        match self.kind {
            ProviderTestKind::OpenAiCompatible => {
                format!("{}/chat/completions ({})", self.base_url, self.model)
            }
            ProviderTestKind::Anthropic => format!("{}/messages ({})", self.base_url, self.model),
            ProviderTestKind::Gemini => {
                format!("{}/models/{}:generateContent", self.base_url, self.model)
            }
        }
    }

    fn provider_label(&self) -> &'static str {
        provider_label(self.provider)
    }
}

fn build_openai_provider_test_body(model: &str) -> Value {
    json!({
        "model": model,
        "messages": [{ "role": "user", "content": "ping" }],
        "stream": false,
        "max_tokens": 1,
    })
}

fn build_anthropic_provider_test_body(model: &str) -> Value {
    json!({
        "model": model,
        "max_tokens": 1,
        "messages": [{
            "role": "user",
            "content": [{ "type": "text", "text": "ping" }]
        }],
    })
}

fn build_gemini_provider_test_body() -> Value {
    json!({
        "contents": [{
            "role": "user",
            "parts": [{ "text": "ping" }]
        }],
        "generationConfig": {
            "maxOutputTokens": 1
        }
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WebSearchAdapter {
    Auto,
    Bing,
    DuckDuckGo,
    Tavily,
    Brave,
    Exa,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WebSearchTestRequest {
    adapter: WebSearchAdapter,
    tavily_key: Option<String>,
    brave_key: Option<String>,
    exa_key: Option<String>,
    tavily_endpoint: String,
    exa_endpoint: String,
}

impl WebSearchTestRequest {
    fn from_settings(settings: &Settings, provider: Option<&str>) -> Result<Self, String> {
        Self::from_settings_with_env(settings, provider, |key| std::env::var(key).ok())
    }

    fn from_settings_with_env(
        settings: &Settings,
        provider: Option<&str>,
        env: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, String> {
        let selected = provider
            .and_then(non_empty)
            .or_else(|| non_empty(settings.web_search_provider.as_str()))
            .unwrap_or("auto");
        let adapter = parse_web_search_adapter(selected)?;
        let request = Self {
            adapter,
            tavily_key: setting_or_env(&settings.tavily_api_key, &env, &["TAVILY_API_KEY"]),
            brave_key: setting_or_env(
                &settings.brave_search_api_key,
                &env,
                &["BRAVE_SEARCH_API_KEY", "BRAVE_API_KEY"],
            ),
            exa_key: setting_or_env(&settings.exa_api_key, &env, &["EXA_API_KEY"]),
            tavily_endpoint: env_first(&env, &["TAVILY_SEARCH_URL", "TAVILY_ENDPOINT_URL"])
                .unwrap_or_else(|| "https://api.tavily.com/search".to_string()),
            exa_endpoint: env_first(&env, &["EXA_MCP_URL"])
                .unwrap_or_else(|| "https://mcp.exa.ai/mcp".to_string()),
        };
        request.validate_keys()?;
        Ok(request)
    }

    fn validate_keys(&self) -> Result<(), String> {
        match self.adapter {
            WebSearchAdapter::Tavily if self.tavily_key.is_none() => Err(
                "Tavily connection test requires a Tavily API Key or TAVILY_API_KEY.".to_string(),
            ),
            WebSearchAdapter::Brave if self.brave_key.is_none() => Err(
                "Brave connection test requires a Brave Search API Key, BRAVE_SEARCH_API_KEY, or BRAVE_API_KEY."
                    .to_string(),
            ),
            WebSearchAdapter::Exa if self.exa_key.is_none() => {
                Err("Exa connection test requires an Exa API Key or EXA_API_KEY.".to_string())
            }
            _ => Ok(()),
        }
    }
}

async fn check_bing_search(client: &reqwest::Client) -> Result<String, String> {
    let target = "https://www.bing.com/search".to_string();
    let resp = client
        .get(&target)
        .timeout(Duration::from_secs(CONNECTION_TEST_TIMEOUT_SECS))
        .query(&[("q", WEB_SEARCH_TEST_QUERY), ("setmkt", "en-US")])
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
        .map_err(|e| request_error("Bing", e))?;
    ensure_success("Bing", resp).await?;
    Ok(format!("{target}?q={WEB_SEARCH_TEST_QUERY}"))
}

async fn check_duckduckgo_search(client: &reqwest::Client) -> Result<String, String> {
    let target = "https://api.duckduckgo.com/".to_string();
    let resp = client
        .get(&target)
        .timeout(Duration::from_secs(CONNECTION_TEST_TIMEOUT_SECS))
        .query(&[
            ("q", WEB_SEARCH_TEST_QUERY),
            ("format", "json"),
            ("no_html", "1"),
            ("no_redirect", "1"),
            ("skip_disambig", "1"),
        ])
        .send()
        .await
        .map_err(|e| request_error("DuckDuckGo", e))?;
    ensure_success("DuckDuckGo", resp).await?;
    Ok(format!("{target}?q={WEB_SEARCH_TEST_QUERY}&format=json"))
}

async fn check_tavily_search(
    client: &reqwest::Client,
    request: &WebSearchTestRequest,
) -> Result<String, String> {
    let key = request
        .tavily_key
        .as_deref()
        .ok_or_else(|| "Tavily connection test requires an API key.".to_string())?;
    let body = json!({
        "query": WEB_SEARCH_TEST_QUERY,
        "search_depth": "basic",
        "max_results": 1,
    });
    let resp = client
        .post(&request.tavily_endpoint)
        .timeout(Duration::from_secs(CONNECTION_TEST_TIMEOUT_SECS))
        .header("User-Agent", "Demiurge WebSearch")
        .bearer_auth(key)
        .header("x-api-key", key)
        .json(&body)
        .send()
        .await
        .map_err(|e| request_error("Tavily", e))?;
    ensure_success("Tavily", resp).await?;
    Ok(request.tavily_endpoint.clone())
}

async fn check_brave_search(
    client: &reqwest::Client,
    request: &WebSearchTestRequest,
) -> Result<String, String> {
    let key = request
        .brave_key
        .as_deref()
        .ok_or_else(|| "Brave connection test requires an API key.".to_string())?;
    let target = "https://api.search.brave.com/res/v1/llm/context".to_string();
    let resp = client
        .get(&target)
        .timeout(Duration::from_secs(CONNECTION_TEST_TIMEOUT_SECS))
        .query(&[("q", WEB_SEARCH_TEST_QUERY)])
        .header("Accept", "application/json")
        .header("X-Subscription-Token", key)
        .send()
        .await
        .map_err(|e| request_error("Brave", e))?;
    ensure_success("Brave", resp).await?;
    Ok(target)
}

async fn check_exa_search(
    client: &reqwest::Client,
    request: &WebSearchTestRequest,
) -> Result<String, String> {
    let key = request
        .exa_key
        .as_deref()
        .ok_or_else(|| "Exa connection test requires an API key.".to_string())?;
    let body = json!({
        "jsonrpc": "2.0",
        "id": "demiurge-web-search-connection-test",
        "method": "tools/call",
        "params": {
            "name": "web_search_exa",
            "arguments": {
                "query": WEB_SEARCH_TEST_QUERY,
                "type": "fast",
                "numResults": 1,
                "livecrawl": "never",
                "contextMaxCharacters": 1000,
            }
        }
    });
    let resp = client
        .post(&request.exa_endpoint)
        .timeout(Duration::from_secs(CONNECTION_TEST_TIMEOUT_SECS))
        .header("Accept", "application/json, text/event-stream")
        .bearer_auth(key)
        .json(&body)
        .send()
        .await
        .map_err(|e| request_error("Exa", e))?;
    ensure_success("Exa", resp).await?;
    Ok(request.exa_endpoint.clone())
}

async fn ensure_success(label: &str, resp: reqwest::Response) -> Result<(), String> {
    let status = resp.status();
    if status.is_success() {
        return Ok(());
    }
    let body = resp.text().await.unwrap_or_default();
    Err(format!(
        "{label} 连接测试返回 HTTP {status}：{}",
        cap_chars(body.trim(), CONNECTION_TEST_MAX_ERROR_CHARS)
    ))
}

fn parse_web_search_adapter(value: &str) -> Result<WebSearchAdapter, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "auto" => Ok(WebSearchAdapter::Auto),
        "bing" => Ok(WebSearchAdapter::Bing),
        "duckduckgo" | "ddg" => Ok(WebSearchAdapter::DuckDuckGo),
        "tavily" => Ok(WebSearchAdapter::Tavily),
        "brave" => Ok(WebSearchAdapter::Brave),
        "exa" => Ok(WebSearchAdapter::Exa),
        other => Err(format!(
            "Web Search provider 不支持 {other:?}；可选：auto, bing, duckduckgo, tavily, brave, exa"
        )),
    }
}

fn normalize_base_url(base_url: &str) -> Result<String, String> {
    let base_url = base_url.trim().trim_end_matches('/').to_string();
    if base_url.is_empty() {
        return Err("Base URL is required for the provider connection test.".to_string());
    }
    let parsed =
        reqwest::Url::parse(&base_url).map_err(|e| format!("Base URL is not a valid URL：{e}"))?;
    match parsed.scheme() {
        "http" | "https" => Ok(base_url),
        _ => Err("Base URL must start with http:// or https://.".to_string()),
    }
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn setting_or_env(
    setting: &str,
    env: &impl Fn(&str) -> Option<String>,
    names: &[&str],
) -> Option<String> {
    non_empty(setting)
        .map(str::to_string)
        .or_else(|| env_first(env, names))
}

fn env_first(env: &impl Fn(&str) -> Option<String>, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| env(name).and_then(|value| non_empty(&value).map(str::to_string)))
}

fn provider_label(provider: ProviderKind) -> &'static str {
    match provider {
        ProviderKind::DeepSeek => "DeepSeek",
        ProviderKind::DashScope => "DashScope",
        ProviderKind::OpenAi => "OpenAI",
        ProviderKind::OpenRouter => "OpenRouter",
        ProviderKind::OpenAiCompatible => "OpenAI Compatible",
        ProviderKind::Local => "Local Endpoint",
        ProviderKind::Anthropic => "Anthropic",
        ProviderKind::Gemini => "Gemini",
        ProviderKind::Glm => "GLM",
        ProviderKind::MiniMax => "MiniMax",
        ProviderKind::Xai => "xAI Grok",
        ProviderKind::Groq => "Groq",
        ProviderKind::Mistral => "Mistral AI",
        ProviderKind::Moonshot => "Moonshot / Kimi",
        ProviderKind::Perplexity => "Perplexity",
        ProviderKind::Doubao => "Doubao (Volcengine Ark)",
        ProviderKind::Hunyuan => "Tencent Hunyuan",
        ProviderKind::StepFun => "StepFun",
        ProviderKind::Custom => "Custom Provider",
    }
}

fn request_error(provider: &str, err: reqwest::Error) -> String {
    if err.is_timeout() {
        format!("{provider} 连接测试超时。")
    } else if err.is_connect() {
        format!("{provider} 连接测试无法连接端点。")
    } else {
        format!("{provider} 连接测试请求失败：{err}")
    }
}

fn cap_chars(value: &str, max_chars: usize) -> String {
    let mut iter = value.chars();
    let head: String = iter.by_ref().take(max_chars).collect();
    if iter.next().is_some() {
        format!("{head}...")
    } else if head.is_empty() {
        "(empty response body)".to_string()
    } else {
        head
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(provider: ProviderKind, api_key: &str) -> Settings {
        Settings {
            provider,
            base_url: " https://example.test/v1/ ".to_string(),
            api_key: api_key.to_string(),
            model: " test-model ".to_string(),
            ..Settings::default()
        }
    }

    #[test]
    fn provider_request_trims_and_routes_openai_compatible() {
        let req = ProviderTestRequest::from_settings(settings(ProviderKind::DeepSeek, "sk-test"))
            .unwrap();
        assert_eq!(req.kind, ProviderTestKind::OpenAiCompatible);
        assert_eq!(req.base_url, "https://example.test/v1");
        assert_eq!(req.model, "test-model");
        assert_eq!(
            req.target(),
            "https://example.test/v1/chat/completions (test-model)"
        );
    }

    #[test]
    fn provider_request_allows_local_without_key() {
        let req = ProviderTestRequest::from_settings(settings(ProviderKind::Local, "")).unwrap();
        assert_eq!(req.kind, ProviderTestKind::OpenAiCompatible);
        assert_eq!(req.api_key, None);
    }

    #[test]
    fn provider_request_requires_key_for_remote_provider() {
        let err =
            ProviderTestRequest::from_settings(settings(ProviderKind::OpenAi, "")).unwrap_err();
        assert!(err.contains("API Key"));
    }

    #[test]
    fn provider_request_routes_anthropic_and_gemini() {
        let anthropic =
            ProviderTestRequest::from_settings(settings(ProviderKind::Anthropic, "sk-test"))
                .unwrap();
        assert_eq!(anthropic.kind, ProviderTestKind::Anthropic);
        assert_eq!(
            anthropic.target(),
            "https://example.test/v1/messages (test-model)"
        );

        let gemini =
            ProviderTestRequest::from_settings(settings(ProviderKind::Gemini, "sk-test")).unwrap();
        assert_eq!(gemini.kind, ProviderTestKind::Gemini);
        assert_eq!(
            gemini.target(),
            "https://example.test/v1/models/test-model:generateContent"
        );
    }

    #[test]
    fn provider_test_bodies_use_minimal_output() {
        let openai = build_openai_provider_test_body("m");
        assert_eq!(openai["model"], "m");
        assert_eq!(openai["stream"], false);
        assert_eq!(openai["max_tokens"], 1);

        let anthropic = build_anthropic_provider_test_body("m");
        assert_eq!(anthropic["model"], "m");
        assert_eq!(anthropic["max_tokens"], 1);

        let gemini = build_gemini_provider_test_body();
        assert_eq!(gemini["generationConfig"]["maxOutputTokens"], 1);
    }

    #[test]
    fn provider_request_rejects_invalid_base_url() {
        let mut cfg = settings(ProviderKind::Local, "");
        cfg.base_url = "ftp://example.test".to_string();
        let err = ProviderTestRequest::from_settings(cfg).unwrap_err();
        assert!(err.contains("http:// or https://"));
    }

    fn web_settings(provider: &str) -> Settings {
        Settings {
            web_search_provider: provider.to_string(),
            ..Settings::default()
        }
    }

    #[test]
    fn web_search_request_uses_provider_override_and_settings_key() {
        let mut cfg = web_settings("auto");
        cfg.tavily_api_key = " tvly-test ".to_string();
        let req =
            WebSearchTestRequest::from_settings_with_env(&cfg, Some("tavily"), |_| None).unwrap();
        assert_eq!(req.adapter, WebSearchAdapter::Tavily);
        assert_eq!(req.tavily_key.as_deref(), Some("tvly-test"));
    }

    #[test]
    fn web_search_request_uses_env_fallbacks() {
        let cfg = web_settings("exa");
        let req = WebSearchTestRequest::from_settings_with_env(&cfg, None, |name| match name {
            "EXA_API_KEY" => Some(" exa-env ".to_string()),
            "EXA_MCP_URL" => Some(" https://exa.test/mcp ".to_string()),
            _ => None,
        })
        .unwrap();
        assert_eq!(req.adapter, WebSearchAdapter::Exa);
        assert_eq!(req.exa_key.as_deref(), Some("exa-env"));
        assert_eq!(req.exa_endpoint, "https://exa.test/mcp");
    }

    #[test]
    fn web_search_request_requires_key_for_keyed_adapters() {
        let err =
            WebSearchTestRequest::from_settings_with_env(&web_settings("brave"), None, |_| None)
                .unwrap_err();
        assert!(err.contains("Brave"));
        assert!(err.contains("API Key"));
    }

    #[test]
    fn web_search_request_allows_public_adapters_without_key() {
        let bing =
            WebSearchTestRequest::from_settings_with_env(&web_settings("bing"), None, |_| None)
                .unwrap();
        assert_eq!(bing.adapter, WebSearchAdapter::Bing);

        let duckduckgo =
            WebSearchTestRequest::from_settings_with_env(&web_settings("duckduckgo"), None, |_| {
                None
            })
            .unwrap();
        assert_eq!(duckduckgo.adapter, WebSearchAdapter::DuckDuckGo);
    }

    #[test]
    fn web_search_adapter_parse_supports_aliases() {
        assert_eq!(
            parse_web_search_adapter("ddg").unwrap(),
            WebSearchAdapter::DuckDuckGo
        );
        assert!(parse_web_search_adapter("unknown").is_err());
    }
}
