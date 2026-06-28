//! Voice API placeholders.
//!
//! The UI and settings can now depend on a stable command surface while the
//! concrete STT/TTS backend is still undecided (GPT-SoVITS, CosyVoice, etc.).
use serde::Serialize;
use tauri::State;

#[derive(Clone, Debug, Serialize)]
pub struct VoiceStatus {
    pub enabled: bool,
    pub stt_backend: String,
    pub tts_backend: String,
    pub voice_id: String,
    pub ready: bool,
    pub reason: String,
}

#[tauri::command]
pub fn voice_status(state: State<'_, crate::AppState>) -> VoiceStatus {
    let settings = state.settings.lock().unwrap().clone();
    let configured = settings.voice_enabled
        && (settings.voice_stt_backend != "none" || settings.voice_tts_backend != "none");
    VoiceStatus {
        enabled: settings.voice_enabled,
        stt_backend: settings.voice_stt_backend,
        tts_backend: settings.voice_tts_backend,
        voice_id: settings.voice_id,
        ready: false,
        reason: if configured {
            "语音 API 已预留，但当前版本还没有接入具体 STT/TTS 后端。".to_string()
        } else {
            "语音未启用或未选择后端。".to_string()
        },
    }
}

#[tauri::command]
pub async fn voice_transcribe(
    audio_path: String,
    state: State<'_, crate::AppState>,
) -> Result<String, String> {
    let _ = audio_path;
    let settings = state.settings.lock().unwrap().clone();
    if !settings.voice_enabled {
        return Err("语音未启用。".to_string());
    }
    Err(format!(
        "语音转写 API 已预留，但尚未接入 STT 后端：{}。",
        settings.voice_stt_backend
    ))
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
