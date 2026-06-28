# 实现说明

本文面向协作者，说明 Demiurge 的项目结构、核心数据流、后端模块、前端模块、安全边界和扩展方式。路线图见 [TODO.md](./TODO.md)，设计背景见 [demiurge-mvp-design.md](./demiurge-mvp-design.md)。

## 总览

Demiurge 是一个 Tauri 桌面应用。前端负责展示和交互，Rust 后端负责 Agent 循环、上下文工程、工具执行、权限控制、持久化和 provider 适配。

```text
React UI
  ├─ invoke: send / settings / sessions / workflow / memory / permission / plan / MCP / WebDAV / OCR / voice commands
  └─ listen: assistant/tool/agent-event/session-engine/confirm/goal/workflow/plan events
        │
        ▼
Rust AppState
  ├─ agent runner + session engine
  ├─ prompt/context/memory/goal
  ├─ tool registry + permission gate
  ├─ provider adapters
  └─ session/settings/keyring persistence
        │
        ▼
LLM endpoint / local tools / OS integrations
```

一次普通对话回合：

```text
用户输入
  -> lib.rs::send
  -> session_engine::begin_turn
  -> agent::run_turn_with_options
  -> prompt::build + budget::history_budget + context trimming
  -> llm::stream_completion
  -> optional tool_calls
  -> permission check + tool execution
  -> tool results fed back to model
  -> final assistant answer
  -> memory extraction
  -> optional goal continuation
  -> session_engine::finish_turn
```

## 项目结构

```text
Demiurge/
├─ README.md
├─ package.json
├─ vite.config.ts
├─ src/
│  ├─ App.tsx
│  ├─ main.tsx
│  ├─ style.css
│  ├─ components/
│  │  ├─ Composer.tsx
│  │  ├─ ConfirmDialog.tsx
│  │  ├─ Markdown.tsx
│  │  ├─ MessageList.tsx
│  │  ├─ SettingsDialog.tsx
│  │  ├─ Sidebar.tsx
│  │  ├─ ToolCard.tsx
│  │  └─ WorkflowsPanel.tsx
│  └─ lib/
│     ├─ api.ts
│     └─ types.ts
├─ src-tauri/
│  ├─ Cargo.toml
│  └─ src/
│     ├─ lib.rs
│     ├─ main.rs
│     ├─ credentials.rs
│     ├─ ocr.rs
│     ├─ voice.rs
│     ├─ agent/
│     ├─ llm/
│     ├─ pack/
│     ├─ permission/
│     ├─ store/
│     └─ tools/
├─ docs/
│  ├─ IMPLEMENTATION.md
│  ├─ TODO.md
│  ├─ demiurge-mvp-design.md
│  ├─ goal-continuous-driving.md
│  ├─ ultracode-agent-orchestration.md
│  └─ workflow-json-dsl.md
└─ packs/
   └─ default/
```

## Rust 后端模块

| 模块 | 职责 | 关键入口 |
|---|---|---|
| `lib.rs` | Tauri command 注册、全局 `AppState`、应用初始化、`send` 分发 | `run()` / `send()` |
| `credentials.rs` | keyring 凭据读写，避免 LLM/Web Search/WebDAV/MCP env 密钥落入 settings 明文 | `hydrate_or_migrate_settings()` / `save_mcp_env_secrets()` |
| `ocr.rs` | OCR 模型路径、模型源、缺模型检查、OCR 推理入口 | `ensure_models()` |
| `voice.rs` | TTS/ASR command surface 预留，设置可见但后端未接入 | `voice_status()` |
| `agent/session_engine.rs` | turn runtime state、入口互斥、中断标记、统一 agent event envelope 和会话写入封装 | `begin_turn()` / `finish_turn()` / `TurnEventEmitter` / `SessionTurnStore` |
| `agent/runner.rs` | Agent loop，处理模型流、tool calls、tool results、最终回答 | `run_turn()` / `run_turn_with_options()` |
| `agent/conversation.rs` | 内部消息结构和 tool call/result 表示 | `Message` / `ToolCall` |
| `agent/prompt.rs` | system prompt 分区组装 | `build()` |
| `agent/budget.rs` | 启发式 token 预算、provider usage 汇总、profile-aware history budget | `history_budget_for_profile()` / `TokenBudgetState` |
| `agent/context.rs` | 历史裁剪，保留最近上下文并返回可摘要旧消息 | `trim_collect_removed_by_tokens()` |
| `agent/summary.rs` | rolling summary 更新 | `update_session_summary()` |
| `agent/memory.rs` | 长期记忆提取、审计面板、编辑/删除/去重 | `extract_and_update()` / `panel_state()` |
| `agent/custom.rs` | `.demiurge/agents/*.json` 自定义 Agent / team 发现、校验、合并 | `resolve_selected()` / `load_agent()` |
| `agent/dream.rs` | `/dream` 记忆整理 | `handle_slash()` |
| `agent/collapse.rs` | `/compact` 与上下文折叠工具 | `inspect()` / `compact_active_session()` |
| `agent/goal.rs` | `/goal`、持续目标状态、预算、续跑和阻塞判定 | `handle_slash()` / `drive_after_turn()` |
| `agent/subagent.rs` | 只读子 Agent、fork/recent/brief context、evidence packet、多 reviewer、硬预算 | `run()` |
| `agent/ultracode.rs` | `/ultracode` 临时编排 overlay | `overlay()` |
| `agent/workflow_journal.rs` | workflow JSONL journal 和 resume overlay | `append()` / `resume_overlay()` |
| `agent/workflow_runtime.rs` | JSON workflow DSL 执行、live panel 状态、durable run snapshot 写入和启动水合 | `launch()` / `run_launched()` / `hydrate_persisted_runs()` |
| `llm/*` | OpenAI-compatible/local/Anthropic/Gemini provider adapters | `stream_completion()` |
| `mcp/mod.rs` | stdio MCP Manager、server lifecycle、tool/resource discovery、resource read、动态 tool definition 与调用分发 | `ensure_initialized()` / `call_tool()` / `read_resource()` |
| `tools/mod.rs` | 工具注册表、schema 输出、权限 metadata、统一执行入口 | `registry()` / `execute()` |
| `permission/mod.rs` | 权限模式决策、confirm 工具的前后端确认往返、权限审计 | `decide_for_mode()` / `confirm()` |
| `pack/mod.rs` | 角色包加载和默认包落地 | `load_pack()` |
| `store/mod.rs` | settings、sessions、权限规则等持久化 | `Settings` / `SessionStore` |

## 前端模块

| 模块 | 职责 |
|---|---|
| `src/App.tsx` | 主状态编排，订阅后端事件，维护消息流、设置、会话、Agent 选择、Plan Mode 控制、backend-driven busy/cancel 状态和 workflow panel |
| `src/lib/api.ts` | Tauri invoke/event 的 typed wrapper，包含 `session_engine_state`、`session-engine-updated` 和统一 `agent-event` |
| `src/lib/types.ts` | 前后端共享 TypeScript 类型 |
| `src/lib/fileProcessing.ts` | 附件读取与提示词拼接辅助 |
| `components/Sidebar.tsx` | 会话列表、会话重命名/删除、角色包选择、基础入口 |
| `components/Composer.tsx` | 输入框、中断/发送状态 |
| `components/MessageList.tsx` | 用户消息、助手消息、工具卡片渲染 |
| `components/Markdown.tsx` | GFM、代码块、KaTeX 渲染 |
| `components/ToolCard.tsx` | tool-start/tool-end 展示，包含 MCP tool/resource 进度摘要 |
| `components/ConfirmDialog.tsx` | 敏感工具确认，支持 once/session/project scope |
| `components/SettingsDialog.tsx` | provider、Web Search、MCP server、OCR、语音、WebDAV、权限、记忆审计设置 |
| `components/WorkflowsPanel.tsx` | workflow 定义、run/stop、agent、phase、log 的 live 状态 |

## 运行数据目录

应用数据目录由 Tauri `app_data_dir` 决定。主要内容：

```text
app_data_dir/
├─ settings.json                 # 非密钥设置
├─ sessions.json                 # 多会话、active session、rolling summary、goal state
├─ permissions.json              # 项目级权限规则
├─ permission_audit.jsonl        # 轻量权限审计
├─ sandbox/                      # 文件工具可访问的工作区
├─ packs/                        # 用户角色包
├─ ocr-models/                   # OCR 模型
└─ sandbox/.demiurge/
   ├─ memory.md                  # 自动长期记忆
   ├─ agents/*.json              # 自定义 Agent / team 定义
   ├─ plans/*.md                 # Plan Mode 生成的待批准实施计划
   ├─ workflows/*.json           # workflow 定义
   ├─ workflow-runs/*/journal.jsonl
   ├─ workflow-runs/*/state.json # durable workflow run snapshot
   └─ screenshots/               # 截图和 OCR 中间文件
```

API Key、WebDAV 密码和 MCP secret env/token 存在系统凭据管理器中，不写入 `settings.json` 或 WebDAV 备份。兼容迁移会读取旧 settings 明文字段并转存到 keyring；运行时 `Settings` 会被水合出内存态 secret，供 provider adapter 和 MCP stdio server 启动使用。

## Agent 循环

1. `send` 捕获当前 active session id，并通过 `session_engine::begin_turn` 建立 turn runtime state、入口互斥、input preview、agent/workflow metadata 和中断标记，避免用户中途切换会话导致写入串台。
2. slash command 先分流，例如 `/goal`、`/compact`、`/dream`、`/ultracode`、`/workflows`、`/workflow resume <run_id>`。
3. 普通回合调用 `run_turn_with_options`；runner 使用 `SessionTurnStore` 统一读取、追加、替换 session messages 与 rolling summary，并在每次变更后持久化；`send_with_agents` 会把前端选中的自定义 Agent 合并成 prompt overlay、工具限制和预算限制。
4. `prompt::build` 组装 engine、persona、project instructions、environment、goal、summary、memory；如果 `settings.permission_mode == plan`，runner 额外注入 Plan Mode overlay，要求只读探索并用 `write_plan` 生成实施计划。
5. `budget` 和 `context` 按预算裁剪历史。
6. provider adapter 发起流式请求。
7. 如果模型返回 tool calls，后端执行工具并把 tool result 写回历史，再进入下一轮模型请求。
8. assistant/tool 事件统一通过 `TurnEventEmitter` 发出；前端仍接收 legacy `assistant-*` / `tool-*` 事件，同时可消费带 turn context 的 `agent-event`。
9. 如果模型给出最终回答，触发 `assistant-done`，随后尝试记忆提取。
10. 如果当前 session 有 active goal，则 `goal::drive_after_turn` 继续调度下一轮，直到目标完成、暂停、阻塞、预算限制、max turns 或中断。
11. 回合退出时 `session_engine::finish_turn` 将 active turn 移入 last turn，并通过 `session-engine-updated` 推送后端 busy/cancel 状态；`interrupt` 通过 `request_interrupt` 把当前 turn 标记为 `cancelling`。

## Provider Capability Profile

`llm/mod.rs` 中的 `ProviderProfile` 是 provider 能力的单一入口，由 `ProviderProfile::for_kind(settings.provider)` 解析当前 provider。它统一描述：

- tool schema dialect：OpenAI-compatible / Anthropic / Gemini。
- structured output dialect：OpenAI `response_format`、Anthropic forced tool schema、Gemini `responseSchema`。
- prompt cache、thinking、parallel tool calls 的 provider 能力样式；没有对应请求选项时保持显式建模但默认不启用请求字段。
- provider token budget：已知 provider 的 input/output 上限会通过 `effective_token_budget()` clamp 用户设置；未知 OpenAI-compatible/custom endpoint 保留用户设置。

请求构造保持分层：runner/subagent/budget 只读取 profile helper，不做 provider-specific match；provider-specific JSON 仍留在 `llm/openai.rs`、`llm/anthropic.rs`、`llm/gemini.rs`。`openai.rs` 负责 `max_tokens`、`parallel_tool_calls`、`response_format`；`anthropic.rs` 负责 `max_tokens`、tool/tool_choice 形态；`gemini.rs` 负责 `generationConfig.maxOutputTokens`、`responseMimeType`、`responseSchema`。

## 上下文工程

system prompt 由多个 section 组成：

- 引擎规则：工具、安全、输出约束。
- 角色设定：当前角色包 `persona.md`。
- 项目指令：沙盒根 `DEMIURGE.md` / `CLAUDE.md`。
- 运行环境：时间戳、沙盒路径、角色包 id、git status 摘要。
- 当前目标：goal objective、status、budget、tokens used、active time。
- 会话摘要：rolling summary。
- 记忆：沙盒 memory、角色包 memory。

上下文压力处理：

- 先压缩或截断老工具输出。
- 再裁剪更旧对话。
- 被裁剪的旧消息会进入 rolling summary。
- `/compact` 可以手动触发。
- `context_inspect` / `context_collapse` 可以由模型触发。

## 工具系统

工具定义集中在 `tools/mod.rs`：

- `name`
- `description`
- `parameters`
- `risk`
- `concurrency`
- `permission`
- `output_policy`

主 schema 只放 core tools。截图、OCR、open_path 等低频工具留在 deferred pool，通过 `tool_search` 发现，再由 `execute_tool` 代理执行。这样可以减少固定 tools JSON 对上下文的占用。

MCP 工具是运行时动态注册的：`agent::runner` 在生成工具 schema 前调用 `mcp::ensure_initialized`，随后 `tools::registry_for_state` 把已连接 server 的 `tools/list` 结果追加为 `mcp__server__tool`。模型调用这些动态工具时，`tools::execute` 直接分发到 `mcp::call_tool`。MCP resources 通过 Settings 面板展示，并通过 core tool `mcp_read_resource` 调用 `resources/read`。

当前核心工具：

- 文件：`read_file`、`write_plan`、`write_file`、`edit_file`、`multi_edit`、`apply_patch`、`undo_edit`
- 搜索导航：`glob`、`grep`、`git_status`
- 执行：`shell`
- 联网：`web_search`、`web_fetch`
- MCP：`mcp_read_resource`，以及运行时动态发现的 `mcp__server__tool`
- 多 Agent：`agent_spawn`
- 上下文：`context_inspect`、`context_collapse`
- 目标：`goal`
- deferred：`open_path`、screen capture、OCR
- workflow/worktree：`worktree_create`

## 安全模型

- `PermissionMode` 支持 `plan` / `default` / `auto` / `bypass`：`default` 走工具默认策略与用户规则；`auto` 自动允许只读工具；`bypass` 跳过确认但仍审计；`plan` 未批准前只允许只读工具和受限 `write_plan`。
- Plan Mode 的计划状态在 `AppState.plan_state` 中维护；`write_plan` 只能写入沙盒 `.demiurge/plans/`，前端通过 `approve_plan` 批准后自动回到 `default` 执行模式。
- 文件工具只能访问沙盒目录。
- 路径先做词法校验，再对最近存在祖先做 canonicalize，防止符号链接和 junction 逃逸。
- 写入、shell、open_path、截图/OCR 等操作走确认门。
- confirm 支持 once/session/project scope。
- `interrupt` 会唤醒所有待确认项并按拒绝处理。
- shell 限制 cwd、timeout 和 output cap。
- MCP 第一阶段仅支持本地 stdio server；server command/env 来自设置页，secret-like env 写入 keyring，`settings.json` 和备份只保留空值。
- MCP 动态工具默认按 annotation 映射风险，执行前接入现有权限确认与审计；`mcp_read_resource` 按外部资源读取处理。
- 子 Agent 只暴露只读工具。
- 权限审计不写完整敏感参数。

## Goal 持续驱动

Goal state 存在当前 session 中。用户通过 `/goal <objective>` 设置目标，可附带 token budget，例如 `/goal 修复构建 +500k`。

状态包括：

- `active`
- `paused`
- `blocked`
- `budget_limited`
- `usage_limited`
- `max_turns`
- `complete`

模型只能通过 `goal` 工具读取状态、标记 complete，或报告 blocked。相同阻塞原因连续出现 3 次后才会真正进入 blocked。

## Workflow JSON DSL

workflow 定义放在沙盒 `.demiurge/workflows/*.json`。运行时支持：

- `log`
- `phase`
- `agent`
- `parallel`
- `pipeline`
- `budget`

运行状态通过 `workflow-updated` 推送到前端，同时写入 `.demiurge/workflow-runs/<run_id>/journal.jsonl` 和 `.demiurge/workflow-runs/<run_id>/state.json`。`journal.jsonl` 保留事件 tail，用于恢复上下文；`state.json` 保存当前 run status、取消请求、phase、agent 进度、预算和 step 计数，用于跨进程水合。

启动时和 Workflows 面板读取时，后端会把 `state.json` 合并回 live panel state。上一个进程仍处于 `running` 的 run 会恢复为 `stale_running`，表示状态、预算和进度可见，但没有 live task 附着；如果 snapshot 中已有取消请求，则恢复为 `killed`。`/workflows` 使用 runtime panel state 输出这些 durable 状态。

`/workflow resume <run_id>` 优先从 journal 生成恢复 overlay；如果 journal 不可读但 `state.json` 存在，则 fallback 到 durable snapshot，把 snapshot 作为恢复依据交给下一轮 agent，避免重复已完成步骤。

## Web Search / Fetch

`web_search` 参数支持：

- `query`
- `allowed_domains`
- `blocked_domains`
- `num_results`
- `context_max_characters`
- `source`
- `livecrawl`
- `search_type`

`source` 可选：

- `auto`
- `bing`
- `duckduckgo`
- `tavily`
- `brave`
- `exa`

`web_fetch` 用于单 URL 抓取，支持 `source=direct|exa`、`context_max_characters` 和 Exa `livecrawl=fallback|always|never`。`web_search` 的 Exa adapter 使用同一组 `livecrawl` 策略，并支持 `search_type=auto|fast|deep`。

外部 adapter 环境变量与 keyring：

- `WEB_SEARCH_ADAPTER`
- `TAVILY_SEARCH_URL` / `TAVILY_ENDPOINT_URL` / `TAVILY_API_KEY`
- `BRAVE_SEARCH_API_KEY` / `BRAVE_API_KEY`
- `EXA_MCP_URL` / `EXA_API_KEY`

Tavily、Brave、Exa key 优先从 settings/keyring 水合，也保留环境变量 fallback。

## 扩展方式

### 新增工具

1. 在 `src-tauri/src/tools/<name>.rs` 实现工具逻辑。
2. 在 `tools/mod.rs` 增加 `mod <name>;`。
3. 在 `registry()` 注册 tool definition。
4. 在 `execute()` 增加分支。
5. 按风险选择 `PermissionPolicy::allow` 或 `PermissionPolicy::ask`。
6. 为解析和安全边界添加单元测试。

### 新增 provider

1. 在 `src-tauri/src/llm/` 增加 adapter，或复用 OpenAI-compatible adapter。
2. 在 `store::ProviderKind` 中增加 provider kind，并在设置 UI / `store::Settings` 中补字段或默认值。
3. 在 `llm/mod.rs::ProviderProfile::for_kind` 中声明 tool/schema dialect、prompt cache、thinking、parallel tool calls、structured output 和 token budget 上限。
4. 实现请求体构造、SSE/stream 解析、tool call 转换；provider-specific JSON 只放在对应 adapter 文件。
5. 为 profile mapping、budget clamp 和 body builder 添加单元测试。

### 新增角色包

在应用数据目录 `packs/<id>/` 下放：

```text
manifest.json
persona.md
memory.md        # 可选
```

`manifest.json` 至少包含 `id`、`name`、`persona`。

## 构建与验证

开发运行：

```bash
npm run tauri dev
```

前端构建：

```bash
npm run build
```

Rust 测试：

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Tauri 打包：

```bash
npm run tauri build
```
