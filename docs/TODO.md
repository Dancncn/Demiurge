# TODO / 路线图

Demiurge 当前的 Agent、工具、上下文、记忆、工作流和桌面底座已经基本成型。接下来的路线重点转向“有温度的本地陪伴”：让应用能理解用户当下状态，在合适的时机提供天气、节奏、提醒、专注和轻量情绪支持。

## 已实现能力摘要

- [x] **桌面应用底座**：Tauri + Rust 后端、React 前端、会话 UI、设置页、工具卡片、权限确认和构建脚本。
- [x] **Agent Loop**：流式输出、工具调用、多轮循环、取消、中断、会话状态同步和持久化。
- [x] **上下文工程**：system prompt 分层、项目指令、角色设定、会话摘要、分层 memory、token-aware 裁剪和 Context 可视化。
- [x] **长期记忆与 Dream**：用户/项目/会话/角色包记忆分层，支持审计、编辑、去重，以及 `/dream` 后台整理入口。
- [x] **Goal 持续驱动**：`/goal`、预算、pause/resume/continue、自动续写和状态栏控制。
- [x] **多 Agent 与 Workflow**：自定义 Agent、`agent_spawn`、证据包、reviewer、JSON DSL、durable run、journal 和 resume。
- [x] **工具系统**：文件、编辑、搜索、Shell、Web Search/Web Fetch、clipboard、package scripts、worktree、context tools 和 deferred tools。
- [x] **权限与安全**：文件沙箱、工具风险分级、一次/会话/项目级授权、Shell 策略、进程树终止和凭据管理器。
- [x] **Provider 与模型能力画像**：OpenAI-compatible、local、Anthropic、Gemini 等 adapter，provider capability profile、reasoning effort、token budget 和流式 usage 规范化。
- [x] **Web Search / Fetch**：Bing、DuckDuckGo fallback、Tavily、Brave、Exa，结果过滤、缓存、来源提示和 fetch adapter 复用。
- [x] **Computer Use 底座**：窗口列表、屏幕截图、点击/输入、OCR 模型下载和 Settings OCR 面板。
- [x] **Voice API 预留与 ASR**：录音入口、设备选择、`voice_transcribe`，支持 DashScope ASR 与 OpenAI-compatible Whisper；TTS adapter 仍保留为后续扩展。
- [x] **角色包与设置**：角色包头像导入、Settings Provider/Web Search/OCR/Memory/Context/WebDAV、连接测试和 API Key 安全存储。

## P0 / 提交前收口

- [ ] **Rust 测试恢复绿色**：同步 provider token profile 相关测试预期，确保 `cargo test --manifest-path src-tauri/Cargo.toml` 通过。
- [ ] **格式化收口**：确保 `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` 通过。
- [ ] **文档去来源化**：README、技术文档和 TODO 只描述 Demiurge 自身能力、兼容项和路线，不写来源对齐叙事。
- [ ] **PR 交接说明**：在 PR 描述里列清楚已完成、未完成、已知风险、测试结果和下一位同伴可以接手的入口。

## P1 / 情感陪伴核心

- [ ] **陪伴状态模型**：为用户状态建立轻量结构：心情、精力、专注状态、最近互动、偏好语气、禁打扰时段。
- [ ] **陪伴记忆抽取**：把“喜欢怎样被提醒”“压力来源”“作息偏好”“常用称呼”等写入用户级记忆，并允许用户审计/删除。
- [ ] **主动关怀策略**：基于时间、最近会话、天气、番茄钟状态生成低频主动提醒，默认克制，避免打扰。
- [ ] **情绪支持回复风格**：增加可配置的陪伴语气档位，例如安静、元气、吐槽、温柔、效率教练。
- [ ] **安全边界**：情绪陪伴只做支持性对话和生活辅助，不替代医疗、心理治疗或紧急干预；高风险表达给出明确求助建议。

## P2 / 天气与本地生活陪伴

- [ ] **天气 Provider API**：新增后端天气查询接口，支持城市配置、自动定位可选项、缓存、失败降级和权限说明。
- [ ] **天气陪伴卡片**：在侧栏或状态区展示今日天气、体感、降雨、空气质量、穿衣/带伞建议。
- [ ] **天气驱动关怀**：根据雨雪、高温、寒潮、空气质量、昼夜变化生成轻量提醒，例如出门、补水、通勤和休息建议。
- [ ] **隐私设置**：用户可选择手动城市、粗略定位或关闭天气陪伴；位置和天气缓存可清除。

## P3 / 番茄钟与节奏陪伴

- [ ] **番茄钟基础计时**：专注、短休息、长休息、自定义时长、暂停/继续/跳过和桌面通知。
- [ ] **陪伴式专注反馈**：开始前帮用户拆目标，结束后简短复盘，连续专注时给出轻量鼓励。
- [ ] **任务绑定**：番茄钟可绑定当前会话、Goal、Workflow 或手动任务标题。
- [ ] **节奏记忆**：记录用户偏好的专注时长、常见中断原因和高效时间段，作为后续提醒依据。
- [ ] **勿扰联动**：专注中减少主动提醒，只保留用户允许的高优先级提示。

## P4 / 语音与桌面陪伴

- [ ] **TTS adapter**：保留统一 API，优先支持外部 HTTP 服务，便于后续接 GPT-SoVITS、CosyVoice 或其他本地/云端方案。
- [ ] **流式语音合成**：复用模型流式文本，按句切分播放，支持打断、静音、音色选择和语速配置。
- [ ] **语音唤醒/快捷键**：支持全局快捷键或按钮触发语音输入；唤醒词作为可选实验能力。
- [ ] **桌面陪伴壳**：透明置顶窗口、轻量状态展示、点击穿透、可收起/展开，避免遮挡工作流。
- [ ] **屏幕感知边界**：截图/OCR/窗口信息必须经过明确权限开关和可见状态提示。

## P5 / 体验与打包

- [ ] **前端 chunk 瘦身**：继续拆分 KaTeX、Mermaid、PDF 等大模块，按需动态 import。
- [ ] **首次启动引导**：引导用户配置 provider、API Key、天气城市、记忆策略、语音和通知权限。
- [ ] **本地数据导出**：导出设置、记忆、番茄钟记录、Goal/Workflow 历史，便于迁移和协作排查。
- [ ] **异常可恢复**：后台任务、番茄钟、会话保存和 Workflow 在应用重启后尽量恢复到可解释状态。

## 暂不做

- [ ] **全自动远程执行环境**：当前聚焦本地桌面、本地权限和可解释执行。
- [ ] **完整浏览器自动化**：Computer Use 先保持截图、OCR、窗口列表和基础输入能力。
- [ ] **大型线上社区/账号体系**：短期内不引入账号、云同步社区和远程角色市场。
- [ ] **不可审计的主动监听**：不做默认常驻麦克风、默认屏幕读取或不可见的位置采集。

## 维护提示

- 架构结构见 [IMPLEMENTATION.md](./IMPLEMENTATION.md)。
- 设计背景见 [demiurge-mvp-design.md](./demiurge-mvp-design.md)。
- 提交前至少运行 `npm run build`、`cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` 和 `cargo test --manifest-path src-tauri/Cargo.toml`。
- 不要提交具体受版权保护的角色素材、语音/美术资产或人格设定。
