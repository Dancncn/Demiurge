//! settings IPC adapter.

use crate::store::Settings;
use crate::*;
use tauri::{AppHandle, State};

#[tauri::command]
pub(crate) fn get_settings(state: State<'_, AppState>) -> Settings {
    crate::biz::settings::get_settings(state.inner())
}

#[tauri::command]
pub(crate) fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<(), String> {
    crate::biz::settings::save_settings(app, state.inner(), settings)
}
