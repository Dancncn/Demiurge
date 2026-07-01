use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, UserAttentionType};

const POMODORO_FILE: &str = "pomodoro.json";
const MIN_DURATION_MINUTES: u64 = 1;
const MAX_DURATION_MINUTES: u64 = 240;
const DEFAULT_FOCUS_MINUTES: u64 = 25;
const DEFAULT_SHORT_BREAK_MINUTES: u64 = 5;
const DEFAULT_LONG_BREAK_MINUTES: u64 = 15;

#[derive(Default)]
pub struct PomodoroRuntime {
    cancel: Option<Arc<AtomicBool>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PomodoroStore {
    #[serde(default)]
    pub timer: PomodoroTimer,
    #[serde(default)]
    pub rhythm: PomodoroRhythmMemory,
}

impl Default for PomodoroStore {
    fn default() -> Self {
        Self {
            timer: PomodoroTimer::default(),
            rhythm: PomodoroRhythmMemory::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PomodoroTimer {
    pub status: String,
    pub mode: String,
    pub run_id: Option<String>,
    pub duration_secs: u64,
    pub remaining_secs: u64,
    pub started_at: Option<u64>,
    pub ends_at: Option<u64>,
    pub paused_at: Option<u64>,
    pub completed_focus_count: u32,
    pub focus_streak: u32,
    pub task: PomodoroTaskBinding,
    pub feedback: PomodoroFeedback,
    pub updated_at: u64,
}

impl Default for PomodoroTimer {
    fn default() -> Self {
        Self {
            status: "idle".to_string(),
            mode: "focus".to_string(),
            run_id: None,
            duration_secs: DEFAULT_FOCUS_MINUTES * 60,
            remaining_secs: 0,
            started_at: None,
            ends_at: None,
            paused_at: None,
            completed_focus_count: 0,
            focus_streak: 0,
            task: PomodoroTaskBinding::default(),
            feedback: PomodoroFeedback::default(),
            updated_at: crate::store::now_millis(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PomodoroTaskBinding {
    pub kind: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_objective: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_run_id: Option<String>,
}

impl Default for PomodoroTaskBinding {
    fn default() -> Self {
        Self {
            kind: "manual".to_string(),
            title: String::new(),
            session_id: None,
            goal_objective: None,
            workflow_run_id: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PomodoroFeedback {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_message: Option<String>,
    #[serde(default)]
    pub plan_steps: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recap_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encouragement: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PomodoroRhythmMemory {
    #[serde(default)]
    pub focus_sessions_completed: u32,
    #[serde(default)]
    pub focus_duration_counts: HashMap<u64, u32>,
    #[serde(default)]
    pub interruption_reasons: HashMap<String, u32>,
    #[serde(default)]
    pub efficient_hour_counts: HashMap<u8, u32>,
    #[serde(default)]
    pub last_completed_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PomodoroPanelState {
    pub timer: PomodoroTimer,
    pub rhythm: PomodoroRhythmMemory,
    pub remaining_secs: u64,
    pub next_mode: String,
    pub path: String,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct PomodoroCompletedEvent {
    pub title: String,
    pub body: String,
    pub state: PomodoroPanelState,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PomodoroStartRequest {
    pub mode: String,
    #[serde(default)]
    pub duration_minutes: Option<u64>,
    #[serde(default)]
    pub task: Option<PomodoroTaskBinding>,
}

pub fn hydrate(app: AppHandle, state: &crate::AppState) {
    let store = read_store_for_state(state);
    if store.timer.status != "running" {
        return;
    }
    let Some(run_id) = store.timer.run_id.clone() else {
        return;
    };
    let Some(ends_at) = store.timer.ends_at else {
        return;
    };
    if ends_at <= crate::store::now_millis() {
        if let Err(e) = complete_run(&app, state, &run_id) {
            eprintln!("Demiurge pomodoro hydration warning: {e}");
        }
    } else {
        spawn_completion_task(app, state, run_id, ends_at);
    }
}

pub fn panel_state(state: &crate::AppState) -> PomodoroPanelState {
    let data_dir = state.data_dir.lock().unwrap().clone();
    panel_state_from_store(&data_dir, read_store(&data_dir))
}

pub fn start(
    app: AppHandle,
    state: &crate::AppState,
    request: PomodoroStartRequest,
) -> Result<PomodoroPanelState, String> {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let mut store = read_store(&data_dir);
    let mode = normalize_mode(&request.mode)?;
    let minutes = duration_minutes_for_mode(&mode, request.duration_minutes)?;
    let duration_secs = minutes * 60;
    let now = crate::store::now_millis();
    let run_id = format!("p_{}", now);
    let task = request.task.unwrap_or_default();
    let title = normalized_task_title(&task).unwrap_or_else(|| default_task_title(&mode));
    let feedback_title = title.clone();
    cancel_runtime_timer(state);
    store.timer = PomodoroTimer {
        status: "running".to_string(),
        mode: mode.clone(),
        run_id: Some(run_id.clone()),
        duration_secs,
        remaining_secs: duration_secs,
        started_at: Some(now),
        ends_at: Some(now + duration_secs * 1000),
        paused_at: None,
        completed_focus_count: store.timer.completed_focus_count,
        focus_streak: store.timer.focus_streak,
        task: PomodoroTaskBinding { title, ..task },
        feedback: PomodoroFeedback {
            start_message: Some(start_message(&mode, minutes, &feedback_title)),
            plan_steps: plan_steps(&mode, &feedback_title),
            completion_message: None,
            recap_prompt: None,
            encouragement: None,
        },
        updated_at: now,
    };
    write_store(&data_dir, &store)?;
    let due_at = store.timer.ends_at.unwrap_or(now);
    spawn_completion_task(app.clone(), state, run_id, due_at);
    let panel = panel_state_from_store(&data_dir, store);
    emit_update(&app, &panel);
    Ok(panel)
}

pub fn pause(app: AppHandle, state: &crate::AppState) -> Result<PomodoroPanelState, String> {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let mut store = read_store(&data_dir);
    if store.timer.status != "running" {
        return Err("No running pomodoro timer to pause.".to_string());
    }
    let now = crate::store::now_millis();
    store.timer.remaining_secs = remaining_secs(&store.timer, now);
    store.timer.status = "paused".to_string();
    store.timer.paused_at = Some(now);
    store.timer.ends_at = None;
    store.timer.updated_at = now;
    cancel_runtime_timer(state);
    write_store(&data_dir, &store)?;
    let panel = panel_state_from_store(&data_dir, store);
    emit_update(&app, &panel);
    Ok(panel)
}

pub fn resume(app: AppHandle, state: &crate::AppState) -> Result<PomodoroPanelState, String> {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let mut store = read_store(&data_dir);
    if store.timer.status != "paused" {
        return Err("No paused pomodoro timer to resume.".to_string());
    }
    let now = crate::store::now_millis();
    let remaining = store.timer.remaining_secs.max(1);
    let run_id = store
        .timer
        .run_id
        .clone()
        .unwrap_or_else(|| format!("p_{}", now));
    store.timer.status = "running".to_string();
    store.timer.paused_at = None;
    store.timer.ends_at = Some(now + remaining * 1000);
    store.timer.updated_at = now;
    write_store(&data_dir, &store)?;
    let due_at = store.timer.ends_at.unwrap_or(now);
    spawn_completion_task(app.clone(), state, run_id, due_at);
    let panel = panel_state_from_store(&data_dir, store);
    emit_update(&app, &panel);
    Ok(panel)
}

pub fn skip(app: AppHandle, state: &crate::AppState) -> Result<PomodoroPanelState, String> {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let mut store = read_store(&data_dir);
    if store.timer.status == "idle" {
        return Err("No active pomodoro timer to skip.".to_string());
    }
    cancel_runtime_timer(state);
    let now = crate::store::now_millis();
    store.timer.status = "idle".to_string();
    store.timer.run_id = None;
    store.timer.remaining_secs = 0;
    store.timer.ends_at = None;
    store.timer.paused_at = None;
    store.timer.feedback.completion_message = Some("这轮已经跳过，稍后可以重新开始。".to_string());
    store.timer.feedback.recap_prompt = None;
    store.timer.feedback.encouragement = None;
    store.timer.updated_at = now;
    write_store(&data_dir, &store)?;
    let panel = panel_state_from_store(&data_dir, store);
    emit_update(&app, &panel);
    Ok(panel)
}

fn complete_run(app: &AppHandle, state: &crate::AppState, run_id: &str) -> Result<(), String> {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let mut store = read_store(&data_dir);
    if store.timer.status != "running" || store.timer.run_id.as_deref() != Some(run_id) {
        return Ok(());
    }
    let now = crate::store::now_millis();
    let completed_mode = store.timer.mode.clone();
    store.timer.status = "idle".to_string();
    store.timer.run_id = None;
    store.timer.remaining_secs = 0;
    store.timer.ends_at = None;
    store.timer.paused_at = None;
    store.timer.feedback.completion_message = Some(completion_message(
        &completed_mode,
        store.timer.focus_streak,
    ));
    store.timer.feedback.recap_prompt = recap_prompt(&completed_mode);
    store.timer.feedback.encouragement = encouragement(&completed_mode, store.timer.focus_streak);
    if completed_mode == "focus" {
        store.timer.completed_focus_count = store.timer.completed_focus_count.saturating_add(1);
        store.timer.focus_streak = store.timer.focus_streak.saturating_add(1);
    } else {
        store.timer.focus_streak = 0;
    }
    store.timer.updated_at = now;
    write_store(&data_dir, &store)?;
    cancel_runtime_timer(state);
    let panel = panel_state_from_store(&data_dir, store);
    emit_update(app, &panel);
    let event = PomodoroCompletedEvent {
        title: notification_title(&completed_mode).to_string(),
        body: notification_body(&panel.timer),
        state: panel,
    };
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.request_user_attention(Some(UserAttentionType::Informational));
    }
    let _ = app.emit("pomodoro-completed", event);
    Ok(())
}

fn read_store_for_state(state: &crate::AppState) -> PomodoroStore {
    let data_dir = state.data_dir.lock().unwrap().clone();
    read_store(&data_dir)
}

fn pomodoro_path(data_dir: &Path) -> PathBuf {
    data_dir.join(POMODORO_FILE)
}

fn read_store(data_dir: &Path) -> PomodoroStore {
    fs::read_to_string(pomodoro_path(data_dir))
        .ok()
        .and_then(|raw| serde_json::from_str::<PomodoroStore>(&raw).ok())
        .unwrap_or_default()
}

fn write_store(data_dir: &Path, store: &PomodoroStore) -> Result<(), String> {
    fs::create_dir_all(data_dir).map_err(|e| format!("Failed to create data directory: {e}"))?;
    let raw = serde_json::to_string_pretty(store)
        .map_err(|e| format!("Failed to serialize pomodoro state: {e}"))?;
    fs::write(pomodoro_path(data_dir), raw)
        .map_err(|e| format!("Failed to write pomodoro state: {e}"))
}

fn panel_state_from_store(data_dir: &Path, mut store: PomodoroStore) -> PomodoroPanelState {
    let now = crate::store::now_millis();
    store.timer.remaining_secs = remaining_secs(&store.timer, now);
    PomodoroPanelState {
        remaining_secs: store.timer.remaining_secs,
        next_mode: next_mode(&store.timer),
        path: pomodoro_path(data_dir).display().to_string(),
        updated_at: now,
        timer: store.timer,
        rhythm: store.rhythm,
    }
}

fn spawn_completion_task(app: AppHandle, state: &crate::AppState, run_id: String, due_at: u64) {
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut runtime = state.pomodoro.lock().unwrap();
        if let Some(old) = runtime.cancel.take() {
            old.store(true, Ordering::Relaxed);
        }
        runtime.cancel = Some(cancel.clone());
    }
    tauri::async_runtime::spawn(async move {
        loop {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let now = crate::store::now_millis();
            if now >= due_at {
                let state = app.state::<crate::AppState>();
                if let Err(e) = complete_run(&app, state.inner(), &run_id) {
                    eprintln!("Demiurge pomodoro completion warning: {e}");
                }
                return;
            }
            tokio::time::sleep(Duration::from_millis((due_at - now).min(1000))).await;
        }
    });
}

fn cancel_runtime_timer(state: &crate::AppState) {
    let mut runtime = state.pomodoro.lock().unwrap();
    if let Some(cancel) = runtime.cancel.take() {
        cancel.store(true, Ordering::Relaxed);
    }
}

fn emit_update(app: &AppHandle, panel: &PomodoroPanelState) {
    let _ = app.emit("pomodoro-updated", panel);
}

fn normalize_mode(mode: &str) -> Result<String, String> {
    match mode.trim().to_ascii_lowercase().as_str() {
        "focus" | "" => Ok("focus".to_string()),
        "short_break" | "short-break" | "short" => Ok("short_break".to_string()),
        "long_break" | "long-break" | "long" => Ok("long_break".to_string()),
        "custom" => Ok("custom".to_string()),
        other => Err(format!("Unsupported pomodoro mode: {other}")),
    }
}

fn duration_minutes_for_mode(mode: &str, requested: Option<u64>) -> Result<u64, String> {
    let minutes = match (mode, requested) {
        ("focus", None) => DEFAULT_FOCUS_MINUTES,
        ("short_break", None) => DEFAULT_SHORT_BREAK_MINUTES,
        ("long_break", None) => DEFAULT_LONG_BREAK_MINUTES,
        ("custom", None) => {
            return Err("Custom pomodoro duration requires duration_minutes.".to_string())
        }
        (_, Some(value)) => value,
        _ => DEFAULT_FOCUS_MINUTES,
    };
    if !(MIN_DURATION_MINUTES..=MAX_DURATION_MINUTES).contains(&minutes) {
        return Err(format!(
            "Pomodoro duration must be between {MIN_DURATION_MINUTES} and {MAX_DURATION_MINUTES} minutes."
        ));
    }
    Ok(minutes)
}

fn remaining_secs(timer: &PomodoroTimer, now: u64) -> u64 {
    match timer.status.as_str() {
        "running" => timer
            .ends_at
            .map(|ends_at| ends_at.saturating_sub(now).saturating_add(999) / 1000)
            .unwrap_or(timer.remaining_secs),
        "paused" => timer.remaining_secs,
        _ => 0,
    }
}

fn next_mode(timer: &PomodoroTimer) -> String {
    if timer.mode != "focus" {
        return "focus".to_string();
    }
    if timer.completed_focus_count > 0 && timer.completed_focus_count % 4 == 0 {
        "long_break".to_string()
    } else {
        "short_break".to_string()
    }
}

fn normalized_task_title(task: &PomodoroTaskBinding) -> Option<String> {
    let title = task.title.trim();
    if title.is_empty() {
        None
    } else {
        Some(title.chars().take(120).collect())
    }
}

fn default_task_title(mode: &str) -> String {
    match mode {
        "short_break" => "Short break".to_string(),
        "long_break" => "Long break".to_string(),
        _ => "Focus session".to_string(),
    }
}

fn start_message(mode: &str, minutes: u64, title: &str) -> String {
    match mode {
        "short_break" => format!("短休息 {minutes} 分钟开始。离开屏幕、喝水，回来再继续。"),
        "long_break" => format!("长休息 {minutes} 分钟开始。让脑子真的换个频道。"),
        _ => format!("专注 {minutes} 分钟开始。先把「{title}」拆成一个可交付的小动作。"),
    }
}

fn plan_steps(mode: &str, title: &str) -> Vec<String> {
    match mode {
        "focus" | "custom" => vec![
            format!("把目标缩成一句：这轮结束时「{title}」要多出什么结果。"),
            "先做 2 分钟准备：打开必要文件、关闭无关入口。".to_string(),
            "只推进一个最小交付；如果卡住，记录阻塞点而不是硬熬。".to_string(),
        ],
        "short_break" => vec![
            "站起来离开屏幕。".to_string(),
            "喝水或伸展，不顺手打开新的信息流。".to_string(),
        ],
        "long_break" => vec![
            "离开当前任务上下文。".to_string(),
            "让眼睛和肩颈休息，回来后再决定下一轮目标。".to_string(),
        ],
        _ => Vec::new(),
    }
}

fn completion_message(mode: &str, focus_streak_before_increment: u32) -> String {
    match mode {
        "short_break" => "短休息结束。可以回到下一轮专注了。".to_string(),
        "long_break" => "长休息结束。下一轮从最轻的动作重新启动。".to_string(),
        _ if focus_streak_before_increment >= 2 => {
            "这一轮完成了，连续专注节奏不错。先记一个小复盘，再决定是否休息。".to_string()
        }
        _ => "这一轮完成了。花十秒写下做到了什么、下一步是什么。".to_string(),
    }
}

fn recap_prompt(mode: &str) -> Option<String> {
    if mode == "focus" {
        Some("复盘 10 秒：完成了什么？下一步是什么？有什么中断要记下？".to_string())
    } else {
        None
    }
}

fn encouragement(mode: &str, focus_streak_before_increment: u32) -> Option<String> {
    if mode != "focus" {
        return None;
    }
    let streak = focus_streak_before_increment.saturating_add(1);
    Some(if streak >= 3 {
        format!("已经连续完成 {streak} 轮专注。保持轻量节奏，下一轮前也可以认真休息。")
    } else if streak == 2 {
        "连续两轮完成。不错，先别急着加码，稳住就很有用。".to_string()
    } else {
        "完成一轮就算数。小步推进比把自己燃尽更可靠。".to_string()
    })
}

fn notification_title(mode: &str) -> &'static str {
    match mode {
        "short_break" => "短休息结束",
        "long_break" => "长休息结束",
        _ => "专注结束",
    }
}

fn notification_body(timer: &PomodoroTimer) -> String {
    let task =
        normalized_task_title(&timer.task).unwrap_or_else(|| default_task_title(&timer.mode));
    format!(
        "{task} · {}",
        timer
            .feedback
            .completion_message
            .clone()
            .unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_duration_requires_valid_minutes() {
        assert!(duration_minutes_for_mode("custom", None).is_err());
        assert_eq!(duration_minutes_for_mode("custom", Some(45)).unwrap(), 45);
        assert!(duration_minutes_for_mode("focus", Some(0)).is_err());
        assert!(duration_minutes_for_mode("focus", Some(241)).is_err());
    }

    #[test]
    fn running_timer_remaining_is_ceil_seconds() {
        let timer = PomodoroTimer {
            status: "running".to_string(),
            ends_at: Some(10_500),
            ..PomodoroTimer::default()
        };
        assert_eq!(remaining_secs(&timer, 10_001), 1);
        assert_eq!(remaining_secs(&timer, 10_500), 0);
    }

    #[test]
    fn panel_state_exposes_next_break_mode() {
        let timer = PomodoroTimer {
            mode: "focus".to_string(),
            completed_focus_count: 4,
            ..PomodoroTimer::default()
        };
        let store = PomodoroStore {
            timer,
            rhythm: PomodoroRhythmMemory::default(),
        };
        let panel = panel_state_from_store(Path::new("."), store);
        assert_eq!(panel.next_mode, "long_break");
    }

    #[test]
    fn feedback_breaks_focus_goal_into_steps_and_encourages_streaks() {
        let steps = plan_steps("focus", "finish review");
        assert_eq!(steps.len(), 3);
        assert!(steps[0].contains("finish review"));
        assert!(recap_prompt("focus").unwrap().contains("复盘"));
        assert!(encouragement("focus", 2).unwrap().contains("连续完成 3 轮"));
    }
}
