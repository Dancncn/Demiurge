//! media IPC Adapter.

use crate::*;
use tauri::AppHandle;

pub(crate) fn ocr_image_bytes(state: &AppState, bytes: Vec<u8>) -> Result<String, String> {
    if !state.settings.lock().unwrap().computer_use_enabled {
        return Err("Computer Use / OCR is not enabled.".to_string());
    }
    let img = image::load_from_memory(&bytes)
        .map_err(|e| format!("读取图片失败：{e}"))?
        .to_rgba8();
    ocr::recognize_rgba(state, img).map(|frame| frame.text)
}

pub(crate) async fn media_generate_image(
    state: &AppState,
    request: media::ImageGenerationRequest,
) -> Result<media::ImageGenerationResult, String> {
    media::generate_image(state, request).await
}

pub(crate) async fn media_synthesize_speech(
    state: &AppState,
    request: media::SpeechSynthesisRequest,
) -> Result<media::SpeechSynthesisResult, String> {
    media::synthesize_speech(state, request).await
}

pub(crate) fn ocr_model_status(state: &AppState) -> ocr::OcrModelStatus {
    ocr::model_status(state)
}

pub(crate) async fn ocr_download_models(
    app: AppHandle,
    state: &AppState,
    source: ocr::OcrModelSource,
) -> Result<ocr::OcrModelStatus, String> {
    ocr::download_models(app, state, source).await
}
