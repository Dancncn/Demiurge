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
            ocr: ocr::OcrState::default(),
        }
    }

    /// 落盘当前会话集合。
    ///
    /// 写盘移出调用方（多为 async runner）的任务：克隆快照后交后台线程落盘，避免在
    /// 流式 / 多步工具回合的关键路径上做同步磁盘写（长历史下整库 JSON 序列化不便宜）。
    /// 用全局单调序号 + 按目录记录「已落盘的最大序号」，保证后产生的快照不会被先完成的
    /// 旧线程覆盖；按目录隔离，单进程多数据目录（含并行测试）也不会互相串号。
    /// 代价：硬退出时最后一次写入可能丢失（毫秒级窗口），对桌面伴侣可接受。
    pub fn persist_sessions(&self) {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        static WRITTEN: OnceLock<Mutex<HashMap<PathBuf, u64>>> = OnceLock::new();

        let dir = self.data_dir.lock().unwrap().clone();
        let store = self.sessions.lock().unwrap().clone();
        let seq = SEQ.fetch_add(1, Ordering::SeqCst) + 1;
        // spawn_blocking 复用 Tauri/tokio 阻塞线程池，避免每次落盘新建 OS 线程；
        // 仍在 async 关键路径之外执行同步磁盘写。
        tauri::async_runtime::spawn_blocking(move || {
            let written = WRITTEN.get_or_init(|| Mutex::new(HashMap::new()));
            let mut map = written.lock().unwrap_or_else(|e| e.into_inner());
            let last = map.entry(dir.clone()).or_insert(0);
            if *last >= seq {
                return; // 已有更新的快照落盘，跳过这次旧数据写入
            }
            match store::save_sessions(&dir, &store) {
                Ok(()) => *last = seq,
                Err(error) => {
                    eprintln!("Failed to persist sessions in {}: {error}", dir.display());
                }
            }
        });
    }
}
