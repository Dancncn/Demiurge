//! Demiurge 引擎 —— Tauri v2 入口：全局状态、命令、构建器。
mod agent;
mod credentials;
mod llm;
mod media;
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

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, PhysicalSize, Size, State};
use tokio::sync::oneshot;

use agent::conversation::Message;
use permission::{PermissionResponse, PermissionRule};
use store::{Session, SessionMeta, SessionStore, Settings};

const DEFAULT_WINDOW_WIDTH: u32 = 1811;
const DEFAULT_WINDOW_HEIGHT: u32 = 1213;

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

#[derive(Serialize)]
struct ContextPanelState {
    message_count: usize,
    user_messages: usize,
    assistant_messages: usize,
    tool_messages: usize,
    summary_chars: usize,
    estimated_history_tokens: usize,
    max_input_tokens: usize,
    reserved_output_tokens: usize,
}

#[derive(Deserialize)]
struct WebDavConfig {
    url: String,
    username: String,
    password: String,
    path: String,
}

#[derive(Serialize)]
struct WebDavBackupFile {
    file_name: String,
    modified_time: String,
    size: u64,
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
    let mut should_drive_goal = false;
    let res = if trimmed == "/dream" || trimmed.starts_with("/dream ") {
        should_drive_goal = true;
        agent::dream::run_manual_dream(&app, st, text).await
    } else if trimmed == "/compact" || trimmed.starts_with("/compact ") {
        should_drive_goal = true;
        agent::collapse::run_manual_compact(&app, st, text).await
    } else if trimmed == "/goal" || trimmed.starts_with("/goal ") {
        match agent::goal::handle_slash(st, trimmed) {
            Ok(agent::goal::GoalSlashOutcome::Respond(body)) => {
                let _ = app.emit("assistant-done", body);
                Ok(())
            }
            Ok(agent::goal::GoalSlashOutcome::Query {
                stored_user_text,
                system_overlay,
                ..
            }) => {
                should_drive_goal = true;
                agent::run_turn_with_options(
                    &app,
                    st,
                    text,
                    agent::TurnOptions {
                        system_overlay: Some(system_overlay),
                        stored_user_text: Some(stored_user_text),
                        workflow_run_id: None,
                        agent_names: Vec::new(),
                        token_budget: None,
                    },
                )
                .await
            }
            Err(e) => Err(e),
        }
    } else if trimmed == "/workflows" {
        should_drive_goal = true;
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
        should_drive_goal = true;
        agent::run_turn_with_options(
            &app,
            st,
            text,
            agent::TurnOptions {
                system_overlay: Some(overlay),
                stored_user_text: None,
                workflow_run_id: Some(run_id),
                agent_names: Vec::new(),
                token_budget: None,
            },
        )
        .await
    } else if trimmed == "/ultracode" || trimmed.starts_with("/ultracode ") {
        should_drive_goal = true;
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
                agent_names: Vec::new(),
                token_budget: None,
            },
        )
        .await
    } else {
        should_drive_goal = true;
        agent::run_turn(&app, st, text).await
    };
    let res = if res.is_ok() && should_drive_goal && !st.cancel.load(Ordering::Relaxed) {
        agent::goal::drive_after_turn(&app, st).await
    } else {
        res
    };
    st.busy.store(false, Ordering::SeqCst);
    res
}

#[tauri::command]
async fn send_with_agents(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
    agent_names: Vec<String>,
) -> Result<(), String> {
    let st = state.inner();
    if st.busy.swap(true, Ordering::SeqCst) {
        return Err("正在处理上一条消息，请稍候。".to_string());
    }
    let res = agent::run_turn_with_options(
        &app,
        st,
        text,
        agent::TurnOptions {
            agent_names,
            ..agent::TurnOptions::default()
        },
    )
    .await;
    let res = if res.is_ok() && !st.cancel.load(Ordering::Relaxed) {
        agent::goal::drive_after_turn(&app, st).await
    } else {
        res
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
    credentials::save_web_search_api_keys(&settings)?;
    credentials::save_webdav_password(&settings.webdav_password)?;
    credentials::save_media_api_key(&settings.media_api_key)?;
    *state.settings.lock().unwrap() = settings.clone();
    let dir = state.data_dir.lock().unwrap().clone();
    store::save_settings(&dir, &settings)
}

#[tauri::command]
async fn webdav_check_connection(
    state: State<'_, AppState>,
    config: WebDavConfig,
) -> Result<String, String> {
    webdav_ensure_collection(&state.http, &config).await?;
    Ok("Connected".to_string())
}

#[tauri::command]
async fn webdav_backup_now(
    state: State<'_, AppState>,
    config: WebDavConfig,
) -> Result<String, String> {
    let client = state.http.clone();
    webdav_ensure_collection(&client, &config).await?;

    let mut settings = state.settings.lock().unwrap().clone();
    settings.api_key.clear();
    settings.tavily_api_key.clear();
    settings.brave_search_api_key.clear();
    settings.exa_api_key.clear();
    settings.webdav_password.clear();
    let sessions = state.sessions.lock().unwrap().clone();
    let payload = json!({
        "app": "Demiurge",
        "version": env!("CARGO_PKG_VERSION"),
        "exported_at": store::now_millis(),
        "settings": settings,
        "sessions": sessions,
    });
    let body = serde_json::to_vec_pretty(&payload).map_err(|e| format!("序列化备份失败：{e}"))?;
    let file_name = format!("demiurge-backup-{}.json", store::now_millis());
    let url = webdav_file_url(&config, &file_name)?;
    let resp = webdav_auth(client.put(url), &config)
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("上传 WebDAV 备份失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("上传 WebDAV 备份失败：HTTP {}", resp.status()));
    }
    Ok(file_name)
}

#[tauri::command]
async fn webdav_list_backups(
    state: State<'_, AppState>,
    config: WebDavConfig,
) -> Result<Vec<WebDavBackupFile>, String> {
    let body = webdav_propfind(&state.http, &config, true).await?;
    Ok(parse_webdav_backup_files(&body))
}

#[tauri::command]
async fn webdav_delete_backup(
    state: State<'_, AppState>,
    config: WebDavConfig,
    file_name: String,
) -> Result<(), String> {
    validate_backup_file_name(&file_name)?;
    let url = webdav_file_url(&config, &file_name)?;
    let resp = webdav_auth(state.http.delete(url), &config)
        .send()
        .await
        .map_err(|e| format!("删除 WebDAV 备份失败：{e}"))?;
    if resp.status().is_success() || resp.status().as_u16() == 404 {
        return Ok(());
    }
    Err(format!("删除 WebDAV 备份失败：HTTP {}", resp.status()))
}

#[tauri::command]
fn permission_panel_state(state: State<'_, AppState>) -> permission::PermissionPanelState {
    permission::panel_state(state.inner())
}

#[tauri::command]
fn permission_reset_rule(
    state: State<'_, AppState>,
    scope: tools::PermissionScope,
    tool: String,
) -> Result<permission::PermissionPanelState, String> {
    permission::reset_rule(state.inner(), scope, &tool)
}

#[tauri::command]
fn list_packs(state: State<'_, AppState>) -> Vec<pack::PackManifest> {
    let dir = state.packs_dir.lock().unwrap().clone();
    pack::list_packs(&dir)
}

#[tauri::command]
fn agent_panel_state(state: State<'_, AppState>) -> agent::custom::AgentPanelState {
    agent::custom::panel_state(state.inner())
}

#[tauri::command]
fn agent_template_json() -> String {
    agent::custom::template_json()
}

#[tauri::command]
fn agent_validate_json(raw_json: String) -> agent::custom::AgentValidationResult {
    agent::custom::validate_raw(&raw_json)
}

#[tauri::command]
fn agent_read_file(
    state: State<'_, AppState>,
    name: String,
) -> Result<agent::custom::AgentEditorFile, String> {
    agent::custom::read_editor_file(state.inner(), &name)
}

#[tauri::command]
fn agent_save_file(
    state: State<'_, AppState>,
    file_name: String,
    raw_json: String,
) -> Result<agent::custom::AgentPanelState, String> {
    let panel = agent::custom::save_editor_file(state.inner(), &file_name, &raw_json)?;
    Ok(panel)
}

#[tauri::command]
fn agent_delete_file(
    state: State<'_, AppState>,
    name: String,
) -> Result<agent::custom::AgentPanelState, String> {
    agent::custom::delete_editor_file(state.inner(), &name)
}

#[tauri::command]
fn goal_panel_state(state: State<'_, AppState>) -> Option<agent::goal::GoalPanelState> {
    agent::goal::panel_state(state.inner())
}

#[tauri::command]
fn goal_pause(state: State<'_, AppState>) -> Result<Option<agent::goal::GoalPanelState>, String> {
    let paused = agent::goal::pause_goal(state.inner()).is_some();
    if !paused {
        return Err("Current goal cannot be paused.".to_string());
    }
    state.persist_sessions();
    Ok(agent::goal::panel_state(state.inner()))
}

#[tauri::command]
async fn goal_resume(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<agent::goal::GoalPanelState>, String> {
    let st = state.inner();
    if st.busy.swap(true, Ordering::SeqCst) {
        return Err("Demiurge is already processing a turn.".to_string());
    }
    let result = async {
        let Some(goal) = agent::goal::resume_goal(st) else {
            return Err("No paused goal to resume.".to_string());
        };
        st.persist_sessions();
        run_goal_control_turn(&app, st, "[Goal resumed]", goal).await
    }
    .await;
    st.busy.store(false, Ordering::SeqCst);
    result
}

#[tauri::command]
async fn goal_continue(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<agent::goal::GoalPanelState>, String> {
    let st = state.inner();
    if st.busy.swap(true, Ordering::SeqCst) {
        return Err("Demiurge is already processing a turn.".to_string());
    }
    let result = async {
        let Some(goal) = agent::goal::continue_from_max_turns(st) else {
            return Err("Current goal is not waiting for continue.".to_string());
        };
        st.persist_sessions();
        run_goal_control_turn(&app, st, "[Goal continued]", goal).await
    }
    .await;
    st.busy.store(false, Ordering::SeqCst);
    result
}

#[tauri::command]
fn goal_clear(state: State<'_, AppState>) -> Option<agent::goal::GoalPanelState> {
    agent::goal::clear_goal(state.inner());
    state.persist_sessions();
    agent::goal::panel_state(state.inner())
}

async fn run_goal_control_turn(
    app: &AppHandle,
    state: &AppState,
    stored_user_text: &str,
    goal: agent::goal::GoalState,
) -> Result<Option<agent::goal::GoalPanelState>, String> {
    let hidden_text = stored_user_text.to_string();
    agent::run_turn_with_options(
        app,
        state,
        hidden_text.clone(),
        agent::TurnOptions {
            system_overlay: Some(agent::goal::build_continuation_prompt(&goal)),
            stored_user_text: Some(hidden_text),
            workflow_run_id: None,
            agent_names: Vec::new(),
            token_budget: None,
        },
    )
    .await?;
    if !state.cancel.load(Ordering::Relaxed) {
        agent::goal::drive_after_turn(app, state).await?;
    }
    Ok(agent::goal::panel_state(state))
}

#[tauri::command]
fn memory_panel_state(state: State<'_, AppState>) -> agent::memory::MemoryPanelState {
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    agent::memory::panel_state(&sandbox)
}

#[tauri::command]
fn memory_update_entry(
    state: State<'_, AppState>,
    id: String,
    kind: String,
    text: String,
) -> Result<agent::memory::MemoryPanelState, String> {
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    agent::memory::update_entry(&sandbox, &id, &kind, &text)
}

#[tauri::command]
fn memory_delete_entry(
    state: State<'_, AppState>,
    id: String,
) -> Result<agent::memory::MemoryPanelState, String> {
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    agent::memory::delete_entry(&sandbox, &id)
}

#[tauri::command]
fn memory_dedupe_apply(
    state: State<'_, AppState>,
) -> Result<agent::memory::MemoryPanelState, String> {
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    agent::memory::apply_dedupe(&sandbox)
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

#[tauri::command]
fn context_panel_state(state: State<'_, AppState>) -> ContextPanelState {
    let settings = state.settings.lock().unwrap().clone();
    let store = state.sessions.lock().unwrap();
    let Some(session) = store.get(&store.active) else {
        return ContextPanelState {
            message_count: 0,
            user_messages: 0,
            assistant_messages: 0,
            tool_messages: 0,
            summary_chars: 0,
            estimated_history_tokens: 0,
            max_input_tokens: settings.max_input_tokens,
            reserved_output_tokens: settings.reserved_output_tokens,
        };
    };
    ContextPanelState {
        message_count: session.messages.len(),
        user_messages: session.messages.iter().filter(|m| m.role == "user").count(),
        assistant_messages: session
            .messages
            .iter()
            .filter(|m| m.role == "assistant")
            .count(),
        tool_messages: session.messages.iter().filter(|m| m.role == "tool").count(),
        summary_chars: session
            .summary
            .as_deref()
            .unwrap_or_default()
            .chars()
            .count(),
        estimated_history_tokens: agent::budget::estimate_messages_tokens(&session.messages),
        max_input_tokens: settings.max_input_tokens,
        reserved_output_tokens: settings.reserved_output_tokens,
    }
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

/// 重命名指定会话。返回清洗后的标题，方便前端保持一致展示。
#[tauri::command]
fn rename_session(state: State<'_, AppState>, id: String, title: String) -> Result<String, String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err("会话标题不能为空".to_string());
    }

    let clean: String = trimmed.chars().take(80).collect();
    {
        let mut store = state.sessions.lock().unwrap();
        let session = store.get_mut(&id).ok_or_else(|| "会话不存在".to_string())?;
        session.title = clean.clone();
        session.updated_at = store::now_millis();
    }
    state.persist_sessions();
    Ok(clean)
}

/// 打开沙盒目录（方便用户放/取文件）。
#[tauri::command]
fn open_sandbox(state: State<'_, AppState>) -> Result<(), String> {
    let dir = state.sandbox_dir.lock().unwrap().clone();
    tools::execute_open(&dir.to_string_lossy()).map(|_| ())
}

#[tauri::command]
fn ocr_image_bytes(state: State<'_, AppState>, bytes: Vec<u8>) -> Result<String, String> {
    let img = image::load_from_memory(&bytes)
        .map_err(|e| format!("读取图片失败：{e}"))?
        .to_rgba8();
    ocr::recognize_rgba(state.inner(), img).map(|frame| frame.text)
}

#[tauri::command]
async fn media_generate_image(
    state: State<'_, AppState>,
    request: media::ImageGenerationRequest,
) -> Result<media::ImageGenerationResult, String> {
    media::generate_image(state.inner(), request).await
}

#[tauri::command]
async fn media_synthesize_speech(
    state: State<'_, AppState>,
    request: media::SpeechSynthesisRequest,
) -> Result<media::SpeechSynthesisResult, String> {
    media::synthesize_speech(state.inner(), request).await
}

fn webdav_auth(req: reqwest::RequestBuilder, config: &WebDavConfig) -> reqwest::RequestBuilder {
    let username = config.username.trim();
    if username.is_empty() {
        req
    } else {
        req.basic_auth(username.to_string(), Some(config.password.clone()))
    }
}

fn webdav_collection_url(config: &WebDavConfig) -> Result<String, String> {
    let base = config.url.trim().trim_end_matches('/');
    if !(base.starts_with("http://") || base.starts_with("https://")) {
        return Err("WebDAV URL must start with http:// or https://.".to_string());
    }
    let path = config.path.trim().trim_matches('/');
    if path.is_empty() {
        Ok(format!("{base}/"))
    } else {
        Ok(format!("{base}/{path}/"))
    }
}

fn webdav_file_url(config: &WebDavConfig, file_name: &str) -> Result<String, String> {
    validate_backup_file_name(file_name)?;
    Ok(format!("{}{}", webdav_collection_url(config)?, file_name))
}

fn validate_backup_file_name(file_name: &str) -> Result<(), String> {
    let valid = file_name.starts_with("demiurge-backup-")
        && file_name.ends_with(".json")
        && !file_name.contains('/')
        && !file_name.contains('\\')
        && !file_name.contains("..");
    if valid {
        Ok(())
    } else {
        Err("Invalid backup file name.".to_string())
    }
}

async fn webdav_propfind(
    client: &reqwest::Client,
    config: &WebDavConfig,
    depth_one: bool,
) -> Result<String, String> {
    let method = reqwest::Method::from_bytes(b"PROPFIND").map_err(|e| e.to_string())?;
    let resp = webdav_auth(
        client.request(method, webdav_collection_url(config)?),
        config,
    )
    .header("Depth", if depth_one { "1" } else { "0" })
    .header("Content-Type", "application/xml")
    .body(
        r#"<?xml version="1.0" encoding="utf-8" ?>
<propfind xmlns="DAV:">
  <prop>
    <displayname />
    <getcontentlength />
    <getlastmodified />
    <resourcetype />
  </prop>
</propfind>"#,
    )
    .send()
    .await
    .map_err(|e| format!("WebDAV PROPFIND 失败：{e}"))?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if status.is_success() || status.as_u16() == 207 {
        Ok(body)
    } else {
        Err(format!("WebDAV PROPFIND 失败：HTTP {status}"))
    }
}

async fn webdav_ensure_collection(
    client: &reqwest::Client,
    config: &WebDavConfig,
) -> Result<(), String> {
    if webdav_propfind(client, config, false).await.is_ok() {
        return Ok(());
    }
    let method = reqwest::Method::from_bytes(b"MKCOL").map_err(|e| e.to_string())?;
    let resp = webdav_auth(
        client.request(method, webdav_collection_url(config)?),
        config,
    )
    .send()
    .await
    .map_err(|e| format!("创建 WebDAV 目录失败：{e}"))?;
    if resp.status().is_success() || resp.status().as_u16() == 405 {
        Ok(())
    } else {
        Err(format!("创建 WebDAV 目录失败：HTTP {}", resp.status()))
    }
}

fn parse_webdav_backup_files(body: &str) -> Vec<WebDavBackupFile> {
    let response_re = Regex::new(r"(?is)<[^:>/]*:?response\b[^>]*>.*?</[^:>/]*:?response>")
        .expect("valid WebDAV response regex");
    let href_re =
        Regex::new(r"(?is)<[^:>/]*:?href[^>]*>(.*?)</[^:>/]*:?href>").expect("valid href regex");
    let modified_re =
        Regex::new(r"(?is)<[^:>/]*:?getlastmodified[^>]*>(.*?)</[^:>/]*:?getlastmodified>")
            .expect("valid modified regex");
    let size_re =
        Regex::new(r"(?is)<[^:>/]*:?getcontentlength[^>]*>(.*?)</[^:>/]*:?getcontentlength>")
            .expect("valid size regex");

    let mut files = Vec::new();
    for response in response_re.find_iter(body).map(|m| m.as_str()) {
        let Some(href) = href_re
            .captures(response)
            .and_then(|c| c.get(1))
            .map(|m| xml_unescape(m.as_str()))
        else {
            continue;
        };
        let file_name = percent_decode(href.trim_end_matches('/').rsplit('/').next().unwrap_or(""));
        if validate_backup_file_name(&file_name).is_err() {
            continue;
        }
        let modified_time = modified_re
            .captures(response)
            .and_then(|c| c.get(1))
            .map(|m| xml_unescape(m.as_str()))
            .unwrap_or_default();
        let size = size_re
            .captures(response)
            .and_then(|c| c.get(1))
            .and_then(|m| m.as_str().trim().parse::<u64>().ok())
            .unwrap_or(0);
        files.push(WebDavBackupFile {
            file_name,
            modified_time,
            size,
        });
    }
    files.sort_by(|a, b| b.file_name.cmp(&a.file_name));
    files
}

fn xml_unescape(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = u8::from_str_radix(&value[i + 1..i + 3], 16) {
                out.push(hex);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
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
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_size(Size::Physical(PhysicalSize {
                    width: DEFAULT_WINDOW_WIDTH,
                    height: DEFAULT_WINDOW_HEIGHT,
                }));
                let _ = window.center();
            }

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
            send_with_agents,
            interrupt,
            respond_confirm,
            get_settings,
            save_settings,
            webdav_check_connection,
            webdav_backup_now,
            webdav_list_backups,
            webdav_delete_backup,
            permission_panel_state,
            permission_reset_rule,
            list_packs,
            agent_panel_state,
            agent_template_json,
            agent_validate_json,
            agent_read_file,
            agent_save_file,
            agent_delete_file,
            goal_panel_state,
            goal_pause,
            goal_resume,
            goal_continue,
            goal_clear,
            memory_panel_state,
            memory_update_entry,
            memory_delete_entry,
            memory_dedupe_apply,
            list_sessions,
            get_history,
            context_panel_state,
            new_session,
            select_session,
            delete_session,
            rename_session,
            open_sandbox,
            ocr_image_bytes,
            media_generate_image,
            media_synthesize_speech,
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
