use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::Emitter;

use super::budget;
use crate::store;

pub const BLOCKED_CONSECUTIVE_THRESHOLD: usize = 3;
pub const MAX_GOAL_TURNS: usize = 150;
const MAX_OBJECTIVE_CHARS: usize = 4000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Active,
    Paused,
    Blocked,
    BudgetLimited,
    UsageLimited,
    MaxTurns,
    Complete,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GoalState {
    pub objective: String,
    pub status: GoalStatus,
    pub token_budget: Option<usize>,
    pub tokens_used: usize,
    pub start_time: u64,
    pub paused_at: Option<u64>,
    pub accumulated_active_ms: u64,
    pub blocked_attempts: usize,
    pub last_block_reason: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    pub turns_executed: usize,
    #[serde(default)]
    pub budget_limit_notified: bool,
}

pub enum GoalSlashOutcome {
    Respond(String),
    Query {
        stored_user_text: String,
        system_overlay: String,
    },
}

pub fn handle_slash(state: &crate::AppState, text: &str) -> Result<GoalSlashOutcome, String> {
    let args = text.trim().trim_start_matches("/goal").trim();
    if args.is_empty() || args.eq_ignore_ascii_case("status") {
        return Ok(GoalSlashOutcome::Respond(status_text(state)));
    }

    let lower = args.to_ascii_lowercase();
    match lower.as_str() {
        "clear" => {
            let cleared = clear_goal(state);
            state.persist_sessions();
            Ok(GoalSlashOutcome::Respond(if cleared {
                "Goal cleared.".to_string()
            } else {
                "No active goal to clear.".to_string()
            }))
        }
        "pause" => {
            let paused = pause_goal(state).is_some();
            state.persist_sessions();
            Ok(GoalSlashOutcome::Respond(if paused {
                "Goal paused.".to_string()
            } else {
                "No active goal to pause.".to_string()
            }))
        }
        "resume" => {
            if active_goal(state)
                .map(|goal| goal.status == GoalStatus::MaxTurns)
                .unwrap_or(false)
            {
                return Ok(GoalSlashOutcome::Respond(format!(
                    "Goal reached max continuation turns ({MAX_GOAL_TURNS}). Run `/goal continue` to reset the counter."
                )));
            }
            let resumed = resume_goal(state).is_some();
            state.persist_sessions();
            if resumed {
                Ok(GoalSlashOutcome::Query {
                    stored_user_text: "[Goal resumed]".to_string(),
                    system_overlay: active_goal(state)
                        .map(|goal| build_continuation_prompt(&goal))
                        .unwrap_or_default(),
                })
            } else {
                Ok(GoalSlashOutcome::Respond(
                    "No paused goal to resume.".to_string(),
                ))
            }
        }
        "continue" => {
            let continued = continue_from_max_turns(state).is_some();
            state.persist_sessions();
            if continued {
                Ok(GoalSlashOutcome::Query {
                    stored_user_text: "[Goal continued]".to_string(),
                    system_overlay: active_goal(state)
                        .map(|goal| build_continuation_prompt(&goal))
                        .unwrap_or_default(),
                })
            } else {
                Ok(GoalSlashOutcome::Respond(
                    "Current goal is not in max-turns state.".to_string(),
                ))
            }
        }
        "complete" => {
            let completed = complete_goal(state).is_some();
            state.persist_sessions();
            Ok(GoalSlashOutcome::Respond(if completed {
                "Goal marked complete.".to_string()
            } else {
                "No active goal to complete.".to_string()
            }))
        }
        _ => {
            if args.chars().count() > MAX_OBJECTIVE_CHARS {
                return Err(format!(
                    "Goal objective is too long (limit {MAX_OBJECTIVE_CHARS} chars). Save details to a file and reference it from a shorter objective."
                ));
            }
            let (objective, token_budget) = parse_objective_and_budget(args);
            if objective.trim().is_empty() {
                return Err("Goal objective cannot be empty.".to_string());
            }
            let previous = active_goal(state).map(|goal| goal.objective);
            set_goal(state, objective.clone(), token_budget);
            increment_turns(state);
            state.persist_sessions();
            Ok(GoalSlashOutcome::Query {
                stored_user_text: objective.clone(),
                system_overlay: build_objective_updated_prompt(&objective, previous.as_deref()),
            })
        }
    }
}

pub async fn drive_after_turn(
    app: &tauri::AppHandle,
    state: &crate::AppState,
) -> Result<(), String> {
    loop {
        if state.cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(());
        }

        let Some(goal) = active_goal(state) else {
            return Ok(());
        };

        match goal.status {
            GoalStatus::Active => {
                if goal.turns_executed >= MAX_GOAL_TURNS {
                    mark_max_turns(state);
                    state.persist_sessions();
                    let _ = app.emit(
                        "assistant-done",
                        format!(
                            "Goal reached max continuation turns ({MAX_GOAL_TURNS}). Run `/goal continue` to reset and continue."
                        ),
                    );
                    return Ok(());
                }

                let turns = increment_turns(state);
                state.persist_sessions();
                let Some(next_goal) = active_goal(state) else {
                    return Ok(());
                };
                let overlay = build_continuation_prompt(&next_goal);
                let _ = app.emit(
                    "goal-progress",
                    json!({
                        "status": status_value(&next_goal.status),
                        "message": format!("Goal continuation #{turns} started."),
                        "turns_executed": next_goal.turns_executed,
                        "tokens_used": next_goal.tokens_used,
                        "token_budget": next_goal.token_budget,
                    }),
                );
                let stored_user_text = format!("[Goal continuation #{turns}]");
                super::run_turn_with_options(
                    app,
                    state,
                    stored_user_text.clone(),
                    super::TurnOptions {
                        system_overlay: Some(overlay),
                        stored_user_text: Some(stored_user_text),
                        workflow_run_id: None,
                    },
                )
                .await?;
            }
            GoalStatus::BudgetLimited => {
                if goal.budget_limit_notified {
                    return Ok(());
                }
                mark_budget_notified(state);
                state.persist_sessions();
                let overlay = build_budget_limit_prompt(&goal);
                let _ = app.emit(
                    "goal-progress",
                    json!({
                        "status": status_value(&goal.status),
                        "message": "Goal token budget reached; preparing budget summary.",
                        "turns_executed": goal.turns_executed,
                        "tokens_used": goal.tokens_used,
                        "token_budget": goal.token_budget,
                    }),
                );
                super::run_turn_with_options(
                    app,
                    state,
                    "[Goal budget limit]".to_string(),
                    super::TurnOptions {
                        system_overlay: Some(overlay),
                        stored_user_text: Some("[Goal budget limit]".to_string()),
                        workflow_run_id: None,
                    },
                )
                .await?;
                return Ok(());
            }
            _ => return Ok(()),
        }
    }
}

pub fn active_goal(state: &crate::AppState) -> Option<GoalState> {
    let store = state.sessions.lock().unwrap();
    store
        .get(&store.active)
        .and_then(|session| session.goal.clone())
}

pub fn set_goal(
    state: &crate::AppState,
    objective: String,
    token_budget: Option<usize>,
) -> GoalState {
    let now = store::now_millis();
    let goal = GoalState {
        objective,
        status: GoalStatus::Active,
        token_budget,
        tokens_used: 0,
        start_time: now,
        paused_at: None,
        accumulated_active_ms: 0,
        blocked_attempts: 0,
        last_block_reason: None,
        created_at: now,
        updated_at: now,
        turns_executed: 0,
        budget_limit_notified: false,
    };
    let mut store = state.sessions.lock().unwrap();
    let active = store.active.clone();
    if let Some(session) = store.get_mut(&active) {
        session.goal = Some(goal.clone());
        session.updated_at = now;
    }
    goal
}

pub fn clear_goal(state: &crate::AppState) -> bool {
    let mut store = state.sessions.lock().unwrap();
    let active = store.active.clone();
    let Some(session) = store.get_mut(&active) else {
        return false;
    };
    let had = session.goal.is_some();
    session.goal = None;
    session.updated_at = store::now_millis();
    had
}

pub fn pause_goal(state: &crate::AppState) -> Option<GoalState> {
    mutate_active_goal(state, |goal| {
        if goal.status != GoalStatus::Active {
            return None;
        }
        let now = store::now_millis();
        goal.accumulated_active_ms = goal
            .accumulated_active_ms
            .saturating_add(now.saturating_sub(goal.start_time));
        goal.paused_at = Some(now);
        goal.status = GoalStatus::Paused;
        goal.updated_at = now;
        Some(goal.clone())
    })
    .flatten()
}

pub fn resume_goal(state: &crate::AppState) -> Option<GoalState> {
    mutate_active_goal(state, |goal| {
        if goal.status != GoalStatus::Paused {
            return None;
        }
        let now = store::now_millis();
        goal.start_time = now;
        goal.paused_at = None;
        goal.status = GoalStatus::Active;
        goal.blocked_attempts = 0;
        goal.last_block_reason = None;
        goal.updated_at = now;
        Some(goal.clone())
    })
    .flatten()
}

pub fn continue_from_max_turns(state: &crate::AppState) -> Option<GoalState> {
    mutate_active_goal(state, |goal| {
        if goal.status != GoalStatus::MaxTurns {
            return None;
        }
        let now = store::now_millis();
        goal.turns_executed = 0;
        goal.status = GoalStatus::Active;
        goal.start_time = now;
        goal.paused_at = None;
        goal.blocked_attempts = 0;
        goal.last_block_reason = None;
        goal.updated_at = now;
        Some(goal.clone())
    })
    .flatten()
}

pub fn complete_goal(state: &crate::AppState) -> Option<GoalState> {
    mutate_active_goal(state, |goal| {
        let now = store::now_millis();
        if goal.status == GoalStatus::Active && goal.paused_at.is_none() {
            goal.accumulated_active_ms = goal
                .accumulated_active_ms
                .saturating_add(now.saturating_sub(goal.start_time));
        }
        goal.status = GoalStatus::Complete;
        goal.updated_at = now;
        Some(goal.clone())
    })
    .flatten()
}

pub fn record_blocked_attempt(
    state: &crate::AppState,
    reason: &str,
) -> Option<(GoalStatus, usize)> {
    mutate_active_goal(state, |goal| {
        if goal.status != GoalStatus::Active {
            return None;
        }
        let normalized = reason.trim().to_ascii_lowercase();
        if goal
            .last_block_reason
            .as_deref()
            .map(|last| last.trim().to_ascii_lowercase() != normalized)
            .unwrap_or(false)
        {
            goal.blocked_attempts = 0;
        }
        goal.last_block_reason = Some(reason.to_string());
        goal.blocked_attempts += 1;
        if goal.blocked_attempts >= BLOCKED_CONSECUTIVE_THRESHOLD {
            goal.status = GoalStatus::Blocked;
        }
        goal.updated_at = store::now_millis();
        Some((goal.status.clone(), goal.blocked_attempts))
    })
    .flatten()
}

pub fn increment_turns(state: &crate::AppState) -> usize {
    mutate_active_goal(state, |goal| {
        goal.turns_executed += 1;
        goal.updated_at = store::now_millis();
        goal.turns_executed
    })
    .unwrap_or(0)
}

pub fn add_provider_usage(
    state: &crate::AppState,
    session_id: &str,
    usage: Option<&crate::llm::Usage>,
) -> bool {
    let Some(tokens) = usage.and_then(|usage| usage.total_or_sum()) else {
        return false;
    };
    add_tokens(state, session_id, tokens);
    true
}

pub fn add_estimated_tokens(state: &crate::AppState, session_id: &str, text: &str) {
    let tokens = budget::estimate_text_tokens(text);
    if tokens == 0 {
        return;
    }
    add_tokens(state, session_id, tokens);
}

fn add_tokens(state: &crate::AppState, session_id: &str, tokens: usize) {
    if tokens == 0 {
        return;
    }
    let mut store = state.sessions.lock().unwrap();
    let Some(session) = store.get_mut(session_id) else {
        return;
    };
    let Some(goal) = session.goal.as_mut() else {
        return;
    };
    if goal.status != GoalStatus::Active {
        return;
    }
    goal.tokens_used = goal.tokens_used.saturating_add(tokens);
    goal.updated_at = store::now_millis();
    if goal
        .token_budget
        .map(|budget| goal.tokens_used >= budget)
        .unwrap_or(false)
    {
        goal.status = GoalStatus::BudgetLimited;
    }
}

pub fn status_text(state: &crate::AppState) -> String {
    let Some(goal) = active_goal(state) else {
        return "No active goal. Set one with `/goal <objective>`.".to_string();
    };
    let tokens = match goal.token_budget {
        Some(budget) => format!("{} / {}", goal.tokens_used, budget),
        None => goal.tokens_used.to_string(),
    };
    let mut lines = vec![
        format!("Goal: {}", goal.objective),
        format!("Status: {}", status_label(&goal.status)),
        format!("Time: {}", format_elapsed(&goal)),
        format!("Tokens: {tokens}"),
        format!("Continuation turns: {}", goal.turns_executed),
    ];
    if goal.status == GoalStatus::MaxTurns {
        lines.push(format!(
            "Hint: Max continuation turns reached ({MAX_GOAL_TURNS}). Run `/goal continue` to reset and continue."
        ));
    }
    lines.join("\n")
}

pub fn build_goal_context_block(state: &crate::AppState) -> String {
    let Some(goal) = active_goal(state) else {
        return String::new();
    };
    let budget = goal
        .token_budget
        .map(|n| format!(" budget=\"{n}\""))
        .unwrap_or_default();
    format!(
        "<active-goal status=\"{}\" elapsed=\"{}\" elapsed_ms=\"{}\" tokens=\"{}\"{} turns=\"{}\">\n{}\n</active-goal>",
        status_value(&goal.status),
        format_elapsed(&goal),
        active_elapsed_ms(&goal),
        goal.tokens_used,
        budget,
        goal.turns_executed,
        goal.objective
    )
}

pub fn build_continuation_prompt(goal: &GoalState) -> String {
    let token_info = if let Some(budget) = goal.token_budget {
        let remaining = budget.saturating_sub(goal.tokens_used);
        format!(
            "Tokens used: {} / {} ({} remaining)",
            goal.tokens_used, budget, remaining
        )
    } else {
        format!("Tokens used: {}", goal.tokens_used)
    };

    format!(
        r#"<goal-steering type="continuation">
You have an active goal to work on. Continue making progress.

## Active Goal
{}

## Status
- Elapsed active time: {}
- {}
- Continuation turns executed: {}

## Instructions

Continue working towards the goal. Do NOT narrow the scope of the goal. Even if you cannot complete everything in one turn, maintain the full objective and make as much progress as possible.

When you believe the goal is fully achieved, use the `goal` tool to mark it complete. Before doing so, perform a strict Completion Audit:

### Completion Audit
1. Derive concrete requirements from the objective and any referenced files.
2. Preserve the original scope. Do not redefine success around what is already done.
3. For every explicit requirement, identify authoritative evidence such as test output, file content, or command result.
4. Treat tests, manifests, and verifiers as evidence only after confirming they actually cover the requirement.
5. Treat uncertain or indirect evidence as not achieved.
6. The audit must prove completion, not merely fail to find remaining work.

### Blocked Audit
If you encounter an obstacle you genuinely cannot overcome:
- Do NOT mark blocked on the first encounter.
- The same blocking condition must persist for at least 3 consecutive continuation turns before you may mark blocked.
- Difficult, slow, or partially incomplete work is NOT blocked.
- If blocked, use the `goal` tool with status "blocked" and a clear reason.

Resume working now.
</goal-steering>"#,
        goal.objective,
        format_elapsed(goal),
        token_info,
        goal.turns_executed
    )
}

pub fn build_budget_limit_prompt(goal: &GoalState) -> String {
    format!(
        r#"<goal-steering type="budget_limit">
## Token Budget Reached

Your token budget for this goal has been exhausted.

- Goal: {}
- Tokens used: {}{}
- Active time: {}

Stop all substantive work immediately. Do NOT start new file edits, tool calls, or explorations.

Instead, provide a brief summary:
1. What has been accomplished so far.
2. What remains to be done.
3. Any blockers or issues encountered.

Then use the `goal` tool to mark the goal as complete if truly done, or leave it in its current state for the user to decide.
</goal-steering>"#,
        goal.objective,
        goal.tokens_used,
        goal.token_budget
            .map(|budget| format!(" / {budget}"))
            .unwrap_or_default(),
        format_elapsed(goal)
    )
}

pub fn build_objective_updated_prompt(new_objective: &str, previous: Option<&str>) -> String {
    let previous = previous
        .map(|p| format!("\nPrevious objective: {p}\n"))
        .unwrap_or_default();
    format!(
        r#"<goal-steering type="objective_updated">
The user has updated the active goal.{previous}
New objective: {new_objective}

Acknowledge the updated objective and begin working towards it. All previous progress that is still relevant should be preserved, but the new objective takes priority.

Follow the same Completion Audit and Blocked Audit rules described in goal-steering messages. Use the `goal` tool to mark complete or blocked.
</goal-steering>"#
    )
}

pub fn completion_report(state: &crate::AppState) -> String {
    let Some(goal) = active_goal(state) else {
        return String::new();
    };
    let budget = match goal.token_budget {
        Some(budget) => format!("Token usage: {} / {}", goal.tokens_used, budget),
        None => format!("Token usage: {}", goal.tokens_used),
    };
    [
        "Goal achieved - usage report:".to_string(),
        format!("  {budget}"),
        format!("  Active time: {}", format_elapsed(&goal)),
        format!("  Continuation turns: {}", goal.turns_executed),
    ]
    .join("\n")
}

fn mark_max_turns(state: &crate::AppState) -> Option<GoalState> {
    mutate_active_goal(state, |goal| {
        if goal.status != GoalStatus::Active {
            return None;
        }
        goal.status = GoalStatus::MaxTurns;
        goal.updated_at = store::now_millis();
        Some(goal.clone())
    })
    .flatten()
}

fn mark_budget_notified(state: &crate::AppState) -> Option<GoalState> {
    mutate_active_goal(state, |goal| {
        goal.budget_limit_notified = true;
        goal.updated_at = store::now_millis();
        Some(goal.clone())
    })
    .flatten()
}

fn mutate_active_goal<R>(
    state: &crate::AppState,
    f: impl FnOnce(&mut GoalState) -> R,
) -> Option<R> {
    let mut store = state.sessions.lock().unwrap();
    let active = store.active.clone();
    let session = store.get_mut(&active)?;
    let goal = session.goal.as_mut()?;
    let result = f(goal);
    session.updated_at = store::now_millis();
    Some(result)
}

fn active_elapsed_ms(goal: &GoalState) -> u64 {
    let ongoing = if goal.status == GoalStatus::Active && goal.paused_at.is_none() {
        store::now_millis().saturating_sub(goal.start_time)
    } else {
        0
    };
    goal.accumulated_active_ms.saturating_add(ongoing)
}

fn format_elapsed(goal: &GoalState) -> String {
    let seconds = active_elapsed_ms(goal) / 1000;
    let minutes = seconds / 60;
    if minutes == 0 {
        format!("{seconds}s")
    } else {
        format!("{minutes}m {}s", seconds % 60)
    }
}

fn status_label(status: &GoalStatus) -> &'static str {
    match status {
        GoalStatus::Active => "Active",
        GoalStatus::Paused => "Paused",
        GoalStatus::Blocked => "Blocked",
        GoalStatus::BudgetLimited => "Budget Limited",
        GoalStatus::UsageLimited => "Usage Limited",
        GoalStatus::MaxTurns => "Max Turns Reached",
        GoalStatus::Complete => "Complete",
    }
}

fn status_value(status: &GoalStatus) -> &'static str {
    match status {
        GoalStatus::Active => "active",
        GoalStatus::Paused => "paused",
        GoalStatus::Blocked => "blocked",
        GoalStatus::BudgetLimited => "budget_limited",
        GoalStatus::UsageLimited => "usage_limited",
        GoalStatus::MaxTurns => "max_turns",
        GoalStatus::Complete => "complete",
    }
}

fn parse_objective_and_budget(input: &str) -> (String, Option<usize>) {
    let budget = parse_token_budget(input);
    let objective = strip_token_budget(input).trim().to_string();
    (objective, budget)
}

fn parse_token_budget(text: &str) -> Option<usize> {
    let patterns = [
        r"(?i)^\s*\+(\d+(?:\.\d+)?)\s*([kmb])\b",
        r"(?i)\s\+(\d+(?:\.\d+)?)\s*([kmb])\s*[.!?]?\s*$",
        r"(?i)\b(?:use|spend)\s+(\d+(?:\.\d+)?)\s*([kmb])\s*tokens?\b",
    ];
    for pattern in patterns {
        let re = Regex::new(pattern).ok()?;
        if let Some(caps) = re.captures(text) {
            let value = caps.get(1)?.as_str().parse::<f64>().ok()?;
            let multiplier = match caps.get(2)?.as_str().to_ascii_lowercase().as_str() {
                "k" => 1_000f64,
                "m" => 1_000_000f64,
                "b" => 1_000_000_000f64,
                _ => return None,
            };
            return Some((value * multiplier).round() as usize);
        }
    }
    None
}

fn strip_token_budget(text: &str) -> String {
    let mut out = text.to_string();
    for pattern in [
        r"(?i)^\s*\+\d+(?:\.\d+)?\s*[kmb]\b\s*",
        r"(?i)\s\+\d+(?:\.\d+)?\s*[kmb]\s*[.!?]?\s*$",
        r"(?i)\b(?:use|spend)\s+\d+(?:\.\d+)?\s*[kmb]\s*tokens?\b",
    ] {
        if let Ok(re) = Regex::new(pattern) {
            out = re.replace(&out, " ").to_string();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_token_budget_shorthand_and_verbose() {
        assert_eq!(parse_token_budget("+500k do it"), Some(500_000));
        assert_eq!(parse_token_budget("do it +2.5M"), Some(2_500_000));
        assert_eq!(
            parse_token_budget("please use 3.5m tokens"),
            Some(3_500_000)
        );
        assert_eq!(parse_token_budget("500k"), None);
    }

    #[test]
    fn strips_budget_from_objective() {
        let (objective, budget) = parse_objective_and_budget("+100k ship the feature");
        assert_eq!(objective, "ship the feature");
        assert_eq!(budget, Some(100_000));
    }

    #[test]
    fn elapsed_includes_accumulated_time() {
        let goal = GoalState {
            objective: "x".to_string(),
            status: GoalStatus::Paused,
            token_budget: None,
            tokens_used: 0,
            start_time: 0,
            paused_at: Some(0),
            accumulated_active_ms: 65_000,
            blocked_attempts: 0,
            last_block_reason: None,
            created_at: 0,
            updated_at: 0,
            turns_executed: 0,
            budget_limit_notified: false,
        };
        assert_eq!(format_elapsed(&goal), "1m 5s");
    }
}
