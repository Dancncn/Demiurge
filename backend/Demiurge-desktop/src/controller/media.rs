//! media IPC adapter.

use crate::*;
use tauri::{AppHandle, State};

#[tauri::command]
pub(crate) fn ocr_image_bytes(
    state: State<'_, AppState>,
    bytes: Vec<u8>,
) -> Result<String, String> {
    crate::biz::media::ocr_image_bytes(state.inner(), bytes)
}

#[tauri::command]
pub(crate) async fn media_generate_image(
    state: State<'_, AppState>,
    request: media::ImageGenerationRequest,
) -> Result<media::ImageGenerationResult, String> {
    crate::biz::media::media_generate_image(state.inner(), request).await
}

#[tauri::command]
pub(crate) async fn media_synthesize_speech(
    state: State<'_, AppState>,
    request: media::SpeechSynthesisRequest,
) -> Result<media::SpeechSynthesisResult, String> {
    crate::biz::media::media_synthesize_speech(state.inner(), request).await
}

#[tauri::command]
pub(crate) fn ocr_model_status(state: State<'_, AppState>) -> ocr::OcrModelStatus {
    crate::biz::media::ocr_model_status(state.inner())
}

#[tauri::command]
pub(crate) async fn ocr_download_models(
    app: AppHandle,
    state: State<'_, AppState>,
    source: ocr::OcrModelSource,
) -> Result<ocr::OcrModelStatus, String> {
    crate::biz::media::ocr_download_models(app, state.inner(), source).await
}
