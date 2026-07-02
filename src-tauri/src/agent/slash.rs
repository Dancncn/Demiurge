//! Slash 命令分发。
//!
//! 把 `send` 里的 `/dream` `/compact` `/goal` `/skills` `/skill` `/effort` `/recall`
//! `/workflows` `/workflow resume` `/ultracode` 分支集中到这里，`lib.rs::send` 仅做
//! 参数解析、turn 生命周期与默认回合路径。
//!
//! `dispatch` 返回 `Some((结果, 是否驱动 goal))` 表示已识别并处理；返回 `None` 表示
//! 不是 slash 命令，调用方走默认路径（高风险检测 + `run_turn`）。
use tauri::AppHandle;

use crate::agent::session_engine::TurnEventEmitter;
use crate::agent::{self, TurnOptions};
use crate::llm;
use crate::pack;
use crate::store::{self, ReasoningEffort, Settings};
use crate::AppState;

pub async fn dispatch(
    app: &AppHandle,
    st: &AppState,
    text: String,
    events: &TurnEventEmitter<'_>,
) -> Option<(Result<(), String>, bool)> {
    let trimmed = text.trim();
    if trimmed == "/dream" || trimmed.starts_with("/dream ") {
        Some((agent::dream::run_manual_dream(app, st, text).await, true))
    } else if trimmed == "/compact" || trimmed.starts_with("/compact ") {
        Some((
            agent::collapse::run_manual_compact(app, st, text).await,
            true,
        ))
    } else if trimmed == "/goal" || trimmed.starts_with("/goal ") {
        match agent::goal::handle_slash(st, trimmed) {
            Ok(agent::goal::GoalSlashOutcome::Respond(body)) => {
                events.assistant_done(body);
                Some((Ok(()), false))
            }
            Ok(agent::goal::GoalSlashOutcome::Query {
                stored_user_text,
                system_overlay,
                ..
            }) => Some((
                agent::run_turn_with_options(
                    app,
                    st,
                    text,
                    TurnOptions {
                        system_overlay: Some(system_overlay),
                        stored_user_text: Some(stored_user_text),
                        workflow_run_id: None,
                        agent_names: Vec::new(),
                        token_budget: None,
                    },
                )
                .await,
                true,
            )),
            Err(e) => Some((Err(e), false)),
        }
    } else if trimmed == "/skills"
        || trimmed.starts_with("/skills ")
        || trimmed == "/skill"
        || trimmed.starts_with("/skill ")
    {
        match agent::skills::slash_response(st, trimmed) {
            Ok(body) => {
                events.assistant_done(body);
                Some((Ok(()), false))
            }
            Err(e) => Some((Err(e), false)),
        }
    } else if trimmed == "/effort" || trimmed.starts_with("/effort ") {
        match handle_effort_slash(app, st, trimmed) {
            Ok(body) => {
                events.assistant_done(body);
                Some((Ok(()), false))
            }
            Err(e) => Some((Err(e), false)),
        }
    } else if trimmed == "/recall" || trimmed.starts_with("/recall ") {
        let query = trimmed.trim_start_matches("/recall").trim();
        match handle_recall_slash(st, query) {
            Ok(body) => {
                events.assistant_done(body);
                Some((Ok(()), false))
            }
            Err(e) => Some((Err(e), false)),
        }
    } else if trimmed == "/workflows" {
        let runs = agent::workflow_runtime::panel_state(st).runs;
        let body = if runs.is_empty() {
            "暂无 workflow run。使用 /ultracode <任务> 或 Workflows 面板会自动创建 run。"
                .to_string()
        } else {
            let mut out = String::from("Workflow runs:\n");
            for run in runs.iter().take(20) {
                let status = match run.status {
                    agent::workflow_runtime::WorkflowStatus::Running => "running",
                    agent::workflow_runtime::WorkflowStatus::StaleRunning => "stale_running",
                    agent::workflow_runtime::WorkflowStatus::Done => "done",
                    agent::workflow_runtime::WorkflowStatus::Failed => "failed",
                    agent::workflow_runtime::WorkflowStatus::Killed => "killed",
                    agent::workflow_runtime::WorkflowStatus::Journaled => "journaled",
                };
                let budget = run
                    .budget
                    .total
                    .map(|total| format!(" budget={}/{}", run.budget.used_total(), total))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "- `{}` status={} steps={}/{} agents={}{} updated_at={} journal={}\n",
                    run.run_id,
                    status,
                    run.steps_done,
                    run.steps_total,
                    run.agents.len(),
                    budget,
                    run.updated_at,
                    run.journal_path
                ));
            }
            out
        };
        events.assistant_done(body);
        Some((Ok(()), true))
    } else if trimmed.starts_with("/workflow resume ") {
        let run_id = trimmed
            .trim_start_matches("/workflow resume ")
            .trim()
            .to_string();
        let overlay = match agent::workflow_runtime::resume_overlay(st, &run_id) {
            Ok(o) => o,
            Err(e) => return Some((Err(e), false)),
        };
        Some((
            agent::run_turn_with_options(
                app,
                st,
                text,
                TurnOptions {
                    system_overlay: Some(overlay),
                    stored_user_text: None,
                    workflow_run_id: Some(run_id),
                    agent_names: Vec::new(),
                    token_budget: None,
                },
            )
            .await,
            true,
        ))
    } else if trimmed == "/ultracode" || trimmed.starts_with("/ultracode ") {
        let task = trimmed
            .strip_prefix("/ultracode")
            .unwrap_or("")
            .trim()
            .to_string();
        let run_id = agent::workflow_journal::new_run_id();
        let overlay = agent::ultracode::overlay(&task);
        Some((
            agent::run_turn_with_options(
                app,
                st,
                text,
                TurnOptions {
                    system_overlay: Some(overlay),
                    stored_user_text: None,
                    workflow_run_id: Some(run_id),
                    agent_names: Vec::new(),
                    token_budget: None,
                },
            )
            .await,
            true,
        ))
    } else {
        None
    }
}

fn effort_usage() -> String {
    let levels = ReasoningEffort::LEVELS
        .iter()
        .map(|level| level.as_str())
        .collect::<Vec<_>>()
        .join("|");
    format!(
        "Usage: /effort [{levels}|auto]\n\n\
         Current levels:\n\
         - low: {}\n\
         - medium: {}\n\
         - high: {}\n\
         - xhigh: {}\n\
         - max: {}\n\
         - auto: {}",
        ReasoningEffort::Low.description(),
        ReasoningEffort::Medium.description(),
        ReasoningEffort::High.description(),
        ReasoningEffort::Xhigh.description(),
        ReasoningEffort::Max.description(),
        ReasoningEffort::Auto.description()
    )
}

fn effort_status(settings: &Settings) -> String {
    let profile = llm::ProviderProfile::for_kind(settings.provider);
    if !profile.supports_reasoning_effort_for_model(&settings.model) {
        return format!(
            "Configured effort: `{}`.\nProvider/model `{}` does not support Demiurge effort parameters yet, so no request field will be sent.",
            settings.reasoning_effort.as_str(),
            settings.model
        );
    }
    let applied = profile
        .effective_reasoning_effort(settings)
        .map(|effort| format!("`{}`", effort.as_str()))
        .unwrap_or_else(|| "provider default".to_string());
    format!(
        "Configured effort: `{}`.\nApplied on next supported request: {applied}.",
        settings.reasoning_effort.as_str()
    )
}

fn handle_recall_slash(state: &AppState, query: &str) -> Result<String, String> {
    if query.is_empty() {
        return Ok(
            "用法：/recall <查询> — 用当前角色包 Lorebook 做召回，返回 chunk 列表与分数。"
                .to_string(),
        );
    }
    let settings = state.settings.lock().unwrap().clone();
    let pack_id = settings.current_pack.clone();
    if pack_id.is_empty() {
        return Err("未选择角色包".to_string());
    }
    let packs_dir = state.packs_dir.lock().unwrap().clone();
    let data_dir = state.data_dir.lock().unwrap().clone();
    let embed = crate::embed::provider_from_settings(&state.http, &settings);
    let detail = pack::lorebook_recall_detail(
        &packs_dir,
        &data_dir,
        &pack_id,
        query,
        10,
        embed.as_deref(),
        settings.hybrid_weight,
    )?;
    Ok(format_recall_detail(&detail))
}

fn format_recall_detail(detail: &pack::LoreRecallDetail) -> String {
    let mut out = format!(
        "Lorebook recall: `{}` ({} chunks indexed, {} hits)\n",
        detail.query,
        detail.total_chunks,
        detail.hits.len()
    );
    if detail.hits.is_empty() {
        out.push_str("无命中。");
        return out;
    }
    for (i, hit) in detail.hits.iter().enumerate() {
        let heading = hit
            .heading
            .as_deref()
            .filter(|h| !h.is_empty())
            .unwrap_or(&hit.title);
        out.push_str(&format!(
            "\n{}. score={:.1} — {}#{} / {}\n",
            i + 1,
            hit.score,
            hit.source,
            hit.chunk_index,
            heading
        ));
        if !hit.matched_terms.is_empty() {
            out.push_str(&format!("   matched: {}\n", hit.matched_terms.join(", ")));
        }
        let preview: String = hit.text.chars().take(200).collect();
        out.push_str(&format!("   {preview}…\n"));
    }
    out
}

fn handle_effort_slash(app: &AppHandle, state: &AppState, text: &str) -> Result<String, String> {
    let arg = text.trim().strip_prefix("/effort").unwrap_or("").trim();
    if arg.is_empty() {
        let settings = state.settings.lock().unwrap().clone();
        return Ok(format!(
            "{}\n\n{}",
            effort_status(&settings),
            effort_usage()
        ));
    }
    let Some(effort) = ReasoningEffort::parse(arg) else {
        return Ok(effort_usage());
    };
    let next = {
        let mut settings = state.settings.lock().unwrap();
        settings.reasoning_effort = effort;
        settings.clone()
    };
    let dir = state.data_dir.lock().unwrap().clone();
    store::save_settings(&dir, &next)?;
    crate::emit_settings_updated(app, &next);
    Ok(format!(
        "Set effort to `{}`: {}\n\n{}",
        effort.as_str(),
        effort.description(),
        effort_status(&next)
    ))
}
