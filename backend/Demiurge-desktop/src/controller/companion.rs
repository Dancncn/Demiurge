//! companion IPC adapter.

use crate::*;
use tauri::{AppHandle, State};

#[tauri::command]
pub(crate) async fn companion_panel_state(
    state: State<'_, AppState>,
) -> Result<companion::CompanionPanelState, String> {
    crate::biz::companion::companion_panel_state(state.inner()).await
}

#[tauri::command]
pub(crate) async fn companion_clear_weather_cache(
    state: State<'_, AppState>,
) -> Result<companion::CompanionPanelState, String> {
    crate::biz::companion::companion_clear_weather_cache(state.inner()).await
}

#[tauri::command]
pub(crate) fn pomodoro_state(state: State<'_, AppState>) -> pomodoro::PomodoroPanelState {
    crate::biz::companion::pomodoro_state(state.inner())
}

#[tauri::command]
pub(crate) fn pomodoro_start(
    app: AppHandle,
    state: State<'_, AppState>,
    request: pomodoro::PomodoroStartRequest,
) -> Result<pomodoro::PomodoroPanelState, String> {
    crate::biz::companion::pomodoro_start(app, state.inner(), request)
}

#[tauri::command]
pub(crate) fn pomodoro_pause(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<pomodoro::PomodoroPanelState, String> {
    crate::biz::companion::pomodoro_pause(app, state.inner())
}

#[tauri::command]
pub(crate) fn pomodoro_resume(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<pomodoro::PomodoroPanelState, String> {
    crate::biz::companion::pomodoro_resume(app, state.inner())
}

#[tauri::command]
pub(crate) fn pomodoro_skip(
    app: AppHandle,
    state: State<'_, AppState>,
    request: Option<pomodoro::PomodoroSkipRequest>,
) -> Result<pomodoro::PomodoroPanelState, String> {
    crate::biz::companion::pomodoro_skip(app, state.inner(), request)
}

#[tauri::command]
pub(crate) fn companion_memory_suggestions(
    state: State<'_, AppState>,
) -> Vec<companion::CompanionMemorySuggestion> {
    crate::biz::companion::companion_memory_suggestions(state.inner())
}

#[tauri::command]
pub(crate) fn companion_memory_queue_state(
    state: State<'_, AppState>,
) -> companion::CompanionMemoryQueueState {
    crate::biz::companion::companion_memory_queue_state(state.inner())
}

#[tauri::command]
pub(crate) fn companion_enqueue_memory_suggestion(
    state: State<'_, AppState>,
    id: String,
) -> Result<companion::CompanionMemoryQueueState, String> {
    crate::biz::companion::companion_enqueue_memory_suggestion(state.inner(), id)
}

#[tauri::command]
pub(crate) fn companion_save_memory_queue_item(
    state: State<'_, AppState>,
    id: String,
    resolution: Option<String>,
) -> Result<companion::CompanionMemoryQueueState, String> {
    crate::biz::companion::companion_save_memory_queue_item(state.inner(), id, resolution)
}

#[tauri::command]
pub(crate) fn companion_ignore_memory_queue_item(
    state: State<'_, AppState>,
    id: String,
) -> Result<companion::CompanionMemoryQueueState, String> {
    crate::biz::companion::companion_ignore_memory_queue_item(state.inner(), id)
}

#[tauri::command]
pub(crate) fn companion_save_all_memory_queue_items(
    state: State<'_, AppState>,
) -> Result<companion::CompanionMemoryQueueState, String> {
    crate::biz::companion::companion_save_all_memory_queue_items(state.inner())
}

#[tauri::command]
pub(crate) fn companion_ignore_all_memory_queue_items(
    state: State<'_, AppState>,
) -> Result<companion::CompanionMemoryQueueState, String> {
    crate::biz::companion::companion_ignore_all_memory_queue_items(state.inner())
}

#[tauri::command]
pub(crate) fn companion_undo_memory_queue_item(
    state: State<'_, AppState>,
    id: String,
) -> Result<companion::CompanionMemoryQueueState, String> {
    crate::biz::companion::companion_undo_memory_queue_item(state.inner(), id)
}
