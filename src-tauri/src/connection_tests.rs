use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};

use crate::llm::{self, ProviderProfile};
use crate::store::{ProviderKind, Settings};

const CONNECTION_TEST_TIMEOUT_SECS: u64 = 20;
const CONNECTION_TEST_MAX_ERROR_CHARS: usize = 600;

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

impl ProviderTestRequest {
    fn from_settings(settings: Settings) -> Result<Self, String> {
        let profile = ProviderProfile::for_kind(settings.provider);
        let api_key = llm::require_api_key(&settings, profile)?.map(str::to_string);
        let base_url = normalize_base_url(&settings.base_url)?;
        let model = settings.model.trim().to_string();
        if model.is_empty() {
            return Err("Model is required for the provider connection test.".to_string());
        }
        let kind = match settings.provider {
            ProviderKind::Anthropic => ProviderTestKind::Anthropic,
            ProviderKind::Gemini => ProviderTestKind::Gemini,
            ProviderKind::DeepSeek
            | ProviderKind::DashScope
            | ProviderKind::OpenAi
            | ProviderKind::OpenRouter
            | ProviderKind::Glm
            | ProviderKind::MiniMax
            | ProviderKind::Custom
            | ProviderKind::OpenAiCompatible
            | ProviderKind::Local => ProviderTestKind::OpenAiCompatible,
        };
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
}
