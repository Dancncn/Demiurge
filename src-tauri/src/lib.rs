//! Demiurge 引擎 —— Tauri v2 入口：全局状态、命令、构建器。
mod agent;
mod llm;
mod pack;
mod permission;
mod store;
mod tools;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tokio::sync::oneshot;

use agent::conversation::Message;
use permission::{PermissionResponse, PermissionRule};
use store::{Session, SessionMeta, SessionStore, Settings};

/// 全局共享状态。路径类字段在 setup() 里填充（需要 AppHandle 才能拿到 app_data_dir）。
pub struct AppState {
    /// 共享 HTTP 客户端（复用连接池 + TLS 会话）
    pub http: reqwest::Client,
    pub settings: Mutex<Settings>,
    /// 多会话 + 当前活动会话
    pub sessions: Mutex<SessionStore>,
    /// 待确认的工具调用：id -> oneshot 发送端
    pub pending_confirms: Mutex<HashMap<String, oneshot::Sender<PermissionResponse>>>,
    /// 本会话内的权限规则：tool -> rule
    pub session_permission_rules: Mutex<HashMap<String, PermissionRule>>,
    /// 用户中断标志
    pub cancel: AtomicBool,
    /// 是否正在处理一轮对话（防止并发 send）
    pub busy: AtomicBool,
    pub data_dir: Mutex<PathBuf>,
    pub sandbox_dir: Mutex<PathBuf>,
    pub packs_dir: Mutex<PathBuf>,
}

impl AppState {
    fn new(http: reqwest::Client) -> Self {
        AppState {
            http,
            settings: Mutex::new(Settings::default()),
            sessions: Mutex::new(SessionStore::default()),
            pending_confirms: Mutex::new(HashMap::new()),
            session_permission_rules: Mutex::new(HashMap::new()),
            cancel: AtomicBool::new(false),
            busy: AtomicBool::new(false),
            data_dir: Mutex::new(PathBuf::new()),
            sandbox_dir: Mutex::new(PathBuf::new()),
            packs_dir: Mutex::new(PathBuf::new()),
        }
    }

    /// 落盘当前会话集合。
    pub fn persist_sessions(&self) {
        let dir = self.data_dir.lock().unwrap().clone();
        let store = self.sessions.lock().unwrap().clone();
        let _ = store::save_sessions(&dir, &store);
    }
}

#[derive(Serialize)]
struct SessionList {
    active: String,
    sessions: Vec<SessionMeta>,
}

fn session_list(store: &SessionStore) -> SessionList {
    let mut metas: Vec<SessionMeta> = store
        .sessions
        .iter()
        .map(|s| SessionMeta {
            id: s.id.clone(),
            title: s.title.clone(),
            updated_at: s.updated_at,
        })
        .collect();
    metas.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    SessionList {
        active: store.active.clone(),
        sessions: metas,
    }
}

// ---------------- Tauri 命令 ----------------

/// 发送一条用户消息，跑完整轮 Agent 循环；过程通过事件流推给前端。
#[tauri::command]
async fn send(app: AppHandle, state: State<'_, AppState>, text: String) -> Result<(), String> {
    let st = state.inner();
    if st.busy.swap(true, Ordering::SeqCst) {
        return Err("正在处理上一条消息，请稍候。".to_string());
    }
    let res = agent::run_turn(&app, st, text).await;
    st.busy.store(false, Ordering::SeqCst);
    res
}

/// 中断当前流式生成。
#[tauri::command]
fn interrupt(state: State<'_, AppState>) {
    state.cancel.store(true, Ordering::Relaxed);
    // 立即唤醒所有正在等待的确认（按「中断」处理），否则确认弹窗的 await 会把整轮卡住最长 5 分钟
    let mut pending = state.pending_confirms.lock().unwrap();
    for (_, tx) in pending.drain() {
        let _ = tx.send(PermissionResponse::deny_once());
    }
}

/// 前端确认对话框的回执：取出对应 oneshot 发送端回填裁决。
#[tauri::command]
fn respond_confirm(
    state: State<'_, AppState>,
    id: String,
    allow: bool,
    scope: tools::PermissionScope,
) {
    if let Some(tx) = state.pending_confirms.lock().unwrap().remove(&id) {
        let _ = tx.send(PermissionResponse { allow, scope });
    }
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    *state.settings.lock().unwrap() = settings.clone();
    let dir = state.data_dir.lock().unwrap().clone();
    store::save_settings(&dir, &settings)
}

#[tauri::command]
fn list_packs(state: State<'_, AppState>) -> Vec<pack::PackManifest> {
    let dir = state.packs_dir.lock().unwrap().clone();
    pack::list_packs(&dir)
}

// ---- 会话管理 ----

#[tauri::command]
fn list_sessions(state: State<'_, AppState>) -> SessionList {
    session_list(&state.sessions.lock().unwrap())
}

/// 当前活动会话的消息历史。
#[tauri::command]
fn get_history(state: State<'_, AppState>) -> Vec<Message> {
    let store = state.sessions.lock().unwrap();
    store
        .get(&store.active)
        .map(|s| s.messages.clone())
        .unwrap_or_default()
}

/// 新建会话并设为活动，返回新会话 id。
#[tauri::command]
fn new_session(state: State<'_, AppState>) -> String {
    let id = {
        let mut store = state.sessions.lock().unwrap();
        let sess = Session::new();
        let id = sess.id.clone();
        store.sessions.push(sess);
        store.active = id.clone();
        id
    };
    state.persist_sessions();
    id
}

/// 切换活动会话。
#[tauri::command]
fn select_session(state: State<'_, AppState>, id: String) {
    {
        let mut store = state.sessions.lock().unwrap();
        if store.sessions.iter().any(|s| s.id == id) {
            store.active = id;
        }
    }
    state.persist_sessions();
}

/// 删除会话；若删的是活动会话，切到最近一个（或新建空会话）。返回新的活动会话 id。
#[tauri::command]
fn delete_session(state: State<'_, AppState>, id: String) -> String {
    let active = {
        let mut store = state.sessions.lock().unwrap();
        store.sessions.retain(|s| s.id != id);
        if store.active == id {
            // 切到最近更新的会话
            store.active = store
                .sessions
                .iter()
                .max_by_key(|s| s.updated_at)
                .map(|s| s.id.clone())
                .unwrap_or_default();
        }
        store.ensure_one();
        store.active.clone()
    };
    state.persist_sessions();
    active
}

/// 打开沙盒目录（方便用户放/取文件）。
#[tauri::command]
fn open_sandbox(state: State<'_, AppState>) -> Result<(), String> {
    let dir = state.sandbox_dir.lock().unwrap().clone();
    tools::execute_open(&dir.to_string_lossy()).map(|_| ())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .expect("failed to build reqwest::Client");

    tauri::Builder::default()
        .manage(AppState::new(http))
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let sandbox = dir.join("sandbox");
            std::fs::create_dir_all(&sandbox)?;
            let packs = dir.join("packs");
            std::fs::create_dir_all(&packs)?;
            pack::ensure_default(&packs)?;

            let settings = store::load_settings(&dir);
            let sessions = store::load_sessions(&dir);

            let state = app.state::<AppState>();
            *state.data_dir.lock().unwrap() = dir;
            *state.sandbox_dir.lock().unwrap() = sandbox;
            *state.packs_dir.lock().unwrap() = packs;
            *state.settings.lock().unwrap() = settings;
            *state.sessions.lock().unwrap() = sessions;
            // 保证落盘一次（迁移/初始化后）
            state.persist_sessions();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            send,
            interrupt,
            respond_confirm,
            get_settings,
            save_settings,
            list_packs,
            list_sessions,
            get_history,
            new_session,
            select_session,
            delete_session,
            open_sandbox,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
