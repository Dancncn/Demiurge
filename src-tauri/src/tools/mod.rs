//! 组件 5/6：工具注册表 + 统一接口 + 执行。
//! 每个工具 = 名称 + 描述 + 输入 JSON Schema + 权限/风险/并发/输出策略 + execute。
//! 作用域是结构性强制的（文件工具被物理限制在沙盒目录），不靠提示词。
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Component, Path, PathBuf};

mod args;
mod edit_file;
mod git_status;
mod glob;
mod grep;
mod open_path;
mod read_file;
mod screen;
mod shell;
mod system_info;
mod web_search;
mod write_file;

pub use edit_file::EditUndoEntry;

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionEffect {
    Allow,
    Deny,
    Ask,
}

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionScope {
    Once,
    Session,
    Project,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolRisk {
    ReadOnly,
    Mutating,
    External,
    Privileged,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolConcurrency {
    ParallelSafe,
    SerialOnly,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolOutputPolicy {
    Inline,
    TruncateForUi,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
pub struct PermissionPolicy {
    pub effect: PermissionEffect,
    pub scope: PermissionScope,
    pub reason: &'static str,
}

impl PermissionPolicy {
    pub const fn allow(reason: &'static str) -> Self {
        PermissionPolicy {
            effect: PermissionEffect::Allow,
            scope: PermissionScope::Once,
            reason,
        }
    }

    pub const fn ask(reason: &'static str) -> Self {
        PermissionPolicy {
            effect: PermissionEffect::Ask,
            scope: PermissionScope::Once,
            reason,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub risk: ToolRisk,
    pub concurrency: ToolConcurrency,
    pub permission: PermissionPolicy,
    pub output_policy: ToolOutputPolicy,
    pub parameters: Value,
}

/// 工具注册表。新增工具只需在这里加一项 + 在 execute 里加一条分支。
pub fn registry() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "open_path",
            description: "用系统默认程序打开一个文件、应用或网址（URL）。例如打开网页、图片、文件夹。会先请用户确认。",
            risk: ToolRisk::Privileged,
            concurrency: ToolConcurrency::SerialOnly,
            permission: PermissionPolicy::ask("会调用系统默认程序或协议处理器，可能启动外部应用。"),
            output_policy: ToolOutputPolicy::Inline,
            parameters: json!({
                "type": "object",
                "properties": {
                    "target": { "type": "string", "description": "要打开的文件路径、网址或应用" }
                },
                "required": ["target"]
            }),
        },
        ToolDefinition {
            name: "read_file",
            description: "读取沙盒目录内某个文本文件的内容。路径相对于沙盒目录，不能访问沙盒之外。",
            risk: ToolRisk::ReadOnly,
            concurrency: ToolConcurrency::ParallelSafe,
            permission: PermissionPolicy::allow("只读取沙盒目录内的文本文件。"),
            output_policy: ToolOutputPolicy::TruncateForUi,
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "相对沙盒目录的文件路径，如 notes/todo.txt" }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "glob",
            description: "在沙盒目录内按 glob pattern 搜索文件路径。适合了解目录结构、查找代码或文档文件。",
            risk: ToolRisk::ReadOnly,
            concurrency: ToolConcurrency::ParallelSafe,
            permission: PermissionPolicy::allow("只列出沙盒目录内匹配的文件路径。"),
            output_policy: ToolOutputPolicy::TruncateForUi,
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "glob 模式，如 **/*.rs、notes/*.md；不能包含 .. 或绝对路径" },
                    "base": { "type": "string", "description": "可选：相对沙盒目录的搜索根目录，默认沙盒根" },
                    "limit": { "type": "integer", "description": "可选：最多返回多少条，默认 200，最大 500" }
                },
                "required": ["pattern"]
            }),
        },
        ToolDefinition {
            name: "grep",
            description: "在沙盒目录内搜索文本内容，返回匹配文件、行号和行摘要。默认按普通文本搜索，可开启 regex。",
            risk: ToolRisk::ReadOnly,
            concurrency: ToolConcurrency::ParallelSafe,
            permission: PermissionPolicy::allow("只读取沙盒目录内的文本文件并返回匹配摘要。"),
            output_policy: ToolOutputPolicy::TruncateForUi,
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "要搜索的文本或正则表达式" },
                    "path": { "type": "string", "description": "可选：相对沙盒目录的文件或目录，默认沙盒根" },
                    "case_sensitive": { "type": "boolean", "description": "可选：是否大小写敏感，默认 false" },
                    "regex": { "type": "boolean", "description": "可选：是否把 query 当作正则表达式，默认 false" },
                    "limit": { "type": "integer", "description": "可选：最多返回多少条匹配，默认 100，最大 300" }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "git_status",
            description: "读取沙盒目录的 Git 状态摘要（git status --short --branch）。只读，不会修改仓库。",
            risk: ToolRisk::ReadOnly,
            concurrency: ToolConcurrency::ParallelSafe,
            permission: PermissionPolicy::allow("只读取 Git 工作区状态。"),
            output_policy: ToolOutputPolicy::TruncateForUi,
            parameters: json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "shell",
            description: "在沙盒目录内执行短时 shell 命令。适合运行构建、测试、脚本和只在项目内生效的命令；执行前会请求确认。",
            risk: ToolRisk::Privileged,
            concurrency: ToolConcurrency::SerialOnly,
            permission: PermissionPolicy::ask("会启动本机 shell 进程并可能修改沙盒目录内文件。"),
            output_policy: ToolOutputPolicy::TruncateForUi,
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "要执行的 shell 命令，例如 npm test 或 cargo check" },
                    "cwd": { "type": "string", "description": "可选：相对沙盒目录的工作目录，默认沙盒根" },
                    "timeout_secs": { "type": "integer", "description": "可选：超时时间，默认 15 秒，最大 60 秒" }
                },
                "required": ["command"]
            }),
        },
        ToolDefinition {
            name: "write_file",
            description: "在沙盒目录内创建或覆盖一个文本文件（不可逆，需用户确认）。路径相对于沙盒目录。",
            risk: ToolRisk::Mutating,
            concurrency: ToolConcurrency::SerialOnly,
            permission: PermissionPolicy::ask("会创建或覆盖沙盒目录内的文件。"),
            output_policy: ToolOutputPolicy::Inline,
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "相对沙盒目录的文件路径" },
                    "content": { "type": "string", "description": "要写入的完整文本内容" }
                },
                "required": ["path", "content"]
            }),
        },
        ToolDefinition {
            name: "edit_file",
            description: "在沙盒目录内编辑已有文本文件：用 new_string 精确替换 old_string。默认要求 old_string 唯一，执行前会展示 diff 预览并请求确认。",
            risk: ToolRisk::Mutating,
            concurrency: ToolConcurrency::SerialOnly,
            permission: PermissionPolicy::ask("会修改沙盒目录内的已有文件。"),
            output_policy: ToolOutputPolicy::Inline,
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "相对沙盒目录的已有文本文件路径" },
                    "old_string": { "type": "string", "description": "要被替换的原文。默认必须在文件中唯一出现" },
                    "new_string": { "type": "string", "description": "替换后的新文本" },
                    "replace_all": { "type": "boolean", "description": "可选：是否替换全部出现位置，默认 false" }
                },
                "required": ["path", "old_string", "new_string"]
            }),
        },
        ToolDefinition {
            name: "multi_edit",
            description: "批量编辑沙盒内多个已有文本文件：每个 edit 使用 old_string/new_string 精确替换。执行前会全量预检并展示聚合 diff 预览。",
            risk: ToolRisk::Mutating,
            concurrency: ToolConcurrency::SerialOnly,
            permission: PermissionPolicy::ask("会批量修改沙盒目录内的已有文件。"),
            output_policy: ToolOutputPolicy::Inline,
            parameters: json!({
                "type": "object",
                "properties": {
                    "edits": {
                        "type": "array",
                        "description": "要应用的编辑列表，最多 20 个。任一 edit 预检失败则不会写入任何文件。",
                        "items": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string", "description": "相对沙盒目录的已有文本文件路径" },
                                "old_string": { "type": "string", "description": "要被替换的原文。默认必须在当前文件内容中唯一出现" },
                                "new_string": { "type": "string", "description": "替换后的新文本" },
                                "replace_all": { "type": "boolean", "description": "可选：是否替换全部出现位置，默认 false" }
                            },
                            "required": ["path", "old_string", "new_string"]
                        }
                    }
                },
                "required": ["edits"]
            }),
        },
        ToolDefinition {
            name: "apply_patch",
            description: "按结构化行 hunk 修改一个或多个沙盒文本文件。每个 hunk 指定 start_line、old_lines、new_lines；全量预检后展示聚合 diff 预览。",
            risk: ToolRisk::Mutating,
            concurrency: ToolConcurrency::SerialOnly,
            permission: PermissionPolicy::ask("会按结构化 patch 批量修改沙盒目录内的已有文件。"),
            output_policy: ToolOutputPolicy::Inline,
            parameters: json!({
                "type": "object",
                "properties": {
                    "hunks": {
                        "type": "array",
                        "description": "要应用的结构化行 hunk，最多 20 个。同文件 hunks 按顺序应用，后续 start_line 基于前一个 hunk 应用后的内容。",
                        "items": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string", "description": "相对沙盒目录的已有文本文件路径" },
                                "start_line": { "type": "integer", "description": "1-based 起始行号" },
                                "old_lines": { "type": "array", "items": { "type": "string" }, "description": "起始行处必须完整匹配的原始行列表，不含行尾换行" },
                                "new_lines": { "type": "array", "items": { "type": "string" }, "description": "替换后的新行列表，不含行尾换行" }
                            },
                            "required": ["path", "start_line", "old_lines", "new_lines"]
                        }
                    }
                },
                "required": ["hunks"]
            }),
        },
        ToolDefinition {
            name: "undo_edit",
            description: "撤销本进程内最近一次成功的 edit_file 修改。撤销前会确认目标文件未被后续外部修改，并展示反向 diff 预览。",
            risk: ToolRisk::Mutating,
            concurrency: ToolConcurrency::SerialOnly,
            permission: PermissionPolicy::ask("会把沙盒目录内最近一次 edit_file 修改恢复到编辑前内容。"),
            output_policy: ToolOutputPolicy::Inline,
            parameters: json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "web_search",
            description: "联网搜索，返回若干结果摘要（基于 DuckDuckGo，无需密钥）。适合查事实、找资料。",
            risk: ToolRisk::External,
            concurrency: ToolConcurrency::ParallelSafe,
            permission: PermissionPolicy::allow("只向搜索服务发送查询并读取公开结果。"),
            output_policy: ToolOutputPolicy::TruncateForUi,
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "搜索关键词" }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "screen_list_windows",
            description: "列出当前桌面上可见窗口的标题、应用名和屏幕坐标。适合在截图前定位目标窗口。",
            risk: ToolRisk::Privileged,
            concurrency: ToolConcurrency::SerialOnly,
            permission: PermissionPolicy::ask("会读取当前桌面窗口标题和位置，可能包含隐私信息。"),
            output_policy: ToolOutputPolicy::TruncateForUi,
            parameters: json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "screen_capture_region",
            description: "截取主显示器上一块屏幕区域，保存为沙盒 .demiurge/screenshots/ 下的 PNG，并返回文件路径和尺寸。坐标为物理像素。",
            risk: ToolRisk::Privileged,
            concurrency: ToolConcurrency::SerialOnly,
            permission: PermissionPolicy::ask("会读取屏幕像素并保存截图，可能包含密钥、聊天或其它隐私信息。"),
            output_policy: ToolOutputPolicy::Inline,
            parameters: json!({
                "type": "object",
                "properties": {
                    "x": { "type": "integer", "description": "截图区域左上角 x，物理像素，相对主显示器" },
                    "y": { "type": "integer", "description": "截图区域左上角 y，物理像素，相对主显示器" },
                    "width": { "type": "integer", "description": "截图宽度，物理像素" },
                    "height": { "type": "integer", "description": "截图高度，物理像素" },
                    "label": { "type": "string", "description": "可选：用于截图文件名的简短标签" }
                },
                "required": ["x", "y", "width", "height"]
            }),
        },
        ToolDefinition {
            name: "screen_capture_window",
            description: "按窗口标题或应用名匹配一个可见窗口，截取整窗或裁剪区域，保存为沙盒 .demiurge/screenshots/ 下的 PNG。裁剪参数为 0~1 比例。",
            risk: ToolRisk::Privileged,
            concurrency: ToolConcurrency::SerialOnly,
            permission: PermissionPolicy::ask("会读取目标窗口的屏幕像素并保存截图，可能包含密钥、聊天或其它隐私信息。"),
            output_policy: ToolOutputPolicy::Inline,
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "可选：要匹配的完整窗口标题" },
                    "app": { "type": "string", "description": "可选：要匹配的应用名。title/app 至少提供一个" },
                    "crop_left": { "type": "number", "description": "可选：左裁剪比例，默认 0" },
                    "crop_top": { "type": "number", "description": "可选：上裁剪比例，默认 0" },
                    "crop_right": { "type": "number", "description": "可选：右边界比例，默认 1" },
                    "crop_bottom": { "type": "number", "description": "可选：下边界比例，默认 1" },
                    "label": { "type": "string", "description": "可选：用于截图文件名的简短标签" }
                }
            }),
        },
        ToolDefinition {
            name: "system_info",
            description: "读取当前时间（UTC）、操作系统、架构、工作目录等基础系统状态。",
            risk: ToolRisk::ReadOnly,
            concurrency: ToolConcurrency::ParallelSafe,
            permission: PermissionPolicy::allow("只读取基础运行环境信息。"),
            output_policy: ToolOutputPolicy::Inline,
            parameters: json!({ "type": "object", "properties": {} }),
        },
    ]
}

pub fn definition_for(name: &str) -> Option<ToolDefinition> {
    registry().into_iter().find(|t| t.name == name)
}

/// 转成 OpenAI tools 数组，发给 LLM。
#[allow(dead_code)]
pub fn schemas_json() -> Value {
    schemas_json_for(crate::llm::ToolSchemaDialect::OpenAiCompatible)
}

pub fn schemas_json_for(dialect: crate::llm::ToolSchemaDialect) -> Value {
    match dialect {
        crate::llm::ToolSchemaDialect::OpenAiCompatible => openai_schemas_json(),
        crate::llm::ToolSchemaDialect::Anthropic => anthropic_schemas_json(),
        crate::llm::ToolSchemaDialect::Gemini => gemini_schemas_json(),
    }
}

fn openai_schemas_json() -> Value {
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

fn anthropic_schemas_json() -> Value {
    let arr: Vec<Value> = registry()
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.parameters,
            })
        })
        .collect();
    Value::Array(arr)
}

fn gemini_schemas_json() -> Value {
    let declarations: Vec<Value> = registry()
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "parameters": t.parameters,
            })
        })
        .collect();
    Value::Array(vec![json!({ "function_declarations": declarations })])
}

/// 直接用系统默认程序打开某路径（供「打开沙盒目录」按钮复用 open_path 逻辑）。
pub fn execute_open(target: &str) -> Result<String, String> {
    open_path::run(serde_json::json!({ "target": target }))
}

pub fn permission_policy_for(name: &str) -> PermissionPolicy {
    definition_for(name)
        .map(|t| t.permission)
        .unwrap_or_else(|| PermissionPolicy::ask("未知工具默认按最高安全级别询问。"))
}

/// 分发执行。MVP 用 async 函数 + match 充当统一执行入口
/// （避免为了少数异步工具引入 async-trait 依赖）。
pub async fn execute(state: &crate::AppState, name: &str, args: Value) -> Result<String, String> {
    match name {
        "open_path" => open_path::run(args),
        "read_file" => read_file::run(state, args),
        "glob" => glob::run(state, args),
        "grep" => grep::run(state, args),
        "git_status" => git_status::run(state, args),
        "shell" => shell::run(state, args),
        "write_file" => write_file::run(state, args),
        "edit_file" => edit_file::run(state, args),
        "multi_edit" => edit_file::multi_run(state, args),
        "apply_patch" => edit_file::patch_run(state, args),
        "undo_edit" => edit_file::undo(state, args),
        "web_search" => web_search::run(state, args).await,
        "screen_list_windows" => screen::list_windows(),
        "screen_capture_region" => screen::capture_region(state, args),
        "screen_capture_window" => screen::capture_window(state, args),
        "system_info" => system_info::run(),
        other => Err(format!("未实现的工具：{other}")),
    }
}

pub fn confirmation_preview(state: &crate::AppState, name: &str, args: Value) -> Option<String> {
    match name {
        "edit_file" => Some(
            edit_file::preview(state, args)
                .unwrap_or_else(|e| format!("无法生成 diff preview：{e}")),
        ),
        "multi_edit" => Some(
            edit_file::multi_preview(state, args)
                .unwrap_or_else(|e| format!("无法生成 multi_edit preview：{e}")),
        ),
        "apply_patch" => Some(
            edit_file::patch_preview(state, args)
                .unwrap_or_else(|e| format!("无法生成 apply_patch preview：{e}")),
        ),
        "undo_edit" => Some(
            edit_file::undo_preview(state, args)
                .unwrap_or_else(|e| format!("无法生成 undo preview：{e}")),
        ),
        "shell" => Some(
            shell::preview(state, args).unwrap_or_else(|e| format!("无法生成 shell preview：{e}")),
        ),
        "screen_list_windows" => Some("将读取当前桌面可见窗口标题、应用名与屏幕位置。".to_string()),
        "screen_capture_region" => Some(
            screen::preview_region(args).unwrap_or_else(|e| format!("无法生成截图 preview：{e}")),
        ),
        "screen_capture_window" => Some(
            screen::preview_window(args)
                .unwrap_or_else(|e| format!("无法生成窗口截图 preview：{e}")),
        ),
        _ => None,
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
