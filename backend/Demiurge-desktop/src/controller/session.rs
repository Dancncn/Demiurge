//! Session IPC Controller。
//!
//! 函数名和参数保持与现有前端 invoke 契约一致。

use serde::Serialize;
use tauri::State;

use crate::agent::conversation::HistoryMessage;
use crate::biz::session::{NavigationSnapshotBo, SessionBiz, SessionListBo, SessionSummaryBo};
use crate::{agent, store, workspace, AppState};

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SessionSummaryResDto {
    id: String,
    title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    workspace_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace_name: Option<String>,
    updated_at: u64,
    archived: bool,
    archived_at: Option<u64>,
}

impl From<SessionSummaryBo> for SessionSummaryResDto {
    fn from(value: SessionSummaryBo) -> Self {
        Self {
            id: value.id,
            title: value.title,
            workspace_path: value.workspace_path,
            workspace_name: value.workspace_name,
            updated_at: value.updated_at,
            archived: value.archived,
            archived_at: value.archived_at,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SessionListResDto {
    active: String,
    sessions: Vec<SessionSummaryResDto>,
}

impl From<SessionListBo> for SessionListResDto {
    fn from(value: SessionListBo) -> Self {
        Self {
            active: value.active,
            sessions: value.sessions.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct NavigationSnapshotResDto {
    session_id: String,
    sessions: Vec<SessionSummaryResDto>,
    history: Vec<HistoryMessage>,
    workspace: workspace::WorkspaceState,
    goal: Option<agent::goal::GoalPanelState>,
}

impl From<NavigationSnapshotBo> for NavigationSnapshotResDto {
    fn from(value: NavigationSnapshotBo) -> Self {
        Self {
            session_id: value.session_id,
            sessions: value.sessions.into_iter().map(Into::into).collect(),
            history: value.history,
            workspace: value.workspace,
            goal: value.goal,
        }
    }
}

#[tauri::command]
pub(crate) fn list_sessions(state: State<'_, AppState>) -> SessionListResDto {
    SessionBiz::list(state.inner()).into()
}

#[tauri::command]
pub(crate) fn session_stats(state: State<'_, AppState>, offset: i64) -> store::StatsPanel {
    SessionBiz::stats(state.inner(), offset)
}

#[tauri::command]
pub(crate) fn get_history(state: State<'_, AppState>) -> Vec<HistoryMessage> {
    SessionBiz::history(state.inner())
}

#[tauri::command]
pub(crate) fn navigation_snapshot(
    state: State<'_, AppState>,
    expected_session_id: Option<String>,
) -> Result<NavigationSnapshotResDto, String> {
    SessionBiz::navigation_snapshot(state.inner(), expected_session_id.as_deref()).map(Into::into)
}

#[tauri::command]
pub(crate) fn new_session(state: State<'_, AppState>) -> Result<NavigationSnapshotResDto, String> {
    SessionBiz::create(state.inner()).map(Into::into)
}

#[tauri::command]
pub(crate) fn select_session(
    state: State<'_, AppState>,
    id: String,
) -> Result<NavigationSnapshotResDto, String> {
    SessionBiz::select(state.inner(), id).map(Into::into)
}

#[tauri::command]
pub(crate) fn delete_session(
    state: State<'_, AppState>,
    id: String,
) -> Result<NavigationSnapshotResDto, String> {
    SessionBiz::delete(state.inner(), id).map(Into::into)
}

#[tauri::command]
pub(crate) fn rename_session(
    state: State<'_, AppState>,
    id: String,
    title: String,
) -> Result<String, String> {
    SessionBiz::rename(state.inner(), id, title)
}

#[tauri::command]
pub(crate) fn set_session_archived(
    state: State<'_, AppState>,
    id: String,
    archived: bool,
) -> Result<(), String> {
    SessionBiz::set_archived(state.inner(), id, archived)
}

#[cfg(test)]
mod tests {
    use crate::biz::session::{SessionListBo, SessionSummaryBo};

    use super::SessionListResDto;

    #[test]
    fn session_list_response_preserves_optional_field_serialization() {
        let response = SessionListResDto::from(SessionListBo {
            active: "session-a".to_string(),
            sessions: vec![SessionSummaryBo {
                id: "session-a".to_string(),
                title: "New session".to_string(),
                workspace_path: String::new(),
                workspace_name: None,
                updated_at: 42,
                archived: false,
                archived_at: None,
            }],
        });

        let value = serde_json::to_value(response).unwrap();
        let session = &value["sessions"][0];
        assert_eq!(session["id"], "session-a");
        assert!(session.get("workspace_path").is_none());
        assert!(session.get("workspace_name").is_none());
    }
}
