//! agent IPC Adapter.

use crate::*;
use std::sync::atomic::Ordering;
use tauri::AppHandle;

pub(crate) async fn send(app: AppHandle, state: &AppState, text: String) -> Result<(), String> {
    let st = state;
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
    let (res, should_drive_goal) =
        match agent::slash::dispatch(&app, st, &turn.session_id, text.clone(), &events).await {
            Some(outcome) => outcome,
            None => {
                if let Some(risk) = companion::detect_high_risk_expression(trimmed) {
                    persist_direct_reply(
                        st,
                        &turn.session_id,
                        text.clone(),
                        risk.support_message.clone(),
                    );
                    events.assistant_done(risk.support_message);
                    (Ok(()), false)
                } else {
                    (
                        agent::run_turn(&app, st, &turn.session_id, text).await,
                        true,
                    )
                }
            }
        };
    let res = if res.is_ok() && should_drive_goal && !st.cancel.load(Ordering::Relaxed) {
        agent::goal::drive_after_turn(&app, st, &turn.session_id).await
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

pub(crate) async fn send_with_agents(
    app: AppHandle,
    state: &AppState,
    text: String,
    agent_names: Vec<String>,
) -> Result<(), String> {
    let st = state;
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
        persist_direct_reply(
            st,
            &turn.session_id,
            text.clone(),
            risk.support_message.clone(),
        );
        let events = agent::session_engine::TurnEventEmitter::new(&app, st);
        events.assistant_done(risk.support_message);
        Ok(())
    } else {
        agent::run_turn_with_options(
            &app,
            st,
            &turn.session_id,
            text,
            agent::TurnOptions {
                agent_names,
                ..agent::TurnOptions::default()
            },
        )
        .await
    };
    let res = if res.is_ok() && should_drive_goal && !st.cancel.load(Ordering::Relaxed) {
        agent::goal::drive_after_turn(&app, st, &turn.session_id).await
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

pub(crate) fn interrupt(app: AppHandle, state: &AppState) {
    agent::session_engine::request_interrupt(&app, state);
    // 立即唤醒所有正在等待的确认（按「中断」处理），否则确认弹窗的 await 会把整轮卡住最长 5 分钟
    crate::permission::deny_pending_confirmations(state);
}

pub(crate) fn session_engine_state(
    state: &AppState,
) -> agent::session_engine::SessionEnginePanelState {
    agent::session_engine::panel_state(state)
}

fn persist_direct_reply(
    state: &AppState,
    session_id: &str,
    user_text: String,
    assistant_text: String,
) {
    let turn_store = agent::session_engine::SessionTurnStore::new(state, session_id.to_string());
    turn_store.append_user_message(user_text);
    turn_store.append_message(crate::agent::conversation::Message::assistant_text(
        assistant_text,
    ));
}
