//! 组件 5/6：工具注册表 + 统一接口 + 执行。
//! 每个工具 = 名称 + 描述 + 输入 JSON Schema + 权限/风险/并发/输出策略 + execute。
//! 作用域是结构性强制的（文件工具被物理限制在沙盒目录），不靠提示词。
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Component, Path, PathBuf};

mod agent_spawn;
mod args;
mod context_tools;
mod edit_file;
mod execute_tool;
mod git_status;
mod glob;
mod goal_tool;
mod grep;
mod open_path;
mod read_file;
mod screen;
mod shell;
mod system_info;
mod tool_search;
mod web_fetch;
mod web_search;
mod worktree;
mod write_file;
mod write_plan;

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
    User,
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

pub const CORE_TOOL_NAMES: &[&str] = &[
    "read_file",
    "glob",
    "grep",
    "git_status",
    "shell",
    "write_plan",
    "write_file",
    "edit_file",
    "multi_edit",
    "apply_patch",
    "undo_edit",
    "web_fetch",
    "web_search",
    "agent_spawn",
    "context_inspect",
    "context_collapse",
    "goal",
    "tool_search",
    "execute_tool",
    "worktree_create",
    "system_info",
];

pub const DEFERRED_TOOL_NAMES: &[&str] = &[
    "open_path",
    "screen_list_windows",
    "screen_capture_region",
    "screen_capture_window",
    "screen_ocr_region",
    "screen_ocr_window",
];

pub fn is_deferred_tool(name: &str) -> bool {
    DEFERRED_TOOL_NAMES.contains(&name)
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
                    "timeout_secs": { "type": "integer", "description": "可选：超时时间，默认 15 秒，strict 模式默认 8 秒，最大 60 秒" },
                    "inherit_env": { "type": "boolean", "description": "可选：是否继承完整进程环境变量。默认 false，只传递最小跨平台环境白名单；strict 模式禁止 true。" },
                    "isolation": { "type": "string", "enum": ["standard", "strict"], "description": "可选：进程隔离策略。standard 为默认轻量隔离；strict 强制清空环境、缩短默认超时，并拒绝联网/依赖安装/破坏性/提权/外部执行类命令。" }
                },
                "required": ["command"]
            }),
        },
        ToolDefinition {
            name: "write_plan",
            description: "在 Plan Mode 中写入当前实施计划文件。只能写入沙盒 .demiurge/plans/ 下的 Markdown 计划。",
            risk: ToolRisk::Mutating,
            concurrency: ToolConcurrency::SerialOnly,
            permission: PermissionPolicy::ask("会创建一份计划文件，等待用户批准后才进入执行阶段。"),
            output_policy: ToolOutputPolicy::Inline,
            parameters: json!({
                "type": "object",
                "properties": {
                    "content": { "type": "string", "description": "完整 Markdown 实施计划内容" }
                },
                "required": ["content"]
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
            name: "web_fetch",
            description: "抓取单个公开 URL，返回统一的标题、正文、来源和 Sources 提醒；可通过 Exa livecrawl 做深抓取。",
            risk: ToolRisk::External,
            concurrency: ToolConcurrency::ParallelSafe,
            permission: PermissionPolicy::allow("只抓取用户指定的公开 http/https URL 或调用配置的 Exa livecrawl 服务。"),
            output_policy: ToolOutputPolicy::TruncateForUi,
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "要抓取的公开 http/https URL；省略 scheme 时默认 https。" },
                    "context_max_characters": { "type": "integer", "description": "可选：输出正文最大字符数，默认 20000，最大 80000。" },
                    "source": { "type": "string", "enum": ["direct", "exa"], "description": "可选：direct 直接抓取；exa 调用 Exa get_contents/livecrawl。默认 direct。" },
                    "livecrawl": { "type": "string", "enum": ["fallback", "always", "never"], "description": "可选：Exa livecrawl 策略；设置后自动走 Exa。fallback 仅在普通抓取不足时深抓取，always 总是深抓取，never 禁用深抓取。" }
                },
                "required": ["url"]
            }),
        },
        ToolDefinition {
            name: "web_search",
            description: "联网搜索，返回带来源链接的结果摘要。适合查事实、找资料、获取近期信息；回答中使用搜索信息时必须附 markdown Sources。",
            risk: ToolRisk::External,
            concurrency: ToolConcurrency::ParallelSafe,
            permission: PermissionPolicy::allow("只向搜索服务发送查询并读取公开结果。"),
            output_policy: ToolOutputPolicy::TruncateForUi,
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "搜索关键词。查询近期信息时应包含当前年份。" },
                    "allowed_domains": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "可选：只保留这些域名的结果，如 [\"docs.rs\", \"github.com\"]。不能与 blocked_domains 同时使用。"
                    },
                    "blocked_domains": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "可选：排除这些域名的结果。不能与 allowed_domains 同时使用。"
                    },
                    "num_results": { "type": "integer", "description": "可选：最多返回多少条结果，默认 8，最大 20。" },
                    "context_max_characters": { "type": "integer", "description": "可选：搜索结果输出的最大字符数，默认 10000，最大 50000。" },
                    "source": { "type": "string", "enum": ["auto", "bing", "duckduckgo", "tavily", "brave", "exa"], "description": "可选：搜索后端，auto、bing、duckduckgo、tavily、brave 或 exa。默认 auto；也可用 WEB_SEARCH_ADAPTER 环境变量指定。" },
                    "livecrawl": { "type": "string", "enum": ["fallback", "always", "never"], "description": "可选：Exa livecrawl 策略；需要深抓取搜索结果时使用，单 URL 深抓取优先用 web_fetch。" },
                    "search_type": { "type": "string", "enum": ["auto", "fast", "deep"], "description": "可选：Exa 搜索类型，auto 自动、fast 快速、deep 深度。" }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "agent_spawn",
            description: "启动一个只读子 Agent 来独立探索、审查、验证或反驳一个子任务。子 Agent 继承项目指令/记忆/会话摘要，可使用只读搜索与文件读取工具，结果只返回给主 Agent。",
            risk: ToolRisk::External,
            concurrency: ToolConcurrency::SerialOnly,
            permission: PermissionPolicy::ask("会额外调用 LLM，并可能把项目上下文和只读工具结果发送给模型服务。"),
            output_policy: ToolOutputPolicy::TruncateForUi,
            parameters: json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string", "description": "给子 Agent 的明确任务指令。必须包含范围、目标和期望输出。" },
                    "label": { "type": "string", "description": "可选：3-6 个词的短标签，用于区分多个子 Agent。" },
                    "agent_type": { "type": "string", "description": "可选：探索类型，如 Explore、Reviewer、Verifier、Critic、Planner。也会尝试匹配 .demiurge/agents/*.json。" },
                    "agent_name": { "type": "string", "description": "可选：.demiurge/agents/*.json 中的自定义 Agent 名称，优先于 agent_type。" },
                    "context_mode": { "type": "string", "description": "可选：brief、recent 或 fork。brief 只给摘要和少量最近消息；recent 给更多最近消息；fork 继承父消息并用 placeholder 修复未配对 tool_call。默认 brief。" },
                    "max_total_tokens": { "type": "integer", "description": "可选：该子 Agent 的硬 token 预算。provider 返回 usage 时使用精确统计，否则回退本地估算。多 reviewer 时会均分到每个 reviewer，保证总预算硬上限。" },
                    "output_format": { "type": "string", "enum": ["plain", "evidence_packet"], "description": "可选：plain 普通结论；evidence_packet 要求输出 verdict、confidence_score、findings/evidence、uncertainties、next_actions 结构化证据包。" },
                    "reviewer_count": { "type": "integer", "description": "可选：1-5。大于 1 时启动多个独立 reviewer，分别按不同视角输出 evidence packet，并返回合成包。默认 1。" }
                },
                "required": ["prompt"]
            }),
        },
        ToolDefinition {
            name: "context_inspect",
            description: "检查当前会话上下文使用情况，包括消息数、摘要大小、估算历史 token 和可折叠消息数。",
            risk: ToolRisk::ReadOnly,
            concurrency: ToolConcurrency::ParallelSafe,
            permission: PermissionPolicy::allow("只读取当前会话上下文统计。"),
            output_policy: ToolOutputPolicy::Inline,
            parameters: json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "context_collapse",
            description: "把当前会话的旧消息压缩进 rolling summary，并保留最近若干条消息。适合上下文接近上限时释放空间。",
            risk: ToolRisk::External,
            concurrency: ToolConcurrency::SerialOnly,
            permission: PermissionPolicy::ask("会调用摘要模型并修改当前会话历史与摘要。"),
            output_policy: ToolOutputPolicy::Inline,
            parameters: json!({
                "type": "object",
                "properties": {
                    "keep_recent": { "type": "integer", "description": "可选：保留最近多少条消息，默认 12，最小 2。" }
                }
            }),
        },
        ToolDefinition {
            name: "goal",
            description: "读取或更新当前会话的持续目标。模型只能读取状态，或在完成审计后标记 complete，或在同一阻塞连续出现 3 次后标记 blocked。",
            risk: ToolRisk::Mutating,
            concurrency: ToolConcurrency::ParallelSafe,
            permission: PermissionPolicy::allow("只更新当前会话的 goal 状态，不访问外部资源。"),
            output_policy: ToolOutputPolicy::Inline,
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["get", "update"],
                        "description": "get 读取当前 goal；update 标记 complete 或 blocked。若提供 status，可省略并默认 update。"
                    },
                    "status": {
                        "type": "string",
                        "enum": ["complete", "blocked"],
                        "description": "update 时必填。只能标记 complete 或 blocked。"
                    },
                    "reason": {
                        "type": "string",
                        "description": "update 时说明完成证据或阻塞原因。"
                    }
                }
            }),
        },
        ToolDefinition {
            name: "tool_search",
            description: "搜索未直接加载进主 tools schema 的 deferred tools。用于发现截图、OCR、打开路径等低频工具。",
            risk: ToolRisk::ReadOnly,
            concurrency: ToolConcurrency::ParallelSafe,
            permission: PermissionPolicy::allow("只搜索本地工具注册表元数据。"),
            output_policy: ToolOutputPolicy::TruncateForUi,
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "要搜索的工具能力，如 screenshot、ocr、open file。" },
                    "limit": { "type": "integer", "description": "可选：最多返回多少个工具，默认 8，最大 20。" }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "execute_tool",
            description: "代理执行 tool_search 发现的 deferred tool。core tool 必须直接调用，不要通过本工具执行。",
            risk: ToolRisk::Privileged,
            concurrency: ToolConcurrency::SerialOnly,
            permission: PermissionPolicy::ask("会执行一个被延迟加载的真实工具，可能读取屏幕、打开路径或触发系统能力。"),
            output_policy: ToolOutputPolicy::TruncateForUi,
            parameters: json!({
                "type": "object",
                "properties": {
                    "tool_name": { "type": "string", "description": "tool_search 返回的 deferred tool 名称。" },
                    "args": { "type": "object", "description": "传给目标工具的参数对象。" }
                },
                "required": ["tool_name", "args"]
            }),
        },
        ToolDefinition {
            name: "worktree_create",
            description: "在沙盒 Git 仓库中创建一个独立 git worktree，用于隔离较大的并行实现或实验分支。",
            risk: ToolRisk::Mutating,
            concurrency: ToolConcurrency::SerialOnly,
            permission: PermissionPolicy::ask("会调用 git worktree add 并在沙盒 .demiurge/worktrees 下创建新工作区。"),
            output_policy: ToolOutputPolicy::Inline,
            parameters: json!({
                "type": "object",
                "properties": {
                    "label": { "type": "string", "description": "worktree 短标签，会用于目录名。" },
                    "branch": { "type": "string", "description": "可选：新分支名，默认 demiurge/<label>。" }
                },
                "required": ["label"]
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
            name: "screen_ocr_region",
            description: "截取主显示器或副屏上的一块区域，用本地 PP-OCRv5 模型识别文本，返回合并文本和每行坐标。坐标为物理像素。",
            risk: ToolRisk::Privileged,
            concurrency: ToolConcurrency::SerialOnly,
            permission: PermissionPolicy::ask("会读取屏幕像素并做本地 OCR，可能包含密钥、聊天或其它隐私信息。"),
            output_policy: ToolOutputPolicy::TruncateForUi,
            parameters: json!({
                "type": "object",
                "properties": {
                    "x": { "type": "integer", "description": "OCR 区域左上角 x，物理像素" },
                    "y": { "type": "integer", "description": "OCR 区域左上角 y，物理像素" },
                    "width": { "type": "integer", "description": "OCR 区域宽度，物理像素" },
                    "height": { "type": "integer", "description": "OCR 区域高度，物理像素" }
                },
                "required": ["x", "y", "width", "height"]
            }),
        },
        ToolDefinition {
            name: "screen_ocr_window",
            description: "按窗口标题或应用名匹配一个可见窗口，截取整窗或裁剪区域，用本地 PP-OCRv5 模型识别文本并返回每行坐标。",
            risk: ToolRisk::Privileged,
            concurrency: ToolConcurrency::SerialOnly,
            permission: PermissionPolicy::ask("会读取目标窗口的屏幕像素并做本地 OCR，可能包含密钥、聊天或其它隐私信息。"),
            output_policy: ToolOutputPolicy::TruncateForUi,
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "可选：要匹配的完整窗口标题" },
                    "app": { "type": "string", "description": "可选：要匹配的应用名。title/app 至少提供一个" },
                    "crop_left": { "type": "number", "description": "可选：左裁剪比例，默认 0" },
                    "crop_top": { "type": "number", "description": "可选：上裁剪比例，默认 0" },
                    "crop_right": { "type": "number", "description": "可选：右边界比例，默认 1" },
                    "crop_bottom": { "type": "number", "description": "可选：下边界比例，默认 1" }
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

pub fn registry_for_state(state: &crate::AppState) -> Vec<ToolDefinition> {
    let mut defs = registry();
    defs.extend(crate::mcp::tool_definitions(state));
    defs
}

pub fn definition_for(name: &str) -> Option<ToolDefinition> {
    registry().into_iter().find(|t| t.name == name)
}

pub fn definition_for_state(state: &crate::AppState, name: &str) -> Option<ToolDefinition> {
    registry_for_state(state)
        .into_iter()
        .find(|t| t.name == name)
}

pub fn deferred_definitions() -> Vec<ToolDefinition> {
    registry()
        .into_iter()
        .filter(|t| is_deferred_tool(t.name))
        .collect()
}

/// 转成 OpenAI tools 数组，发给 LLM。
#[allow(dead_code)]
pub fn schemas_json() -> Value {
    schemas_json_for(crate::llm::ToolSchemaDialect::OpenAiCompatible)
}

pub fn schemas_json_for(dialect: crate::llm::ToolSchemaDialect) -> Value {
    schemas_json_for_defs(dialect, &registry())
}

pub fn main_schemas_json_for(dialect: crate::llm::ToolSchemaDialect) -> Value {
    schemas_json_for_names(dialect, CORE_TOOL_NAMES)
}

pub fn main_schemas_json_for_state(
    state: &crate::AppState,
    dialect: crate::llm::ToolSchemaDialect,
) -> Value {
    schemas_json_for_defs(dialect, &registry_for_state(state))
}

pub fn schemas_json_for_names(dialect: crate::llm::ToolSchemaDialect, names: &[&str]) -> Value {
    let defs = registry()
        .into_iter()
        .filter(|t| names.contains(&t.name))
        .collect::<Vec<_>>();
    schemas_json_for_defs(dialect, &defs)
}

pub fn schemas_json_for_names_state(
    state: &crate::AppState,
    dialect: crate::llm::ToolSchemaDialect,
    names: &[&str],
) -> Value {
    let defs = registry_for_state(state)
        .into_iter()
        .filter(|t| names.contains(&t.name))
        .collect::<Vec<_>>();
    schemas_json_for_defs(dialect, &defs)
}

fn schemas_json_for_defs(dialect: crate::llm::ToolSchemaDialect, defs: &[ToolDefinition]) -> Value {
    match dialect {
        crate::llm::ToolSchemaDialect::OpenAiCompatible => openai_schemas_json(defs),
        crate::llm::ToolSchemaDialect::Anthropic => anthropic_schemas_json(defs),
        crate::llm::ToolSchemaDialect::Gemini => gemini_schemas_json(defs),
    }
}

fn openai_schemas_json(defs: &[ToolDefinition]) -> Value {
    let arr: Vec<Value> = defs
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

fn anthropic_schemas_json(defs: &[ToolDefinition]) -> Value {
    let arr: Vec<Value> = defs
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

fn gemini_schemas_json(defs: &[ToolDefinition]) -> Value {
    let declarations: Vec<Value> = defs
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

pub fn permission_policy_for_state(state: &crate::AppState, name: &str) -> PermissionPolicy {
    definition_for_state(state, name)
        .map(|t| t.permission)
        .unwrap_or_else(|| PermissionPolicy::ask("未知工具默认按最高安全级别询问。"))
}

/// 分发执行。MVP 用 async 函数 + match 充当统一执行入口
/// （避免为了少数异步工具引入 async-trait 依赖）。
pub async fn execute(state: &crate::AppState, name: &str, args: Value) -> Result<String, String> {
    if crate::mcp::is_mcp_tool_name(name) {
        return crate::mcp::call_tool(state, name, args).await;
    }
    match name {
        "open_path" => open_path::run(args),
        "read_file" => read_file::run(state, args),
        "glob" => glob::run(state, args),
        "grep" => grep::run(state, args),
        "git_status" => git_status::run(state, args),
        "shell" => shell::run(state, args),
        "write_plan" => write_plan::run(state, args),
        "write_file" => write_file::run(state, args),
        "edit_file" => edit_file::run(state, args),
        "multi_edit" => edit_file::multi_run(state, args),
        "apply_patch" => edit_file::patch_run(state, args),
        "undo_edit" => edit_file::undo(state, args),
        "web_fetch" => web_fetch::run(state, args).await,
        "web_search" => web_search::run(state, args).await,
        "agent_spawn" => agent_spawn::run(state, args).await,
        "context_inspect" => context_tools::inspect(state),
        "context_collapse" => context_tools::collapse(state, args).await,
        "goal" => goal_tool::run(state, args),
        "tool_search" => tool_search::run(args),
        "execute_tool" => execute_tool::run(state, args).await,
        "worktree_create" => worktree::create(state, args),
        "screen_list_windows" => screen::list_windows(state),
        "screen_capture_region" => screen::capture_region(state, args),
        "screen_capture_window" => screen::capture_window(state, args),
        "screen_ocr_region" => screen::ocr_region(state, args),
        "screen_ocr_window" => screen::ocr_window(state, args),
        "system_info" => system_info::run(),
        other => Err(format!("未实现的工具：{other}")),
    }
}

pub async fn execute_subagent_readonly(
    state: &crate::AppState,
    name: &str,
    args: Value,
) -> Result<String, String> {
    match name {
        "read_file" => read_file::run(state, args),
        "glob" => glob::run(state, args),
        "grep" => grep::run(state, args),
        "git_status" => git_status::run(state, args),
        "system_info" => system_info::run(),
        "web_fetch" => web_fetch::run(state, args).await,
        "web_search" => web_search::run(state, args).await,
        "context_inspect" => context_tools::inspect(state),
        other => Err(format!("子 Agent 不允许使用工具：{other}")),
    }
}

pub fn permission_summary(name: &str, args: &Value) -> String {
    let str_arg = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or("").trim();
    let int_arg = |key: &str| args.get(key).and_then(Value::as_i64);

    match name {
        "open_path" => {
            let target = str_arg("target");
            if target.starts_with("http://") || target.starts_with("https://") {
                format!("将用系统默认程序打开外部链接：{target}")
            } else if !target.is_empty() {
                format!("将用系统默认程序打开路径或应用：{target}")
            } else {
                "将调用系统默认程序打开目标。".to_string()
            }
        }
        "shell" => {
            let command = str_arg("command");
            let cwd = str_arg("cwd");
            let env_note = if args
                .get("inherit_env")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                "将继承完整环境变量"
            } else {
                "将使用最小环境白名单"
            };
            let safety = shell::safety_summary(command);
            if cwd.is_empty() {
                format!("将在沙盒根目录执行 shell 命令：{command}。风险：{safety}；{env_note}。")
            } else {
                format!(
                    "将在沙盒内 `{cwd}` 执行 shell 命令：{command}。风险：{safety}；{env_note}。"
                )
            }
        }
        "write_plan" => "将写入 Plan Mode 实施计划文件，等待用户批准。".to_string(),
        "write_file" => {
            let path = str_arg("path");
            format!("将创建或覆盖沙盒内文件：{path}")
        }
        "edit_file" => {
            let path = str_arg("path");
            if args
                .get("replace_all")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                format!("将批量替换沙盒内文件的匹配内容：{path}")
            } else {
                format!("将精确替换沙盒内文件的一处匹配内容：{path}")
            }
        }
        "multi_edit" => {
            let count = args
                .get("edits")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            format!("将批量应用 {count} 个文本编辑；任一预检失败则不会写入。")
        }
        "apply_patch" => {
            let count = args
                .get("hunks")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            format!("将按结构化 patch 应用 {count} 个 hunk；执行前会预检匹配行。")
        }
        "undo_edit" => "将撤销本进程最近一次成功编辑，并先确认目标文件未被外部修改。".to_string(),
        "context_collapse" => {
            "将调用模型把旧会话压缩进 rolling summary，并修改当前会话历史。".to_string()
        }
        "agent_spawn" => {
            let label = str_arg("label");
            let mode = str_arg("context_mode");
            let label = if label.is_empty() {
                "只读子 Agent"
            } else {
                label
            };
            if mode.is_empty() {
                format!("将额外启动 {label}，并可能把项目上下文发送给模型服务。")
            } else {
                format!("将以 `{mode}` 上下文模式额外启动 {label}，并可能调用模型服务。")
            }
        }
        "execute_tool" => {
            let tool_name = str_arg("tool_name");
            format!("将执行按需发现的 deferred tool：{tool_name}")
        }
        "worktree_create" => {
            let label = str_arg("label");
            let branch = str_arg("branch");
            if branch.is_empty() {
                format!("将在沙盒 Git 仓库创建独立 worktree：{label}")
            } else {
                format!("将在沙盒 Git 仓库创建 worktree `{label}` 和分支 `{branch}`。")
            }
        }
        "screen_list_windows" => {
            "将读取当前桌面可见窗口标题、应用名和屏幕位置，可能包含隐私信息。".to_string()
        }
        "screen_capture_region" => format!(
            "将截取屏幕区域 x={} y={} width={} height={} 并保存到沙盒。",
            int_arg("x").map_or("?".to_string(), |v| v.to_string()),
            int_arg("y").map_or("?".to_string(), |v| v.to_string()),
            int_arg("width").map_or("?".to_string(), |v| v.to_string()),
            int_arg("height").map_or("?".to_string(), |v| v.to_string())
        ),
        "screen_capture_window" => {
            let title = str_arg("title");
            let app = str_arg("app");
            format!("将截取匹配窗口 title=`{title}` app=`{app}`，截图可能包含隐私信息。")
        }
        "screen_ocr_region" => format!(
            "将截取屏幕区域 x={} y={} width={} height={} 并用本地 OCR 识别文本。",
            int_arg("x").map_or("?".to_string(), |v| v.to_string()),
            int_arg("y").map_or("?".to_string(), |v| v.to_string()),
            int_arg("width").map_or("?".to_string(), |v| v.to_string()),
            int_arg("height").map_or("?".to_string(), |v| v.to_string())
        ),
        "screen_ocr_window" => {
            let title = str_arg("title");
            let app = str_arg("app");
            format!("将对匹配窗口 title=`{title}` app=`{app}` 截图并做本地 OCR，可能包含隐私信息。")
        }
        _ => definition_for(name)
            .map(|t| format!("{}：{}", t.name, t.permission.reason))
            .unwrap_or_else(|| format!("未知工具 `{name}` 将按最高安全级别处理。")),
    }
}

pub fn permission_summary_for_state(state: &crate::AppState, name: &str, args: &Value) -> String {
    if let Some(summary) = crate::mcp::permission_summary(state, name) {
        return summary;
    }
    permission_summary(name, args)
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
        "execute_tool" => Some(
            execute_tool::preview(args)
                .unwrap_or_else(|e| format!("无法生成 execute_tool preview：{e}")),
        ),
        "worktree_create" => Some(
            worktree::preview(args).unwrap_or_else(|e| format!("无法生成 worktree preview：{e}")),
        ),
        "screen_list_windows" => Some("将读取当前桌面可见窗口标题、应用名与屏幕位置。".to_string()),
        "screen_capture_region" => Some(
            screen::preview_region(args).unwrap_or_else(|e| format!("无法生成截图 preview：{e}")),
        ),
        "screen_capture_window" => Some(
            screen::preview_window(args)
                .unwrap_or_else(|e| format!("无法生成窗口截图 preview：{e}")),
        ),
        "screen_ocr_region" => Some(
            screen::preview_ocr_region(args)
                .unwrap_or_else(|e| format!("无法生成 OCR preview：{e}")),
        ),
        "screen_ocr_window" => Some(
            screen::preview_ocr_window(args)
                .unwrap_or_else(|e| format!("无法生成窗口 OCR preview：{e}")),
        ),
        _ => None,
    }
}

pub fn affected_paths(name: &str, args: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |value: String| {
        if !value.trim().is_empty() && !out.contains(&value) {
            out.push(value);
        }
    };
    let str_arg = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or("").trim();

    match name {
        "open_path" => push(str_arg("target").to_string()),
        "shell" => push(str_arg("cwd").to_string()),
        "write_file" | "edit_file" => push(str_arg("path").to_string()),
        "multi_edit" => {
            if let Some(edits) = args.get("edits").and_then(Value::as_array) {
                for edit in edits {
                    if let Some(path) = edit.get("path").and_then(Value::as_str) {
                        push(path.to_string());
                    }
                }
            }
        }
        "apply_patch" => {
            if let Some(hunks) = args.get("hunks").and_then(Value::as_array) {
                for hunk in hunks {
                    if let Some(path) = hunk.get("path").and_then(Value::as_str) {
                        push(path.to_string());
                    }
                }
            }
        }
        "worktree_create" => {
            let label = str_arg("label");
            if !label.is_empty() {
                push(format!(".worktrees/{label}"));
            }
        }
        "screen_capture_region"
        | "screen_capture_window"
        | "screen_ocr_region"
        | "screen_ocr_window" => {
            push(".demiurge/screens".to_string());
        }
        _ => {}
    }

    out
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
