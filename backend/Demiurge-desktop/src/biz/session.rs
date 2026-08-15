//! 会话管理用例。
//!
//! 这里是会话入口当前的深模块：调用方不需要了解活动会话切换、
//! Workspace 同步、删除保护、权限清理和失败回滚的顺序。

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use crate::agent::conversation::HistoryMessage;
use crate::store::{self, Session};
use crate::{agent, permission, workspace, AppState};

#[derive(Clone, Debug)]
pub(crate) struct SessionSummaryBo {
    pub id: String,
    pub title: String,
    pub workspace_path: String,
    pub workspace_name: Option<String>,
    pub updated_at: u64,
    pub archived: bool,
    pub archived_at: Option<u64>,
}

#[derive(Clone, Debug)]
pub(crate) struct SessionListBo {
    pub active: String,
    pub sessions: Vec<SessionSummaryBo>,
}

#[derive(Clone, Debug)]
pub(crate) struct NavigationSnapshotBo {
    pub session_id: String,
    pub sessions: Vec<SessionSummaryBo>,
    pub history: Vec<HistoryMessage>,
    pub workspace: workspace::WorkspaceState,
    pub goal: Option<agent::goal::GoalPanelState>,
}

/// 会话查询与生命周期用例。
///
/// 不引入 trait：当前只有桌面本地实现，先通过这个真实接缝集中行为；
/// 当出现第二种实现或独立 Core crate 后再抽象接口。
pub(crate) struct SessionBiz;

impl SessionBiz {
    pub fn list(state: &AppState) -> SessionListBo {
        session_list(&state.sessions.lock().unwrap())
    }

    pub fn stats(state: &AppState, offset: i64) -> store::StatsPanel {
        let model = state.settings.lock().unwrap().model.clone();
        let sessions = state.sessions.lock().unwrap();
        store::compute_stats(&sessions, offset, model)
    }

    pub fn history(state: &AppState) -> Vec<HistoryMessage> {
        let sessions = state.sessions.lock().unwrap();
        sessions
            .get(&sessions.active)
            .map(|session| session.messages.iter().map(HistoryMessage::from).collect())
            .unwrap_or_default()
    }

    pub fn navigation_snapshot(
        state: &AppState,
        expected_session_id: Option<&str>,
    ) -> Result<NavigationSnapshotBo, String> {
        navigation_snapshot_for_state(state, expected_session_id)
    }

    pub fn create(state: &AppState) -> Result<NavigationSnapshotBo, String> {
        let workspace_path = state
            .sandbox_dir
            .lock()
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let id = {
            let mut sessions = state.sessions.lock().unwrap();
            let mut session = Session::new();
            session.workspace_path = workspace_path;
            let id = session.id.clone();
            sessions.sessions.push(session);
            sessions.active = id.clone();
            id
        };
        state.persist_sessions();
        navigation_snapshot_for_state(state, Some(&id))
    }

    pub fn select(state: &AppState, id: String) -> Result<NavigationSnapshotBo, String> {
        let configured = {
            let sessions = state.sessions.lock().unwrap();
            sessions
                .get(&id)
                .map(|session| session.workspace_path.clone())
                .ok_or_else(|| "会话不存在".to_string())?
        };

        // 与 turn 启动使用相同的锁顺序，把活动会话与 Workspace 切换作为一个事务。
        let runtime = state.session_engine.lock().unwrap();
        workspace::ensure_session_workspace_switch_allowed(state, &configured)?;
        let previous_active = {
            let mut sessions = state.sessions.lock().unwrap();
            let previous = sessions.active.clone();
            sessions.active = id.clone();
            previous
        };
        if let Err(error) = workspace::sync_active_session_workspace(state) {
            state.sessions.lock().unwrap().active = previous_active;
            let rollback = workspace::sync_active_session_workspace(state);
            return Err(match rollback {
                Ok(()) => error,
                Err(rollback_error) => {
                    format!("{error}；恢复原会话工作区失败：{rollback_error}")
                }
            });
        }
        state.persist_sessions();
        drop(runtime);
        navigation_snapshot_for_state(state, Some(&id))
    }

    pub fn delete(state: &AppState, id: String) -> Result<NavigationSnapshotBo, String> {
        let active = delete_session(state, id)?;
        navigation_snapshot_for_state(state, Some(&active))
    }

    pub fn rename(state: &AppState, id: String, title: String) -> Result<String, String> {
        let clean = normalize_title(&title)?;
        {
            let mut sessions = state.sessions.lock().unwrap();
            let session = sessions
                .get_mut(&id)
                .ok_or_else(|| "会话不存在".to_string())?;
            session.title = clean.clone();
            session.updated_at = store::now_millis();
        }
        state.persist_sessions();
        Ok(clean)
    }

    pub fn set_archived(state: &AppState, id: String, archived: bool) -> Result<(), String> {
        let mut sessions = state.sessions.lock().unwrap();
        let session = sessions
            .get_mut(&id)
            .ok_or_else(|| "会话不存在".to_string())?;
        session.archived = archived;
        session.archived_at = archived.then(store::now_millis);
        drop(sessions);
        state.persist_sessions();
        Ok(())
    }
}

fn normalize_title(title: &str) -> Result<String, String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err("会话标题不能为空".to_string());
    }
    Ok(trimmed.chars().take(80).collect())
}

fn session_list(store: &store::SessionStore) -> SessionListBo {
    let mut sessions: Vec<SessionSummaryBo> = store
        .sessions
        .iter()
        .map(|session| SessionSummaryBo {
            id: session.id.clone(),
            title: session.title.clone(),
            workspace_path: session.workspace_path.clone(),
            workspace_name: Path::new(&session.workspace_path)
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string),
            updated_at: session.updated_at,
            archived: session.archived,
            archived_at: session.archived_at,
        })
        .collect();
    sessions.sort_by(|a, b| {
        a.archived
            .cmp(&b.archived)
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });
    SessionListBo {
        active: store.active.clone(),
        sessions,
    }
}

fn navigation_snapshot_for_state(
    state: &AppState,
    expected_session_id: Option<&str>,
) -> Result<NavigationSnapshotBo, String> {
    let (session_id, sessions, history, workspace_path, goal) = {
        let store = state.sessions.lock().unwrap();
        if let Some(expected) = expected_session_id {
            if store.active != expected {
                return Err(format!(
                    "活动会话已改变：期望 `{expected}`，实际 `{}`",
                    store.active
                ));
            }
        }
        let session = store
            .get(&store.active)
            .ok_or_else(|| "当前活动会话不存在".to_string())?;
        let list = session_list(&store);
        (
            session.id.clone(),
            list.sessions,
            session.messages.iter().map(HistoryMessage::from).collect(),
            session.workspace_path.clone(),
            session.goal.clone(),
        )
    };

    // Workspace 必须来自与历史和目标同一个会话快照，不能在释放锁后读取全局 sandbox。
    let workspace_root = if workspace_path.trim().is_empty() {
        state.sandbox_dir.lock().unwrap().clone()
    } else {
        PathBuf::from(workspace_path)
    };
    Ok(NavigationSnapshotBo {
        session_id,
        sessions,
        history,
        workspace: workspace::inspect_workspace(&workspace_root),
        goal: goal.as_ref().map(agent::goal::panel_state_from_goal),
    })
}

fn delete_session(state: &AppState, id: String) -> Result<String, String> {
    let current_workspace = state
        .sandbox_dir
        .lock()
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    // 保持 engine 锁直到删除结束；begin_turn 使用相同锁顺序，校验和删除因此是原子的。
    let runtime = state.session_engine.lock().unwrap();
    if agent::session_engine::blocks_session_deletion(&runtime, &id) {
        return Err("正在生成回复，暂时不能删除本轮所属会话".to_string());
    }
    let (active, previous_store) = {
        let mut sessions = state.sessions.lock().unwrap();
        if !sessions.sessions.iter().any(|session| session.id == id) {
            return Err("会话不存在".to_string());
        }
        if sessions.active == id && state.busy.load(Ordering::Acquire) {
            return Err("正在生成回复，暂时不能删除当前会话".to_string());
        }
        let previous = sessions.clone();
        sessions.sessions.retain(|session| session.id != id);
        if sessions.active == id {
            sessions.active = sessions
                .sessions
                .iter()
                .max_by_key(|session| session.updated_at)
                .map(|session| session.id.clone())
                .unwrap_or_default();
        }
        if sessions.sessions.is_empty() {
            let mut session = Session::new();
            session.workspace_path = current_workspace;
            sessions.active = session.id.clone();
            sessions.sessions.push(session);
        } else {
            sessions.ensure_one();
        }
        (sessions.active.clone(), previous)
    };
    if let Err(error) = workspace::sync_active_session_workspace(state) {
        *state.sessions.lock().unwrap() = previous_store;
        let rollback = workspace::sync_active_session_workspace(state);
        return Err(match rollback {
            Ok(()) => error,
            Err(rollback_error) => {
                format!("{error}；恢复删除前会话工作区失败：{rollback_error}")
            }
        });
    }
    permission::clear_session_rules(state, &id);
    state.persist_sessions();
    drop(runtime);
    Ok(active)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::agent::conversation::Message;
    use crate::store::{self, Session};
    use crate::{agent, AppState};

    use super::{normalize_title, SessionBiz};

    #[test]
    fn snapshot_binds_history_workspace_and_goal_to_one_session() {
        let root = std::env::temp_dir().join(format!(
            "demiurge_navigation_snapshot_{}",
            store::new_session_id()
        ));
        let workspace_a = root.join("workspace-a");
        let workspace_b = root.join("workspace-b");
        fs::create_dir_all(&workspace_a).unwrap();
        fs::create_dir_all(&workspace_b).unwrap();

        let state = AppState::new(reqwest::Client::new());
        *state.data_dir.lock().unwrap() = root.clone();
        // 全局 sandbox 故意指向 B；A 的快照仍必须读取 A 自己的 Workspace。
        *state.sandbox_dir.lock().unwrap() = workspace_b.clone();

        let mut session_a = Session::new();
        session_a.id = "session-a".to_string();
        session_a.workspace_path = workspace_a.to_string_lossy().to_string();
        session_a.messages.push(Message::user("history-a"));
        let mut session_b = Session::new();
        session_b.id = "session-b".to_string();
        session_b.workspace_path = workspace_b.to_string_lossy().to_string();
        session_b.messages.push(Message::user("history-b"));
        {
            let mut sessions = state.sessions.lock().unwrap();
            sessions.active = session_a.id.clone();
            sessions.sessions = vec![session_a, session_b];
        }
        agent::goal::set_goal_for_session(&state, "session-a", "goal-a".to_string(), None);

        let snapshot = SessionBiz::navigation_snapshot(&state, Some("session-a")).unwrap();
        assert_eq!(snapshot.session_id, "session-a");
        assert_eq!(snapshot.history.len(), 1);
        assert_eq!(snapshot.history[0].content.as_deref(), Some("history-a"));
        assert_eq!(snapshot.workspace.name, "workspace-a");
        assert_eq!(
            snapshot.goal.as_ref().map(|goal| goal.objective.as_str()),
            Some("goal-a")
        );

        state.sessions.lock().unwrap().active = "session-b".to_string();
        assert!(SessionBiz::navigation_snapshot(&state, Some("session-a")).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rename_trims_and_limits_the_title() {
        let title = format!("  {}  ", "a".repeat(100));

        let result = normalize_title(&title).unwrap();

        assert_eq!(result.chars().count(), 80);
        assert!(normalize_title("   ").is_err());
    }
}
