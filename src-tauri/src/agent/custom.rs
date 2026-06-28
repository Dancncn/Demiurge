use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const AGENTS_DIR: &str = ".demiurge/agents";
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
}

#[derive(Clone, Debug, Deserialize)]
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
}

#[derive(Clone, Debug, Serialize)]
pub struct AgentPanelState {
    pub definitions: Vec<AgentDefinitionInfo>,
    pub agents_dir: String,
}

#[derive(Clone, Debug)]
pub struct ResolvedAgents {
    pub definitions: Vec<AgentDefinitionInfo>,
    pub prompt_overlay: String,
    pub allowed_tools: Vec<String>,
    pub max_input_tokens: Option<usize>,
    pub reserved_output_tokens: Option<usize>,
    pub max_steps: Option<usize>,
}

pub fn ensure_dir(state: &crate::AppState) -> Result<PathBuf, String> {
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let dir = sandbox.join(AGENTS_DIR);
    fs::create_dir_all(&dir).map_err(|e| format!("创建 Agent 目录失败：{e}"))?;
    Ok(dir)
}

pub fn panel_state(state: &crate::AppState) -> AgentPanelState {
    let dir = ensure_dir(state).unwrap_or_else(|_| PathBuf::from(AGENTS_DIR));
    AgentPanelState {
        definitions: list_definitions(state),
        agents_dir: dir.to_string_lossy().to_string(),
    }
}

pub fn list_definitions(state: &crate::AppState) -> Vec<AgentDefinitionInfo> {
    let Ok(dir) = ensure_dir(state) else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let valid_tools = valid_tool_names();
    let mut out = entries
        .filter_map(Result::ok)
        .filter_map(|entry| definition_from_path(&entry.path(), &valid_tools).ok())
        .collect::<Vec<_>>();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn load_agent(state: &crate::AppState, name: &str) -> Result<AgentDefinitionInfo, String> {
    let dir = ensure_dir(state)?;
    let path = find_agent_path(&dir, name).ok_or_else(|| format!("未找到 Agent `{name}`。"))?;
    definition_from_path(&path, &valid_tool_names())
}

pub fn resolve_selected(state: &crate::AppState, names: &[String]) -> Result<ResolvedAgents, String> {
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
        }
    }

    Ok(ResolvedAgents {
        prompt_overlay: build_overlay(&definitions),
        definitions,
        allowed_tools: allowed,
        max_input_tokens,
        reserved_output_tokens,
        max_steps,
    })
}

fn min_assign(slot: &mut Option<usize>, value: Option<usize>) {
    if let Some(value) = value {
        *slot = Some(slot.map(|cur| cur.min(value)).unwrap_or(value));
    }
}

fn definition_from_path(path: &Path, valid_tools: &HashSet<String>) -> Result<AgentDefinitionInfo, String> {
    if path.extension().and_then(|s| s.to_str()) != Some("json") {
        return Err("not an agent json".to_string());
    }
    let raw = fs::read_to_string(path).map_err(|e| format!("读取 Agent 清单失败：{e}"))?;
    let parsed: AgentFile = serde_json::from_str(&raw).map_err(|e| format!("解析 Agent JSON 失败：{e}"))?;
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
    })
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
