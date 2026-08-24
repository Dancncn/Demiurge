//! goal IPC Adapter.

use crate::*;
use std::sync::atomic::Ordering;
use tauri::AppHandle;

pub(crate) fn goal_panel_state(state: &AppState) -> Option<agent::goal::GoalPanelState> {
    agent::goal::panel_state(state)
}

pub(crate) fn goal_pause(state: &AppState) -> Result<Option<agent::goal::GoalPanelState>, String> {
    let paused = agent::goal::pause_goal(state).is_some();
    if !paused {
        return Err("Current goal cannot be paused.".to_string());
    }
    state.persist_sessions();
    Ok(agent::goal::panel_state(state))
}

pub(crate) async fn goal_resume(
    app: AppHandle,
    state: &AppState,
) -> Result<Option<agent::goal::GoalPanelState>, String> {
    let st = state;
    let session_id = st.sessions.lock().unwrap().active.clone();
    let input = "[Goal resumed]";
    let turn = agent::session_engine::begin_turn(
        &app,
        st,
        agent::session_engine::TurnStart {
            entrypoint: agent::session_engine::TurnEntrypoint::Send,
            session_id,
            input: input.to_string(),
            workflow_run_id: None,
            agent_names: Vec::new(),
        },
    )?;
    let result = async {
        let Some(goal) = agent::goal::resume_goal_for_session(st, &turn.session_id) else {
            return Err("No paused goal to resume.".to_string());
        };
        st.persist_sessions();
        run_goal_control_turn(&app, st, &turn.session_id, input, goal).await
    }
    .await;
    finish_command_turn(&app, st, &turn, &result);
    result
}

pub(crate) async fn goal_continue(
    app: AppHandle,
    state: &AppState,
) -> Result<Option<agent::goal::GoalPanelState>, String> {
    let st = state;
    let session_id = st.sessions.lock().unwrap().active.clone();
    let input = "[Goal continued]";
    let turn = agent::session_engine::begin_turn(
        &app,
        st,
        agent::session_engine::TurnStart {
            entrypoint: agent::session_engine::TurnEntrypoint::Send,
            session_id,
            input: input.to_string(),
            workflow_run_id: None,
            agent_names: Vec::new(),
        },
    )?;
    let result = async {
        let Some(goal) = agent::goal::continue_from_max_turns_for_session(st, &turn.session_id)
        else {
            return Err("Current goal is not waiting for continue.".to_string());
        };
        st.persist_sessions();
        run_goal_control_turn(&app, st, &turn.session_id, input, goal).await
    }
    .await;
    finish_command_turn(&app, st, &turn, &result);
    result
}

pub(crate) fn goal_clear(state: &AppState) -> Option<agent::goal::GoalPanelState> {
    agent::goal::clear_goal(state);
    state.persist_sessions();
    agent::goal::panel_state(state)
}

fn finish_command_turn<T>(
    app: &AppHandle,
    state: &AppState,
    turn: &agent::session_engine::TurnHandle,
    result: &Result<T, String>,
) {
    let status = if state.cancel.load(Ordering::Relaxed) {
        agent::session_engine::TurnStatus::Interrupted
    } else if result.is_ok() {
        agent::session_engine::TurnStatus::Completed
    } else {
        agent::session_engine::TurnStatus::Failed
    };
    agent::session_engine::finish_turn(app, state, turn, status, result.as_ref().err().cloned());
}

async fn run_goal_control_turn(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    stored_user_text: &str,
    goal: agent::goal::GoalState,
) -> Result<Option<agent::goal::GoalPanelState>, String> {
    let hidden_text = stored_user_text.to_string();
    agent::run_turn_with_options(
        app,
        state,
        session_id,
        hidden_text.clone(),
        agent::TurnOptions {
            system_overlay: Some(agent::goal::build_continuation_prompt(&goal)),
            stored_user_text: Some(hidden_text),
            workflow_run_id: None,
            agent_names: Vec::new(),
            token_budget: None,
            user_images: Vec::new(),
            conversation_context: None,
            silent_assistant_marker: None,
        },
    )
    .await?;
    if !state.cancel.load(Ordering::Relaxed) {
        agent::goal::drive_after_turn(app, state, session_id).await?;
    }
    Ok(agent::goal::goal_for_session(state, session_id)
        .as_ref()
        .map(agent::goal::panel_state_from_goal))
}
