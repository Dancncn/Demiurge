//! agent_config IPC adapter.

use crate::*;
use tauri::State;

#[tauri::command]
pub(crate) fn agent_panel_state(state: State<'_, AppState>) -> agent::custom::AgentPanelState {
    crate::biz::agent_config::agent_panel_state(state.inner())
}

#[tauri::command]
pub(crate) fn agent_template_json() -> String {
    crate::biz::agent_config::agent_template_json()
}

#[tauri::command]
pub(crate) fn agent_validate_json(raw_json: String) -> agent::custom::AgentValidationResult {
    crate::biz::agent_config::agent_validate_json(raw_json)
}

#[tauri::command]
pub(crate) fn agent_read_file(
    state: State<'_, AppState>,
    name: String,
) -> Result<agent::custom::AgentEditorFile, String> {
    crate::biz::agent_config::agent_read_file(state.inner(), name)
}

#[tauri::command]
pub(crate) fn agent_save_file(
    state: State<'_, AppState>,
    file_name: String,
    raw_json: String,
) -> Result<agent::custom::AgentPanelState, String> {
    crate::biz::agent_config::agent_save_file(state.inner(), file_name, raw_json)
}

#[tauri::command]
pub(crate) fn agent_delete_file(
    state: State<'_, AppState>,
    name: String,
) -> Result<agent::custom::AgentPanelState, String> {
    crate::biz::agent_config::agent_delete_file(state.inner(), name)
}
