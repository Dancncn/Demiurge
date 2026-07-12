//! 组件 7：权限门。auto 直接放行；confirm 类弹前端确认对话框，等用户裁决。
//! 确保有副作用的操作在执行前获得用户许可。
use std::collections::{BTreeMap, HashMap};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

use crate::store;
use crate::store::PermissionMode;
use crate::tools::{PermissionEffect, PermissionPolicy, PermissionScope, ToolRisk};

static SEQ: AtomicU64 = AtomicU64::new(1);
static FILE_SEQ: AtomicU64 = AtomicU64::new(1);

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
    /// 角色卡 runtime.permissions 偏好覆盖（介于 user 规则与 tool 默认之间）。
    CardOverlay,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PermissionDecision {
    pub effect: PermissionEffect,
    pub scope: PermissionScope,
    pub reason: String,
    pub source: PermissionDecisionSource,
    pub mode: Option<PermissionMode>,
}

impl PermissionDecision {
    pub fn from_policy(policy: PermissionPolicy) -> Self {
        PermissionDecision {
            effect: policy.effect,
            scope: policy.scope,
            reason: policy.reason.to_string(),
            source: PermissionDecisionSource::ToolDefault,
            mode: None,
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

/// Immutable identity captured before a permission decision is evaluated.
///
/// The same context must be reused for decision lookup, auditing, and a
/// remembered confirmation response. This prevents a session switch while a
/// confirmation is open from redirecting the remembered rule into another
/// session or project bucket.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PermissionContext {
    pub session_id: Option<String>,
    pub workspace_identity: Option<String>,
    /// Any failure to establish a trustworthy session/workspace pairing.
    /// Decisions using this context must fail closed instead of falling back
    /// to broader User or tool-default Allow rules.
    pub boundary_error: Option<String>,
    /// Card policy is frozen with the context so switching packs while a
    /// confirmation is open cannot change whether/how the response is stored.
    card_preferences: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PermissionAuditEntry {
    pub timestamp: u64,
    pub tool: String,
    pub effect: PermissionEffect,
    pub scope: PermissionScope,
    pub source: PermissionDecisionSource,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<PermissionMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_identity: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PermissionRuleView {
    pub tool: String,
    pub effect: PermissionEffect,
    pub scope: PermissionScope,
    pub reason: String,
    pub updated_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_identity: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PermissionToolView {
    pub tool: String,
    pub description: String,
    pub risk: ToolRisk,
    pub default_effect: PermissionEffect,
    pub default_scope: PermissionScope,
    pub default_reason: String,
    /// 角色卡对该工具声明的偏好（如 "allow"/"deny"/"ask_once"/"ask_every_time"），None 表示无覆盖。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_preference: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PermissionPanelState {
    pub rules: Vec<PermissionRuleView>,
    pub audit: Vec<PermissionAuditEntry>,
    pub tools: Vec<PermissionToolView>,
    pub notices: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PermissionRuleInput {
    pub tool: String,
    pub effect: PermissionEffect,
    pub scope: PermissionScope,
    pub reason: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub workspace_identity: Option<String>,
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
    pub session_id: &'a str,
    pub tool: &'a str,
    pub args_pretty: &'a str,
    pub description: &'a str,
    pub risk: ToolRisk,
    pub decision: PermissionDecision,
    pub summary: String,
    pub preview: Option<String>,
    pub affected_paths: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PermissionPromptPayload<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    pub tool: &'a str,
    pub args: &'a str,
    pub description: &'a str,
    pub risk: ToolRisk,
    pub effect: PermissionEffect,
    pub scope: PermissionScope,
    pub source: PermissionDecisionSource,
    pub reason: &'a str,
    pub summary: String,
    pub preview: Option<&'a str>,
    pub affected_paths: Vec<String>,
}

pub fn active_context(state: &crate::AppState) -> PermissionContext {
    let session_id = state.sessions.lock().unwrap().active.clone();
    context_for_session(state, &session_id)
}

/// Captures an immutable authorization context for a specific session.
/// Runner paths pass the turn-owned id from `begin_turn`; UI paths use
/// `active_context`. A mismatched or unresolvable workspace is represented as
/// an explicit boundary error and must never degrade into a broader rule.
pub fn context_for_session(state: &crate::AppState, session_id: &str) -> PermissionContext {
    let configured_workspace = {
        let sessions = state.sessions.lock().unwrap();
        sessions
            .get(session_id)
            .map(|session| session.workspace_path.clone())
    };
    let card_preferences = load_card_prefs(state);

    let Some(configured_workspace) = configured_workspace else {
        return PermissionContext {
            session_id: (!session_id.is_empty()).then(|| session_id.to_string()),
            workspace_identity: None,
            boundary_error: Some("权限上下文对应的会话不存在，已拒绝自动授权".to_string()),
            card_preferences,
        };
    };

    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let sandbox_identity = canonical_workspace_identity(&sandbox);
    let configured_identity = if configured_workspace.trim().is_empty() {
        None
    } else {
        Some(canonical_workspace_identity(Path::new(
            &configured_workspace,
        )))
    };

    let (workspace_identity, boundary_error) = match (sandbox_identity, configured_identity) {
        (Ok(sandbox), None) => (Some(sandbox), None),
        (Ok(sandbox), Some(Ok(configured))) if sandbox == configured => (Some(configured), None),
        (Ok(_), Some(Ok(_))) => (
            None,
            Some("会话项目与当前工具工作区不一致，已拒绝自动授权".to_string()),
        ),
        (Err(error), _) => (
            None,
            Some(format!("无法验证当前工具工作区，已拒绝自动授权：{error}")),
        ),
        (_, Some(Err(error))) => (
            None,
            Some(format!("无法验证会话项目工作区，已拒绝自动授权：{error}")),
        ),
    };

    PermissionContext {
        session_id: Some(session_id.to_string()),
        workspace_identity,
        boundary_error,
        card_preferences,
    }
}

pub fn decide(
    state: &crate::AppState,
    context: &PermissionContext,
    tool: &str,
    default_policy: PermissionPolicy,
) -> PermissionDecision {
    if let Some(error) = context.boundary_error.as_deref() {
        return boundary_denial(error);
    }

    if let Some(session_id) = context.session_id.as_deref() {
        if let Some(rule) = state
            .session_permission_rules
            .lock()
            .unwrap()
            .get(session_id)
            .and_then(|rules| rules.get(tool))
            .cloned()
        {
            return decision_from_rule(rule);
        }
    }

    let data_dir = state.data_dir.lock().unwrap().clone();
    let _store_guard = state.permission_store_lock.lock().unwrap();
    if let Some(workspace_identity) = context.workspace_identity.as_deref() {
        match load_project_rules(&data_dir, workspace_identity) {
            Ok(mut rules) => {
                if let Some(rule) = rules.remove(tool) {
                    return decision_from_rule(rule);
                }
            }
            Err(error) => {
                return boundary_denial(&format!("项目权限存储不可用：{error}"));
            }
        }
    }

    match load_user_rules(&data_dir) {
        Ok(mut rules) => {
            if let Some(rule) = rules.remove(tool) {
                return decision_from_rule(rule);
            }
        }
        Err(error) => return boundary_denial(&format!("用户权限存储不可用：{error}")),
    }
    drop(_store_guard);

    // 角色卡 runtime.permissions 偏好覆盖：介于 user 规则与 tool 默认之间。
    // 显式 user/project/session 规则已在上面命中并返回，这里只在没有用户规则时生效。
    if let Some(decision) = card_overlay(context, tool, default_policy.effect) {
        return decision;
    }

    PermissionDecision::from_policy(default_policy)
}

pub fn decide_for_mode(
    state: &crate::AppState,
    context: &PermissionContext,
    tool: &str,
    default_policy: PermissionPolicy,
    risk: ToolRisk,
) -> PermissionDecision {
    let mode = state.settings.lock().unwrap().permission_mode;
    let plan = state.plan_state.lock().unwrap().clone();
    if let Some(error) = context.boundary_error.as_deref() {
        let mut decision = boundary_denial(error);
        decision.mode = Some(mode);
        return decision;
    }
    let mut decision = match mode {
        PermissionMode::Default => decide(state, context, tool, default_policy),
        PermissionMode::Auto => {
            if risk == ToolRisk::ReadOnly {
                PermissionDecision {
                    effect: PermissionEffect::Allow,
                    scope: PermissionScope::Once,
                    reason: "Auto 模式自动允许只读工具。".to_string(),
                    source: PermissionDecisionSource::ToolDefault,
                    mode: None,
                }
            } else {
                decide(state, context, tool, default_policy)
            }
        }
        PermissionMode::Bypass => PermissionDecision {
            effect: PermissionEffect::Allow,
            scope: PermissionScope::Once,
            reason: "Bypass 模式已开启：跳过确认并允许工具执行。".to_string(),
            source: PermissionDecisionSource::UserOverride,
            mode: None,
        },
        PermissionMode::Plan => {
            if plan.approved {
                decide(state, context, tool, default_policy)
            } else if risk == ToolRisk::ReadOnly {
                PermissionDecision {
                    effect: PermissionEffect::Allow,
                    scope: PermissionScope::Once,
                    reason: "Plan Mode 未批准前允许只读探索。".to_string(),
                    source: PermissionDecisionSource::ToolDefault,
                    mode: None,
                }
            } else if tool == "write_plan" {
                PermissionDecision {
                    effect: PermissionEffect::Allow,
                    scope: PermissionScope::Once,
                    reason: "Plan Mode 允许写入受限计划文件。".to_string(),
                    source: PermissionDecisionSource::ToolDefault,
                    mode: None,
                }
            } else {
                PermissionDecision {
                    effect: PermissionEffect::Deny,
                    scope: PermissionScope::Once,
                    reason: "Plan Mode 未批准前阻止写入、shell、外部发布和系统能力。".to_string(),
                    source: PermissionDecisionSource::ToolDefault,
                    mode: None,
                }
            }
        }
    };
    decision.mode = Some(mode);
    decision
}

/// 角色卡 permission 偏好类型（从 runtime.permissions 的字符串值解析）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardPreference {
    Deny,
    AskOnce,
    AskEveryTime,
}

fn parse_card_preference(value: &str) -> Option<CardPreference> {
    match value.trim().to_ascii_lowercase().as_str() {
        "deny" | "always_deny" => Some(CardPreference::Deny),
        "ask_once" | "ask" => Some(CardPreference::AskOnce),
        "ask_every_time" | "ask_everytime" => Some(CardPreference::AskEveryTime),
        // `allow` / `always_allow` 也不在运行时放行：角色包只能收紧本地默认权限。
        // manifest 校验会拒绝这些值；这里再 fail closed，防止绕过校验的调用路径。
        _ => None,
    }
}

/// 角色包只能保持或收紧工具默认权限，不能把 Ask/Deny 降为 Allow/Ask。
fn card_preference_effect(
    pref: CardPreference,
    default_effect: PermissionEffect,
) -> Option<PermissionEffect> {
    match pref {
        CardPreference::Deny => Some(PermissionEffect::Deny),
        CardPreference::AskOnce | CardPreference::AskEveryTime
            if default_effect != PermissionEffect::Deny =>
        {
            Some(PermissionEffect::Ask)
        }
        CardPreference::AskOnce | CardPreference::AskEveryTime => None,
    }
}

fn load_card_prefs(state: &crate::AppState) -> std::collections::BTreeMap<String, String> {
    let packs_dir = state.packs_dir.lock().unwrap().clone();
    let pack_id = state.settings.lock().unwrap().current_pack.clone();
    crate::pack::permission_preferences(&packs_dir, &pack_id)
}

/// 把角色卡偏好解析成可执行的 PermissionDecision。None 表示无偏好或为 default。
fn card_overlay(
    context: &PermissionContext,
    tool: &str,
    default_effect: PermissionEffect,
) -> Option<PermissionDecision> {
    let raw = context.card_preferences.get(tool)?;
    let pref = parse_card_preference(raw)?;
    let effect = card_preference_effect(pref, default_effect)?;
    let reason = format!("角色卡权限偏好：{raw}");
    Some(PermissionDecision {
        effect,
        scope: PermissionScope::Once,
        reason,
        source: PermissionDecisionSource::CardOverlay,
        mode: None,
    })
}

fn boundary_denial(reason: &str) -> PermissionDecision {
    PermissionDecision {
        effect: PermissionEffect::Deny,
        scope: PermissionScope::Once,
        reason: reason.to_string(),
        source: PermissionDecisionSource::ToolDefault,
        mode: None,
    }
}

pub fn remember_response(
    state: &crate::AppState,
    context: &PermissionContext,
    tool: &str,
    response: &PermissionResponse,
) -> Result<PermissionScope, String> {
    if let Some(error) = context.boundary_error.as_deref() {
        return Err(format!("权限上下文不可用，无法记住规则：{error}"));
    }
    let card_pref = context
        .card_preferences
        .get(tool)
        .and_then(|v| parse_card_preference(v));

    // 角色卡 ask_every_time 偏好：禁止持久化 remember，确保每次都弹确认。
    if card_pref == Some(CardPreference::AskEveryTime) {
        return Ok(PermissionScope::Once);
    }

    // 角色卡 ask_once 偏好：用户选 Once 时自动升级为 Session，
    // 保证"每会话只问一次"语义（不必让用户手动挑 Session scope）。
    let scope =
        if card_pref == Some(CardPreference::AskOnce) && response.scope == PermissionScope::Once {
            PermissionScope::Session
        } else {
            response.scope
        };

    if scope == PermissionScope::Once {
        return Ok(PermissionScope::Once);
    }

    let rule = PermissionRule {
        tool: tool.to_string(),
        effect: if response.allow {
            PermissionEffect::Allow
        } else {
            PermissionEffect::Deny
        },
        scope,
        reason: "用户在确认弹窗中选择记住此决策。".to_string(),
        updated_at: store::now_millis(),
    };

    match scope {
        PermissionScope::Once => Ok(PermissionScope::Once),
        PermissionScope::Session => {
            let session_id = context.session_id.as_ref().ok_or_else(|| {
                "Cannot remember a Session permission without an active session.".to_string()
            })?;
            insert_session_rule(state, session_id, tool, rule)?;
            Ok(PermissionScope::Session)
        }
        PermissionScope::Project => {
            let workspace_identity = context.workspace_identity.as_deref().ok_or_else(|| {
                "Cannot remember a Project permission without a canonical workspace.".to_string()
            })?;
            let data_dir = state.data_dir.lock().unwrap().clone();
            let _store_guard = state.permission_store_lock.lock().unwrap();
            let mut rules = load_project_rules(&data_dir, workspace_identity)?;
            rules.insert(tool.to_string(), rule);
            save_project_rules(&data_dir, workspace_identity, rules)?;
            Ok(PermissionScope::Project)
        }
        PermissionScope::User => {
            let data_dir = state.data_dir.lock().unwrap().clone();
            let _store_guard = state.permission_store_lock.lock().unwrap();
            let mut rules = load_user_rules(&data_dir)?;
            rules.insert(tool.to_string(), rule);
            save_user_rules(&data_dir, &rules)?;
            Ok(PermissionScope::User)
        }
    }
}

pub fn audit(
    state: &crate::AppState,
    context: &PermissionContext,
    tool: &str,
    decision: &PermissionDecision,
) {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let entry = PermissionAuditEntry {
        timestamp: store::now_millis(),
        tool: tool.to_string(),
        effect: decision.effect,
        scope: decision.scope,
        source: decision.source.clone(),
        reason: decision.reason.clone(),
        mode: decision.mode,
        session_id: context.session_id.clone(),
        workspace_identity: context.workspace_identity.clone(),
    };
    let _store_guard = state.permission_store_lock.lock().unwrap();
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
        session_id: req.session_id,
        tool: req.tool,
        args: req.args_pretty,
        description: req.description,
        risk: req.risk,
        effect: req.decision.effect,
        scope: req.decision.scope,
        source: req.decision.source.clone(),
        reason: &req.decision.reason,
        summary: req.summary,
        preview: req.preview.as_deref(),
        affected_paths: req.affected_paths,
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
    let context = active_context(state);
    let data_dir = state.data_dir.lock().unwrap().clone();
    let mut rules = Vec::new();
    let mut notices = Vec::new();
    if let Some(error) = context.boundary_error.as_ref() {
        notices.push(error.clone());
    }
    if let Some(session_id) = context.session_id.as_deref() {
        if let Some(session_rules) = state
            .session_permission_rules
            .lock()
            .unwrap()
            .get(session_id)
        {
            for rule in session_rules.values() {
                rules.push(rule_view(rule, Some(session_id), None));
            }
        }
    }
    let (audit, persisted_notices) = {
        let _store_guard = state.permission_store_lock.lock().unwrap();
        if let Some(workspace_identity) = context.workspace_identity.as_deref() {
            match load_project_rules(&data_dir, workspace_identity) {
                Ok(project_rules) => {
                    for rule in project_rules.values() {
                        rules.push(rule_view(rule, None, Some(workspace_identity)));
                    }
                }
                Err(error) => notices.push(format!("项目权限存储不可用：{error}")),
            }
        }
        match load_user_rules(&data_dir) {
            Ok(user_rules) => {
                for rule in user_rules.values() {
                    rules.push(rule_view(rule, None, None));
                }
            }
            Err(error) => notices.push(format!("用户权限存储不可用：{error}")),
        }
        let mut persisted_notices = Vec::new();
        if data_dir.join("permissions.json").exists() {
            persisted_notices.push(
                "检测到旧版无项目身份的 permissions.json；为防止跨项目授权扩散，该文件不会自动应用，请在当前项目重新确认 Project 规则。"
                    .to_string(),
            );
        }
        (load_recent_audit(&data_dir, 80), persisted_notices)
    };
    notices.extend(persisted_notices);
    rules.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.tool.cmp(&b.tool))
    });
    PermissionPanelState {
        rules,
        audit,
        tools: tool_views(&context.card_preferences),
        notices,
    }
}

pub fn reset_rule(
    state: &crate::AppState,
    scope: PermissionScope,
    tool: &str,
    expected_session_id: Option<&str>,
    expected_workspace_identity: Option<&str>,
) -> Result<PermissionPanelState, String> {
    let context = active_context(state);
    if let Some(error) = context.boundary_error.as_deref() {
        return Err(format!("权限上下文已失效，请刷新后重试：{error}"));
    }
    match scope {
        PermissionScope::Once => {}
        PermissionScope::Session => {
            let session_id = expected_session_id.ok_or_else(|| {
                "Session permission reset requires the rule's session identity.".to_string()
            })?;
            ensure_expected_session(&context, session_id)?;
            let mut buckets = state.session_permission_rules.lock().unwrap();
            if let Some(rules) = buckets.get_mut(session_id) {
                rules.remove(tool);
                if rules.is_empty() {
                    buckets.remove(session_id);
                }
            }
        }
        PermissionScope::Project => {
            let workspace_identity = expected_workspace_identity.ok_or_else(|| {
                "Project permission reset requires the rule's workspace identity.".to_string()
            })?;
            ensure_expected_workspace(&context, workspace_identity)?;
            let data_dir = state.data_dir.lock().unwrap().clone();
            let _store_guard = state.permission_store_lock.lock().unwrap();
            let mut rules = load_project_rules(&data_dir, workspace_identity)?;
            rules.remove(tool);
            save_project_rules(&data_dir, workspace_identity, rules)?;
        }
        PermissionScope::User => {
            let data_dir = state.data_dir.lock().unwrap().clone();
            let _store_guard = state.permission_store_lock.lock().unwrap();
            let mut rules = load_user_rules(&data_dir)?;
            rules.remove(tool);
            save_user_rules(&data_dir, &rules)?;
        }
    }
    Ok(panel_state(state))
}

pub fn upsert_rule(
    state: &crate::AppState,
    input: PermissionRuleInput,
) -> Result<PermissionPanelState, String> {
    let context = active_context(state);
    if let Some(error) = context.boundary_error.as_deref() {
        return Err(format!("权限上下文已失效，请刷新后重试：{error}"));
    }
    if input.scope == PermissionScope::Once {
        return Err("Once scope is only valid for a single confirmation response.".to_string());
    }
    if crate::tools::definition_for_state(state, &input.tool).is_none() {
        return Err(format!("Unknown tool `{}`.", input.tool));
    }
    let reason = if input.reason.trim().is_empty() {
        "User-managed permission rule.".to_string()
    } else {
        input.reason.trim().to_string()
    };
    let rule = PermissionRule {
        tool: input.tool.clone(),
        effect: input.effect,
        scope: input.scope,
        reason,
        updated_at: store::now_millis(),
    };
    match input.scope {
        PermissionScope::Once => {}
        PermissionScope::Session => {
            let session_id = input
                .session_id
                .as_deref()
                .or(context.session_id.as_deref())
                .ok_or_else(|| {
                    "Cannot save a Session permission without an active session.".to_string()
                })?;
            ensure_expected_session(&context, session_id)?;
            insert_session_rule(state, session_id, &input.tool, rule)?;
        }
        PermissionScope::Project => {
            let workspace_identity = input
                .workspace_identity
                .as_deref()
                .or(context.workspace_identity.as_deref())
                .ok_or_else(|| {
                    "Cannot save a Project permission without a canonical workspace.".to_string()
                })?;
            ensure_expected_workspace(&context, workspace_identity)?;
            let data_dir = state.data_dir.lock().unwrap().clone();
            let _store_guard = state.permission_store_lock.lock().unwrap();
            let mut rules = load_project_rules(&data_dir, workspace_identity)?;
            rules.insert(input.tool, rule);
            save_project_rules(&data_dir, workspace_identity, rules)?;
        }
        PermissionScope::User => {
            let data_dir = state.data_dir.lock().unwrap().clone();
            let _store_guard = state.permission_store_lock.lock().unwrap();
            let mut rules = load_user_rules(&data_dir)?;
            rules.insert(input.tool, rule);
            save_user_rules(&data_dir, &rules)?;
        }
    }
    Ok(panel_state(state))
}

/// Inserts a Session rule while holding the session store lock until the rule
/// is visible. Session deletion takes the same locks in this order, so either
/// the rule is inserted before deletion and then cleared, or the missing
/// session is observed and no orphan authorization bucket is created.
fn insert_session_rule(
    state: &crate::AppState,
    session_id: &str,
    tool: &str,
    rule: PermissionRule,
) -> Result<(), String> {
    let sessions = state.sessions.lock().unwrap();
    if sessions.get(session_id).is_none() {
        return Err("Cannot save a Session permission for a deleted session.".to_string());
    }
    state
        .session_permission_rules
        .lock()
        .unwrap()
        .entry(session_id.to_string())
        .or_default()
        .insert(tool.to_string(), rule);
    Ok(())
}

fn ensure_expected_session(context: &PermissionContext, expected: &str) -> Result<(), String> {
    match context.session_id.as_deref() {
        Some(current) if current == expected => Ok(()),
        _ => Err("会话权限规则已过期：当前会话与界面显示的规则不一致，请刷新后重试".to_string()),
    }
}

fn ensure_expected_workspace(context: &PermissionContext, expected: &str) -> Result<(), String> {
    match context.workspace_identity.as_deref() {
        Some(current) if current == expected => Ok(()),
        _ => Err("项目权限规则已过期：当前项目与界面显示的规则不一致，请刷新后重试".to_string()),
    }
}

pub fn clear_session_rules(state: &crate::AppState, session_id: &str) {
    state
        .session_permission_rules
        .lock()
        .unwrap()
        .remove(session_id);
}

fn rule_view(
    rule: &PermissionRule,
    session_id: Option<&str>,
    workspace_identity: Option<&str>,
) -> PermissionRuleView {
    PermissionRuleView {
        tool: rule.tool.clone(),
        effect: rule.effect,
        scope: rule.scope,
        reason: rule.reason.clone(),
        updated_at: rule.updated_at,
        session_id: session_id.map(str::to_string),
        workspace_identity: workspace_identity.map(str::to_string),
    }
}

fn tool_views(card_prefs: &std::collections::BTreeMap<String, String>) -> Vec<PermissionToolView> {
    let mut tools = crate::tools::registry()
        .into_iter()
        .map(|tool| {
            let card_preference = card_prefs.get(tool.name).cloned();
            PermissionToolView {
                tool: tool.name.to_string(),
                description: tool.description.to_string(),
                risk: tool.risk,
                default_effect: tool.permission.effect,
                default_scope: tool.permission.scope,
                default_reason: tool.permission.reason.to_string(),
                card_preference,
            }
        })
        .collect::<Vec<_>>();
    tools.sort_by(|a, b| a.tool.cmp(&b.tool));
    tools
}

fn decision_from_rule(rule: PermissionRule) -> PermissionDecision {
    PermissionDecision {
        effect: rule.effect,
        scope: rule.scope,
        reason: rule.reason,
        source: PermissionDecisionSource::UserOverride,
        mode: None,
    }
}

const PROJECT_PERMISSION_STORE_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct ProjectPermissionStore {
    version: u32,
    #[serde(default)]
    projects: HashMap<String, HashMap<String, PermissionRule>>,
}

impl Default for ProjectPermissionStore {
    fn default() -> Self {
        Self {
            version: PROJECT_PERMISSION_STORE_VERSION,
            projects: HashMap::new(),
        }
    }
}

fn canonical_workspace_identity(path: &Path) -> Result<String, String> {
    if !path.is_dir() {
        return Err(format!(
            "Project workspace does not exist or is not a directory: {}",
            path.display()
        ));
    }
    let canonical = std::fs::canonicalize(path).map_err(|error| {
        format!(
            "Failed to resolve canonical project workspace `{}`: {error}",
            path.display()
        )
    })?;

    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;

        let text = canonical.to_string_lossy();
        let normalized = if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{rest}")
        } else if let Some(rest) = text.strip_prefix(r"\\?\") {
            rest.to_string()
        } else {
            text.into_owned()
        };
        if canonical.to_str().is_some() {
            return Ok(format!("path:{}", normalized.replace('\\', "/")));
        }

        // Windows paths can contain unpaired UTF-16 code units. Keep the
        // identity injective instead of letting lossy display conversion make
        // two distinct paths share a permission bucket.
        let encoded = canonical
            .as_os_str()
            .encode_wide()
            .map(|unit| format!("{unit:04x}"))
            .collect::<String>();
        Ok(format!("windows-wide:{encoded}"))
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::ffi::OsStrExt;

        if let Some(text) = canonical.to_str() {
            return Ok(format!("path:{text}"));
        }

        // Unix paths are arbitrary byte strings. Hex encoding the exceptional
        // non-UTF-8 case avoids collisions introduced by to_string_lossy().
        let encoded = canonical
            .as_os_str()
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        Ok(format!("unix-bytes:{encoded}"))
    }
}

fn load_project_store(dir: &Path) -> Result<ProjectPermissionStore, String> {
    let path = dir.join("project_permissions.json");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ProjectPermissionStore::default());
        }
        Err(error) => {
            return Err(format!(
                "Failed to read project permission store `{}`: {error}",
                path.display()
            ));
        }
    };
    let store = serde_json::from_str::<ProjectPermissionStore>(&text).map_err(|error| {
        format!(
            "Failed to parse project permission store `{}`: {error}",
            path.display()
        )
    })?;
    if store.version != PROJECT_PERMISSION_STORE_VERSION {
        return Err(format!(
            "Unsupported project permission store version {} in `{}`.",
            store.version,
            path.display()
        ));
    }
    Ok(store)
}

fn load_project_rules(
    dir: &Path,
    workspace_identity: &str,
) -> Result<HashMap<String, PermissionRule>, String> {
    Ok(load_project_store(dir)?
        .projects
        .remove(workspace_identity)
        .unwrap_or_default())
}

fn save_project_rules(
    dir: &Path,
    workspace_identity: &str,
    rules: HashMap<String, PermissionRule>,
) -> Result<(), String> {
    let path = dir.join("project_permissions.json");
    let mut store = load_project_store(dir)?;
    if rules.is_empty() {
        store.projects.remove(workspace_identity);
    } else {
        store.projects.insert(workspace_identity.to_string(), rules);
    }
    let json = serde_json::to_string_pretty(&store).map_err(|error| error.to_string())?;
    atomic_write_file(&path, json.as_bytes())
}

fn load_user_rules(dir: &Path) -> Result<HashMap<String, PermissionRule>, String> {
    let p = dir.join("user_permissions.json");
    load_rules_file(&p)
}

fn save_user_rules(dir: &Path, rules: &HashMap<String, PermissionRule>) -> Result<(), String> {
    let p = dir.join("user_permissions.json");
    save_rules_file(&p, rules)
}

fn load_rules_file(p: &Path) -> Result<HashMap<String, PermissionRule>, String> {
    let text = match std::fs::read_to_string(p) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(error) => {
            return Err(format!(
                "Failed to read permission rules `{}`: {error}",
                p.display()
            ))
        }
    };
    serde_json::from_str(&text).map_err(|error| {
        format!(
            "Failed to parse permission rules `{}`: {error}",
            p.display()
        )
    })
}

fn save_rules_file(p: &Path, rules: &HashMap<String, PermissionRule>) -> Result<(), String> {
    let json = serde_json::to_string_pretty(rules).map_err(|e| e.to_string())?;
    atomic_write_file(p, json.as_bytes())
}

fn atomic_write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("权限文件缺少父目录：{}", path.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("无法创建权限目录 `{}`：{error}", parent.display()))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("permissions");

    let temp_path = loop {
        let seq = FILE_SEQ.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".{name}.{}.{}.tmp", std::process::id(), seq));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut file) => {
                let write_result = (|| -> Result<(), String> {
                    file.write_all(bytes).map_err(|error| error.to_string())?;
                    file.flush().map_err(|error| error.to_string())?;
                    file.sync_all().map_err(|error| error.to_string())?;
                    Ok(())
                })();
                if let Err(error) = write_result {
                    drop(file);
                    let _ = std::fs::remove_file(&candidate);
                    return Err(format!(
                        "无法写入权限临时文件 `{}`：{error}",
                        candidate.display()
                    ));
                }
                drop(file);
                break candidate;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "无法创建权限临时文件 `{}`：{error}",
                    candidate.display()
                ))
            }
        }
    };

    if let Err(error) = std::fs::rename(&temp_path, path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(format!(
            "无法原子替换权限文件 `{}`：{error}",
            path.display()
        ));
    }

    #[cfg(unix)]
    if let Ok(directory) = std::fs::File::open(parent) {
        let _ = directory.sync_all();
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Session, SessionStore};

    fn temp_dir(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "demiurge_permission_{label}_{}",
            crate::store::new_session_id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn session_with(id: &str, workspace: &Path) -> Session {
        let mut session = Session::new();
        session.id = id.to_string();
        session.workspace_path = workspace.to_string_lossy().into_owned();
        session
    }

    fn test_state(data_dir: &Path, workspace: &Path, sessions: Vec<Session>) -> crate::AppState {
        let state = crate::AppState::new(reqwest::Client::new());
        std::fs::create_dir_all(data_dir.join("packs")).unwrap();
        *state.data_dir.lock().unwrap() = data_dir.to_path_buf();
        *state.sandbox_dir.lock().unwrap() = workspace.to_path_buf();
        *state.packs_dir.lock().unwrap() = data_dir.join("packs");
        let active = sessions
            .first()
            .map(|session| session.id.clone())
            .unwrap_or_default();
        *state.sessions.lock().unwrap() = SessionStore { active, sessions };
        state
    }

    fn select_session_for_test(state: &crate::AppState, id: &str, workspace: &Path) {
        {
            let mut sessions = state.sessions.lock().unwrap();
            let session = sessions.get_mut(id).unwrap();
            session.workspace_path = workspace.to_string_lossy().into_owned();
            sessions.active = id.to_string();
        }
        *state.sandbox_dir.lock().unwrap() = workspace.to_path_buf();
    }

    fn test_policy() -> PermissionPolicy {
        PermissionPolicy::ask("test default")
    }

    #[test]
    fn parses_card_preference_strings() {
        // 角色包不能通过 allow / always_allow 放宽本地默认权限。
        assert_eq!(parse_card_preference("allow"), None);
        assert_eq!(parse_card_preference("Always_Allow"), None);
        // deny / always_deny → Deny
        assert_eq!(parse_card_preference("deny"), Some(CardPreference::Deny));
        assert_eq!(
            parse_card_preference("always_deny"),
            Some(CardPreference::Deny)
        );
        // ask_once / ask → AskOnce
        assert_eq!(
            parse_card_preference("ask_once"),
            Some(CardPreference::AskOnce)
        );
        assert_eq!(parse_card_preference("ask"), Some(CardPreference::AskOnce));
        // ask_every_time / ask_everytime → AskEveryTime
        assert_eq!(
            parse_card_preference("ask_every_time"),
            Some(CardPreference::AskEveryTime)
        );
        assert_eq!(
            parse_card_preference("ask_everytime"),
            Some(CardPreference::AskEveryTime)
        );
        // default / 空 / 未知 → None（回落到 tool 默认）
        assert_eq!(parse_card_preference("default"), None);
        assert_eq!(parse_card_preference(""), None);
        assert_eq!(parse_card_preference("unknown_value"), None);
        // 大小写与首尾空白不敏感
        assert_eq!(
            parse_card_preference("  DENY  "),
            Some(CardPreference::Deny)
        );
    }

    #[test]
    fn card_preference_only_preserves_or_tightens_default_effect() {
        assert_eq!(
            card_preference_effect(CardPreference::Deny, PermissionEffect::Allow),
            Some(PermissionEffect::Deny)
        );
        assert_eq!(
            card_preference_effect(CardPreference::Deny, PermissionEffect::Ask),
            Some(PermissionEffect::Deny)
        );
        assert_eq!(
            card_preference_effect(CardPreference::AskOnce, PermissionEffect::Allow),
            Some(PermissionEffect::Ask)
        );
        assert_eq!(
            card_preference_effect(CardPreference::AskEveryTime, PermissionEffect::Ask),
            Some(PermissionEffect::Ask)
        );
        assert_eq!(
            card_preference_effect(CardPreference::AskEveryTime, PermissionEffect::Deny),
            None
        );
    }

    #[test]
    fn session_rules_are_isolated_and_confirmation_uses_captured_session() {
        let root = temp_dir("session_scope");
        let data_dir = root.join("data");
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::create_dir_all(&workspace).unwrap();
        let state = test_state(
            &data_dir,
            &workspace,
            vec![
                session_with("session-a", &workspace),
                session_with("session-b", &workspace),
            ],
        );

        let context_a = active_context(&state);
        assert_eq!(context_a.session_id.as_deref(), Some("session-a"));

        // Simulate switching sessions while the confirmation dialog is open.
        // The response must still be remembered in the captured A bucket.
        select_session_for_test(&state, "session-b", &workspace);
        remember_response(
            &state,
            &context_a,
            "shell",
            &PermissionResponse {
                allow: true,
                scope: PermissionScope::Session,
            },
        )
        .unwrap();

        let context_b = active_context(&state);
        assert_eq!(
            decide(&state, &context_b, "shell", test_policy()).effect,
            PermissionEffect::Ask
        );

        remember_response(
            &state,
            &context_b,
            "shell",
            &PermissionResponse {
                allow: false,
                scope: PermissionScope::Session,
            },
        )
        .unwrap();
        assert_eq!(
            decide(&state, &context_b, "shell", test_policy()).effect,
            PermissionEffect::Deny
        );

        select_session_for_test(&state, "session-a", &workspace);
        assert_eq!(
            decide(&state, &active_context(&state), "shell", test_policy()).effect,
            PermissionEffect::Allow
        );

        clear_session_rules(&state, "session-a");
        assert_eq!(
            decide(&state, &active_context(&state), "shell", test_policy()).effect,
            PermissionEffect::Ask
        );
        assert!(state
            .session_permission_rules
            .lock()
            .unwrap()
            .contains_key("session-b"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn deferred_targets_do_not_share_remembered_rules_or_wrapper_audit_identity() {
        let root = temp_dir("deferred_target_scope");
        let data_dir = root.join("data");
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::create_dir_all(&workspace).unwrap();
        let state = test_state(
            &data_dir,
            &workspace,
            vec![session_with("session-a", &workspace)],
        );
        let context = active_context(&state);
        let open = crate::tools::authorization_target_for_state(
            &state,
            "execute_tool",
            &serde_json::json!({
                "tool_name": "open_path",
                "args": {"target": "https://example.com"}
            }),
        );
        let screen = crate::tools::authorization_target_for_state(
            &state,
            "execute_tool",
            &serde_json::json!({
                "tool_name": "screen_capture_region",
                "args": {"x": 1, "y": 2, "width": 3, "height": 4}
            }),
        );
        assert_eq!(open.name, "open_path");
        assert_eq!(screen.name, "screen_capture_region");

        remember_response(
            &state,
            &context,
            &open.name,
            &PermissionResponse {
                allow: true,
                scope: PermissionScope::Session,
            },
        )
        .unwrap();
        let open_decision = decide(
            &state,
            &context,
            &open.name,
            open.definition.as_ref().unwrap().permission,
        );
        assert_eq!(open_decision.effect, PermissionEffect::Allow);
        assert_eq!(
            decide(
                &state,
                &context,
                &screen.name,
                screen.definition.as_ref().unwrap().permission,
            )
            .effect,
            PermissionEffect::Ask
        );

        audit(&state, &context, &open.name, &open_decision);
        let entries = load_recent_audit(&data_dir, 10);
        assert!(entries.iter().any(|entry| entry.tool == "open_path"));
        assert!(entries.iter().all(|entry| entry.tool != "execute_tool"));
        assert!(!state
            .session_permission_rules
            .lock()
            .unwrap()
            .get("session-a")
            .is_some_and(|rules| rules.contains_key("execute_tool")));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn project_rules_are_bucketed_by_canonical_workspace_identity() {
        let root = temp_dir("project_scope");
        let data_dir = root.join("data");
        let workspace_a = root.join("workspace-a");
        let workspace_b = root.join("workspace-b");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::create_dir_all(&workspace_a).unwrap();
        std::fs::create_dir_all(&workspace_b).unwrap();
        let state = test_state(
            &data_dir,
            &workspace_a,
            vec![
                session_with("session-a", &workspace_a),
                session_with("session-b", &workspace_b),
            ],
        );

        let context_a = active_context(&state);
        // As with Session scope, a response received after navigation must be
        // written to the project captured when the prompt was created.
        select_session_for_test(&state, "session-b", &workspace_b);
        remember_response(
            &state,
            &context_a,
            "shell",
            &PermissionResponse {
                allow: true,
                scope: PermissionScope::Project,
            },
        )
        .unwrap();

        let context_b = active_context(&state);
        assert_ne!(context_a.workspace_identity, context_b.workspace_identity);
        assert_eq!(
            decide(&state, &context_b, "shell", test_policy()).effect,
            PermissionEffect::Ask
        );
        remember_response(
            &state,
            &context_b,
            "shell",
            &PermissionResponse {
                allow: false,
                scope: PermissionScope::Project,
            },
        )
        .unwrap();
        assert_eq!(
            decide(&state, &context_b, "shell", test_policy()).effect,
            PermissionEffect::Deny
        );

        // A command racing with the two-lock session/workspace transition
        // must not apply either project's remembered rule.
        save_user_rules(
            &data_dir,
            &HashMap::from([(
                "shell".to_string(),
                PermissionRule {
                    tool: "shell".to_string(),
                    effect: PermissionEffect::Allow,
                    scope: PermissionScope::User,
                    reason: "broader allow".to_string(),
                    updated_at: 1,
                },
            )]),
        )
        .unwrap();
        state.sessions.lock().unwrap().active = "session-a".to_string();
        let transition_context = active_context(&state);
        assert!(transition_context.workspace_identity.is_none());
        assert!(transition_context.boundary_error.is_some());
        assert_eq!(
            decide(
                &state,
                &transition_context,
                "shell",
                PermissionPolicy::allow("broad default"),
            )
            .effect,
            PermissionEffect::Deny
        );

        select_session_for_test(&state, "session-a", &workspace_a.join("."));
        let equivalent_context_a = active_context(&state);
        assert_eq!(
            equivalent_context_a.workspace_identity,
            context_a.workspace_identity
        );
        assert_eq!(
            decide(&state, &equivalent_context_a, "shell", test_policy()).effect,
            PermissionEffect::Allow
        );

        let store = load_project_store(&data_dir).unwrap();
        assert_eq!(store.projects.len(), 2);
        assert!(store
            .projects
            .contains_key(context_a.workspace_identity.as_deref().unwrap()));
        assert!(store
            .projects
            .contains_key(context_b.workspace_identity.as_deref().unwrap()));

        let panel = panel_state(&state);
        let project_rule = panel
            .rules
            .iter()
            .find(|rule| rule.scope == PermissionScope::Project)
            .unwrap();
        assert_eq!(project_rule.effect, PermissionEffect::Allow);
        assert_eq!(
            project_rule.workspace_identity,
            context_a.workspace_identity
        );
        assert!(panel
            .rules
            .iter()
            .all(|rule| rule.effect != PermissionEffect::Deny));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_unscoped_project_rules_are_not_applied_or_migrated() {
        let root = temp_dir("legacy_project_scope");
        let data_dir = root.join("data");
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::create_dir_all(&workspace).unwrap();

        let legacy_rule = PermissionRule {
            tool: "shell".to_string(),
            effect: PermissionEffect::Allow,
            scope: PermissionScope::Project,
            reason: "legacy unscoped rule".to_string(),
            updated_at: 1,
        };
        save_rules_file(
            &data_dir.join("permissions.json"),
            &HashMap::from([("shell".to_string(), legacy_rule)]),
        )
        .unwrap();

        let state = test_state(
            &data_dir,
            &workspace,
            vec![session_with("session-a", &workspace)],
        );
        let context = active_context(&state);
        assert_eq!(
            decide(&state, &context, "shell", test_policy()).effect,
            PermissionEffect::Ask
        );
        assert!(!data_dir.join("project_permissions.json").exists());
        assert!(data_dir.join("permissions.json").exists());

        // Creating a new scoped rule must not use the legacy file as a seed.
        remember_response(
            &state,
            &context,
            "edit_file",
            &PermissionResponse {
                allow: true,
                scope: PermissionScope::Project,
            },
        )
        .unwrap();
        let scoped_rules =
            load_project_rules(&data_dir, context.workspace_identity.as_deref().unwrap()).unwrap();
        assert!(scoped_rules.contains_key("edit_file"));
        assert!(!scoped_rules.contains_key("shell"));
        assert_eq!(
            decide(&state, &context, "shell", test_policy()).effect,
            PermissionEffect::Ask
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn malformed_or_unsupported_project_store_fails_closed_over_broader_allow() {
        let root = temp_dir("project_store_fail_closed");
        let data_dir = root.join("data");
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::create_dir_all(&workspace).unwrap();
        let state = test_state(
            &data_dir,
            &workspace,
            vec![session_with("session-a", &workspace)],
        );
        let context = active_context(&state);
        let user_rule = PermissionRule {
            tool: "shell".to_string(),
            effect: PermissionEffect::Allow,
            scope: PermissionScope::User,
            reason: "broader allow".to_string(),
            updated_at: 1,
        };
        save_user_rules(
            &data_dir,
            &HashMap::from([("shell".to_string(), user_rule)]),
        )
        .unwrap();

        std::fs::write(data_dir.join("project_permissions.json"), "{not-json").unwrap();
        assert_eq!(
            decide(
                &state,
                &context,
                "shell",
                PermissionPolicy::allow("broad default"),
            )
            .effect,
            PermissionEffect::Deny
        );
        assert!(panel_state(&state)
            .notices
            .iter()
            .any(|notice| notice.contains("项目权限存储不可用")));

        std::fs::write(
            data_dir.join("project_permissions.json"),
            r#"{"version":999,"projects":{}}"#,
        )
        .unwrap();
        assert_eq!(
            decide(
                &state,
                &context,
                "shell",
                PermissionPolicy::allow("broad default"),
            )
            .effect,
            PermissionEffect::Deny
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn concurrent_project_updates_keep_both_buckets_and_leave_no_temp_file() {
        use std::sync::{Arc, Barrier};

        let root = temp_dir("project_store_concurrency");
        let data_dir = root.join("data");
        let workspace_a = root.join("workspace-a");
        let workspace_b = root.join("workspace-b");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::create_dir_all(&workspace_a).unwrap();
        std::fs::create_dir_all(&workspace_b).unwrap();
        let state = Arc::new(test_state(
            &data_dir,
            &workspace_a,
            vec![
                session_with("session-a", &workspace_a),
                session_with("session-b", &workspace_b),
            ],
        ));
        let context_a = context_for_session(&state, "session-a");
        select_session_for_test(&state, "session-b", &workspace_b);
        let context_b = context_for_session(&state, "session-b");
        let barrier = Arc::new(Barrier::new(3));

        let mut handles = Vec::new();
        for (context, tool) in [
            (context_a.clone(), "shell"),
            (context_b.clone(), "edit_file"),
        ] {
            let state = Arc::clone(&state);
            let barrier = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                remember_response(
                    &state,
                    &context,
                    tool,
                    &PermissionResponse {
                        allow: true,
                        scope: PermissionScope::Project,
                    },
                )
                .unwrap();
            }));
        }
        barrier.wait();
        for handle in handles {
            handle.join().unwrap();
        }

        let store = load_project_store(&data_dir).unwrap();
        assert_eq!(store.projects.len(), 2);
        assert!(store
            .projects
            .get(context_a.workspace_identity.as_deref().unwrap())
            .is_some_and(|rules| rules.contains_key("shell")));
        assert!(store
            .projects
            .get(context_b.workspace_identity.as_deref().unwrap())
            .is_some_and(|rules| rules.contains_key("edit_file")));
        assert!(std::fs::read_dir(&data_dir).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        }));

        drop(state);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn stale_rule_operations_cannot_modify_the_new_active_session() {
        let root = temp_dir("stale_session_rule");
        let data_dir = root.join("data");
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::create_dir_all(&workspace).unwrap();
        let state = test_state(
            &data_dir,
            &workspace,
            vec![
                session_with("session-a", &workspace),
                session_with("session-b", &workspace),
            ],
        );
        let context_a = context_for_session(&state, "session-a");
        remember_response(
            &state,
            &context_a,
            "shell",
            &PermissionResponse {
                allow: true,
                scope: PermissionScope::Session,
            },
        )
        .unwrap();
        select_session_for_test(&state, "session-b", &workspace);
        let context_b = active_context(&state);
        remember_response(
            &state,
            &context_b,
            "shell",
            &PermissionResponse {
                allow: false,
                scope: PermissionScope::Session,
            },
        )
        .unwrap();

        assert!(reset_rule(
            &state,
            PermissionScope::Session,
            "shell",
            Some("session-a"),
            None,
        )
        .is_err());
        assert_eq!(
            decide(&state, &context_b, "shell", test_policy()).effect,
            PermissionEffect::Deny
        );
        assert_eq!(
            decide(&state, &context_a, "shell", test_policy()).effect,
            PermissionEffect::Allow
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn deleted_session_confirmation_cannot_recreate_an_orphan_rule_bucket() {
        let root = temp_dir("deleted_session_rule");
        let data_dir = root.join("data");
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::create_dir_all(&workspace).unwrap();
        let state = test_state(
            &data_dir,
            &workspace,
            vec![session_with("session-a", &workspace)],
        );
        let context = active_context(&state);
        state
            .sessions
            .lock()
            .unwrap()
            .sessions
            .retain(|session| session.id != "session-a");

        assert!(remember_response(
            &state,
            &context,
            "shell",
            &PermissionResponse {
                allow: true,
                scope: PermissionScope::Session,
            },
        )
        .is_err());
        assert!(!state
            .session_permission_rules
            .lock()
            .unwrap()
            .contains_key("session-a"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn audit_records_session_and_workspace_context_and_reads_legacy_lines() {
        let root = temp_dir("audit_context");
        let data_dir = root.join("data");
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::create_dir_all(&workspace).unwrap();
        let state = test_state(
            &data_dir,
            &workspace,
            vec![session_with("session-a", &workspace)],
        );
        let context = active_context(&state);
        let decision = PermissionDecision::from_policy(test_policy());
        audit(&state, &context, "shell", &decision);

        let legacy = r#"{"timestamp":1,"tool":"legacy","effect":"ask","scope":"once","source":"tool_default","reason":"old audit"}"#;
        let mut file = OpenOptions::new()
            .append(true)
            .open(data_dir.join("permission_audit.jsonl"))
            .unwrap();
        writeln!(file, "{legacy}").unwrap();

        let entries = load_recent_audit(&data_dir, 10);
        let current = entries.iter().find(|entry| entry.tool == "shell").unwrap();
        assert_eq!(current.session_id.as_deref(), Some("session-a"));
        assert_eq!(current.workspace_identity, context.workspace_identity);
        let old = entries.iter().find(|entry| entry.tool == "legacy").unwrap();
        assert!(old.session_id.is_none());
        assert!(old.workspace_identity.is_none());

        let _ = std::fs::remove_dir_all(root);
    }
}
