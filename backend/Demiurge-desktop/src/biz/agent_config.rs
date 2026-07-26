//! agent_config IPC Adapter.

use crate::*;

pub(crate) fn agent_panel_state(state: &AppState) -> agent::custom::AgentPanelState {
    agent::custom::panel_state(state)
}

pub(crate) fn agent_template_json() -> String {
    agent::custom::template_json()
}

pub(crate) fn agent_validate_json(raw_json: String) -> agent::custom::AgentValidationResult {
    agent::custom::validate_raw(&raw_json)
}

pub(crate) fn agent_read_file(
    state: &AppState,
    name: String,
) -> Result<agent::custom::AgentEditorFile, String> {
    agent::custom::read_editor_file(state, &name)
}

pub(crate) fn agent_save_file(
    state: &AppState,
    file_name: String,
    raw_json: String,
) -> Result<agent::custom::AgentPanelState, String> {
    let panel = agent::custom::save_editor_file(state, &file_name, &raw_json)?;
    Ok(panel)
}

pub(crate) fn agent_delete_file(
    state: &AppState,
    name: String,
) -> Result<agent::custom::AgentPanelState, String> {
    agent::custom::delete_editor_file(state, &name)
}
