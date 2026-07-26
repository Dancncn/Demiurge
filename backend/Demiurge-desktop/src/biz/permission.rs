//! permission IPC Adapter.

use crate::permission::PermissionResponse;
use crate::store::PermissionMode;
use crate::store::Settings;
use crate::*;
use tauri::AppHandle;
use tauri::Emitter;

pub(crate) fn respond_confirm(
    state: &AppState,
    id: String,
    allow: bool,
    scope: tools::PermissionScope,
) {
    if let Some(tx) = state.pending_confirms.lock().unwrap().remove(&id) {
        let _ = tx.send(PermissionResponse { allow, scope });
    }
}

pub(crate) fn set_permission_mode(
    app: AppHandle,
    state: &AppState,
    mode: PermissionMode,
) -> Result<Settings, String> {
    let next = {
        let mut settings = state.settings.lock().unwrap();
        settings.permission_mode = mode;
        let next = settings.clone();
        let dir = state.data_dir.lock().unwrap().clone();
        store::save_settings(&dir, &next)?;
        next
    };
    if mode == PermissionMode::Plan {
        let mut plan = state.plan_state.lock().unwrap();
        plan.active = true;
        plan.approved = false;
        plan.approved_at = None;
    }
    let _ = app.emit("permission-mode-updated", mode);
    let _ = app.emit("plan-updated", state.plan_state.lock().unwrap().clone());
    Ok(next)
}

pub(crate) fn plan_state(state: &AppState) -> PlanState {
    state.plan_state.lock().unwrap().clone()
}

pub(crate) fn approve_plan(app: AppHandle, state: &AppState) -> Result<PlanState, String> {
    let next = {
        let mut plan = state.plan_state.lock().unwrap();
        if plan.path.is_none() {
            return Err("当前没有可批准的计划文件。".to_string());
        }
        plan.active = false;
        plan.approved = true;
        plan.approved_at = Some(store::now_millis());
        plan.clone()
    };
    {
        let mut settings = state.settings.lock().unwrap();
        settings.permission_mode = PermissionMode::Default;
        let dir = state.data_dir.lock().unwrap().clone();
        store::save_settings(&dir, &settings)?;
        let _ = app.emit("permission-mode-updated", settings.permission_mode);
    }
    let _ = app.emit("plan-updated", next.clone());
    Ok(next)
}

pub(crate) fn reject_plan(app: AppHandle, state: &AppState) -> PlanState {
    let next = {
        let mut plan = state.plan_state.lock().unwrap();
        plan.reset();
        plan.clone()
    };
    let _ = app.emit("plan-updated", next.clone());
    next
}

pub(crate) fn permission_panel_state(state: &AppState) -> permission::PermissionPanelState {
    permission::panel_state(state)
}

pub(crate) fn shell_policy_state() -> tools::ShellPolicyState {
    tools::shell_policy_state()
}

pub(crate) fn permission_reset_rule(
    state: &AppState,
    scope: tools::PermissionScope,
    tool: String,
    session_id: Option<String>,
    workspace_identity: Option<String>,
) -> Result<permission::PermissionPanelState, String> {
    permission::reset_rule(
        state,
        scope,
        &tool,
        session_id.as_deref(),
        workspace_identity.as_deref(),
    )
}

pub(crate) fn permission_upsert_rule(
    state: &AppState,
    input: permission::PermissionRuleInput,
) -> Result<permission::PermissionPanelState, String> {
    permission::upsert_rule(state, input)
}
