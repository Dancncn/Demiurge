# TODO / 路线图

Demiurge 当前已经从 MVP 进入 Agent 能力打磨阶段。这里记录已实现能力和下一步路线，方便协作时快速判断优先级。

## 已实现

- [x] **桌面应用骨架**：Tauri + Rust 后端、React 前端、多会话 UI、设置弹窗、工具卡片、确认弹窗。
- [x] **流式 Agent Loop**：模型流式输出、工具调用、工具结果回填、多轮循环、用户中断。
- [x] **Provider Adapter**：OpenAI-compatible、local、Anthropic、Gemini。
- [x] **工具注册表**：统一 metadata、JSON Schema、权限策略、并发策略和输出策略。
- [x] **文件沙盒**：读写限定沙盒目录，词法校验 + canonicalize 防逃逸。
- [x] **代码工具**：`read_file`、`write_file`、`edit_file`、`multi_edit`、`apply_patch`、`undo_edit`。
- [x] **搜索导航工具**：`glob`、`grep`、`git_status`、`web_fetch`。
- [x] **Shell 工具**：确认门、沙盒 cwd、超时、输出截断、执行前预览。
- [x] **Web Search / Fetch**：Bing、DuckDuckGo fallback、Tavily、Brave、Exa adapter，域名过滤、结果数量、context cap、Sources 提醒；`web_fetch` 支持 direct 抓取和 Exa `livecrawl` 深抓取。
- [x] **权限系统**：本次/会话/项目级确认，轻量 audit log。
- [x] **上下文工程**：system prompt 分区、项目指令、角色设定、运行环境、会话摘要、memory、token-aware 裁剪。
- [x] **Context Collapse**：`/compact`、`context_inspect`、`context_collapse`。
- [x] **长期记忆**：自动提取偏好和项目约束，写入 `.demiurge/memory.md`。
- [x] **Dream 记忆整理**：`/dream` 入口和保守整理流程。
- [x] **Goal 持续驱动**：`/goal`、token budget、pause/resume/continue、自动续跑、模型 `goal` 工具。
- [x] **多 Agent 首版**：`/ultracode`、只读 `agent_spawn`、fork context。
- [x] **Deferred Tools**：`tool_search` / `execute_tool` 按需发现和执行低频工具。
- [x] **Workflow Journal / Resume**：JSONL journal、`/workflows`、`/workflow resume <run_id>`。
- [x] **Workflow JSON DSL / Live Panel**：`agent`、`parallel`、`pipeline`、`phase`、`budget`、`log` step，顶部 Workflows 面板 run/stop/resume。
- [x] **Worktree Isolation**：`worktree_create` 创建隔离 worktree。
- [x] **Computer Use 首层能力**：窗口列表、屏幕截图、区域/窗口 OCR、本地 OCR 模型下载。
- [x] **Voice API 预留**：TTS/ASR adapter 接口保留，具体后端待选型。
- [x] **API Key 安全存储**：LLM、Tavily、Brave、Exa、WebDAV 密钥使用系统凭据管理器，`settings.json` 只保存非密钥配置。
- [x] **设置与备份**：设置页覆盖 provider、Web Search、OCR、语音占位和 WebDAV 备份；WebDAV 支持连接检查、手动备份、备份列表和删除。
- [x] **项目记忆审计 UI**：设置面板可查看、编辑、删除和应用重复记忆去重。
- [x] **自定义 Agent 模板**：`.demiurge/agents/*.json` 支持 prompt、allowed tools、budget、handoff format 和团队组合，前端顶栏可多选 Agent。
- [x] **子 Agent 证据包与多 reviewer**：`agent_spawn` 支持 `output_format=evidence_packet`、`reviewer_count` 和 `max_total_tokens` 硬预算。

## P0：下一阶段主线

- [ ] **MCP 接入**：配置 stdio server，支持启动/停止/健康检查、tool discovery、resource 读取、权限风险映射和设置 UI。
- [ ] **Plan Mode**：引入真正的“先计划、后执行”模式；生成计划文件，用户批准边界后再允许写入、shell、外部发布类操作。
- [ ] **Provider Capability Profile 2.0**：补 prompt caching、thinking、parallel tool calls、max token 差异、结构化输出能力，避免高级 provider 行为散落在 runner/tool 层。
- [ ] **Workflow durable run**：把 live run 从进程内状态升级为跨进程 durable background execution；恢复时不仅生成 resume overlay，还能恢复 run 状态、取消状态和预算状态。

## P1：已落地能力的成熟化

- [ ] **子 Agent 证据包强校验**：把 evidence packet 从提示词约束升级为 provider-level structured output / JSON schema 校验，并引入专门 judge/synthesizer 回合。
- [ ] **Agent JSON 编辑器**：在 UI 中创建、编辑、校验 `.demiurge/agents/*.json`，补示例模板、导入/导出和 schema 校验。
- [ ] **Goal 状态 UI**：显示当前 goal、状态、预算、pause/resume/continue/clear 控制，并把自动续跑进度做成更清晰的状态条。
- [ ] **进度与错误体验**：给 Web Search、长 workflow、Goal 续跑补更细进度 UI；LLM、网络、工具错误更友好地显示在聊天流里，并提供重试提示。
- [ ] **Shell / 进程隔离增强**：在现有 strict policy 基础上评估平台级 sandbox、命令 allowlist/denylist 可视化、联网/依赖安装/外部执行策略配置。

## P2：产品体验补全

- [ ] **设置里测试连接**：验证 provider、base_url、model、LLM key、Web Search key 和 WebDAV 配置是否可用。
- [ ] **权限规则可视化管理**：在已有 permission panel 基础上补更细的规则说明、搜索/过滤和批量清理。
- [ ] **上下文可视化**：展示 system/tools/history/output reserve、summary、memory、可折叠消息和预算消耗细节。
- [ ] **WebFetch / WebSearch adapter 抽象**：抽共享解析、清洗、来源整理模块，减少重复代码，并补来源质量提示和 provider 边缘行为测试。
- [ ] **OCR 体验补全**：模型源选择、下载进度、缺模型引导、国内镜像说明。
- [ ] **角色包头像与导入器**：读取 `manifest.avatar` 替换默认头像；支持拖入 zip，解压到 `packs/` 并校验 manifest。
- [ ] **更多内置工具**：`list_dir`、`http_get`、`clipboard` 等，按权限分级加入。

## 可选方向

- [ ] **TTS adapter**：优先接外部 HTTP 服务，如 GPT-SoVITS、CosyVoice。
- [ ] **ASR adapter**：语音输入管线，支持热键或按钮触发。
- [ ] **流式逐句合成**：复用现有流式文本，按句切分送入 TTS。
- [ ] **桌宠外壳**：透明置顶窗口、Live2D、点击穿透、主动提醒、屏幕感知。
- [ ] **i18n**：界面文案中英文切换。
- [ ] **打包瘦身**：按需拆分 MiSans、KaTeX 和前端 chunks。

## 暂不做

- [ ] **向量库 / 长期记忆 RAG**：当前先保持 Markdown memory；等 Markdown 记忆规模或检索质量成为瓶颈再评估。
- [ ] **完整 JS workflow engine**：当前先维护 Rust 原生 JSON DSL。
- [ ] **全自动远程执行环境**：当前聚焦本地桌面和本地沙盒。
- [ ] **Remote Control / Computer Use 高阶自动化**：当前只保留屏幕截图、OCR、窗口列表等保守能力。

## 贡献提示

- 技术结构见 [IMPLEMENTATION.md](./IMPLEMENTATION.md)。
- 设计背景见 [demiurge-mvp-design.md](./demiurge-mvp-design.md)。
- 提交前建议运行 `npm run build` 和 `cargo test --manifest-path src-tauri/Cargo.toml`。
- 不要把具体角色素材、语音、美术或基于特定作品的人格设定提交进仓库。
