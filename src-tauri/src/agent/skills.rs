use std::cmp::Reverse;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;

const MAX_SKILL_FILE_BYTES: u64 = 64 * 1024;
const MAX_REFERENCE_FILE_BYTES: u64 = 32 * 1024;
const MAX_SELECTED_SKILLS: usize = 4;
const MAX_SKILL_BODY_CHARS: usize = 4_000;
const MAX_REFERENCE_CHARS: usize = 2_000;
const MAX_CONTEXT_CHARS: usize = 14_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub scope: SkillScope,
    pub skill_dir: PathBuf,
    pub skill_path: PathBuf,
    pub body: String,
    pub triggers: Vec<String>,
    pub declared_tool_needs: Vec<String>,
    pub required_permissions: Vec<String>,
    pub references: Vec<String>,
    pub always_include: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillScope {
    Global,
    Project,
    Repository,
    Pack,
    Claude,
    Legacy,
}

impl SkillScope {
    fn label(&self) -> &'static str {
        match self {
            SkillScope::Global => "global",
            SkillScope::Project => "project",
            SkillScope::Repository => "repository",
            SkillScope::Pack => "pack",
            SkillScope::Claude => "claude",
            SkillScope::Legacy => "legacy",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SkillSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub scope: SkillScope,
    pub path: String,
    pub triggers: Vec<String>,
    pub declared_tool_needs: Vec<String>,
    pub required_permissions: Vec<String>,
    pub references: Vec<String>,
    pub selected: bool,
    pub match_score: i32,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct SkillPanelState {
    pub skills: Vec<SkillSummary>,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct SkillContext {
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct SkillCatalog {
    pub skills: Vec<SkillDefinition>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct SkillFrontMatter {
    name: Option<String>,
    description: Option<String>,
    #[serde(default = "yaml_null")]
    triggers: YamlValue,
    #[serde(default = "yaml_null")]
    keywords: YamlValue,
    #[serde(default = "yaml_null")]
    tools: YamlValue,
    #[serde(default = "yaml_null")]
    declared_tool_needs: YamlValue,
    #[serde(default = "yaml_null")]
    required_permissions: YamlValue,
    #[serde(default = "yaml_null")]
    references: YamlValue,
    #[serde(default)]
    always_include: bool,
}

fn yaml_null() -> YamlValue {
    YamlValue::Null
}

pub fn context_for_turn(
    sandbox: &Path,
    data_dir: &Path,
    packs_dir: &Path,
    pack_id: &str,
    user_text: Option<&str>,
) -> SkillContext {
    let catalog = discover(sandbox, data_dir, packs_dir, pack_id);
    let selected = select(&catalog.skills, user_text);
    let text = render_context(&selected, &catalog.diagnostics);
    SkillContext { text }
}

pub fn panel_state(
    sandbox: &Path,
    data_dir: &Path,
    packs_dir: &Path,
    pack_id: &str,
    query: Option<&str>,
) -> SkillPanelState {
    let catalog = discover(sandbox, data_dir, packs_dir, pack_id);
    let selected = select(&catalog.skills, query);
    let selected_ids = selected
        .iter()
        .map(|(skill, score)| (skill.id.as_str(), *score))
        .collect::<Vec<_>>();
    let mut skills = catalog
        .skills
        .iter()
        .map(|skill| {
            let score = selected_ids
                .iter()
                .find_map(|(id, score)| (*id == skill.id).then_some(*score))
                .unwrap_or_else(|| match_score(skill, query.unwrap_or_default()));
            summary(
                skill,
                selected_ids.iter().any(|(id, _)| *id == skill.id),
                score,
            )
        })
        .collect::<Vec<_>>();
    skills.sort_by(|a, b| {
        b.selected
            .cmp(&a.selected)
            .then_with(|| b.match_score.cmp(&a.match_score))
            .then_with(|| a.scope.label().cmp(b.scope.label()))
            .then_with(|| a.name.cmp(&b.name))
    });
    SkillPanelState {
        skills,
        diagnostics: catalog.diagnostics,
    }
}

pub fn slash_response(state: &crate::AppState, text: &str) -> Result<String, String> {
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let data_dir = state.data_dir.lock().unwrap().clone();
    let packs_dir = state.packs_dir.lock().unwrap().clone();
    let settings = state.settings.lock().unwrap().clone();
    let query = text
        .trim()
        .strip_prefix("/skills")
        .or_else(|| text.trim().strip_prefix("/skill"))
        .unwrap_or("")
        .trim();
    let query = (!query.is_empty()).then_some(query);
    let panel = panel_state(
        &sandbox,
        &data_dir,
        &packs_dir,
        &settings.current_pack,
        query,
    );
    Ok(format_panel(&panel, query))
}

pub fn discover(sandbox: &Path, data_dir: &Path, packs_dir: &Path, pack_id: &str) -> SkillCatalog {
    let mut catalog = SkillCatalog::default();
    for (scope, base) in [
        (SkillScope::Global, data_dir.join("skills")),
        (
            SkillScope::Project,
            sandbox.join(".demiurge").join("skills"),
        ),
        (SkillScope::Repository, sandbox.join("skills")),
        (SkillScope::Pack, packs_dir.join(pack_id).join("skills")),
        (SkillScope::Claude, sandbox.join(".claude").join("skills")),
    ] {
        discover_skill_dir(&mut catalog, scope, &base);
    }

    for (label, rel) in [
        ("Project skills", ".demiurge/skills.md"),
        ("Repository skills", "skills/README.md"),
        ("Claude skills", ".claude/skills.md"),
    ] {
        let path = sandbox.join(rel);
        if let Some(raw) = read_limited_text(&path, MAX_SKILL_FILE_BYTES) {
            catalog.skills.push(SkillDefinition {
                id: sanitize_id(&format!("legacy-{label}")),
                name: label.to_string(),
                description: format!("Legacy Markdown skill notes from {rel}."),
                scope: SkillScope::Legacy,
                skill_dir: path.parent().unwrap_or(sandbox).to_path_buf(),
                skill_path: path,
                body: raw,
                triggers: Vec::new(),
                declared_tool_needs: Vec::new(),
                required_permissions: Vec::new(),
                references: Vec::new(),
                always_include: true,
            });
        }
    }

    catalog.skills.sort_by(|a, b| {
        a.scope
            .label()
            .cmp(b.scope.label())
            .then(a.name.cmp(&b.name))
    });
    catalog
}

fn discover_skill_dir(catalog: &mut SkillCatalog, scope: SkillScope, base: &Path) {
    if !base.is_dir() {
        return;
    }

    let direct = base.join("SKILL.md");
    if direct.is_file() {
        match read_skill(scope.clone(), base, &direct) {
            Ok(skill) => catalog.skills.push(skill),
            Err(err) => catalog.diagnostics.push(err),
        }
    }

    let Ok(entries) = fs::read_dir(base) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skill_path = path.join("SKILL.md");
        if !skill_path.is_file() {
            continue;
        }
        match read_skill(scope.clone(), &path, &skill_path) {
            Ok(skill) => catalog.skills.push(skill),
            Err(err) => catalog.diagnostics.push(err),
        }
    }
}

fn read_skill(
    scope: SkillScope,
    skill_dir: &Path,
    skill_path: &Path,
) -> Result<SkillDefinition, String> {
    let raw = read_limited_text(skill_path, MAX_SKILL_FILE_BYTES)
        .ok_or_else(|| format!("Skipped unreadable skill {}", skill_path.display()))?;
    let (front, body) = split_frontmatter(&raw)?;
    let meta = front
        .map(|text| {
            serde_yaml::from_str::<SkillFrontMatter>(text)
                .map_err(|e| format!("Invalid skill frontmatter in {}: {e}", skill_path.display()))
        })
        .transpose()?
        .unwrap_or_default();
    let fallback_name = skill_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("skill")
        .trim()
        .to_string();
    let name = meta
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&fallback_name)
        .to_string();
    let description = meta
        .description
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let mut triggers = yaml_strings(&meta.triggers);
    triggers.extend(yaml_strings(&meta.keywords));
    triggers.sort();
    triggers.dedup();
    let mut declared_tool_needs = yaml_strings(&meta.tools);
    declared_tool_needs.extend(yaml_strings(&meta.declared_tool_needs));
    declared_tool_needs.sort();
    declared_tool_needs.dedup();
    let required_permissions = yaml_strings(&meta.required_permissions);
    let references = yaml_strings(&meta.references)
        .into_iter()
        .filter(|reference| is_safe_relative(reference))
        .collect::<Vec<_>>();
    let id = sanitize_id(&format!("{}-{}", scope.label(), name));
    Ok(SkillDefinition {
        id,
        name,
        description,
        scope,
        skill_dir: skill_dir.to_path_buf(),
        skill_path: skill_path.to_path_buf(),
        body: body.trim().to_string(),
        triggers,
        declared_tool_needs,
        required_permissions,
        references,
        always_include: meta.always_include,
    })
}

fn split_frontmatter(raw: &str) -> Result<(Option<&str>, &str), String> {
    let trimmed = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    if !trimmed.starts_with("---\n") && !trimmed.starts_with("---\r\n") {
        return Ok((None, raw));
    }
    let body_start = if trimmed.starts_with("---\r\n") { 5 } else { 4 };
    let rest = &trimmed[body_start..];
    let Some(end) = rest.find("\n---") else {
        return Err("Skill frontmatter starts with --- but has no closing ---".to_string());
    };
    let front = &rest[..end];
    let after_marker = &rest[end + 1..];
    let body = after_marker
        .strip_prefix("---\r\n")
        .or_else(|| after_marker.strip_prefix("---\n"))
        .or_else(|| after_marker.strip_prefix("---"))
        .unwrap_or(after_marker);
    Ok((Some(front), body))
}

fn yaml_strings(value: &YamlValue) -> Vec<String> {
    match value {
        YamlValue::Null => Vec::new(),
        YamlValue::String(s) => split_csv_like(s),
        YamlValue::Sequence(seq) => seq
            .iter()
            .flat_map(yaml_strings)
            .filter(|s| !s.trim().is_empty())
            .collect(),
        YamlValue::Mapping(map) => map
            .keys()
            .filter_map(|key| key.as_str().map(str::to_string))
            .collect(),
        YamlValue::Bool(value) => vec![value.to_string()],
        YamlValue::Number(value) => vec![value.to_string()],
        YamlValue::Tagged(tagged) => yaml_strings(&tagged.value),
    }
}

fn split_csv_like(value: &str) -> Vec<String> {
    value
        .split([',', '\n'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn select<'a>(
    skills: &'a [SkillDefinition],
    user_text: Option<&str>,
) -> Vec<(&'a SkillDefinition, i32)> {
    let query = user_text.unwrap_or_default();
    let mut scored = skills
        .iter()
        .filter_map(|skill| {
            let score = match_score(skill, query);
            (skill.always_include || score > 0).then_some((skill, score))
        })
        .collect::<Vec<_>>();
    scored.sort_by_key(|(skill, score)| {
        (
            !skill.always_include,
            Reverse(*score),
            skill.scope.label().to_string(),
            skill.name.clone(),
        )
    });
    scored.truncate(MAX_SELECTED_SKILLS);
    scored
}

fn match_score(skill: &SkillDefinition, user_text: &str) -> i32 {
    let query = normalize(user_text);
    if query.is_empty() {
        return if skill.always_include { 1 } else { 0 };
    }

    let mut score = 0;
    let name = normalize(&skill.name);
    if !name.is_empty() && query.contains(&name) {
        score += 8;
    }
    for word in name.split_whitespace().filter(|word| word.len() >= 3) {
        if query.contains(word) {
            score += 2;
        }
    }
    let description = normalize(&skill.description);
    for word in description
        .split_whitespace()
        .filter(|word| word.len() >= 4)
    {
        if query.contains(word) {
            score += 1;
        }
    }
    for trigger in &skill.triggers {
        let trigger = normalize(trigger);
        if trigger == "*" || trigger == "always" {
            score += 2;
        } else if !trigger.is_empty() && query.contains(&trigger) {
            score += 10;
        } else {
            for word in trigger.split_whitespace().filter(|word| word.len() >= 4) {
                if query.contains(word) {
                    score += 2;
                }
            }
        }
    }
    score
}

fn normalize(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn summary(skill: &SkillDefinition, selected: bool, match_score: i32) -> SkillSummary {
    SkillSummary {
        id: skill.id.clone(),
        name: skill.name.clone(),
        description: skill.description.clone(),
        scope: skill.scope.clone(),
        path: skill.skill_path.display().to_string(),
        triggers: skill.triggers.clone(),
        declared_tool_needs: skill.declared_tool_needs.clone(),
        required_permissions: skill.required_permissions.clone(),
        references: skill.references.clone(),
        selected,
        match_score,
    }
}

fn render_context(selected: &[(&SkillDefinition, i32)], diagnostics: &[String]) -> String {
    let mut parts = Vec::new();
    for (skill, score) in selected {
        let mut part = String::new();
        part.push_str(&format!(
            "## {} [{} / score={}]\n",
            skill.name,
            skill.scope.label(),
            score
        ));
        if !skill.description.is_empty() {
            part.push_str(&format!("Description: {}\n", skill.description));
        }
        if !skill.declared_tool_needs.is_empty() {
            part.push_str(&format!(
                "Declared tool needs: {}\n",
                skill.declared_tool_needs.join(", ")
            ));
        }
        if !skill.required_permissions.is_empty() {
            part.push_str(&format!(
                "Required permissions: {}\n",
                skill.required_permissions.join(", ")
            ));
        }
        part.push_str(&format!("Source: {}\n\n", skill.skill_path.display()));
        part.push_str("### SKILL.md\n");
        part.push_str(&cap_chars(&skill.body, MAX_SKILL_BODY_CHARS));
        let refs = render_references(skill);
        if !refs.is_empty() {
            part.push_str("\n\n### References\n");
            part.push_str(&refs);
        }
        parts.push(part);
    }
    if !diagnostics.is_empty() {
        parts.push(format!(
            "## Skill diagnostics\n{}",
            diagnostics
                .iter()
                .take(5)
                .map(|item| format!("- {item}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    cap_chars(&parts.join("\n\n"), MAX_CONTEXT_CHARS)
}

fn render_references(skill: &SkillDefinition) -> String {
    let mut out = Vec::new();
    for reference in &skill.references {
        if !is_safe_relative(reference) {
            continue;
        }
        let path = skill.skill_dir.join(reference);
        let Some(text) = read_limited_text(&path, MAX_REFERENCE_FILE_BYTES) else {
            continue;
        };
        out.push(format!(
            "#### {}\n{}",
            reference,
            cap_chars(text.trim(), MAX_REFERENCE_CHARS)
        ));
    }
    out.join("\n\n")
}

fn format_panel(panel: &SkillPanelState, query: Option<&str>) -> String {
    let mut out = if let Some(query) = query {
        format!("Skills matching `{query}`:\n")
    } else {
        "Skills:\n".to_string()
    };
    if panel.skills.is_empty() {
        out.push_str("- No skills found. Add SKILL.md files under app data `skills/`, sandbox `.demiurge/skills/`, repository `skills/`, current pack `skills/`, or `.claude/skills/`.\n");
    } else {
        for skill in panel.skills.iter().take(40) {
            let selected = if skill.selected { " selected" } else { "" };
            let description = if skill.description.is_empty() {
                "".to_string()
            } else {
                format!(" - {}", skill.description)
            };
            out.push_str(&format!(
                "- `{}` [{} score={}]{}{}\n",
                skill.name,
                skill.scope.label(),
                skill.match_score,
                selected,
                description
            ));
            if !skill.triggers.is_empty() {
                out.push_str(&format!("  triggers: {}\n", skill.triggers.join(", ")));
            }
            if !skill.declared_tool_needs.is_empty() {
                out.push_str(&format!(
                    "  tools: {}\n",
                    skill.declared_tool_needs.join(", ")
                ));
            }
            if !skill.required_permissions.is_empty() {
                out.push_str(&format!(
                    "  permissions: {}\n",
                    skill.required_permissions.join(", ")
                ));
            }
        }
    }
    if !panel.diagnostics.is_empty() {
        out.push_str("\nDiagnostics:\n");
        for item in panel.diagnostics.iter().take(10) {
            out.push_str(&format!("- {item}\n"));
        }
    }
    out
}

fn read_limited_text(path: &Path, max_bytes: u64) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > max_bytes {
        return None;
    }
    fs::read_to_string(path).ok()
}

fn cap_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        let note = "\n[truncated]";
        let take = max_chars.saturating_sub(note.chars().count()).max(1);
        format!("{}{}", value.chars().take(take).collect::<String>(), note)
    }
}

fn sanitize_id(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in value.to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "skill".to_string()
    } else {
        out
    }
}

fn is_safe_relative(value: &str) -> bool {
    let path = Path::new(value);
    if value.trim().is_empty() || path.is_absolute() {
        return false;
    }
    path.components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "demiurge_skills_test_{}_{}",
            name,
            crate::store::now_millis()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parses_skill_frontmatter_lists_and_body() {
        let root = temp_root("parse");
        let skill_dir = root.join("skills").join("review");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: Evidence Review
description: Review code with evidence.
triggers:
  - review
  - evidence
tools: [grep, read_file]
required_permissions: read_only
references:
  - refs/checklist.md
---
Use evidence before conclusions.
"#,
        )
        .unwrap();
        let skill =
            read_skill(SkillScope::Project, &skill_dir, &skill_dir.join("SKILL.md")).unwrap();
        assert_eq!(skill.name, "Evidence Review");
        assert_eq!(skill.triggers, vec!["evidence", "review"]);
        assert_eq!(skill.declared_tool_needs, vec!["grep", "read_file"]);
        assert_eq!(skill.required_permissions, vec!["read_only"]);
        assert_eq!(skill.references, vec!["refs/checklist.md"]);
        assert!(skill.body.contains("Use evidence"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn discovers_global_project_and_pack_skills_and_injects_references() {
        let sandbox = temp_root("sandbox");
        let data = temp_root("data");
        let packs = temp_root("packs");
        let global = data.join("skills").join("web");
        let project = sandbox.join(".demiurge").join("skills").join("rust");
        let pack = packs.join("default").join("skills").join("persona");
        fs::create_dir_all(global.join("refs")).unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&pack).unwrap();
        fs::write(global.join("refs").join("guide.md"), "Use current sources.").unwrap();
        fs::write(
            global.join("SKILL.md"),
            "---\nname: Web Research\ndescription: Search and fetch current web sources.\ntriggers: [search, web]\ntools: [web_search, web_fetch]\nreferences: [refs/guide.md]\n---\nAlways cite sources.",
        )
        .unwrap();
        fs::write(
            project.join("SKILL.md"),
            "---\nname: Rust Repair\ntriggers: [rust, cargo]\n---\nRun cargo tests.",
        )
        .unwrap();
        fs::write(
            pack.join("SKILL.md"),
            "---\nname: Pack Voice\nalways_include: true\n---\nKeep pack tone.",
        )
        .unwrap();

        let context = context_for_turn(
            &sandbox,
            &data,
            &packs,
            "default",
            Some("please search the web"),
        );
        assert!(context.text.contains("Web Research"));
        assert!(context
            .text
            .contains("Declared tool needs: web_fetch, web_search"));
        assert!(context.text.contains("Use current sources."));
        assert!(context.text.contains("Pack Voice"));
        let panel = panel_state(
            &sandbox,
            &data,
            &packs,
            "default",
            Some("please search the web"),
        );
        assert!(panel
            .skills
            .iter()
            .any(|skill| skill.scope == SkillScope::Global && skill.selected));

        let _ = fs::remove_dir_all(&sandbox);
        let _ = fs::remove_dir_all(&data);
        let _ = fs::remove_dir_all(&packs);
    }

    #[test]
    fn rejects_unsafe_reference_paths() {
        assert!(is_safe_relative("refs/guide.md"));
        assert!(is_safe_relative("./refs/guide.md"));
        assert!(!is_safe_relative("../secret.md"));
        assert!(!is_safe_relative("refs/../../secret.md"));
        assert!(!is_safe_relative("C:/secret.md"));
        assert!(!is_safe_relative(""));
    }
}
