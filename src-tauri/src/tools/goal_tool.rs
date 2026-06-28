use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct GoalArgs {
    action: Option<String>,
    status: Option<String>,
    reason: Option<String>,
}

pub fn run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let args: GoalArgs = serde_json::from_value(args).unwrap_or(GoalArgs {
        action: None,
        status: None,
        reason: None,
    });
    let action = args
        .action
        .as_deref()
        .or(if args.status.is_some() {
            Some("update")
        } else {
            Some("get")
        })
        .unwrap_or("get");

    if action == "get" {
        return Ok(snapshot(state)
            .unwrap_or_else(|| {
                json!({
                    "success": true,
                    "message": "No active goal. The user can set one with `/goal <objective>`."
                })
            })
            .to_string());
    }

    if action != "update" {
        return Err("action must be `get` or `update`.".to_string());
    }

    let status = args
        .status
        .as_deref()
        .ok_or_else(|| "status is required for update.".to_string())?;
    match status {
        "complete" => {
            if crate::agent::goal::active_goal(state).is_none() {
                return Err("No active goal to update.".to_string());
            }
            let report = crate::agent::goal::completion_report(state);
            crate::agent::goal::complete_goal(state);
            state.persist_sessions();
            Ok(json!({
                "success": true,
                "goal": snapshot(state),
                "report": report,
                "reason": args.reason.unwrap_or_default(),
            })
            .to_string())
        }
        "blocked" => {
            let reason = args
                .reason
                .unwrap_or_else(|| "unspecified blocker".to_string());
            let Some((next_status, attempts)) =
                crate::agent::goal::record_blocked_attempt(state, &reason)
            else {
                return Err("Goal is not in a state that accepts blocked attempts.".to_string());
            };
            state.persist_sessions();
            let message = if next_status == crate::agent::goal::GoalStatus::Blocked {
                format!("Goal marked as blocked after {attempts} consecutive attempts. Reason: {reason}")
            } else {
                format!("Blocked attempt {attempts} recorded. The goal remains active; the same condition must persist for 3 consecutive turns before it is marked blocked.")
            };
            Ok(json!({
                "success": true,
                "goal": snapshot(state),
                "message": message,
            })
            .to_string())
        }
        _ => Err("status must be `complete` or `blocked`.".to_string()),
    }
}

fn snapshot(state: &crate::AppState) -> Option<Value> {
    let goal = crate::agent::goal::active_goal(state)?;
    Some(json!({
        "objective": goal.objective,
        "status": goal.status,
        "tokens_used": goal.tokens_used,
        "token_budget": goal.token_budget,
        "turns_executed": goal.turns_executed,
        "blocked_attempts": goal.blocked_attempts,
        "last_block_reason": goal.last_block_reason,
    }))
}
