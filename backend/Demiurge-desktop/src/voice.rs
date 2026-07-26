//! Voice adapters.
//!
//! STT (speech-to-text) is wired to cloud transcription endpoints that follow
//! the OpenAI Whisper `/audio/transcriptions` multipart shape. The active
//! backend is selected by `settings.voice_stt_backend`:
//!   - `dashscope`  → Aliyun Bailian / DashScope ASR (`qwen3-asr-flash`)
//!   - `openai`     → the active provider's OpenAI-compatible whisper endpoint
//! TTS can route to the DashScope media adapter or a user-managed GPT-SoVITS
//! HTTP service for one-shot synthesis.
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

use crate::connection_tests::ConnectionTestResult;
use crate::media::{dashscope_api_key, dashscope_base_url};
use crate::store::Settings;

const VOICE_CONNECTION_TEST_TIMEOUT_SECS: u64 = 20;
const VOICE_TTS_TIMEOUT_SECS: u64 = 90;
const VOICE_CONNECTION_TEST_TEXT: &str = "Demiurge voice test.";
const MAX_STT_AUDIO_BYTES: usize = 25 * 1024 * 1024;
const MAX_STT_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_TTS_JSON_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_TTS_AUDIO_RESPONSE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
pub struct VoiceStatus {
    pub enabled: bool,
    pub stt_backend: String,
    pub tts_backend: String,
    pub voice_id: String,
    pub ready: bool,
    pub reason: String,
    pub tts_ready: bool,
    pub tts_reason: String,
    pub speed: f32,
    pub emotion: String,
    pub streaming: bool,
    pub fallback_enabled: bool,
}

/// Whether STT is actually usable for the given settings: enabled, a supported
/// backend selected, and the corresponding credential resolvable.
fn stt_ready(settings: &Settings) -> (bool, String) {
    if !settings.voice_enabled {
        return (false, "语音未启用。".to_string());
    }
    match settings
        .voice_stt_backend
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "dashscope" => {
            if dashscope_api_key(settings).is_some() {
                (true, "DashScope ASR 已就绪。".to_string())
            } else {
                (
                    false,
                    "DashScope STT 未找到 API 密钥（请在「媒体」或当前供应商中配置）。".to_string(),
                )
            }
        }
        "openai" => {
            if !settings.api_key.trim().is_empty() {
                (true, "OpenAI 兼容 Whisper 已就绪。".to_string())
            } else {
                (
                    false,
                    "OpenAI 兼容 STT 需要当前供应商的 API 密钥。".to_string(),
                )
            }
        }
        "none" | "" => (
            false,
            "未选择 STT 后端（可设为 dashscope 或 openai）。".to_string(),
        ),
        other => (
            false,
            format!("未知的 STT 后端「{other}」（支持 dashscope / openai）。"),
        ),
    }
}

fn tts_ready(settings: &Settings) -> (bool, String) {
    if !settings.voice_enabled {
        return (false, "语音未启用。".to_string());
    }
    match normalize_tts_backend(&settings.voice_tts_backend).as_str() {
        "dashscope" => {
            if dashscope_api_key(settings).is_some() {
                (true, "DashScope TTS 已就绪。".to_string())
            } else {
                (
                    false,
                    "DashScope TTS 未找到 API 密钥（请在「媒体」或当前供应商中配置）。".to_string(),
                )
            }
        }
        "gpt-sovits" => {
            if let Err(reason) = validate_local_tts_url(settings) {
                (false, reason)
            } else if resolve_voice_id(settings, None)
                .or_else(|| env_value("DEMIURGE_GPT_SOVITS_REF_AUDIO"))
                .is_some()
            {
                (true, "GPT-SoVITS 本地 TTS 已配置参考音频。".to_string())
            } else {
                (
                    false,
                    "GPT-SoVITS 需要参考音频路径，请填写 Voice ID 或 DEMIURGE_GPT_SOVITS_REF_AUDIO。"
                        .to_string(),
                )
            }
        }
        "cosyvoice" => match validate_local_tts_url(settings) {
            Ok(()) => (
                true,
                "CosyVoice 本地 TTS 地址配置有效，可进行连接测试。".to_string(),
            ),
            Err(reason) => (false, reason),
        },
        "none" | "" => (false, "未选择 TTS 后端。".to_string()),
        other => (
            false,
            format!("未知的 TTS 后端「{other}」（支持 dashscope / gpt-sovits / cosyvoice）。"),
        ),
    }
}

pub fn voice_status(state: &crate::AppState) -> VoiceStatus {
    let settings = state.settings.lock().unwrap().clone();
    let (ready, reason) = stt_ready(&settings);
    let (tts_ready, tts_reason) = tts_ready(&settings);
    VoiceStatus {
        enabled: settings.voice_enabled,
        stt_backend: settings.voice_stt_backend.clone(),
        tts_backend: settings.voice_tts_backend.clone(),
        voice_id: settings.voice_id.clone(),
        ready,
        reason,
        tts_ready,
        tts_reason,
        speed: normalized_speed(settings.voice_speed),
        emotion: settings.voice_emotion.clone(),
        streaming: settings.voice_streaming,
        fallback_enabled: settings.voice_tts_fallback,
    }
}

/// Transcribe in-memory audio bytes (recorded in the WebView) via the configured
/// cloud STT backend. `mime_type` defaults to `audio/webm` (MediaRecorder output).
pub async fn voice_transcribe(
    audio: Vec<u8>,
    mime_type: Option<String>,
    language: Option<String>,
    state: &crate::AppState,
) -> Result<String, String> {
    let settings = state.settings.lock().unwrap().clone();
    if !settings.voice_enabled {
        return Err("语音未启用。".to_string());
    }
    if audio.is_empty() {
        return Err("没有可转写的音频。".to_string());
    }
    validate_stt_audio_size(audio.len())?;
    let mime = mime_type
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("audio/webm")
        .to_string();
    let lang = language
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let backend = settings.voice_stt_backend.trim().to_ascii_lowercase();
    match backend.as_str() {
        "dashscope" => {
            let key = dashscope_api_key(&settings)
                .ok_or_else(|| "DashScope STT 未找到 API 密钥。".to_string())?;
            let url = format!(
                "{}/compatible-mode/v1/audio/transcriptions",
                dashscope_base_url(&settings)
            );
            transcribe_multipart(&state.http, url, key, "qwen3-asr-flash", audio, &mime, lang).await
        }
        "openai" => {
            let key = settings.api_key.trim().to_string();
            if key.is_empty() {
                return Err("OpenAI 兼容 STT 需要当前供应商的 API 密钥。".to_string());
            }
            let url = format!(
                "{}/audio/transcriptions",
                settings.base_url.trim_end_matches('/')
            );
            transcribe_multipart(&state.http, url, key, "whisper-1", audio, &mime, lang).await
        }
        "none" | "" => Err("未选择 STT 后端（可在设置中设为 dashscope 或 openai）。".to_string()),
        other => Err(format!(
            "未知的 STT 后端「{other}」（支持 dashscope / openai）。"
        )),
    }
}

/// POST audio to an OpenAI-Whisper-shaped `/audio/transcriptions` endpoint and
/// return the recognized text.
async fn transcribe_multipart(
    http: &reqwest::Client,
    url: String,
    api_key: String,
    model: &str,
    audio: Vec<u8>,
    mime: &str,
    language: Option<String>,
) -> Result<String, String> {
    let file_name = if mime.contains("mp4") || mime.contains("m4a") {
        "audio.m4a"
    } else if mime.contains("wav") {
        "audio.wav"
    } else if mime.contains("mpeg") || mime.contains("mp3") {
        "audio.mp3"
    } else {
        "audio.webm"
    };
    let part = reqwest::multipart::Part::bytes(audio)
        .file_name(file_name.to_string())
        .mime_str(mime)
        .map_err(|e| format!("音频 MIME 无效：{e}"))?;
    let mut form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("model", model.to_string());
    if let Some(l) = language {
        form = form.text("language", l);
    }
    let resp = http
        .post(url)
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("STT 请求失败：{e}"))?;
    let status = resp.status();
    let body = read_response_limited(resp, MAX_STT_RESPONSE_BYTES, "STT").await?;
    let text = String::from_utf8_lossy(&body);
    if !status.is_success() {
        return Err(format!("STT 返回 HTTP {status}：{text}"));
    }
    let value: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| format!("STT 返回的 JSON 无法解析：{e}"))?;
    value["text"]
        .as_str()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "STT 响应中没有文本字段。".to_string())
}

pub async fn voice_synthesize(
    text: String,
    voice_id: Option<String>,
    speed: Option<f32>,
    emotion: Option<String>,
    streaming: Option<bool>,
    state: &crate::AppState,
) -> Result<String, String> {
    let settings = state.settings.lock().unwrap().clone();
    if !settings.voice_enabled {
        return Err("语音未启用。".to_string());
    }
    let text = text.trim();
    if text.is_empty() {
        return Err("Speech synthesis text is required.".to_string());
    }

    let options = TtsOptions::from_settings(&settings, voice_id, speed, emotion, streaming);
    let backend = normalize_tts_backend(&settings.voice_tts_backend);
    match synthesize_with_backend(state, &settings, text, &backend, &options).await {
        Ok(url) => Ok(url),
        Err(primary_error)
            if options.allow_fallback
                && backend != "dashscope"
                && dashscope_api_key(&settings).is_some() =>
        {
            synthesize_with_backend(state, &settings, text, "dashscope", &options)
                .await
                .map_err(|fallback_error| {
                    format!("{primary_error}\nDashScope fallback also failed: {fallback_error}")
                })
        }
        Err(error) => Err(error),
    }
}

pub async fn voice_tts_check(
    settings: Settings,
    state: &crate::AppState,
) -> Result<ConnectionTestResult, String> {
    if !settings.voice_enabled {
        return Err("语音未启用。".to_string());
    }
    let backend = normalize_tts_backend(&settings.voice_tts_backend);
    let mut options = TtsOptions::from_settings(&settings, None, None, None, Some(false));
    options.timeout_secs = VOICE_CONNECTION_TEST_TIMEOUT_SECS;
    let target = voice_tts_target(&settings, &backend);
    let started = Instant::now();
    let url = synthesize_with_backend(
        state,
        &settings,
        VOICE_CONNECTION_TEST_TEXT,
        &backend,
        &options,
    )
    .await?;
    Ok(ConnectionTestResult {
        ok: true,
        target,
        detail: format!(
            "TTS connection ok. Received {} audio reference.",
            if url.starts_with("data:") {
                "inline"
            } else {
                "remote"
            }
        ),
        latency_ms: started.elapsed().as_millis() as u64,
    })
}

#[derive(Clone, Debug)]
struct TtsOptions {
    requested_voice_id: Option<String>,
    speed: f32,
    emotion: String,
    streaming: bool,
    allow_fallback: bool,
    timeout_secs: u64,
}

impl TtsOptions {
    fn from_settings(
        settings: &Settings,
        requested_voice_id: Option<String>,
        speed: Option<f32>,
        emotion: Option<String>,
        streaming: Option<bool>,
    ) -> Self {
        TtsOptions {
            requested_voice_id,
            speed: normalized_speed(speed.unwrap_or(settings.voice_speed)),
            emotion: emotion
                .unwrap_or_else(|| settings.voice_emotion.clone())
                .trim()
                .to_string(),
            streaming: streaming.unwrap_or(settings.voice_streaming),
            allow_fallback: settings.voice_tts_fallback,
            timeout_secs: VOICE_TTS_TIMEOUT_SECS,
        }
    }
}

async fn synthesize_with_backend(
    state: &crate::AppState,
    settings: &Settings,
    text: &str,
    backend: &str,
    options: &TtsOptions,
) -> Result<String, String> {
    match backend {
        "dashscope" => synthesize_with_dashscope(state, settings, text, options).await,
        "gpt-sovits" => synthesize_with_gpt_sovits(&state.http, settings, text, options).await,
        "cosyvoice" => synthesize_with_cosyvoice(&state.http, settings, text, options).await,
        "none" | "" => Err(
            "No TTS backend selected. Set voice TTS backend to dashscope, gpt-sovits, or cosyvoice."
                .to_string(),
        ),
        other => Err(format!(
            "Unknown TTS backend `{other}`. Supported backends: dashscope, gpt-sovits, cosyvoice."
        )),
    }
}

async fn synthesize_with_dashscope(
    state: &crate::AppState,
    settings: &Settings,
    text: &str,
    options: &TtsOptions,
) -> Result<String, String> {
    let voice = resolve_dashscope_voice(settings, options);
    let key = dashscope_api_key(settings).ok_or_else(|| {
        "Media API Key is missing. Configure DashScope in Settings > Providers or Media."
            .to_string()
    })?;
    let model = settings.tts_model.trim();
    let model = if model.is_empty() {
        "qwen3-tts-flash"
    } else {
        model
    };
    let mut parameters = json!({});
    if (options.speed - 1.0).abs() > f32::EPSILON {
        parameters["speed"] = json!(options.speed);
    }
    if !options.emotion.is_empty() {
        parameters["emotion"] = json!(options.emotion);
    }
    let mut body = json!({
        "model": model,
        "input": {
            "text": text,
            "voice": voice,
            "language_type": "Chinese"
        }
    });
    if parameters
        .as_object()
        .map(|m| !m.is_empty())
        .unwrap_or(false)
    {
        body["parameters"] = parameters;
    }
    let url = format!(
        "{}/api/v1/services/aigc/multimodal-generation/generation",
        dashscope_base_url(settings)
    );
    let resp = state
        .http
        .post(url)
        .timeout(Duration::from_secs(options.timeout_secs))
        .bearer_auth(key)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("DashScope TTS request failed: {e}"))?;
    let status = resp.status();
    let body = read_response_limited(resp, MAX_TTS_JSON_RESPONSE_BYTES, "DashScope TTS").await?;
    let text = String::from_utf8_lossy(&body);
    if !status.is_success() {
        return Err(format!("DashScope TTS returned HTTP {status}: {text}"));
    }
    let value: Value = serde_json::from_slice(&body)
        .map_err(|e| format!("DashScope TTS returned invalid JSON: {e}"))?;
    value["output"]["audio"]["url"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "DashScope TTS returned no audio URL.".to_string())
}

fn normalize_tts_backend(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "aliyun" | "bailian" | "media" => "dashscope".to_string(),
        "gpt_sovits" | "gptsovits" => "gpt-sovits".to_string(),
        "cosy" | "cosy_voice" | "cosy-voice" => "cosyvoice".to_string(),
        other => other.to_string(),
    }
}

fn voice_tts_target(settings: &Settings, backend: &str) -> String {
    match backend {
        "dashscope" => format!(
            "{}/api/v1/services/aigc/multimodal-generation/generation ({})",
            dashscope_base_url(settings),
            settings.tts_model.trim()
        ),
        "gpt-sovits" | "cosyvoice" => format!("{}/tts ({backend})", gpt_sovits_base_url(settings)),
        other => other.to_string(),
    }
}

fn normalized_speed(speed: f32) -> f32 {
    if speed.is_finite() {
        (speed * 100.0).round().clamp(50.0, 200.0) / 100.0
    } else {
        1.0
    }
}

fn resolve_voice_id(settings: &Settings, requested: Option<&str>) -> Option<String> {
    requested
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let value = settings.voice_id.trim();
            (!value.is_empty()).then_some(value)
        })
        .map(str::to_string)
}

fn resolve_dashscope_voice(settings: &Settings, options: &TtsOptions) -> String {
    resolve_voice_id(settings, options.requested_voice_id.as_deref())
        .or_else(|| {
            let value = settings.tts_voice.trim();
            (!value.is_empty()).then(|| value.to_string())
        })
        .unwrap_or_else(|| "Cherry".to_string())
}

fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn gpt_sovits_base_url(settings: &Settings) -> String {
    let value = settings.media_base_url.trim().trim_end_matches('/');
    if value.is_empty() || value == "https://dashscope.aliyuncs.com" {
        "http://127.0.0.1:9880".to_string()
    } else {
        value.to_string()
    }
}

fn validate_local_tts_url(settings: &Settings) -> Result<(), String> {
    let value = gpt_sovits_base_url(settings);
    let url = reqwest::Url::parse(&value).map_err(|e| format!("本地 TTS 地址无效：{e}"))?;
    if matches!(url.scheme(), "http" | "https") {
        Ok(())
    } else {
        Err("本地 TTS 地址必须使用 http 或 https。".to_string())
    }
}

async fn synthesize_with_gpt_sovits(
    http: &reqwest::Client,
    settings: &Settings,
    text: &str,
    options: &TtsOptions,
) -> Result<String, String> {
    let ref_audio_path = resolve_voice_id(settings, options.requested_voice_id.as_deref())
        .or_else(|| env_value("DEMIURGE_GPT_SOVITS_REF_AUDIO"))
        .ok_or_else(|| {
            "GPT-SoVITS requires a reference audio path. Set Voice ID or DEMIURGE_GPT_SOVITS_REF_AUDIO."
                .to_string()
        })?;

    let prompt_text = env_value("DEMIURGE_GPT_SOVITS_PROMPT_TEXT").unwrap_or_default();
    let prompt_lang =
        env_value("DEMIURGE_GPT_SOVITS_PROMPT_LANG").unwrap_or_else(|| "zh".to_string());
    let text_lang = env_value("DEMIURGE_GPT_SOVITS_TEXT_LANG").unwrap_or_else(|| "zh".to_string());
    let url = format!("{}/tts", gpt_sovits_base_url(settings));
    let mut body = json!({
        "text": text,
        "text_lang": text_lang,
        "ref_audio_path": ref_audio_path,
        "prompt_text": prompt_text,
        "prompt_lang": prompt_lang,
        "text_split_method": "cut5",
        "batch_size": 1,
        "media_type": "wav",
        "streaming_mode": options.streaming,
        "parallel_infer": true,
        "speed_factor": options.speed,
    });
    if !options.emotion.is_empty() {
        body["emotion"] = json!(options.emotion);
    }

    let resp = http
        .post(url)
        .timeout(Duration::from_secs(options.timeout_secs))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("GPT-SoVITS request failed: {e}"))?;
    decode_audio_response(resp, "GPT-SoVITS").await
}

async fn synthesize_with_cosyvoice(
    http: &reqwest::Client,
    settings: &Settings,
    text: &str,
    options: &TtsOptions,
) -> Result<String, String> {
    let voice = resolve_voice_id(settings, options.requested_voice_id.as_deref())
        .or_else(|| {
            let value = settings.tts_voice.trim();
            (!value.is_empty()).then(|| value.to_string())
        })
        .unwrap_or_else(|| "default".to_string());
    let url = format!("{}/tts", gpt_sovits_base_url(settings));
    let mut body = json!({
        "text": text,
        "voice": voice,
        "speaker": voice,
        "speed": options.speed,
        "stream": options.streaming,
        "streaming_mode": options.streaming,
        "format": "wav",
    });
    if !options.emotion.is_empty() {
        body["emotion"] = json!(options.emotion);
    }
    let resp = http
        .post(url)
        .timeout(Duration::from_secs(options.timeout_secs))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("CosyVoice request failed: {e}"))?;
    decode_audio_response(resp, "CosyVoice").await
}

async fn decode_audio_response(resp: reqwest::Response, label: &str) -> Result<String, String> {
    let status = resp.status();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("audio/wav")
        .split(';')
        .next()
        .unwrap_or("audio/wav")
        .trim()
        .to_string();
    let bytes = read_response_limited(resp, MAX_TTS_AUDIO_RESPONSE_BYTES, label).await?;
    if !status.is_success() {
        let detail = String::from_utf8_lossy(&bytes);
        return Err(format!("{label} returned HTTP {status}: {detail}"));
    }
    if bytes.is_empty() {
        return Err(format!("{label} returned empty audio."));
    }
    if content_type.contains("json") {
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|e| format!("{label} returned invalid JSON audio response: {e}"))?;
        if let Some(url) = value
            .pointer("/url")
            .and_then(Value::as_str)
            .or_else(|| value.pointer("/audio_url").and_then(Value::as_str))
            .or_else(|| value.pointer("/output/audio/url").and_then(Value::as_str))
        {
            return Ok(url.to_string());
        }
        if let Some(data) = value
            .pointer("/audio")
            .and_then(Value::as_str)
            .or_else(|| value.pointer("/data").and_then(Value::as_str))
        {
            if data.starts_with("data:") {
                return Ok(data.to_string());
            }
            return Ok(format!("data:audio/wav;base64,{data}"));
        }
        return Err(format!(
            "{label} JSON response did not include an audio URL or base64 audio."
        ));
    }

    Ok(format!(
        "data:{};base64,{}",
        content_type,
        BASE64_STANDARD.encode(bytes)
    ))
}

fn validate_stt_audio_size(size: usize) -> Result<(), String> {
    if size <= MAX_STT_AUDIO_BYTES {
        Ok(())
    } else {
        Err(format!(
            "STT audio is too large: {size} bytes (maximum {MAX_STT_AUDIO_BYTES} bytes)."
        ))
    }
}

fn append_limited_chunk(
    output: &mut Vec<u8>,
    chunk: &[u8],
    limit: usize,
    label: &str,
) -> Result<(), String> {
    if chunk.len() > limit.saturating_sub(output.len()) {
        return Err(format!(
            "{label} response exceeded the {limit}-byte safety limit."
        ));
    }
    output.extend_from_slice(chunk);
    Ok(())
}

async fn read_response_limited(
    response: reqwest::Response,
    limit: usize,
    label: &str,
) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .map(|length| length > limit as u64)
        .unwrap_or(false)
    {
        return Err(format!(
            "{label} response exceeded the {limit}-byte safety limit."
        ));
    }

    let mut output = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("{label} response read failed: {e}"))?;
        append_limited_chunk(&mut output, &chunk, limit, label)?;
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_tts_backend_aliases() {
        assert_eq!(normalize_tts_backend("gpt_sovits"), "gpt-sovits");
        assert_eq!(normalize_tts_backend("cosy-voice"), "cosyvoice");
        assert_eq!(normalize_tts_backend("media"), "dashscope");
    }

    #[test]
    fn clamps_voice_speed_for_provider_requests() {
        assert_eq!(normalized_speed(0.1), 0.5);
        assert_eq!(normalized_speed(2.8), 2.0);
        assert_eq!(normalized_speed(f32::NAN), 1.0);
        assert_eq!(normalized_speed(1.234), 1.23);
    }

    #[test]
    fn tts_options_prefer_explicit_values() {
        let settings = Settings {
            voice_speed: 0.9,
            voice_emotion: "calm".to_string(),
            voice_streaming: false,
            voice_tts_fallback: true,
            ..Settings::default()
        };
        let options = TtsOptions::from_settings(
            &settings,
            Some("voice-a".to_string()),
            Some(1.4),
            Some("happy".to_string()),
            Some(true),
        );
        assert_eq!(options.requested_voice_id.as_deref(), Some("voice-a"));
        assert_eq!(options.speed, 1.4);
        assert_eq!(options.emotion, "happy");
        assert!(options.streaming);
        assert!(options.allow_fallback);
    }

    #[test]
    fn rejects_oversized_stt_audio_before_network_io() {
        assert!(validate_stt_audio_size(MAX_STT_AUDIO_BYTES).is_ok());
        let error = validate_stt_audio_size(MAX_STT_AUDIO_BYTES + 1).unwrap_err();
        assert!(error.contains("too large"));
    }

    #[test]
    fn bounded_response_buffer_rejects_chunk_crossing_limit() {
        let mut body = vec![1, 2, 3];
        let error = append_limited_chunk(&mut body, &[4, 5], 4, "TTS").unwrap_err();
        assert!(error.contains("exceeded"));
        assert_eq!(body, vec![1, 2, 3]);
    }
}
