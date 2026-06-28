//! Demiurge 引擎 —— Tauri v2 入口：全局状态、命令、构建器。
mod agent;
mod credentials;
mod llm;
mod ocr;
mod pack;
mod permission;
mod store;
mod tools;
mod voice;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
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
    /// 本进程内最近 edit_file 修改记录，用于 undo_edit 安全撤销
    pub edit_undo_stack: Mutex<Vec<tools::EditUndoEntry>>,
    pub workflow_runs: Mutex<Vec<agent::workflow_runtime::WorkflowRunProgress>>,
    pub workflow_cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// 用户中断标志
    pub cancel: AtomicBool,
    /// 是否正在处理一轮对话（防止并发 send）
    pub busy: AtomicBool,
    pub data_dir: Mutex<PathBuf>,
    pub sandbox_dir: Mutex<PathBuf>,
    pub packs_dir: Mutex<PathBuf>,
    pub ocr: ocr::OcrState,
}

impl AppState {
    fn new(http: reqwest::Client) -> Self {
        AppState {
            http,
            settings: Mutex::new(Settings::default()),
            sessions: Mutex::new(SessionStore::default()),
            pending_confirms: Mutex::new(HashMap::new()),
            session_permission_rules: Mutex::new(HashMap::new()),
            edit_undo_stack: Mutex::new(Vec::new()),
            workflow_runs: Mutex::new(Vec::new()),
            workflow_cancels: Mutex::new(HashMap::new()),
            cancel: AtomicBool::new(false),
            busy: AtomicBool::new(false),
            data_dir: Mutex::new(PathBuf::new()),
            sandbox_dir: Mutex::new(PathBuf::new()),
            packs_dir: Mutex::new(PathBuf::new()),
            ocr: ocr::OcrState::default(),
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
    let trimmed = text.trim();
    let res = if trimmed == "/dream" || trimmed.starts_with("/dream ") {
        agent::dream::run_manual_dream(&app, st, text).await
    } else if trimmed == "/compact" || trimmed.starts_with("/compact ") {
        agent::collapse::run_manual_compact(&app, st, text).await
    } else if trimmed == "/workflows" {
        let runs = agent::workflow_journal::list(st);
        let body = if runs.is_empty() {
            "暂无 workflow journal。使用 /ultracode <任务> 会自动创建 run。".to_string()
        } else {
            let mut out = String::from("Workflow runs:\n");
            for run in runs.iter().take(20) {
                out.push_str(&format!(
                    "- `{}` updated_at={} journal={}\n",
                    run.run_id, run.updated_at, run.journal_path
                ));
            }
            out
        };
        let _ = app.emit("assistant-done", body);
        Ok(())
    } else if trimmed.starts_with("/workflow resume ") {
        let run_id = trimmed
            .trim_start_matches("/workflow resume ")
            .trim()
            .to_string();
        let overlay = agent::workflow_journal::resume_overlay(st, &run_id)?;
        agent::run_turn_with_options(
            &app,
            st,
            text,
            agent::TurnOptions {
                system_overlay: Some(overlay),
                stored_user_text: None,
                workflow_run_id: Some(run_id),
            },
        )
        .await
    } else if trimmed == "/ultracode" || trimmed.starts_with("/ultracode ") {
        let task = trimmed
            .strip_prefix("/ultracode")
            .unwrap_or("")
            .trim()
            .to_string();
        let run_id = agent::workflow_journal::new_run_id();
        let overlay = agent::ultracode::overlay(&task);
        agent::run_turn_with_options(
            &app,
            st,
            text,
            agent::TurnOptions {
                system_overlay: Some(overlay),
                stored_user_text: None,
                workflow_run_id: Some(run_id),
            },
        )
        .await
    } else {
        agent::run_turn(&app, st, text).await
    };
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
    credentials::save_api_key(&settings.api_key)?;
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

#[tauri::command]
fn ocr_model_status(state: State<'_, AppState>) -> ocr::OcrModelStatus {
    ocr::model_status(state.inner())
}

#[tauri::command]
async fn ocr_download_models(
    app: AppHandle,
    state: State<'_, AppState>,
    source: ocr::OcrModelSource,
) -> Result<ocr::OcrModelStatus, String> {
    ocr::download_models(app, state.inner(), source).await
}

#[tauri::command]
fn workflow_panel_state(state: State<'_, AppState>) -> agent::workflow_runtime::WorkflowPanelState {
    agent::workflow_runtime::panel_state(state.inner())
}

#[tauri::command]
fn workflow_run(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<String, String> {
    let run_id = agent::workflow_runtime::launch(&app, state.inner(), &name)?;
    let app_for_task = app.clone();
    let run_id_for_task = run_id.clone();
    tauri::async_runtime::spawn(async move {
        agent::workflow_runtime::run_launched(app_for_task, run_id_for_task, name).await;
    });
    Ok(run_id)
}

#[tauri::command]
fn workflow_stop(app: AppHandle, state: State<'_, AppState>, run_id: String) -> Result<(), String> {
    agent::workflow_runtime::stop(&app, state.inner(), &run_id)
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

            let mut settings = store::load_settings(&dir);
            if let Err(e) = credentials::hydrate_or_migrate_settings(&dir, &mut settings) {
                eprintln!("Demiurge credential warning: {e}");
            }
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
            ocr_model_status,
            ocr_download_models,
            workflow_panel_state,
            workflow_run,
            workflow_stop,
            voice::voice_status,
            voice::voice_transcribe,
            voice::voice_synthesize,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
