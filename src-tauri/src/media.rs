use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::store::{ProviderKind, Settings};
use crate::AppState;

const DEFAULT_DASHSCOPE_BASE: &str = "https://dashscope.aliyuncs.com";
const AIGC_GENERATION_PATH: &str = "/api/v1/services/aigc/multimodal-generation/generation";

#[derive(Deserialize)]
pub struct ImageGenerationRequest {
    pub prompt: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub size: String,
    #[serde(default)]
    pub negative_prompt: String,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub prompt_extend: bool,
    #[serde(default)]
    pub watermark: bool,
}

#[derive(Serialize, Clone)]
pub struct GeneratedImage {
    pub url: String,
}

#[derive(Serialize)]
pub struct ImageGenerationResult {
    pub request_id: String,
    pub images: Vec<GeneratedImage>,
    pub usage: Value,
}

#[derive(Deserialize)]
pub struct SpeechSynthesisRequest {
    pub text: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub voice: String,
    #[serde(default)]
    pub language_type: String,
}

#[derive(Serialize)]
pub struct SpeechSynthesisResult {
    pub request_id: String,
    pub url: String,
    pub usage: Value,
}

pub(crate) fn dashscope_base_url(settings: &Settings) -> String {
    let raw = settings.media_base_url.trim();
    let base = if raw.is_empty() {
        DEFAULT_DASHSCOPE_BASE
    } else {
        raw.trim_end_matches('/')
    };
    base.strip_suffix("/compatible-mode/v1")
        .unwrap_or(base)
        .trim_end_matches('/')
        .to_string()
}

pub(crate) fn dashscope_api_key(settings: &Settings) -> Option<String> {
    let media_key = settings.media_api_key.trim();
    if !media_key.is_empty() {
        return Some(media_key.to_string());
    }
    if settings.provider == ProviderKind::DashScope {
        let llm_key = settings.api_key.trim();
        if !llm_key.is_empty() {
            return Some(llm_key.to_string());
        }
    }
    std::env::var("DASHSCOPE_API_KEY")
        .ok()
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
}

fn normalize_dashscope_size(size: &str) -> String {
    let value = size.trim();
    if value.is_empty() {
        "1024*1024".to_string()
    } else {
        value.replace('x', "*").replace('X', "*")
    }
}

fn request_id(value: &Value) -> String {
    value["request_id"].as_str().unwrap_or_default().to_string()
}

fn usage(value: &Value) -> Value {
    value.get("usage").cloned().unwrap_or_else(|| json!({}))
}

fn collect_image_urls(value: &Value) -> Vec<GeneratedImage> {
    let mut urls = Vec::new();
    if let Some(choices) = value["output"]["choices"].as_array() {
        for choice in choices {
            if let Some(content) = choice["message"]["content"].as_array() {
                for part in content {
                    if let Some(url) = part["image"].as_str() {
                        urls.push(GeneratedImage {
                            url: url.to_string(),
                        });
                    }
                }
            }
        }
    }
    if let Some(results) = value["output"]["results"].as_array() {
        for result in results {
            if let Some(url) = result["url"]
                .as_str()
                .or_else(|| result["image_url"].as_str())
            {
                urls.push(GeneratedImage {
                    url: url.to_string(),
                });
            }
        }
    }
    if let Some(url) = value["output"]["image_url"].as_str() {
        urls.push(GeneratedImage {
            url: url.to_string(),
        });
    }
    urls
}

async fn dashscope_post(
    state: &AppState,
    settings: &Settings,
    body: Value,
) -> Result<Value, String> {
    let key = dashscope_api_key(settings).ok_or_else(|| {
        "Media API Key is missing. Configure DashScope in Settings > Providers or Media."
            .to_string()
    })?;
    let url = format!("{}{}", dashscope_base_url(settings), AIGC_GENERATION_PATH);
    let resp = state
        .http
        .post(url)
        .bearer_auth(key)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("DashScope request failed: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!("DashScope returned HTTP {status}: {text}"));
    }
    serde_json::from_str::<Value>(&text)
        .map_err(|e| format!("DashScope returned invalid JSON: {e}"))
}

pub async fn generate_image(
    state: &AppState,
    request: ImageGenerationRequest,
) -> Result<ImageGenerationResult, String> {
    let prompt = request.prompt.trim();
    if prompt.is_empty() {
        return Err("Prompt is required.".to_string());
    }
    let settings = state.settings.lock().unwrap().clone();
    let model = if request.model.trim().is_empty() {
        settings.image_model.trim()
    } else {
        request.model.trim()
    };
    let model = if model.is_empty() {
        "qwen-image-2.0"
    } else {
        model
    };
    let size = if request.size.trim().is_empty() {
        settings.image_size.trim()
    } else {
        request.size.trim()
    };

    let mut parameters = json!({
        "size": normalize_dashscope_size(size),
        "prompt_extend": request.prompt_extend,
        "watermark": request.watermark,
    });
    if let Some(seed) = request.seed {
        parameters["seed"] = json!(seed);
    }

    let mut content = vec![json!({ "text": prompt })];
    if !request.negative_prompt.trim().is_empty() {
        content.push(
            json!({ "text": format!("Negative prompt: {}", request.negative_prompt.trim()) }),
        );
    }

    let body = json!({
        "model": model,
        "input": {
            "messages": [
                {
                    "role": "user",
                    "content": content
                }
            ]
        },
        "parameters": parameters
    });
    let value = dashscope_post(state, &settings, body).await?;
    let images = collect_image_urls(&value);
    if images.is_empty() {
        return Err("DashScope image generation returned no image URLs.".to_string());
    }
    Ok(ImageGenerationResult {
        request_id: request_id(&value),
        images,
        usage: usage(&value),
    })
}

pub async fn synthesize_speech(
    state: &AppState,
    request: SpeechSynthesisRequest,
) -> Result<SpeechSynthesisResult, String> {
    let text = request.text.trim();
    if text.is_empty() {
        return Err("Text is required.".to_string());
    }
    let settings = state.settings.lock().unwrap().clone();
    let model = if request.model.trim().is_empty() {
        settings.tts_model.trim()
    } else {
        request.model.trim()
    };
    let model = if model.is_empty() {
        "qwen3-tts-flash"
    } else {
        model
    };
    let voice = if request.voice.trim().is_empty() {
        settings.tts_voice.trim()
    } else {
        request.voice.trim()
    };
    let voice = if voice.is_empty() { "Cherry" } else { voice };
    let language_type = if request.language_type.trim().is_empty() {
        "Chinese"
    } else {
        request.language_type.trim()
    };
    let body = json!({
        "model": model,
        "input": {
            "text": text,
            "voice": voice,
            "language_type": language_type
        }
    });
    let value = dashscope_post(state, &settings, body).await?;
    let url = value["output"]["audio"]["url"]
        .as_str()
        .ok_or_else(|| "DashScope TTS returned no audio URL.".to_string())?
        .to_string();
    Ok(SpeechSynthesisResult {
        request_id: request_id(&value),
        url,
        usage: usage(&value),
    })
}
