use std::collections::HashSet;
use std::fs;
use std::future::Future;
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use super::subagent::{SubagentContextMode, SubagentRequest};
use super::{budget, subagent, workflow_journal};
use crate::store;

const WORKFLOW_DIR: &str = ".demiurge/workflows";
const MAX_PARALLEL_ITEMS: usize = 8;
const RUN_STATE_SCHEMA_VERSION: u32 = 1;
const RUN_STATE_FILE: &str = "state.json";
const RUN_STATE_TMP_FILE: &str = "state.json.tmp";

type StepFuture<'a> = Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>>;

#[derive(Clone, Debug, Serialize)]
pub struct WorkflowDefinitionInfo {
    pub name: String,
    pub description: String,
    pub path: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct WorkflowPanelState {
    pub definitions: Vec<WorkflowDefinitionInfo>,
    pub runs: Vec<WorkflowRunProgress>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkflowRunProgress {
    pub run_id: String,
    pub name: String,
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
    pub label: String,
    pub phase: Option<String>,
    pub status: WorkflowStatus,
    pub result: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct WorkflowRunStateFile {
    schema_version: u32,
    run: WorkflowRunProgress,
}

#[derive(Clone, Debug, Deserialize)]
struct WorkflowFile {
    name: Option<String>,
    description: Option<String>,
    steps: Vec<WorkflowStep>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WorkflowStep {
    Log {
        message: String,
    },
    Phase {
        name: String,
        steps: Vec<WorkflowStep>,
    },
    Agent {
        prompt: String,
        label: Option<String>,
        agent_type: Option<String>,
        agent: Option<String>,
        context_mode: Option<String>,
    },
    Parallel {
        items: Vec<WorkflowStep>,
    },
    Pipeline {
        items: Vec<WorkflowStep>,
    },
    Budget {
        total: Option<usize>,
    },
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
        });
    }
    runs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    WorkflowPanelState { definitions, runs }
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
    match workflow_journal::resume_overlay(state, run_id) {
        Ok(overlay) => Ok(overlay),
        Err(journal_err) => {
            let sandbox = state.sandbox_dir.lock().unwrap().clone();
            let Some(run) = read_run_state_in_root(&sandbox, run_id) else {
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
            let parsed = serde_json::from_str::<WorkflowFile>(&raw).ok();
            let name = parsed
                .as_ref()
                .and_then(|w| w.name.clone())
                .or_else(|| path.file_stem().map(|s| s.to_string_lossy().to_string()))?;
            Some(WorkflowDefinitionInfo {
                name,
                description: parsed.and_then(|w| w.description).unwrap_or_default(),
                path: path.to_string_lossy().to_string(),
            })
        })
        .collect::<Vec<_>>();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn launch(app: &AppHandle, state: &crate::AppState, name: &str) -> Result<String, String> {
    let (workflow, path) = load_workflow(state, name)?;
    let run_id = workflow_journal::new_run_id();
    let journal_path = state
        .sandbox_dir
        .lock()
        .unwrap()
        .join(".demiurge")
        .join("workflow-runs")
        .join(&run_id)
        .join("journal.jsonl")
        .to_string_lossy()
        .to_string();
    let now = store::now_millis();
    let progress = WorkflowRunProgress {
        run_id: run_id.clone(),
        name: workflow.name.clone().unwrap_or_else(|| name.to_string()),
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
        steps_total: count_steps(&workflow.steps),
        steps_done: 0,
    };
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
        json!({ "name": name, "path": path.to_string_lossy() }),
    );
    Ok(run_id)
}

pub async fn run_launched(app: AppHandle, run_id: String, name: String) {
    let state = app.state::<crate::AppState>();
    let result = async {
        let (workflow, _) = load_workflow(state.inner(), &name)?;
        for step in workflow.steps {
            run_step(&app, state.inner(), &run_id, None, step).await?;
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
    step: WorkflowStep,
) -> StepFuture<'a> {
    Box::pin(async move {
        if is_cancelled(state, run_id) {
            return Ok(());
        }
        match step {
            WorkflowStep::Log { message } => {
                push_log(app, state, run_id, message.clone());
                let _ =
                    workflow_journal::append(state, run_id, "log", json!({ "message": message }));
            }
            WorkflowStep::Phase { name, steps } => {
                set_phase(app, state, run_id, Some(name.clone()));
                let _ = workflow_journal::append(
                    state,
                    run_id,
                    "phase_started",
                    json!({ "name": name }),
                );
                for child in steps {
                    run_step(app, state, run_id, Some(name.clone()), child).await?;
                }
                let _ =
                    workflow_journal::append(state, run_id, "phase_done", json!({ "name": name }));
            }
            WorkflowStep::Agent {
                prompt,
                label,
                agent_type,
                agent,
                context_mode,
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
                )
                .await?;
            }
            WorkflowStep::Parallel { items } => {
                if items.len() > MAX_PARALLEL_ITEMS {
                    return Err(format!(
                        "parallel 最多支持 {MAX_PARALLEL_ITEMS} 个 item，当前 {} 个。",
                        items.len()
                    ));
                }
                let futures = items
                    .into_iter()
                    .map(|item| run_step(app, state, run_id, phase.clone(), item))
                    .collect::<Vec<_>>();
                let results = futures_util::future::join_all(futures).await;
                for result in results {
                    result?;
                }
            }
            WorkflowStep::Pipeline { items } => {
                for item in items {
                    run_step(app, state, run_id, phase.clone(), item).await?;
                }
            }
            WorkflowStep::Budget { total } => {
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
                let _ =
                    workflow_journal::append(state, run_id, "budget", json!({ "total": total }));
            }
        }
        mark_step_done(app, state, run_id);
        Ok(())
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
) -> Result<(), String> {
    let id = next_agent_id(state, run_id);
    if workflow_budget(state, run_id).is_some_and(|budget| budget.is_exhausted()) {
        let message = "workflow token budget exhausted before agent step".to_string();
        push_log(app, state, run_id, message.clone());
        let _ = workflow_journal::append(state, run_id, "token_budget_exhausted", json!({}));
        return Err(message);
    }
    let label = label.unwrap_or_else(|| format!("agent-{id}"));
    push_agent(app, state, run_id, id, label.clone(), phase.clone());
    let _ = workflow_journal::append(
        state,
        run_id,
        "agent_started",
        json!({ "agent_id": id, "label": label, "phase": phase, "prompt": prompt, "agent": agent_name.clone() }),
    );
    let mode = SubagentContextMode::parse(context_mode.as_deref());
    let cancel = state.workflow_cancels.lock().unwrap().get(run_id).cloned();
    let result = subagent::run(
        state,
        SubagentRequest {
            prompt,
            label: Some(label.clone()),
            agent_type,
            agent_name,
            context_mode: mode,
            max_total_tokens: workflow_budget(state, run_id).and_then(|budget| budget.remaining()),
            output_format: subagent::SubagentOutputFormat::Plain,
            reviewer_count: 1,
            cancel,
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

fn load_workflow(state: &crate::AppState, name: &str) -> Result<(WorkflowFile, PathBuf), String> {
    let dir = ensure_dir(state)?;
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
    label: String,
    phase: Option<String>,
) {
    let mut runs = state.workflow_runs.lock().unwrap();
    if let Some(run) = runs.iter_mut().find(|run| run.run_id == run_id) {
        run.agents.push(WorkflowAgentProgress {
            id,
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

fn count_steps(steps: &[WorkflowStep]) -> usize {
    steps
        .iter()
        .map(|step| match step {
            WorkflowStep::Log { .. } | WorkflowStep::Agent { .. } | WorkflowStep::Budget { .. } => {
                1
            }
            WorkflowStep::Phase { steps, .. } => 1 + count_steps(steps),
            WorkflowStep::Parallel { items } | WorkflowStep::Pipeline { items } => {
                1 + count_steps(items)
            }
        })
        .sum()
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

fn persist_all_run_snapshots(state: &crate::AppState) {
    let runs = state.workflow_runs.lock().unwrap().clone();
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    for run in runs {
        if run.status == WorkflowStatus::Journaled {
            continue;
        }
        let _ = write_run_state_in_root(&sandbox, &run);
    }
}

fn write_run_state_in_root(root: &Path, run: &WorkflowRunProgress) -> Result<(), String> {
    let dir = workflow_journal::run_dir(root, &run.run_id);
    fs::create_dir_all(&dir).map_err(|e| format!("创建 workflow state 目录失败：{e}"))?;
    let target = dir.join(RUN_STATE_FILE);
    let tmp = dir.join(RUN_STATE_TMP_FILE);
    let payload = WorkflowRunStateFile {
        schema_version: RUN_STATE_SCHEMA_VERSION,
        run: run.clone(),
    };
    let body = serde_json::to_vec_pretty(&payload)
        .map_err(|e| format!("序列化 workflow state 失败：{e}"))?;
    fs::write(&tmp, body).map_err(|e| format!("写入 workflow state 临时文件失败：{e}"))?;
    if target.exists() {
        fs::remove_file(&target).map_err(|e| format!("替换 workflow state 失败：{e}"))?;
    }
    fs::rename(&tmp, &target).map_err(|e| format!("提交 workflow state 失败：{e}"))
}

fn list_persisted_run_states(state: &crate::AppState) -> Vec<WorkflowRunProgress> {
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    list_run_states_in_root(&sandbox)
}

fn read_run_state_in_root(root: &Path, run_id: &str) -> Option<WorkflowRunProgress> {
    read_run_state_file(&workflow_journal::run_dir(root, run_id).join(RUN_STATE_FILE))
        .map(normalize_restored_run)
}

fn list_run_states_in_root(root: &Path) -> Vec<WorkflowRunProgress> {
    let runs_dir = root.join(workflow_journal::JOURNAL_DIR);
    let Ok(entries) = fs::read_dir(runs_dir) else {
        return Vec::new();
    };
    let mut runs = entries
        .filter_map(Result::ok)
        .filter_map(|entry| read_run_state_file(&entry.path().join(RUN_STATE_FILE)))
        .map(normalize_restored_run)
        .collect::<Vec<_>>();
    runs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    runs
}

fn read_run_state_file(path: &Path) -> Option<WorkflowRunProgress> {
    let raw = fs::read_to_string(path).ok()?;
    let parsed = serde_json::from_str::<WorkflowRunStateFile>(&raw).ok()?;
    if parsed.schema_version != RUN_STATE_SCHEMA_VERSION {
        return None;
    }
    if parsed.run.run_id.trim().is_empty() {
        return None;
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
            WorkflowStep::Budget { total } => assert_eq!(*total, Some(12000)),
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
            status: WorkflowStatus::Killed,
            cancel_requested: true,
            current_phase: Some("phase-a".to_string()),
            agents: vec![WorkflowAgentProgress {
                id: 1,
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
    fn resume_overlay_falls_back_to_state_snapshot() {
        let root = std::env::temp_dir().join(format!(
            "demiurge_workflow_resume_state_{}",
            store::new_session_id()
        ));
        let state = crate::AppState::new(reqwest::Client::new());
        *state.sandbox_dir.lock().unwrap() = root.clone();
        let run = WorkflowRunProgress {
            run_id: "wf_resume_state".to_string(),
            name: "resume-state".to_string(),
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
        };
        write_run_state_in_root(&root, &run).unwrap();

        let overlay = resume_overlay(&state, "wf_resume_state").unwrap();

        assert!(overlay.contains("durable state snapshot"));
        assert!(overlay.contains("wf_resume_state"));
        assert!(overlay.contains("\"steps_done\": 3"));

        let _ = std::fs::remove_dir_all(root);
    }
}
