//! Voice adapters.
//!
//! STT (speech-to-text) is wired to cloud transcription endpoints that follow
//! the OpenAI Whisper `/audio/transcriptions` multipart shape. The active
//! backend is selected by `settings.voice_stt_backend`:
//!   - `dashscope`  → Aliyun Bailian / DashScope ASR (`qwen3-asr-flash`)
//!   - `openai`     → the active provider's OpenAI-compatible whisper endpoint
//! TTS is still a reserved placeholder.
use serde::Serialize;
use tauri::State;

use crate::media::{dashscope_api_key, dashscope_base_url};
use crate::store::Settings;

#[derive(Clone, Debug, Serialize)]
pub struct VoiceStatus {
    pub enabled: bool,
    pub stt_backend: String,
    pub tts_backend: String,
    pub voice_id: String,
    pub ready: bool,
    pub reason: String,
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

#[tauri::command]
pub fn voice_status(state: State<'_, crate::AppState>) -> VoiceStatus {
    let settings = state.settings.lock().unwrap().clone();
    let (ready, reason) = stt_ready(&settings);
    VoiceStatus {
        enabled: settings.voice_enabled,
        stt_backend: settings.voice_stt_backend.clone(),
        tts_backend: settings.voice_tts_backend.clone(),
        voice_id: settings.voice_id.clone(),
        ready,
        reason,
    }
}

/// Transcribe in-memory audio bytes (recorded in the WebView) via the configured
/// cloud STT backend. `mime_type` defaults to `audio/webm` (MediaRecorder output).
#[tauri::command]
pub async fn voice_transcribe(
    audio: Vec<u8>,
    mime_type: Option<String>,
    language: Option<String>,
    state: State<'_, crate::AppState>,
) -> Result<String, String> {
    let settings = state.settings.lock().unwrap().clone();
    if !settings.voice_enabled {
        return Err("语音未启用。".to_string());
    }
    if audio.is_empty() {
        return Err("没有可转写的音频。".to_string());
    }
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
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!("STT 返回 HTTP {status}：{text}"));
    }
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("STT 返回的 JSON 无法解析：{e}"))?;
    value["text"]
        .as_str()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "STT 响应中没有文本字段。".to_string())
}

#[tauri::command]
pub async fn voice_synthesize(
    text: String,
    voice_id: Option<String>,
    state: State<'_, crate::AppState>,
) -> Result<String, String> {
    let _ = (text, voice_id);
    let settings = state.settings.lock().unwrap().clone();
    if !settings.voice_enabled {
        return Err("语音未启用。".to_string());
    }
    Err(format!(
        "语音合成 API 已预留，但尚未接入 TTS 后端：{}。",
        settings.voice_tts_backend
    ))
}
