# TODO / 路线图

Demiurge 当前已经具备本地桌面 Agent 的主体能力：会话、工具、权限、上下文、记忆、工作流、角色卡和本地 Lorebook RAG。这个文档先记录已经完成的功能，再列出已有雏形但仍需要打磨的缺口，最后保留下一阶段的陪伴向路线。

## 已实现能力账本

### 桌面应用与基础体验

- [x] **Tauri 桌面底座**：Rust 后端、React 前端、Vite 构建、Tauri dev/build 流程和桌面窗口集成。
- [x] **会话 UI**：侧栏会话列表、消息流、输入框、工具卡片、设置弹窗、状态栏和基础错误展示。
- [x] **会话持久化**：多会话保存、恢复、重命名、删除、活跃会话切换和基础统计。
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
- [x] **TTS API 预留**：保留语音输出配置字段和 adapter 接口，方便后续接 GPT-SoVITS、CosyVoice 或外部 HTTP 服务。

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

## 已有雏形但需要优化

### 本批次已完成（原型打磨）

- [x] **结构化角色卡编辑器**：`src/components/pack-editor/` 表单化编辑 Character Card / Runtime / Lorebook / 示例对话 / OOC 规则，保留可折叠原始 JSON 回退；复用 `read_pack_manifest_json` / `save_pack_manifest_json` 往返。
- [x] **角色包素材管理**：`open_pack_dir` / `import_pack_lore_files` / `list_pack_files` / `read_pack_file` 命令 + `PackFileBrowser`；`PackManifest` 增 `credits` / `license`，`import_pack_zip` 返回 `PackImportResult { manifest, warnings }` 并在前端展示授权缺失警告。
- [x] **Lorebook 召回可视化**：`lorebook_index_status` / `lorebook_recall_detail` / `lorebook_rebuild_index` 命令 + `LorebookRecallPanel`（chunk 列表、命中关键词高亮、score、索引状态、手动重建）；`/recall <query>` slash 命令。
- [x] **向量 RAG / embedding（远程优先脚手架）**：`src-tauri/src/embed/` 的 `EmbeddingProvider` trait + 远程 OpenAI 兼容 provider + `LoreChunk.embedding` 缓存 + RRF 混合召回权重（`hybrid_weight`）。详见 [docs/modules/20-lorebook-vector-rag.md](./modules/20-lorebook-vector-rag.md)。
- [x] **Memory namespace 落地**：`scope_files` 按 `runtime.memory.namespace` 隔离 user/project 到 `user.{ns}.md` / `memory.{ns}.md`，default 走 legacy；`memory_migrate_namespace` 命令 + 面板迁移入口；`/dream` 与自动抽取写 namespaced 路径。
- [x] **Permission preference 强约束**：`CharacterRuntime.permissions` 解析为 `CardOverlay` 决策，插入 `decide` 的 user 规则与 tool 默认之间；`ask_every_time` 禁止持久化；`permission_panel_state` 暴露 `card_preference`。
- [x] **前端体积治理**：Markdown/KaTeX/highlight、Mermaid、PDF、ZIP、Live2D 已按需加载或独立 vendor chunk。

### 下次打磨（不完善处详述）

> 以下是上述已完成特性里**仍需打磨**的点，按特性分组，列出具体缺口与建议做法，供下一轮迭代直接取用。

**#1 结构化角色卡编辑器**
- 表单 ↔ 原始 JSON 双向同步：当前以表单为单一真源、原始 JSON 只读展示。若用户直接改 JSON（未来开放），需要双向 diff 与校验回写，避免覆盖表单状态。
- 嵌套列表编辑：`AutoSkillBinding.when`（skill + 关键词列表）的增删/重排 UX 较简陋，可加 drag-handle 与去重提示。
- 表单校验内联提示：`save_pack_manifest_json` 后端会校验路径与 persona 存在性，失败仅弹错误条；可前置前端校验并把错误定位到字段。
- i18n 覆盖：部分字段标签可能仍走英文硬编码，需过一遍 `settings.card.*` keys。

**#2 角色包素材管理**
- `import_pack_lore_files` 后未自动刷新 Lorebook 索引：导入的 lore 文件要等下次查询触发缓存失效或手动重建才进召回。建议导入成功后自动调 `lorebook_rebuild_index` 或在面板提示"已导入，点此重建"。
- `PackFileBrowser` 大文件/二进制预览：`read_pack_file` 有 512KiB 截断，但图片预览无缩略图、无分页；大目录（接近 `MAX_PACK_LIST_ENTRIES=1000`）无懒加载。
- credits 编辑器：当前是 `KVRows` 式行编辑，`AssetCredit` 的 4 字段（asset/author/source/license）挤在一行，可改卡片式。
- 批量 lore 导入无进度：多文件上传时无逐文件进度，仅成功后刷新。

**#3 Lorebook 召回可视化**
- IDF 预计算：`lore_search_stats` 每次查询都重算 document_frequency（O(n) in chunks）；500 文件 ×数千 chunk 时有感延迟，可缓存 IDF 到 `LoreIndexCache`。
- CJK 关键词高亮：`matched_terms` 对中文 ngram 命中可能只整词高亮，不高亮命中的 bigram/trigram 子串。
- `/recall` 与面板割裂：slash 在对话里输出文本卡片，面板是独立 UI；可让 `/recall` 结果也高亮跳转到面板，或面板复用同一渲染。

**#4 向量 RAG / embedding**
- 本地 fastembed 真实推理：`embeddings-local` cargo feature 已留 `LocalEmbeddingProvider` 桩，但 `embed` 直接返回错误。后续接 `fastembed` crate + BAAI/bge-small-zh-v1.5 + 模型可选下载（复用 OCR 下载 UX）。
- Cross-encoder reranker：`select_lore_hits` 取 top-N 后、`render_lore_hits` 前可插 reranker（`reranker`/`ort` + MiniLM）；当前未接入。
- `embedding_api_key` 凭据化：目前存 settings（明文），应迁入 `credentials.rs` keyring，与 LLM API Key 一致；字段注释已标 TODO。
- 维度自动探测：用户需手填 `embedding_dims`；可加"连接测试"时拉一次 embedding 自动填维度（复用 `provider_check_connection` 模式）。
- embed 失败静默降级：`ensure_chunk_embeddings` / `select_lore_hits` 在 provider 报错时静默回落纯 BM25，用户无感知。应在 recall 面板与 Context 报告里标注"embedding provider 不可用，已降级"。
- 热路径线程开销：每 turn 一个 query embed 起独立 OS 线程；可改 `tokio::task::block_in_place`（Tauri multi-thread runtime 下安全）或复用 runtime 句柄，避免线程创建开销。

**#5 Memory namespace**
- session 层不隔离：`session-memory/{session_id}.md` 不受 namespace 影响（ephemeral 设计），但跨 pack 切换的会话可能含混合 namespace 事实；需在面板说明 session 不隔离的原因。
- 迁移策略单一：`memory_migrate_namespace` 只复制（源保留），不合并、不去重、不重命名。切 namespace 后 legacy 文件孤立需手动 migrate；可在 pack 切换时检测 namespace 变化并提示"是否迁移旧记忆"。
- namespace 输入是自由文本：迁移表单让用户手输目标 namespace，无下拉选已有 pack 的 namespace。可列出所有 pack 的 namespace 供选择。
- 面板无 legacy 回退提示：切到 namespace 后，面板只显示 namespaced 路径，不提示"legacy `user.md` 仍存在、可用迁移导入"。

**#6 Permission preference 强约束**
- `ask_once` 语义不完整：当前 `ask_once` 与 `ask_every_time` 都只触发 Ask 弹窗；`ask_once` 不会自动 session-remember（用户仍要在弹窗选 scope）。应让 `ask_once` 在用户首次 allow 后自动落 Session 规则，`ask_every_time` 强制 Once（已实现）。
- card overlay 仅在无 user/project/session 规则时生效：这是设计（显式用户规则优先），但用户设了 project allow 又被 card deny 时会困惑；面板已展示 `card_preference`，但工具列表未汇总"哪些工具被 card 覆盖"，可加一列。
- 默认 pack manifest 的 `weather`/`screen_ocr` 偏好可能指向不在 registry 的工具：当前 `Default` 兜底（不报错），但用户无提示；可在面板标注"card 偏好指向未知工具"。
- 多 pack 冲突：当前只一个 active pack，无冲突；未来若支持多 pack 叠加，需定义"最严格 wins"策略。

### 其他未完成特性（原列表保留）

- [ ] **Voice TTS 闭环**：ASR 已接入，TTS 仍是预留接口；需要流式合成、播放队列、打断、音色和语速配置。
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

- [ ] **TTS adapter**：保留统一 API，优先支持外部 HTTP 服务，便于后续接 GPT-SoVITS、CosyVoice 或其他本地/云端方案。
- [ ] **GPT-SoVITS / CosyVoice 接入要求**：TTS provider 需要支持 base URL、模型/音色 ID、语速、情感参数、流式/非流式模式、连接测试、失败降级和播放队列状态展示；不要把本地语音模型打进默认安装包。
- [ ] **流式语音合成**：复用模型流式文本，按句切分播放，支持打断、静音、音色选择和语速配置。
- [ ] **语音唤醒/快捷键**：支持全局快捷键或按钮触发语音输入；唤醒词作为可选实验能力。
- [ ] **桌面陪伴壳**：透明置顶窗口、轻量状态展示、点击穿透、可收起/展开，避免遮挡工作流。
- [x] **Live2D 面板 MVP**：使用 `untitled-pixi-live2d-engine`（PixiJS v8 原生渲染管线，Cubism 2–5）在应用内渲染 Live2D 模型面板；角色包 manifest 新增 `live2d` 字段指向 `.model3.json`；支持文件夹导入、idle 物理/眨眼、缩放和拖拽；Cubism Core 由用户自行下载（`npm run fetch:cubism-core`）。
- [ ] **Live2D 桌宠方案**：在面板 MVP 基础上，做独立透明置顶桌宠窗口，支持透明背景、置顶、拖拽、缩放、隐藏/显示和基础表情动作。
- [ ] **Live2D 资产管理（扩展）**：在已落地的 `live2d` 路径字段基础上，扩展模型版本、默认动作、表情映射和授权说明；导入时校验模型文件、纹理路径和包内相对路径，避免路径穿越。
- [ ] **Live2D 状态映射**：把 Companion 状态映射到表情/动作，例如专注中低动作频率、休息时轻松动作、天气提醒时短动作；动作触发必须低频，避免干扰工作。
- [ ] **Live2D 与语音联动**：TTS 播放时驱动口型或简化嘴型动画；无 TTS 时只做轻量 idle，不做默认常驻麦克风监听。
- [ ] **桌宠窗口权限边界**：桌宠不默认读取屏幕、麦克风或精确位置；截图/OCR/语音/天气都必须使用已有显式权限和可见状态提示。
- [ ] **屏幕感知边界**：截图/OCR/窗口信息必须经过明确权限开关和可见状态提示。

## P5 / 体验与打包

- [ ] **首次启动引导**：引导用户配置 provider、API Key、天气城市、记忆策略、语音和通知权限。
- [ ] **本地数据导出**：导出设置、记忆、番茄钟记录、Goal/Workflow 历史、角色包索引状态，便于迁移和协作排查。
- [ ] **异常可恢复**：后台任务、番茄钟、会话保存和 Workflow 在应用重启后尽量恢复到可解释状态。
- [ ] **打包与模型资产策略**：OCR、后续 TTS/embedding 模型保持可选下载，避免默认包体过大。

## 暂不做

- [ ] **全自动远程执行环境**：当前聚焦本地桌面、本地权限和可解释执行。
- [ ] **默认常驻屏幕/麦克风监听**：不做默认屏幕读取、默认常驻麦克风或不可见的位置采集。
- [ ] **大型线上社区/账号体系**：短期内不引入账号、云同步社区和远程角色市场。
- [ ] **未经授权的受版权保护角色资产分发**：项目只提供通用示例，不提交具体受版权保护的角色素材、语音、美术或人格设定。

## 维护提示

- 架构结构见 [IMPLEMENTATION.md](./IMPLEMENTATION.md)。
- 设计背景见 [demiurge-mvp-design.md](./demiurge-mvp-design.md)。
- 提交前至少运行 `npm run build`、`cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` 和 `cargo test --manifest-path src-tauri/Cargo.toml`。
