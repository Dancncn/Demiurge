//! Runtime state assembled by the desktop starter.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use tokio::sync::oneshot;

use crate::permission::{PermissionResponse, PermissionRule};
use crate::store::{self, SessionStore, Settings};
use crate::{agent, mcp, ocr, pomodoro, tools};

/// 全局共享状态。路径类字段在 setup() 里填充（需要 AppHandle 才能拿到 app_data_dir）。
pub struct AppState {
    /// 共享 HTTP 客户端（复用连接池 + TLS 会话）
    pub http: reqwest::Client,
    pub settings: Mutex<Settings>,
    /// 多会话 + 当前活动会话
    pub sessions: Mutex<SessionStore>,
    /// 待确认的工具调用：id -> oneshot 发送端
    pub pending_confirms: Mutex<HashMap<String, oneshot::Sender<PermissionResponse>>>,
    /// 会话权限规则：session id -> (tool -> rule)。
    /// 每个会话拥有独立规则桶，切换活动会话不会继承其他会话的记忆授权。
    pub session_permission_rules: Mutex<HashMap<String, HashMap<String, PermissionRule>>>,
    /// 串行权限规则与审计文件的读取、更新和原子替换，防止丢更新或读到半写文件。
    pub permission_store_lock: Mutex<()>,
    /// 使用量 JSONL 的追加锁；统计从该日志重放，不参与主循环决策。
    pub usage_log_lock: Mutex<()>,
    /// Provider-local model circuit breakers. Routing state is process-local;
    /// the durable audit log remains the source of truth for requests.
    pub model_route_health: Mutex<HashMap<String, crate::model_routing::RouteHealth>>,
    /// 当前计划模式的计划文件状态。
    pub plan_state: Mutex<PlanState>,
    /// 本进程内最近 edit_file 修改记录，用于 undo_edit 安全撤销
    pub edit_undo_stack: Mutex<Vec<tools::EditUndoEntry>>,
    pub workflow_runs: Mutex<Vec<agent::workflow_runtime::WorkflowRunProgress>>,
    pub workflow_cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub pomodoro: Mutex<pomodoro::PomodoroRuntime>,
    pub session_engine: Mutex<agent::session_engine::SessionEngineState>,
    pub mcp: mcp::McpManager,
    /// 用户中断标志
    pub cancel: AtomicBool,
    /// 是否正在处理一轮对话（防止并发 send）
    pub busy: AtomicBool,
    pub data_dir: Mutex<PathBuf>,
    pub sandbox_dir: Mutex<PathBuf>,
    pub packs_dir: Mutex<PathBuf>,
    pub pets_dir: Mutex<PathBuf>,
    pub ocr: ocr::OcrState,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct PlanState {
    pub active: bool,
    pub approved: bool,
    pub path: Option<String>,
    pub content: Option<String>,
    pub created_at: Option<u64>,
    pub approved_at: Option<u64>,
}

/// A captured store and its ordering token travel together to the blocking writer.
struct SessionPersistenceSnapshot {
    dir: PathBuf,
    store: SessionStore,
    sequence: u64,
}

impl SessionPersistenceSnapshot {
    /// Returns false when a newer snapshot has already reached this directory.
    fn write(&self) -> Result<bool, String> {
        static WRITTEN: OnceLock<Mutex<HashMap<PathBuf, u64>>> = OnceLock::new();
        let written = WRITTEN.get_or_init(|| Mutex::new(HashMap::new()));
        let mut map = written.lock().unwrap_or_else(|e| e.into_inner());
        let last = map.entry(self.dir.clone()).or_insert(0);
        if *last >= self.sequence {
            return Ok(false);
        }
        store::save_sessions(&self.dir, &self.store)?;
        *last = self.sequence;
        Ok(true)
    }
}

impl PlanState {
    pub fn reset(&mut self) {
        *self = PlanState::default();
    }
}

impl AppState {
    pub(crate) fn new(http: reqwest::Client) -> Self {
        AppState {
            http,
            settings: Mutex::new(Settings::default()),
            sessions: Mutex::new(SessionStore::default()),
            pending_confirms: Mutex::new(HashMap::new()),
            session_permission_rules: Mutex::new(HashMap::new()),
            permission_store_lock: Mutex::new(()),
            usage_log_lock: Mutex::new(()),
            model_route_health: Mutex::new(HashMap::new()),
            plan_state: Mutex::new(PlanState::default()),
            edit_undo_stack: Mutex::new(Vec::new()),
            workflow_runs: Mutex::new(Vec::new()),
            workflow_cancels: Mutex::new(HashMap::new()),
            pomodoro: Mutex::new(pomodoro::PomodoroRuntime::default()),
            session_engine: Mutex::new(agent::session_engine::SessionEngineState::default()),
            mcp: mcp::McpManager::default(),
            cancel: AtomicBool::new(false),
            busy: AtomicBool::new(false),
            data_dir: Mutex::new(PathBuf::new()),
            sandbox_dir: Mutex::new(PathBuf::new()),
            packs_dir: Mutex::new(PathBuf::new()),
            pets_dir: Mutex::new(PathBuf::new()),
            ocr: ocr::OcrState::default(),
        }
    }

    /// 落盘当前会话集合。
    ///
    /// 写盘移出调用方（多为 async runner）的任务：克隆快照后交后台线程落盘，避免在
    /// 流式 / 多步工具回合的关键路径上做同步磁盘写（长历史下整库 JSON 序列化不便宜）。
    /// 用全局单调序号 + 按目录记录「已落盘的最大序号」，保证后产生的快照不会被先完成的
    /// 旧线程覆盖；按目录隔离，单进程多数据目录（含并行测试）也不会互相串号。
    /// 返回不代表落盘完成；退出前尚未提交的快照仍可能丢失，当前没有关闭 flush。
    pub fn persist_sessions(&self) {
        let snapshot = self.capture_sessions_snapshot(|| {});
        // spawn_blocking 复用 Tauri/tokio 阻塞线程池，避免每次落盘新建 OS 线程；
        // 仍在 async 关键路径之外执行同步磁盘写。
        tauri::async_runtime::spawn_blocking(move || {
            if let Err(error) = snapshot.write() {
                eprintln!(
                    "Failed to persist sessions in {}: {error}",
                    snapshot.dir.display()
                );
            }
        });
    }

    fn capture_sessions_snapshot(
        &self,
        after_capture: impl FnOnce(),
    ) -> SessionPersistenceSnapshot {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let dir = self.data_dir.lock().unwrap().clone();
        // Allocate the order while the store is locked: a newer store must never
        // receive an older sequence when the caller is preempted after cloning.
        let (store, sequence) = {
            let sessions = self.sessions.lock().unwrap();
            let store = sessions.clone();
            let sequence = SEQ.fetch_add(1, Ordering::SeqCst) + 1;
            (store, sequence)
        };
        // This handoff seam also lets tests delay a captured snapshot without
        // holding the sessions lock or relying on probabilistic thread timing.
        after_capture();
        SessionPersistenceSnapshot {
            dir,
            store,
            sequence,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::mpsc;
    use std::time::Duration;

    fn persistence_fixture() -> (PathBuf, Arc<AppState>) {
        let root = std::env::temp_dir().join(format!(
            "demiurge_session_persistence_{}",
            store::new_session_id()
        ));
        fs::create_dir_all(&root).unwrap();
        let state = Arc::new(AppState::new(reqwest::Client::new()));
        *state.data_dir.lock().unwrap() = root.clone();
        let mut session = store::Session::new();
        session.title = "older".into();
        *state.sessions.lock().unwrap() = SessionStore {
            active: session.id.clone(),
            sessions: vec![session],
        };
        (root, state)
    }

    #[test]
    fn session_persistence_delayed_capture_cannot_overwrite_newer_store() {
        let (root, state) = persistence_fixture();
        let (captured_tx, captured_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let older_state = Arc::clone(&state);
        let older = std::thread::spawn(move || {
            older_state.capture_sessions_snapshot(|| {
                captured_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            })
        });
        captured_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        state.sessions.lock().unwrap().sessions[0].title = "newer".into();
        let newer = state.capture_sessions_snapshot(|| {});
        assert!(newer.write().unwrap());
        release_tx.send(()).unwrap();
        let older = older.join().unwrap();
        assert!(!older.write().unwrap());
        let loaded = store::load_sessions(&root);
        assert_eq!(loaded.sessions[0].title, "newer");
        assert!(!store::backup_path(&root.join("sessions.json")).exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn session_persistence_failed_write_can_retry_the_same_snapshot() {
        let (root, state) = persistence_fixture();
        let blocked_dir = root.join("blocked");
        fs::write(&blocked_dir, "not a directory").unwrap();
        *state.data_dir.lock().unwrap() = blocked_dir.clone();
        let snapshot = state.capture_sessions_snapshot(|| {});
        assert!(snapshot.write().is_err());
        fs::remove_file(&blocked_dir).unwrap();
        assert!(snapshot.write().unwrap());
        assert_eq!(
            store::load_sessions(&blocked_dir).sessions[0].title,
            "older"
        );
        assert!(!snapshot.write().unwrap());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn session_persistence_stale_write_preserves_backup_for_corruption_recovery() {
        let (root, state) = persistence_fixture();
        let older = state.capture_sessions_snapshot(|| {});
        assert!(older.write().unwrap());
        state.sessions.lock().unwrap().sessions[0].title = "newer".into();
        assert!(state.capture_sessions_snapshot(|| {}).write().unwrap());
        let primary = root.join("sessions.json");
        let backup = store::backup_path(&primary);
        let backup_bytes = fs::read(&backup).unwrap();
        assert!(!older.write().unwrap());
        assert_eq!(fs::read(&backup).unwrap(), backup_bytes);
        assert_eq!(store::load_sessions(&root).sessions[0].title, "newer");

        fs::write(&primary, "broken primary").unwrap();
        assert_eq!(store::load_sessions(&root).sessions[0].title, "older");
        let repaired: SessionStore = serde_json::from_slice(&fs::read(&primary).unwrap()).unwrap();
        assert_eq!(repaired.sessions[0].title, "older");
        assert_eq!(fs::read(&backup).unwrap(), backup_bytes);
        let preserved: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("sessions.json.corrupt-")
            })
            .collect();
        assert_eq!(preserved.len(), 1);
        assert_eq!(fs::read_to_string(&preserved[0]).unwrap(), "broken primary");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn session_persistence_orders_each_data_directory_independently() {
        let (root, state) = persistence_fixture();
        let first_dir = root.join("first");
        let second_dir = root.join("second");
        *state.data_dir.lock().unwrap() = first_dir.clone();
        let first = state.capture_sessions_snapshot(|| {});
        *state.data_dir.lock().unwrap() = second_dir.clone();
        state.sessions.lock().unwrap().sessions[0].title = "second".into();
        let second = state.capture_sessions_snapshot(|| {});
        assert!(second.write().unwrap());
        assert!(first.write().unwrap());
        assert_eq!(store::load_sessions(&first_dir).sessions[0].title, "older");
        assert_eq!(
            store::load_sessions(&second_dir).sessions[0].title,
            "second"
        );
        fs::remove_dir_all(&root).unwrap();
    }
}
