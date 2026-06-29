//! Session Engine：集中管理当前 turn 的运行状态、入口互斥和中断标记。
//!
//! 第一阶段保持既有 assistant/tool 事件协议不变，只把原先散落在 Tauri
//! command 入口里的 busy/cancel 状态收敛到一个可查询、可扩展的运行时状态。

use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter};

use super::conversation::Message;
use crate::{store, tools};

const INPUT_PREVIEW_CHARS: usize = 160;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    Running,
    Cancelling,
    Completed,
    Interrupted,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TurnEntrypoint {
    Send,
    SendWithAgents,
}

#[derive(Clone, Debug, Serialize)]
pub struct TurnRunState {
    pub id: String,
    pub session_id: String,
    pub entrypoint: TurnEntrypoint,
    pub status: TurnStatus,
    pub input_preview: String,
    pub workflow_run_id: Option<String>,
    pub agent_names: Vec<String>,
    pub started_at: u64,
    pub updated_at: u64,
    pub completed_at: Option<u64>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct SessionEngineState {
    pub active_turn: Option<TurnRunState>,
    pub last_turn: Option<TurnRunState>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SessionEnginePanelState {
    pub busy: bool,
    pub cancel_requested: bool,
    pub active_turn: Option<TurnRunState>,
    pub last_turn: Option<TurnRunState>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TurnEventContext {
    pub id: String,
    pub session_id: String,
    pub status: TurnStatus,
}

#[derive(Clone, Debug, Serialize)]
pub struct AgentEventEnvelope<T>
where
    T: Serialize,
{
    pub kind: &'static str,
    pub turn: Option<TurnEventContext>,
    pub timestamp: u64,
    pub payload: T,
}

#[derive(Clone, Debug, Serialize)]
pub struct AssistantErrorEvent {
    pub kind: String,
    pub message: String,
    pub hint: String,
    pub retryable: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ToolStartEvent {
    pub tool_call_id: String,
    pub name: String,
    pub args: Value,
    pub description: Option<&'static str>,
    pub risk: Option<tools::ToolRisk>,
    pub permission_effect: Option<tools::PermissionEffect>,
    pub concurrency: Option<tools::ToolConcurrency>,
    pub output_policy: Option<tools::ToolOutputPolicy>,
    pub preview: Option<String>,
    pub affected_paths: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ToolEndEvent {
    pub tool_call_id: String,
    pub name: String,
    pub ok: bool,
    pub denied: bool,
    pub result: String,
    pub duration_ms: u64,
    pub error_hint: Option<String>,
    pub source_quality: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct TurnStart {
    pub entrypoint: TurnEntrypoint,
    pub session_id: String,
    pub input: String,
    pub workflow_run_id: Option<String>,
    pub agent_names: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct TurnHandle {
    pub id: String,
}

pub struct SessionTurnStore<'a> {
    state: &'a crate::AppState,
    session_id: String,
}

impl<'a> SessionTurnStore<'a> {
    pub fn new(state: &'a crate::AppState, session_id: String) -> Self {
        SessionTurnStore { state, session_id }
    }

    pub fn snapshot(&self) -> (Vec<Message>, Option<String>) {
        let store = self.state.sessions.lock().unwrap();
        store
            .get(&self.session_id)
            .map(|session| (session.messages.clone(), session.summary.clone()))
            .unwrap_or_else(|| (Vec::new(), None))
    }

    pub fn append_user_message(&self, text: String) {
        self.mutate_and_persist(|session| {
            session.messages.push(Message::user(text));
            if session.title == "新对话" {
                session.title = store::derive_title(&session.messages);
            }
        });
    }

    pub fn append_message(&self, message: Message) {
        self.mutate_and_persist(|session| {
            session.messages.push(message);
        });
    }

    pub fn replace_messages(&self, messages: Vec<Message>) {
        self.mutate_and_persist(|session| {
            session.messages = messages;
        });
    }

    pub fn replace_messages_and_summary(&self, messages: Vec<Message>, summary: Option<String>) {
        self.mutate_and_persist(|session| {
            session.messages = messages;
            session.summary = summary;
        });
    }

    fn mutate_and_persist(&self, mutate: impl FnOnce(&mut store::Session)) {
        let changed = {
            let mut store = self.state.sessions.lock().unwrap();
            if let Some(session) = store.get_mut(&self.session_id) {
                mutate(session);
                session.updated_at = store::now_millis();
                true
            } else {
                false
            }
        };
        if changed {
            self.state.persist_sessions();
        }
    }
}

#[derive(Clone)]
pub struct TurnEventEmitter<'a> {
    app: AppHandle,
    state: &'a crate::AppState,
}

impl<'a> TurnEventEmitter<'a> {
    pub fn new(app: &AppHandle, state: &'a crate::AppState) -> Self {
        TurnEventEmitter {
            app: app.clone(),
            state,
        }
    }

    pub fn assistant_start(&self) {
        self.emit_legacy_and_unified("assistant-start", "assistant_start", ());
    }

    pub fn assistant_delta(&self, delta: &str) {
        self.emit_legacy_and_unified("assistant-delta", "assistant_delta", delta.to_string());
    }

    /// 推理型模型在正文之前输出的思维链增量。单独走 `assistant-reasoning` 事件，
    /// 前端渲染成「思考中」气泡，消除推理阶段的界面静默。
    pub fn assistant_reasoning(&self, delta: &str) {
        self.emit_legacy_and_unified(
            "assistant-reasoning",
            "assistant_reasoning",
            delta.to_string(),
        );
    }

    pub fn assistant_done(&self, text: String) {
        self.emit_legacy_and_unified("assistant-done", "assistant_done", text);
    }

    pub fn assistant_error(&self, event: AssistantErrorEvent) {
        self.emit_legacy_and_unified("assistant-error", "assistant_error", event);
    }

    pub fn assistant_interrupted(&self) {
        self.emit_legacy_and_unified("assistant-interrupted", "assistant_interrupted", ());
    }

    pub fn tool_start(&self, event: ToolStartEvent) {
        self.emit_legacy_and_unified("tool-start", "tool_start", event);
    }

    pub fn tool_end(&self, event: ToolEndEvent) {
        self.emit_legacy_and_unified("tool-end", "tool_end", event);
    }

    fn emit_legacy_and_unified<T>(&self, legacy_event: &str, kind: &'static str, payload: T)
    where
        T: Serialize + Clone,
    {
        let _ = self.app.emit(legacy_event, payload.clone());
        let _ = self.app.emit(
            "agent-event",
            AgentEventEnvelope {
                kind,
                turn: current_turn_context(self.state),
                timestamp: store::now_millis(),
                payload,
            },
        );
    }
}

pub fn panel_state(state: &crate::AppState) -> SessionEnginePanelState {
    let runtime = state.session_engine.lock().unwrap().clone();
    SessionEnginePanelState {
        busy: state.busy.load(Ordering::SeqCst),
        cancel_requested: state.cancel.load(Ordering::Relaxed),
        active_turn: runtime.active_turn,
        last_turn: runtime.last_turn,
    }
}

pub fn begin_turn(
    app: &AppHandle,
    state: &crate::AppState,
    start: TurnStart,
) -> Result<TurnHandle, String> {
    if state.busy.swap(true, Ordering::SeqCst) {
        return Err("正在处理上一条消息，请稍候。".to_string());
    }

    state.cancel.store(false, Ordering::Relaxed);
    let now = store::now_millis();
    let turn = TurnRunState {
        id: new_turn_id(),
        session_id: start.session_id,
        entrypoint: start.entrypoint,
        status: TurnStatus::Running,
        input_preview: preview(&start.input),
        workflow_run_id: start.workflow_run_id,
        agent_names: start.agent_names,
        started_at: now,
        updated_at: now,
        completed_at: None,
        error: None,
    };
    let handle = TurnHandle {
        id: turn.id.clone(),
    };

    {
        let mut runtime = state.session_engine.lock().unwrap();
        runtime.active_turn = Some(turn);
    }
    emit_update(app, state);
    Ok(handle)
}

pub fn finish_turn(
    app: &AppHandle,
    state: &crate::AppState,
    handle: &TurnHandle,
    status: TurnStatus,
    error: Option<String>,
) {
    let now = store::now_millis();
    {
        let mut runtime = state.session_engine.lock().unwrap();
        if let Some(active) = runtime.active_turn.as_mut() {
            if active.id == handle.id {
                active.status = status;
                active.updated_at = now;
                active.completed_at = Some(now);
                active.error = error;
                runtime.last_turn = runtime.active_turn.take();
            }
        }
    }
    state.busy.store(false, Ordering::SeqCst);
    emit_update(app, state);
}

pub fn request_interrupt(app: &AppHandle, state: &crate::AppState) {
    state.cancel.store(true, Ordering::Relaxed);
    {
        let mut runtime = state.session_engine.lock().unwrap();
        if let Some(active) = runtime.active_turn.as_mut() {
            if active.status == TurnStatus::Running {
                active.status = TurnStatus::Cancelling;
                active.updated_at = store::now_millis();
            }
        }
    }
    emit_update(app, state);
}

pub fn emit_update(app: &AppHandle, state: &crate::AppState) {
    let _ = app.emit("session-engine-updated", panel_state(state));
}

fn new_turn_id() -> String {
    format!("turn_{}", store::new_session_id().trim_start_matches("s_"))
}

fn current_turn_context(state: &crate::AppState) -> Option<TurnEventContext> {
    state
        .session_engine
        .lock()
        .unwrap()
        .active_turn
        .as_ref()
        .map(|turn| TurnEventContext {
            id: turn.id.clone(),
            session_id: turn.session_id.clone(),
            status: turn.status.clone(),
        })
}

fn preview(input: &str) -> String {
    let clean = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.chars().count() <= INPUT_PREVIEW_CHARS {
        clean
    } else {
        let head: String = clean.chars().take(INPUT_PREVIEW_CHARS).collect();
        format!("{head}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_collapses_whitespace_and_truncates() {
        let input = format!(" hello\n{}\tworld ", "a".repeat(180));
        let out = preview(&input);
        assert!(!out.contains('\n'));
        assert!(!out.contains('\t'));
        assert!(out.ends_with('…'));
        assert!(out.chars().count() <= INPUT_PREVIEW_CHARS + 1);
    }

    #[test]
    fn finish_moves_active_turn_to_last_turn() {
        let now = store::now_millis();
        let mut runtime = SessionEngineState {
            active_turn: Some(TurnRunState {
                id: "turn_test".to_string(),
                session_id: "s_test".to_string(),
                entrypoint: TurnEntrypoint::Send,
                status: TurnStatus::Running,
                input_preview: "hello".to_string(),
                workflow_run_id: None,
                agent_names: Vec::new(),
                started_at: now,
                updated_at: now,
                completed_at: None,
                error: None,
            }),
            last_turn: None,
        };

        let active = runtime.active_turn.as_mut().unwrap();
        active.status = TurnStatus::Completed;
        active.completed_at = Some(now);
        runtime.last_turn = runtime.active_turn.take();

        assert!(runtime.active_turn.is_none());
        assert_eq!(
            runtime.last_turn.as_ref().map(|turn| &turn.status),
            Some(&TurnStatus::Completed)
        );
    }

    #[test]
    fn session_turn_store_appends_user_and_derives_title() {
        let dir = std::env::temp_dir().join(format!(
            "demiurge_session_engine_{}",
            store::new_session_id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let state = crate::AppState::new(reqwest::Client::new());
        *state.data_dir.lock().unwrap() = dir.clone();

        let session = store::Session::new();
        let session_id = session.id.clone();
        {
            let mut sessions = state.sessions.lock().unwrap();
            sessions.active = session_id.clone();
            sessions.sessions.push(session);
        }

        let turn_store = SessionTurnStore::new(&state, session_id.clone());
        turn_store.append_user_message("please inspect the repo".to_string());

        let sessions = state.sessions.lock().unwrap();
        let session = sessions.get(&session_id).unwrap();
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].role, "user");
        assert_eq!(session.title, "please inspect the repo");
        // persist_sessions 现在走后台线程落盘，轮询等待写入完成。
        let mut persisted = false;
        for _ in 0..300 {
            if dir.join("sessions.json").exists() {
                persisted = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(persisted, "sessions.json 应由后台写盘线程持久化");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
