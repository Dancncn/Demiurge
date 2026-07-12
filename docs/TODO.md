# TODO / 路线图

> 文档状态：2026-07-12 已重新核对完成项与剩余项；本轮代码审查的证据和优先级见 [代码审查报告](./CODE-REVIEW-2026-07-12.md)。

Demiurge 当前已经具备本地桌面 Agent 的主体能力：会话、工具、权限、上下文、记忆、工作流、角色卡和本地 Lorebook RAG。这个文档先记录已经完成的功能，再列出已有雏形但仍需要打磨的缺口，最后保留下一阶段的陪伴向路线。

## 已实现能力账本

### 桌面应用与基础体验

- [x] **Tauri 桌面底座**：Rust 后端、React 前端、Vite 构建、Tauri dev/build 流程和桌面窗口集成。
- [x] **会话 UI**：侧栏会话列表、消息流、输入框、工具卡片、设置弹窗、状态栏和基础错误展示。
- [x] **会话持久化**：多会话保存、恢复、重命名、删除、活跃会话切换和基础统计。
- [x] **会话级项目文件夹**：选择本地项目、随会话保存路径、切换会话恢复项目；生成回复期间禁止跨项目切换。
- [x] **项目浏览器**：目录懒加载、重目录过滤、限长文本预览、二进制识别、未提交文件列表和手动刷新。
- [x] **Git 分支入口**：查看/搜索本地与远程分支、显示当前分支和脏状态、切换前提示、冲突安全失败。
- [x] **编辑活动与流式动效**：编辑工具显示受影响文件并可展开详情；流式文字按帧合并、尾部词片段淡入并尊重 reduced-motion。
- [x] **Settings 面板**：Provider、Web Search、OCR、Memory、Context、WebDAV、Permission、Shell、MCP、Voice 等设置入口。
- [x] **提交前门禁**：`cargo fmt --check`、Rust 单元测试、前端构建可以作为提交前验证基线。

### Agent Loop 与上下文工程

- [x] **Agentic Loop**：支持模型流式响应、工具调用、多轮工具回合、取消、中断和 turn 状态同步。
- [x] **System Prompt 分层**：基础人格、角色包 persona、技能、项目指令、记忆、摘要、Goal、环境、工具与安全规则分层组装。
- [x] **Prompt 预算报告**：每个 prompt section 有优先级、字符数、估算 token、包含/截断状态，供 Context 面板查看。
- [x] **Token-aware 历史裁剪**：按 provider/model 能力画像估算输入预算，并裁剪历史消息。
- [x] **上下文折叠**：支持对长会话进行摘要压缩，降低长期会话上下文压力。

### 记忆与 Dream

- [x] **分层 Markdown 记忆**：支持 user、project、session、pack 记忆文件。
- [x] **Memory 面板**：查看、添加、编辑、删除、去重记忆条目。
- [x] **自动记忆抽取**：可从对话中提取长期偏好和事实，写入对应记忆层。
- [x] **`/dream` 后台整理**：支持记忆整理、审计、去重和结果回写。
- [x] **Context 记忆来源可视化**：可以查看当前上下文引用了哪些记忆来源。

### Goal、多 Agent 与 Workflow

- [x] **Goal 持续驱动**：`/goal`、token budget、pause/resume/continue/clear、自动续写和状态栏控制。
- [x] **自定义 Agent**：支持 JSON 定义 Agent 名称、说明、prompt、允许工具、输出格式和预算。
- [x] **子 Agent 调度**：`agent_spawn` 支持并行任务、上下文裁剪、预算 footer 和结果回传。
- [x] **Reviewer / 证据包**：支持 reviewer 协作和 evidence packet 格式约束。
- [x] **Workflow JSON DSL**：支持 durable workflow run、phase、agent step、budget、journal、resume 和 stale run 恢复。
- [x] **Workflow 面板**：展示定义、运行状态、日志、phase 和 agent 结果。

### 工具系统与安全

- [x] **文件与编辑工具**：读文件、列目录、搜索、补丁编辑、多文件编辑、undo 和漂移保护。
- [x] **Shell 工具**：命令风险分类、超时、环境变量清理、进程树终止、严格策略和隔离规格。
- [x] **搜索与导航工具**：文件 glob/grep、目录快照、package scripts、worktree 辅助。
- [x] **Web Search / Fetch**：Bing、DuckDuckGo fallback、Tavily、Brave、Exa；支持结果过滤、缓存、来源提示和 fetch adapter。
- [x] **Clipboard 工具**：平台剪贴板读写命令选择和安全校验。
- [x] **权限系统**：工具风险分级、一次/会话/项目级授权、权限审计、规则编辑和默认策略展示。
- [x] **凭据管理器**：API Key 与敏感 MCP 环境变量通过凭据存储，不写入普通 settings 文件。

### Provider 与模型能力

- [x] **多 Provider adapter**：OpenAI-compatible、local、Anthropic、Gemini 等 provider 路由。
- [x] **连接测试**：Provider 和 Web Search key/base URL/model 的测试入口。
- [x] **模型能力画像**：reasoning effort、工具能力、token budget、provider 特有参数和流式 usage 规范化。
- [x] **Reasoning Effort**：支持 `auto/low/medium/high/xhigh/max`，并按 provider/model 能力自动降级。

### Computer Use 与 OCR

- [x] **窗口与屏幕基础能力**：窗口列表、屏幕截图、区域截图、点击和输入入口。
- [x] **OCR 模型管理**：Settings OCR 面板、模型状态、下载源选择、缺失文件提示和手动安装说明。
- [x] **OCR 调用链路**：屏幕/图片 OCR 能力接入后端，作为 Computer Use 的感知底座。

### Voice

- [x] **录音输入**：前端录音入口、设备选择和 voice 状态展示。
- [x] **ASR/STT adapter**：`voice_transcribe` 支持 DashScope ASR 和 OpenAI-compatible Whisper。
- [x] **TTS 已接通 DashScope + GPT-SoVITS 双后端**（保留配置字段与 adapter 接口，`voice.rs:193-249`；dashscope 默认音色 Cherry/模型 `qwen3-tts-flash`，gpt-sovits 默认 base `http://127.0.0.1:9880`）。

### 角色包、角色卡与 Lorebook RAG

- [x] **角色包基础**：`packs/<id>/manifest.json`、`persona.md`、头像导入、zip 导入和路径安全校验。
- [x] **manifest 2.0**：新增 Character Card 与 Runtime Capability 两层结构。
- [x] **Character Card**：身份、背景、人格、说话风格、称呼、口癖、禁用表达、关系、开场白、示例对话和 OOC 规则。
- [x] **Runtime Capability**：角色级 Skill 推荐/禁用/关键词自动激活、Memory 策略、Voice 偏好和 Permission 偏好。
- [x] **角色卡运行时注入**：会话启动时把 persona、Character Card、Runtime Policy 和 Lorebook Index 合入上下文。
- [x] **pack-scoped Skill**：角色包内 `skills/<skill>/SKILL.md` 可作为角色专属技能单元。
- [x] **Skill 绑定策略**：角色卡可推荐、禁用或按关键词自动激活对应 skill。
- [x] **Settings 角色卡编辑**：支持查看、编辑、保存、导出当前角色卡 manifest JSON。
- [x] **Lorebook RAG**：支持 `lore/*.md`、`.txt`、目录递归、frontmatter `title/tags/keywords/priority`。
- [x] **Lorebook 分块索引**：按 Markdown 标题和段落分块，缓存到本地索引；按文件集合、大小和修改时间失效。
- [x] **Lorebook 检索注入**：按当前用户输入进行短语匹配、中文 ngram 和 BM25 稀疏召回，注入 `Retrieved Lorebook`。
- [x] **Lorebook UI**：Settings 中展示 lorebook 条目、添加目录模板、输入查询并预览真实召回片段。
- [x] **默认角色包示例**：`packs/default` 展示 persona、manifest 2.0、lore 目录和 pack tone guard skill。

## P0/P1 / 代码审查修复队列

- [ ] **角色包权限只能收紧**：未受信 manifest 不得把 Ask/Deny 降为 Allow；若要放宽，必须独立展示权限差异并按包指纹取得显式用户授权。
- [ ] **会话—工作区原子快照**：用 navigation epoch 或后端原子命令绑定 `session_id + history + workspace + goal`；所有慢响应和 legacy 事件必须验证 session/turn 归属。
- [ ] **项目/会话权限真正隔离**：Project scope 按 canonical 项目根分桶；Session scope 按 session id 分桶并在切换后不串用。
- [ ] **撤销记录绑定项目身份**：undo entry 保存 canonical workspace/root 与目标绝对身份，切换项目后只允许撤销同一根内记录。
- [ ] **角色包 IPC 根目录校验**：所有 pack id 在 join 前验证；列表、读取、Live2D 与 lore 命令都必须确认解析结果仍在 `packs_dir`。
- [ ] **Live2D 事务化安全导入**：不可信资源引用拒绝绝对路径和 `..`，重命名也做 containment；先在临时目录完整校验，再原子替换旧模型。
- [ ] **公开 URL SSRF 防护**：拒绝 loopback、私网、链路本地、未指定/保留地址和凭据 URL；逐跳解析 DNS 并复核重定向目标。
- [ ] **Windows 系统打开去除命令解释器**：避免 `cmd /C start` 处理不可信目标；至少拒绝/正确封装 shell 元字符，并为 URL/路径添加注入回归测试。
- [ ] **deferred 工具按目标授权**：`execute_tool` 的记忆规则包含实际 tool name，或在 wrapper 内再次走目标工具权限门。
- [ ] **MCP 注解只作提示**：外部 server 自报 read-only 不能让 Auto 自动放行；动态工具风险下限保持 External/Privileged。
- [ ] **分支命令验证 expected workspace**：列表缓存按项目失效，loading 时不可点击；后端切换时比较调用方看到的项目根。

## P2/P3 / 正确性、协议、体验与门禁修复队列

- [ ] **恢复 Rust 格式门禁**：修正 `src-tauri/src/pack/live2d.rs:303` 的 rustfmt 差异，确保全仓 `cargo fmt -- --check` 通过。
- [ ] **历史工具状态结构化**：持久化 ok/denied/failed、错误、耗时和受影响路径；未知状态不显示为绿色成功。
- [ ] **第三种供应商 SSE 对齐**：复用公共解码器，处理无换行流尾、多行 data、命名/内嵌错误，并拒绝静默 JSON 丢弃。
- [ ] **完整流终止校验与解码上限**：缺少协议终止事件的 clean EOF 不能归一为 stop；为单行、单事件和累计缓冲设置字节上限。
- [ ] **canonical done 与统一事件信封**：最终完整正文修复漏 delta；主时间线按 session/turn reducer 消费统一事件。
- [ ] **流式渲染性能**：历史 Markdown/ToolCard 保持稳定 memo，自动滚动尊重用户位置，Mermaid 只在流完成后渲染一次。
- [ ] **工作区组件竞态与响应式/无障碍**：目录、changes、preview 使用 generation；Git→非 Git 回到 Files；项目面板在窄窗口改为 drawer，并补键盘/ARIA。
- [ ] **前端测试基线**：增加组件、延迟竞态、跨会话事件、流式 fixture、性能、980/1280/1811px 布局和可访问性回归。

## 已有雏形但需要优化

### 本批次已完成（原型打磨）

- [x] **结构化角色卡编辑器**：`src/components/pack-editor/` 表单化编辑 Character Card / Runtime / Lorebook / 示例对话 / OOC 规则，保留可折叠原始 JSON 回退；复用 `read_pack_manifest_json` / `save_pack_manifest_json` 往返。
- [x] **角色包素材管理**：`open_pack_dir` / `import_pack_lore_files` / `list_pack_files` / `read_pack_file` 命令 + `PackFileBrowser`；`PackManifest` 增 `credits` / `license`，`import_pack_zip` 返回 `PackImportResult { manifest, warnings }` 并在前端展示授权缺失警告。
- [x] **Lorebook 召回可视化**：`lorebook_index_status` / `lorebook_recall_detail` / `lorebook_rebuild_index` 命令 + `LorebookRecallPanel`（chunk 列表、命中关键词高亮、score、索引状态、手动重建）；`/recall <query>` slash 命令。
- [x] **向量 RAG / embedding（远程优先脚手架）**：`src-tauri/src/embed/` 的 `EmbeddingProvider` trait + 远程 OpenAI 兼容 provider + `LoreChunk.embedding` 缓存 + RRF 混合召回权重（`hybrid_weight`）。详见 [docs/modules/20-lorebook-vector-rag.md](./modules/20-lorebook-vector-rag.md)。
- [x] **Memory namespace 落地**：`scope_files` 按 `runtime.memory.namespace` 隔离 user/project 到 `user.{ns}.md` / `memory.{ns}.md`，default 走 legacy；`memory_migrate_namespace` 命令 + 面板迁移入口；`/dream` 与自动抽取写 namespaced 路径。
- [x] **Permission preference 强约束**：`CharacterRuntime.permissions` 解析为 `CardOverlay` 决策，插入 `decide` 的 user 规则与 tool 默认之间；`ask_every_time` 禁止持久化；`permission_panel_state` 暴露 `card_preference`。
- [x] **前端体积治理**：Markdown/KaTeX/highlight、Mermaid、PDF、ZIP、Live2D 已按需加载或独立 vendor chunk。

### 本批次继续打磨（已完成）

- [x] **#2 lore 导入后自动重建索引**：`import_pack_lore_files` 成功后 best-effort 调 `lorebook_rebuild_index`，导入的 lore 立即可见，不必等下次查询或手动重建。
- [x] **#3 IDF 预计算缓存**：`LoreIndexCache` 增 `stats`（document_count/average_len/document_frequency），建索引时预算、chunks 重建时刷新；`score_all_lore_hits` 改为接收缓存 stats，避免每查询 O(n) 重算。
- [x] **#4 embedding 失败降级提示**：`LoreRecallDetail` 增 `embedding_status`（disabled/ok/degraded）；`/recall` 输出状态行 + 每条 `dense_score`；`LorebookRecallPanel` 加状态徽标。
- [x] **#4 维度自动探测**：`embedding_probe` 命令 + `EmbeddingProbeResult`，前端"测试连接/探测维度"按钮成功后自动回填 `embedding_dims`。
- [x] **#4 embedding_api_key 凭据化**：已由前会话完成——`credentials.rs` 的 `load/save_embedding_api_key` + `hydrate_or_migrate_settings` 启动从 keyring 加载、`redacted_settings` 落盘前清空；测试 `save_settings_does_not_persist_api_key` 已断言。
- [x] **#5 namespace 迁移下拉 + legacy 提示**：迁移目标改为列出已有 pack namespace 的 Select；namespace 激活时面板提示"legacy user.md/memory.md 仍保留，可从 default 迁移导入"。
- [x] **#6 ask_once 自动 session-remember**：`remember_response` 在 card 偏好为 `ask_once` 且用户选 Once 时自动升级为 Session 持久化，落实"每会话只问一次"语义；`ask_every_time` 仍禁止持久化。

### 仍需打磨（按特性）

**#1 结构化角色卡编辑器**
- 表单 ↔ 原始 JSON 双向同步（当前表单为单一真源、JSON 只读；未来开放直接改 JSON 需 diff 回写）。
- `AutoSkillBinding.when` 嵌套列表增删/重排 UX（drag-handle + 去重提示）。
- 表单校验内联提示（`save_pack_manifest_json` 失败时把错误定位到字段，而非仅弹错误条）。
- i18n 覆盖（过一遍 `settings.card.*` keys，补可能仍走英文硬编码的字段）。

**#2 角色包素材管理**
- `PackFileBrowser` 大文件/二进制预览（图片缩略图、分页、大目录接近 `MAX_PACK_LIST_ENTRIES=1000` 时懒加载）。
- credits 编辑器改卡片式（`AssetCredit` 4 字段不再挤一行）。
- 批量 lore 导入逐文件进度。

**#3 Lorebook 召回可视化**
- CJK 关键词高亮（`matched_terms` 对中文 ngram 命中高亮 bigram/trigram 子串，非仅整词）。
- `/recall` 与面板联动（slash 结果高亮跳转面板，或面板复用同一渲染）。

**#4 向量 RAG / embedding**
- 本地 fastembed 真实推理（`embeddings-local` feature 桩已留；后续接 `fastembed` crate + BAAI/bge-small-zh-v1.5 + 模型可选下载复用 OCR UX）。
- Cross-encoder reranker（`select_lore_hits` top-N 后、`render_lore_hits` 前插 `reranker`/`ort` + MiniLM）。
- 热路径线程开销（每 turn query embed 起独立 OS 线程；可改 `tokio::task::block_in_place` 或复用 runtime 句柄）。

**#5 Memory namespace**
- session 层不隔离的面板说明（ephemeral 设计；跨 pack 切换会话可能含混合 namespace 事实）。
- 迁移策略单一（只复制不合并/去重/重命名；可在 pack 切换时检测 namespace 变化提示迁移）。
- 多 pack 冲突策略（当前单 active pack 无冲突；未来多 pack 叠加需定义"最严格 wins"）。

**#6 Permission preference 强约束**
- 工具列表汇总"哪些工具被 card 覆盖"列（当前 `card_preference` 只在选中工具详情展示）。
- card 偏好指向未知工具的提示（`Default` 兜底时不报错但无提示）。
- 多 pack 冲突策略（同 #5）。


### 其他未完成特性（原列表保留）

- [x] **TTS 双后端已接通**（dashscope + gpt-sovits，音色/模型可配，`voice.rs:193-249`）。
- [ ] **流式合成、播放队列、打断、语速/情感参数**：当前为非流式一次性合成；尚需按句切分播放、中断、静音、音色选择和语速配置。
- [ ] **Computer Use 自动化闭环**：已有截图/OCR/点击/输入底座，但还缺少完整浏览器/桌面任务规划、可视化确认和失败恢复。
- [ ] **Workflow 编辑体验**：已有 JSON DSL 和面板，但仍需要更友好的表单编辑、模板库、dry-run 和失败节点重试。
- [ ] **文档同步**：README、IMPLEMENTATION 和模块文档需要随角色卡/Lorebook RAG 的最终交互继续补充截图和使用例。

## P1 / 情感陪伴核心

- [x] **陪伴状态模型雏形**：Settings 已支持心情、精力、专注状态、偏好语气、免打扰时段；聊天页有 Companion 卡片展示状态与建议。
- [x] **陪伴记忆建议雏形**：Settings 可把“喜欢怎样被提醒”“免打扰时段”“天气陪伴城市”等稳定偏好手动写入用户级记忆，并继续通过 Memory 面板审计/删除。
- [x] **主动关怀策略雏形**：后端 `companion_panel_state` 会基于陪伴状态和天气生成克制建议，并通过 `Companion Context` 注入对话上下文；后续再接后台低频触发、番茄钟状态和通知权限。
- [x] **情绪支持回复风格雏形**：已支持安静、温柔、元气、吐槽、效率教练等语气档位，并进入 Companion 状态。
- [x] **安全边界雏形**：Settings 和 `Companion Context` 中加入陪伴安全边界；后续需要把高风险表达检测接入对话运行时和记忆审计。

## P2 / 天气与本地生活陪伴

- [x] **天气 Provider API 雏形**：新增 Open-Meteo 后端查询，支持手动城市、内存缓存、缓存清理、失败静默降级和隐私说明；自动定位仅预留配置位。
- [x] **天气陪伴卡片雏形**：聊天页 Companion 卡片展示城市、天气、温度、体感和天气建议。
- [x] **天气驱动关怀雏形**：根据降雨、高温、低温、风力生成轻量提醒，例如带伞、补水、保暖和通勤留意。
- [x] **隐私设置增强**：已支持手动城市/关闭天气/粗略定位城市估算/weather provider 可选项/天气与位置缓存清理，并在 Settings 中展示数据保留说明。

## P1/P2 继续增强

- [x] **陪伴记忆待确认队列**：把当前“手动写入记忆建议”升级为队列化流程；建议项需要有来源会话、建议原因、目标 scope、kind、正文、创建时间和状态（待确认/已保存/已忽略）。
- [x] **LLM 陪伴记忆抽取**：在用户授权后，从对话中提取压力来源、作息偏好、常用称呼、提醒偏好、讨厌的提醒方式和适合的鼓励方式；先进入待确认队列，而不是直接写入长期记忆。
- [x] **记忆抽取权限与审计**：Settings 需要提供开关、抽取范围说明、最近抽取记录、批量忽略/保存、撤销写入和跳转 Memory 面板入口。
- [x] **陪伴记忆去重与合并**：写入前检查 user memory 中是否已有相近条目；重复时提示合并、替换或保留新条目，避免长期记忆越写越乱。
- [x] **高风险表达检测**：把自伤、危机、医疗/心理治疗替代等风险表达接入运行时检测，触发支持性回复和现实求助建议；检测结果不写入普通记忆，避免形成不必要的敏感持久化。
- [x] **主动提醒调度器**：基于时间、天气、免打扰、最近会话、专注状态生成低频提醒候选；默认只在 Companion 卡片展示，桌面通知需要单独授权。
- [x] **天气 provider 可插拔**：保留 Open-Meteo 作为无 key 默认源，同时抽象 provider 接口，后续可接高德、和风天气或 Web Search fallback；所有 provider 都需要在 Settings 中说明发送的数据。
- [x] **天气数据治理**：天气缓存需要可视化状态、过期时间、手动清理和错误降级说明；如果后续加入粗略定位，必须支持关闭、清除位置缓存和仅保存城市级信息。
- [x] **天气建议细化**：补充空气质量、紫外线、昼夜温差、通勤时段降雨、极端天气预警；建议文案保持克制，避免频繁主动打扰。

## P3 / 番茄钟与节奏陪伴

- [x] **番茄钟基础计时**：专注、短休息、长休息、自定义时长、暂停/继续/跳过和桌面通知。
- [x] **陪伴式专注反馈**：开始前帮用户拆目标，结束后简短复盘，连续专注时给出轻量鼓励。
- [x] **任务绑定**：番茄钟可绑定当前会话、Goal、Workflow 或手动任务标题。
- [x] **节奏记忆**：记录用户偏好的专注时长、常见中断原因和高效时间段，作为后续提醒依据。
- [x] **勿扰联动**：专注中减少主动提醒，只保留用户允许的高优先级提示。

## P4 / 语音与桌面陪伴

- [x] **TTS adapter 已接通**（base URL / 音色 / 非流式 / 并行推断，dashscope + gpt-sovits 双后端，`voice.rs:193-249`）。
- [x] **GPT-SoVITS / CosyVoice 接入要求**：本地 TTS 支持 GPT-SoVITS / CosyVoice alias、语速、情感参数、streaming 请求参数、连接测试、失败降级到 DashScope 和前端播放队列状态；本地语音模型仍不打进默认安装包。
- [x] **流式语音合成**：复用模型流式文本，按句切分进入播放队列，支持打断、静音、音色选择和语速配置。
- [x] **语音通话 / 和角色打电话**：第一阶段已落地回合制语音通话面板（接通/挂断、静音、时长、按键说话、STT → LLM → 流式 TTS、简单 VAD 端点检测和本地半双工打断）；通话记忆与 Live2D 口型/动作联动留给后续专门阶段。
- [x] **语音唤醒/快捷键**：支持应用聚焦时的可配置语音快捷键和按钮触发；全局快捷键与唤醒词仍作为可选实验方向。
- [x] **桌面陪伴壳**：新增非 Live2D 的透明置顶桌面小窗，支持轻量状态展示、点击穿透、可收起/展开和主窗口入口，避免遮挡工作流。
- [x] **Live2D 面板 MVP**：使用 `untitled-pixi-live2d-engine`（PixiJS v8 原生渲染管线，Cubism 2–5）在应用内渲染 Live2D 模型面板；角色包 manifest 新增 `live2d` 字段指向 `.model3.json`；支持文件夹导入、idle 物理/眨眼、缩放和拖拽；Cubism Core 由用户自行下载（`npm run fetch:cubism-core`）。
- [ ] **Live2D 桌宠方案**：在面板 MVP 基础上，做独立透明置顶桌宠窗口，支持透明背景、置顶、拖拽、缩放、隐藏/显示和基础表情动作。
- [ ] **Live2D 资产管理（扩展）**：在已落地的 `live2d` 路径字段基础上，扩展模型版本、默认动作、表情映射和授权说明；导入时校验模型文件、纹理路径和包内相对路径，避免路径穿越。
- [ ] **Live2D 状态映射**：把 Companion 状态映射到表情/动作，例如专注中低动作频率、休息时轻松动作、天气提醒时短动作；动作触发必须低频，避免干扰工作。
- [ ] **Live2D 与语音联动**：TTS 播放时驱动口型或简化嘴型动画；无 TTS 时只做轻量 idle，不做默认常驻麦克风监听。
- [x] **桌宠窗口权限边界**：桌面陪伴壳不默认读取屏幕、麦克风或精确位置；Settings 与桌面小窗展示屏幕/麦克风/位置状态，截图/OCR/语音/天气沿用已有显式权限与状态提示。
- [x] **屏幕感知边界**：截图/OCR/窗口信息继续受 `computer_use_enabled` 和逐次权限确认门控，Settings 的 OCR 区域展示开启/关闭状态和读取边界。

## P5 / 体验与打包

- [ ] **首次启动引导**：引导用户配置 provider、API Key、天气城市、记忆策略、语音和通知权限。
- [ ] **本地数据导出**：导出设置、记忆、番茄钟记录、Goal/Workflow 历史、角色包索引状态，便于迁移和协作排查。
- [ ] **异常可恢复**：后台任务、番茄钟、会话保存和 Workflow 在应用重启后尽量恢复到可解释状态。
- [ ] **打包与模型资产策略**：OCR、后续 TTS/embedding 模型保持可选下载，避免默认包体过大。
- [ ] **前端重包继续拆分**：生产构建已通过，但 Live2D vendor chunk 仍约 1.1 MB 并触发非阻断体积警告。
- [ ] **真实端点契约回归**：离线解析测试已覆盖两类流式协议的正文、思考、工具、usage、流尾和错误；发布前补充真实网络下的限流、取消、代理分片与断流回归。

## 暂不做

- [ ] **全自动远程执行环境**：当前聚焦本地桌面、本地权限和可解释执行。
- [ ] **默认常驻屏幕/麦克风监听**：不做默认屏幕读取、默认常驻麦克风或不可见的位置采集。
- [ ] **大型线上社区/账号体系**：短期内不引入账号、云同步社区和远程角色市场。
- [ ] **未经授权的受版权保护角色资产分发**：项目只提供通用示例，不提交具体受版权保护的角色素材、语音、美术或人格设定。

## 维护提示

- 架构结构见 [IMPLEMENTATION.md](./IMPLEMENTATION.md)。
- 当前审查结论见 [CODE-REVIEW-2026-07-12.md](./CODE-REVIEW-2026-07-12.md)。
- 设计背景见 [demiurge-mvp-design.md](./demiurge-mvp-design.md)。
- 提交前至少运行 `npm run build`、`cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` 和 `cargo test --manifest-path src-tauri/Cargo.toml`；2026-07-12 基线为 Rust 215 项测试通过。
