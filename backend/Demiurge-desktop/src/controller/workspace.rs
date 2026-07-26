//! Workspace 与 Git IPC Adapter。

use tauri::{AppHandle, State};

use crate::workspace::{
    GitBranch, GitChangedFile, WorkspaceEntry, WorkspaceFilePreview, WorkspaceState,
};
use crate::AppState;

#[tauri::command]
pub(crate) fn select_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<WorkspaceState, String> {
    crate::workspace::select_workspace(app, state.inner(), path)
}

#[tauri::command]
pub(crate) fn list_workspace_directory(
    state: State<'_, AppState>,
    relative_path: Option<String>,
) -> Result<Vec<WorkspaceEntry>, String> {
    crate::workspace::list_workspace_directory(state.inner(), relative_path)
}

#[tauri::command]
pub(crate) fn read_workspace_file(
    state: State<'_, AppState>,
    relative_path: String,
) -> Result<WorkspaceFilePreview, String> {
    crate::workspace::read_workspace_file(state.inner(), relative_path)
}

#[tauri::command]
pub(crate) fn git_branches(
    state: State<'_, AppState>,
    expected_workspace_path: String,
) -> Result<Vec<GitBranch>, String> {
    crate::workspace::git_branches(state.inner(), expected_workspace_path)
}

#[tauri::command]
pub(crate) fn switch_git_branch(
    app: AppHandle,
    state: State<'_, AppState>,
    branch: String,
    expected_workspace_path: String,
) -> Result<WorkspaceState, String> {
    crate::workspace::switch_git_branch(app, state.inner(), branch, expected_workspace_path)
}

#[tauri::command]
pub(crate) fn git_changed_files(state: State<'_, AppState>) -> Result<Vec<GitChangedFile>, String> {
    crate::workspace::git_changed_files(state.inner())
}

#[tauri::command]
pub(crate) fn workspace_state(state: State<'_, AppState>) -> WorkspaceState {
    crate::workspace::workspace_state(state.inner())
}
