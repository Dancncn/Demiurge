//! workflow_entry IPC adapter.

use crate::*;
use tauri::{AppHandle, State};

#[tauri::command]
pub(crate) fn workflow_panel_state(
    state: State<'_, AppState>,
) -> agent::workflow_runtime::WorkflowPanelState {
    crate::biz::workflow_entry::workflow_panel_state(state.inner())
}

#[tauri::command]
pub(crate) fn workflow_run(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<String, String> {
    crate::biz::workflow_entry::workflow_run(app, state.inner(), name)
}

#[tauri::command]
pub(crate) fn workflow_stop(
    app: AppHandle,
    state: State<'_, AppState>,
    run_id: String,
) -> Result<(), String> {
    crate::biz::workflow_entry::workflow_stop(app, state.inner(), run_id)
}
