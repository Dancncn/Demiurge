# 实现说明（给协作者）

面向想读懂 / 扩展代码的人。设计动机见 [demiurge-mvp-design.md](./demiurge-mvp-design.md)，待办见 [TODO.md](./TODO.md)。

## 总览

```
React UI (webview)  ──invoke 命令──▶  Rust 内核 (Tauri)  ──HTTP 流──▶  LLM 端点
       ▲                                    │
       └──────── emit 事件（流式/工具/确认）──┘
```

- **后端 = Rust**，承载全部 agent 逻辑；**前端 = React**，只做展示与交互。
- 二者通过 **Tauri 命令**（前端→后端，请求/响应）与 **Tauri 事件**（后端→前端，单向推送）通信。

## Rust 内核（`src-tauri/src/`）

| 模块 | 职责 | 关键入口 |
|---|---|---|
| `lib.rs` | 全局状态 `AppState`、命令注册、Tauri builder、`setup()` 初始化路径/持久化 | `run()` |
| `agent/runner.rs` | **Agent 循环**：调 LLM → 执行工具 → 喂回 → 重复 | `run_turn()` |
| `agent/conversation.rs` | OpenAI 兼容消息结构（role / tool_calls / tool_result） | `Message` |
| `agent/budget.rs` | 轻量 token-aware budget：估算 system/tools/history/output reserve，占用并给历史分配预算 | `history_budget()` |
| `agent/context.rs` | 对话历史裁剪（先截老工具输出，再丢更老回合，并返回可摘要的旧消息） | `trim_collect_removed_by_tokens()` |
| `agent/summary.rs` | 会话 rolling summary：把被裁剪的旧消息压缩成短期会话摘要 | `update_session_summary()` |
| `agent/collapse.rs` | 上下文折叠：检查上下文压力，手动/工具触发压缩旧消息并保留最近消息 | `inspect()` / `compact_active_session()` |
| `agent/memory.rs` | 自动长期记忆提取：保守提取用户偏好/项目约束并追加到沙盒 `.demiurge/memory.md` | `extract_and_update()` |
| `agent/prompt.rs` | Phase 2 prompt section builder：engine/persona/project/environment/session summary/memory 分区 | `build()` |
| `agent/persona.rs` | 引擎基础指令片段 | `engine_base()` |
| `agent/workflow_journal.rs` | Ultracode workflow journal：记录 `/ultracode` run 事件并生成 resume overlay | `append()` / `resume_overlay()` |
| `agent/workflow_runtime.rs` | Rust 原生 workflow JSON DSL：加载 `.demiurge/workflows/*.json`，执行 `agent` / `parallel` / `pipeline` / `phase` / `budget`，并向前端推送 live panel 状态 | `panel_state()` / `launch()` / `run_launched()` / `stop()` |
| `llm/mod.rs` | Provider adapter 分发入口；统一 `AssistantTurn` / `ProviderProfile`，按设置分发到 OpenAI-compatible / local / Anthropic / Gemini | `stream_completion()` |
| `llm/openai.rs` / `local.rs` / `anthropic.rs` / `gemini.rs` | 各 provider 的请求构造、SSE 解析、工具调用格式转换 | `stream_completion()` |
| `tools/mod.rs` | 工具注册表 + metadata（risk/concurrency/permission/output policy）+ 统一执行入口 + 沙盒路径解析 | `registry()` / `execute()` |
| `tools/*.rs` | 各内置工具实现 | `open_path` / `read_file` / `write_file` / `edit_file` / `multi_edit` / `apply_patch` / `undo_edit` / `shell` / `web_search` / `agent_spawn` / `system_info` / `glob` / `grep` / `git_status` / screen/OCR 工具 |
| `permission/mod.rs` | confirm 工具的前端确认往返（oneshot 通道） | `confirm()` |
| `pack/mod.rs` | 角色包清单加载、首启动落地默认包 | `load_pack()` / `ensure_default()` |
| `store/mod.rs` | 设置 / 多会话 JSON 落盘（含旧版迁移） | `Settings` / `Session` / `SessionStore` |

数据目录（Tauri `app_data_dir`）下：`settings.json`、`sessions.json`（多会话 + 活动会话 + 每会话 rolling summary）、`sandbox/`、`packs/`。
（旧版单会话 `conversation.json` 在首次启动时会自动迁移成一条会话。）

### Agent 循环（`run_turn`）
1. 追加用户消息、持久化。
2. 通过 `agent/prompt.rs` 拼分区化 system prompt（引擎规则、角色人格、项目指令、运行环境、会话摘要、只读 memory），再由 `agent/budget.rs` 估算 system/tools/output reserve 占用并按剩余 token 预算裁剪历史，随后流式调 LLM；正文增量通过 `assistant-delta` 事件实时推给前端。
3. 若返回 `tool_calls`：把带 tool_calls 的 assistant 消息入历史，逐个工具——
   - `emit tool-start` → 权限门（auto 直接放行 / confirm 走前端确认）→ 执行 → `emit tool-end`；
   - 把结果作为 `tool` 消息喂回（**每个 tool_call 都必须有对应结果**，否则下一轮 400）。
   - 然后回到第 2 步，让模型基于结果继续。
4. 无 tool_calls → 最终答复，`emit assistant-done`，随后由 `agent/memory.rs` 尝试把本轮稳定偏好/项目约束追加到沙盒 `.demiurge/memory.md`（失败不影响答复）。
5. 全程检查 `cancel` 标志；用户中断会唤醒待确认项并尽快收尾。

### Provider Adapter

`llm/mod.rs` 保持统一入口 `stream_completion()`，调用方继续传入 Demiurge 内部的 `Message` / `ToolCall` 历史和当前 provider 方言的工具 schema。模块内部根据 `Settings.provider` 分发：

- `open_ai_compatible`：调用 `{base_url}/chat/completions`，沿用 OpenAI-compatible `messages` / `tools` / `tool_calls` 格式，支持 DeepSeek 等兼容端点。
- `local`：复用 OpenAI-compatible adapter，但允许 API Key 为空，适配 LM Studio / Ollama OpenAI-compatible / vLLM 等本地服务。
- `anthropic`：调用 Anthropic Messages API `{base_url}/messages`，把内部 `system`、assistant `tool_calls` 和 `tool` result 转换为 Anthropic `system`、`tool_use`、`tool_result` content blocks，并解析 `text_delta` / `input_json_delta` 流。
- `gemini`：调用 Google AI Studio REST `models/{model}:streamGenerateContent?alt=sse`，把内部消息转换为 `systemInstruction` / `contents` / `functionCall` / `functionResponse`，并解析 SSE 中的 `text` 与 `functionCall` parts。

工具 schema 由 `tools::main_schemas_json_for(profile.tool_schema_dialect)` 生成：OpenAI-compatible/local 使用 OpenAI function tools；Anthropic 使用 `input_schema`；Gemini 使用 `function_declarations`。主 schema 只包含 core tools；低频工具（screen/OCR/open_path 等）留在 deferred pool，通过 `tool_search` 发现、`execute_tool` 代理执行，保持 tools JSON 稳定。历史消息仍以统一 `Message` / `ToolCall` 结构持久化，不保存厂商原生格式。

### Project Context / Memory 首阶段

`agent/prompt.rs` 负责把 system prompt 拆成清晰分区：

- **引擎规则**：来自 `agent/persona.rs`，描述工具、权限、沙盒和输出约束。
- **角色设定**：来自当前角色包 `persona.md`。
- **项目指令**：从沙盒根读取 `DEMIURGE.md` 和 `CLAUDE.md`（可同时存在，单文件 32 KiB 上限）。
- **运行环境**：包含当前 Unix 毫秒时间戳、沙盒工作区路径、当前角色包 id、`git status --short --branch` 摘要（非 git 目录自动降级）。
- **会话摘要**：每个 `Session` 可持有一段 rolling summary；当历史超出 token-aware history budget 并被裁剪时，`agent/summary.rs` 会把被移除的旧消息与已有摘要合并成新的短期会话摘要。
- **记忆**：只读加载沙盒根 `memory.md`、沙盒根 `.demiurge/memory.md`、当前角色包 `memory.md`（可选，单文件 32 KiB 上限）；自动记忆提取只会追加写入沙盒 `.demiurge/memory.md`，不写角色包。

这些 project/memory/environment/summary section 总体再做字符上限截断，避免挤占对话历史预算。`agent/budget.rs` 用启发式 token 估算把 system prompt、工具 schema、历史消息和输出预留纳入同一预算；`agent/context.rs` 按预算裁剪历史并返回被移除的旧消息；`agent/summary.rs` 只维护当前会话的短期摘要。`agent/memory.rs` 在最终答复后保守提取长期有用信息，写入本地 Markdown memory，不做向量库/RAG。

### 前后端事件协议
后端 emit（`src-tauri` → 前端 `src/lib/api.ts` 监听）：

| 事件 | 载荷 | 含义 |
|---|---|---|
| `assistant-start` | — | 一次 LLM 调用开始 |
| `assistant-delta` | `string` | 正文增量 token |
| `assistant-done` | `string` | 最终答复（完整正文） |
| `assistant-interrupted` | — | 本轮被用户中断 |
| `tool-start` | `{tool_call_id, name, args, description?, risk?, permission_effect?, concurrency?}` | 工具开始执行；前端用 `tool_call_id` 关联卡片 |
| `tool-end` | `{tool_call_id, name, ok, result}` | 工具结束（ok=是否放行） |
| `tool-confirm-request` | `{id, tool, args, description?, risk?, effect?, scope?, reason?, summary?, preview?}` | 请前端弹确认框；`preview` 可携带 diff/shell 等执行前预览；前端回 `respond_confirm(id, allow, scope)` |
| `workflow-updated` | `WorkflowPanelState` | workflow 定义、run、agent 状态或日志变化；`WorkflowsPanel` 用它刷新 live 状态 |

前端命令（`invoke`）：`send` / `interrupt` / `respond_confirm(id, allow, scope)` / `get_settings` / `save_settings` / `list_packs` / `list_sessions` / `get_history` / `new_session` / `select_session` / `delete_session` / `open_sandbox` / `workflow_panel_state` / `workflow_run` / `workflow_stop`。
Agent 循环在开始时**捕获活动会话 id**，整轮写入都落到这一段对话——即便用户中途切换会话也不会串台。

## 安全模型要点
- **沙盒**：`read_file` / `write_file` 限定在 `app_data_dir/sandbox`。路径先做词法 `..` 折叠并 `starts_with` 校验，
  再对「目标最近存在的祖先」做 `canonicalize` 二次校验，挡住 junction / 符号链接逃逸。
- **文件工具**：`read_file` / `write_file` / `edit_file` / `multi_edit` / `apply_patch` / `undo_edit` 限定在沙盒目录；`edit_file` 只修改已有 UTF-8 文本文件，默认要求 `old_string` 唯一，执行前在确认弹窗展示 diff preview，成功后把编辑前后内容记录到进程内 undo 栈；`multi_edit` 对多个精确替换先做全量预检并展示聚合 diff，确认后按文件写入并为每个被修改文件记录 undo；`apply_patch` 使用结构化行 hunk，只在指定行完整匹配 `old_lines` 时应用，预检全通过后展示聚合 diff 并记录 undo；`undo_edit` 只能撤销最近一次 `edit_file` / `multi_edit` / `apply_patch` 写入记录，执行前同样确认并校验目标文件当前内容未偏离记录。
- **搜索工具**：`glob` / `grep` 只遍历沙盒内文件，拒绝绝对路径与 `..` 越界输入，并对返回条数、单文件大小和行长度设上限。
- **Web Search**：`web_search` 默认使用 Bing 结果页抽取，DuckDuckGo Instant Answer 作为 fallback；支持 `allowed_domains` / `blocked_domains`、`num_results`、`context_max_characters` 和 `source`。工具结果始终附来源链接和 Sources 提醒，模型使用联网信息答复时必须在末尾列出 markdown sources。
- **Deferred Tools**：`tool_search` 搜索未直接加载的低频工具，`execute_tool` 代理执行。这样主工具 schema 在会话中更稳定，减少固定上下文成本；代理执行仍走确认门。
- **Context Collapse**：`/compact`、`context_inspect`、`context_collapse` 共用 `agent/collapse.rs`，把旧消息压入会话 rolling summary，并避免留下孤儿 `tool` 消息。
- **Fork Subagent**：`agent_spawn` 的 `context_mode=fork` 会继承父会话消息，并用固定 placeholder 修复当前未完成 tool call 的配对；子 Agent 仍只能执行只读工具，非只读调用会被拒绝。
- **Workflow Runtime / Journal / Worktree**：`/ultracode` run 会写 `.demiurge/workflow-runs/<run_id>/journal.jsonl`，`/workflows` 列 run，`/workflow resume <run_id>` 用 journal 生成恢复 overlay；`.demiurge/workflows/*.json` 由 `agent/workflow_runtime.rs` 执行，支持 `log` / `phase` / `agent` / `parallel` / `pipeline` / `budget` step，运行状态经 `workflow-updated` 推给 Workflows 面板；`worktree_create` 可在沙盒 Git 仓库下创建隔离 worktree。
- **shell**：标为 confirm + serial，只能在沙盒内的工作目录启动短时 shell 进程；执行前展示命令/cwd/超时，运行时限制超时与输出长度。
- **open_path**：标为 confirm（执行前用户确认），并拒绝 UNC 路径与非安全 URL 协议（只放行 http/https/file/mailto）。
- **权限门**：confirm 类工具经 `oneshot` 通道等待前端裁决；确认弹窗支持「仅本次 / 本会话 / 本项目」allow/deny。`Session` 规则只存在内存中，`Project` 规则保存到 app data 的 `permissions.json`，每次默认放行、命中规则或用户裁决都会向 `permission_audit.jsonl` 追加轻量审计记录（不写完整工具参数）。`interrupt` 会立即唤醒所有待确认项按拒绝处理。
- **项目上下文 / 记忆**：`DEMIURGE.md` / `CLAUDE.md` / memory 文件只从沙盒或当前角色包目录加载，单文件大小受限；读取失败不会中断 Agent 回合。自动记忆提取只追加写入沙盒 `.demiurge/memory.md`，且提取失败会静默跳过。
- **API Key**：MVP 以明文存 `settings.json`（仅本机）。后续可改 Windows 凭据管理器（keyring）。

## 前端（`src/`）
- `App.tsx`：编排——持有状态、注册事件、渲染布局。把后端事件流翻译成展示项 `DisplayItem[]`。
- `components/`：`Sidebar` / `Composer` / `MessageList`（含用户气泡、助手消息、思考态）/ `Markdown`（GFM + KaTeX + 代码块复制 + 流式防闪烁）/ `ToolCard` / `ConfirmDialog` / `SettingsDialog` / `WorkflowsPanel` / `Icons`。
- `lib/api.ts`：Tauri 命令 + 事件订阅的类型化封装。`lib/types.ts`：与 Rust 对应的类型。
- 视觉为 ChatGPT 风浅色主题（`src/style.css`）。

## 构建
- 标准流程：`npm install` → `npm run tauri dev` / `npm run tauri build`。克隆即可独立编译，无需额外配置。
- （本地可选加速）`src-tauri/.cargo/config.toml` 可把 `target-dir` 指向另一个 Tauri 项目的 target，
  复用其已编译依赖树省一次冷编译。该文件含绝对路径、**仅本机有效，已加入 `.gitignore` 不入库**——
  公开仓库的克隆者走标准独立编译，不受影响。
- crate 源码本就全局缓存在 `~/.cargo/registry`，跨项目共享；只有「已编译产物」需共享 target-dir 才能复用。

## 如何扩展

**加一个工具**：在 `tools/mod.rs` 的 `registry()` 加一项（名称/描述/JSON Schema，并补齐 `ToolRisk` / `ToolConcurrency` / `PermissionPolicy` / `ToolOutputPolicy`），在 `execute()` 加一条分支，新建 `tools/<name>.rs` 实现。confirm 类会自动走确认门。

**加一个角色包**：在数据目录 `packs/<id>/` 放 `manifest.json` + `persona.md`（见设计文档格式），在应用里选择即可。

**换 LLM 端点**：设置里先选 `provider`，再填写对应的 `base_url` + `model` + `api_key`。OpenAI-compatible 适合 DeepSeek 等兼容端点；local 适合 LM Studio / Ollama OpenAI-compatible / vLLM 且 API Key 可为空；Anthropic 默认可用 `https://api.anthropic.com/v1`；Gemini 默认可用 `https://generativelanguage.googleapis.com/v1beta`。
