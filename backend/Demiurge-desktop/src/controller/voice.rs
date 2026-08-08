//! Voice IPC Adapter。

use tauri::State;

use crate::connection_tests::ConnectionTestResult;
use crate::store::Settings;
use crate::voice::VoiceStatus;
use crate::AppState;

#[tauri::command]
pub(crate) fn voice_status(state: State<'_, AppState>) -> VoiceStatus {
    crate::voice::voice_status(state.inner())
}

#[tauri::command]
pub(crate) async fn voice_transcribe(
    audio: Vec<u8>,
    mime_type: Option<String>,
    language: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    crate::voice::voice_transcribe(audio, mime_type, language, state.inner()).await
}

#[tauri::command]
pub(crate) async fn voice_synthesize(
    text: String,
    voice_id: Option<String>,
    speed: Option<f32>,
    emotion: Option<String>,
    streaming: Option<bool>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    crate::voice::voice_synthesize(text, voice_id, speed, emotion, streaming, state.inner()).await
}

#[tauri::command]
pub(crate) async fn voice_tts_check(
    settings: Settings,
    state: State<'_, AppState>,
) -> Result<ConnectionTestResult, String> {
    crate::voice::voice_tts_check(settings, state.inner()).await
}
