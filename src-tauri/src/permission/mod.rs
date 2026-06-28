//! 组件 7：权限门。auto 直接放行；confirm 类弹前端确认对话框，等用户裁决。
//! 确保有副作用的操作在执行前获得用户许可。
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

use crate::store;
use crate::tools::{PermissionEffect, PermissionPolicy, PermissionScope, ToolRisk};

static SEQ: AtomicU64 = AtomicU64::new(1);

fn next_id() -> String {
    format!("confirm_{}", SEQ.fetch_add(1, Ordering::Relaxed))
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDecisionSource {
    ToolDefault,
    UserOverride,
    UnknownTool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PermissionDecision {
    pub effect: PermissionEffect,
    pub scope: PermissionScope,
    pub reason: String,
    pub source: PermissionDecisionSource,
}

impl PermissionDecision {
    pub fn from_policy(policy: PermissionPolicy) -> Self {
        PermissionDecision {
            effect: policy.effect,
            scope: policy.scope,
            reason: policy.reason.to_string(),
            source: PermissionDecisionSource::ToolDefault,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PermissionRule {
    pub tool: String,
    pub effect: PermissionEffect,
    pub scope: PermissionScope,
    pub reason: String,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PermissionAuditEntry {
    pub timestamp: u64,
    pub tool: String,
    pub effect: PermissionEffect,
    pub scope: PermissionScope,
    pub source: PermissionDecisionSource,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PermissionRuleView {
    pub tool: String,
    pub effect: PermissionEffect,
    pub scope: PermissionScope,
    pub reason: String,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct PermissionPanelState {
    pub rules: Vec<PermissionRuleView>,
    pub audit: Vec<PermissionAuditEntry>,
}

#[derive(Clone, Debug)]
pub struct PermissionResponse {
    pub allow: bool,
    pub scope: PermissionScope,
}

impl PermissionResponse {
    pub fn deny_once() -> Self {
        PermissionResponse {
            allow: false,
            scope: PermissionScope::Once,
        }
    }
}

pub struct PermissionRequest<'a> {
    pub tool: &'a str,
    pub args_pretty: &'a str,
    pub description: &'a str,
    pub risk: ToolRisk,
    pub decision: PermissionDecision,
    pub summary: String,
    pub preview: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PermissionPromptPayload<'a> {
    pub id: &'a str,
    pub tool: &'a str,
    pub args: &'a str,
    pub description: &'a str,
    pub risk: ToolRisk,
    pub effect: PermissionEffect,
    pub scope: PermissionScope,
    pub reason: &'a str,
    pub summary: String,
    pub preview: Option<&'a str>,
}

pub fn decide(
    state: &crate::AppState,
    tool: &str,
    default_policy: PermissionPolicy,
) -> PermissionDecision {
    if let Some(rule) = state
        .session_permission_rules
        .lock()
        .unwrap()
        .get(tool)
        .cloned()
    {
        return decision_from_rule(rule);
    }

    let data_dir = state.data_dir.lock().unwrap().clone();
    if let Some(rule) = load_project_rules(&data_dir).remove(tool) {
        return decision_from_rule(rule);
    }

    PermissionDecision::from_policy(default_policy)
}

pub fn remember_response(
    state: &crate::AppState,
    tool: &str,
    response: &PermissionResponse,
) -> Result<(), String> {
    if response.scope == PermissionScope::Once {
        return Ok(());
    }

    let rule = PermissionRule {
        tool: tool.to_string(),
        effect: if response.allow {
            PermissionEffect::Allow
        } else {
            PermissionEffect::Deny
        },
        scope: response.scope,
        reason: "用户在确认弹窗中选择记住此决策。".to_string(),
        updated_at: store::now_millis(),
    };

    match response.scope {
        PermissionScope::Once => Ok(()),
        PermissionScope::Session => {
            state
                .session_permission_rules
                .lock()
                .unwrap()
                .insert(tool.to_string(), rule);
            Ok(())
        }
        PermissionScope::Project => {
            let data_dir = state.data_dir.lock().unwrap().clone();
            let mut rules = load_project_rules(&data_dir);
            rules.insert(tool.to_string(), rule);
            save_project_rules(&data_dir, &rules)
        }
    }
}

pub fn audit(state: &crate::AppState, tool: &str, decision: &PermissionDecision) {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let entry = PermissionAuditEntry {
        timestamp: store::now_millis(),
        tool: tool.to_string(),
        effect: decision.effect,
        scope: decision.scope,
        source: decision.source.clone(),
        reason: decision.reason.clone(),
    };
    let _ = append_audit(&data_dir, &entry);
}

/// 向前端发起一次确认请求并 await 结果。
/// 机制：生成唯一 id → 存入 pending map 的 oneshot 发送端 → emit 事件给前端 →
/// 前端弹窗 → 用户点击后 invoke `respond_confirm(id, allow, scope)` → 命令侧取出 sender 回填 →
/// 这里的 rx 收到裁决。超时（5 分钟）按拒绝处理。
pub async fn confirm(
    app: &AppHandle,
    state: &crate::AppState,
    req: PermissionRequest<'_>,
) -> PermissionResponse {
    let id = next_id();
    let (tx, rx) = oneshot::channel::<PermissionResponse>();
    state
        .pending_confirms
        .lock()
        .unwrap()
        .insert(id.clone(), tx);

    let payload = PermissionPromptPayload {
        id: &id,
        tool: req.tool,
        args: req.args_pretty,
        description: req.description,
        risk: req.risk,
        effect: req.decision.effect,
        scope: req.decision.scope,
        reason: &req.decision.reason,
        summary: req.summary,
        preview: req.preview.as_deref(),
    };

    let _ = app.emit("tool-confirm-request", payload);

    match tokio::time::timeout(Duration::from_secs(300), rx).await {
        Ok(Ok(v)) => v,
        _ => {
            // 超时或通道异常：清理并按拒绝处理
            state.pending_confirms.lock().unwrap().remove(&id);
            PermissionResponse::deny_once()
        }
    }
}

pub fn panel_state(state: &crate::AppState) -> PermissionPanelState {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let mut rules = Vec::new();
    for rule in state.session_permission_rules.lock().unwrap().values() {
        rules.push(rule_view(rule));
    }
    for rule in load_project_rules(&data_dir).values() {
        rules.push(rule_view(rule));
    }
    rules.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.tool.cmp(&b.tool))
    });
    PermissionPanelState {
        rules,
        audit: load_recent_audit(&data_dir, 80),
    }
}

pub fn reset_rule(
    state: &crate::AppState,
    scope: PermissionScope,
    tool: &str,
) -> Result<PermissionPanelState, String> {
    match scope {
        PermissionScope::Once => {}
        PermissionScope::Session => {
            state.session_permission_rules.lock().unwrap().remove(tool);
        }
        PermissionScope::Project => {
            let data_dir = state.data_dir.lock().unwrap().clone();
            let mut rules = load_project_rules(&data_dir);
            rules.remove(tool);
            save_project_rules(&data_dir, &rules)?;
        }
    }
    Ok(panel_state(state))
}

fn rule_view(rule: &PermissionRule) -> PermissionRuleView {
    PermissionRuleView {
        tool: rule.tool.clone(),
        effect: rule.effect,
        scope: rule.scope,
        reason: rule.reason.clone(),
        updated_at: rule.updated_at,
    }
}

fn decision_from_rule(rule: PermissionRule) -> PermissionDecision {
    PermissionDecision {
        effect: rule.effect,
        scope: rule.scope,
        reason: rule.reason,
        source: PermissionDecisionSource::UserOverride,
    }
}

fn load_project_rules(dir: &Path) -> HashMap<String, PermissionRule> {
    let p = dir.join("permissions.json");
    std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str::<HashMap<String, PermissionRule>>(&s).ok())
        .unwrap_or_default()
}

fn save_project_rules(dir: &Path, rules: &HashMap<String, PermissionRule>) -> Result<(), String> {
    let p = dir.join("permissions.json");
    let json = serde_json::to_string_pretty(rules).map_err(|e| e.to_string())?;
    std::fs::write(&p, json).map_err(|e| e.to_string())
}

fn load_recent_audit(dir: &Path, limit: usize) -> Vec<PermissionAuditEntry> {
    let p = dir.join("permission_audit.jsonl");
    let mut entries = std::fs::read_to_string(&p)
        .ok()
        .map(|text| {
            text.lines()
                .filter_map(|line| serde_json::from_str::<PermissionAuditEntry>(line).ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    entries.truncate(limit);
    entries
}

fn append_audit(dir: &Path, entry: &PermissionAuditEntry) -> Result<(), String> {
    let p = dir.join("permission_audit.jsonl");
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)
        .map_err(|e| e.to_string())?;
    let json = serde_json::to_string(entry).map_err(|e| e.to_string())?;
    writeln!(f, "{json}").map_err(|e| e.to_string())
}
