# TODO / 路线图

Demiurge 当前已经从 MVP 进入 Agent 能力打磨阶段。下面把现有 TODO、Claude Code 主线对齐差距、架构路线图合并成一份去重后的优先级路线图，便于协作时判断先后顺序。

## 已实现

- [x] **桌面应用骨架**：Tauri + Rust 后端、React 前端、会话 UI、设置页、工具卡片、确认弹窗。
- [x] **流式 Agent Loop**：模型流式输出、工具调用、工具结果回填、多轮循环、用户中断。
- [x] **Session Engine 成熟化**：后端集中 turn runtime state、入口互斥、中断状态和最近回合记录；`TurnEventEmitter` 同时发送 legacy assistant/tool 事件与统一 `agent-event`；`SessionTurnStore` 收敛会话写入和持久化；前端 busy/cancel 状态由 `session-engine-updated` 驱动。
- [x] **Provider Adapter**：OpenAI-compatible、local、Anthropic、Gemini。
- [x] **Provider Capability Profile 2.0 first slice**：集中 provider capability flags、tool schema dialect、structured output 能力标记和 provider body-builder gating。
- [x] **工具注册表**：统一 metadata、JSON Schema、权限策略、并发属性和输出属性。
- [x] **文件沙箱**：读写限定沙箱目录、路径校验 + canonicalize 防逃逸。
- [x] **编辑工具**：`read_file`、`write_file`、`edit_file`、`multi_edit`、`apply_patch`、`undo_edit`。
- [x] **搜索与状态工具**：`glob`、`grep`、`git_status`、`web_fetch`。
- [x] **更多内置 typed tools**：`list_dir`、`http_get`、`clipboard`、`package_scripts` 已接入核心工具；只读/外部/特权权限分级固定，`clipboard` 与 `shell` 仍需确认，包脚本工具只生成建议命令、不直接执行。
- [x] **Shell 工具**：确认弹窗、沙箱 cwd、超时、输出截断、执行前预览。
- [x] **Shell / 进程隔离加强**：shell policy state 可视化展示 env allowlist、strict deny 风险和命令模式；执行层使用独立进程组/进程树并在超时时终止整棵进程树；新增 `sandboxed` isolation，在 macOS 走 `sandbox-exec`、Linux/WSL 走 `bubblewrap`，不可用时 fail closed；补齐联网、依赖安装、外部执行、提权和破坏性命令策略测试。
- [x] **Web Search / Fetch**：Bing、DuckDuckGo fallback、Tavily、Brave、Exa adapter，结果过滤、缓存、context cap、Sources 提醒；`web_fetch` 支持 direct 抓取与 Exa `livecrawl` 抓取。
- [x] **WebFetch / WebSearch adapter 去重**：抽取共享 JSON/SSE 解析、HTML/text 清洗、source markdown 输出、source-quality 计数和 Exa MCP 调用外壳，减少 `web_fetch` / `web_search` adapter 重复。
- [x] **权限系统**：一次/会话/项目级确认，危险操作 audit log。
- [x] **上下文管理**：system prompt 分层、项目指令、角色设定、工作区环境、会话摘要、memory、token-aware 裁剪。
- [x] **Context Collapse**：`/compact`、`context_inspect`、`context_collapse`。
- [x] **长期记忆**：自动提取偏好和项目约束，写入 `.demiurge/memory.md`。
- [x] **Dream 后台流程**：`/dream` 入口和本地任务流程。
- [x] **Goal 后台流程**：`/goal`、token budget、pause/resume/continue、自动续写、模型 `goal` 工具。
- [x] **多 Agent 雏形**：`/ultracode`、只读 `agent_spawn`、fork context。
- [x] **Deferred Tools**：`tool_search` / `execute_tool` 延迟发现和执行低频工具。
- [x] **Workflow Journal / Resume**：JSONL journal、`/workflows`、`/workflow resume <run_id>`。
- [x] **Workflow JSON DSL / Live Panel**：`agent`、`parallel`、`pipeline`、`phase`、`budget`、`log` step，以及 Workflows 页 run/stop/resume。
- [x] **Workflow durable run**：`.demiurge/workflow-runs/<run_id>/state.json` 持久化 run snapshot；启动和面板打开时水合 persisted run state；恢复 `stale_running`、取消状态、预算和进度；journal 不可读时 `/workflow resume <run_id>` 可从 durable snapshot 生成恢复 overlay。
- [x] **Worktree Isolation**：`worktree_create` 工具创建隔离 worktree。
- [x] **Computer Use 底层能力**：窗口列表、屏幕截图、点击/输入 OCR、以及 OCR 模型下载。
- [x] **OCR 体验补全**：Settings OCR 面板支持源说明、缺文件清单、下载进度、手动安装提示和 ModelScope 国内镜像文档。
- [x] **角色包头像与导入器**：`manifest.avatar` 会被校验并读取为前端头像 data URL；Settings 支持拖拽/选择 zip 导入，后端校验 manifest 与安全路径后解压到 `packs/`；未提交具体受版权保护的角色资产、语音/美术资产或人格设定。
- [x] **Voice API 预留**：TTS/ASR adapter 接口保留，设置页露出占位。
- [x] **API Key 安全存储**：LLM、Tavily、Brave、Exa、WebDAV 密钥使用系统凭据管理器，`settings.json` 只保存密钥引用。
- [x] **设置与备份**：设置页包含 provider、Web Search、OCR、存储占位和 WebDAV 备份；WebDAV 支持连接检查、手动备份、备份列表、删除。
- [x] **设置连接测试**：Settings 支持直接验证 LLM Provider/base_url/model/key、Web Search provider/key 和 WebDAV 连接；测试使用当前表单值，不要求先保存密钥。
- [x] **细粒度上下文可视化**：Settings Context 页展示 system/tools/history/output reserve、summary、memory source、预算消耗、history role breakdown 和 prompt section token/截断细节。
- [x] **项目记忆审计 UI**：长期记忆可查看、编辑、删除，对应条目自动去重。
- [x] **自定义 Agent 模板**：`.demiurge/agents/*.json` 支持 prompt、allowed tools、budget、handoff format 和评审定义；前端对话可多选 Agent。
- [x] **多 Agent 证据包与 reviewer**：`agent_spawn` 支持 `output_format=evidence_packet`、`reviewer_count` 和 `max_total_tokens` 硬预算。
- [x] **MCP 集成第一阶段**：Rust 原生 stdio MCP Manager，支持 server 配置、启动/停止/刷新、tool discovery、resource list/read、secret env keyring、水合/脱敏、动态 `mcp__server__tool` 工具注册、权限风险分级和 Settings/ToolCard UI。

## P0 / 核心架构与安全边界

- [x] **MCP 集成第一阶段**：实现完整 MCP Manager，优先 stdio server 配置、启动/停止/健康检查、tool discovery、resource 读取、凭据/env/token 管理、工具调用 UI 渲染，并把 MCP 工具接入权限模型与风险分级；后续再扩展 HTTP/SSE、OAuth、MCP-backed skills。
- [x] **真实 Plan Mode**：实现用户主导的“先计划、后执行”模式；生成计划文件，在用户批准边界之后才允许写入、shell、外部发布等动作，并支持 plan/default/auto/bypass 等权限模式的清晰切换。
- [x] **Provider Capability Profile 2.0**：在 first slice 基础上完成 prompt caching、thinking、parallel tool calls、max token 差异、structured output/schema dialect、provider-specific token budgets 的统一建模，并把 runner/budget 层切到 profile helper。
- [x] **Workflow durable run**：把 live run 从进程内状态升级为跨进程 durable background execution，能够在应用重启后恢复真实 run 状态、取消状态、预算状态和进度，而不只是生成 resume overlay。
- [x] **Shell / 进程隔离加强**：从 policy-level 约束推进到 OS-level process isolation；补齐平台 sandbox、可视化 allowlist/denylist、联网/依赖安装/外部执行策略，并深测 macOS/Linux 跨平台安全策略。
- [x] **Session Engine 成熟化**：引入或继续拆分 Session Engine，覆盖 turn management、多 tool-call loop、状态、取消、重试、错误、前端事件、store decoupling、React transcript/tool event rendering 与 Rust backend event stream。

## P1 / 高杠杆 Agent 与工作流体验

- [x] **Goal 状态栏与控制**：显示当前 goal、状态、预算、pause/resume/continue/clear 控制，并让自动续写进度与可恢复状态更加透明。
- [x] **进度与错误可见性**：细化 Web Search、长 workflow、Goal continuation 的 progress UI；改进 LLM、网络、工具错误展示，提供更友好的失败说明、重试按钮、错误 retry 线索和 source-quality hints。
- [x] **多 Agent 证据包强校验**：把 evidence packet 从提示词约束升级为 provider-level structured output / JSON Schema 校验；严格按自定义 Agent `handoff_format` 校验，并增加专门 judge/synthesizer 回合做多 reviewer 综合，而不只做确定性合并。
- [x] **Agent JSON 编辑器**：在 UI 中创建、编辑、校验 `.demiurge/agents/*.json`，提供示例模板、schema validation、导入/导出，以及更完整的 per-agent runtime statistics。
- [x] **Permission System 2.0 与可视化**：升级 rule-based Permission Engine，支持 allow/deny/ask effect 与 once/session/project/user scope；确认 UI 显示工具名、参数摘要、影响路径、风险、命中规则和 allow-once/session/project/deny 选项；增强 permission panel 的规则说明、增删改查和 audit 入口。
- [x] **Edit Tool + Diff UI**：完善精确编辑、diff preview、用户确认、apply/reject、undo 和 tool-result feedback，保持写操作可解释、可回滚。
- [x] **Project Context Builder / Prompt Context Builder**：统一 ordered sections：static system prompt、pack persona、skills、project instructions、memories、summary、environment、tools、safety rules；支持 git status、README、package/framework detection、DEMIURGE.md/CLAUDE.md、目录结构、用户记忆，并及早设计 section priority 与预算策略，避免上下文膨胀。

## P2 / 产品体验与能力补全

- [x] **设置连接测试**：验证 provider、base_url、model、LLM key、Web Search key 和 WebDAV 连接是否可用。
- [x] **细粒度上下文可视化**：展示 system/tools/history/output reserve、summary、memory、预算消耗和 prompt section 细节。
- [x] **WebFetch / WebSearch adapter 去重**：抽取共享解析、清洗和来源处理模块，减少重复代码，让来源质量提示和 provider 边缘行为更一致。
- [x] **OCR 体验补全**：补齐模型源选择、下载进度、缺模型引导和国内镜像文档。
- [x] **角色包头像与导入器**：读取 `manifest.avatar` 替换默认头像；创建拖拽 zip 导入器，解压到 `packs/` 并校验 manifest，避免提交具体受版权保护的角色资产、语音/美术资产或人格设定。
- [x] **更多内置工具**：增加 `list_dir`、`http_get`、`clipboard`、包脚本等 typed tools，并配置合适权限分级；继续优先 typed tools，shell 保持后置和强确认。
- [ ] **Skills 支持**：实现 Markdown skills、slash command、pack-scoped/global skill directories、自动推荐、`SKILL.md` context injection、declared tool needs、required_permissions 和 references。
- [ ] **记忆分层与手动维护**：把 memory 分为 user/project/session/pack scopes；先做 read-only loading 和手动编辑，再考虑自动总结/抽取增强。
- [ ] **Provider / adapter 扩展**：在能力画像基础上继续规范 Anthropic、OpenAI-compatible、Gemini、本地模型适配器、schema dialect adaptation 与 streaming normalization。
- [ ] **应用体验补齐**：增加 i18n 中英文切换；拆分 MiSans、KaTeX 和前端 chunks 以瘦身安装包。

## Optional / 方向探索

- [ ] **TTS adapter**：优先接外部 HTTP 服务，例如 GPT-SoVITS、CosyVoice。
- [ ] **ASR adapter**：支持热键或按钮触发的语音输入。
- [ ] **流式语音合成**：复用流式文本，按句切分发送给 TTS。
- [ ] **桌面宠物壳**：透明置顶窗口、Live2D、点击穿透、主动提醒、可选屏幕感知。
- [ ] **运行时 scheduled tasks**：作为中期能力探索，等 Session Engine、权限和 durable workflow 稳定后再接入。
- [ ] **读-only subagent 与复杂编排**：继续演进 parallel research、code review workflow、verification workflow、workflow monitor UI，但避免过早引入完整多 Agent 复杂度。
- [ ] **向量检索实验**：继续保持 Markdown memory 为主，只在 Markdown 记忆规模或检索质量成为瓶颈时评估 vector search/RAG。

## Not now / 暂不做

- [ ] **完整 JavaScript workflow engine**：当前继续维护 Rust 原生 JSON DSL。
- [ ] **全自动远程执行环境**：当前聚焦本地桌面和本地沙箱。
- [ ] **Remote Control / ACP / Artifact Hosting**：等核心架构稳定后再考虑完整 Remote Control Server、ACP 链接和 Artifact Hosting。
- [ ] **Computer Use / Chrome Use 高阶自动化**：当前只保留屏幕截图、OCR、窗口列表等本地基础能力，不复制完整 native Computer/Chrome 实现。
- [ ] **Bun/Node sidecar 核心依赖**：长期核心保持 Rust，不把 Bun/Node sidecar 变成核心依赖。
- [ ] **直接复制 Claude Code Best 的复杂度**：不要照搬 Ink UI/REPL、Commander CLI tree、raw shell hooks、CCB feature flags 等；优先本地桌面 Agent 的安全、可解释和可恢复。

## 维护提示

- 架构结构见 [IMPLEMENTATION.md](./IMPLEMENTATION.md)。
- 设计背景见 [demiurge-mvp-design.md](./demiurge-mvp-design.md)。
- 定期把架构路线图和主线差距同步回本文件，保持 TODO 单一入口。
- 提交前至少运行 `npm run build` 和 `cargo test --manifest-path src-tauri/Cargo.toml`。
- 不要提交具体角色素材、语音/美术资产，或基于特定受版权作品的人格设定。
