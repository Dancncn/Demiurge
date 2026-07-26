//! permission IPC adapter.

use crate::store::{PermissionMode, Settings};
use crate::*;
use tauri::{AppHandle, State};

#[tauri::command]
pub(crate) fn respond_confirm(
    state: State<'_, AppState>,
    id: String,
    allow: bool,
    scope: tools::PermissionScope,
) {
    crate::biz::permission::respond_confirm(state.inner(), id, allow, scope)
}

#[tauri::command]
pub(crate) fn set_permission_mode(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: PermissionMode,
) -> Result<Settings, String> {
    crate::biz::permission::set_permission_mode(app, state.inner(), mode)
}

#[tauri::command]
pub(crate) fn plan_state(state: State<'_, AppState>) -> PlanState {
    crate::biz::permission::plan_state(state.inner())
}

#[tauri::command]
pub(crate) fn approve_plan(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<PlanState, String> {
    crate::biz::permission::approve_plan(app, state.inner())
}

#[tauri::command]
pub(crate) fn reject_plan(app: AppHandle, state: State<'_, AppState>) -> PlanState {
    crate::biz::permission::reject_plan(app, state.inner())
}

#[tauri::command]
pub(crate) fn permission_panel_state(
    state: State<'_, AppState>,
) -> permission::PermissionPanelState {
    crate::biz::permission::permission_panel_state(state.inner())
}

#[tauri::command]
pub(crate) fn shell_policy_state() -> tools::ShellPolicyState {
    crate::biz::permission::shell_policy_state()
}

#[tauri::command]
pub(crate) fn permission_reset_rule(
    state: State<'_, AppState>,
    scope: tools::PermissionScope,
    tool: String,
    session_id: Option<String>,
    workspace_identity: Option<String>,
) -> Result<permission::PermissionPanelState, String> {
    crate::biz::permission::permission_reset_rule(
        state.inner(),
        scope,
        tool,
        session_id,
        workspace_identity,
    )
}

#[tauri::command]
pub(crate) fn permission_upsert_rule(
    state: State<'_, AppState>,
    input: permission::PermissionRuleInput,
) -> Result<permission::PermissionPanelState, String> {
    crate::biz::permission::permission_upsert_rule(state.inner(), input)
}
