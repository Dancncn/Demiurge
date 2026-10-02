use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::future::Future;
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use super::execution_context::ExecutionContext;
use super::subagent::{SubagentContextMode, SubagentRequest};
use super::{budget, subagent, workflow_journal};
use crate::store;

#[path = "workflow_schema.rs"]
mod workflow_schema;

use workflow_schema::{
    builtin_templates, count_steps, dry_run, find_step, node_id, render_workflow,
    validate_workflow, WorkflowFile, WorkflowStep, MAX_PARALLEL_ITEMS,
};
pub use workflow_schema::{
    WorkflowDryRun, WorkflowInputDefinition, WorkflowInputKind, WorkflowTemplateInfo,
    WorkflowValidationIssue, WorkflowValidationLevel, WorkflowValidationReport,
};

const WORKFLOW_DIR: &str = ".demiurge/workflows";
const RUN_STATE_SCHEMA_VERSION: u32 = 2;
const RUN_STATE_FILE: &str = "state.json";

type StepFuture<'a> = Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>>;

#[derive(Clone, Debug, Serialize)]
pub struct WorkflowDefinitionInfo {
    pub name: String,
    pub description: String,
    pub path: String,
    pub inputs: Vec<WorkflowInputDefinition>,
    pub valid: bool,
    pub issues: Vec<WorkflowValidationIssue>,
    pub steps_total: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct WorkflowPanelState {
    pub definitions: Vec<WorkflowDefinitionInfo>,
    pub runs: Vec<WorkflowRunProgress>,
    pub templates: Vec<WorkflowTemplateInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkflowRunProgress {
    pub run_id: String,
    pub name: String,
    #[serde(default)]
    pub definition_name: String,
    pub status: WorkflowStatus,
    #[serde(default)]
    pub cancel_requested: bool,
    pub current_phase: Option<String>,
    pub agents: Vec<WorkflowAgentProgress>,
    pub logs: Vec<String>,
    pub journal_path: String,
    pub started_at: u64,
    pub updated_at: u64,
    pub error: Option<String>,
    pub budget: budget::TokenBudgetState,
    pub steps_total: usize,
    pub steps_done: usize,
    #[serde(default)]
    pub input_values: BTreeMap<String, Value>,
    #[serde(default)]
    pub failed_node: Option<WorkflowFailedNode>,
    #[serde(default)]
    pub parent_run_id: Option<String>,
    #[serde(default)]
    pub retry_node_id: Option<String>,
    #[serde(default)]
    execution: Option<ExecutionContext>,
    #[serde(default)]
    definition_snapshot: Option<WorkflowFile>,
    /// Only populated from a verified on-disk location; legacy metadata is not authority.
    #[serde(skip)]
    storage_root: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    Running,
    StaleRunning,
    Done,
    Failed,
    Killed,
    Journaled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkflowAgentProgress {
    pub id: u64,
    #[serde(default)]
    pub node_id: String,
    pub label: String,
    pub phase: Option<String>,
    pub status: WorkflowStatus,
    pub result: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkflowFailedNode {
    pub node_id: String,
    pub kind: String,
    pub label: String,
    pub phase: Option<String>,
    pub error: String,
    pub retryable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct WorkflowRunStateFile {
    schema_version: u32,
    run: WorkflowRunProgress,
}

pub fn ensure_dir(state: &crate::AppState) -> Result<PathBuf, String> {
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let dir = sandbox.join(WORKFLOW_DIR);
    fs::create_dir_all(&dir).map_err(|e| format!("创建 workflow 目录失败：{e}"))?;
    Ok(dir)
}

pub fn panel_state(state: &crate::AppState) -> WorkflowPanelState {
    let definitions = list_definitions(state);
    let mut runs = state.workflow_runs.lock().unwrap().clone();
    let mut seen = runs
        .iter()
        .map(|run| run.run_id.clone())
        .collect::<HashSet<_>>();
    for run in list_persisted_run_states(state) {
        if seen.contains(&run.run_id) {
            continue;
        }
        seen.insert(run.run_id.clone());
        runs.push(run);
    }
    for info in workflow_journal::list(state) {
        if seen.contains(&info.run_id) {
            continue;
        }
        seen.insert(info.run_id.clone());
        runs.push(WorkflowRunProgress {
            run_id: info.run_id,
            name: "journal".to_string(),
            definition_name: String::new(),
            status: WorkflowStatus::Journaled,
            cancel_requested: false,
            current_phase: None,
            agents: Vec::new(),
            logs: Vec::new(),
            journal_path: info.journal_path,
            started_at: info.updated_at,
            updated_at: info.updated_at,
            error: None,
            budget: budget::TokenBudgetState::default(),
            steps_total: 0,
            steps_done: 0,
            input_values: BTreeMap::new(),
            failed_node: None,
            parent_run_id: None,
            retry_node_id: None,
            execution: None,
            definition_snapshot: None,
            storage_root: None,
        });
    }
    runs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    WorkflowPanelState {
        definitions,
        runs,
        templates: builtin_templates(),
    }
}

pub fn hydrate_persisted_runs(state: &crate::AppState) {
    let persisted = list_persisted_run_states(state);
    if persisted.is_empty() {
        return;
    }
    let mut runs = state.workflow_runs.lock().unwrap();
    let mut seen = runs
        .iter()
        .map(|run| run.run_id.clone())
        .collect::<HashSet<_>>();
    for run in persisted {
        if seen.insert(run.run_id.clone()) {
            runs.push(run);
        }
    }
    runs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
}

pub fn resume_overlay(state: &crate::AppState, run_id: &str) -> Result<String, String> {
    let root = registered_storage_root(state, run_id)?
        .unwrap_or_else(|| state.sandbox_dir.lock().unwrap().clone());
    let run = registered_run(state, run_id)
        .ok()
        .or_else(|| read_run_state_in_root(&root, run_id));
    if let Some(run) = &run {
        let (execution, _) = owned_definition(state, run)?;
        let caller = ExecutionContext::for_session(
            state,
            &super::session_engine::execution_session_id(state),
        )?;
        if caller != execution {
            return Err(
                "Resume this workflow from its original session and workspace.".to_string(),
            );
        }
    }
    let journal = if run.is_some() {
        workflow_journal::resume_overlay_in_root(&root, run_id)
    } else {
        workflow_journal::resume_overlay(state, run_id)
    };
    match journal {
        Ok(overlay) => Ok(overlay),
        Err(journal_err) => {
            let Some(run) = run else {
                return Err(journal_err);
            };
            let snapshot = serde_json::to_string_pretty(&run)
                .map_err(|e| format!("序列化 workflow state 失败：{e}"))?;
            Ok(format!(
                "你正在恢复 Ultracode workflow run `{run_id}`。\n\
                 该 run 没有可读取的 journal tail，但找到了 durable state snapshot。请先根据 snapshot 复盘已完成事项、未完成事项和下一步，然后继续执行；不要重复已经完成的安全操作。\n\n\
                 ```json\n{snapshot}\n```"
            ))
        }
    }
}

pub fn list_definitions(state: &crate::AppState) -> Vec<WorkflowDefinitionInfo> {
    let Ok(dir) = ensure_dir(state) else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                return None;
            }
            let raw = fs::read_to_string(&path).ok()?;
            let parsed = serde_json::from_str::<WorkflowFile>(&raw);
            let name = parsed
                .as_ref()
                .ok()
                .and_then(|w| w.name.clone())
                .or_else(|| path.file_stem().map(|s| s.to_string_lossy().to_string()))?;
            let (description, inputs, valid, issues, steps_total) = match parsed {
                Ok(workflow) => {
                    let report = validate_workflow(&workflow, &definition_probe_inputs(&workflow));
                    (
                        workflow.description.unwrap_or_default(),
                        workflow.inputs,
                        report.valid,
                        report.issues,
                        count_steps(&workflow.steps),
                    )
                }
                Err(error) => (
                    String::new(),
                    Vec::new(),
                    false,
                    vec![WorkflowValidationIssue {
                        level: WorkflowValidationLevel::Error,
                        path: "document".to_string(),
                        message: format!("invalid workflow JSON: {error}"),
                    }],
                    0,
                ),
            };
            Some(WorkflowDefinitionInfo {
                name,
                description,
                path: path.to_string_lossy().to_string(),
                inputs,
                valid,
                issues,
                steps_total,
            })
        })
        .collect::<Vec<_>>();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn launch(app: &AppHandle, state: &crate::AppState, name: &str) -> Result<String, String> {
    launch_with_inputs(app, state, name, BTreeMap::new())
}

pub fn launch_with_inputs(
    app: &AppHandle,
    state: &crate::AppState,
    name: &str,
    inputs: BTreeMap<String, Value>,
) -> Result<String, String> {
    let progress = prepare_run(state, name, inputs)?;
    let run_id = progress.run_id.clone();
    let path = progress.logs.first().cloned().unwrap_or_default();
    state.workflow_runs.lock().unwrap().push(progress);
    state
        .workflow_cancels
        .lock()
        .unwrap()
        .insert(run_id.clone(), Arc::new(AtomicBool::new(false)));
    emit_update(app, state);
    let _ = workflow_journal::append(
        state,
        &run_id,
        "workflow_started",
        json!({"name": name, "path": path}),
    );
    Ok(run_id)
}

fn prepare_run(
    state: &crate::AppState,
    name: &str,
    inputs: BTreeMap<String, Value>,
) -> Result<WorkflowRunProgress, String> {
    let execution = ExecutionContext::capture_selected(state)?;
    let (workflow, path) = load_workflow_in_root(&execution.workspace_root, name)?;
    let report = validate_workflow(&workflow, &inputs);
    if !report.valid {
        return Err(format_validation_errors(&report.issues));
    }
    let rendered = render_workflow(&workflow, &report.normalized_inputs)?;
    let run_id = workflow_journal::new_run_id();
    let journal_path = execution
        .workspace_root
        .join(".demiurge")
        .join("workflow-runs")
        .join(&run_id)
        .join("journal.jsonl")
        .to_string_lossy()
        .to_string();
    let now = store::now_millis();
    let progress = WorkflowRunProgress {
        run_id: run_id.clone(),
        name: rendered.name.clone().unwrap_or_else(|| name.to_string()),
        definition_name: name.to_string(),
        status: WorkflowStatus::Running,
        cancel_requested: false,
        current_phase: None,
        agents: Vec::new(),
        logs: vec![format!("loaded {}", path.display())],
        journal_path,
        started_at: now,
        updated_at: now,
        error: None,
        budget: budget::TokenBudgetState::default(),
        steps_total: count_steps(&rendered.steps),
        steps_done: 0,
        input_values: report.normalized_inputs,
        failed_node: None,
        parent_run_id: None,
        retry_node_id: None,
        execution: Some(execution),
        definition_snapshot: Some(rendered),
        storage_root: None,
    };
    Ok(progress)
}

pub fn validate_definition(
    state: &crate::AppState,
    name: &str,
    inputs: BTreeMap<String, Value>,
) -> Result<WorkflowValidationReport, String> {
    let (workflow, _) = load_workflow(state, name)?;
    Ok(validate_workflow(&workflow, &inputs))
}

pub fn dry_run_definition(
    state: &crate::AppState,
    name: &str,
    inputs: BTreeMap<String, Value>,
) -> Result<WorkflowDryRun, String> {
    let (workflow, _) = load_workflow(state, name)?;
    Ok(dry_run(&workflow, name, &inputs))
}

pub fn install_template(
    state: &crate::AppState,
    template_id: &str,
    requested_name: Option<String>,
) -> Result<WorkflowDefinitionInfo, String> {
    let template = builtin_templates()
        .into_iter()
        .find(|template| template.id == template_id)
        .ok_or_else(|| format!("unknown workflow template `{template_id}`"))?;
    let mut definition = template.definition;
    let file_name = requested_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(&template.id);
    let safe_name = sanitize_name(file_name);
    if safe_name.is_empty() {
        return Err("workflow template name is invalid".to_string());
    }
    if let Some(name) = requested_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        if let Some(object) = definition.as_object_mut() {
            object.insert("name".to_string(), Value::String(name.to_string()));
        }
    }

    let workflow: WorkflowFile = serde_json::from_value(definition.clone())
        .map_err(|error| format!("invalid built-in workflow template: {error}"))?;
    let report = validate_workflow(&workflow, &definition_probe_inputs(&workflow));
    if !report.valid {
        return Err(format_validation_errors(&report.issues));
    }

    let dir = ensure_dir(state)?;
    let target = dir.join(format!("{safe_name}.json"));
    if target.exists() {
        return Err(format!(
            "workflow `{safe_name}` already exists; choose another name"
        ));
    }
    let temp = dir.join(format!(".{safe_name}.json.tmp"));
    let body = serde_json::to_vec_pretty(&definition)
        .map_err(|error| format!("serialize workflow template failed: {error}"))?;
    fs::write(&temp, body).map_err(|error| format!("write workflow template failed: {error}"))?;
    fs::rename(&temp, &target)
        .map_err(|error| format!("commit workflow template failed: {error}"))?;

    list_definitions(state)
        .into_iter()
        .find(|definition| Path::new(&definition.path) == target)
        .ok_or_else(|| "installed workflow template could not be reloaded".to_string())
}

pub fn launch_failed_node_retry(
    app: &AppHandle,
    state: &crate::AppState,
    parent_run_id: &str,
) -> Result<String, String> {
    let parent = state
        .workflow_runs
        .lock()
        .unwrap()
        .iter()
        .find(|run| run.run_id == parent_run_id)
        .cloned()
        .or_else(|| {
            let root = state.sandbox_dir.lock().unwrap().clone();
            read_run_state_in_root(&root, parent_run_id)
        })
        .ok_or_else(|| format!("workflow run `{parent_run_id}` was not found"))?;
    let retry = prepare_retry(state, &parent)?;
    let run_id = retry.run_id.clone();
    let node_id = retry.retry_node_id.clone();
    state.workflow_runs.lock().unwrap().push(retry);
    state
        .workflow_cancels
        .lock()
        .unwrap()
        .insert(run_id.clone(), Arc::new(AtomicBool::new(false)));
    let _ = workflow_journal::append(
        state,
        &run_id,
        "workflow_node_retry_started",
        json!({
            "parent_run_id": parent_run_id, "node_id": node_id, "definition": parent.definition_name,
        }),
    );
    emit_update(app, state);
    Ok(run_id)
}

fn prepare_retry(
    state: &crate::AppState,
    parent: &WorkflowRunProgress,
) -> Result<WorkflowRunProgress, String> {
    let parent_run_id = &parent.run_id;
    let failed = parent
        .failed_node
        .clone()
        .filter(|failed| failed.retryable)
        .ok_or_else(|| "this workflow has no retryable failed node".to_string())?;
    let definition_name = if parent.definition_name.trim().is_empty() {
        parent.name.clone()
    } else {
        parent.definition_name.clone()
    };
    let (execution, rendered) = owned_definition(state, &parent)?;
    let (step, _) = find_step(&rendered.steps, &failed.node_id).ok_or_else(|| {
        format!(
            "failed node `{}` no longer exists in workflow `{definition_name}`",
            failed.node_id
        )
    })?;

    let run_id = workflow_journal::new_run_id();
    let now = store::now_millis();
    let journal_path = workflow_journal::run_dir(&execution.workspace_root, &run_id)
        .join("journal.jsonl")
        .to_string_lossy()
        .to_string();
    Ok(WorkflowRunProgress {
        run_id: run_id.clone(),
        name: format!("{} / retry {}", parent.name, failed.label),
        definition_name,
        status: WorkflowStatus::Running,
        cancel_requested: false,
        current_phase: failed.phase.clone(),
        agents: Vec::new(),
        logs: vec![format!(
            "retrying node {} from run {}",
            failed.node_id, parent_run_id
        )],
        journal_path,
        started_at: now,
        updated_at: now,
        error: None,
        budget: parent.budget.clone(),
        steps_total: count_steps(std::slice::from_ref(&step)),
        steps_done: 0,
        input_values: parent.input_values.clone(),
        failed_node: None,
        parent_run_id: Some(parent_run_id.to_string()),
        retry_node_id: Some(failed.node_id.clone()),
        execution: Some(execution),
        definition_snapshot: Some(rendered),
        storage_root: None,
    })
}

pub async fn run_failed_node_retry(app: AppHandle, run_id: String) {
    let state = app.state::<crate::AppState>();
    let result = async {
        let run = state
            .workflow_runs
            .lock()
            .unwrap()
            .iter()
            .find(|run| run.run_id == run_id)
            .cloned()
            .ok_or_else(|| format!("retry run `{run_id}` was not found"))?;
        let node_id = run
            .retry_node_id
            .clone()
            .ok_or_else(|| "retry run is missing its node id".to_string())?;
        let (_, rendered) = owned_definition(state.inner(), &run)?;
        let (step, phase) = find_step(&rendered.steps, &node_id)
            .ok_or_else(|| format!("retry node `{node_id}` no longer exists"))?;
        run_step(&app, state.inner(), &run_id, phase, node_id, step).await?;
        if is_cancelled(state.inner(), &run_id) {
            mark_run(
                state.inner(),
                &run_id,
                WorkflowStatus::Killed,
                true,
                Some("user stopped workflow node retry".to_string()),
            );
        } else {
            mark_run(state.inner(), &run_id, WorkflowStatus::Done, false, None);
            let _ = workflow_journal::append(
                state.inner(),
                &run_id,
                "workflow_node_retry_done",
                json!({}),
            );
        }
        emit_update(&app, state.inner());
        Ok::<(), String>(())
    }
    .await;
    if let Err(error) = result {
        mark_run(
            state.inner(),
            &run_id,
            WorkflowStatus::Failed,
            false,
            Some(error.clone()),
        );
        let _ = workflow_journal::append(
            state.inner(),
            &run_id,
            "workflow_node_retry_failed",
            json!({ "error": error }),
        );
        emit_update(&app, state.inner());
    }
    state.workflow_cancels.lock().unwrap().remove(&run_id);
}

pub fn workflow_validate(
    state: &crate::AppState,
    name: String,
    inputs: BTreeMap<String, Value>,
) -> Result<WorkflowValidationReport, String> {
    validate_definition(state, &name, inputs)
}

pub fn workflow_dry_run(
    state: &crate::AppState,
    name: String,
    inputs: BTreeMap<String, Value>,
) -> Result<WorkflowDryRun, String> {
    dry_run_definition(state, &name, inputs)
}

pub fn workflow_templates() -> Vec<WorkflowTemplateInfo> {
    builtin_templates()
}

pub fn workflow_install_template(
    state: &crate::AppState,
    template_id: String,
    name: Option<String>,
) -> Result<WorkflowDefinitionInfo, String> {
    install_template(state, &template_id, name)
}

pub fn workflow_run_with_inputs(
    app: AppHandle,
    state: &crate::AppState,
    name: String,
    inputs: BTreeMap<String, Value>,
) -> Result<String, String> {
    let run_id = launch_with_inputs(&app, state, &name, inputs)?;
    let run_id_for_task = run_id.clone();
    tauri::async_runtime::spawn(async move {
        run_launched(app, run_id_for_task, name).await;
    });
    Ok(run_id)
}

pub fn workflow_retry_failed_node(
    app: AppHandle,
    state: &crate::AppState,
    run_id: String,
) -> Result<String, String> {
    let retry_run_id = launch_failed_node_retry(&app, state, &run_id)?;
    let retry_run_id_for_task = retry_run_id.clone();
    tauri::async_runtime::spawn(async move {
        run_failed_node_retry(app, retry_run_id_for_task).await;
    });
    Ok(retry_run_id)
}

pub async fn run_launched(app: AppHandle, run_id: String, _name: String) {
    let state = app.state::<crate::AppState>();
    let result = async {
        let run = registered_run(state.inner(), &run_id)?;
        let (_, workflow) = owned_definition(state.inner(), &run)?;
        for (index, step) in workflow.steps.into_iter().enumerate() {
            let generated = format!("steps[{index}]");
            let step_id = node_id(&step, &generated);
            run_step(&app, state.inner(), &run_id, None, step_id, step).await?;
            if is_cancelled(state.inner(), &run_id) {
                mark_run(
                    state.inner(),
                    &run_id,
                    WorkflowStatus::Killed,
                    true,
                    Some("用户停止 workflow".to_string()),
                );
                emit_update(&app, state.inner());
                return Ok(());
            }
        }
        mark_run(state.inner(), &run_id, WorkflowStatus::Done, false, None);
        let _ = workflow_journal::append(state.inner(), &run_id, "workflow_done", json!({}));
        emit_update(&app, state.inner());
        Ok::<(), String>(())
    }
    .await;

    if let Err(e) = result {
        mark_run(
            state.inner(),
            &run_id,
            WorkflowStatus::Failed,
            false,
            Some(e.clone()),
        );
        let _ = workflow_journal::append(
            state.inner(),
            &run_id,
            "workflow_failed",
            json!({ "error": e }),
        );
        emit_update(&app, state.inner());
    }
    state.workflow_cancels.lock().unwrap().remove(&run_id);
}

pub fn stop(app: &AppHandle, state: &crate::AppState, run_id: &str) -> Result<(), String> {
    let Some(flag) = state.workflow_cancels.lock().unwrap().get(run_id).cloned() else {
        return Err("该 workflow 当前没有运行中的任务。".to_string());
    };
    flag.store(true, Ordering::Relaxed);
    mark_run(
        state,
        run_id,
        WorkflowStatus::Killed,
        true,
        Some("用户请求停止".to_string()),
    );
    let _ = workflow_journal::append(state, run_id, "workflow_killed", json!({}));
    emit_update(app, state);
    Ok(())
}

fn run_step<'a>(
    app: &'a AppHandle,
    state: &'a crate::AppState,
    run_id: &'a str,
    phase: Option<String>,
    step_id: String,
    step: WorkflowStep,
) -> StepFuture<'a> {
    Box::pin(async move {
        if is_cancelled(state, run_id) {
            return Ok(());
        }
        let step_kind = step.kind().to_string();
        let step_label = step.label();
        let retry_phase = phase.clone();
        let result = async {
            match step {
                WorkflowStep::Log { message, .. } => {
                    push_log(app, state, run_id, message.clone());
                    let _ = workflow_journal::append(
                        state,
                        run_id,
                        "log",
                        json!({ "message": message }),
                    );
                }
                WorkflowStep::Phase { name, steps, .. } => {
                    set_phase(app, state, run_id, Some(name.clone()));
                    let _ = workflow_journal::append(
                        state,
                        run_id,
                        "phase_started",
                        json!({ "name": name }),
                    );
                    for (index, child) in steps.into_iter().enumerate() {
                        let generated = format!("{step_id}.steps[{index}]");
                        let child_id = node_id(&child, &generated);
                        run_step(app, state, run_id, Some(name.clone()), child_id, child).await?;
                    }
                    let _ = workflow_journal::append(
                        state,
                        run_id,
                        "phase_done",
                        json!({ "name": name }),
                    );
                }
                WorkflowStep::Agent {
                    prompt,
                    label,
                    agent_type,
                    agent,
                    context_mode,
                    ..
                } => {
                    run_agent_step(
                        app,
                        state,
                        run_id,
                        phase,
                        prompt,
                        label,
                        agent_type,
                        agent,
                        context_mode,
                        step_id.clone(),
                    )
                    .await?;
                }
                WorkflowStep::Parallel { items, .. } => {
                    if items.len() > MAX_PARALLEL_ITEMS {
                        return Err(format!(
                            "parallel 最多支持 {MAX_PARALLEL_ITEMS} 个 item，当前 {} 个。",
                            items.len()
                        ));
                    }
                    let futures = items
                        .into_iter()
                        .enumerate()
                        .map(|(index, item)| {
                            let generated = format!("{step_id}.items[{index}]");
                            let child_id = node_id(&item, &generated);
                            run_step(app, state, run_id, phase.clone(), child_id, item)
                        })
                        .collect::<Vec<_>>();
                    let results = futures_util::future::join_all(futures).await;
                    for result in results {
                        result?;
                    }
                }
                WorkflowStep::Pipeline { items, .. } => {
                    for (index, item) in items.into_iter().enumerate() {
                        let generated = format!("{step_id}.items[{index}]");
                        let child_id = node_id(&item, &generated);
                        run_step(app, state, run_id, phase.clone(), child_id, item).await?;
                    }
                }
                WorkflowStep::Budget { total, .. } => {
                    set_budget(app, state, run_id, budget::TokenBudgetState::new(total));
                    push_log(
                        app,
                        state,
                        run_id,
                        format!(
                            "budget total set to {}",
                            total
                                .map(|n| n.to_string())
                                .unwrap_or_else(|| "unlimited".to_string())
                        ),
                    );
                    let _ = workflow_journal::append(
                        state,
                        run_id,
                        "budget",
                        json!({ "total": total }),
                    );
                }
            }
            mark_step_done(app, state, run_id);
            Ok::<(), String>(())
        }
        .await;
        if let Err(error) = &result {
            record_failed_node(
                app,
                state,
                run_id,
                WorkflowFailedNode {
                    node_id: step_id,
                    kind: step_kind,
                    label: step_label,
                    phase: retry_phase,
                    error: error.clone(),
                    retryable: !is_cancelled(state, run_id),
                },
            );
        }
        result
    })
}

async fn run_agent_step(
    app: &AppHandle,
    state: &crate::AppState,
    run_id: &str,
    phase: Option<String>,
    prompt: String,
    label: Option<String>,
    agent_type: Option<String>,
    agent_name: Option<String>,
    context_mode: Option<String>,
    node_id: String,
) -> Result<(), String> {
    let id = next_agent_id(state, run_id);
    if workflow_budget(state, run_id).is_some_and(|budget| budget.is_exhausted()) {
        let message = "workflow token budget exhausted before agent step".to_string();
        push_log(app, state, run_id, message.clone());
        let _ = workflow_journal::append(state, run_id, "token_budget_exhausted", json!({}));
        return Err(message);
    }
    let label = label.unwrap_or_else(|| format!("agent-{id}"));
    push_agent(
        app,
        state,
        run_id,
        id,
        node_id.clone(),
        label.clone(),
        phase.clone(),
    );
    let _ = workflow_journal::append(
        state,
        run_id,
        "agent_started",
        json!({ "agent_id": id, "node_id": node_id, "label": label, "phase": phase, "prompt": prompt, "agent": agent_name.clone() }),
    );
    let mode = SubagentContextMode::parse(context_mode.as_deref());
    let run = registered_run(state, run_id)?;
    let (execution, _) = owned_definition(state, &run)?;
    let cancel = state
        .workflow_cancels
        .lock()
        .unwrap()
        .get(run_id)
        .cloned()
        .ok_or_else(|| "Workflow has no live cancellation token.".to_string())?;
    let result = subagent::run(
        state,
        SubagentRequest {
            prompt,
            label: Some(label.clone()),
            agent_type,
            agent_name,
            model: None,
            model_tier: None,
            scope: subagent::SubagentScope::ReadOnly,
            context_mode: mode,
            max_total_tokens: workflow_budget(state, run_id).and_then(|budget| budget.remaining()),
            output_format: subagent::SubagentOutputFormat::Plain,
            reviewer_count: 1,
            cancel: Some(cancel),
            execution: Some(execution),
        },
    )
    .await;

    match result {
        Ok(text) => {
            if is_cancelled(state, run_id) {
                update_agent(
                    app,
                    state,
                    run_id,
                    id,
                    WorkflowStatus::Killed,
                    Some(text.clone()),
                    None,
                );
                let _ = workflow_journal::append(
                    state,
                    run_id,
                    "agent_killed",
                    json!({ "agent_id": id, "label": label, "result": text }),
                );
                return Ok(());
            }
            record_budget_estimate(app, state, run_id, budget::estimate_text_tokens(&text));
            update_agent(
                app,
                state,
                run_id,
                id,
                WorkflowStatus::Done,
                Some(text.clone()),
                None,
            );
            let _ = workflow_journal::append(
                state,
                run_id,
                "agent_done",
                json!({ "agent_id": id, "label": label, "result": text }),
            );
            Ok(())
        }
        Err(e) => {
            update_agent(
                app,
                state,
                run_id,
                id,
                WorkflowStatus::Failed,
                None,
                Some(e.clone()),
            );
            let _ = workflow_journal::append(
                state,
                run_id,
                "agent_failed",
                json!({ "agent_id": id, "label": label, "error": e }),
            );
            Err(e)
        }
    }
}

fn definition_probe_inputs(workflow: &WorkflowFile) -> BTreeMap<String, Value> {
    workflow
        .inputs
        .iter()
        .map(|field| {
            let value = field.default.clone().unwrap_or_else(|| match field.kind {
                WorkflowInputKind::Text | WorkflowInputKind::Textarea => {
                    Value::String("sample".to_string())
                }
                WorkflowInputKind::Number => json!(field.min.unwrap_or(0.0)),
                WorkflowInputKind::Boolean => Value::Bool(false),
                WorkflowInputKind::Select => field
                    .options
                    .first()
                    .cloned()
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            });
            (field.key.clone(), value)
        })
        .collect()
}

fn format_validation_errors(issues: &[WorkflowValidationIssue]) -> String {
    let details = issues
        .iter()
        .filter(|issue| issue.level == WorkflowValidationLevel::Error)
        .take(12)
        .map(|issue| format!("{}: {}", issue.path, issue.message))
        .collect::<Vec<_>>()
        .join("\n");
    if details.is_empty() {
        "workflow validation failed".to_string()
    } else {
        format!("workflow validation failed:\n{details}")
    }
}

fn load_workflow(state: &crate::AppState, name: &str) -> Result<(WorkflowFile, PathBuf), String> {
    let dir = ensure_dir(state)?;
    load_workflow_from_dir(dir, name)
}

fn load_workflow_in_root(root: &Path, name: &str) -> Result<(WorkflowFile, PathBuf), String> {
    load_workflow_from_dir(root.join(WORKFLOW_DIR), name)
}

fn load_workflow_from_dir(dir: PathBuf, name: &str) -> Result<(WorkflowFile, PathBuf), String> {
    let requested = name.trim();
    if requested.is_empty() {
        return Err("workflow 名称不能为空。".to_string());
    }
    let path = if let Some(path) = find_workflow_path(&dir, requested) {
        path
    } else {
        let safe = sanitize_name(requested);
        if safe.is_empty() {
            return Err("workflow 名称至少需要包含一个字母、数字、下划线或连字符。".to_string());
        }
        dir.join(format!("{safe}.json"))
    };
    let raw = fs::read_to_string(&path)
        .map_err(|e| format!("读取 workflow `{name}` 失败：{e}。路径：{}", path.display()))?;
    let workflow = serde_json::from_str::<WorkflowFile>(&raw)
        .map_err(|e| format!("解析 workflow JSON 失败：{e}"))?;
    Ok((workflow, path))
}

fn find_workflow_path(dir: &PathBuf, requested: &str) -> Option<PathBuf> {
    let requested_safe = sanitize_name(requested);
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string());
        if stem.as_deref() == Some(requested) || stem.as_deref() == Some(requested_safe.as_str()) {
            return Some(path);
        }
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(workflow) = serde_json::from_str::<WorkflowFile>(&raw) else {
            continue;
        };
        if workflow
            .name
            .as_deref()
            .map(|name| name == requested || sanitize_name(name) == requested_safe)
            .unwrap_or(false)
        {
            return Some(path);
        }
    }
    None
}

fn sanitize_name(name: &str) -> String {
    name.trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn is_cancelled(state: &crate::AppState, run_id: &str) -> bool {
    state
        .workflow_cancels
        .lock()
        .unwrap()
        .get(run_id)
        .map(|flag| flag.load(Ordering::Relaxed))
        .unwrap_or(false)
}

fn next_agent_id(state: &crate::AppState, run_id: &str) -> u64 {
    let runs = state.workflow_runs.lock().unwrap();
    runs.iter()
        .find(|run| run.run_id == run_id)
        .map(|run| run.agents.iter().map(|a| a.id).max().unwrap_or(0) + 1)
        .unwrap_or(1)
}

fn push_agent(
    app: &AppHandle,
    state: &crate::AppState,
    run_id: &str,
    id: u64,
    node_id: String,
    label: String,
    phase: Option<String>,
) {
    let mut runs = state.workflow_runs.lock().unwrap();
    if let Some(run) = runs.iter_mut().find(|run| run.run_id == run_id) {
        run.agents.push(WorkflowAgentProgress {
            id,
            node_id,
            label,
            phase,
            status: WorkflowStatus::Running,
            result: None,
            error: None,
        });
        run.updated_at = store::now_millis();
    }
    drop(runs);
    emit_update(app, state);
}

fn record_failed_node(
    app: &AppHandle,
    state: &crate::AppState,
    run_id: &str,
    failed: WorkflowFailedNode,
) {
    let mut should_emit = false;
    {
        let mut runs = state.workflow_runs.lock().unwrap();
        if let Some(run) = runs.iter_mut().find(|run| run.run_id == run_id) {
            // Container steps also observe their child's error. The first record
            // is the deepest node, which is the only useful retry target.
            if run.failed_node.is_none() {
                run.failed_node = Some(failed.clone());
                run.updated_at = store::now_millis();
                should_emit = true;
            }
        }
    }
    if should_emit {
        let _ = workflow_journal::append(
            state,
            run_id,
            "workflow_node_failed",
            json!({
                "node_id": failed.node_id,
                "kind": failed.kind,
                "label": failed.label,
                "phase": failed.phase,
                "error": failed.error,
                "retryable": failed.retryable,
            }),
        );
        emit_update(app, state);
    }
}

fn update_agent(
    app: &AppHandle,
    state: &crate::AppState,
    run_id: &str,
    id: u64,
    status: WorkflowStatus,
    result: Option<String>,
    error: Option<String>,
) {
    let mut runs = state.workflow_runs.lock().unwrap();
    if let Some(run) = runs.iter_mut().find(|run| run.run_id == run_id) {
        if let Some(agent) = run.agents.iter_mut().find(|agent| agent.id == id) {
            agent.status = status;
            agent.result = result.map(|s| cap_chars(&s, 1200));
            agent.error = error;
        }
        run.updated_at = store::now_millis();
    }
    drop(runs);
    emit_update(app, state);
}

fn workflow_budget(state: &crate::AppState, run_id: &str) -> Option<budget::TokenBudgetState> {
    state
        .workflow_runs
        .lock()
        .unwrap()
        .iter()
        .find(|run| run.run_id == run_id)
        .map(|run| run.budget.clone())
}

fn set_budget(
    app: &AppHandle,
    state: &crate::AppState,
    run_id: &str,
    next_budget: budget::TokenBudgetState,
) {
    let mut runs = state.workflow_runs.lock().unwrap();
    if let Some(run) = runs.iter_mut().find(|run| run.run_id == run_id) {
        run.budget = next_budget;
        run.updated_at = store::now_millis();
    }
    drop(runs);
    emit_update(app, state);
}

fn record_budget_estimate(app: &AppHandle, state: &crate::AppState, run_id: &str, tokens: usize) {
    if tokens == 0 {
        return;
    }
    let mut snapshot = None;
    let mut runs = state.workflow_runs.lock().unwrap();
    if let Some(run) = runs.iter_mut().find(|run| run.run_id == run_id) {
        if run.budget.total.is_some() {
            run.budget.record_estimated(tokens);
            run.updated_at = store::now_millis();
            snapshot = Some(run.budget.clone());
        }
    }
    drop(runs);
    if let Some(budget) = snapshot {
        let _ = workflow_journal::append(
            state,
            run_id,
            "token_budget_used",
            json!({
                "used": budget.used_total(),
                "used_exact": budget.used_exact,
                "used_estimated": budget.used_estimated,
                "total": budget.total,
                "remaining": budget.remaining(),
            }),
        );
        emit_update(app, state);
    }
}

fn set_phase(app: &AppHandle, state: &crate::AppState, run_id: &str, phase: Option<String>) {
    let mut runs = state.workflow_runs.lock().unwrap();
    if let Some(run) = runs.iter_mut().find(|run| run.run_id == run_id) {
        run.current_phase = phase;
        run.updated_at = store::now_millis();
    }
    drop(runs);
    emit_update(app, state);
}

fn mark_step_done(app: &AppHandle, state: &crate::AppState, run_id: &str) {
    let mut runs = state.workflow_runs.lock().unwrap();
    if let Some(run) = runs.iter_mut().find(|run| run.run_id == run_id) {
        run.steps_done = run.steps_done.saturating_add(1).min(run.steps_total);
        run.updated_at = store::now_millis();
    }
    drop(runs);
    emit_update(app, state);
}

fn push_log(app: &AppHandle, state: &crate::AppState, run_id: &str, message: String) {
    let mut runs = state.workflow_runs.lock().unwrap();
    if let Some(run) = runs.iter_mut().find(|run| run.run_id == run_id) {
        run.logs.push(message);
        if run.logs.len() > 80 {
            let drain = run.logs.len() - 80;
            run.logs.drain(0..drain);
        }
        run.updated_at = store::now_millis();
    }
    drop(runs);
    emit_update(app, state);
}

fn registered_run(state: &crate::AppState, run_id: &str) -> Result<WorkflowRunProgress, String> {
    state
        .workflow_runs
        .lock()
        .unwrap()
        .iter()
        .find(|run| run.run_id == run_id)
        .cloned()
        .ok_or_else(|| format!("Workflow run `{run_id}` was not found."))
}

fn owned_definition(
    state: &crate::AppState,
    run: &WorkflowRunProgress,
) -> Result<(ExecutionContext, WorkflowFile), String> {
    let execution = run.execution.clone().ok_or_else(|| {
        "Legacy workflow has no verified execution identity; start a new run.".to_string()
    })?;
    execution.validate(state)?;
    let definition = run
        .definition_snapshot
        .clone()
        .ok_or_else(|| "Workflow has no captured definition; start a new run.".to_string())?;
    Ok((execution, definition))
}

fn storage_root(run: &WorkflowRunProgress) -> Result<PathBuf, String> {
    if let Some(execution) = &run.execution {
        execution.validate_root()?;
        return Ok(execution.workspace_root.clone());
    }
    let root = run
        .storage_root
        .clone()
        .ok_or_else(|| "Workflow has no verified storage location.".to_string())?;
    if fs::canonicalize(&root).ok().as_ref() != Some(&root) || !root.is_dir() {
        return Err("Workflow storage location changed or is unavailable.".to_string());
    }
    Ok(root)
}

pub(crate) fn registered_storage_root(
    state: &crate::AppState,
    run_id: &str,
) -> Result<Option<PathBuf>, String> {
    let run = state
        .workflow_runs
        .lock()
        .unwrap()
        .iter()
        .find(|run| run.run_id == run_id)
        .cloned();
    run.map(|run| storage_root(&run)).transpose()
}

fn persist_all_run_snapshots(state: &crate::AppState) {
    // Serialize before cloning so a delayed emitter cannot overwrite a newer snapshot.
    static WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _write = WRITE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let runs = state.workflow_runs.lock().unwrap().clone();
    for run in runs {
        if run.status == WorkflowStatus::Journaled {
            continue;
        }
        match storage_root(&run).and_then(|root| write_run_state_in_root(&root, &run)) {
            Ok(()) => {}
            Err(error) => eprintln!(
                "Workflow snapshot {} was not persisted: {error}",
                run.run_id
            ),
        }
    }
}

fn write_run_state_in_root(root: &Path, run: &WorkflowRunProgress) -> Result<(), String> {
    if let Some(execution) = &run.execution {
        execution.validate_root()?;
        if fs::canonicalize(root).ok().as_ref() != Some(&execution.workspace_root) {
            return Err("Refusing to persist workflow outside its launch workspace.".to_string());
        }
    }
    let dir = workflow_journal::run_dir(root, &run.run_id);
    fs::create_dir_all(&dir).map_err(|e| format!("创建 workflow state 目录失败：{e}"))?;
    let target = dir.join(RUN_STATE_FILE);
    let payload = WorkflowRunStateFile {
        schema_version: RUN_STATE_SCHEMA_VERSION,
        run: run.clone(),
    };
    let body = serde_json::to_vec_pretty(&payload)
        .map_err(|e| format!("序列化 workflow state 失败：{e}"))?;
    demiurge_framework::persistence::atomic_write(&target, &body, false)
        .map_err(|e| format!("提交 workflow state 失败：{e}"))
}

fn list_persisted_run_states(state: &crate::AppState) -> Vec<WorkflowRunProgress> {
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    list_run_states_in_root(&sandbox)
}

fn read_run_state_in_root(root: &Path, run_id: &str) -> Option<WorkflowRunProgress> {
    read_run_state_file(
        &workflow_journal::run_dir(root, run_id).join(RUN_STATE_FILE),
        root,
    )
    .map(normalize_restored_run)
}

fn list_run_states_in_root(root: &Path) -> Vec<WorkflowRunProgress> {
    let runs_dir = root.join(workflow_journal::JOURNAL_DIR);
    let Ok(entries) = fs::read_dir(runs_dir) else {
        return Vec::new();
    };
    let mut runs = entries
        .filter_map(Result::ok)
        .filter_map(|entry| read_run_state_file(&entry.path().join(RUN_STATE_FILE), root))
        .map(normalize_restored_run)
        .collect::<Vec<_>>();
    runs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    runs
}

fn read_run_state_file(path: &Path, root: &Path) -> Option<WorkflowRunProgress> {
    let raw = fs::read_to_string(path).ok()?;
    let mut parsed = serde_json::from_str::<WorkflowRunStateFile>(&raw).ok()?;
    if !matches!(parsed.schema_version, 1 | RUN_STATE_SCHEMA_VERSION) {
        return None;
    }
    if parsed.run.run_id.trim().is_empty() {
        return None;
    }
    let root = fs::canonicalize(root).ok()?;
    let expected_path = workflow_journal::run_dir(&root, &parsed.run.run_id).join(RUN_STATE_FILE);
    if fs::canonicalize(path).ok()? != expected_path {
        return None;
    }
    if let Some(execution) = &parsed.run.execution {
        if execution.workspace_root != root || execution.validate_root().is_err() {
            return None;
        }
    }
    parsed.run.storage_root = Some(root.clone());
    parsed.run.journal_path = workflow_journal::run_dir(&root, &parsed.run.run_id)
        .join("journal.jsonl")
        .to_string_lossy()
        .to_string();
    if parsed.run.execution.is_none() || parsed.run.definition_snapshot.is_none() {
        if let Some(failed) = parsed.run.failed_node.as_mut() {
            failed.retryable = false;
        }
    }
    Some(parsed.run)
}

fn normalize_restored_run(mut run: WorkflowRunProgress) -> WorkflowRunProgress {
    if run.status == WorkflowStatus::Running {
        if run.cancel_requested {
            run.status = WorkflowStatus::Killed;
            if run.error.is_none() {
                run.error = Some("Workflow was stopping when Demiurge exited.".to_string());
            }
        } else {
            run.status = WorkflowStatus::StaleRunning;
            if run.error.is_none() {
                run.error = Some(
                    "Workflow was running when Demiurge exited; no live task is attached."
                        .to_string(),
                );
            }
        }
    }
    run
}

fn mark_run(
    state: &crate::AppState,
    run_id: &str,
    status: WorkflowStatus,
    cancel_requested: bool,
    error: Option<String>,
) {
    let mut runs = state.workflow_runs.lock().unwrap();
    if let Some(run) = runs.iter_mut().find(|run| run.run_id == run_id) {
        run.status = status;
        run.cancel_requested = cancel_requested;
        run.error = error;
        run.updated_at = store::now_millis();
    }
}

fn emit_update(app: &AppHandle, state: &crate::AppState) {
    persist_all_run_snapshots(state);
    let _ = app.emit("workflow-updated", panel_state(state));
}

fn cap_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(max).collect();
        format!("{head}\n…[workflow result truncated]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ownership_fixture() -> (
        PathBuf,
        crate::AppState,
        PathBuf,
        PathBuf,
        WorkflowRunProgress,
    ) {
        let root = std::env::temp_dir().join(format!(
            "demiurge_workflow_fixture_{}",
            store::new_session_id()
        ));
        let a = root.join("a");
        let b = root.join("b");
        for (dir, marker) in [(&a, "PROJECT_A_MARKER"), (&b, "PROJECT_B_MARKER")] {
            fs::create_dir_all(dir.join(WORKFLOW_DIR)).unwrap();
            fs::write(dir.join("AGENTS.md"), marker).unwrap();
            fs::write(dir.join("sample.txt"), marker).unwrap();
            fs::write(dir.join(format!("{marker}.txt")), marker).unwrap();
            fs::create_dir_all(dir.join(".demiurge/agents")).unwrap();
            fs::write(
                dir.join(".demiurge/agents/reader.json"),
                json!({"name":"reader", "prompt":marker}).to_string(),
            )
            .unwrap();
            fs::write(
                dir.join("package.json"),
                json!({"scripts": {"inspect": marker}}).to_string(),
            )
            .unwrap();
            fs::write(
                dir.join(WORKFLOW_DIR).join("owner.json"),
                json!({"name":"owner", "steps":[{"type":"log", "id":"first", "message":marker}]})
                    .to_string(),
            )
            .unwrap();
        }
        let a = fs::canonicalize(a).unwrap();
        let b = fs::canonicalize(b).unwrap();
        let state = crate::AppState::new(reqwest::Client::new());
        *state.data_dir.lock().unwrap() = root.join("data");
        *state.packs_dir.lock().unwrap() = root.join("packs");
        *state.sandbox_dir.lock().unwrap() = a.clone();
        let mut session_a = store::Session::new();
        session_a.id = "session-a".to_string();
        session_a.workspace_path = a.to_string_lossy().to_string();
        let mut session_b = store::Session::new();
        session_b.id = "session-b".to_string();
        session_b.workspace_path = b.to_string_lossy().to_string();
        *state.sessions.lock().unwrap() = store::SessionStore {
            active: session_a.id.clone(),
            sessions: vec![session_a, session_b],
        };
        let run = prepare_run(&state, "owner", BTreeMap::new()).unwrap();
        (root, state, a, b, run)
    }

    #[tokio::test]
    async fn workflow_ownership_tools_prompt_and_definition_stay_with_launch_project() {
        let (root, state, a, b, run) = ownership_fixture();
        let execution = run.execution.clone().unwrap();
        state.sessions.lock().unwrap().active = "session-b".to_string();
        *state.sandbox_dir.lock().unwrap() = b;
        fs::remove_file(a.join(WORKFLOW_DIR).join("owner.json")).unwrap();
        let (owner, definition) = owned_definition(&state, &run).unwrap();
        assert_eq!(owner.session_id, "session-a");
        assert!(
            matches!(&definition.steps[0], WorkflowStep::Log { message, .. } if message == "PROJECT_A_MARKER")
        );
        for (name, args) in [
            ("read_file", json!({"path":"sample.txt"})),
            (
                "grep",
                json!({"query":"PROJECT_A_MARKER", "path":"sample.txt"}),
            ),
            ("package_scripts", json!({})),
            ("glob", json!({"pattern":"*.txt"})),
            ("list_dir", json!({})),
        ] {
            let result =
                crate::tools::execute_subagent_readonly_in_context(&state, &execution, name, args)
                    .await
                    .unwrap();
            assert!(
                result.contains("PROJECT_A_MARKER"),
                "{name} returned {result}"
            );
            assert!(!result.contains("PROJECT_B_MARKER"));
        }
        let inspected = crate::tools::execute_subagent_readonly_in_context(
            &state,
            &execution,
            "context_inspect",
            json!({}),
        )
        .await
        .unwrap();
        assert!(inspected.contains("session-a"));
        let settings = state.settings.lock().unwrap().clone();
        let prompt = super::super::prompt::build_for_execution(
            &state, &execution, &settings, "", None, "inspect",
        );
        assert!(prompt.contains("PROJECT_A_MARKER"));
        assert!(!prompt.contains("PROJECT_B_MARKER"));
        let template = super::super::custom::load_agent_in_workspace(
            &state,
            &execution.workspace_root,
            "reader",
        )
        .unwrap();
        assert_eq!(template.prompt, "PROJECT_A_MARKER");
        assert!(crate::tools::execute_subagent_readonly_in_context(
            &state,
            &execution,
            "read_file",
            json!({"path":"../b/sample.txt"})
        )
        .await
        .is_err());
        state
            .sessions
            .lock()
            .unwrap()
            .sessions
            .retain(|session| session.id != "session-a");
        assert!(crate::tools::execute_subagent_readonly_in_context(
            &state,
            &execution,
            "read_file",
            json!({"path":"sample.txt"})
        )
        .await
        .is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn workflow_ownership_recovery_preserves_identity_and_rejects_relocated_snapshots() {
        let (root, state, a, b, run) = ownership_fixture();
        write_run_state_in_root(&a, &run).unwrap();
        let restored = read_run_state_in_root(&a, &run.run_id).unwrap();
        assert_eq!(restored.status, WorkflowStatus::StaleRunning);
        assert_eq!(restored.execution, run.execution);
        assert!(owned_definition(&state, &restored).is_ok());
        assert!(write_run_state_in_root(&b, &run).is_err());
        let original = workflow_journal::run_dir(&a, &run.run_id).join(RUN_STATE_FILE);
        let relocated = workflow_journal::run_dir(&b, &run.run_id).join(RUN_STATE_FILE);
        fs::create_dir_all(relocated.parent().unwrap()).unwrap();
        fs::copy(&original, &relocated).unwrap();
        assert!(read_run_state_in_root(&b, &run.run_id).is_none());
        let mut legacy: Value =
            serde_json::from_str(&fs::read_to_string(&original).unwrap()).unwrap();
        legacy["schema_version"] = json!(1);
        legacy["run"].as_object_mut().unwrap().remove("execution");
        legacy["run"]
            .as_object_mut()
            .unwrap()
            .remove("definition_snapshot");
        fs::write(&original, legacy.to_string()).unwrap();
        let legacy = read_run_state_in_root(&a, &run.run_id).unwrap();
        assert_eq!(storage_root(&legacy).unwrap(), a);
        assert!(owned_definition(&state, &legacy).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn workflow_ownership_retry_and_resume_use_captured_definition_and_owner() {
        let (root, state, a, b, mut run) = ownership_fixture();
        run.status = WorkflowStatus::Failed;
        run.failed_node = Some(WorkflowFailedNode {
            node_id: "first".to_string(),
            kind: "log".to_string(),
            label: "first".to_string(),
            phase: None,
            error: "test failure".to_string(),
            retryable: true,
        });
        write_run_state_in_root(&a, &run).unwrap();
        let overlay = resume_overlay(&state, &run.run_id).unwrap();
        assert!(overlay.contains("durable state snapshot"));
        assert!(overlay.contains("PROJECT_A_MARKER"));
        state.sessions.lock().unwrap().active = "session-b".to_string();
        *state.sandbox_dir.lock().unwrap() = b.clone();
        state.workflow_runs.lock().unwrap().push(run.clone());
        fs::remove_file(a.join(WORKFLOW_DIR).join("owner.json")).unwrap();
        assert!(resume_overlay(&state, &run.run_id)
            .unwrap_err()
            .contains("original session"));
        let retry = prepare_retry(&state, &run).unwrap();
        assert_eq!(retry.execution, run.execution);
        assert_eq!(retry.parent_run_id.as_deref(), Some(run.run_id.as_str()));
        assert_eq!(storage_root(&retry).unwrap(), a);
        assert!(
            matches!(&retry.definition_snapshot.as_ref().unwrap().steps[0], WorkflowStep::Log { message, .. } if message == "PROJECT_A_MARKER")
        );
        state.workflow_runs.lock().unwrap().push(retry.clone());
        workflow_journal::append(&state, &retry.run_id, "retry", json!({})).unwrap();
        persist_all_run_snapshots(&state);
        assert!(!b.join(workflow_journal::JOURNAL_DIR).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn workflow_ownership_project_switch_keeps_journal_and_snapshot() {
        let root = std::env::temp_dir().join(format!(
            "demiurge_workflow_ownership_{}",
            store::new_session_id()
        ));
        let project_a = root.join("a");
        let project_b = root.join("b");
        fs::create_dir_all(&project_a).unwrap();
        fs::create_dir_all(&project_b).unwrap();
        let project_a = fs::canonicalize(project_a).unwrap();
        let project_b = fs::canonicalize(project_b).unwrap();
        let state = crate::AppState::new(reqwest::Client::new());
        *state.sandbox_dir.lock().unwrap() = project_a.clone();
        let run: WorkflowRunProgress = serde_json::from_value(json!({
            "run_id": "wf_ownership", "name": "owner", "definition_name": "owner",
            "status": "running", "current_phase": null, "agents": [], "logs": [],
            "journal_path": workflow_journal::run_dir(&project_a, "wf_ownership").join("journal.jsonl"),
            "started_at": 1, "updated_at": 1, "error": null, "budget": {"total": null, "used_exact": 0, "used_estimated": 0},
            "steps_total": 0, "steps_done": 0,
            "execution": {"session_id": "session-a", "workspace_root": project_a},
            "definition_snapshot": {"schema_version": 1, "name": "owner", "description": null, "inputs": [], "steps": []}
        })).unwrap();
        state.workflow_runs.lock().unwrap().push(run);
        *state.sandbox_dir.lock().unwrap() = project_b.clone();
        workflow_journal::append(&state, "wf_ownership", "after_switch", json!({})).unwrap();
        persist_all_run_snapshots(&state);
        assert!(
            workflow_journal::run_dir(&project_a, "wf_ownership")
                .join("journal.jsonl")
                .exists(),
            "journal must stay in its launch project"
        );
        assert!(
            workflow_journal::run_dir(&project_a, "wf_ownership")
                .join(RUN_STATE_FILE)
                .exists(),
            "snapshot must stay in its launch project"
        );
        assert!(
            !project_b.join(workflow_journal::JOURNAL_DIR).exists(),
            "an unrelated selected project must receive no workflow writes"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sanitizes_workflow_names() {
        assert_eq!(sanitize_name(" review plan! "), "review-plan");
    }

    #[test]
    fn workflow_name_matches_sanitized_definition_name() {
        let dir = std::env::temp_dir().join(format!(
            "demiurge_workflow_name_{}",
            crate::store::now_millis()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("agent-review.json");
        std::fs::write(&path, r#"{ "name": "Agent Review", "steps": [] }"#).unwrap();

        assert_eq!(find_workflow_path(&dir, "Agent Review").unwrap(), path);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn parses_workflow_json() {
        let raw = r#"{
          "name": "demo",
          "steps": [
            {"type": "budget", "total": 12000},
            {"type": "log", "message": "hello"},
            {"type": "phase", "name": "find", "steps": [
              {"type": "agent", "label": "reader", "prompt": "inspect"}
            ]}
          ]
        }"#;
        let parsed = serde_json::from_str::<WorkflowFile>(raw).unwrap();
        assert_eq!(parsed.steps.len(), 3);
        match &parsed.steps[0] {
            WorkflowStep::Budget { total, .. } => assert_eq!(*total, Some(12000)),
            _ => panic!("expected budget step"),
        }
    }

    #[test]
    fn caps_long_agent_result() {
        assert!(cap_chars(&"x".repeat(20), 5).contains("truncated"));
    }

    #[test]
    fn writes_run_state_snapshot() {
        let root = std::env::temp_dir().join(format!(
            "demiurge_workflow_state_{}",
            store::new_session_id()
        ));
        let run = WorkflowRunProgress {
            run_id: "wf_state_test".to_string(),
            name: "state-test".to_string(),
            definition_name: "state-test".to_string(),
            execution: None,
            definition_snapshot: None,
            storage_root: None,
            status: WorkflowStatus::Killed,
            cancel_requested: true,
            current_phase: Some("phase-a".to_string()),
            agents: vec![WorkflowAgentProgress {
                id: 1,
                node_id: "reader".to_string(),
                label: "reader".to_string(),
                phase: Some("phase-a".to_string()),
                status: WorkflowStatus::Done,
                result: Some("ok".to_string()),
                error: None,
            }],
            logs: vec!["loaded demo".to_string()],
            journal_path: workflow_journal::run_dir(&root, "wf_state_test")
                .join("journal.jsonl")
                .to_string_lossy()
                .to_string(),
            started_at: 10,
            updated_at: 20,
            error: Some("stopped".to_string()),
            budget: budget::TokenBudgetState {
                total: Some(100),
                used_exact: 12,
                used_estimated: 8,
            },
            steps_total: 4,
            steps_done: 2,
            input_values: BTreeMap::new(),
            failed_node: None,
            parent_run_id: None,
            retry_node_id: None,
        };

        write_run_state_in_root(&root, &run).unwrap();
        let raw = std::fs::read_to_string(
            workflow_journal::run_dir(&root, "wf_state_test").join(RUN_STATE_FILE),
        )
        .unwrap();
        let parsed: WorkflowRunStateFile = serde_json::from_str(&raw).unwrap();

        assert_eq!(parsed.schema_version, RUN_STATE_SCHEMA_VERSION);
        assert_eq!(parsed.run.run_id, "wf_state_test");
        assert_eq!(parsed.run.status, WorkflowStatus::Killed);
        assert!(parsed.run.cancel_requested);
        assert_eq!(parsed.run.budget.used_total(), 20);
        assert_eq!(parsed.run.steps_done, 2);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn restores_running_snapshot_as_stale_run() {
        let root = std::env::temp_dir().join(format!(
            "demiurge_workflow_restore_{}",
            store::new_session_id()
        ));
        let run = WorkflowRunProgress {
            run_id: "wf_restore_test".to_string(),
            name: "restore-test".to_string(),
            definition_name: "restore-test".to_string(),
            execution: None,
            definition_snapshot: None,
            storage_root: None,
            status: WorkflowStatus::Running,
            cancel_requested: false,
            current_phase: Some("phase-a".to_string()),
            agents: Vec::new(),
            logs: vec!["loaded demo".to_string()],
            journal_path: workflow_journal::run_dir(&root, "wf_restore_test")
                .join("journal.jsonl")
                .to_string_lossy()
                .to_string(),
            started_at: 10,
            updated_at: 20,
            error: None,
            budget: budget::TokenBudgetState {
                total: Some(100),
                used_exact: 12,
                used_estimated: 8,
            },
            steps_total: 4,
            steps_done: 2,
            input_values: BTreeMap::new(),
            failed_node: None,
            parent_run_id: None,
            retry_node_id: None,
        };
        write_run_state_in_root(&root, &run).unwrap();

        let restored = list_run_states_in_root(&root);

        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].status, WorkflowStatus::StaleRunning);
        assert!(!restored[0].cancel_requested);
        assert_eq!(restored[0].budget.used_total(), 20);
        assert_eq!(restored[0].steps_done, 2);
        assert!(restored[0]
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("no live task"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_resume_overlay_refuses_to_guess_execution_identity() {
        let root = std::env::temp_dir().join(format!(
            "demiurge_workflow_resume_state_{}",
            store::new_session_id()
        ));
        let state = crate::AppState::new(reqwest::Client::new());
        *state.sandbox_dir.lock().unwrap() = root.clone();
        let run = WorkflowRunProgress {
            run_id: "wf_resume_state".to_string(),
            name: "resume-state".to_string(),
            definition_name: "resume-state".to_string(),
            execution: None,
            definition_snapshot: None,
            storage_root: None,
            status: WorkflowStatus::Failed,
            cancel_requested: false,
            current_phase: Some("verify".to_string()),
            agents: Vec::new(),
            logs: vec!["failed at verify".to_string()],
            journal_path: workflow_journal::run_dir(&root, "wf_resume_state")
                .join("journal.jsonl")
                .to_string_lossy()
                .to_string(),
            started_at: 10,
            updated_at: 20,
            error: Some("verification failed".to_string()),
            budget: budget::TokenBudgetState {
                total: Some(100),
                used_exact: 30,
                used_estimated: 0,
            },
            steps_total: 5,
            steps_done: 3,
            input_values: BTreeMap::new(),
            failed_node: Some(WorkflowFailedNode {
                node_id: "verify".to_string(),
                kind: "agent".to_string(),
                label: "verify".to_string(),
                phase: Some("verify".to_string()),
                error: "verification failed".to_string(),
                retryable: true,
            }),
            parent_run_id: None,
            retry_node_id: None,
        };
        write_run_state_in_root(&root, &run).unwrap();

        let error = resume_overlay(&state, "wf_resume_state").unwrap_err();
        assert!(error.contains("verified execution identity"));

        let _ = std::fs::remove_dir_all(root);
    }
}
