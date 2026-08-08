# 实现说明

> 文档状态：2026-07-26 已按当前源码复核。统一事件归并、流式协议边界、工作流表单与恢复、语音/陪伴交互、本地状态恢复和前端测试基线已收口；当前验证基线为前端 35 项、生产构建和 Rust 287 项测试通过。审查结论见 [代码审查报告](./CODE-REVIEW-2026-07-12.md)。

本文面向协作者，说明 Demiurge 的项目结构、核心数据流、后端模块、前端模块、安全边界和扩展方式。逐子系统的深度技术原理见 [modules/](./modules/README.md)（从[架构总览](./modules/01-architecture-overview.md)开始），路线图见 [TODO.md](./TODO.md)，设计背景见 [demiurge-mvp-design.md](./demiurge-mvp-design.md)。

## 总览

Demiurge 是一个 Tauri 桌面应用。前端负责展示和交互，Rust 后端负责 Agent 循环、上下文工程、工具执行、权限控制、持久化和 provider 适配。

```text
React UI
  ├─ invoke: send / settings / sessions / workspace / Git / workflow / memory / permission / plan / MCP / WebDAV / OCR / voice / desktop companion commands
  └─ listen: assistant/tool/agent-event/session-engine/workspace/confirm/goal/workflow/plan events
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

## 桌面窗口与主题

- 主窗口控制统一由 Rust 按 label 获取窗口；Windows 使用 `ShowWindow`、`IsZoomed` 和 `PostMessageW` 操作真实顶层窗口，其他平台使用 Tauri 窗口 API。
- `widgets` 小工具窗口在应用启动时于主线程预创建并隐藏，关闭请求只隐藏窗口；再次打开复用同一 webview，主窗口关闭仍退出应用。
- `WidgetsWindowShell` 承载吉签、陪伴状态和番茄钟，并在窗口隐藏时停止轮询、重新可见时立即刷新。
- 外观内部键继续使用 `material_bloom`，界面显示名为“水晶花”/`Crystal Bloom`；水晶花与经典主题共享 Material 版式和响应式规则，分别使用粉白紫与中性浅深色板。
- 沙盒目录选择通过静态导入的 `folderPicker` 统一处理选择、取消、不可用和失败状态；Tauri capability 显式授予 `dialog:allow-open`。

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
├─ frontend/src/
│  ├─ app/
│  │  ├─ App.tsx
│  │  └─ Sidebar.tsx
│  ├─ main.tsx
│  ├─ style.css
│  ├─ features/
│  │  ├─ agent/  chat/  companion/  live2d/
│  │  ├─ media/  pack/  settings/  voice/
│  │  └─ workflow/  workspace/
│  ├─ shared/components/
│  └─ lib/
│     ├─ api.ts
│     └─ types.ts
├─ backend/
│  ├─ common/          # Shared serialized contracts
│  ├─ core/            # Session/Goal domain state
│  ├─ framework/       # Persistence and WebDAV adapters
│  └─ desktop/src/
│     ├─ controller/   # Tauri command adapters
│     ├─ biz/          # Use-case orchestration
│     ├─ starter/      # AppState
│     ├─ starter.rs    # Runtime assembly
│     └─ agent/ llm/ tools/ store/ ...
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
| `lib.rs` | Tauri command 注册、全局 `AppState`、应用初始化、`send` 分发、上下文面板聚合、项目工作区同步和桌面陪伴壳窗口同步 | `run()` / `send()` / `context_panel_state()` / `desktop_companion_show_main()` |
| `workspace.rs` | 会话级项目文件夹绑定、懒加载目录树、限长文本预览、Git 状态/分支枚举与受保护的分支切换 | `select_workspace()` / `list_workspace_directory()` / `read_workspace_file()` / `switch_git_branch()` |
| `connection_tests.rs` | Settings 连接测试；用当前表单设置验证 LLM Provider、Web Search 和 WebDAV 以外的网络 key，不要求先保存密钥 | `test_provider()` / `test_web_search()` |
| `credentials.rs` | keyring 凭据读写，避免 LLM/Web Search/WebDAV/MCP env 密钥落入 settings 明文 | `hydrate_or_migrate_settings()` / `save_mcp_env_secrets()` |
| `ocr.rs` | OCR 模型路径、ModelScope/Hugging Face 源、下载进度事件、缺模型检查、手动安装提示和 OCR 推理入口 | `model_status()` / `download_models()` / `recognize_rgba()` |
| `voice.rs` | STT(ASR) 已接入 DashScope `qwen3-asr-flash` / OpenAI 兼容 Whisper，并限制上传体积；TTS 已接通 DashScope + GPT-SoVITS / CosyVoice(alias) 本地后端，限制响应体积，支持语速、情感、连接测试和本地失败后 DashScope 降级；本地语音模型不随默认安装包分发 | `voice_transcribe()` / `voice_synthesize()` / `voice_status()` / `voice_tts_check()` |
| `media.rs` | DashScope 媒体后端（图像生成）与 voice 用的 dashscope 凭据 / base_url 辅助 | `generate_image()` / `dashscope_api_key()` |
| `pomodoro.rs` | 番茄钟运行时、持久化状态、完成事件、节奏记忆和勿扰联动所需的面板状态 | `pomodoro_state()` / `pomodoro_start()` / `pomodoro_pause()` / `pomodoro_resume()` / `pomodoro_skip()` |
| `agent/session_engine.rs` | turn runtime state、入口互斥、中断标记、统一 agent event envelope 和会话写入封装 | `begin_turn()` / `finish_turn()` / `TurnEventEmitter` / `SessionTurnStore` |
| `agent/runner.rs` | Agent loop，处理模型流、tool calls、tool results、最终回答 | `run_turn()` / `run_turn_with_options()` |
| `agent/conversation.rs` | 内部消息结构、tool call/result 表示，以及结构化工具执行状态与元数据的历史兼容 | `Message` / `ToolCall` / `ToolExecutionMeta` |
| `agent/prompt.rs` | system prompt 分区组装，注入 persona、skills、instructions、scoped memories、summary、environment、tools 和 safety sections | `build_for_session_input()` / `build_with_report()` |
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
| `agent/workflow_schema.rs` | Workflow 输入 schema、变量模板渲染、静态校验、dry-run 计划和内置模板 | `validate_workflow()` / `render_workflow()` / `dry_run()` / `builtin_templates()` |
| `agent/workflow_runtime.rs` | JSON workflow DSL 执行、表单输入、模板安装、dry-run、失败节点重试、live panel 状态、durable run snapshot 写入和启动水合 | `launch()` / `workflow_run_with_inputs()` / `workflow_retry_failed_node()` / `hydrate_persisted_runs()` |
| `llm/*` | OpenAI-compatible/local/Anthropic/Gemini provider adapters；公共 `SseDecoder` 处理任意字节分片、CR/LF、流尾与多行 data；适配器归一化正文、思考、工具、usage、错误和 finish reason | `stream_completion()` / `ProviderProfile::for_kind()` / `SseDecoder` |
| `mcp/mod.rs` | stdio MCP Manager、server lifecycle、tool/resource discovery、resource read、动态 tool definition 与调用分发 | `ensure_initialized()` / `call_tool()` / `read_resource()` |
| `tools/mod.rs` | 工具注册表、schema 输出、权限 metadata、统一执行入口 | `registry()` / `execute()` |
| `tools/list_dir.rs` | 沙盒目录直接子项枚举，按 dir/file/other 排序，默认隐藏 dotfile 并支持数量截断 | `run()` |
| `tools/http_get.rs` | 轻量公开 HTTP(S) GET；经 `safe_http` 后返回状态、content-type、最终 URL 和截断正文 | `run()` |
| `tools/safe_http.rs` | direct URL 的 SSRF 边界：逐跳 DNS/IP 校验、地址固定、禁代理/自动重定向、peer 复核与超时 | `get_public()` |
| `tools/clipboard.rs` | 读取系统剪贴板文本并截断输出；按特权工具处理，执行前确认 | `run()` |
| `tools/package_scripts.rs` | 读取沙盒 `package.json` scripts，检测包管理器并生成建议 shell 命令；不直接执行脚本 | `run()` |
| `tools/web_common.rs` | Web Search / Fetch 共享 JSON/SSE 解析、HTML/text 清洗、source markdown 输出、source-quality 计数和 Exa MCP 调用外壳 | `parse_json_payloads()` / `append_source_lines()` / `call_exa_mcp()` |
| `tools/shell.rs` | shell 风险分类、policy state、standard/strict/sandboxed isolation、平台 process containment 和 sandbox wrapper | `run()` / `preview()` / `policy_state()` |
| `permission/mod.rs` | turn-owned 权限上下文、Session/Project/User 分层规则、confirm 往返、原子规则持久化与带身份审计 | `context_for_session()` / `decide_for_mode()` / `confirm()` |
| `pack/mod.rs` | 角色包加载、manifest 校验、头像 data URL 读取、zip 导入校验与默认包落地 | `list_packs()` / `load_pack()` / `import_zip()` |
| `pack/live2d.rs` | Live2D staging 导入、全 FileReferences containment/ASCII 重写、目录+manifest 回滚提交与受检路径解析 | `import_live2d_folder()` / `resolve_live2d_model_path()` |
| `store/mod.rs` | settings、sessions、权限规则等持久化；会话采用临时文件原子替换、上一版本备份与损坏恢复 | `Settings` / `SessionStore` / `atomic_write_text()` |

## 前端模块

| 模块 | 职责 |
|---|---|
| `frontend/src/app/App.tsx` | 主状态编排，订阅后端事件，维护消息流、设置、会话、会话级工作区、Agent 选择、Plan Mode、busy/cancel、workflow、语音和桌面陪伴状态 |
| `frontend/src/lib/api.ts` | Tauri invoke/event 的 typed wrapper，包含 session engine、工作区/Git 和统一 `agent-event` 契约 |
| `frontend/src/lib/types.ts` | 前后端共享 TypeScript 类型 |
| `frontend/src/lib/fileProcessing.ts` | 附件读取与提示词拼接辅助；PDF.js 与 JSZip 仅在处理对应附件时按需导入 |
| `frontend/src/lib/useStreamingTtsQueue.ts` | 把 assistant 流式文本按句切分成 TTS 播放队列，支持停止、静音、队列状态和语速/情感/streaming 参数透传 |
| `components/Sidebar.tsx` | 会话列表、会话重命名/删除、会话绑定项目名称、角色包选择和基础入口 |
| `components/Composer.tsx` | 输入框、中断/发送状态、项目选择和分支切换入口 |
| `components/BranchSwitcher.tsx` | 当前/远程分支搜索、脏工作区提示、切换确认与错误反馈 |
| `components/WorkspaceExplorer.tsx` | 项目文件树懒加载、文件预览、Git 更改列表、项目刷新与重新选择 |
| `components/MessageList.tsx` | 用户消息、助手消息、工具卡片渲染 |
| `components/PomodoroCard.tsx` | 聊天页番茄钟控制面板，支持任务绑定、暂停/继续/跳过、中断原因、桌面通知和节奏摘要 |
| `components/VoiceCallPanel.tsx` | 第一阶段语音通话面板：接通/挂断、静音、时长、按键说话、简单 VAD 端点检测、STT → LLM → 流式 TTS 和本地半双工打断 |
| `components/DesktopCompanionShell.tsx` | 独立透明桌面陪伴壳窗口：展示头像、陪伴状态、天气/建议、置顶、点击穿透、收起/展开和权限边界状态 |
| `components/Live2DPanel.tsx` / `Live2DWindowShell.tsx` | asset protocol 直载模型、分阶段进度、主面板 keep-alive，以及隐藏时暂停 ticker 的独立透明 Live2D 窗口 |
| `components/Select.tsx` / `lib/selectPosition.ts` | body portal 下拉菜单；按视口可用空间自动向上/向下展开并限制高度与水平边界 |
| `components/Markdown.tsx` | 轻量 Markdown 入口，通过 `React.lazy` 延迟加载完整渲染器 |
| `components/MarkdownRenderer.tsx` | GFM、代码块、highlight.js、KaTeX 与 Mermaid 渲染；流式尾部词片段淡入并尊重 reduced-motion |
| `components/ToolCard.tsx` | tool-start/tool-end 展示；编辑工具显示受影响文件活动、可展开详情、差异与回滚提示 |
| `components/ConfirmDialog.tsx` | 敏感工具确认，支持 once/session/project scope |
| `components/SettingsDialog.tsx` | provider、Persona Pack zip 导入、Web Search、MCP server、OCR 模型源/下载进度/缺模型引导、语音、WebDAV、权限、Companion/Weather、分层记忆维护和 Context 可视化设置，以及 Provider/Web Search/WebDAV 连接测试 |
| `components/WorkflowsPanel.tsx` | workflow 定义编辑、输入表单、内置模板、校验/dry-run、run/stop、失败节点重试及 live 状态 |

## 运行数据目录

应用数据目录由 Tauri `app_data_dir` 决定。主要内容：

```text
app_data_dir/
├─ settings.json                 # 非密钥设置
├─ sessions.json                 # 多会话、active session、workspace_path、rolling summary、goal state
├─ project_permissions.json      # versioned；canonical workspace identity → 项目级规则
├─ user_permissions.json         # 用户级权限规则
├─ permission_audit.jsonl        # 带 session/workspace identity 的轻量权限审计
├─ permissions.json              # 旧版无项目身份文件；仅保留，不读取或迁移
├─ companion-memory-queue.json   # 陪伴记忆待确认队列
├─ pomodoro.json                 # 番茄钟当前状态、任务绑定和节奏记忆
├─ memory/user.md                # user-scope 手动记忆
├─ skills/*/SKILL.md             # global skills
├─ sandbox/                      # 未选择项目时的默认工作区
├─ packs/                        # 用户角色包；可包含 pack memory 和 pack skills
├─ ocr-models/                   # OCR 模型
└─ <当前会话项目>/.demiurge/
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

## 项目工作区、Git 与会话绑定

`Session.workspace_path` 是项目选择的持久化真值；`SessionMeta` 同时返回 `workspace_path` 与派生的 `workspace_name`，供侧栏展示。选择或切换会话时，后端先 canonicalize 目标目录，再同步 `AppState.sandbox_dir`。旧会话、空路径或已移除目录会安全回退到应用数据目录的默认 `sandbox/`。

项目浏览命令只接受相对路径，拒绝绝对路径、`..`、空字节和被忽略目录；访问现有文件前再次 canonicalize 并验证仍位于项目根内。目录按层懒加载，预览最多读取 256 KiB，二进制或非 UTF-8 内容不作为文本返回。

Git 调用使用固定参数数组与 `current_dir`，不经过 shell。分支列表按 `workspace.path + generation` 失效；枚举与切换都提交 `expected_workspace_path`，后端 canonicalize 后要求它等于当前项目根，并在整个切换期间持有工作区锁。分支只能从当前项目的枚举结果中选择；生成回复期间禁止改变项目或分支。脏工作区的确认是前端防误触提示，真正冲突仍由 `git switch` 失败并原样返回错误，不会自动丢弃改动。

## Agent 循环

1. `send` 捕获当前 active session id，并通过 `session_engine::begin_turn` 建立 turn runtime state、入口互斥、input preview、agent/workflow metadata 和中断标记；`TurnHandle.session_id` 随后成为整轮不可变所有者。turn 登记与会话删除使用相同锁序，运行或取消中的所属会话不可删除。
2. slash command 先分流，例如 `/skills`、`/skill`、`/goal`、`/effort`、`/compact`、`/dream`、`/ultracode`、`/workflows`、`/workflow resume <run_id>`。
3. 普通回合调用带显式 `session_id` 的 `run_turn_with_options`；runner 在任何 MCP/外部初始化 await 前验证目标，并用该 id 构造 `SessionTurnStore`，统一读取、追加、替换 messages 与 rolling summary。slash、Goal 续跑、子 Agent、context/goal/write_plan 工具也使用同一 turn-owned id。
4. runner 同时在异步初始化前用 turn-owned session 捕获不可变 `PermissionContext`；后续权限查找、审计和确认记忆始终复用该 session/canonical workspace identity。`prompt::build_for_session_input` 按同一 session 组装 engine、persona、skills、project instructions、environment、goal、summary 和 scoped memories；如果 `settings.permission_mode == plan`，runner 额外注入 Plan Mode overlay，要求只读探索并用 `write_plan` 生成实施计划。
5. `budget` 和 `context` 按预算裁剪历史。
6. provider adapter 发起流式请求。
7. 如果模型返回 tool calls，后端执行工具并把 tool result 写回历史，再进入下一轮模型请求。
8. assistant/tool 事件统一通过 `TurnEventEmitter` 发出；后端为兼容继续双发 legacy 事件和带 turn context 的 `agent-event`，主时间线只消费统一信封并拒绝非当前 session 的事件。确认与 Goal 进度载荷另带 `session_id`，走同一前端归属检查。
9. 如果模型给出最终回答，触发 `assistant-done`，随后尝试记忆提取。
10. 如果 turn-owned session 有 active goal，则 `goal::drive_after_turn(session_id)` 继续调度下一轮，直到目标完成、暂停、阻塞、预算限制、max turns 或中断；切换侧栏 active session 不会改变续跑目标。
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

Reasoning effort follows the same profile-gated path. `settings.reasoning_effort` stores `auto|low|medium|high|xhigh|max`; `/effort` and the Settings Provider panel write the same field. `ProviderProfile` resolves `DEMIURGE_EFFORT_LEVEL` first, then settings, and only supported provider/model pairs emit request fields: OpenAI official chat completions use `reasoning_effort` plus `max_completion_tokens` for reasoning/Codex models, mapping `xhigh|max` to `xhigh` only for xhigh-capable GPT-5.x/Codex models and otherwise falling back to `high`; Anthropic uses `output_config.effort` with `anthropic-beta: effort-2025-11-24` for supported model names; and Gemini uses `generationConfig.thinkingConfig.thinkingBudget` for thinking-capable Gemini models. Generic OpenAI-compatible providers remain explicit unsupported to avoid sending non-portable fields to DeepSeek/DashScope/local gateways; `DEMIURGE_ALWAYS_ENABLE_EFFORT` can force model gating when testing a new model.

## Skills

Skills 是 Markdown 目录能力，不依赖额外运行时。发现顺序覆盖 global、project、repository、pack 和兼容目录：

- `app_data_dir/skills/*/SKILL.md`
- `sandbox/.demiurge/skills/*/SKILL.md`
- `sandbox/skills/*/SKILL.md`
- `packs/<pack_id>/skills/*/SKILL.md`
- `sandbox/.demiurge/compat/skills/*/SKILL.md`

`SKILL.md` 支持 YAML frontmatter：`name`、`description`、`triggers`/`keywords`、`tools`/`declared_tool_needs`、`required_permissions`、`references`、`always_include`。`prompt::build_for_input` 会根据当前 user text 自动选择 always_include 和匹配分最高的 skills，把 skill body、declared tool needs、required permissions 和安全相对 references 注入 system prompt；references 只能读取 skill 目录内的安全相对路径。`/skills` 和 `/skill` 返回当前可发现 skills、scope、匹配分与选中状态，供用户检查推荐结果。

## Memory

Memory 仍以 Markdown 为主，但读写路径已分层：

- user：`app_data_dir/memory/user.md`
- project：`sandbox/.demiurge/memory.md`
- session：`sandbox/.demiurge/session-memory/<session_id>.md`
- pack：`packs/<pack_id>/memory.md`
- project legacy：`sandbox/memory.md` 只读兼容加载

Prompt 会按 user/project/session/pack 加载分层 memory，并继续兼容旧的 project legacy memory。Settings Memory 面板按 scope 展示 path、条目、重复项和统计，支持对 user/project/session/pack 手动新增、编辑 kind/text、删除和去重；通用自动 memory extraction 仍优先写入 project scope。

Memory、`/dream` 整理结果和 SessionStore 写入均使用同目录临时文件原子替换；保留上一版本备份，并在主文件损坏时尝试恢复。上下文折叠只在摘要生成成功后提交历史与 summary，取消或摘要失败不会先删除原消息。

Companion memory 走独立的用户确认链路：手动陪伴建议和授权后的 LLM 陪伴抽取都会先写入 `companion-memory-queue.json`，每条包含来源会话、原因、scope、kind、正文、创建时间和状态。Settings 的 Companion 页提供抽取开关、范围说明、最近队列记录、批量保存/忽略、撤销写入和跳转 Memory 面板；保存前会检查目标 scope 的相近记忆，提示合并、替换或保留新条目，已保存记录可撤销并恢复队列状态。

## Companion / Weather

`companion.rs` 负责聊天页 Companion 卡片、陪伴 prompt context、记忆队列、高风险表达检测和天气陪伴。高风险表达检测在 `send` / `send_with_agents` 进入 LLM 前运行，命中自伤/危机或医疗/心理治疗替代诉求时直接持久化一条支持性回复，并跳过普通记忆抽取。

天气使用 provider 抽象，当前默认实现是无 key 的 Open-Meteo：手动城市会先地理编码再查询 forecast；粗略定位模式只在用户开启且未填写城市时通过 IP 估算城市，并仅缓存城市级信息。天气缓存和粗略位置缓存均为 30 分钟 TTL，可从 Companion 卡片或 Settings 天气数据治理区域清理。天气卡片展示 provider、缓存过期、错误降级、温度/体感/AQI/UV 等摘要；建议覆盖降雨、通勤降雨概率、体感高低温、昼夜温差、紫外线、空气质量、湿度、风力和极端天气信号，文案保持低频克制。

## Voice / Desktop Companion Shell

语音链路分三层：`VoiceCallPanel` 负责前端录音与通话 UI，`voice_transcribe` 负责 STT，`useStreamingTtsQueue` 负责把模型文本增量按句切分后调用 `voice_synthesize` 播放。Settings 的 Voice 页可配置 STT/TTS backend、音色、语速、情感、streaming 请求参数、失败降级、应用聚焦快捷键和 TTS 连接测试。TTS backend 支持 DashScope、GPT-SoVITS 和 CosyVoice alias；本地后端默认只连 `media_base_url` 指向的外部服务，不把模型权重打进安装包。当前“流式”指文本分句生成并排队播放，真正的音频字节流解码仍未实现。

语音通话当前是第一阶段回合制体验：点击电话按钮或语音快捷键打开面板，接通后按键说话，录音结束走 STT，转写文本进入当前会话，assistant 流式回答同步进入 TTS 队列。面板显示接通/挂断、静音、时长和 TTS 队列状态，并在开始录音前停止当前 TTS，形成本地半双工打断。简单 VAD 用音量阈值检测端点；通话记忆归档与 Live2D 口型/动作联动尚未接入。

桌面陪伴壳是非 Live2D 的独立 Tauri webview window，label 为 `desktop_companion`。`lib.rs` 根据 `Settings` 中的 `desktop_companion_enabled / always_on_top / click_through / collapsed` 创建或隐藏透明无边框窗口，并用 `set_ignore_cursor_events` 实现点击穿透。`main.tsx` 根据当前窗口 label 渲染 `DesktopCompanionShell` 或主应用；陪伴壳展示当前角色头像、陪伴 focus/mood、天气/建议摘要、置顶/穿透/收起控制、主窗口入口和屏幕/麦克风/位置状态。

Live2D 另有 label 为 `live2d` 的透明置顶窗口。启动时只隐藏预创建 webview，首次呼出后才加载模型；关闭请求改为隐藏，模型、纹理和 WebGL 实例继续复用。Rust 显隐事件驱动 `Live2DPanel.active`：隐藏时 `ticker.stop()`，再次显示时 `ticker.start()`，避免后台持续占用 GPU。主窗口内的 Live2D 面板首次访问后也保持挂载，切换对话/图像视图只暂停和恢复 ticker，不重复请求模型资源。

## Pomodoro / Focus Rhythm

`pomodoro.rs` 负责本地番茄钟状态和节奏陪伴。状态持久化到 `pomodoro.json`，包含当前 timer、任务绑定、连续专注计数、偏好时长计数、中断原因计数、高效小时计数和最近完成时间。前端通过 `PomodoroCard` 调用 `pomodoro_start/pause/resume/skip`，可选择专注、短休息、长休息或自定义时长，并把计时绑定到当前会话、Goal、Workflow run 或手动标题。

计时完成后后端发出 `pomodoro-updated` 和 `pomodoro-completed` 事件，前端用 Web Notification 和 Tauri attention request 做桌面提醒。专注开始时会生成轻量拆解步骤，结束后给出复盘提示和连续完成鼓励；跳过专注时可记录中断原因。Companion 面板读取 Pomodoro 状态，专注中会收起低优先级主动提醒，只保留高优先级提示和专注保护提示。

## 上下文工程

system prompt 由多个 section 组成：

- 引擎规则：工具、安全、输出约束。
- 角色设定：当前角色包 `persona.md`。
- Skills：当前输入匹配或 always_include 的 Markdown skills。
- 项目指令：沙盒根 `DEMIURGE.md` / `SYSTEM.md` / `AGENTS.md`，并额外注入 `README.md`、包/框架检测和目录快照。
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

MCP 工具是运行时动态注册的：`agent::runner` 在生成工具 schema 前调用 `mcp::ensure_initialized`，随后 `tools::registry_for_state` 把已连接 server 的 `tools/list` 结果追加为 `mcp__server__tool`。模型调用这些动态工具时，`tools::execute` 直接分发到 `mcp::call_tool`。服务端 annotation 只保留为 UI/并发提示，本地授权风险由 `authorization_risk` 施加 `External`/`Privileged` 下限，动态工具默认权限固定为 `Ask/Once`。MCP resources 通过 Settings 面板展示，并通过 core tool `mcp_read_resource` 调用 `resources/read`。

当前核心工具：

- 文件与目录：`read_file`、`list_dir`、`write_plan`、`write_file`、`edit_file`、`multi_edit`、`apply_patch`、`undo_edit`
- 搜索导航：`glob`、`grep`、`git_status`
- 执行：`shell`（standard / strict / sandboxed isolation）
- 联网：`web_search`、`web_fetch`、`http_get`
- 系统读取：`clipboard`（读取剪贴板需确认）
- 系统信息：`system_info`（读取时间、OS、架构、工作目录等基础环境，只读）
- 包脚本：`package_scripts`（只读取 scripts 并生成建议 shell 命令，不执行脚本）
- MCP：`mcp_read_resource`，以及运行时动态发现的 `mcp__server__tool`
- 多 Agent：`agent_spawn`
- 上下文：`context_inspect`、`context_collapse`
- 目标：`goal`
- deferred：`open_path`、screen capture、OCR
- workflow/worktree：`worktree_create`

## 安全模型

> 当前限制：以下机制描述设计目标，不代表所有边界已闭环。代码审查列出的 12 项 P1 已全部修复，包括角色包/IPC 信任根、权限作用域、跨项目 undo、系统打开、direct HTTP SSRF、Live2D 导入、deferred 授权、导航事务、分支/回合归属和动态外部工具风险下限；目录/预览等组件级竞态仍在 P2 队列。详情见 [代码审查报告](./CODE-REVIEW-2026-07-12.md)。

- `PermissionMode` 支持 `plan` / `default` / `auto` / `bypass`：`default` 与 `auto` 都先走本地规则链和工具默认策略，只有有效裁决为 Allow 才自动执行；`bypass` 显式跳过确认但仍审计；`plan` 未批准前只允许本地 `ReadOnly` 工具和受限 `write_plan`。动态外部工具不会被归类为本地 `ReadOnly`。
- Plan Mode 的计划状态在 `AppState.plan_state` 中维护；`write_plan` 只能写入沙盒 `.demiurge/plans/`，前端通过 `approve_plan` 批准后自动回到 `default` 执行模式。
- 文件与 shell 工具只能访问当前会话项目根；未选择项目时使用默认沙盒。
- 路径先做词法校验，再对最近存在祖先做 canonicalize，防止符号链接和 junction 逃逸。
- 回复生成期间禁止切换到其他项目或 Git 分支，避免工具执行根目录在回合中途改变。
- `navigation_snapshot` 在同一 SessionStore 锁下捕获 active `session_id`、会话列表、历史、session-owned workspace path 与 Goal；workspace 状态按该捕获路径检查，不再从稍后可变的全局 sandbox 推导。新建/选择/删除直接返回快照，选择/删除同步失败会回滚会话状态。
- 前端 `NavigationEpoch` 同时维护 navigation epoch 与同 epoch request 序号：新用户导航作废全部旧请求，后台刷新只允许最后发起者提交；快照还必须返回期望 session id。`navigationPending` 同步锁定会话导航和可能作用于项目的交互。
- Session 规则按 session id 存在内存独立桶中并随会话删除；Project 规则以 canonical workspace identity 为键持久化，User 规则才是全局规则。
- runner 在异步初始化前捕获 turn-owned 权限上下文，整个回合的规则查找、审计和确认记忆不再依赖可变 active session；设置面板更新/清除也校验界面携带的具体身份。
- `project_permissions.json` 与 `user_permissions.json` 受同一存储锁保护并以同目录临时文件原子替换；边界或存储错误按 `Deny(Once)` 处理。旧 `permissions.json` 不自动应用，避免把无项目身份的历史授权扩散到任意项目。
- 动态外部工具始终保留 `Ask/Once` 默认权限；服务端自报的 read-only/destructive/open-world annotation 只影响展示与并发提示，本地有效风险不会低于 `External`/`Privileged`。Auto 不会以风险分类覆盖规则链中的 `Ask` 或 `Deny`。
- 写入、shell、open_path、截图/OCR 等操作走确认门。
- 屏幕感知工具受 `computer_use_enabled` 统一开关和逐次确认门控；关闭时 `screen_list_windows`、截图和 OCR 入口会拒绝执行，Settings 的 OCR 区域展示当前边界。
- 桌面陪伴壳只是透明状态窗口，不默认读取屏幕、麦克风或精确位置；小窗和 Settings 会展示屏幕工具、语音与位置/天气状态。麦克风只由录音按钮或应用聚焦快捷键触发，天气只按设置中的手动城市或粗略城市模式查询。
- confirm 支持 once/session/project/user scope；记忆持久化失败时，本次审计作用域降为 once 并记录失败原因。
- `interrupt` 会唤醒所有待确认项并按拒绝处理。
- shell 限制 cwd、timeout、output cap 和环境变量；所有 shell 子进程使用独立进程组/进程树，超时时终止整棵进程树。
- shell `strict` isolation 强制最小环境白名单，并拒绝联网、依赖安装、破坏性、提权和外部执行类命令；Settings 的 Permission Rules 区域展示 env allowlist、strict deny 风险、命令模式和平台 containment 状态。
- shell `sandboxed` isolation 在 strict 策略基础上要求 OS sandbox wrapper：macOS 使用 `sandbox-exec` profile 限制写入路径并拒绝网络，Linux/WSL 使用 `bubblewrap` 绑定沙盒/临时目录并 `--unshare-net`；Windows 原生明确不支持 filesystem/network sandbox，保留进程树 containment 并 fail closed。
- `clipboard` 按 privileged/ask 处理，避免未确认读取系统剪贴板中的密钥、聊天或临时敏感数据。
- `package_scripts` 只读取沙盒 `package.json` 的 scripts 字段并返回建议 shell 命令；脚本执行仍必须走 `shell` 的确认门和隔离策略。
- `http_get` 与 direct `web_fetch` 只访问公开 HTTP(S) 地址：每个 redirect hop 都重新解析并检查全部 DNS 答案，拒绝非公网 IP，通过 reqwest DNS override 固定地址，禁用代理与自动重定向，并复核实际 peer。DNS/连接/逐读/请求都有超时。
- Live2D 文件夹先复制到包内 staging，源 symlink/junction 与特殊文件直接拒绝；全部 model3 引用经统一的便携 Normal-component 校验和 canonical containment，非 ASCII 资源只在 staging 内重命名。模型树、manifest 和最终 bundle 复核通过后才清理旧备份，提交错误会恢复旧目录与清单。
- `resolve_pack_dir` 是角色包路径的唯一信任根解析器：id 只允许最长 128 字节的 ASCII 字母、数字、`-`、`_`，根与现有目标都 canonicalize，目标必须是非 symlink/junction 的直接子目录。IPC、zip 目标、manifest/files/lore/Live2D、当前包设置、pack memory、pack skills 和梦境材料均复用该边界；无错误返回通道在失败时省略 pack 来源。
- 对有效 `execute_tool`，runner 在事件/权限门前把外层参数解析为真实 deferred target；真实工具名成为规则、角色包 overlay、remember 和审计 identity，内层 args 驱动风险、summary、preview 与 affected paths。外层名称仅保留给模型 tool-call/result 配对与执行分发。
- MCP 第一阶段仅支持本地 stdio server；server command/env 来自设置页，secret-like env 写入 keyring，`settings.json` 和备份只保留空值。
- MCP 动态工具默认按 annotation 映射风险，执行前接入现有权限确认与审计；`mcp_read_resource` 按外部资源读取处理。
- 子 Agent 只暴露 `SUBAGENT_READONLY_TOOL_NAMES` 中的只读/外部读取工具，不暴露 `shell`、`clipboard` 和写入类工具。
- 权限审计不写完整敏感参数，但记录实际裁决所绑定的 session/workspace identity；旧审计行保持兼容读取。

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

Workflow schema 可声明字符串、数字、布尔等表单输入，并在消息、phase 名称和 agent prompt 中用 `{{input_name}}` 引用。运行前会执行静态校验和 dry-run，前端可安装内置模板、填写输入并查看执行计划；失败的 run 可从记录的失败节点发起重试。

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

`web_fetch` 用于单 URL 抓取，支持 `source=direct|exa`、`context_max_characters` 和 Exa `livecrawl=fallback|always|never`。direct 路径与 `http_get` 共用 `safe_http` 的本机 SSRF 防护；`source=exa` 或显式 `livecrawl` 会把 URL 交给外部抓取服务，不经过本机 DNS/IP pin。`web_search` 的 Exa adapter 使用同一组 `livecrawl` 策略，并支持 `search_type=auto|fast|deep`。

`tools/web_common.rs` 承载两个工具共用的内容逻辑：JSON/SSE payload 解析、HTML/text 清洗、URL/title/domain 规范化、统一 `WebSource`、Sources/Links markdown 行生成、source-quality 链接计数，以及 Exa MCP endpoint/key/env fallback/request envelope。direct 网络安全边界独立收口在 `tools/safe_http.rs`，避免内容清洗与连接授权混为一层。

外部 adapter 环境变量与 keyring：

- `WEB_SEARCH_ADAPTER`
- `TAVILY_SEARCH_URL` / `TAVILY_ENDPOINT_URL` / `TAVILY_API_KEY`
- `BRAVE_SEARCH_API_KEY` / `BRAVE_API_KEY`
- `EXA_MCP_URL` / `EXA_API_KEY`

Tavily、Brave、Exa key 优先从 settings/keyring 水合，也保留环境变量 fallback。

## 扩展方式

### 新增工具

1. 在 `backend/Demiurge-desktop/src/tools/<name>.rs` 实现工具逻辑。
2. 在 `tools/mod.rs` 增加 `mod <name>;`。
3. 在 `registry()` 注册 tool definition。
4. 在 `execute()` 增加分支。
5. 按风险选择 `PermissionPolicy::allow` 或 `PermissionPolicy::ask`。
6. 为解析和安全边界添加单元测试。

### 新增 provider

1. 在 `backend/Demiurge-desktop/src/llm/` 增加 adapter，或复用 OpenAI-compatible adapter。
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

`manifest.json` 至少包含 `id`、`name`、`persona`，可选 `avatar` 指向同包内的 png、jpg、jpeg、webp 或 gif 文件。Settings 支持导入 zip 角色包；后端要求 zip 中只有一个 `manifest.json`，校验 pack id/name、`persona`、`avatar` 和所有条目的安全相对路径，拒绝 zip-slip、重复 pack id、空/超大包，并在 canonical `packs` 根的同级临时目录完成校验后 rename 到受检直接子目录 `packs/<id>/`。

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

前端测试：

```bash
npm test
```

前端体积治理集中在 `vite.config.ts` 和重模块入口：`manualChunks` 将 Mermaid diagram chunks、Mermaid parser、Cytoscape/D3/graph layout、Markdown/KaTeX/highlight、PDF.js 和 JSZip 分离；`components/Markdown.tsx` 只保留轻量 Suspense 入口，完整渲染器在消息区域需要时加载；PDF/ZIP 解析也按需导入。2026-07-26 的生产构建通过，但仍对约 1.1 MB 的 Live2D vendor chunk 给出非阻断体积警告；这属于已知性能优化项，不应描述为零 warning。

Rust 测试：

```bash
cargo test --manifest-path backend/Demiurge-desktop/Cargo.toml
```

2026-07-26 验证结果：43 项前端测试与 287 项 Rust 测试全部通过；`npm run build` 和 `cargo fmt --check` 通过。图像面板的分辨率菜单另在 1100×680 本地页面中验证为 body portal、靠近底边时自动向上且完整位于视口内。生产构建仍有约 1.1 MB 的 Live2D vendor chunk 非阻断体积警告；Rust 仍有两个既有未使用方法警告；供应商专项只覆盖离线解析与请求体契约，发布前仍应执行真实端点网络契约测试。

Tauri 打包：

```bash
npm run tauri build
```

当前 Windows bundle 目标为 NSIS，尚未接入 updater 与发布签名。腾讯服务器/Gitea 的建议分发拓扑、匿名下载边界、Tauri updater 签名和 Authenticode 区别见 [`DISTRIBUTION.md`](DISTRIBUTION.md)。
