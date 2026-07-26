//! workflow_entry IPC Adapter.

use crate::*;
use tauri::AppHandle;

pub(crate) fn workflow_panel_state(
    state: &AppState,
) -> agent::workflow_runtime::WorkflowPanelState {
    agent::workflow_runtime::panel_state(state)
}

pub(crate) fn workflow_run(
    app: AppHandle,
    state: &AppState,
    name: String,
) -> Result<String, String> {
    let run_id = agent::workflow_runtime::launch(&app, state, &name)?;
    let app_for_task = app.clone();
    let run_id_for_task = run_id.clone();
    tauri::async_runtime::spawn(async move {
        agent::workflow_runtime::run_launched(app_for_task, run_id_for_task, name).await;
    });
    Ok(run_id)
}

pub(crate) fn workflow_stop(
    app: AppHandle,
    state: &AppState,
    run_id: String,
) -> Result<(), String> {
    agent::workflow_runtime::stop(&app, state, &run_id)
}
