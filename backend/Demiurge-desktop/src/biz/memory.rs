//! memory IPC Adapter.

use crate::agent::conversation::Message;
use crate::*;
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

pub(crate) fn memory_panel_state(state: &AppState) -> agent::memory::MemoryPanelState {
    let (data, sandbox, packs, pack_id, session_id) = memory_context(state);
    agent::memory::panel_state(&data, &sandbox, &packs, &pack_id, &session_id)
}

pub(crate) fn memory_add_entry(
    state: &AppState,
    scope: String,
    kind: String,
    text: String,
) -> Result<agent::memory::MemoryPanelState, String> {
    let (data, sandbox, packs, pack_id, session_id) = memory_context(state);
    agent::memory::add_entry(
        &data,
        &sandbox,
        &packs,
        &pack_id,
        &session_id,
        &scope,
        &kind,
        &text,
    )
}

pub(crate) fn memory_update_entry(
    state: &AppState,
    id: String,
    kind: String,
    text: String,
) -> Result<agent::memory::MemoryPanelState, String> {
    let (data, sandbox, packs, pack_id, session_id) = memory_context(state);
    agent::memory::update_entry(
        &data,
        &sandbox,
        &packs,
        &pack_id,
        &session_id,
        &id,
        &kind,
        &text,
    )
}

pub(crate) fn memory_delete_entry(
    state: &AppState,
    id: String,
) -> Result<agent::memory::MemoryPanelState, String> {
    let (data, sandbox, packs, pack_id, session_id) = memory_context(state);
    agent::memory::delete_entry(&data, &sandbox, &packs, &pack_id, &session_id, &id)
}

pub(crate) fn memory_dedupe_apply(
    state: &AppState,
) -> Result<agent::memory::MemoryPanelState, String> {
    let (data, sandbox, packs, pack_id, session_id) = memory_context(state);
    agent::memory::apply_dedupe(&data, &sandbox, &packs, &pack_id, &session_id)
}

pub(crate) fn memory_migrate_namespace(
    state: &AppState,
    from_ns: String,
    to_ns: String,
) -> Result<agent::memory::MemoryPanelState, String> {
    let (data, sandbox, packs, pack_id, session_id) = memory_context(state);
    agent::memory::migrate_namespace(&data, &sandbox, &from_ns, &to_ns)?;
    Ok(agent::memory::panel_state(
        &data,
        &sandbox,
        &packs,
        &pack_id,
        &session_id,
    ))
}

pub(crate) fn context_panel_state(state: &AppState) -> ContextPanelState {
    let settings = state.settings.lock().unwrap().clone();
    let (messages, summary) = {
        let store = state.sessions.lock().unwrap();
        store
            .get(&store.active)
            .map(|session| (session.messages.clone(), session.summary.clone()))
            .unwrap_or_else(|| (Vec::new(), None))
    };

    let packs_dir = state.packs_dir.lock().unwrap().clone();
    let sandbox_dir = state.sandbox_dir.lock().unwrap().clone();
    let data_dir = state.data_dir.lock().unwrap().clone();
    let session_id = state.sessions.lock().unwrap().active.clone();
    let persona_text = pack::load_pack(&packs_dir, &settings.current_pack)
        .map(|p| p.persona_text)
        .unwrap_or_default();
    let prompt_build =
        agent::prompt::build_with_report(state, &settings, &persona_text, summary.as_deref());
    let profile = llm::ProviderProfile::for_kind(settings.provider);
    let tools_schema = if profile.supports_tools {
        tools::main_schemas_json_for(profile.tool_schema_dialect)
    } else {
        profile.empty_tool_schema()
    };
    let budget =
        agent::budget::history_budget(&settings, &prompt_build.text, &tools_schema, &messages);
    let summary_text = summary.as_deref().unwrap_or_default();
    let summary_chars = summary_text.chars().count();
    let summary_tokens = agent::budget::estimate_text_tokens(summary_text);
    let prompt_section_tokens = prompt_build
        .sections
        .iter()
        .map(|section| section.tokens)
        .sum::<usize>();
    let input_budget_used_tokens = budget
        .system_tokens
        .saturating_add(budget.tools_tokens)
        .saturating_add(budget.history_tokens);
    let input_budget_remaining_tokens = budget
        .max_input_tokens
        .saturating_sub(input_budget_used_tokens);
    let projected_total_tokens =
        input_budget_used_tokens.saturating_add(budget.reserved_output_tokens);
    let history_remaining_tokens = budget
        .history_budget_tokens
        .saturating_sub(budget.history_tokens);
    let history_over_budget_tokens = budget
        .history_tokens
        .saturating_sub(budget.history_budget_tokens);

    ContextPanelState {
        message_count: messages.len(),
        user_messages: messages.iter().filter(|m| m.role == "user").count(),
        assistant_messages: messages.iter().filter(|m| m.role == "assistant").count(),
        tool_messages: messages.iter().filter(|m| m.role == "tool").count(),
        summary_chars,
        summary_tokens,
        system_prompt_chars: prompt_build.prompt_chars,
        system_prompt_tokens: budget.system_tokens,
        estimated_history_tokens: budget.history_tokens,
        tools_tokens: budget.tools_tokens,
        history_budget_tokens: budget.history_budget_tokens,
        history_remaining_tokens,
        history_over_budget_tokens,
        max_input_tokens: budget.max_input_tokens,
        reserved_output_tokens: budget.reserved_output_tokens,
        input_budget_used_tokens,
        input_budget_remaining_tokens,
        projected_total_tokens,
        prompt_section_tokens,
        budget_items: context_budget_items(&budget),
        history_buckets: context_history_buckets(&messages),
        memory_sources: context_memory_sources(
            &data_dir,
            &sandbox_dir,
            &packs_dir,
            &settings.current_pack,
            &session_id,
        ),
        prompt_sections: prompt_build.sections,
    }
}

pub(crate) fn skill_panel_state(
    state: &AppState,
    query: Option<String>,
) -> agent::skills::SkillPanelState {
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let data_dir = state.data_dir.lock().unwrap().clone();
    let packs_dir = state.packs_dir.lock().unwrap().clone();
    let pack_id = state.settings.lock().unwrap().current_pack.clone();
    let trimmed = query.as_deref().map(str::trim).filter(|s| !s.is_empty());
    agent::skills::panel_state(&sandbox, &data_dir, &packs_dir, &pack_id, trimmed)
}

pub(crate) fn open_skills_dir(state: &AppState) -> Result<(), String> {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let dir = data_dir.join("skills");
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    tools::execute_open(&dir.to_string_lossy()).map(|_| ())
}

#[derive(Serialize)]
pub(crate) struct ContextPanelState {
    message_count: usize,
    user_messages: usize,
    assistant_messages: usize,
    tool_messages: usize,
    summary_chars: usize,
    summary_tokens: usize,
    system_prompt_chars: usize,
    system_prompt_tokens: usize,
    estimated_history_tokens: usize,
    tools_tokens: usize,
    history_budget_tokens: usize,
    history_remaining_tokens: usize,
    history_over_budget_tokens: usize,
    max_input_tokens: usize,
    reserved_output_tokens: usize,
    input_budget_used_tokens: usize,
    input_budget_remaining_tokens: usize,
    projected_total_tokens: usize,
    prompt_section_tokens: usize,
    budget_items: Vec<ContextBudgetItem>,
    history_buckets: Vec<ContextHistoryBucket>,
    memory_sources: Vec<ContextMemorySource>,
    prompt_sections: Vec<agent::prompt::PromptSectionReport>,
}

#[derive(Serialize)]
struct ContextBudgetItem {
    id: String,
    label: String,
    tokens: usize,
    limit_tokens: Option<usize>,
    detail: String,
}

#[derive(Serialize)]
struct ContextHistoryBucket {
    role: String,
    label: String,
    messages: usize,
    tokens: usize,
}

#[derive(Serialize)]
struct ContextMemorySource {
    id: String,
    label: String,
    path: String,
    exists: bool,
    chars: usize,
    tokens: usize,
    entries: usize,
}

pub(crate) fn memory_context(state: &AppState) -> (PathBuf, PathBuf, PathBuf, String, String) {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let sandbox_dir = state.sandbox_dir.lock().unwrap().clone();
    let packs_dir = state.packs_dir.lock().unwrap().clone();
    let pack_id = state.settings.lock().unwrap().current_pack.clone();
    let session_id = state.sessions.lock().unwrap().active.clone();
    (data_dir, sandbox_dir, packs_dir, pack_id, session_id)
}

fn context_budget_items(budget: &agent::budget::ContextBudget) -> Vec<ContextBudgetItem> {
    vec![
        ContextBudgetItem {
            id: "system".to_string(),
            label: "System prompt".to_string(),
            tokens: budget.system_tokens,
            limit_tokens: Some(budget.max_input_tokens),
            detail:
                "Packed persona, instructions, summary, memories, environment and safety sections."
                    .to_string(),
        },
        ContextBudgetItem {
            id: "tools".to_string(),
            label: "Tool schemas".to_string(),
            tokens: budget.tools_tokens,
            limit_tokens: Some(budget.max_input_tokens),
            detail: "Serialized tool definitions supplied to the provider.".to_string(),
        },
        ContextBudgetItem {
            id: "history".to_string(),
            label: "History".to_string(),
            tokens: budget.history_tokens,
            limit_tokens: Some(budget.history_budget_tokens),
            detail: "Current session messages before token-aware trimming.".to_string(),
        },
        ContextBudgetItem {
            id: "output_reserve".to_string(),
            label: "Output reserve".to_string(),
            tokens: budget.reserved_output_tokens,
            limit_tokens: Some(budget.max_input_tokens),
            detail: "Tokens reserved for the model response.".to_string(),
        },
    ]
}

fn context_history_buckets(messages: &[Message]) -> Vec<ContextHistoryBucket> {
    let mut buckets = [
        ("system", "System"),
        ("user", "User"),
        ("assistant", "Assistant"),
        ("tool", "Tool"),
        ("other", "Other"),
    ]
    .into_iter()
    .map(|(role, label)| ContextHistoryBucket {
        role: role.to_string(),
        label: label.to_string(),
        messages: 0,
        tokens: 0,
    })
    .collect::<Vec<_>>();

    for message in messages {
        let idx = match message.role.as_str() {
            "system" => 0,
            "user" => 1,
            "assistant" => 2,
            "tool" => 3,
            _ => 4,
        };
        buckets[idx].messages += 1;
        buckets[idx].tokens = buckets[idx]
            .tokens
            .saturating_add(agent::budget::estimate_message_tokens(message));
    }
    buckets
}

fn context_memory_sources(
    data_dir: &Path,
    sandbox_dir: &Path,
    packs_dir: &Path,
    pack_id: &str,
    session_id: &str,
) -> Vec<ContextMemorySource> {
    let mut sources =
        agent::memory::scoped_memory_paths(data_dir, sandbox_dir, packs_dir, pack_id, session_id)
            .into_iter()
            .map(|(id, label, path)| context_memory_source(&id, &format!("{label} memory"), &path))
            .collect::<Vec<_>>();
    sources.push(context_memory_source(
        "project_legacy",
        "Project legacy memory",
        &sandbox_dir.join("memory.md"),
    ));
    sources
}

fn context_memory_source(id: &str, label: &str, path: &Path) -> ContextMemorySource {
    let raw = fs::read_to_string(path).unwrap_or_default();
    let exists = path.is_file();
    let chars = raw.chars().count();
    let tokens = agent::budget::estimate_text_tokens(&raw);
    let entries = raw
        .lines()
        .filter(|line| line.trim_start().starts_with("- ["))
        .count();
    ContextMemorySource {
        id: id.to_string(),
        label: label.to_string(),
        path: path.to_string_lossy().to_string(),
        exists,
        chars,
        tokens,
        entries,
    }
}

/// 技能面板状态。可选 `query` 用于按用户输入对技能做匹配/检索打分。
/// 打开全局技能目录(供「在文件夹中显示」按钮使用)。
#[cfg(test)]
mod context_panel_tests {
    use super::*;

    #[test]
    fn context_history_buckets_group_roles_and_tokens() {
        let messages = vec![
            Message::user("hello"),
            Message::assistant_text("world"),
            Message::tool_result("call_1", "read_file", "tool output"),
        ];
        let buckets = context_history_buckets(&messages);

        let user = buckets.iter().find(|bucket| bucket.role == "user").unwrap();
        assert_eq!(user.messages, 1);
        assert!(user.tokens > 0);

        let assistant = buckets
            .iter()
            .find(|bucket| bucket.role == "assistant")
            .unwrap();
        assert_eq!(assistant.messages, 1);
        assert!(assistant.tokens > 0);

        let tool = buckets.iter().find(|bucket| bucket.role == "tool").unwrap();
        assert_eq!(tool.messages, 1);
        assert!(tool.tokens > 0);
    }

    #[test]
    fn context_memory_sources_report_existing_memory_files() {
        let root = std::env::temp_dir().join(format!(
            "demiurge_context_panel_{}",
            crate::store::now_millis()
        ));
        let data = root.join("data");
        let packs = root.join("packs");
        let sandbox = root.join("sandbox");
        let pack = packs.join("default");
        fs::create_dir_all(data.join("memory")).unwrap();
        fs::create_dir_all(sandbox.join(".demiurge")).unwrap();
        fs::create_dir_all(sandbox.join(".demiurge").join("session-memory")).unwrap();
        fs::create_dir_all(&pack).unwrap();
        fs::write(
            data.join("memory").join("user.md"),
            "- [user] user preference\n",
        )
        .unwrap();
        fs::write(sandbox.join("memory.md"), "- [project] remember this\n").unwrap();
        fs::write(
            sandbox.join(".demiurge").join("memory.md"),
            "- [project] project fact\n",
        )
        .unwrap();
        fs::write(
            sandbox
                .join(".demiurge")
                .join("session-memory")
                .join("session_1.md"),
            "- [session] session fact\n",
        )
        .unwrap();
        fs::write(pack.join("memory.md"), "pack note").unwrap();

        let sources = context_memory_sources(&data, &sandbox, &packs, "default", "session_1");

        let user = sources.iter().find(|source| source.id == "user").unwrap();
        assert!(user.exists);
        assert_eq!(user.entries, 1);

        let project = sources
            .iter()
            .find(|source| source.id == "project")
            .unwrap();
        assert!(project.exists);
        assert_eq!(project.entries, 1);
        assert!(project.tokens > 0);

        let session = sources
            .iter()
            .find(|source| source.id == "session")
            .unwrap();
        assert!(session.exists);
        assert_eq!(session.entries, 1);

        let pack = sources.iter().find(|source| source.id == "pack").unwrap();
        assert!(pack.exists);
        assert_eq!(pack.entries, 0);

        let legacy = sources
            .iter()
            .find(|source| source.id == "project_legacy")
            .unwrap();
        assert!(legacy.exists);
        assert_eq!(legacy.entries, 1);

        let _ = fs::remove_dir_all(root);
    }
}
