//! Workflow IPC Adapter。

use std::collections::BTreeMap;

use serde_json::Value;
use tauri::{AppHandle, State};

use crate::agent::workflow_runtime::{
    WorkflowDefinitionInfo, WorkflowDryRun, WorkflowTemplateInfo, WorkflowValidationReport,
};
use crate::AppState;

#[tauri::command]
pub(crate) fn workflow_validate(
    state: State<'_, AppState>,
    name: String,
    inputs: BTreeMap<String, Value>,
) -> Result<WorkflowValidationReport, String> {
    crate::agent::workflow_runtime::workflow_validate(state.inner(), name, inputs)
}

#[tauri::command]
pub(crate) fn workflow_dry_run(
    state: State<'_, AppState>,
    name: String,
    inputs: BTreeMap<String, Value>,
) -> Result<WorkflowDryRun, String> {
    crate::agent::workflow_runtime::workflow_dry_run(state.inner(), name, inputs)
}

#[tauri::command]
pub(crate) fn workflow_templates() -> Vec<WorkflowTemplateInfo> {
    crate::agent::workflow_runtime::workflow_templates()
}

#[tauri::command]
pub(crate) fn workflow_install_template(
    state: State<'_, AppState>,
    template_id: String,
    name: Option<String>,
) -> Result<WorkflowDefinitionInfo, String> {
    crate::agent::workflow_runtime::workflow_install_template(state.inner(), template_id, name)
}

#[tauri::command]
pub(crate) fn workflow_run_with_inputs(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    inputs: BTreeMap<String, Value>,
) -> Result<String, String> {
    crate::agent::workflow_runtime::workflow_run_with_inputs(app, state.inner(), name, inputs)
}

#[tauri::command]
pub(crate) fn workflow_retry_failed_node(
    app: AppHandle,
    state: State<'_, AppState>,
    run_id: String,
) -> Result<String, String> {
    crate::agent::workflow_runtime::workflow_retry_failed_node(app, state.inner(), run_id)
}
