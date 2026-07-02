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
use serde::Serialize;
use serde_json::json;
use tauri::State;

use crate::media::{self, dashscope_api_key, dashscope_base_url, SpeechSynthesisRequest};
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
    let requested_voice_id = voice_id;
    let settings = state.settings.lock().unwrap().clone();
    if !settings.voice_enabled {
        return Err("语音未启用。".to_string());
    }
    let text = text.trim();
    if text.is_empty() {
        return Err("Speech synthesis text is required.".to_string());
    }

    let backend = settings.voice_tts_backend.trim().to_ascii_lowercase();
    match backend.as_str() {
        "dashscope" | "aliyun" | "bailian" | "media" => {
            let voice = requested_voice_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .or_else(|| {
                    let value = settings.voice_id.trim();
                    (!value.is_empty()).then_some(value)
                })
                .or_else(|| {
                    let value = settings.tts_voice.trim();
                    (!value.is_empty()).then_some(value)
                })
                .unwrap_or("Cherry")
                .to_string();
            let result = media::synthesize_speech(
                state.inner(),
                SpeechSynthesisRequest {
                    text: text.to_string(),
                    model: settings.tts_model.clone(),
                    voice,
                    language_type: "Chinese".to_string(),
                },
            )
            .await?;
            Ok(result.url)
        }
        "gpt-sovits" | "gpt_sovits" | "gptsovits" => {
            synthesize_with_gpt_sovits(&state.http, &settings, text, requested_voice_id).await
        }
        "none" | "" => Err(
            "No TTS backend selected. Set voice TTS backend to dashscope or gpt-sovits."
                .to_string(),
        ),
        other => Err(format!(
            "Unknown TTS backend `{other}`. Supported backends: dashscope, gpt-sovits."
        )),
    }
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

async fn synthesize_with_gpt_sovits(
    http: &reqwest::Client,
    settings: &Settings,
    text: &str,
    requested_voice_id: Option<String>,
) -> Result<String, String> {
    let ref_audio_path = requested_voice_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let value = settings.voice_id.trim();
            (!value.is_empty()).then_some(value)
        })
        .map(str::to_string)
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
    let body = json!({
        "text": text,
        "text_lang": text_lang,
        "ref_audio_path": ref_audio_path,
        "prompt_text": prompt_text,
        "prompt_lang": prompt_lang,
        "text_split_method": "cut5",
        "batch_size": 1,
        "media_type": "wav",
        "streaming_mode": false,
        "parallel_infer": true,
    });

    let resp = http
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("GPT-SoVITS request failed: {e}"))?;
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
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("GPT-SoVITS response read failed: {e}"))?;
    if !status.is_success() {
        let detail = String::from_utf8_lossy(&bytes);
        return Err(format!("GPT-SoVITS returned HTTP {status}: {detail}"));
    }
    if bytes.is_empty() {
        return Err("GPT-SoVITS returned empty audio.".to_string());
    }

    Ok(format!(
        "data:{};base64,{}",
        content_type,
        BASE64_STANDARD.encode(bytes)
    ))
}
