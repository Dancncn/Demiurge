//! 组件 7：权限门。auto 直接放行；confirm 类弹前端确认对话框，等用户裁决。
//! 确保有副作用的操作在执行前获得用户许可。
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

use crate::tools::{PermissionEffect, PermissionPolicy, PermissionScope, ToolRisk};

static SEQ: AtomicU64 = AtomicU64::new(1);

fn next_id() -> String {
    format!("confirm_{}", SEQ.fetch_add(1, Ordering::Relaxed))
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDecisionSource {
    ToolDefault,
    UserOverride,
    UnknownTool,
}

#[derive(Clone, Debug, Serialize)]
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

pub struct PermissionRequest<'a> {
    pub tool: &'a str,
    pub args_pretty: &'a str,
    pub description: &'a str,
    pub risk: ToolRisk,
    pub decision: PermissionDecision,
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

/// 向前端发起一次确认请求并 await 结果。
/// 机制：生成唯一 id → 存入 pending map 的 oneshot 发送端 → emit 事件给前端 →
/// 前端弹窗 → 用户点击后 invoke `respond_confirm(id, allow)` → 命令侧取出 sender 回填 →
/// 这里的 rx 收到布尔值。超时（5 分钟）按拒绝处理。
pub async fn confirm(app: &AppHandle, state: &crate::AppState, req: PermissionRequest<'_>) -> bool {
    let id = next_id();
    let (tx, rx) = oneshot::channel::<bool>();
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
        summary: format!("{}：{}", req.tool, req.decision.reason),
        preview: req.preview.as_deref(),
    };

    let _ = app.emit("tool-confirm-request", payload);

    match tokio::time::timeout(Duration::from_secs(300), rx).await {
        Ok(Ok(v)) => v,
        _ => {
            // 超时或通道异常：清理并按拒绝处理
            state.pending_confirms.lock().unwrap().remove(&id);
            false
        }
    }
}
