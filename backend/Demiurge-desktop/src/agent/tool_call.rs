//! One tool call: target resolution, permission, confirmation, execution and reporting.
//! The model loop only consumes the paired result; UI and tool side effects use a private seam.
use super::conversation::{Message, ToolCall, ToolExecutionRecord, ToolExecutionStatus};
use super::{session_engine, workflow_journal};
use crate::{permission, tools, AppState};
use permission::{PermissionContext, PermissionRequest, PermissionResponse};
use serde_json::{json, Value};
use std::future::Future;
use std::sync::atomic::Ordering;
use std::time::Instant;
use tauri::{AppHandle, Emitter};

const UI_RESULT_CAP: usize = 2000;

pub(crate) struct ToolCallOutcome {
    pub message: Message,
    pub ui_result: String,
}

trait ToolCallIo: Sync {
    fn tool_start(&self, event: session_engine::ToolStartEvent);
    fn tool_end(&self, event: session_engine::ToolEndEvent);
    fn plan_updated(&self);
    fn confirm(
        &self,
        request: PermissionRequest<'_>,
    ) -> impl Future<Output = PermissionResponse> + Send;
    fn execute(
        &self,
        name: &str,
        args: Value,
    ) -> impl Future<Output = Result<String, String>> + Send;
}

struct DesktopIo<'a> {
    app: &'a AppHandle,
    state: &'a AppState,
    events: &'a session_engine::TurnEventEmitter<'a>,
}

impl ToolCallIo for DesktopIo<'_> {
    fn tool_start(&self, event: session_engine::ToolStartEvent) {
        self.events.tool_start(event);
    }
    fn tool_end(&self, event: session_engine::ToolEndEvent) {
        self.events.tool_end(event);
    }
    fn plan_updated(&self) {
        let _ = self.app.emit(
            "plan-updated",
            self.state.plan_state.lock().unwrap().clone(),
        );
    }
    async fn confirm(&self, request: PermissionRequest<'_>) -> PermissionResponse {
        permission::confirm(self.app, self.state, request).await
    }
    async fn execute(&self, name: &str, args: Value) -> Result<String, String> {
        tools::execute(self.state, name, args).await
    }
}

pub(crate) async fn execute(
    app: &AppHandle,
    state: &AppState,
    context: &PermissionContext,
    events: &session_engine::TurnEventEmitter<'_>,
    call: &ToolCall,
    workflow_run_id: Option<&str>,
) -> ToolCallOutcome {
    execute_with_io(
        state,
        context,
        call,
        workflow_run_id,
        &DesktopIo { app, state, events },
    )
    .await
}

async fn execute_with_io(
    state: &AppState,
    permission_context: &PermissionContext,
    tc: &ToolCall,
    workflow_run_id: Option<&str>,
    io: &impl ToolCallIo,
) -> ToolCallOutcome {
    let sid = permission_context.session_id.as_deref().unwrap_or_default();
    let name = tc.function.name.clone();
    if state.cancel.load(Ordering::Relaxed) {
        let result = "[已被用户中断，未执行]".to_string();
        return ToolCallOutcome {
            ui_result: result.clone(),
            message: Message::tool_result_with_execution(
                tc.id.clone(),
                name,
                result,
                ToolExecutionRecord {
                    status: ToolExecutionStatus::Denied,
                    error: Some("Interrupted before execution".to_string()),
                    duration_ms: 0,
                    affected_paths: Vec::new(),
                },
            ),
        };
    }
    let args: serde_json::Value =
        serde_json::from_str(&tc.function.arguments).unwrap_or_else(|_| json!({}));
    let authorization = tools::authorization_target_for_state(state, &name, &args);
    let permission_name = authorization.name;
    let permission_args = authorization.args;
    let tool_def = authorization.definition;
    let wrapped_by = authorization.wrapped_by;
    let preview = tools::confirmation_preview(state, &permission_name, permission_args.clone());
    let affected_paths = tools::affected_paths(&permission_name, &permission_args);

    io.tool_start(session_engine::ToolStartEvent {
        tool_call_id: tc.id.clone(),
        name: name.clone(),
        args: args.clone(),
        description: tool_def.as_ref().map(|t| t.description),
        risk: tool_def.as_ref().map(|t| t.risk),
        permission_effect: tool_def.as_ref().map(|t| t.permission.effect),
        concurrency: tool_def.as_ref().map(|t| t.concurrency),
        output_policy: tool_def.as_ref().map(|t| t.output_policy),
        preview: preview.clone(),
        affected_paths: affected_paths.clone(),
    });
    if let Some(run_id) = workflow_run_id {
        let _ = workflow_journal::append(
            state,
            run_id,
            "tool_started",
            json!({
                "tool_call_id": tc.id.clone(),
                "name": name.clone(),
                "args": args.clone(),
                "permission_target": permission_name.clone(),
                "wrapped_by": wrapped_by,
            }),
        );
    }

    // 权限门（confirm 等待期间若用户点「停止」，interrupt 会立即唤醒并返回 deny-once）
    let default_policy = tools::permission_policy_for_state(state, &permission_name);
    let risk = tool_def
        .as_ref()
        .map(|t| t.risk)
        .unwrap_or(tools::ToolRisk::Privileged);
    let mut decision = permission::decide_for_mode(
        state,
        &permission_context,
        &permission_name,
        default_policy,
        risk,
    );
    permission::audit(state, &permission_context, &permission_name, &decision);
    let allowed = match decision.effect {
        tools::PermissionEffect::Allow => true,
        tools::PermissionEffect::Deny => false,
        tools::PermissionEffect::Ask => {
            let pretty = serde_json::to_string_pretty(&permission_args).unwrap_or_default();
            let description = tool_def
                .as_ref()
                .map(|t| t.description)
                .unwrap_or("未知工具");
            let risk = tool_def
                .as_ref()
                .map(|t| t.risk)
                .unwrap_or(tools::ToolRisk::Privileged);
            let summary =
                tools::permission_summary_for_state(state, &permission_name, &permission_args);
            let response = io
                .confirm(PermissionRequest {
                    session_id: &sid,
                    tool: &permission_name,
                    args_pretty: &pretty,
                    description,
                    risk,
                    decision: decision.clone(),
                    summary,
                    preview: preview.clone(),
                    affected_paths: affected_paths.clone(),
                })
                .await;
            let remembered_scope = permission::remember_response(
                state,
                &permission_context,
                &permission_name,
                &response,
            );
            let (effective_scope, persistence_error) = match remembered_scope {
                Ok(scope) => (scope, None),
                Err(error) => (tools::PermissionScope::Once, Some(error)),
            };
            decision.effect = if response.allow {
                tools::PermissionEffect::Allow
            } else {
                tools::PermissionEffect::Deny
            };
            decision.scope = effective_scope;
            decision.source = permission::PermissionDecisionSource::UserOverride;
            let base_reason = if response.allow {
                "用户在确认弹窗中允许本次操作。".to_string()
            } else {
                "用户在确认弹窗中拒绝本次操作。".to_string()
            };
            decision.reason = match persistence_error {
                None => base_reason,
                Some(error) => format!("{base_reason} 规则未持久化：{error}"),
            };
            permission::audit(state, &permission_context, &permission_name, &decision);
            response.allow
        }
    };

    let interrupted = state.cancel.load(Ordering::Relaxed);
    let tool_started_at = Instant::now();
    let (result, execution_status, execution_error) = if interrupted {
        (
            "[Interrupted before execution]".to_string(),
            ToolExecutionStatus::Denied,
            Some("Interrupted before execution".to_string()),
        )
    } else if !allowed {
        (
            "[User denied this operation]".to_string(),
            ToolExecutionStatus::Denied,
            Some("User denied this operation".to_string()),
        )
    } else {
        match io.execute(&name, args.clone()).await {
            Ok(s) => {
                if name == "write_plan" {
                    io.plan_updated();
                }
                (s, ToolExecutionStatus::Ok, None)
            }
            Err(e) => (format!("Error: {e}"), ToolExecutionStatus::Failed, Some(e)),
        }
    };
    let tool_ok = execution_status == ToolExecutionStatus::Ok;
    let denied = execution_status == ToolExecutionStatus::Denied;
    let duration_ms = tool_started_at.elapsed().as_millis() as u64;
    let error_hint = tool_error_hint(&permission_name, &result, tool_ok, denied);
    let source_quality = source_quality_hint(&permission_name, &result, tool_ok);

    io.tool_end(session_engine::ToolEndEvent {
        tool_call_id: tc.id.clone(),
        name: name.clone(),
        ok: tool_ok,
        denied,
        result: truncate_ui(&result),
        duration_ms,
        error_hint: error_hint.clone(),
        source_quality: source_quality.clone(),
    });
    if let Some(run_id) = workflow_run_id {
        let _ = workflow_journal::append(
            state,
            run_id,
            "tool_done",
            json!({
                "tool_call_id": tc.id.clone(),
                "name": name.clone(),
                "permission_target": permission_name.clone(),
                "ok": tool_ok,
                "denied": denied,
                "status": execution_status.clone(),
                "error": execution_error.clone(),
                "duration_ms": duration_ms,
                "affected_paths": affected_paths.clone(),
                "result": truncate_ui(&result),
            }),
        );
    }

    ToolCallOutcome {
        ui_result: truncate_ui(&result),
        message: Message::tool_result_with_execution(
            tc.id.clone(),
            name,
            result,
            ToolExecutionRecord {
                status: execution_status,
                error: execution_error,
                duration_ms,
                affected_paths,
            },
        ),
    }
}

fn truncate_ui(s: &str) -> String {
    if s.chars().count() <= UI_RESULT_CAP {
        s.to_string()
    } else {
        let head: String = s.chars().take(UI_RESULT_CAP).collect();
        format!("{head}…（已截断，共 {} 字）", s.chars().count())
    }
}

fn tool_error_hint(name: &str, result: &str, ok: bool, denied: bool) -> Option<String> {
    if denied {
        return Some("Permission denied before execution. Change the permission rule or retry and allow once.".to_string());
    }
    if ok {
        return None;
    }
    let lower = result.to_ascii_lowercase();
    if name == "web_search" || name == "web_fetch" {
        if lower.contains("api key") || lower.contains("401") || lower.contains("403") {
            return Some("Search/fetch provider authentication failed. Check the configured API key or switch provider.".to_string());
        }
        if lower.contains("timeout") || lower.contains("timed out") {
            return Some("Network lookup timed out. Retry once, reduce result depth, or switch to another search provider.".to_string());
        }
        if lower.contains("http 429") || lower.contains("rate limit") {
            return Some(
                "Provider rate limit hit. Wait briefly or switch to another search provider."
                    .to_string(),
            );
        }
        return Some("Network tool failed. Check provider settings, allowed domains, and local connectivity before retrying.".to_string());
    }
    if lower.contains("not found") || lower.contains("no such file") {
        return Some("Target path was not found. Check the path and retry with an exact sandbox-relative path.".to_string());
    }
    if lower.contains("permission") || lower.contains("access") {
        return Some("The operation was blocked by filesystem or process permissions. Check the target path and permission rule.".to_string());
    }
    Some("Review the tool arguments and retry after correcting the failing input.".to_string())
}

fn source_quality_hint(name: &str, result: &str, ok: bool) -> Option<serde_json::Value> {
    if !ok || !matches!(name, "web_search" | "web_fetch") {
        return None;
    }
    let source_count = crate::tools::source_link_count(result);
    let (level, hint) = if source_count >= 3 {
        (
            "strong",
            "Enough source links were returned for cross-checking.",
        )
    } else if source_count >= 1 {
        (
            "limited",
            "Only a small number of source links were returned. Consider another query or provider if the answer needs verification.",
        )
    } else {
        (
            "none",
            "No usable source links were found. Retry with a narrower query or a different search provider.",
        )
    };
    Some(json!({
        "level": level,
        "source_count": source_count,
        "hint": hint,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Mutex;

    struct Fixture {
        state: AppState,
        root: PathBuf,
        context: PermissionContext,
    }

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "demiurge_tool_transaction_{}",
                crate::store::new_session_id()
            ));
            let project = root.join("project");
            std::fs::create_dir_all(&project).unwrap();
            std::fs::create_dir_all(root.join("data")).unwrap();
            let state = AppState::new(reqwest::Client::new());
            *state.data_dir.lock().unwrap() = root.join("data");
            *state.sandbox_dir.lock().unwrap() = project.clone();
            let mut session = crate::store::Session::new();
            session.workspace_path = project.to_string_lossy().into_owned();
            let id = session.id.clone();
            {
                let mut sessions = state.sessions.lock().unwrap();
                sessions.active = id.clone();
                sessions.sessions.push(session);
            }
            let context = permission::context_for_session(&state, &id);
            assert!(context.boundary_error.is_none());
            Self {
                state,
                root,
                context,
            }
        }

        fn write_call(&self) -> ToolCall {
            ToolCall {
                id: "write-1".to_string(),
                kind: "function".to_string(),
                function: super::super::conversation::FunctionCall {
                    name: "write_file".to_string(),
                    arguments: json!({ "path": "note.md", "content": "approved" }).to_string(),
                },
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    struct TestIo<'a> {
        state: &'a AppState,
        response: PermissionResponse,
        cancel_on_confirm: bool,
        executions: AtomicUsize,
        confirmations: Mutex<Vec<String>>,
        starts: Mutex<Vec<session_engine::ToolStartEvent>>,
        ends: Mutex<Vec<session_engine::ToolEndEvent>>,
    }

    impl<'a> TestIo<'a> {
        fn new(state: &'a AppState, allow: bool) -> Self {
            Self {
                state,
                response: PermissionResponse {
                    allow,
                    scope: tools::PermissionScope::Once,
                },
                cancel_on_confirm: false,
                executions: AtomicUsize::new(0),
                confirmations: Mutex::new(Vec::new()),
                starts: Mutex::new(Vec::new()),
                ends: Mutex::new(Vec::new()),
            }
        }
    }

    impl ToolCallIo for TestIo<'_> {
        fn tool_start(&self, event: session_engine::ToolStartEvent) {
            self.starts.lock().unwrap().push(event);
        }
        fn tool_end(&self, event: session_engine::ToolEndEvent) {
            self.ends.lock().unwrap().push(event);
        }
        fn plan_updated(&self) {}
        async fn confirm(&self, request: PermissionRequest<'_>) -> PermissionResponse {
            self.confirmations
                .lock()
                .unwrap()
                .push(request.tool.to_string());
            if self.cancel_on_confirm {
                self.state.cancel.store(true, Ordering::Relaxed);
            }
            self.response.clone()
        }
        async fn execute(&self, name: &str, args: Value) -> Result<String, String> {
            self.executions.fetch_add(1, Ordering::Relaxed);
            tools::execute(self.state, name, args).await
        }
    }

    #[tokio::test]
    async fn cancellation_after_approval_prevents_file_write_and_pairs_result() {
        let fixture = Fixture::new();
        let call = fixture.write_call();
        let mut io = TestIo::new(&fixture.state, true);
        io.cancel_on_confirm = true;
        let outcome = execute_with_io(&fixture.state, &fixture.context, &call, None, &io).await;
        assert_eq!(
            io.executions.load(Ordering::Relaxed),
            0,
            "approval must not override a later cancellation"
        );
        assert!(!fixture.root.join("project/note.md").exists());
        assert_eq!(outcome.message.tool_call_id.as_deref(), Some("write-1"));
        let record = outcome.message.tool_execution.unwrap();
        assert_eq!(record.status, ToolExecutionStatus::Denied);
        assert_eq!(
            record.error.as_deref(),
            Some("Interrupted before execution")
        );
        assert_eq!(io.ends.lock().unwrap().len(), 1);
    }
    #[tokio::test]
    async fn approved_write_has_one_side_effect_and_a_paired_success() {
        let fixture = Fixture::new();
        let io = TestIo::new(&fixture.state, true);
        let call = fixture.write_call();
        let outcome = execute_with_io(&fixture.state, &fixture.context, &call, None, &io).await;
        assert_eq!(
            std::fs::read_to_string(fixture.root.join("project/note.md")).unwrap(),
            "approved"
        );
        assert_eq!(io.executions.load(Ordering::Relaxed), 1);
        assert_eq!(*io.confirmations.lock().unwrap(), vec!["write_file"]);
        assert_eq!(
            outcome.message.tool_call_id.as_deref(),
            Some(call.id.as_str())
        );
        assert_eq!(
            outcome.message.tool_execution.unwrap().status,
            ToolExecutionStatus::Ok
        );
        assert_eq!(io.starts.lock().unwrap().len(), 1);
        let ends = io.ends.lock().unwrap();
        assert_eq!(ends.len(), 1);
        assert!(ends[0].ok);
        assert!(!ends[0].denied);
    }

    #[tokio::test]
    async fn rejected_write_and_pre_cancelled_call_never_execute() {
        let fixture = Fixture::new();
        let io = TestIo::new(&fixture.state, false);
        let call = fixture.write_call();
        let outcome = execute_with_io(&fixture.state, &fixture.context, &call, None, &io).await;
        assert_eq!(
            outcome.message.tool_execution.unwrap().status,
            ToolExecutionStatus::Denied
        );
        assert_eq!(io.executions.load(Ordering::Relaxed), 0);
        assert_eq!(io.ends.lock().unwrap().len(), 1);
        assert!(io.ends.lock().unwrap()[0].denied);
        fixture.state.cancel.store(true, Ordering::Relaxed);
        let io = TestIo::new(&fixture.state, true);
        let outcome = execute_with_io(&fixture.state, &fixture.context, &call, None, &io).await;
        assert_eq!(outcome.message.tool_call_id.as_deref(), Some("write-1"));
        assert_eq!(
            outcome.message.tool_execution.unwrap().status,
            ToolExecutionStatus::Denied
        );
        assert_eq!(io.executions.load(Ordering::Relaxed), 0);
        assert!(io.confirmations.lock().unwrap().is_empty());
        assert!(io.starts.lock().unwrap().is_empty());
        assert!(!fixture.root.join("project/note.md").exists());
    }

    #[tokio::test]
    async fn failed_tool_keeps_error_and_call_identity() {
        let fixture = Fixture::new();
        let io = TestIo::new(&fixture.state, true);
        let mut call = fixture.write_call();
        call.function.arguments = json!({"path": "../outside.md", "content": "escape"}).to_string();
        let outcome = execute_with_io(&fixture.state, &fixture.context, &call, None, &io).await;
        assert_eq!(io.executions.load(Ordering::Relaxed), 1);
        assert_eq!(outcome.message.tool_call_id.as_deref(), Some("write-1"));
        let record = outcome.message.tool_execution.unwrap();
        assert_eq!(record.status, ToolExecutionStatus::Failed);
        assert!(record.error.is_some());
        assert!(!fixture.root.join("outside.md").exists());
        let ends = io.ends.lock().unwrap();
        assert_eq!(ends.len(), 1);
        assert!(!ends[0].ok);
        assert!(!ends[0].denied);
    }

    #[tokio::test]
    async fn deferred_tool_confirmation_uses_real_target_and_denial_has_no_effect() {
        let fixture = Fixture::new();
        let io = TestIo::new(&fixture.state, false);
        let mut call = fixture.write_call();
        call.function.name = "execute_tool".into();
        call.function.arguments =
            json!({"tool_name": "open_path", "args": {"target": "https://example.com"}})
                .to_string();
        let outcome = execute_with_io(&fixture.state, &fixture.context, &call, None, &io).await;
        assert_eq!(*io.confirmations.lock().unwrap(), vec!["open_path"]);
        assert_eq!(io.executions.load(Ordering::Relaxed), 0);
        assert_eq!(outcome.message.name.as_deref(), Some("execute_tool"));
        assert_eq!(
            outcome.message.tool_execution.unwrap().status,
            ToolExecutionStatus::Denied
        );
        let entries = permission::panel_state(&fixture.state).audit;
        assert!(entries.iter().any(|entry| entry.tool == "open_path"));
        assert!(entries.iter().all(|entry| entry.tool != "execute_tool"));
    }

    #[tokio::test]
    async fn full_tool_result_is_preserved_when_ui_preview_is_truncated() {
        let fixture = Fixture::new();
        let content = "界".repeat(UI_RESULT_CAP + 100);
        std::fs::write(fixture.root.join("project/long.txt"), &content).unwrap();
        let io = TestIo::new(&fixture.state, true);
        let mut call = fixture.write_call();
        call.function.name = "read_file".into();
        call.function.arguments = json!({"path": "long.txt"}).to_string();
        let outcome = execute_with_io(&fixture.state, &fixture.context, &call, None, &io).await;
        assert!(outcome
            .message
            .content
            .as_deref()
            .unwrap()
            .contains(&content));
        assert!(!outcome.ui_result.contains(&content));
        assert!(outcome.ui_result.contains("已截断"));
        assert_eq!(io.ends.lock().unwrap()[0].result, outcome.ui_result);
    }

    #[test]
    fn source_quality_counts_shared_source_blocks() {
        let strong = source_quality_hint(
            "web_search",
            "Web search results\n\nLinks:\n1. [A](https://a.example)\n2. [B](https://b.example)\n3. [C](https://c.example)\n",
            true,
        )
        .unwrap();
        assert_eq!(strong["level"], "strong");
        assert_eq!(strong["source_count"].as_u64(), Some(3));

        let limited = source_quality_hint(
            "web_fetch",
            "Content:\n[Inline](https://inline.example)\n\nSources:\n- [A](https://a.example)\n\nREMINDER: cite sources",
            true,
        )
        .unwrap();
        assert_eq!(limited["level"], "limited");
        assert_eq!(limited["source_count"].as_u64(), Some(1));

        let none = source_quality_hint("web_fetch", "Content only", true).unwrap();
        assert_eq!(none["level"], "none");
        assert_eq!(none["source_count"].as_u64(), Some(0));
    }

    #[test]
    fn source_quality_ignores_failed_or_non_web_tools() {
        assert!(
            source_quality_hint("web_search", "Links:\n1. [A](https://a.example)", false).is_none()
        );
        assert!(
            source_quality_hint("read_file", "Sources:\n- [A](https://a.example)", true).is_none()
        );
    }
}
