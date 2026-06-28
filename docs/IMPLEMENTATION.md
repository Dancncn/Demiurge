# 实现说明

本文面向协作者，说明 Demiurge 的项目结构、核心数据流、后端模块、前端模块、安全边界和扩展方式。路线图见 [TODO.md](./TODO.md)，设计背景见 [demiurge-mvp-design.md](./demiurge-mvp-design.md)。

## 总览

Demiurge 是一个 Tauri 桌面应用。前端负责展示和交互，Rust 后端负责 Agent 循环、上下文工程、工具执行、权限控制、持久化和 provider 适配。

```text
React UI
  ├─ invoke: send / settings / connection tests / sessions / workflow / memory / permission / plan / MCP / WebDAV / OCR / voice commands
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
│  ├─ ocr-models.md
│  ├─ ultracode-agent-orchestration.md
│  └─ workflow-json-dsl.md
└─ packs/
   └─ default/
```

## Rust 后端模块

| 模块 | 职责 | 关键入口 |
|---|---|---|
| `lib.rs` | Tauri command 注册、全局 `AppState`、应用初始化、`send` 分发和上下文面板聚合 | `run()` / `send()` / `context_panel_state()` |
| `connection_tests.rs` | Settings 连接测试；用当前表单设置验证 LLM Provider、Web Search 和 WebDAV 以外的网络 key，不要求先保存密钥 | `test_provider()` / `test_web_search()` |
| `credentials.rs` | keyring 凭据读写，避免 LLM/Web Search/WebDAV/MCP env 密钥落入 settings 明文 | `hydrate_or_migrate_settings()` / `save_mcp_env_secrets()` |
| `ocr.rs` | OCR 模型路径、ModelScope/Hugging Face 源、下载进度事件、缺模型检查、手动安装提示和 OCR 推理入口 | `model_status()` / `download_models()` / `recognize_rgba()` |
| `voice.rs` | TTS/ASR command surface 预留，设置可见但后端未接入 | `voice_status()` |
| `agent/session_engine.rs` | turn runtime state、入口互斥、中断标记、统一 agent event envelope 和会话写入封装 | `begin_turn()` / `finish_turn()` / `TurnEventEmitter` / `SessionTurnStore` |
| `agent/runner.rs` | Agent loop，处理模型流、tool calls、tool results、最终回答 | `run_turn()` / `run_turn_with_options()` |
| `agent/conversation.rs` | 内部消息结构和 tool call/result 表示 | `Message` / `ToolCall` |
| `agent/prompt.rs` | system prompt 分区组装，注入 persona、skills、instructions、scoped memories、summary、environment、tools 和 safety sections | `build_for_input()` / `build_with_report()` |
| `agent/budget.rs` | 启发式 token 预算、provider usage 汇总、profile-aware history budget | `history_budget_for_profile()` / `TokenBudgetState` |
| `agent/context.rs` | 历史裁剪，保留最近上下文并返回可摘要旧消息 | `trim_collect_removed_by_tokens()` |
| `agent/summary.rs` | rolling summary 更新 | `update_session_summary()` |
| `agent/skills.rs` | Markdown skills 发现、frontmatter 解析、自动选择、slash 输出、references 注入和 Settings panel 摘要 | `build_context()` / `handle_slash()` / `panel_state()` |
| `agent/memory.rs` | 长期记忆提取、user/project/session/pack 分层记忆、审计面板、新增/编辑/删除/去重 | `extract_and_update()` / `panel_state()` / `add_entry()` |
| `agent/custom.rs` | `.demiurge/agents/*.json` 自定义 Agent / team 发现、校验、合并 | `resolve_selected()` / `load_agent()` |
| `agent/dream.rs` | `/dream` 记忆整理 | `handle_slash()` |
| `agent/collapse.rs` | `/compact` 与上下文折叠工具 | `inspect()` / `compact_active_session()` |
| `agent/goal.rs` | `/goal`、持续目标状态、预算、续跑和阻塞判定 | `handle_slash()` / `drive_after_turn()` |
| `agent/subagent.rs` | 只读子 Agent、fork/recent/brief context、evidence packet、多 reviewer、硬预算 | `run()` |
| `agent/ultracode.rs` | `/ultracode` 临时编排 overlay | `overlay()` |
| `agent/workflow_journal.rs` | workflow JSONL journal 和 resume overlay | `append()` / `resume_overlay()` |
| `agent/workflow_runtime.rs` | JSON workflow DSL 执行、live panel 状态、durable run snapshot 写入和启动水合 | `launch()` / `run_launched()` / `hydrate_persisted_runs()` |
| `llm/*` | OpenAI-compatible/local/Anthropic/Gemini provider adapters；profile adapter routing、schema dialect、token clamp、usage 和 finish reason 归一化 | `stream_completion()` / `ProviderProfile::for_kind()` |
| `mcp/mod.rs` | stdio MCP Manager、server lifecycle、tool/resource discovery、resource read、动态 tool definition 与调用分发 | `ensure_initialized()` / `call_tool()` / `read_resource()` |
| `tools/mod.rs` | 工具注册表、schema 输出、权限 metadata、统一执行入口 | `registry()` / `execute()` |
| `tools/list_dir.rs` | 沙盒目录直接子项枚举，按 dir/file/other 排序，默认隐藏 dotfile 并支持数量截断 | `run()` |
| `tools/http_get.rs` | 轻量公开 http/https GET，返回状态、content-type、最终 URL 和截断正文 | `run()` |
| `tools/clipboard.rs` | 读取系统剪贴板文本并截断输出；按特权工具处理，执行前确认 | `run()` |
| `tools/package_scripts.rs` | 读取沙盒 `package.json` scripts，检测包管理器并生成建议 shell 命令；不直接执行脚本 | `run()` |
| `tools/web_common.rs` | Web Search / Fetch 共享 JSON/SSE 解析、HTML/text 清洗、source markdown 输出、source-quality 计数和 Exa MCP 调用外壳 | `parse_json_payloads()` / `append_source_lines()` / `call_exa_mcp()` |
| `tools/shell.rs` | shell 风险分类、policy state、standard/strict/sandboxed isolation、平台 process containment 和 sandbox wrapper | `run()` / `preview()` / `policy_state()` |
| `permission/mod.rs` | 权限模式决策、confirm 工具的前后端确认往返、权限审计 | `decide_for_mode()` / `confirm()` |
| `pack/mod.rs` | 角色包加载、manifest 校验、头像 data URL 读取、zip 导入校验与默认包落地 | `list_packs()` / `load_pack()` / `import_zip()` |
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
| `components/SettingsDialog.tsx` | provider、Persona Pack zip 导入、Web Search、MCP server、OCR 模型源/下载进度/缺模型引导、语音、WebDAV、权限、分层记忆维护和 Context 可视化设置，以及 Provider/Web Search/WebDAV 连接测试 |
| `components/WorkflowsPanel.tsx` | workflow 定义、run/stop、agent、phase、log 的 live 状态 |

## 运行数据目录

应用数据目录由 Tauri `app_data_dir` 决定。主要内容：

```text
app_data_dir/
├─ settings.json                 # 非密钥设置
├─ sessions.json                 # 多会话、active session、rolling summary、goal state
├─ permissions.json              # 项目级权限规则
├─ permission_audit.jsonl        # 轻量权限审计
├─ memory/user.md                # user-scope 手动记忆
├─ skills/*/SKILL.md             # global skills
├─ sandbox/                      # 文件工具可访问的工作区
├─ packs/                        # 用户角色包；可包含 pack memory 和 pack skills
├─ ocr-models/                   # OCR 模型
└─ sandbox/.demiurge/
   ├─ memory.md                  # project-scope 分层记忆
   ├─ session-memory/*.md        # session-scope 分层记忆
   ├─ skills/*/SKILL.md          # project skills
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
2. slash command 先分流，例如 `/skills`、`/skill`、`/goal`、`/compact`、`/dream`、`/ultracode`、`/workflows`、`/workflow resume <run_id>`。
3. 普通回合调用 `run_turn_with_options`；runner 使用 `SessionTurnStore` 统一读取、追加、替换 session messages 与 rolling summary，并在每次变更后持久化；`send_with_agents` 会把前端选中的自定义 Agent 合并成 prompt overlay、工具限制和预算限制。
4. `prompt::build_for_input` 组装 engine、persona、skills、project instructions、environment、goal、summary 和 scoped memories；如果 `settings.permission_mode == plan`，runner 额外注入 Plan Mode overlay，要求只读探索并用 `write_plan` 生成实施计划。
5. `budget` 和 `context` 按预算裁剪历史。
6. provider adapter 发起流式请求。
7. 如果模型返回 tool calls，后端执行工具并把 tool result 写回历史，再进入下一轮模型请求。
8. assistant/tool 事件统一通过 `TurnEventEmitter` 发出；前端仍接收 legacy `assistant-*` / `tool-*` 事件，同时可消费带 turn context 的 `agent-event`。
9. 如果模型给出最终回答，触发 `assistant-done`，随后尝试记忆提取。
10. 如果当前 session 有 active goal，则 `goal::drive_after_turn` 继续调度下一轮，直到目标完成、暂停、阻塞、预算限制、max turns 或中断。
11. 回合退出时 `session_engine::finish_turn` 将 active turn 移入 last turn，并通过 `session-engine-updated` 推送后端 busy/cancel 状态；`interrupt` 通过 `request_interrupt` 把当前 turn 标记为 `cancelling`。

## Settings 连接测试

Settings 面板提供三类连接测试：

- `provider_check_connection(settings)`：使用当前表单值直接验证 active LLM provider、`base_url`、`model` 和 LLM key；OpenAI-compatible/local 走 `/chat/completions`，Anthropic 走 `/messages`，Gemini 走 `:generateContent`，请求限制为最小 1 token。
- `web_search_check_connection(settings, provider)`：使用当前表单值验证选中的 Web Search provider；Tavily、Brave、Exa 优先使用表单 key，随后 fallback 到环境变量，Bing/DuckDuckGo 不要求 key。
- `webdav_check_connection(config)`：复用 WebDAV collection 检查和必要时的 `MKCOL` 创建逻辑。

连接测试不调用 `save_settings`，因此用户可以在保存前验证刚输入的 key、base_url 和 model；测试结果返回统一的 `ConnectionTestResult`，前端展示 detail、target 和耗时。

## Provider Capability Profile

`llm/mod.rs` 中的 `ProviderProfile` 是 provider 能力的单一入口，由 `ProviderProfile::for_kind(settings.provider)` 解析当前 provider。它统一描述：

- tool schema dialect：OpenAI-compatible / Anthropic / Gemini。
- adapter kind：OpenAI-compatible/local、Anthropic、Gemini 的实际请求和连接测试路由。
- structured output dialect：OpenAI `response_format`、Anthropic forced tool schema、Gemini `responseSchema`。
- prompt cache、thinking、parallel tool calls 的 provider 能力样式；没有对应请求选项时保持显式建模但默认不启用请求字段。
- provider token budget：已知 provider 的 input/output 上限会通过 `effective_token_budget()` clamp 用户设置；未知 OpenAI-compatible/custom endpoint 保留用户设置。

请求构造保持分层：runner/subagent/budget/connection tests 只读取 profile helper 或 adapter kind，不复制 provider-specific match；provider-specific JSON 仍留在 `llm/openai.rs`、`llm/anthropic.rs`、`llm/gemini.rs`。`openai.rs` 负责 `max_tokens`、`parallel_tool_calls`、`response_format`；`anthropic.rs` 负责 `max_tokens`、tool/tool_choice 形态；`gemini.rs` 负责 `generationConfig.maxOutputTokens`、`responseMimeType`、`responseSchema`。streaming parser 统一通过 `merge_usage()` 合并 provider usage，并通过 `normalize_finish_reason()` 把 OpenAI/Anthropic/Gemini 的 stop、tool_calls、length、content_filter、interrupted 等结束原因归一化给 runner。

## Skills

Skills 是 Markdown 目录能力，不依赖额外运行时。发现顺序覆盖 global、project、repository、pack 和 Claude 兼容目录：

- `app_data_dir/skills/*/SKILL.md`
- `sandbox/.demiurge/skills/*/SKILL.md`
- `sandbox/skills/*/SKILL.md`
- `packs/<pack_id>/skills/*/SKILL.md`
- `sandbox/.claude/skills/*/SKILL.md`

`SKILL.md` 支持 YAML frontmatter：`name`、`description`、`triggers`/`keywords`、`tools`/`declared_tool_needs`、`required_permissions`、`references`、`always_include`。`prompt::build_for_input` 会根据当前 user text 自动选择 always_include 和匹配分最高的 skills，把 skill body、declared tool needs、required permissions 和安全相对 references 注入 system prompt；references 只能读取 skill 目录内的安全相对路径。`/skills` 和 `/skill` 返回当前可发现 skills、scope、匹配分与选中状态，供用户检查推荐结果。

## Memory

Memory 仍以 Markdown 为主，但读写路径已分层：

- user：`app_data_dir/memory/user.md`
- project：`sandbox/.demiurge/memory.md`
- session：`sandbox/.demiurge/session-memory/<session_id>.md`
- pack：`packs/<pack_id>/memory.md`
- project legacy：`sandbox/memory.md` 只读兼容加载

Prompt 会按 user/project/session/pack 加载分层 memory，并继续兼容旧的 project legacy memory。Settings Memory 面板按 scope 展示 path、条目、重复项和统计，支持对 user/project/session/pack 手动新增、编辑 kind/text、删除和去重；自动 memory extraction 仍优先写入 project scope，后续再扩展到更细粒度的自动归档策略。

## 上下文工程

system prompt 由多个 section 组成：

- 引擎规则：工具、安全、输出约束。
- 角色设定：当前角色包 `persona.md`。
- Skills：当前输入匹配或 always_include 的 Markdown skills。
- 项目指令：沙盒根 `DEMIURGE.md` / `CLAUDE.md`。
- 运行环境：时间戳、沙盒路径、角色包 id、git status 摘要。
- 当前目标：goal objective、status、budget、tokens used、active time。
- 会话摘要：rolling summary。
- 记忆：user/project/session/pack scopes，并兼容旧 project memory。

上下文压力处理：

- 先压缩或截断老工具输出。
- 再裁剪更旧对话。
- 被裁剪的旧消息会进入 rolling summary。
- `/compact` 可以手动触发。
- `context_inspect` / `context_collapse` 可以由模型触发。

Settings 的 Context 页通过 `context_panel_state` 展示当前上下文预算和 prompt 组成细节。后端会聚合 system/tools/history/output reserve 的预算占用、history 预算余量与 over-budget 状态、summary 字符数和 token 估算、memory 来源（user/project/session/pack/project legacy）、history role breakdown，以及每个 prompt section 的优先级、字符数、原始字符数、token 估算、是否纳入和是否截断。前端用预算条、role breakdown 表、memory source 卡片和 prompt section 表渲染这些数据，用于判断上下文压力来自哪里。

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

- 文件与目录：`read_file`、`list_dir`、`write_plan`、`write_file`、`edit_file`、`multi_edit`、`apply_patch`、`undo_edit`
- 搜索导航：`glob`、`grep`、`git_status`
- 执行：`shell`（standard / strict / sandboxed isolation）
- 联网：`web_search`、`web_fetch`、`http_get`
- 系统读取：`clipboard`（读取剪贴板需确认）
- 包脚本：`package_scripts`（只读取 scripts 并生成建议 shell 命令，不执行脚本）
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
- shell 限制 cwd、timeout、output cap 和环境变量；所有 shell 子进程使用独立进程组/进程树，超时时终止整棵进程树。
- shell `strict` isolation 强制最小环境白名单，并拒绝联网、依赖安装、破坏性、提权和外部执行类命令；Settings 的 Permission Rules 区域展示 env allowlist、strict deny 风险、命令模式和平台 containment 状态。
- shell `sandboxed` isolation 在 strict 策略基础上要求 OS sandbox wrapper：macOS 使用 `sandbox-exec` profile 限制写入路径并拒绝网络，Linux/WSL 使用 `bubblewrap` 绑定沙盒/临时目录并 `--unshare-net`；Windows 原生明确不支持 filesystem/network sandbox，保留进程树 containment 并 fail closed。
- `clipboard` 按 privileged/ask 处理，避免未确认读取系统剪贴板中的密钥、聊天或临时敏感数据。
- `package_scripts` 只读取沙盒 `package.json` 的 scripts 字段并返回建议 shell 命令；脚本执行仍必须走 `shell` 的确认门和隔离策略。
- MCP 第一阶段仅支持本地 stdio server；server command/env 来自设置页，secret-like env 写入 keyring，`settings.json` 和备份只保留空值。
- MCP 动态工具默认按 annotation 映射风险，执行前接入现有权限确认与审计；`mcp_read_resource` 按外部资源读取处理。
- 子 Agent 只暴露 `SUBAGENT_READONLY_TOOL_NAMES` 中的只读/外部读取工具，不暴露 `shell`、`clipboard` 和写入类工具。
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

`tools/web_common.rs` 承载两个工具共用的边界逻辑：JSON/SSE payload 解析、HTML/text 清洗、URL/title/domain 规范化、统一 `WebSource`、Sources/Links markdown 行生成、source-quality 链接计数，以及 Exa MCP endpoint/key/env fallback/request envelope。`web_search` 和 `web_fetch` 只保留各自 provider 参数组装、结果抽取和 direct fetch 逻辑，避免来源提示、截断和 Exa 边缘行为在两个 adapter 中漂移。

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
3. 在 `llm/mod.rs::ProviderProfile::for_kind` 中声明 adapter kind、tool/schema dialect、prompt cache、thinking、parallel tool calls、structured output 和 token budget 上限。
4. 实现请求体构造、SSE/stream 解析、tool call 转换；provider-specific JSON 只放在对应 adapter 文件，finish reason 走 `normalize_finish_reason()`，usage 走 `merge_usage()`。
5. 为 profile mapping、adapter routing、budget clamp、body builder 和 streaming normalization 添加单元测试。

### 新增角色包

在应用数据目录 `packs/<id>/` 下放：

```text
manifest.json
persona.md
memory.md        # 可选
avatar.png       # 可选；也可使用 jpg/jpeg/webp/gif
```

`manifest.json` 至少包含 `id`、`name`、`persona`，可选 `avatar` 指向同包内的 png、jpg、jpeg、webp 或 gif 文件。Settings 支持导入 zip 角色包；后端要求 zip 中只有一个 `manifest.json`，校验 pack id/name、`persona`、`avatar` 和所有条目的安全相对路径，拒绝 zip-slip、重复 pack id、空/超大包，并解压到应用数据目录的 `packs/<id>/`。

仓库中不要提交具体受版权保护的角色资产、语音/美术资产或基于特定作品的人格设定；示例包应保持通用、原创或只包含占位内容。

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
