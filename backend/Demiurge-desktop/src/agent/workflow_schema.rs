use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const MAX_PARALLEL_ITEMS: usize = 8;
const MAX_WORKFLOW_NODES: usize = 256;
const MAX_WORKFLOW_DEPTH: usize = 12;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct WorkflowFile {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub inputs: Vec<WorkflowInputDefinition>,
    #[serde(default)]
    pub steps: Vec<WorkflowStep>,
}

fn default_schema_version() -> u32 {
    1
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkflowInputDefinition {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub kind: WorkflowInputKind,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub placeholder: String,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowInputKind {
    #[default]
    Text,
    Textarea,
    Number,
    Boolean,
    Select,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum WorkflowStep {
    Log {
        #[serde(default)]
        id: Option<String>,
        message: String,
    },
    Phase {
        #[serde(default)]
        id: Option<String>,
        name: String,
        steps: Vec<WorkflowStep>,
    },
    Agent {
        #[serde(default)]
        id: Option<String>,
        prompt: String,
        label: Option<String>,
        agent_type: Option<String>,
        agent: Option<String>,
        context_mode: Option<String>,
    },
    Parallel {
        #[serde(default)]
        id: Option<String>,
        items: Vec<WorkflowStep>,
    },
    Pipeline {
        #[serde(default)]
        id: Option<String>,
        items: Vec<WorkflowStep>,
    },
    Budget {
        #[serde(default)]
        id: Option<String>,
        total: Option<usize>,
    },
}

impl WorkflowStep {
    fn explicit_id(&self) -> Option<&str> {
        match self {
            Self::Log { id, .. }
            | Self::Phase { id, .. }
            | Self::Agent { id, .. }
            | Self::Parallel { id, .. }
            | Self::Pipeline { id, .. }
            | Self::Budget { id, .. } => id.as_deref(),
        }
    }

    pub(super) fn kind(&self) -> &'static str {
        match self {
            Self::Log { .. } => "log",
            Self::Phase { .. } => "phase",
            Self::Agent { .. } => "agent",
            Self::Parallel { .. } => "parallel",
            Self::Pipeline { .. } => "pipeline",
            Self::Budget { .. } => "budget",
        }
    }

    pub(super) fn label(&self) -> String {
        match self {
            Self::Log { message, .. } => cap_chars(message, 72),
            Self::Phase { name, .. } => name.clone(),
            Self::Agent { label, prompt, .. } => {
                label.clone().unwrap_or_else(|| cap_chars(prompt, 72))
            }
            Self::Parallel { items, .. } => format!("{} parallel branches", items.len()),
            Self::Pipeline { items, .. } => format!("{} pipeline steps", items.len()),
            Self::Budget { total, .. } => total
                .map(|value| format!("token budget {value}"))
                .unwrap_or_else(|| "unlimited token budget".to_string()),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowValidationLevel {
    Error,
    Warning,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowValidationIssue {
    pub level: WorkflowValidationLevel,
    pub path: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct WorkflowValidationReport {
    pub valid: bool,
    pub issues: Vec<WorkflowValidationIssue>,
    pub normalized_inputs: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Serialize)]
pub struct WorkflowDryRunNode {
    pub node_id: String,
    pub kind: String,
    pub label: String,
    pub preview: String,
    pub children: Vec<WorkflowDryRunNode>,
}

#[derive(Clone, Debug, Serialize)]
pub struct WorkflowDryRun {
    pub name: String,
    pub valid: bool,
    pub issues: Vec<WorkflowValidationIssue>,
    pub normalized_inputs: BTreeMap<String, Value>,
    pub steps_total: usize,
    pub nodes: Vec<WorkflowDryRunNode>,
}

#[derive(Clone, Debug, Serialize)]
pub struct WorkflowTemplateInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub definition: Value,
}

pub(super) fn validate_workflow(
    workflow: &WorkflowFile,
    provided_inputs: &BTreeMap<String, Value>,
) -> WorkflowValidationReport {
    let mut issues = Vec::new();
    if workflow.schema_version != 1 {
        issues.push(error(
            "schema_version",
            format!(
                "unsupported workflow schema version {}; expected 1",
                workflow.schema_version
            ),
        ));
    }
    if workflow.steps.is_empty() {
        issues.push(error("steps", "workflow must contain at least one step"));
    }

    let normalized_inputs = validate_inputs(&workflow.inputs, provided_inputs, &mut issues);
    if let Some(name) = &workflow.name {
        validate_template(name, "name", &normalized_inputs, &mut issues);
    }
    if let Some(description) = &workflow.description {
        validate_template(description, "description", &normalized_inputs, &mut issues);
    }
    let mut explicit_ids = HashSet::new();
    let mut node_count = 0usize;
    validate_steps(
        &workflow.steps,
        "steps",
        0,
        &mut node_count,
        &mut explicit_ids,
        &normalized_inputs,
        &mut issues,
    );
    if node_count > MAX_WORKFLOW_NODES {
        issues.push(error(
            "steps",
            format!("workflow contains {node_count} nodes; maximum is {MAX_WORKFLOW_NODES}"),
        ));
    }

    WorkflowValidationReport {
        valid: !issues
            .iter()
            .any(|issue| issue.level == WorkflowValidationLevel::Error),
        issues,
        normalized_inputs,
    }
}

fn validate_inputs(
    definitions: &[WorkflowInputDefinition],
    provided: &BTreeMap<String, Value>,
    issues: &mut Vec<WorkflowValidationIssue>,
) -> BTreeMap<String, Value> {
    let mut normalized = BTreeMap::new();
    let mut seen = HashSet::new();
    for (index, field) in definitions.iter().enumerate() {
        let path = format!("inputs[{index}]");
        let key = field.key.trim();
        if !valid_identifier(key) {
            issues.push(error(
                format!("{path}.key"),
                "input key must use 1-64 ASCII letters, numbers, '_' or '-'",
            ));
            continue;
        }
        if !seen.insert(key.to_string()) {
            issues.push(error(
                format!("{path}.key"),
                format!("duplicate input key `{key}`"),
            ));
            continue;
        }
        if field.label.trim().is_empty() {
            issues.push(error(format!("{path}.label"), "input label is required"));
        }
        if field.kind == WorkflowInputKind::Select {
            if field.options.is_empty() {
                issues.push(error(
                    format!("{path}.options"),
                    "select input must define at least one option",
                ));
            }
            let mut option_set = HashSet::new();
            for option in &field.options {
                if option.trim().is_empty() || !option_set.insert(option) {
                    issues.push(error(
                        format!("{path}.options"),
                        "select options must be non-empty and unique",
                    ));
                    break;
                }
            }
        }
        if field.min.zip(field.max).is_some_and(|(min, max)| min > max) {
            issues.push(error(
                format!("{path}.min"),
                "number input min cannot be greater than max",
            ));
        }

        let value = provided.get(key).cloned().or_else(|| field.default.clone());
        match value {
            Some(value) => {
                validate_input_value(field, &value, &path, issues);
                normalized.insert(key.to_string(), value);
            }
            None if field.required => issues.push(error(
                format!("values.{key}"),
                format!("required input `{}` is missing", field.label),
            )),
            None => {}
        }
    }

    for key in provided.keys() {
        if !seen.contains(key) {
            issues.push(warning(
                format!("values.{key}"),
                format!("unknown input `{key}` will be ignored"),
            ));
        }
    }
    normalized
}

fn validate_input_value(
    field: &WorkflowInputDefinition,
    value: &Value,
    path: &str,
    issues: &mut Vec<WorkflowValidationIssue>,
) {
    let value_path = format!("values.{}", field.key);
    match field.kind {
        WorkflowInputKind::Text | WorkflowInputKind::Textarea => match value.as_str() {
            Some(text) if field.required && text.trim().is_empty() => issues.push(error(
                value_path,
                format!("required input `{}` cannot be empty", field.label),
            )),
            Some(_) => {}
            None => issues.push(error(value_path, "expected a string value")),
        },
        WorkflowInputKind::Number => match value.as_f64() {
            Some(number) => {
                if field.min.is_some_and(|min| number < min) {
                    issues.push(error(
                        value_path.clone(),
                        format!("value must be at least {}", field.min.unwrap_or_default()),
                    ));
                }
                if field.max.is_some_and(|max| number > max) {
                    issues.push(error(
                        value_path,
                        format!("value must be at most {}", field.max.unwrap_or_default()),
                    ));
                }
            }
            None => issues.push(error(value_path, "expected a numeric value")),
        },
        WorkflowInputKind::Boolean => {
            if !value.is_boolean() {
                issues.push(error(value_path, "expected a boolean value"));
            }
        }
        WorkflowInputKind::Select => match value.as_str() {
            Some(selected) if field.options.iter().any(|option| option == selected) => {}
            Some(selected) => issues.push(error(
                value_path,
                format!("`{selected}` is not one of the allowed options"),
            )),
            None => issues.push(error(value_path, "expected a string option")),
        },
    }

    if let Some(default) = &field.default {
        let mut default_issues = Vec::new();
        let mut field_without_default = field.clone();
        field_without_default.default = None;
        validate_input_value_without_default(&field_without_default, default, &mut default_issues);
        if !default_issues.is_empty() {
            issues.push(error(
                format!("{path}.default"),
                "default value does not match the input definition",
            ));
        }
    }
}

fn validate_input_value_without_default(
    field: &WorkflowInputDefinition,
    value: &Value,
    issues: &mut Vec<()>,
) {
    let valid = match field.kind {
        WorkflowInputKind::Text | WorkflowInputKind::Textarea => value.is_string(),
        WorkflowInputKind::Number => value.as_f64().is_some_and(|number| {
            !field.min.is_some_and(|min| number < min) && !field.max.is_some_and(|max| number > max)
        }),
        WorkflowInputKind::Boolean => value.is_boolean(),
        WorkflowInputKind::Select => value
            .as_str()
            .is_some_and(|selected| field.options.iter().any(|option| option == selected)),
    };
    if !valid {
        issues.push(());
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_steps(
    steps: &[WorkflowStep],
    path: &str,
    depth: usize,
    node_count: &mut usize,
    explicit_ids: &mut HashSet<String>,
    inputs: &BTreeMap<String, Value>,
    issues: &mut Vec<WorkflowValidationIssue>,
) {
    if depth > MAX_WORKFLOW_DEPTH {
        issues.push(error(
            path,
            format!("workflow nesting exceeds {MAX_WORKFLOW_DEPTH} levels"),
        ));
        return;
    }
    for (index, step) in steps.iter().enumerate() {
        *node_count = node_count.saturating_add(1);
        let step_path = format!("{path}[{index}]");
        if let Some(id) = step.explicit_id() {
            if !valid_identifier(id) {
                issues.push(error(
                    format!("{step_path}.id"),
                    "node id must use 1-64 ASCII letters, numbers, '_' or '-'",
                ));
            } else if !explicit_ids.insert(id.to_string()) {
                issues.push(error(
                    format!("{step_path}.id"),
                    format!("duplicate node id `{id}`"),
                ));
            }
        }

        match step {
            WorkflowStep::Log { message, .. } => {
                validate_template(message, &format!("{step_path}.message"), inputs, issues);
            }
            WorkflowStep::Phase { name, steps, .. } => {
                if name.trim().is_empty() {
                    issues.push(error(format!("{step_path}.name"), "phase name is required"));
                }
                validate_template(name, &format!("{step_path}.name"), inputs, issues);
                if steps.is_empty() {
                    issues.push(error(
                        format!("{step_path}.steps"),
                        "phase must contain at least one step",
                    ));
                }
                validate_steps(
                    steps,
                    &format!("{step_path}.steps"),
                    depth + 1,
                    node_count,
                    explicit_ids,
                    inputs,
                    issues,
                );
            }
            WorkflowStep::Agent {
                prompt,
                label,
                agent_type,
                agent,
                context_mode,
                ..
            } => {
                if prompt.trim().is_empty() {
                    issues.push(error(
                        format!("{step_path}.prompt"),
                        "agent prompt is required",
                    ));
                }
                validate_template(prompt, &format!("{step_path}.prompt"), inputs, issues);
                for (field, value) in [
                    ("label", label.as_deref()),
                    ("agent_type", agent_type.as_deref()),
                    ("agent", agent.as_deref()),
                ] {
                    if let Some(value) = value {
                        validate_template(value, &format!("{step_path}.{field}"), inputs, issues);
                    }
                }
                if let Some(mode) = context_mode {
                    validate_template(mode, &format!("{step_path}.context_mode"), inputs, issues);
                    if !mode.contains("{{")
                        && !matches!(
                            mode.trim().to_ascii_lowercase().as_str(),
                            "minimal" | "project" | "full"
                        )
                    {
                        issues.push(error(
                            format!("{step_path}.context_mode"),
                            "context_mode must be minimal, project or full",
                        ));
                    }
                }
            }
            WorkflowStep::Parallel { items, .. } => {
                if items.is_empty() {
                    issues.push(error(
                        format!("{step_path}.items"),
                        "parallel step must contain at least one item",
                    ));
                }
                if items.len() > MAX_PARALLEL_ITEMS {
                    issues.push(error(
                        format!("{step_path}.items"),
                        format!("parallel step supports at most {MAX_PARALLEL_ITEMS} items"),
                    ));
                }
                validate_steps(
                    items,
                    &format!("{step_path}.items"),
                    depth + 1,
                    node_count,
                    explicit_ids,
                    inputs,
                    issues,
                );
            }
            WorkflowStep::Pipeline { items, .. } => {
                if items.is_empty() {
                    issues.push(error(
                        format!("{step_path}.items"),
                        "pipeline step must contain at least one item",
                    ));
                }
                validate_steps(
                    items,
                    &format!("{step_path}.items"),
                    depth + 1,
                    node_count,
                    explicit_ids,
                    inputs,
                    issues,
                );
            }
            WorkflowStep::Budget { total, .. } => {
                if total == &Some(0) {
                    issues.push(error(
                        format!("{step_path}.total"),
                        "token budget must be greater than zero or null",
                    ));
                }
            }
        }
    }
}

fn validate_template(
    source: &str,
    path: &str,
    inputs: &BTreeMap<String, Value>,
    issues: &mut Vec<WorkflowValidationIssue>,
) {
    if let Err(message) = render_template(source, inputs) {
        issues.push(error(path, message));
    }
}

pub(super) fn render_workflow(
    workflow: &WorkflowFile,
    inputs: &BTreeMap<String, Value>,
) -> Result<WorkflowFile, String> {
    Ok(WorkflowFile {
        schema_version: workflow.schema_version,
        name: workflow
            .name
            .as_deref()
            .map(|value| render_template(value, inputs))
            .transpose()?,
        description: workflow
            .description
            .as_deref()
            .map(|value| render_template(value, inputs))
            .transpose()?,
        inputs: workflow.inputs.clone(),
        steps: workflow
            .steps
            .iter()
            .map(|step| render_step(step, inputs))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn render_step(
    step: &WorkflowStep,
    inputs: &BTreeMap<String, Value>,
) -> Result<WorkflowStep, String> {
    Ok(match step {
        WorkflowStep::Log { id, message } => WorkflowStep::Log {
            id: id.clone(),
            message: render_template(message, inputs)?,
        },
        WorkflowStep::Phase { id, name, steps } => WorkflowStep::Phase {
            id: id.clone(),
            name: render_template(name, inputs)?,
            steps: steps
                .iter()
                .map(|step| render_step(step, inputs))
                .collect::<Result<Vec<_>, _>>()?,
        },
        WorkflowStep::Agent {
            id,
            prompt,
            label,
            agent_type,
            agent,
            context_mode,
        } => WorkflowStep::Agent {
            id: id.clone(),
            prompt: render_template(prompt, inputs)?,
            label: render_optional(label, inputs)?,
            agent_type: render_optional(agent_type, inputs)?,
            agent: render_optional(agent, inputs)?,
            context_mode: render_optional(context_mode, inputs)?,
        },
        WorkflowStep::Parallel { id, items } => WorkflowStep::Parallel {
            id: id.clone(),
            items: items
                .iter()
                .map(|step| render_step(step, inputs))
                .collect::<Result<Vec<_>, _>>()?,
        },
        WorkflowStep::Pipeline { id, items } => WorkflowStep::Pipeline {
            id: id.clone(),
            items: items
                .iter()
                .map(|step| render_step(step, inputs))
                .collect::<Result<Vec<_>, _>>()?,
        },
        WorkflowStep::Budget { id, total } => WorkflowStep::Budget {
            id: id.clone(),
            total: *total,
        },
    })
}

fn render_optional(
    value: &Option<String>,
    inputs: &BTreeMap<String, Value>,
) -> Result<Option<String>, String> {
    value
        .as_deref()
        .map(|value| render_template(value, inputs))
        .transpose()
}

fn render_template(source: &str, inputs: &BTreeMap<String, Value>) -> Result<String, String> {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find("}}") else {
            return Err("template contains an unclosed `{{` marker".to_string());
        };
        let raw_key = after_start[..end].trim();
        let key = raw_key.strip_prefix("inputs.").unwrap_or(raw_key).trim();
        if !valid_identifier(key) {
            return Err(format!("invalid template input reference `{raw_key}`"));
        }
        let value = inputs
            .get(key)
            .ok_or_else(|| format!("template input `{key}` has no value"))?;
        out.push_str(&value_to_template_text(value)?);
        rest = &after_start[end + 2..];
    }
    if rest.contains("}}") {
        return Err("template contains a closing `}}` without an opening marker".to_string());
    }
    out.push_str(rest);
    Ok(out)
}

fn value_to_template_text(value: &Value) -> Result<String, String> {
    match value {
        Value::String(value) => Ok(value.clone()),
        Value::Number(value) => Ok(value.to_string()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Null => Ok(String::new()),
        Value::Array(_) | Value::Object(_) => {
            Err("workflow template values must be scalar".to_string())
        }
    }
}

pub(super) fn dry_run(
    workflow: &WorkflowFile,
    fallback_name: &str,
    provided_inputs: &BTreeMap<String, Value>,
) -> WorkflowDryRun {
    let report = validate_workflow(workflow, provided_inputs);
    let rendered = if report.valid {
        render_workflow(workflow, &report.normalized_inputs).ok()
    } else {
        None
    };
    let nodes = rendered
        .as_ref()
        .map(|workflow| build_plan_nodes(&workflow.steps, "steps"))
        .unwrap_or_default();
    WorkflowDryRun {
        name: rendered
            .as_ref()
            .and_then(|workflow| workflow.name.clone())
            .or_else(|| workflow.name.clone())
            .unwrap_or_else(|| fallback_name.to_string()),
        valid: report.valid,
        issues: report.issues,
        normalized_inputs: report.normalized_inputs,
        steps_total: rendered
            .as_ref()
            .map(|workflow| count_steps(&workflow.steps))
            .unwrap_or(0),
        nodes,
    }
}

fn build_plan_nodes(steps: &[WorkflowStep], path: &str) -> Vec<WorkflowDryRunNode> {
    steps
        .iter()
        .enumerate()
        .map(|(index, step)| {
            let generated = format!("{path}[{index}]");
            let node_id = node_id(step, &generated);
            let (preview, children) = match step {
                WorkflowStep::Log { message, .. } => (cap_chars(message, 180), Vec::new()),
                WorkflowStep::Phase { steps, .. } => (
                    format!("Run {} phase steps in order", steps.len()),
                    build_plan_nodes(steps, &format!("{node_id}.steps")),
                ),
                WorkflowStep::Agent {
                    prompt,
                    agent,
                    context_mode,
                    ..
                } => (
                    format!(
                        "{}{}{}",
                        cap_chars(prompt, 180),
                        agent
                            .as_deref()
                            .map(|value| format!(" / agent {value}"))
                            .unwrap_or_default(),
                        context_mode
                            .as_deref()
                            .map(|value| format!(" / context {value}"))
                            .unwrap_or_default()
                    ),
                    Vec::new(),
                ),
                WorkflowStep::Parallel { items, .. } => (
                    format!("Run {} branches concurrently", items.len()),
                    build_plan_nodes(items, &format!("{node_id}.items")),
                ),
                WorkflowStep::Pipeline { items, .. } => (
                    format!("Run {} pipeline items in order", items.len()),
                    build_plan_nodes(items, &format!("{node_id}.items")),
                ),
                WorkflowStep::Budget { total, .. } => (
                    total
                        .map(|value| format!("Set token budget to {value}"))
                        .unwrap_or_else(|| "Remove the workflow token limit".to_string()),
                    Vec::new(),
                ),
            };
            WorkflowDryRunNode {
                node_id,
                kind: step.kind().to_string(),
                label: step.label(),
                preview,
                children,
            }
        })
        .collect()
}

pub(super) fn node_id(step: &WorkflowStep, generated: &str) -> String {
    step.explicit_id()
        .map(str::to_string)
        .unwrap_or_else(|| generated.to_string())
}

pub(super) fn find_step(
    steps: &[WorkflowStep],
    wanted: &str,
) -> Option<(WorkflowStep, Option<String>)> {
    find_step_inner(steps, wanted, "steps", None)
}

fn find_step_inner(
    steps: &[WorkflowStep],
    wanted: &str,
    path: &str,
    phase: Option<String>,
) -> Option<(WorkflowStep, Option<String>)> {
    for (index, step) in steps.iter().enumerate() {
        let generated = format!("{path}[{index}]");
        let id = node_id(step, &generated);
        if id == wanted {
            return Some((step.clone(), phase));
        }
        match step {
            WorkflowStep::Phase { name, steps, .. } => {
                if let Some(found) =
                    find_step_inner(steps, wanted, &format!("{id}.steps"), Some(name.clone()))
                {
                    return Some(found);
                }
            }
            WorkflowStep::Parallel { items, .. } | WorkflowStep::Pipeline { items, .. } => {
                if let Some(found) =
                    find_step_inner(items, wanted, &format!("{id}.items"), phase.clone())
                {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

pub(super) fn count_steps(steps: &[WorkflowStep]) -> usize {
    steps
        .iter()
        .map(|step| match step {
            WorkflowStep::Log { .. } | WorkflowStep::Agent { .. } | WorkflowStep::Budget { .. } => {
                1
            }
            WorkflowStep::Phase { steps, .. } => 1 + count_steps(steps),
            WorkflowStep::Parallel { items, .. } | WorkflowStep::Pipeline { items, .. } => {
                1 + count_steps(items)
            }
        })
        .sum()
}

pub(super) fn builtin_templates() -> Vec<WorkflowTemplateInfo> {
    vec![
        WorkflowTemplateInfo {
            id: "review-and-implement".to_string(),
            name: "Review and implement".to_string(),
            description: "Inspect a target, implement the change, then run a focused review."
                .to_string(),
            definition: json!({
                "schema_version": 1,
                "name": "Review {{target}}",
                "description": "Review and improve {{target}} with a bounded agent pipeline.",
                "inputs": [
                    { "key": "target", "label": "Target", "kind": "text", "required": true, "placeholder": "src/feature" },
                    { "key": "goal", "label": "Goal", "kind": "textarea", "required": true },
                    { "key": "review_depth", "label": "Review depth", "kind": "select", "default": "focused", "options": ["focused", "thorough"] }
                ],
                "steps": [
                    { "type": "budget", "id": "budget", "total": 32000 },
                    { "type": "phase", "id": "inspect", "name": "Inspect", "steps": [
                        { "type": "agent", "id": "reader", "label": "Reader", "context_mode": "project", "prompt": "Inspect {{target}} for this goal: {{goal}}" }
                    ]},
                    { "type": "phase", "id": "implement", "name": "Implement", "steps": [
                        { "type": "agent", "id": "builder", "label": "Builder", "context_mode": "full", "prompt": "Implement this goal in {{target}}: {{goal}}. Review depth: {{review_depth}}." }
                    ]},
                    { "type": "agent", "id": "reviewer", "label": "Reviewer", "context_mode": "project", "prompt": "Review the implementation for {{goal}} in {{target}}. Be {{review_depth}}." }
                ]
            }),
        },
        WorkflowTemplateInfo {
            id: "parallel-research".to_string(),
            name: "Parallel research".to_string(),
            description: "Run independent readers in parallel and finish with a synthesis agent."
                .to_string(),
            definition: json!({
                "schema_version": 1,
                "name": "Research {{topic}}",
                "inputs": [
                    { "key": "topic", "label": "Topic", "kind": "text", "required": true },
                    { "key": "constraints", "label": "Constraints", "kind": "textarea", "default": "Use repository evidence." }
                ],
                "steps": [
                    { "type": "parallel", "id": "research", "items": [
                        { "type": "agent", "id": "architecture", "label": "Architecture", "context_mode": "project", "prompt": "Research architecture for {{topic}}. {{constraints}}" },
                        { "type": "agent", "id": "tests", "label": "Tests", "context_mode": "project", "prompt": "Research tests and risks for {{topic}}. {{constraints}}" }
                    ]},
                    { "type": "agent", "id": "synthesis", "label": "Synthesis", "context_mode": "project", "prompt": "Synthesize repository findings for {{topic}}. {{constraints}}" }
                ]
            }),
        },
    ]
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn error(path: impl Into<String>, message: impl Into<String>) -> WorkflowValidationIssue {
    WorkflowValidationIssue {
        level: WorkflowValidationLevel::Error,
        path: path.into(),
        message: message.into(),
    }
}

fn warning(path: impl Into<String>, message: impl Into<String>) -> WorkflowValidationIssue {
    WorkflowValidationIssue {
        level: WorkflowValidationLevel::Warning,
        path: path.into(),
        message: message.into(),
    }
}

fn cap_chars(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_string()
    } else {
        format!("{}...", value.chars().take(max).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> WorkflowFile {
        serde_json::from_str(raw).expect("workflow should parse")
    }

    #[test]
    fn validates_inputs_and_renders_scalar_templates() {
        let workflow = parse(
            r#"{
              "name": "Build {{target}}",
              "inputs": [
                {"key":"target","label":"Target","required":true},
                {"key":"count","label":"Count","kind":"number","default":2,"min":1,"max":3},
                {"key":"enabled","label":"Enabled","kind":"boolean","default":true}
              ],
              "steps": [{"type":"log","id":"start","message":"{{target}}/{{count}}/{{enabled}}"}]
            }"#,
        );
        let inputs = BTreeMap::from([("target".to_string(), json!("src"))]);
        let report = validate_workflow(&workflow, &inputs);
        assert!(report.valid, "{:?}", report.issues);
        let rendered = render_workflow(&workflow, &report.normalized_inputs).unwrap();
        match &rendered.steps[0] {
            WorkflowStep::Log { message, .. } => assert_eq!(message, "src/2/true"),
            _ => panic!("expected log"),
        }
    }

    #[test]
    fn rejects_duplicate_nodes_and_invalid_parallel_shape() {
        let workflow = parse(
            r#"{
              "steps": [
                {"type":"log","id":"same","message":"one"},
                {"type":"parallel","id":"same","items":[]}
              ]
            }"#,
        );
        let report = validate_workflow(&workflow, &BTreeMap::new());
        assert!(!report.valid);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.message.contains("duplicate")));
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.message.contains("at least one")));
    }

    #[test]
    fn dry_run_is_non_mutating_and_exposes_stable_node_ids() {
        let workflow = parse(
            r#"{
              "name":"Demo",
              "steps":[{"type":"phase","id":"phase-a","name":"A","steps":[
                {"type":"agent","id":"agent-a","prompt":"Inspect"}
              ]}]
            }"#,
        );
        let dry = dry_run(&workflow, "demo", &BTreeMap::new());
        assert!(dry.valid);
        assert_eq!(dry.steps_total, 2);
        assert_eq!(dry.nodes[0].node_id, "phase-a");
        assert_eq!(dry.nodes[0].children[0].node_id, "agent-a");
        assert!(find_step(&workflow.steps, "agent-a").is_some());
    }

    #[test]
    fn reports_missing_template_value_before_execution() {
        let workflow = parse(r#"{"steps":[{"type":"agent","prompt":"Do {{missing}}"}]}"#);
        let report = validate_workflow(&workflow, &BTreeMap::new());
        assert!(!report.valid);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.message.contains("has no value")));
    }
}
