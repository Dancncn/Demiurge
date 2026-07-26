//! agent IPC adapter.

use crate::*;
use tauri::{AppHandle, State};

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
pub(crate) fn interrupt(app: AppHandle, state: State<'_, AppState>) {
    crate::biz::agent::interrupt(app, state.inner())
}

#[tauri::command]
pub(crate) fn session_engine_state(
    state: State<'_, AppState>,
) -> agent::session_engine::SessionEnginePanelState {
    crate::biz::agent::session_engine_state(state.inner())
}
