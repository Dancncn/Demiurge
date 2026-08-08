//! window IPC adapter.

use crate::*;
use tauri::{AppHandle, State};

#[tauri::command]
pub(crate) fn main_window_minimize(app: AppHandle) -> Result<(), String> {
    crate::biz::window::main_window_minimize(app)
}

#[tauri::command]
pub(crate) fn main_window_toggle_maximize(app: AppHandle) -> Result<(), String> {
    crate::biz::window::main_window_toggle_maximize(app)
}

#[tauri::command]
pub(crate) fn main_window_close(app: AppHandle) -> Result<(), String> {
    crate::biz::window::main_window_close(app)
}

#[tauri::command]
pub(crate) fn desktop_companion_restore(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    crate::biz::window::desktop_companion_restore(app, state.inner())
}

#[tauri::command]
pub(crate) fn desktop_companion_show_main(app: AppHandle) -> Result<(), String> {
    crate::biz::window::desktop_companion_show_main(app)
}

#[tauri::command]
pub(crate) async fn open_widgets_window(app: AppHandle) -> Result<(), String> {
    crate::biz::window::open_widgets_window(app).await
}

#[tauri::command]
pub(crate) async fn open_live2d_window(app: AppHandle) -> Result<(), String> {
    crate::biz::window::open_live2d_window(app).await
}
