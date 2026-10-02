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
    /// Identifies one answer; Goal continuations share a turn but not an answer.
    pub response_id: String,
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
    pub session_id: String,
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
            session.append_message(Message::user(text));
            if session.title == "新对话" {
                session.title = store::derive_title(&session.messages);
            }
        });
    }

    pub fn append_message(&self, message: Message) {
        self.mutate_and_persist(|session| {
            session.append_message(message);
        });
    }

    /// Append a complete provider request to the session audit log. `tools`
    /// is kept as the exact JSON schema used by the provider so this method
    /// can be called directly from the runner without coupling core to
    /// serde_json.
    pub fn append_model_request(
        &self,
        messages: &[Message],
        tools: &Value,
        provider: &str,
        model: &str,
        purpose: &str,
    ) {
        let tools_json = serde_json::to_string(tools).unwrap_or_else(|_| tools.to_string());
        let full_audit = std::env::var("DEMIURGE_MODEL_AUDIT_MODE")
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "full" | "inline" | "1" | "true"
                )
            })
            .unwrap_or(false);
        self.mutate_and_persist(|session| {
            if full_audit {
                session.append_model_request(
                    messages.to_vec(),
                    tools_json,
                    provider.to_string(),
                    model.to_string(),
                    purpose.to_string(),
                );
            } else {
                session.append_model_request_compact(
                    messages.to_vec(),
                    tools_json,
                    provider.to_string(),
                    model.to_string(),
                    purpose.to_string(),
                );
            }
        });
    }

    pub fn commit_compaction(
        &self,
        expected_messages: &[Message],
        expected_summary: &Option<String>,
        messages: Vec<Message>,
        summary: Option<String>,
    ) -> Result<bool, String> {
        {
            let mut store = self.state.sessions.lock().unwrap();
            let session = store
                .get_mut(&self.session_id)
                .ok_or_else(|| "The target session no longer exists.".to_string())?;
            if session.messages != expected_messages || &session.summary != expected_summary {
                return Ok(false);
            }
            session.replace_projection(messages, summary);
        }
        self.state.persist_sessions();
        Ok(true)
    }

    fn mutate_and_persist(&self, mutate: impl FnOnce(&mut store::Session)) {
        let changed = {
            let mut store = self.state.sessions.lock().unwrap();
            if let Some(session) = store.get_mut(&self.session_id) {
                mutate(session);
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
struct ResponseEventIdentity {
    turn: Option<TurnEventContext>,
    response_id: String,
}

impl ResponseEventIdentity {
    fn new(turn: Option<TurnEventContext>) -> Self {
        Self {
            turn,
            response_id: format!(
                "response_{}",
                store::new_session_id().trim_start_matches("s_")
            ),
        }
    }

    fn context(&self, current: Option<TurnEventContext>) -> Option<TurnEventContext> {
        let mut captured = self.turn.clone()?;
        // Status may advance to cancelling; ownership never follows a new turn.
        if let Some(current) = current {
            if current.id == captured.id && current.session_id == captured.session_id {
                captured.status = current.status;
            }
        }
        Some(captured)
    }
}

#[derive(Clone)]
pub struct TurnEventEmitter<'a> {
    app: AppHandle,
    state: &'a crate::AppState,
    identity: ResponseEventIdentity,
}

impl<'a> TurnEventEmitter<'a> {
    pub fn new(app: &AppHandle, state: &'a crate::AppState) -> Self {
        TurnEventEmitter {
            app: app.clone(),
            state,
            identity: ResponseEventIdentity::new(current_turn_context(state)),
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
                turn: self.identity.context(current_turn_context(self.state)),
                response_id: self.identity.response_id.clone(),
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
    // Serialize turn registration with session deletion. A sender may have
    // captured the session id immediately before another command deletes that
    // session; validating while holding the engine lock prevents registering a
    // turn whose storage target no longer exists.
    let mut runtime = state.session_engine.lock().unwrap();
    if state
        .sessions
        .lock()
        .unwrap()
        .get(&start.session_id)
        .is_none()
    {
        state.busy.store(false, Ordering::SeqCst);
        return Err("The target session no longer exists.".to_string());
    }

    let now = store::now_millis();
    let turn = TurnRunState {
        id: new_turn_id(),
        session_id: start.session_id.clone(),
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
        session_id: start.session_id,
    };

    runtime.active_turn = Some(turn);
    drop(runtime);
    emit_update(app, state);
    Ok(handle)
}

/// Returns whether deleting `session_id` would remove the storage target of a
/// currently running turn. Callers that need an atomic check-and-delete must
/// keep the same `SessionEngineState` lock held until session removal finishes.
pub fn blocks_session_deletion(runtime: &SessionEngineState, session_id: &str) -> bool {
    runtime.active_turn.as_ref().is_some_and(|turn| {
        turn.session_id == session_id
            && matches!(turn.status, TurnStatus::Running | TurnStatus::Cancelling)
    })
}

/// Resolves the session that owns work executed inside the current turn.
/// Tool implementations that cannot receive a session id directly must use
/// this helper instead of the mutable sidebar selection.
pub fn execution_session_id(state: &crate::AppState) -> String {
    let runtime = state.session_engine.lock().unwrap();
    if let Some(turn) = runtime.active_turn.as_ref() {
        return turn.session_id.clone();
    }
    state.sessions.lock().unwrap().active.clone()
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
    use std::sync::{mpsc, Arc};

    #[test]
    fn answers_get_distinct_identity_while_emitter_clones_keep_the_same_answer() {
        let turn = TurnEventContext {
            id: "turn_goal".to_string(),
            session_id: "session_a".to_string(),
            status: TurnStatus::Running,
        };
        let first = ResponseEventIdentity::new(Some(turn.clone()));
        let cloned = first.clone();
        let continuation = ResponseEventIdentity::new(Some(turn));
        assert_eq!(first.response_id, cloned.response_id);
        assert_ne!(first.response_id, continuation.response_id);
        assert_eq!(
            first.turn.as_ref().unwrap().id,
            continuation.turn.as_ref().unwrap().id
        );
    }

    #[test]
    fn delayed_emitter_keeps_its_owner_after_engine_navigation_or_completion() {
        let original = TurnEventContext {
            id: "turn_a".to_string(),
            session_id: "session_a".to_string(),
            status: TurnStatus::Running,
        };
        let identity = ResponseEventIdentity::new(Some(original.clone()));
        let cancelling = identity
            .context(Some(TurnEventContext {
                status: TurnStatus::Cancelling,
                ..original
            }))
            .unwrap();
        assert_eq!(cancelling.status, TurnStatus::Cancelling);

        let delayed = identity
            .context(Some(TurnEventContext {
                id: "turn_b".to_string(),
                session_id: "session_b".to_string(),
                status: TurnStatus::Running,
            }))
            .unwrap();
        assert_eq!(delayed.id, "turn_a");
        assert_eq!(delayed.session_id, "session_a");
        assert_eq!(identity.context(None).unwrap().id, "turn_a");
        assert!(ResponseEventIdentity::new(None)
            .context(identity.turn.clone())
            .is_none());
    }

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
        assert_eq!(session.events.len(), 1);
        assert_eq!(session.events[0].seq, 1);
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

    #[test]
    fn session_turn_store_records_complete_model_request_without_changing_projection() {
        let dir = std::env::temp_dir().join(format!(
            "demiurge_session_model_request_{}",
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

        let request_messages = vec![Message::system("system"), Message::user("hello")];
        let tools = serde_json::json!([{"type": "function", "function": {"name": "grep"}}]);
        let turn_store = SessionTurnStore::new(&state, session_id.clone());
        turn_store.append_model_request(
            &request_messages,
            &tools,
            "openai",
            "test-model",
            "agent_turn",
        );

        let sessions = state.sessions.lock().unwrap();
        let session = sessions.get(&session_id).unwrap();
        assert!(session.messages.is_empty());
        assert!(session.summary.is_none());
        assert_eq!(session.events.len(), 1);
        assert!(matches!(
            &session.events[0].kind,
            store::SessionEventKind::ModelRequest {
                messages,
                tools: stored_tools,
                provider,
                model,
                purpose,
            } if messages == &request_messages
                && stored_tools == &serde_json::to_string(&tools).unwrap()
                && provider == "openai"
                && model == "test-model"
                && purpose == "agent_turn"
        ));
        drop(sessions);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn compaction_commit_rejects_a_stale_snapshot_without_losing_messages() {
        let dir = std::env::temp_dir().join(format!(
            "demiurge_session_compaction_{}",
            store::new_session_id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let state = crate::AppState::new(reqwest::Client::new());
        *state.data_dir.lock().unwrap() = dir.clone();

        let mut session = store::Session::new();
        session.messages = vec![Message::user("old"), Message::assistant_text("reply")];
        let session_id = session.id.clone();
        {
            let mut sessions = state.sessions.lock().unwrap();
            sessions.active = session_id.clone();
            sessions.sessions.push(session);
        }

        let turn_store = SessionTurnStore::new(&state, session_id.clone());
        let (snapshot, summary) = turn_store.snapshot();
        turn_store.append_message(Message::user("concurrent"));
        let committed = turn_store
            .commit_compaction(
                &snapshot,
                &summary,
                vec![Message::assistant_text("kept")],
                Some("summary".to_string()),
            )
            .unwrap();

        assert!(!committed);
        let sessions = state.sessions.lock().unwrap();
        let current = sessions.get(&session_id).unwrap();
        assert_eq!(current.messages.len(), 3);
        assert_eq!(current.messages[2].content.as_deref(), Some("concurrent"));
        assert!(current.summary.is_none());
        drop(sessions);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delayed_turn_stays_bound_to_its_start_session_and_cannot_be_deleted() {
        let root = std::env::temp_dir().join(format!(
            "demiurge_turn_session_binding_{}",
            store::new_session_id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let state = Arc::new(crate::AppState::new(reqwest::Client::new()));
        *state.data_dir.lock().unwrap() = root.clone();
        *state.sandbox_dir.lock().unwrap() = root.clone();

        let mut session_a = store::Session::new();
        session_a.id = "session-a".to_string();
        session_a.workspace_path = root.to_string_lossy().to_string();
        let mut session_b = store::Session::new();
        session_b.id = "session-b".to_string();
        session_b.workspace_path = session_a.workspace_path.clone();
        {
            let mut sessions = state.sessions.lock().unwrap();
            sessions.active = session_a.id.clone();
            sessions.sessions = vec![session_a, session_b];
        }

        let now = store::now_millis();
        state.busy.store(true, Ordering::SeqCst);
        state.session_engine.lock().unwrap().active_turn = Some(TurnRunState {
            id: "turn-a".to_string(),
            session_id: "session-a".to_string(),
            entrypoint: TurnEntrypoint::Send,
            status: TurnStatus::Running,
            input_preview: "delayed".to_string(),
            workflow_run_id: None,
            agent_names: Vec::new(),
            started_at: now,
            updated_at: now,
            completed_at: None,
            error: None,
        });

        // Hold the worker before its first write, mirroring an awaited provider
        // initialization. The UI changes active session while it is suspended.
        let (ready_tx, ready_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let worker_state = Arc::clone(&state);
        let worker = std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            resume_rx.recv().unwrap();
            let turn_store = SessionTurnStore::new(&worker_state, "session-a".to_string());
            turn_store.append_user_message("message for A".to_string());
            turn_store.append_message(Message::tool_result(
                "call-a",
                "read_file",
                "tool output for A",
            ));
            turn_store.append_message(Message::assistant_text("reply for A"));
        });

        ready_rx.recv().unwrap();
        state.sessions.lock().unwrap().active = "session-b".to_string();
        assert_eq!(execution_session_id(&state), "session-a");

        let running_error =
            crate::biz::session::SessionBiz::delete(&state, "session-a".to_string())
                .expect_err("a running turn must keep ownership of its start session");
        assert!(running_error.contains("本轮所属会话"));
        state
            .session_engine
            .lock()
            .unwrap()
            .active_turn
            .as_mut()
            .unwrap()
            .status = TurnStatus::Cancelling;
        assert!(crate::biz::session::SessionBiz::delete(&state, "session-a".to_string()).is_err());

        resume_tx.send(()).unwrap();
        worker.join().unwrap();

        let sessions = state.sessions.lock().unwrap();
        let a = sessions.get("session-a").unwrap();
        let b = sessions.get("session-b").unwrap();
        assert_eq!(
            a.messages
                .iter()
                .map(|message| message.role.as_str())
                .collect::<Vec<_>>(),
            vec!["user", "tool", "assistant"]
        );
        assert!(a.messages[1]
            .content
            .as_deref()
            .is_some_and(|content| content.contains("tool output for A")));
        assert_eq!(a.messages[2].content.as_deref(), Some("reply for A"));
        assert!(b.messages.is_empty());
        drop(sessions);
        drop(state);
        let _ = std::fs::remove_dir_all(root);
    }
}
