//! Demiurge 引擎 —— Tauri v2 入口：全局状态、命令、构建器。
mod agent;
mod companion;
mod connection_tests;
mod credentials;
mod llm;
pub mod mcp;
mod media;
mod ocr;
mod pack;
mod permission;
mod pomodoro;
mod startup;
mod store;
mod tools;
mod voice;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, PhysicalSize, Size, State};
use tokio::sync::oneshot;

use agent::conversation::Message;
use permission::{PermissionResponse, PermissionRule};
use store::{PermissionMode, ReasoningEffort, Session, SessionMeta, SessionStore, Settings};

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
    fn new(http: reqwest::Client) -> Self {
        AppState {
            http,
            settings: Mutex::new(Settings::default()),
            sessions: Mutex::new(SessionStore::default()),
            pending_confirms: Mutex::new(HashMap::new()),
            session_permission_rules: Mutex::new(HashMap::new()),
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
        std::thread::spawn(move || {
            let written = WRITTEN.get_or_init(|| Mutex::new(HashMap::new()));
            let mut map = written.lock().unwrap_or_else(|e| e.into_inner());
            let last = map.entry(dir.clone()).or_insert(0);
            if *last >= seq {
                return; // 已有更新的快照落盘，跳过这次旧数据写入
            }
            let _ = store::save_sessions(&dir, &store);
            *last = seq;
        });
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
    summary_tokens: usize,
    system_prompt_chars: usize,
    system_prompt_tokens: usize,
    estimated_history_tokens: usize,
    tools_tokens: usize,
    history_budget_tokens: usize,
    history_remaining_tokens: usize,
    history_over_budget_tokens: usize,
    max_input_tokens: usize,
    reserved_output_tokens: usize,
    input_budget_used_tokens: usize,
    input_budget_remaining_tokens: usize,
    projected_total_tokens: usize,
    prompt_section_tokens: usize,
    budget_items: Vec<ContextBudgetItem>,
    history_buckets: Vec<ContextHistoryBucket>,
    memory_sources: Vec<ContextMemorySource>,
    prompt_sections: Vec<agent::prompt::PromptSectionReport>,
}

#[derive(Serialize)]
struct ContextBudgetItem {
    id: String,
    label: String,
    tokens: usize,
    limit_tokens: Option<usize>,
    detail: String,
}

#[derive(Serialize)]
struct ContextHistoryBucket {
    role: String,
    label: String,
    messages: usize,
    tokens: usize,
}

#[derive(Serialize)]
struct ContextMemorySource {
    id: String,
    label: String,
    path: String,
    exists: bool,
    chars: usize,
    tokens: usize,
    entries: usize,
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

fn memory_context(state: &AppState) -> (PathBuf, PathBuf, PathBuf, String, String) {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let sandbox_dir = state.sandbox_dir.lock().unwrap().clone();
    let packs_dir = state.packs_dir.lock().unwrap().clone();
    let pack_id = state.settings.lock().unwrap().current_pack.clone();
    let session_id = state.sessions.lock().unwrap().active.clone();
    (data_dir, sandbox_dir, packs_dir, pack_id, session_id)
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

fn effort_usage() -> String {
    let levels = ReasoningEffort::LEVELS
        .iter()
        .map(|level| level.as_str())
        .collect::<Vec<_>>()
        .join("|");
    format!(
        "Usage: /effort [{levels}|auto]\n\n\
         Current levels:\n\
         - low: {}\n\
         - medium: {}\n\
         - high: {}\n\
         - xhigh: {}\n\
         - max: {}\n\
         - auto: {}",
        ReasoningEffort::Low.description(),
        ReasoningEffort::Medium.description(),
        ReasoningEffort::High.description(),
        ReasoningEffort::Xhigh.description(),
        ReasoningEffort::Max.description(),
        ReasoningEffort::Auto.description()
    )
}

fn effort_status(settings: &Settings) -> String {
    let profile = llm::ProviderProfile::for_kind(settings.provider);
    if !profile.supports_reasoning_effort_for_model(&settings.model) {
        return format!(
            "Configured effort: `{}`.\nProvider/model `{}` does not support Demiurge effort parameters yet, so no request field will be sent.",
            settings.reasoning_effort.as_str(),
            settings.model
        );
    }
    let applied = profile
        .effective_reasoning_effort(settings)
        .map(|effort| format!("`{}`", effort.as_str()))
        .unwrap_or_else(|| "provider default".to_string());
    format!(
        "Configured effort: `{}`.\nApplied on next supported request: {applied}.",
        settings.reasoning_effort.as_str()
    )
}

fn emit_settings_updated(app: &AppHandle, settings: &Settings) {
    let _ = app.emit("settings-updated", settings.clone());
}

fn handle_effort_slash(app: &AppHandle, state: &AppState, text: &str) -> Result<String, String> {
    let arg = text.trim().strip_prefix("/effort").unwrap_or("").trim();
    if arg.is_empty() {
        let settings = state.settings.lock().unwrap().clone();
        return Ok(format!(
            "{}\n\n{}",
            effort_status(&settings),
            effort_usage()
        ));
    }
    let Some(effort) = ReasoningEffort::parse(arg) else {
        return Ok(effort_usage());
    };
    let next = {
        let mut settings = state.settings.lock().unwrap();
        settings.reasoning_effort = effort;
        settings.clone()
    };
    let dir = state.data_dir.lock().unwrap().clone();
    store::save_settings(&dir, &next)?;
    emit_settings_updated(app, &next);
    Ok(format!(
        "Set effort to `{}`: {}\n\n{}",
        effort.as_str(),
        effort.description(),
        effort_status(&next)
    ))
}

fn persist_direct_reply(
    state: &AppState,
    session_id: &str,
    user_text: String,
    assistant_text: String,
) {
    {
        let mut sessions = state.sessions.lock().unwrap();
        if let Some(session) = sessions.get_mut(session_id) {
            session.messages.push(Message::user(user_text));
            session
                .messages
                .push(Message::assistant_text(assistant_text));
            session.updated_at = store::now_millis();
        }
    }
    state.persist_sessions();
}

// ---------------- Tauri 命令 ----------------

/// 发送一条用户消息，跑完整轮 Agent 循环；过程通过事件流推给前端。
#[tauri::command]
async fn send(app: AppHandle, state: State<'_, AppState>, text: String) -> Result<(), String> {
    let st = state.inner();
    let session_id = st.sessions.lock().unwrap().active.clone();
    let turn = agent::session_engine::begin_turn(
        &app,
        st,
        agent::session_engine::TurnStart {
            entrypoint: agent::session_engine::TurnEntrypoint::Send,
            session_id: session_id.clone(),
            input: text.clone(),
            workflow_run_id: None,
            agent_names: Vec::new(),
        },
    )?;
    let events = agent::session_engine::TurnEventEmitter::new(&app, st);
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
                events.assistant_done(body);
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
    } else if trimmed == "/skills"
        || trimmed.starts_with("/skills ")
        || trimmed == "/skill"
        || trimmed.starts_with("/skill ")
    {
        let body = agent::skills::slash_response(st, trimmed)?;
        events.assistant_done(body);
        Ok(())
    } else if trimmed == "/effort" || trimmed.starts_with("/effort ") {
        let body = handle_effort_slash(&app, st, trimmed)?;
        events.assistant_done(body);
        Ok(())
    } else if trimmed == "/workflows" {
        should_drive_goal = true;
        let runs = agent::workflow_runtime::panel_state(st).runs;
        let body = if runs.is_empty() {
            "暂无 workflow run。使用 /ultracode <任务> 或 Workflows 面板会自动创建 run。"
                .to_string()
        } else {
            let mut out = String::from("Workflow runs:\n");
            for run in runs.iter().take(20) {
                let status = match run.status {
                    agent::workflow_runtime::WorkflowStatus::Running => "running",
                    agent::workflow_runtime::WorkflowStatus::StaleRunning => "stale_running",
                    agent::workflow_runtime::WorkflowStatus::Done => "done",
                    agent::workflow_runtime::WorkflowStatus::Failed => "failed",
                    agent::workflow_runtime::WorkflowStatus::Killed => "killed",
                    agent::workflow_runtime::WorkflowStatus::Journaled => "journaled",
                };
                let budget = run
                    .budget
                    .total
                    .map(|total| format!(" budget={}/{}", run.budget.used_total(), total))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "- `{}` status={} steps={}/{} agents={}{} updated_at={} journal={}\n",
                    run.run_id,
                    status,
                    run.steps_done,
                    run.steps_total,
                    run.agents.len(),
                    budget,
                    run.updated_at,
                    run.journal_path
                ));
            }
            out
        };
        events.assistant_done(body);
        Ok(())
    } else if trimmed.starts_with("/workflow resume ") {
        let run_id = trimmed
            .trim_start_matches("/workflow resume ")
            .trim()
            .to_string();
        let overlay = agent::workflow_runtime::resume_overlay(st, &run_id)?;
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
    } else if let Some(risk) = companion::detect_high_risk_expression(trimmed) {
        persist_direct_reply(st, &session_id, text.clone(), risk.support_message.clone());
        events.assistant_done(risk.support_message);
        Ok(())
    } else {
        should_drive_goal = true;
        agent::run_turn(&app, st, text).await
    };
    let res = if res.is_ok() && should_drive_goal && !st.cancel.load(Ordering::Relaxed) {
        agent::goal::drive_after_turn(&app, st).await
    } else {
        res
    };
    let status = if st.cancel.load(Ordering::Relaxed) {
        agent::session_engine::TurnStatus::Interrupted
    } else if res.is_ok() {
        agent::session_engine::TurnStatus::Completed
    } else {
        agent::session_engine::TurnStatus::Failed
    };
    let error = res.as_ref().err().cloned();
    agent::session_engine::finish_turn(&app, st, &turn, status, error);
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
    let session_id = st.sessions.lock().unwrap().active.clone();
    let turn = agent::session_engine::begin_turn(
        &app,
        st,
        agent::session_engine::TurnStart {
            entrypoint: agent::session_engine::TurnEntrypoint::SendWithAgents,
            session_id: session_id.clone(),
            input: text.clone(),
            workflow_run_id: None,
            agent_names: agent_names.clone(),
        },
    )?;
    let mut should_drive_goal = true;
    let res = if let Some(risk) = companion::detect_high_risk_expression(&text) {
        should_drive_goal = false;
        persist_direct_reply(st, &session_id, text.clone(), risk.support_message.clone());
        let events = agent::session_engine::TurnEventEmitter::new(&app, st);
        events.assistant_done(risk.support_message);
        Ok(())
    } else {
        agent::run_turn_with_options(
            &app,
            st,
            text,
            agent::TurnOptions {
                agent_names,
                ..agent::TurnOptions::default()
            },
        )
        .await
    };
    let res = if res.is_ok() && should_drive_goal && !st.cancel.load(Ordering::Relaxed) {
        agent::goal::drive_after_turn(&app, st).await
    } else {
        res
    };
    let status = if st.cancel.load(Ordering::Relaxed) {
        agent::session_engine::TurnStatus::Interrupted
    } else if res.is_ok() {
        agent::session_engine::TurnStatus::Completed
    } else {
        agent::session_engine::TurnStatus::Failed
    };
    let error = res.as_ref().err().cloned();
    agent::session_engine::finish_turn(&app, st, &turn, status, error);
    res
}

/// 中断当前流式生成。
#[tauri::command]
fn interrupt(app: AppHandle, state: State<'_, AppState>) {
    agent::session_engine::request_interrupt(&app, state.inner());
    // 立即唤醒所有正在等待的确认（按「中断」处理），否则确认弹窗的 await 会把整轮卡住最长 5 分钟
    let mut pending = state.pending_confirms.lock().unwrap();
    for (_, tx) in pending.drain() {
        let _ = tx.send(PermissionResponse::deny_once());
    }
}

#[tauri::command]
fn session_engine_state(
    state: State<'_, AppState>,
) -> agent::session_engine::SessionEnginePanelState {
    agent::session_engine::panel_state(state.inner())
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
fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<(), String> {
    let current_launch_on_startup = state.settings.lock().unwrap().launch_on_startup;
    if settings.launch_on_startup != current_launch_on_startup {
        startup::apply_launch_on_startup(settings.launch_on_startup)?;
    }
    credentials::save_api_key(&settings.api_key)?;
    credentials::save_web_search_api_keys(&settings)?;
    credentials::save_webdav_password(&settings.webdav_password)?;
    credentials::save_media_api_key(&settings.media_api_key)?;
    credentials::save_mcp_env_secrets(&settings)?;
    *state.settings.lock().unwrap() = settings.clone();
    let dir = state.data_dir.lock().unwrap().clone();
    store::save_settings(&dir, &settings)?;
    emit_settings_updated(&app, &settings);
    Ok(())
}

#[tauri::command]
async fn provider_check_connection(
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<connection_tests::ConnectionTestResult, String> {
    connection_tests::test_provider(&state.http, settings).await
}

#[tauri::command]
async fn web_search_check_connection(
    state: State<'_, AppState>,
    settings: Settings,
    provider: Option<String>,
) -> Result<connection_tests::ConnectionTestResult, String> {
    connection_tests::test_web_search(&state.http, settings, provider).await
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

    let settings = store::redacted_settings(&state.settings.lock().unwrap().clone());
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
fn set_permission_mode(
    app: AppHandle,
    state: State<'_, AppState>,
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

#[tauri::command]
fn plan_state(state: State<'_, AppState>) -> PlanState {
    state.plan_state.lock().unwrap().clone()
}

#[tauri::command]
fn approve_plan(app: AppHandle, state: State<'_, AppState>) -> Result<PlanState, String> {
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

#[tauri::command]
fn reject_plan(app: AppHandle, state: State<'_, AppState>) -> PlanState {
    let next = {
        let mut plan = state.plan_state.lock().unwrap();
        plan.reset();
        plan.clone()
    };
    let _ = app.emit("plan-updated", next.clone());
    next
}

#[tauri::command]
fn permission_panel_state(state: State<'_, AppState>) -> permission::PermissionPanelState {
    permission::panel_state(state.inner())
}

#[tauri::command]
fn shell_policy_state() -> tools::ShellPolicyState {
    tools::shell_policy_state()
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
fn permission_upsert_rule(
    state: State<'_, AppState>,
    input: permission::PermissionRuleInput,
) -> Result<permission::PermissionPanelState, String> {
    permission::upsert_rule(state.inner(), input)
}

#[tauri::command]
async fn mcp_panel_state(state: State<'_, AppState>) -> Result<mcp::McpPanelState, String> {
    mcp::ensure_initialized(state.inner()).await;
    Ok(mcp::panel_state(state.inner()))
}

#[tauri::command]
async fn mcp_refresh(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<mcp::McpPanelState, String> {
    mcp::refresh_all(state.inner()).await;
    let panel = mcp::panel_state(state.inner());
    let _ = app.emit("mcp-updated", panel.clone());
    Ok(panel)
}

#[tauri::command]
async fn mcp_set_server_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    enabled: bool,
) -> Result<mcp::McpPanelState, String> {
    {
        let mut settings = state.settings.lock().unwrap();
        let server = settings
            .mcp_servers
            .iter_mut()
            .find(|server| server.name == name)
            .ok_or_else(|| format!("MCP server `{name}` 不存在。"))?;
        server.enabled = enabled;
        let dir = state.data_dir.lock().unwrap().clone();
        store::save_settings(&dir, &settings)?;
    }
    mcp::disconnect_server(state.inner(), &name).await;
    mcp::ensure_initialized(state.inner()).await;
    let panel = mcp::panel_state(state.inner());
    let _ = app.emit("mcp-updated", panel.clone());
    Ok(panel)
}

#[tauri::command]
fn list_packs(state: State<'_, AppState>) -> Vec<pack::PackManifest> {
    let dir = state.packs_dir.lock().unwrap().clone();
    pack::list_packs(&dir)
}

#[tauri::command]
fn import_pack_zip(
    state: State<'_, AppState>,
    file_name: String,
    bytes: Vec<u8>,
) -> Result<pack::PackManifest, String> {
    let dir = state.packs_dir.lock().unwrap().clone();
    pack::import_zip(&dir, &file_name, bytes)
}

#[tauri::command]
fn read_pack_manifest_json(state: State<'_, AppState>, id: String) -> Result<String, String> {
    let dir = state.packs_dir.lock().unwrap().clone();
    pack::read_manifest_json(&dir, &id)
}

#[tauri::command]
fn save_pack_manifest_json(
    state: State<'_, AppState>,
    id: String,
    raw_json: String,
) -> Result<pack::PackManifest, String> {
    let dir = state.packs_dir.lock().unwrap().clone();
    pack::save_manifest_json(&dir, &id, &raw_json)
}

#[tauri::command]
fn preview_pack_lorebook(
    state: State<'_, AppState>,
    id: String,
    query: String,
) -> Result<String, String> {
    let packs_dir = state.packs_dir.lock().unwrap().clone();
    let data_dir = state.data_dir.lock().unwrap().clone();
    Ok(pack::lorebook_context(
        &packs_dir,
        &data_dir,
        &id,
        Some(&query),
    ))
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
    let (data, sandbox, packs, pack_id, session_id) = memory_context(state.inner());
    agent::memory::panel_state(&data, &sandbox, &packs, &pack_id, &session_id)
}

#[tauri::command]
fn memory_add_entry(
    state: State<'_, AppState>,
    scope: String,
    kind: String,
    text: String,
) -> Result<agent::memory::MemoryPanelState, String> {
    let (data, sandbox, packs, pack_id, session_id) = memory_context(state.inner());
    agent::memory::add_entry(
        &data,
        &sandbox,
        &packs,
        &pack_id,
        &session_id,
        &scope,
        &kind,
        &text,
    )
}

#[tauri::command]
fn memory_update_entry(
    state: State<'_, AppState>,
    id: String,
    kind: String,
    text: String,
) -> Result<agent::memory::MemoryPanelState, String> {
    let (data, sandbox, packs, pack_id, session_id) = memory_context(state.inner());
    agent::memory::update_entry(
        &data,
        &sandbox,
        &packs,
        &pack_id,
        &session_id,
        &id,
        &kind,
        &text,
    )
}

#[tauri::command]
fn memory_delete_entry(
    state: State<'_, AppState>,
    id: String,
) -> Result<agent::memory::MemoryPanelState, String> {
    let (data, sandbox, packs, pack_id, session_id) = memory_context(state.inner());
    agent::memory::delete_entry(&data, &sandbox, &packs, &pack_id, &session_id, &id)
}

#[tauri::command]
fn memory_dedupe_apply(
    state: State<'_, AppState>,
) -> Result<agent::memory::MemoryPanelState, String> {
    let (data, sandbox, packs, pack_id, session_id) = memory_context(state.inner());
    agent::memory::apply_dedupe(&data, &sandbox, &packs, &pack_id, &session_id)
}

// ---- 会话管理 ----

#[tauri::command]
fn list_sessions(state: State<'_, AppState>) -> SessionList {
    session_list(&state.sessions.lock().unwrap())
}

/// 仪表盘统计：会话/消息/估算 token/活跃天数/连续天数/高峰时段 + 活跃热力图。
/// `offset` 为客户端时区偏移（分钟，JS Date.getTimezoneOffset()）。
#[tauri::command]
fn session_stats(state: State<'_, AppState>, offset: i64) -> store::StatsPanel {
    let model = state.settings.lock().unwrap().model.clone();
    let store_guard = state.sessions.lock().unwrap();
    store::compute_stats(&store_guard, offset, model)
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
    let (messages, summary) = {
        let store = state.sessions.lock().unwrap();
        store
            .get(&store.active)
            .map(|session| (session.messages.clone(), session.summary.clone()))
            .unwrap_or_else(|| (Vec::new(), None))
    };

    let packs_dir = state.packs_dir.lock().unwrap().clone();
    let sandbox_dir = state.sandbox_dir.lock().unwrap().clone();
    let data_dir = state.data_dir.lock().unwrap().clone();
    let session_id = state.sessions.lock().unwrap().active.clone();
    let persona_text = pack::load_pack(&packs_dir, &settings.current_pack)
        .map(|p| p.persona_text)
        .unwrap_or_default();
    let prompt_build = agent::prompt::build_with_report(
        state.inner(),
        &settings,
        &persona_text,
        summary.as_deref(),
    );
    let profile = llm::ProviderProfile::for_kind(settings.provider);
    let tools_schema = if profile.supports_tools {
        tools::main_schemas_json_for(profile.tool_schema_dialect)
    } else {
        profile.empty_tool_schema()
    };
    let budget =
        agent::budget::history_budget(&settings, &prompt_build.text, &tools_schema, &messages);
    let summary_text = summary.as_deref().unwrap_or_default();
    let summary_chars = summary_text.chars().count();
    let summary_tokens = agent::budget::estimate_text_tokens(summary_text);
    let prompt_section_tokens = prompt_build
        .sections
        .iter()
        .map(|section| section.tokens)
        .sum::<usize>();
    let input_budget_used_tokens = budget
        .system_tokens
        .saturating_add(budget.tools_tokens)
        .saturating_add(budget.history_tokens);
    let input_budget_remaining_tokens = budget
        .max_input_tokens
        .saturating_sub(input_budget_used_tokens);
    let projected_total_tokens =
        input_budget_used_tokens.saturating_add(budget.reserved_output_tokens);
    let history_remaining_tokens = budget
        .history_budget_tokens
        .saturating_sub(budget.history_tokens);
    let history_over_budget_tokens = budget
        .history_tokens
        .saturating_sub(budget.history_budget_tokens);

    ContextPanelState {
        message_count: messages.len(),
        user_messages: messages.iter().filter(|m| m.role == "user").count(),
        assistant_messages: messages.iter().filter(|m| m.role == "assistant").count(),
        tool_messages: messages.iter().filter(|m| m.role == "tool").count(),
        summary_chars,
        summary_tokens,
        system_prompt_chars: prompt_build.prompt_chars,
        system_prompt_tokens: budget.system_tokens,
        estimated_history_tokens: budget.history_tokens,
        tools_tokens: budget.tools_tokens,
        history_budget_tokens: budget.history_budget_tokens,
        history_remaining_tokens,
        history_over_budget_tokens,
        max_input_tokens: budget.max_input_tokens,
        reserved_output_tokens: budget.reserved_output_tokens,
        input_budget_used_tokens,
        input_budget_remaining_tokens,
        projected_total_tokens,
        prompt_section_tokens,
        budget_items: context_budget_items(&budget),
        history_buckets: context_history_buckets(&messages),
        memory_sources: context_memory_sources(
            &data_dir,
            &sandbox_dir,
            &packs_dir,
            &settings.current_pack,
            &session_id,
        ),
        prompt_sections: prompt_build.sections,
    }
}

fn context_budget_items(budget: &agent::budget::ContextBudget) -> Vec<ContextBudgetItem> {
    vec![
        ContextBudgetItem {
            id: "system".to_string(),
            label: "System prompt".to_string(),
            tokens: budget.system_tokens,
            limit_tokens: Some(budget.max_input_tokens),
            detail:
                "Packed persona, instructions, summary, memories, environment and safety sections."
                    .to_string(),
        },
        ContextBudgetItem {
            id: "tools".to_string(),
            label: "Tool schemas".to_string(),
            tokens: budget.tools_tokens,
            limit_tokens: Some(budget.max_input_tokens),
            detail: "Serialized tool definitions supplied to the provider.".to_string(),
        },
        ContextBudgetItem {
            id: "history".to_string(),
            label: "History".to_string(),
            tokens: budget.history_tokens,
            limit_tokens: Some(budget.history_budget_tokens),
            detail: "Current session messages before token-aware trimming.".to_string(),
        },
        ContextBudgetItem {
            id: "output_reserve".to_string(),
            label: "Output reserve".to_string(),
            tokens: budget.reserved_output_tokens,
            limit_tokens: Some(budget.max_input_tokens),
            detail: "Tokens reserved for the model response.".to_string(),
        },
    ]
}

fn context_history_buckets(messages: &[Message]) -> Vec<ContextHistoryBucket> {
    let mut buckets = [
        ("system", "System"),
        ("user", "User"),
        ("assistant", "Assistant"),
        ("tool", "Tool"),
        ("other", "Other"),
    ]
    .into_iter()
    .map(|(role, label)| ContextHistoryBucket {
        role: role.to_string(),
        label: label.to_string(),
        messages: 0,
        tokens: 0,
    })
    .collect::<Vec<_>>();

    for message in messages {
        let idx = match message.role.as_str() {
            "system" => 0,
            "user" => 1,
            "assistant" => 2,
            "tool" => 3,
            _ => 4,
        };
        buckets[idx].messages += 1;
        buckets[idx].tokens = buckets[idx]
            .tokens
            .saturating_add(agent::budget::estimate_message_tokens(message));
    }
    buckets
}

fn context_memory_sources(
    data_dir: &Path,
    sandbox_dir: &Path,
    packs_dir: &Path,
    pack_id: &str,
    session_id: &str,
) -> Vec<ContextMemorySource> {
    let mut sources =
        agent::memory::scoped_memory_paths(data_dir, sandbox_dir, packs_dir, pack_id, session_id)
            .into_iter()
            .map(|(id, label, path)| context_memory_source(&id, &format!("{label} memory"), &path))
            .collect::<Vec<_>>();
    sources.push(context_memory_source(
        "project_legacy",
        "Project legacy memory",
        &sandbox_dir.join("memory.md"),
    ));
    sources
}

fn context_memory_source(id: &str, label: &str, path: &Path) -> ContextMemorySource {
    let raw = fs::read_to_string(path).unwrap_or_default();
    let exists = path.is_file();
    let chars = raw.chars().count();
    let tokens = agent::budget::estimate_text_tokens(&raw);
    let entries = raw
        .lines()
        .filter(|line| line.trim_start().starts_with("- ["))
        .count();
    ContextMemorySource {
        id: id.to_string(),
        label: label.to_string(),
        path: path.to_string_lossy().to_string(),
        exists,
        chars,
        tokens,
        entries,
    }
}

/// 技能面板状态。可选 `query` 用于按用户输入对技能做匹配/检索打分。
#[tauri::command]
fn skill_panel_state(
    state: State<'_, AppState>,
    query: Option<String>,
) -> agent::skills::SkillPanelState {
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let data_dir = state.data_dir.lock().unwrap().clone();
    let packs_dir = state.packs_dir.lock().unwrap().clone();
    let pack_id = state.settings.lock().unwrap().current_pack.clone();
    let trimmed = query.as_deref().map(str::trim).filter(|s| !s.is_empty());
    agent::skills::panel_state(&sandbox, &data_dir, &packs_dir, &pack_id, trimmed)
}

/// 打开全局技能目录(供「在文件夹中显示」按钮使用)。
#[tauri::command]
fn open_skills_dir(state: State<'_, AppState>) -> Result<(), String> {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let dir = data_dir.join("skills");
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    tools::execute_open(&dir.to_string_lossy()).map(|_| ())
}

#[cfg(test)]
mod context_panel_tests {
    use super::*;

    #[test]
    fn context_history_buckets_group_roles_and_tokens() {
        let messages = vec![
            Message::user("hello"),
            Message::assistant_text("world"),
            Message::tool_result("call_1", "read_file", "tool output"),
        ];
        let buckets = context_history_buckets(&messages);

        let user = buckets.iter().find(|bucket| bucket.role == "user").unwrap();
        assert_eq!(user.messages, 1);
        assert!(user.tokens > 0);

        let assistant = buckets
            .iter()
            .find(|bucket| bucket.role == "assistant")
            .unwrap();
        assert_eq!(assistant.messages, 1);
        assert!(assistant.tokens > 0);

        let tool = buckets.iter().find(|bucket| bucket.role == "tool").unwrap();
        assert_eq!(tool.messages, 1);
        assert!(tool.tokens > 0);
    }

    #[test]
    fn context_memory_sources_report_existing_memory_files() {
        let root = std::env::temp_dir().join(format!(
            "demiurge_context_panel_{}",
            crate::store::now_millis()
        ));
        let data = root.join("data");
        let packs = root.join("packs");
        let sandbox = root.join("sandbox");
        let pack = packs.join("default");
        fs::create_dir_all(data.join("memory")).unwrap();
        fs::create_dir_all(sandbox.join(".demiurge")).unwrap();
        fs::create_dir_all(sandbox.join(".demiurge").join("session-memory")).unwrap();
        fs::create_dir_all(&pack).unwrap();
        fs::write(
            data.join("memory").join("user.md"),
            "- [user] user preference\n",
        )
        .unwrap();
        fs::write(sandbox.join("memory.md"), "- [project] remember this\n").unwrap();
        fs::write(
            sandbox.join(".demiurge").join("memory.md"),
            "- [project] project fact\n",
        )
        .unwrap();
        fs::write(
            sandbox
                .join(".demiurge")
                .join("session-memory")
                .join("session_1.md"),
            "- [session] session fact\n",
        )
        .unwrap();
        fs::write(pack.join("memory.md"), "pack note").unwrap();

        let sources = context_memory_sources(&data, &sandbox, &packs, "default", "session_1");

        let user = sources.iter().find(|source| source.id == "user").unwrap();
        assert!(user.exists);
        assert_eq!(user.entries, 1);

        let project = sources
            .iter()
            .find(|source| source.id == "project")
            .unwrap();
        assert!(project.exists);
        assert_eq!(project.entries, 1);
        assert!(project.tokens > 0);

        let session = sources
            .iter()
            .find(|source| source.id == "session")
            .unwrap();
        assert!(session.exists);
        assert_eq!(session.entries, 1);

        let pack = sources.iter().find(|source| source.id == "pack").unwrap();
        assert!(pack.exists);
        assert_eq!(pack.entries, 0);

        let legacy = sources
            .iter()
            .find(|source| source.id == "project_legacy")
            .unwrap();
        assert!(legacy.exists);
        assert_eq!(legacy.entries, 1);

        let _ = fs::remove_dir_all(root);
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
    if !state.settings.lock().unwrap().computer_use_enabled {
        return Err("Computer Use / OCR is not enabled.".to_string());
    }
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

#[tauri::command]
async fn companion_panel_state(
    state: State<'_, AppState>,
) -> Result<companion::CompanionPanelState, String> {
    Ok(companion::panel_state(state.inner()).await)
}

#[tauri::command]
async fn companion_clear_weather_cache(
    state: State<'_, AppState>,
) -> Result<companion::CompanionPanelState, String> {
    companion::clear_weather_cache();
    Ok(companion::panel_state(state.inner()).await)
}

#[tauri::command]
fn pomodoro_state(state: State<'_, AppState>) -> pomodoro::PomodoroPanelState {
    pomodoro::panel_state(state.inner())
}

#[tauri::command]
fn pomodoro_start(
    app: AppHandle,
    state: State<'_, AppState>,
    request: pomodoro::PomodoroStartRequest,
) -> Result<pomodoro::PomodoroPanelState, String> {
    pomodoro::start(app, state.inner(), request)
}

#[tauri::command]
fn pomodoro_pause(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<pomodoro::PomodoroPanelState, String> {
    pomodoro::pause(app, state.inner())
}

#[tauri::command]
fn pomodoro_resume(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<pomodoro::PomodoroPanelState, String> {
    pomodoro::resume(app, state.inner())
}

#[tauri::command]
fn pomodoro_skip(
    app: AppHandle,
    state: State<'_, AppState>,
    request: Option<pomodoro::PomodoroSkipRequest>,
) -> Result<pomodoro::PomodoroPanelState, String> {
    pomodoro::skip(app, state.inner(), request)
}

#[tauri::command]
fn companion_memory_suggestions(
    state: State<'_, AppState>,
) -> Vec<companion::CompanionMemorySuggestion> {
    let settings = state.settings.lock().unwrap().clone();
    companion::memory_suggestions(&settings)
}

#[tauri::command]
fn companion_memory_queue_state(
    state: State<'_, AppState>,
) -> companion::CompanionMemoryQueueState {
    companion_queue_state(state.inner())
}

#[tauri::command]
fn companion_enqueue_memory_suggestion(
    state: State<'_, AppState>,
    id: String,
) -> Result<companion::CompanionMemoryQueueState, String> {
    let settings = state.settings.lock().unwrap().clone();
    let suggestion = companion::memory_suggestion_by_id(&settings, &id)
        .ok_or_else(|| format!("Unknown companion memory suggestion: {id}"))?;
    let data_dir = state.data_dir.lock().unwrap().clone();
    let session_id = state.sessions.lock().unwrap().active.clone();
    companion::enqueue_memory_suggestion(&data_dir, &session_id, suggestion)
        .map(|_| companion_queue_state(state.inner()))
}

#[tauri::command]
fn companion_save_memory_queue_item(
    state: State<'_, AppState>,
    id: String,
    resolution: Option<String>,
) -> Result<companion::CompanionMemoryQueueState, String> {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let item = companion::pending_memory_queue_item(&data_dir, &id)
        .ok_or_else(|| format!("Unknown pending companion memory queue item: {id}"))?;
    let (data, sandbox, packs, pack_id, session_id) = memory_context(state.inner());
    let panel = agent::memory::panel_state(&data, &sandbox, &packs, &pack_id, &session_id);
    let duplicate = find_similar_memory_entry(&panel, &item);
    let resolution = resolution.unwrap_or_default();
    let saved_id = match (duplicate, resolution.as_str()) {
        (Some(existing), "merge") => {
            let merged = merge_memory_text(&existing.text, &item.text);
            agent::memory::update_entry(
                &data,
                &sandbox,
                &packs,
                &pack_id,
                &session_id,
                &existing.id,
                &item.kind,
                &merged,
            )?;
            Some(existing.id)
        }
        (Some(existing), "replace") => {
            agent::memory::update_entry(
                &data,
                &sandbox,
                &packs,
                &pack_id,
                &session_id,
                &existing.id,
                &item.kind,
                &item.text,
            )?;
            Some(existing.id)
        }
        (Some(_), "keep_new") | (None, _) => {
            let panel = agent::memory::add_entry(
                &data,
                &sandbox,
                &packs,
                &pack_id,
                &session_id,
                &item.scope,
                &item.kind,
                &item.text,
            )?;
            find_saved_memory_id(&panel, &item)
        }
        (Some(_), _) => {
            return Err("Similar memory exists; choose merge, replace, or keep_new.".to_string())
        }
    };
    companion::mark_memory_queue_item(&data_dir, &id, "saved", saved_id)?;
    Ok(companion_queue_state(state.inner()))
}

#[tauri::command]
fn companion_ignore_memory_queue_item(
    state: State<'_, AppState>,
    id: String,
) -> Result<companion::CompanionMemoryQueueState, String> {
    let data_dir = state.data_dir.lock().unwrap().clone();
    companion::mark_memory_queue_item(&data_dir, &id, "ignored", None)?;
    Ok(companion_queue_state(state.inner()))
}

#[tauri::command]
fn companion_save_all_memory_queue_items(
    state: State<'_, AppState>,
) -> Result<companion::CompanionMemoryQueueState, String> {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let pending = companion::memory_queue_state(&data_dir)
        .items
        .into_iter()
        .filter(|item| item.status == "pending")
        .collect::<Vec<_>>();
    let (data, sandbox, packs, pack_id, session_id) = memory_context(state.inner());
    for item in pending {
        let panel = agent::memory::panel_state(&data, &sandbox, &packs, &pack_id, &session_id);
        let duplicate = find_similar_memory_entry(&panel, &item);
        let saved_id = if let Some(existing) = duplicate {
            let merged = merge_memory_text(&existing.text, &item.text);
            agent::memory::update_entry(
                &data,
                &sandbox,
                &packs,
                &pack_id,
                &session_id,
                &existing.id,
                &item.kind,
                &merged,
            )?;
            Some(existing.id)
        } else {
            let panel = agent::memory::add_entry(
                &data,
                &sandbox,
                &packs,
                &pack_id,
                &session_id,
                &item.scope,
                &item.kind,
                &item.text,
            )?;
            find_saved_memory_id(&panel, &item)
        };
        companion::mark_memory_queue_item(&data_dir, &item.id, "saved", saved_id)?;
    }
    Ok(companion_queue_state(state.inner()))
}

#[tauri::command]
fn companion_ignore_all_memory_queue_items(
    state: State<'_, AppState>,
) -> Result<companion::CompanionMemoryQueueState, String> {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let pending_ids = companion::memory_queue_state(&data_dir)
        .items
        .into_iter()
        .filter(|item| item.status == "pending")
        .map(|item| item.id)
        .collect::<Vec<_>>();
    for id in pending_ids {
        companion::mark_memory_queue_item(&data_dir, &id, "ignored", None)?;
    }
    Ok(companion_queue_state(state.inner()))
}

#[tauri::command]
fn companion_undo_memory_queue_item(
    state: State<'_, AppState>,
    id: String,
) -> Result<companion::CompanionMemoryQueueState, String> {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let item = companion::memory_queue_state(&data_dir)
        .items
        .into_iter()
        .find(|item| item.id == id && item.status == "saved")
        .ok_or_else(|| format!("Unknown saved companion memory queue item: {id}"))?;
    let memory_id = item
        .saved_memory_id
        .clone()
        .ok_or_else(|| "Saved memory id is not available for undo.".to_string())?;
    let (data, sandbox, packs, pack_id, session_id) = memory_context(state.inner());
    agent::memory::delete_entry(&data, &sandbox, &packs, &pack_id, &session_id, &memory_id)?;
    companion::mark_memory_queue_item(&data_dir, &id, "pending", None)?;
    Ok(companion_queue_state(state.inner()))
}

fn companion_queue_state(state: &AppState) -> companion::CompanionMemoryQueueState {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let (data, sandbox, packs, pack_id, session_id) = memory_context(state);
    let panel = agent::memory::panel_state(&data, &sandbox, &packs, &pack_id, &session_id);
    let mut queue = companion::memory_queue_state(&data_dir);
    for item in &mut queue.items {
        if item.status != "pending" {
            continue;
        }
        if let Some(existing) = find_similar_memory_entry(&panel, item) {
            item.duplicate_memory_id = Some(existing.id);
            item.duplicate_memory_text = Some(existing.text);
        }
    }
    queue
}

fn find_saved_memory_id(
    panel: &agent::memory::MemoryPanelState,
    item: &companion::CompanionMemoryQueueItem,
) -> Option<String> {
    panel
        .entries
        .iter()
        .filter(|entry| {
            entry.scope == item.scope && entry.kind == item.kind && entry.text == item.text
        })
        .max_by_key(|entry| entry.line)
        .map(|entry| entry.id.clone())
}

fn find_similar_memory_entry(
    panel: &agent::memory::MemoryPanelState,
    item: &companion::CompanionMemoryQueueItem,
) -> Option<agent::memory::MemoryEntry> {
    panel
        .entries
        .iter()
        .filter(|entry| entry.scope == item.scope)
        .find(|entry| memory_text_similar(&entry.text, &item.text))
        .cloned()
}

fn memory_text_similar(a: &str, b: &str) -> bool {
    let a_key = normalize_memory_text_key(a);
    let b_key = normalize_memory_text_key(b);
    if a_key.is_empty() || b_key.is_empty() {
        return false;
    }
    if a_key == b_key
        || (a_key.len() > 14 && b_key.contains(&a_key))
        || (b_key.len() > 14 && a_key.contains(&b_key))
    {
        return true;
    }
    let a_words = a_key
        .split_whitespace()
        .collect::<std::collections::HashSet<_>>();
    let b_words = b_key
        .split_whitespace()
        .collect::<std::collections::HashSet<_>>();
    if a_words.len() < 3 || b_words.len() < 3 {
        return false;
    }
    let intersection = a_words.intersection(&b_words).count();
    let union = a_words.union(&b_words).count().max(1);
    (intersection as f32 / union as f32) >= 0.72
}

fn normalize_memory_text_key(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('-')
        .trim()
        .trim_start_matches("[user]")
        .trim_start_matches("[project]")
        .trim_start_matches("[session]")
        .trim_start_matches("[pack]")
        .trim_start_matches("[preference]")
        .trim_start_matches("[boundary]")
        .trim_start_matches("[routine]")
        .trim_start_matches("[stress]")
        .trim_start_matches("[encouragement]")
        .trim()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn merge_memory_text(existing: &str, incoming: &str) -> String {
    if memory_text_similar(existing, incoming) {
        incoming.to_string()
    } else {
        format!("{}; {}", existing.trim(), incoming.trim())
    }
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
            if let Err(e) = startup::apply_launch_on_startup(settings.launch_on_startup) {
                eprintln!("Demiurge startup integration warning: {e}");
            }
            let sessions = store::load_sessions(&dir);

            let state = app.state::<AppState>();
            *state.data_dir.lock().unwrap() = dir;
            *state.sandbox_dir.lock().unwrap() = sandbox;
            *state.packs_dir.lock().unwrap() = packs;
            *state.settings.lock().unwrap() = settings;
            *state.sessions.lock().unwrap() = sessions;
            agent::workflow_runtime::hydrate_persisted_runs(state.inner());
            pomodoro::hydrate(app.handle().clone(), state.inner());
            // 保证落盘一次（迁移/初始化后）
            state.persist_sessions();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            send,
            send_with_agents,
            interrupt,
            session_engine_state,
            respond_confirm,
            get_settings,
            save_settings,
            provider_check_connection,
            web_search_check_connection,
            set_permission_mode,
            plan_state,
            approve_plan,
            reject_plan,
            webdav_check_connection,
            webdav_backup_now,
            webdav_list_backups,
            webdav_delete_backup,
            permission_panel_state,
            shell_policy_state,
            permission_reset_rule,
            permission_upsert_rule,
            mcp_panel_state,
            mcp_refresh,
            mcp_set_server_enabled,
            list_packs,
            import_pack_zip,
            read_pack_manifest_json,
            save_pack_manifest_json,
            preview_pack_lorebook,
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
            memory_add_entry,
            memory_update_entry,
            memory_delete_entry,
            memory_dedupe_apply,
            list_sessions,
            session_stats,
            get_history,
            context_panel_state,
            skill_panel_state,
            open_skills_dir,
            new_session,
            select_session,
            delete_session,
            rename_session,
            open_sandbox,
            ocr_image_bytes,
            media_generate_image,
            media_synthesize_speech,
            companion_panel_state,
            companion_clear_weather_cache,
            pomodoro_state,
            pomodoro_start,
            pomodoro_pause,
            pomodoro_resume,
            pomodoro_skip,
            companion_memory_suggestions,
            companion_memory_queue_state,
            companion_enqueue_memory_suggestion,
            companion_save_memory_queue_item,
            companion_ignore_memory_queue_item,
            companion_save_all_memory_queue_items,
            companion_ignore_all_memory_queue_items,
            companion_undo_memory_queue_item,
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
