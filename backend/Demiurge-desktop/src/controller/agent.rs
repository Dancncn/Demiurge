//! agent IPC adapter.

use crate::*;
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use serde::Deserialize;
use tauri::{AppHandle, State};

const MAX_IMAGES: usize = 8;
const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
const MAX_TOTAL_IMAGE_BYTES: usize = 20 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ImageInputDto {
    mime_type: String,
    data: String,
    #[serde(default)]
    name: Option<String>,
}

#[tauri::command]
pub(crate) async fn send(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
) -> Result<(), String> {
    crate::biz::agent::send(app, state.inner(), text).await
}

#[tauri::command]
pub(crate) async fn send_with_agents(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
    agent_names: Vec<String>,
) -> Result<(), String> {
    crate::biz::agent::send_with_agents(app, state.inner(), text, agent_names).await
}

#[tauri::command]
pub(crate) async fn send_multimodal(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
    images: Vec<ImageInputDto>,
    agent_names: Vec<String>,
) -> Result<(), String> {
    let images = validate_images(images)?;
    crate::biz::agent::send_multimodal(app, state.inner(), text, images, agent_names).await
}

fn validate_images(
    images: Vec<ImageInputDto>,
) -> Result<Vec<agent::conversation::ImageAttachment>, String> {
    if images.is_empty() || images.len() > MAX_IMAGES {
        return Err(format!("Attach between 1 and {MAX_IMAGES} images."));
    }

    let mut total = 0usize;
    let mut validated = Vec::with_capacity(images.len());
    for image in images {
        let mime_type = image.mime_type.trim().to_ascii_lowercase();
        if !matches!(
            mime_type.as_str(),
            "image/jpeg" | "image/png" | "image/gif" | "image/webp"
        ) {
            return Err(format!(
                "Unsupported image type '{}'. Use JPEG, PNG, GIF or WebP.",
                image.mime_type
            ));
        }
        let decoded = BASE64_STANDARD
            .decode(image.data.as_bytes())
            .map_err(|_| "An attached image contains invalid base64 data.".to_string())?;
        if decoded.is_empty() || decoded.len() > MAX_IMAGE_BYTES {
            return Err(format!(
                "Each image must be between 1 byte and {} MB.",
                MAX_IMAGE_BYTES / 1024 / 1024
            ));
        }
        total = total.saturating_add(decoded.len());
        if total > MAX_TOTAL_IMAGE_BYTES {
            return Err(format!(
                "Attached images exceed the {} MB total limit.",
                MAX_TOTAL_IMAGE_BYTES / 1024 / 1024
            ));
        }
        let name = image
            .name
            .map(|name| name.trim().chars().take(255).collect::<String>())
            .filter(|name| !name.is_empty());
        validated.push(agent::conversation::ImageAttachment {
            mime_type,
            data: image.data,
            name,
        });
    }
    Ok(validated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_supported_images_and_rejects_other_mime_types() {
        let valid = validate_images(vec![ImageInputDto {
            mime_type: "image/png".to_string(),
            data: "YWJj".to_string(),
            name: Some("sample.png".to_string()),
        }])
        .unwrap();
        assert_eq!(valid[0].name.as_deref(), Some("sample.png"));

        let error = validate_images(vec![ImageInputDto {
            mime_type: "image/svg+xml".to_string(),
            data: "YWJj".to_string(),
            name: None,
        }])
        .unwrap_err();
        assert!(error.contains("Unsupported image type"));
    }
}

#[tauri::command]
pub(crate) fn interrupt(app: AppHandle, state: State<'_, AppState>) {
    crate::biz::agent::interrupt(app, state.inner())
}

#[tauri::command]
pub(crate) fn session_engine_state(
    state: State<'_, AppState>,
) -> agent::session_engine::SessionEnginePanelState {
    crate::biz::agent::session_engine_state(state.inner())
}
