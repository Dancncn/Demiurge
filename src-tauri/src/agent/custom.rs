use std::collections::{BTreeMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const AGENTS_DIR: &str = ".demiurge/agents";
const AGENT_STATS_FILE: &str = ".demiurge/agent_stats.json";
const MAX_TEXT_CHARS: usize = 16_000;
const MAX_TEAM_DEPTH: usize = 8;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Template,
    Team,
}

fn default_kind() -> AgentKind {
    AgentKind::Template
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AgentBudget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_input_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reserved_output_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_steps: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_total_tokens: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentFile {
    pub name: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_kind")]
    pub kind: AgentKind,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub budget: Option<AgentBudget>,
    #[serde(default)]
    pub handoff_format: String,
    #[serde(default)]
    pub members: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AgentDefinitionInfo {
    pub name: String,
    pub description: String,
    pub kind: AgentKind,
    pub path: String,
    pub prompt: String,
    pub allowed_tools: Vec<String>,
    pub invalid_tools: Vec<String>,
    pub budget: Option<AgentBudget>,
    pub handoff_format: String,
    pub members: Vec<String>,
    pub runtime: AgentRuntimeStats,
}

#[derive(Clone, Debug, Serialize)]
pub struct AgentPanelState {
    pub definitions: Vec<AgentDefinitionInfo>,
    pub agents_dir: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AgentRuntimeStats {
    pub run_count: u64,
    pub total_tokens: u64,
    pub error_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct AgentStatsFile {
    #[serde(default)]
    agents: BTreeMap<String, AgentRuntimeStats>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AgentEditorFile {
    pub name: String,
    pub file_name: String,
    pub path: String,
    pub raw_json: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AgentValidationResult {
    pub ok: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub normalized_name: String,
    pub suggested_file_name: String,
}

#[derive(Clone, Debug)]
pub struct ResolvedAgents {
    pub definitions: Vec<AgentDefinitionInfo>,
    pub prompt_overlay: String,
    pub allowed_tools: Vec<String>,
    pub max_input_tokens: Option<usize>,
    pub reserved_output_tokens: Option<usize>,
    pub max_steps: Option<usize>,
    pub max_total_tokens: Option<usize>,
}

pub fn ensure_dir(state: &crate::AppState) -> Result<PathBuf, String> {
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let dir = sandbox.join(AGENTS_DIR);
    fs::create_dir_all(&dir).map_err(|e| format!("创建 Agent 目录失败：{e}"))?;
    Ok(dir)
}

fn stats_path(state: &crate::AppState) -> Result<PathBuf, String> {
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let path = sandbox.join(AGENT_STATS_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建 Agent 统计目录失败：{e}"))?;
    }
    Ok(path)
}

pub fn panel_state(state: &crate::AppState) -> AgentPanelState {
    let dir = ensure_dir(state).unwrap_or_else(|_| PathBuf::from(AGENTS_DIR));
    AgentPanelState {
        definitions: list_definitions(state),
        agents_dir: dir.to_string_lossy().to_string(),
    }
}

pub fn template_json() -> String {
    let template = serde_json::json!({
        "name": "researcher",
        "description": "Explore the codebase and return concise evidence.",
        "kind": "template",
        "prompt": "You are a focused read-only researcher. Inspect relevant files, cite concrete paths, and hand off findings with risks and verification notes.",
        "allowed_tools": ["read_file", "grep", "glob", "git_status", "web_fetch", "web_search"],
        "budget": {
            "max_input_tokens": 16000,
            "reserved_output_tokens": 2000,
            "max_steps": 6,
            "max_total_tokens": 12000
        },
        "handoff_format": "Return: findings, evidence, risks, and suggested next actions.",
        "members": []
    });
    serde_json::to_string_pretty(&template).unwrap_or_else(|_| "{}".to_string())
}

pub fn validate_raw(raw_json: &str) -> AgentValidationResult {
    validate_agent_json(raw_json)
}

pub fn read_editor_file(state: &crate::AppState, name: &str) -> Result<AgentEditorFile, String> {
    let dir = ensure_dir(state)?;
    let path = find_agent_path(&dir, name).ok_or_else(|| format!("Agent `{name}` not found."))?;
    let raw_json =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read agent JSON: {e}"))?;
    let def = definition_from_path(&path, &valid_tool_names())?;
    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("{}.json", sanitize_name(&def.name)));
    Ok(AgentEditorFile {
        name: def.name,
        file_name,
        path: path.to_string_lossy().to_string(),
        raw_json,
    })
}

pub fn save_editor_file(
    state: &crate::AppState,
    file_name: &str,
    raw_json: &str,
) -> Result<AgentPanelState, String> {
    let validation = validate_agent_json(raw_json);
    if !validation.ok {
        return Err(validation.errors.join("\n"));
    }
    let dir = ensure_dir(state)?;
    let safe_file_name = safe_agent_file_name(if file_name.trim().is_empty() {
        &validation.suggested_file_name
    } else {
        file_name
    })?;
    let path = dir.join(safe_file_name);
    let parsed: serde_json::Value =
        serde_json::from_str(raw_json).map_err(|e| format!("Invalid agent JSON: {e}"))?;
    let formatted =
        serde_json::to_string_pretty(&parsed).map_err(|e| format!("Failed to format JSON: {e}"))?;
    fs::write(&path, formatted).map_err(|e| format!("Failed to save agent JSON: {e}"))?;
    Ok(panel_state(state))
}

pub fn delete_editor_file(state: &crate::AppState, name: &str) -> Result<AgentPanelState, String> {
    let dir = ensure_dir(state)?;
    let path = find_agent_path(&dir, name).ok_or_else(|| format!("Agent `{name}` not found."))?;
    fs::remove_file(&path).map_err(|e| format!("Failed to delete agent JSON: {e}"))?;
    Ok(panel_state(state))
}

pub fn list_definitions(state: &crate::AppState) -> Vec<AgentDefinitionInfo> {
    let Ok(dir) = ensure_dir(state) else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let valid_tools = valid_tool_names();
    let stats = load_stats(state).unwrap_or_default();
    let mut out = entries
        .filter_map(Result::ok)
        .filter_map(|entry| definition_from_path(&entry.path(), &valid_tools).ok())
        .collect::<Vec<_>>();
    for def in &mut out {
        if let Some(runtime) = stats.agents.get(&def.name) {
            def.runtime = runtime.clone();
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn record_runtime_start(state: &crate::AppState, definitions: &[AgentDefinitionInfo]) {
    if definitions.is_empty() {
        return;
    }
    let _ = update_stats(state, |stats| {
        let now = crate::store::now_millis();
        for def in definitions {
            let entry = stats.agents.entry(def.name.clone()).or_default();
            entry.run_count = entry.run_count.saturating_add(1);
            entry.last_used_at = Some(now);
        }
    });
}

pub fn record_runtime_usage(
    state: &crate::AppState,
    definitions: &[AgentDefinitionInfo],
    tokens: usize,
) {
    if definitions.is_empty() || tokens == 0 {
        return;
    }
    let _ = update_stats(state, |stats| {
        let now = crate::store::now_millis();
        for def in definitions {
            let entry = stats.agents.entry(def.name.clone()).or_default();
            entry.total_tokens = entry.total_tokens.saturating_add(tokens as u64);
            entry.last_used_at = Some(now);
        }
    });
}

pub fn record_runtime_error(
    state: &crate::AppState,
    definitions: &[AgentDefinitionInfo],
    error: &str,
) {
    if definitions.is_empty() {
        return;
    }
    let _ = update_stats(state, |stats| {
        let now = crate::store::now_millis();
        let brief = cap_chars(error.to_string(), 800);
        for def in definitions {
            let entry = stats.agents.entry(def.name.clone()).or_default();
            entry.error_count = entry.error_count.saturating_add(1);
            entry.last_used_at = Some(now);
            entry.last_error = Some(brief.clone());
        }
    });
}

pub fn load_agent(state: &crate::AppState, name: &str) -> Result<AgentDefinitionInfo, String> {
    let dir = ensure_dir(state)?;
    let path = find_agent_path(&dir, name).ok_or_else(|| format!("未找到 Agent `{name}`。"))?;
    definition_from_path(&path, &valid_tool_names())
}

pub fn resolve_selected(
    state: &crate::AppState,
    names: &[String],
) -> Result<ResolvedAgents, String> {
    let mut queue = VecDeque::new();
    for name in names.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        queue.push_back((name.to_string(), 0usize));
    }

    let mut seen = HashSet::new();
    let mut definitions = Vec::new();
    while let Some((name, depth)) = queue.pop_front() {
        if depth > MAX_TEAM_DEPTH || !seen.insert(name.clone()) {
            continue;
        }
        let def = load_agent(state, &name)?;
        if def.kind == AgentKind::Team {
            for member in &def.members {
                queue.push_back((member.clone(), depth + 1));
            }
        }
        definitions.push(def);
    }

    let mut allowed = Vec::new();
    let mut max_input_tokens = None;
    let mut reserved_output_tokens = None;
    let mut max_steps = None;
    let mut max_total_tokens = None;
    for def in &definitions {
        for tool in &def.allowed_tools {
            if !allowed.contains(tool) {
                allowed.push(tool.clone());
            }
        }
        if let Some(budget) = &def.budget {
            min_assign(&mut max_input_tokens, budget.max_input_tokens);
            min_assign(&mut reserved_output_tokens, budget.reserved_output_tokens);
            min_assign(&mut max_steps, budget.max_steps);
            min_assign(&mut max_total_tokens, budget.max_total_tokens);
        }
    }

    Ok(ResolvedAgents {
        prompt_overlay: build_overlay(&definitions),
        definitions,
        allowed_tools: allowed,
        max_input_tokens,
        reserved_output_tokens,
        max_steps,
        max_total_tokens,
    })
}

fn min_assign(slot: &mut Option<usize>, value: Option<usize>) {
    if let Some(value) = value {
        *slot = Some(slot.map(|cur| cur.min(value)).unwrap_or(value));
    }
}

fn definition_from_path(
    path: &Path,
    valid_tools: &HashSet<String>,
) -> Result<AgentDefinitionInfo, String> {
    if path.extension().and_then(|s| s.to_str()) != Some("json") {
        return Err("not an agent json".to_string());
    }
    let raw = fs::read_to_string(path).map_err(|e| format!("读取 Agent 清单失败：{e}"))?;
    let parsed: AgentFile =
        serde_json::from_str(&raw).map_err(|e| format!("解析 Agent JSON 失败：{e}"))?;
    let name = parsed
        .name
        .filter(|s| !s.trim().is_empty())
        .or_else(|| path.file_stem().map(|s| s.to_string_lossy().to_string()))
        .ok_or_else(|| "Agent 缺少 name".to_string())?;
    let mut allowed_tools = Vec::new();
    let mut invalid_tools = Vec::new();
    for tool in parsed.allowed_tools {
        if valid_tools.contains(&tool) {
            if !allowed_tools.contains(&tool) {
                allowed_tools.push(tool);
            }
        } else if !invalid_tools.contains(&tool) {
            invalid_tools.push(tool);
        }
    }
    Ok(AgentDefinitionInfo {
        name,
        description: parsed.description,
        kind: parsed.kind,
        path: path.to_string_lossy().to_string(),
        prompt: cap_chars(parsed.prompt, MAX_TEXT_CHARS),
        allowed_tools,
        invalid_tools,
        budget: parsed.budget,
        handoff_format: cap_chars(parsed.handoff_format, MAX_TEXT_CHARS / 2),
        members: parsed.members,
        runtime: AgentRuntimeStats::default(),
    })
}

fn load_stats(state: &crate::AppState) -> Result<AgentStatsFile, String> {
    let path = stats_path(state)?;
    if !path.exists() {
        return Ok(AgentStatsFile::default());
    }
    let raw = fs::read_to_string(&path).map_err(|e| format!("读取 Agent 统计失败：{e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("解析 Agent 统计失败：{e}"))
}

fn save_stats(state: &crate::AppState, stats: &AgentStatsFile) -> Result<(), String> {
    let path = stats_path(state)?;
    let raw =
        serde_json::to_string_pretty(stats).map_err(|e| format!("序列化 Agent 统计失败：{e}"))?;
    fs::write(&path, raw).map_err(|e| format!("保存 Agent 统计失败：{e}"))
}

fn update_stats(
    state: &crate::AppState,
    mut update: impl FnMut(&mut AgentStatsFile),
) -> Result<(), String> {
    let mut stats = load_stats(state).unwrap_or_default();
    update(&mut stats);
    save_stats(state, &stats)
}

fn find_agent_path(dir: &Path, requested: &str) -> Option<PathBuf> {
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
        let Ok(agent) = serde_json::from_str::<AgentFile>(&raw) else {
            continue;
        };
        if agent
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

fn validate_agent_json(raw_json: &str) -> AgentValidationResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let parsed = match serde_json::from_str::<AgentFile>(raw_json) {
        Ok(parsed) => parsed,
        Err(err) => {
            return AgentValidationResult {
                ok: false,
                errors: vec![format!("Invalid JSON: {err}")],
                warnings,
                normalized_name: String::new(),
                suggested_file_name: "agent.json".to_string(),
            };
        }
    };

    let name = parsed.name.unwrap_or_default().trim().to_string();
    if name.is_empty() {
        errors.push("Agent name is required.".to_string());
    }
    if parsed.kind == AgentKind::Template && parsed.prompt.trim().is_empty() {
        warnings.push("Template agents usually need a prompt.".to_string());
    }
    if parsed.kind == AgentKind::Team && parsed.members.is_empty() {
        warnings.push("Team agents should list at least one member.".to_string());
    }
    if parsed.kind == AgentKind::Template && !parsed.members.is_empty() {
        warnings
            .push("Template agents ignore members; use kind \"team\" for composition.".to_string());
    }
    let valid_tools = valid_tool_names();
    let invalid_tools = parsed
        .allowed_tools
        .iter()
        .filter(|tool| !valid_tools.contains(*tool))
        .cloned()
        .collect::<Vec<_>>();
    if !invalid_tools.is_empty() {
        warnings.push(format!(
            "Unknown tools will be ignored: {}",
            invalid_tools.join(", ")
        ));
    }
    if let Some(budget) = parsed.budget {
        if budget
            .reserved_output_tokens
            .zip(budget.max_input_tokens)
            .map(|(reserved, max_input)| reserved >= max_input)
            .unwrap_or(false)
        {
            errors.push("reserved_output_tokens must be lower than max_input_tokens.".to_string());
        }
        if budget.max_steps == Some(0) {
            errors.push("max_steps must be greater than 0.".to_string());
        }
    }

    let suggested_file_name =
        safe_agent_file_name(&name).unwrap_or_else(|_| "agent.json".to_string());
    AgentValidationResult {
        ok: errors.is_empty(),
        errors,
        warnings,
        normalized_name: name,
        suggested_file_name,
    }
}

fn safe_agent_file_name(input: &str) -> Result<String, String> {
    let trimmed = input.trim().trim_end_matches(".json");
    let safe = sanitize_name(trimmed);
    if safe.is_empty() {
        return Err("Agent file name cannot be empty.".to_string());
    }
    Ok(format!("{safe}.json"))
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

fn valid_tool_names() -> HashSet<String> {
    crate::tools::registry()
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect()
}

fn build_overlay(definitions: &[AgentDefinitionInfo]) -> String {
    if definitions.is_empty() {
        return String::new();
    }
    let mut out = String::from("# 选中的 Agent 模板\n");
    out.push_str("本轮应按下列用户选择的 Agent 模板/团队工作。多个模板被选择时，请综合它们的职责，按 handoff 格式交付。\n");
    for def in definitions {
        out.push_str(&format!(
            "\n## {} ({:?})\n- description: {}\n- allowed_tools: {}\n",
            def.name,
            def.kind,
            def.description,
            if def.allowed_tools.is_empty() {
                "默认主工具集".to_string()
            } else {
                def.allowed_tools.join(", ")
            }
        ));
        if !def.members.is_empty() {
            out.push_str(&format!("- members: {}\n", def.members.join(", ")));
        }
        if let Some(budget) = &def.budget {
            out.push_str(&format!("- budget: {:?}\n", budget));
        }
        if !def.prompt.trim().is_empty() {
            out.push_str("### prompt\n");
            out.push_str(&def.prompt);
            out.push('\n');
        }
        if !def.handoff_format.trim().is_empty() {
            out.push_str("### handoff_format\n");
            out.push_str(&def.handoff_format);
            out.push('\n');
        }
    }
    cap_chars(out, MAX_TEXT_CHARS)
}

fn cap_chars(s: String, max: usize) -> String {
    if s.chars().count() <= max {
        s
    } else {
        let head: String = s.chars().take(max).collect();
        format!("{head}\n…[Agent 模板内容已截断]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_agent_total_token_budget() {
        let raw = r#"{
          "name": "budgeted",
          "budget": { "max_input_tokens": 4000, "max_total_tokens": 12000, "max_steps": 3 }
        }"#;
        let parsed: AgentFile = serde_json::from_str(raw).unwrap();
        let budget = parsed.budget.unwrap();
        assert_eq!(budget.max_input_tokens, Some(4000));
        assert_eq!(budget.max_total_tokens, Some(12000));
        assert_eq!(budget.max_steps, Some(3));
    }

    #[test]
    fn template_agent_json_is_valid() {
        let validation = validate_agent_json(&template_json());
        assert!(validation.ok, "{:?}", validation.errors);
        assert_eq!(validation.suggested_file_name, "researcher.json");
    }

    #[test]
    fn validation_requires_name() {
        let validation = validate_agent_json(r#"{ "prompt": "work" }"#);
        assert!(!validation.ok);
        assert!(validation.errors.iter().any(|error| error.contains("name")));
    }

    #[test]
    fn validation_rejects_invalid_budget_shape() {
        let validation = validate_agent_json(
            r#"{
              "name": "bad-budget",
              "budget": { "max_input_tokens": 1000, "reserved_output_tokens": 1000, "max_steps": 0 }
            }"#,
        );
        assert!(!validation.ok);
        assert_eq!(validation.errors.len(), 2);
    }

    #[test]
    fn runtime_stats_accumulate_safely() {
        let mut stats = AgentStatsFile::default();
        let entry = stats.agents.entry("researcher".to_string()).or_default();
        entry.run_count = entry.run_count.saturating_add(1);
        entry.total_tokens = entry.total_tokens.saturating_add(1200);
        assert_eq!(stats.agents["researcher"].run_count, 1);
        assert_eq!(stats.agents["researcher"].total_tokens, 1200);
    }
}
