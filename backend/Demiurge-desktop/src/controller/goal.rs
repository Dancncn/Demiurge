//! goal IPC adapter.

use crate::*;
use tauri::{AppHandle, State};

#[tauri::command]
pub(crate) fn goal_panel_state(state: State<'_, AppState>) -> Option<agent::goal::GoalPanelState> {
    crate::biz::goal::goal_panel_state(state.inner())
}

#[tauri::command]
pub(crate) fn goal_pause(
    state: State<'_, AppState>,
) -> Result<Option<agent::goal::GoalPanelState>, String> {
    crate::biz::goal::goal_pause(state.inner())
}

#[tauri::command]
pub(crate) async fn goal_resume(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<agent::goal::GoalPanelState>, String> {
    crate::biz::goal::goal_resume(app, state.inner()).await
}

#[tauri::command]
pub(crate) async fn goal_continue(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<agent::goal::GoalPanelState>, String> {
    crate::biz::goal::goal_continue(app, state.inner()).await
}

#[tauri::command]
pub(crate) fn goal_clear(state: State<'_, AppState>) -> Option<agent::goal::GoalPanelState> {
    crate::biz::goal::goal_clear(state.inner())
}
