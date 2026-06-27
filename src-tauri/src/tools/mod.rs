//! 组件 5/6：工具注册表 + 统一接口 + 执行。
//! 每个工具 = 名称 + 描述 + 输入 JSON Schema + 权限 + execute。循环遍历这张表。
//! 作用域是结构性强制的（文件工具被物理限制在沙盒目录），不靠提示词。
use serde_json::{json, Value};
use std::path::{Component, Path, PathBuf};

mod open_path;
mod read_file;
mod system_info;
mod web_search;
mod write_file;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Permission {
    /// 只读 / 幂等 / 低风险 —— 直接执行
    Auto,
    /// 不可逆 / 有副作用 —— 执行前必须前端确认
    Confirm,
}

pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub permission: Permission,
    pub parameters: Value,
}

/// 工具注册表。新增工具只需在这里加一项 + 在 execute 里加一条分支。
pub fn registry() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "open_path",
            description: "用系统默认程序打开一个文件、应用或网址（URL）。例如打开网页、图片、文件夹。会先请用户确认。",
            // 会以系统默认语义执行（可启动可执行/协议处理器），故需用户确认；同时拒绝 UNC 与危险协议
            permission: Permission::Confirm,
            parameters: json!({
                "type": "object",
                "properties": {
                    "target": { "type": "string", "description": "要打开的文件路径、网址或应用" }
                },
                "required": ["target"]
            }),
        },
        ToolDef {
            name: "read_file",
            description: "读取沙盒目录内某个文本文件的内容。路径相对于沙盒目录，不能访问沙盒之外。",
            permission: Permission::Auto,
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "相对沙盒目录的文件路径，如 notes/todo.txt" }
                },
                "required": ["path"]
            }),
        },
        ToolDef {
            name: "write_file",
            description: "在沙盒目录内创建或覆盖一个文本文件（不可逆，需用户确认）。路径相对于沙盒目录。",
            permission: Permission::Confirm,
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "相对沙盒目录的文件路径" },
                    "content": { "type": "string", "description": "要写入的完整文本内容" }
                },
                "required": ["path", "content"]
            }),
        },
        ToolDef {
            name: "web_search",
            description: "联网搜索，返回若干结果摘要（基于 DuckDuckGo，无需密钥）。适合查事实、找资料。",
            permission: Permission::Auto,
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "搜索关键词" }
                },
                "required": ["query"]
            }),
        },
        ToolDef {
            name: "system_info",
            description: "读取当前时间（UTC）、操作系统、架构、工作目录等基础系统状态。",
            permission: Permission::Auto,
            parameters: json!({ "type": "object", "properties": {} }),
        },
    ]
}

/// 转成 OpenAI tools 数组，发给 LLM。
pub fn schemas_json() -> Value {
    let arr: Vec<Value> = registry()
        .iter()
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            })
        })
        .collect();
    Value::Array(arr)
}

/// 直接用系统默认程序打开某路径（供「打开沙盒目录」按钮复用 open_path 逻辑）。
pub fn execute_open(target: &str) -> Result<String, String> {
    open_path::run(serde_json::json!({ "target": target }))
}

pub fn permission_for(name: &str) -> Permission {
    registry()
        .iter()
        .find(|t| t.name == name)
        .map(|t| t.permission)
        .unwrap_or(Permission::Confirm) // 未知工具按最严处理
}

/// 分发执行。MVP 用 async 函数 + match 充当统一执行入口
/// （避免为了少数异步工具引入 async-trait 依赖）。
pub async fn execute(state: &crate::AppState, name: &str, args: Value) -> Result<String, String> {
    match name {
        "open_path" => open_path::run(args),
        "read_file" => read_file::run(state, args),
        "write_file" => write_file::run(state, args),
        "web_search" => web_search::run(state, args).await,
        "system_info" => system_info::run(),
        other => Err(format!("未实现的工具：{other}")),
    }
}

// ---- 沙盒路径解析（供 read_file / write_file 共用）----

/// 在不要求路径存在的前提下做规范化（解析 . 与 ..），再校验仍位于沙盒内。
/// 这样写文件到尚不存在的路径也能做越界检查。
pub(crate) fn resolve_in_sandbox(sandbox: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return Err("路径必须是相对沙盒目录的相对路径，不能是绝对路径".to_string());
    }

    // 1) 词法解析（折叠 . 与 ..），不允许越过沙盒根
    let mut out = sandbox.to_path_buf();
    for comp in rel_path.components() {
        match comp {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            Component::ParentDir => {
                // 不允许越过沙盒根
                if out == *sandbox {
                    return Err("路径越界：不允许访问沙盒目录之外".to_string());
                }
                out.pop();
            }
            // 盘符/根等异常组件直接拒绝
            _ => return Err("非法路径组件".to_string()),
        }
    }

    if !out.starts_with(sandbox) {
        return Err("路径越界：不允许访问沙盒目录之外".to_string());
    }

    // 2) 真实路径校验：对沙盒根与「目标最近存在的祖先」分别 canonicalize，
    //    用解析后的真实路径再判一次包含关系，挡住 junction/符号链接逃逸。
    //    （std::fs 操作会跟随 reparse point，纯词法 starts_with 拦不住。）
    let canonical_sandbox =
        std::fs::canonicalize(sandbox).map_err(|e| format!("规范化沙盒目录失败：{e}"))?;
    let canonical_ancestor = canonical_existing_ancestor(&out)?;
    if !canonical_ancestor.starts_with(&canonical_sandbox) {
        return Err("路径越界：经链接解析后指向沙盒之外".to_string());
    }

    Ok(out)
}

/// 找到路径最近的「已存在祖先」并 canonicalize（解析其中所有 reparse point）。
/// 写新文件时目标本身可能尚不存在，但其已存在的父链路若含逃逸链接即会在此被发现。
fn canonical_existing_ancestor(p: &Path) -> Result<PathBuf, String> {
    let mut cur: &Path = p;
    loop {
        if cur.exists() {
            return std::fs::canonicalize(cur).map_err(|e| format!("规范化路径失败：{e}"));
        }
        match cur.parent() {
            Some(parent) => cur = parent,
            None => return Err("无法解析路径".to_string()),
        }
    }
}
