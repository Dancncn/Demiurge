# 实现说明

本文面向协作者，说明 Demiurge 的项目结构、核心数据流、后端模块、前端模块、安全边界和扩展方式。路线图见 [TODO.md](./TODO.md)，设计背景见 [demiurge-mvp-design.md](./demiurge-mvp-design.md)。

## 总览

Demiurge 是一个 Tauri 桌面应用。前端负责展示和交互，Rust 后端负责 Agent 循环、上下文工程、工具执行、权限控制、持久化和 provider 适配。

```text
React UI
  ├─ invoke: send / settings / sessions / workflow commands
  └─ listen: assistant/tool/confirm/workflow events
        │
        ▼
Rust AppState
  ├─ agent runner
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
  -> agent::run_turn_with_options
  -> prompt::build + budget::history_budget + context trimming
  -> llm::stream_completion
  -> optional tool_calls
  -> permission check + tool execution
  -> tool results fed back to model
  -> final assistant answer
  -> memory extraction
  -> optional goal continuation
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
| `credentials.rs` | keyring 凭据读写，避免 LLM key 落入 settings 明文 | `load_api_key()` / `save_api_key()` |
| `ocr.rs` | OCR 模型路径、模型源、缺模型检查、OCR 推理入口 | `ensure_models()` |
| `voice.rs` | TTS/ASR adapter 预留接口 | adapter traits |
| `agent/runner.rs` | Agent loop，处理模型流、tool calls、tool results、最终回答 | `run_turn()` / `run_turn_with_options()` |
| `agent/conversation.rs` | 内部消息结构和 tool call/result 表示 | `Message` / `ToolCall` |
| `agent/prompt.rs` | system prompt 分区组装 | `build()` |
| `agent/budget.rs` | 启发式 token 预算和 history budget | `history_budget()` |
| `agent/context.rs` | 历史裁剪，保留最近上下文并返回可摘要旧消息 | `trim_collect_removed_by_tokens()` |
| `agent/summary.rs` | rolling summary 更新 | `update_session_summary()` |
| `agent/memory.rs` | 长期记忆提取并写入 `.demiurge/memory.md` | `extract_and_update()` |
| `agent/dream.rs` | `/dream` 记忆整理 | `handle_slash()` |
| `agent/collapse.rs` | `/compact` 与上下文折叠工具 | `inspect()` / `compact_active_session()` |
| `agent/goal.rs` | `/goal`、持续目标状态、预算、续跑和阻塞判定 | `handle_slash()` / `drive_after_turn()` |
| `agent/subagent.rs` | 只读子 Agent、fork/recent/brief context | `run()` |
| `agent/ultracode.rs` | `/ultracode` 临时编排 overlay | `handle_slash()` |
| `agent/workflow_journal.rs` | workflow JSONL journal 和 resume overlay | `append()` / `resume_overlay()` |
| `agent/workflow_runtime.rs` | JSON workflow DSL 执行与 live panel 状态 | `launch()` / `run_launched()` |
| `llm/*` | OpenAI-compatible/local/Anthropic/Gemini provider adapters | `stream_completion()` |
| `tools/mod.rs` | 工具注册表、schema 输出、权限 metadata、统一执行入口 | `registry()` / `execute()` |
| `permission/mod.rs` | confirm 工具的前后端确认往返 | `confirm()` |
| `pack/mod.rs` | 角色包加载和默认包落地 | `load_pack()` |
| `store/mod.rs` | settings、sessions、权限规则等持久化 | `Settings` / `SessionStore` |

## 前端模块

| 模块 | 职责 |
|---|---|
| `src/App.tsx` | 主状态编排，订阅后端事件，维护消息流、设置、会话和 workflow panel |
| `src/lib/api.ts` | Tauri invoke/event 的 typed wrapper |
| `src/lib/types.ts` | 前后端共享 TypeScript 类型 |
| `components/Sidebar.tsx` | 会话列表、角色包选择、基础入口 |
| `components/Composer.tsx` | 输入框、中断/发送状态 |
| `components/MessageList.tsx` | 用户消息、助手消息、工具卡片渲染 |
| `components/Markdown.tsx` | GFM、代码块、KaTeX 渲染 |
| `components/ToolCard.tsx` | tool-start/tool-end 展示 |
| `components/ConfirmDialog.tsx` | 敏感工具确认 |
| `components/SettingsDialog.tsx` | provider、model、base_url、key 设置 |
| `components/WorkflowsPanel.tsx` | workflow 定义、run、agent、log 的 live 状态 |

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
   ├─ workflows/*.json           # workflow 定义
   ├─ workflow-runs/*/journal.jsonl
   └─ screenshots/               # 截图和 OCR 中间文件
```

API Key 存在系统凭据管理器中，不写入 `settings.json`。Web Search 外部 adapter key 当前通过环境变量读取。

## Agent 循环

1. `send` 捕获当前 active session id，避免用户中途切换会话导致写入串台。
2. slash command 先分流，例如 `/goal`、`/compact`、`/dream`、`/ultracode`、workflow 命令。
3. 普通回合调用 `run_turn_with_options`，把用户消息写入 session。
4. `prompt::build` 组装 engine、persona、project instructions、environment、goal、summary、memory。
5. `budget` 和 `context` 按预算裁剪历史。
6. provider adapter 发起流式请求。
7. 如果模型返回 tool calls，后端执行工具并把 tool result 写回历史，再进入下一轮模型请求。
8. 如果模型给出最终回答，触发 `assistant-done`，随后尝试记忆提取。
9. 如果当前 session 有 active goal，则 `goal::drive_after_turn` 继续调度下一轮，直到目标完成、暂停、阻塞、预算限制、max turns 或中断。

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

当前核心工具：

- 文件：`read_file`、`write_file`、`edit_file`、`multi_edit`、`apply_patch`、`undo_edit`
- 搜索导航：`glob`、`grep`、`git_status`
- 执行：`shell`
- 联网：`web_search`
- 多 Agent：`agent_spawn`
- 上下文：`context_inspect`、`context_collapse`
- 目标：`goal`
- deferred：`open_path`、screen capture、OCR
- workflow/worktree：`worktree_create`

## 安全模型

- 文件工具只能访问沙盒目录。
- 路径先做词法校验，再对最近存在祖先做 canonicalize，防止符号链接和 junction 逃逸。
- 写入、shell、open_path、截图/OCR 等操作走确认门。
- confirm 支持 once/session/project scope。
- `interrupt` 会唤醒所有待确认项并按拒绝处理。
- shell 限制 cwd、timeout 和 output cap。
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

运行状态通过 `workflow-updated` 推送到前端。journal 写入 `.demiurge/workflow-runs/<run_id>/journal.jsonl`，可通过 `/workflow resume <run_id>` 生成恢复 overlay。

## Web Search

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

外部 adapter 环境变量：

- `WEB_SEARCH_ADAPTER`
- `TAVILY_SEARCH_URL` / `TAVILY_ENDPOINT_URL` / `TAVILY_API_KEY`
- `BRAVE_SEARCH_API_KEY` / `BRAVE_API_KEY`
- `EXA_MCP_URL` / `EXA_API_KEY`

## 扩展方式

### 新增工具

1. 在 `src-tauri/src/tools/<name>.rs` 实现工具逻辑。
2. 在 `tools/mod.rs` 增加 `mod <name>;`。
3. 在 `registry()` 注册 tool definition。
4. 在 `execute()` 增加分支。
5. 按风险选择 `PermissionPolicy::allow` 或 `PermissionPolicy::ask`。
6. 为解析和安全边界添加单元测试。

### 新增 provider

1. 在 `src-tauri/src/llm/` 增加 adapter。
2. 实现请求体构造、SSE/stream 解析、tool call 转换。
3. 在 `llm/mod.rs` 增加 provider kind 和 dispatch。
4. 在设置 UI 和 `store::Settings` 中补字段或默认值。

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
