# Demiurge 审计与百炼冒烟测试报告

> 日期：2026-07-02 ｜ 范围：TODO/Mock/Demo 扫描 + 代码质量/布局审计 + 阿里云百炼端到端跑通测试
> 方法：TODO/审计由多 Agent workflow（6 finder + 逐文件对抗性验证 + 综合报告，51 agents / 2.5M tokens）完成；冒烟测试在本机实跑。

---

## 0. 总览结论

| 维度 | 结论 |
|---|---|
| **demo/mock 占位** | ✅ **不存在运行时伪数据**。唯一 `stub` 是 `LocalEmbeddingProvider`（feature 门控、安全围栏完整）；唯一 `mock` 是前端浏览器预览回退（`__TAURI_INTERNALS__` 缺失时才用，真应用永不触发）。 |
| **非系统化布局** | ⚠️ **存在，但属可维护性债**。后端 `pack/mod.rs`(2834 行)、`lib.rs`(2467 行)、前端 `SettingsDialog.tsx`(4357 行)、`i18n.tsx`(1708 行)呈"杂物抽屉"式组织；无正确性缺陷。 |
| **代码质量** | 29 条验证问题（2 HIGH / 11 MEDIUM / 16 LOW）。最严重：`embedding_api_key` 明文落盘 `settings.json`（`redacted_settings` 漏清）。 |
| **百炼 LLM** | ✅ `deepseek-v4-flash` 同步 + 流式 SSE 均通，`reasoning_content` 正确路由。 |
| **百炼 TTS** | ✅ `qwen3-tts-flash` 返回音频 URL，已生成参考音频。 |
| **GPT-SoVITS** | ⚠️ dan 模型加载成功，但 `/tts` 被 `torchcodec`↔FFmpeg ABI 不匹配阻塞（GPTSoVits conda 环境问题，非 Demiurge 代码）。 |
| **OCR** | ✅ PP-OCRv5 模型 3 文件齐全；引擎代码随应用编译通过。 |
| **Live2D** | ✅ 三月七 Cubism v3 模型文件齐全合法。 |
| **应用启动** | ✅ Tauri 应用以百炼配置编译并启动（`demiurge.exe` 运行中，无 panic）。 |

---

## 1. 冒烟测试报告（阿里云百炼）

### 1.1 LLM 对话（dashscope provider + deepseek-v4-flash）

| 测试 | 结果 |
|---|---|
| 同步 `/chat/completions`（`stream:false`，ping） | ✅ HTTP 200，1.3s，回复 "pong"，含 `reasoning_content` |
| 流式 SSE（`stream:true`，"用一句话介绍你自己"） | ✅ HTTP 200，6 个 content delta + 26 个 reasoning_content delta，`[DONE]` 正常收尾 |
| Demiurge 适配器契约 | ✅ `src-tauri/src/llm/openai.rs:195-199` 把 `delta.reasoning_content` 单独路由到 `StreamDelta::Reasoning`，不污染正文；`openai_stream_routes_reasoning_separately_from_content` 单测覆盖 DeepSeek-V4 推理模型路径 |

配置（已写入 `settings.json`，key 走 keyring）：
```
provider = dashscope
base_url = https://dashscope.aliyuncs.com/compatible-mode/v1
model    = deepseek-v4-flash
```
> 注：百炼确实托管 `deepseek-v4-flash`（DeepSeek-V4 推理模型），返回 `reasoning_content`。Demiurge 的 OpenAI 兼容适配器 + reasoning 路由与该模型契约完全匹配。

### 1.2 TTS 语音合成

| 后端 | 结果 |
|---|---|
| **百炼 qwen3-tts-flash**（`media.rs` 原生 `/api/v1/services/aigc/multimodal-generation/generation`） | ✅ HTTP 200，同步返回 wav URL，下载 161KB 参考音频（"你好，我是三月七，今天也要加油哦。"） |
| **GPT-SoVITS**（`voice.rs` → `http://127.0.0.1:9880/tts`） | ⚠️ 服务器加载 `gpt_dan.ckpt` + `sovits_dan.pth` + BERT/HuBERT 成功；`/tts` 返回 HTTP 400 `TorchCodec is required for load_with_torchcodec` |

**GPT-SoVITS 阻塞根因（环境，非 Demiurge 代码）**：
- GPTSoVits conda env：torch 2.11.0+cu128 / torchaudio 2.11.0 / CUDA 可用 ✓
- 装了 `torchcodec 0.14.0`，但其 `libtorchcodec_core4.dll` 依赖项缺失
- env 有 FFmpeg 8（`avcodec-62.dll` 等）；torchcodec 0.14 期望不同 FFmpeg ABI → DLL 加载失败
- 显式把 `Library/bin` 加 PATH 仍失败 → 确认 ABI 不匹配，非 PATH 问题
- **Demiurge `voice.rs:267-340` 的 GPT-SoVITS HTTP 契约正确**（body shape 与 `api_v2.py /tts` 一致：`text/text_lang/ref_audio_path/prompt_text/prompt_lang/text_split_method=batch_size=1/media_type=wav/streaming_mode=false/parallel_infer=true`）

**修复方向（用户自行处理 GPT-SoVITS env）**：降级 conda ffmpeg 到 7.x（avcodec-61）匹配 torchcodec 0.14；或装匹配 FFmpeg 8 的 torchcodec 版本；或设 `torchaudio` 用 `soundfile` backend 绕过 torchcodec。

### 1.3 OCR（PP-OCRv5）

| 项 | 结果 |
|---|---|
| 模型文件 | ✅ `%APPDATA%/com.demiurge.engine/models/ocr/pp-ocrv5-mobile/`：`pp-ocrv5_mobile_det.onnx`(4.8MB) + `pp-ocrv5_mobile_rec.onnx`(16.5MB) + `ppocrv5_dict.txt`(74KB) |
| 引擎代码 | ✅ `oar-ocr` crate 随 Tauri 应用编译通过；`ocr.rs` 状态机 + 下载 UX 完整 |
| 运行时识别 | 未驱动（需 Tauri webview UI 调 `ocr_image_bytes` 命令；本会话无法自动化原生 webview） |

### 1.4 Live2D（三月七）

| 项 | 结果 |
|---|---|
| 模型文件 | ✅ `D:/User/Documents/Tencent Files/三月七/三月七/`：`三月七.model3.json` + `三月七.moc3`(3.7MB) + `三月七.physics3.json` + `三月七.cdi3.json` + `motions/` + `exp/` + 2 纹理（`三月七.4096/texture_00.png`/`texture_01.png`） |
| model3.json 结构 | ✅ Cubism v3 合法：`FileReferences.Moc/Textures/Physics/DisplayInfo` 齐全；`EyeBlink` 组就绪（`ParamEyeLOpen/ROpen`） |
| LipSync 组 | `Ids` 为空 → TTS 口型联动待补（与文档 `19-live2d-panel.md` 一致） |
| 运行时渲染 | 需 Tauri webview + Cubism Core（`npm run fetch:cubism-core`） |

### 1.5 应用启动

| 项 | 结果 |
|---|---|
| `npm run tauri dev` | ✅ Vite 6.4.3 dev server（port 38741）+ cargo 增量编译 1.15s + `demiurge.exe` 启动（PID 40232） |
| 设置加载 | ✅ 百炼配置 hydrate 成功（无 credential warning），key 已迁移至 keyring |
| 编译警告 | 2 个 dead-code warning：`mcp/mod.rs:179 capabilities` 字段未读、`tools/mod.rs:849 permission_policy_for` 未使用 |
| ⚠️ 构建路径 | `src-tauri/.cargo/config.toml:12` 把 `target-dir` 指向 `D:/Project/Project-1/babel-window-translator/src-tauri/target`（另一项目）。已 git-ignore，但跨项目共享 target 目录属非系统化布局。 |
| UI 驱动 | 未自动化（Tauri 原生 webview 无 CDP 接口；Vite dev server 单独打开缺 `__TAURI_INTERNALS__` 无法 invoke 命令） |

### 1.6 冒烟测试资产清单

| 资产 | 路径 | 状态 |
|---|---|---|
| GPT-SoVITS t2s | `D:/User/Documents/Tencent Files/gpt_dan.ckpt` (155MB) | ✅ |
| GPT-SoVITS vits | `D:/User/Documents/Tencent Files/sovits_dan.pth` (85MB) | ✅ |
| Live2D | `D:/User/Documents/Tencent Files/三月七/三月七/` | ✅ |
| OCR 模型 | `%APPDATA%/com.demiurge.engine/models/ocr/pp-ocrv5-mobile/` | ✅ |
| 参考音频 | `.tmp/ref_bailian.wav` (161KB，百炼 TTS 生成) | ✅ |
| settings 备份 | `%APPDATA%/com.demiurge.engine/settings.json.bak` | ✅ |

---

## 2. TODO / Mock / Demo 占位扫描报告

> 对抗性验证后 findings，按 category 分组。占位定性：`todo-comment`=纯注释；`stub`/`placeholder`/`unimplemented`=功能未实现但代码已占位；`mock-data`/`demo-data`/`hardcoded-fake`=伪数据；`doc-drift`=文档与代码现状不符。

### 统计

| 指标 | 数量 |
|---|---|
| 纯 TODO 注释（todo-comment） | 3 |
| 真占位（stub + hardcoded-fake + 真功能缺失型 todo） | 3 |
| 文档漂移（doc-drift） | 12 |
| **总 findings** | **18** |

**是否存在 demo / mock 占位？—— 否。** 无 `mock-data` / `demo-data` 类伪数据残留。唯一 `hardcoded-fake` 是游离的 GPT-SoVITS 启动脚本（含本机绝对路径，非运行时 mock）。唯一 `stub` 是 `LocalEmbeddingProvider`（feature 门控、安全围栏完整，启用即报错而非返回假数据）。其余 14 条为文档/TODO 描述与代码现状不符的漂移。

补充（手动闭合 TS 扫描缺口）：前端 `src/components/Dashboard.tsx:25-26,106` 的 `mockStats()` 是浏览器预览回退（`__TAURI_INTERNALS__` 缺失时才用，真 Tauri 应用永不触发），与 `App.tsx:55 PREVIEW_SETTINGS` 同模式，verifier 已判 `not-an-issue`。TS 无运行时 mock/demo 数据。

### 2.1 todo-comment（3 条）

- **[MEDIUM]** `docs/TODO.md:140` — `[ ] Voice TTS 闭环:ASR 已接入,TTS 仍是预留接口`。但 `voice.rs:193-249` 的 `voice_synthesize` 已实现 dashscope + gpt-sovits 两路后端，绝非"预留接口"。流式/播放队列/打断/语速确实未实现。**建议**：改为 `[x] TTS 双后端已接通`，剩余拆 `[ ]`。
- **[MEDIUM]** `docs/TODO.md:182` — `[ ] TTS adapter:...便于后续接 GPT-SoVITS`。voice.rs gpt-sovits 分支已支持 base URL/音色/非流式/并行推断。仍缺语速/情感/流式/连接测试/降级/播放队列。**建议**：拆分并标 `[x]`。
- **[LOW]** `docs/TODO.md:67` — 已实现账本把 TTS 仅描述为"预留 API 字段"，与下方 P4 待办对同一能力状态自相矛盾。**建议**：订正为"已接通双后端"。

### 2.2 stub（1 条）

- **[MEDIUM]** `src-tauri/src/embed/mod.rs:143` — `LocalEmbeddingProvider` 在 `#[cfg(feature="embeddings-local")]` 门控下，`dims()` 返回 0、`embed()` 恒返回带中文说明的 `Err`。`Cargo.toml:17` 该 feature 为空。安全围栏完整（不返回假数据，启用即早失败）。**建议**：接 `fastembed` crate 实现本地推理，或明确文档标注。

### 2.3 hardcoded-fake（1 条）

- **[MEDIUM]** `scripts/gpt-sovits-dan-tts.yaml` + `start-gpt-sovits-dan.ps1` — 既未被 git 追踪（`??`）也未被 `.gitignore` 排除，处于游离状态。YAML `custom` 段硬编码本机绝对路径 `D:\User\Documents\Tencent Files\gpt_dan.ckpt`。`git add scripts/` 会泄露本机环境。PS1 已 `param` 参数化（仅默认值是本机路径）。**建议**：加 `.gitignore` 或参数化后入库；补 `scripts/README`。

### 2.4 doc-drift（12 条，最系统化问题）

- **[HIGH]** `README.md:76` — 称"TTS 接口仍为预留占位"，但 `voice.rs:193-249` 已实现双后端 TTS 并注册为 Tauri command。README 是用户首要入口，功能存在性描述错误直接误导。
- **[HIGH]** `docs/modules/15-multimodal-computer-use.md:246` — 称 voice_synthesize "纯占位:恒返回错误...丢弃 text/voice_id（`let _ = (text, voice_id)`）"。实际函数位于 193-249，真正使用参数并分派两路后端。文档自引虚构代码模式。
- **[HIGH]** `docs/modules/15-multimodal-computer-use.md:244` — 同文档多处（§3/§4/§4.3 标题/表格）系统性称 TTS 预留/占位/恒返回错误。
- **[HIGH]** `src-tauri/src/lib.rs:2` — 声明 `mod companion/embed/startup`，但 `README.md:176-188` 与 `IMPLEMENTATION.md:70-85` Project Structure 树未列出（companion.rs 1589 行）。
- **[HIGH]** `docs/IMPLEMENTATION.md:106` — 写"TTS 仍为预留占位"。
- **[MEDIUM]** `docs/modules/17-frontend-architecture.md:220` — 称"后端 STT/TTS 是占位实现（见 api.ts:118 注释 backend not implemented）"。api.ts:118 实为 `agentSaveFile` 与语音无关；后端三命令均已实现。
- **[MEDIUM]** `docs/modules/14-pack-system.md:240` — 断言"语音（TTS/ASR）后端未接通"。
- **[MEDIUM]** `docs/modules/13-persistence-config.md:320` — 已知限制写"无 RAG / 无向量检索"，但 `embed/mod.rs` 已实现 `RemoteEmbeddingProvider` + RRF 混合召回（`docs/modules/20`）。
- **[MEDIUM]** `docs/modules/19-live2d-panel.md:121` — 称"TTS adapter 尚未接通"。核心结论（lip-sync 未接入 Live2D）正确，仅表述漂移。
- **[MEDIUM]** `docs/demiurge-mvp-design.md:97` — 存档文档把 TTS 标"🔜 接口已预留、未接后端"。
- **[MEDIUM]** `src-tauri/src/pack/mod.rs:1` — 模块注释自称"MVP 文本版清单"，与现状（Live2D 归一化 + lorebook BM25/dense/RRF + credits/skills）严重漂移。
- **[LOW]** `scripts/start-gpt-sovits-dan.ps1:1` — 文件名嵌入个人代号 'dan'，无 `scripts/README` 说明。

### 2.5 其他

- **[MEDIUM]** `src-tauri/src/store/mod.rs:321` — `pub embedding_api_key: String` 带 TODO 注释"后续接入凭据管理器"。`credentials.rs` 已实现完整 keyring 管理器（6 类密钥），唯独无 embedding 变体。注释与凭据管理器已存在的事实矛盾（详见审计 security 段）。

---

## 3. 代码质量 + 布局审计报告

### 统计

| 维度 | 条数 |
|---|---|
| security | 2 |
| layout | 6 |
| quality | 21 |
| **合计** | **29** |
| HIGH | 2 |
| MEDIUM | 11 |
| LOW | 16 |

**结论：是否存在非系统化布局？—— 是。** 后端 `pack/mod.rs`(2834 行 god module) 与 `lib.rs`(2467 行/126 函数) 单文件堆叠 5+ 独立职责；前端 `SettingsDialog.tsx`(4357 行)、`i18n.tsx`(1708 行内联双语词典)、`companion.rs`(1589 行捆绑 4 子系统) 同样"杂物抽屉"式。布局是最系统性的技术债，但均为可维护性而非正确性缺陷。

### 3.1 Security

- **[HIGH]** `src-tauri/src/store/mod.rs:562` — `redacted_settings`(555-571) 清空了 api_key/tavily/brave/exa/webdav_password/media_api_key 及 MCP secret env，**唯独漏清 `embedding_api_key`**。`save_settings`(573-578) 用 redacted 结果 `fs::write`，导致 embedding key 明文写入 `settings.json`。单测 `save_settings_does_not_persist_api_key`(671) 只校验前 5 个 key，未覆盖 embedding。`embed/mod.rs:123` 直接读 `settings.embedding_api_key` 作 bearer。**建议**：`redacted_settings` 对 `embedding_api_key` 执行清空；纳入 `SecretKind` 枚举走 keyring；补单测断言。

### 3.2 Layout

- **[HIGH]** `src-tauri/src/pack/mod.rs:1` — 2834 行 god module，混合 5 类职责（类型定义 / lorebook BM25+dense+RRF / Live2D 导入 / pack 文件浏览 / manifest 校验+分块）。`pack/` 目录下仅此一文件。**建议**：拆 `pack/manifest.rs`/`lorebook.rs`/`live2d.rs`/`files.rs`。
- **[MEDIUM]** `src-tauri/src/lib.rs:407` — `send`(~170 行) 单函数分支处理 `/dream`/`/compact`/`/goal`/`/skills`/`/effort`/`/recall`/`/workflows`/`/ultracode` + 高风险检测 + turn 编排。`lib.rs` 全文 2467 行/126 函数偏胖。**建议**：slash 分流下沉到 `agent::slash::dispatch`。
- **[LOW]** `src-tauri/src/companion.rs:1` — 1589 行捆绑 4 子系统（高风险检测/记忆队列/记忆抽取/天气）。**建议**：拆 `companion/{safety,memory_queue,extraction,weather}.rs`。
- **[LOW]** `src/components/SettingsDialog.tsx:1` — 4357 行单体设置组件。**建议**：按 tab 拆 `settings/` 子目录。
- **[LOW]** `src/lib/i18n.tsx:1` — 1708 行内联双语词典 + Provider 逻辑混杂。**建议**：拆 `i18n/locales/{zh,en}.ts` + `I18nProvider.tsx`，类型约束保证 key 对齐。
- **[LOW]** `src/lib/` — 8 文件扁平堆积，`IMPLEMENTATION.md`/`README` 结构树滞后。**建议**：优先更新结构树。

### 3.3 Quality（21 条，节选）

- **[MEDIUM]** `src-tauri/src/agent/runner.rs:146` — `run_turn_with_options` 487 行超长编排函数（setup/MCP/prompt/LLM 循环/工具/中断/内存/落盘）。**建议**：拆 `prepare_turn`/`llm_step`/`tool_step`/`finalize_turn`。
- **[MEDIUM]** `src-tauri/src/lib.rs:121` — `AppState` 全用 `std::sync::Mutex`，lib.rs 93 处 `lock().unwrap()`。持锁线程 panic 会 poison，后续 unwrap 二次 panic，命令直接失败而非优雅降级。**建议**：改 `parking_lot::Mutex`（无 poison）或 `map_err` 转 `anyhow`。
- **[MEDIUM]** `src-tauri/src/lib.rs:2212` — `parse_webdav_backup_files` 每次调用 `Regex::new` 编译 4 条正则并 `expect`。**建议**：`LazyLock`/`OnceLock` 缓存（lib.rs 已用该模式）。
- **[MEDIUM]** `src-tauri/src/tools/mod.rs:173` — `registry()` 554 行单 vec! 字面量。新增工具需同时改 `registry`/`execute`/`permission_summary`/`affected_paths`/`confirmation_preview` 多处。**建议**：宏或 builder + 按工具单文件聚合。
- **[MEDIUM]** `src-tauri/src/companion.rs:519` — `read_memory_queue` 用 `.ok().and_then(.ok()).unwrap_or_default()` 把"文件缺失"与"队列损坏"一并吞为空 Vec。调用方会用单项列表覆盖损坏文件，静默销毁原队列。**建议**：返回 `Result`，区分 NotFound 与其它错误。
- **[MEDIUM]** `src/components/Select.tsx:59` — 自定义下拉缺 ARIA 语义（无 `aria-haspopup`/`role=listbox`/`aria-selected`，无方向键导航），违反 WCAG 4.1.2。**建议**：补全 ARIA + 键盘导航。
- **[MEDIUM]** `src/components/SettingsDialog.tsx:582` — 主组件 ~3775 行，60+ useState，7 useEffect，跨十余功能域。**建议**：按功能域拆 `<OcrSettingsSection/>` 等。
- **[MEDIUM]** `src-tauri/src/mcp/mod.rs:771` — `pending.lock().unwrap()` 裸 unwrap（注：原述"持锁跨 await"有误，每次取锁单语句内释放，无死锁；真问题仅生产路径裸 unwrap）。
- **[LOW]** `src/App.tsx:569` — 4 个重复的 click-outside useEffect。**建议**：抽 `useClickOutside` hook。
- **[LOW]** `src/App.tsx:521` — 5 个 `listenXxx().then()` 无 `.catch`。**建议**：补 catch。
- **[LOW]** `src/components/{WorkflowsPanel,SettingsDialog,SkillsPanel}.tsx` — 多处 listen 链无 catch / `openSkillsDir().catch(()=>{})` 完全吞错。
- **[LOW]** `src/components/{MessageList,MarkdownRenderer,MermaidBlock}.tsx` — 剪贴板复制逻辑三处逐字重复（含 1600ms 魔法数）。**建议**：抽 `useCopyToClipboard` hook。
- **[LOW]** `src/components/Live2DPanel.tsx:18` — `appRef/modelRef` 用 `any` + eslint-disable（pixi 动态 import 设计，无功能风险）。**建议**：定义最小接口获类型安全。
- **[LOW]** `src/components/MediaStudio.tsx:31` — 初始 prompt 硬编码示例文案 `'A clean native desktop app screenshot...'`，与组件空状态设计矛盾，看似 demo 残留。**建议**：置空或移至 i18n。
- **[LOW]** `src/components/PomodoroCard.tsx:75` — `setInterval(refresh,1000)` 空依赖 effect，无论 running/paused/idle 都每秒 IPC，且已注册事件监听冗余。**建议**：非 active 时 `clearInterval` 或拉长到 5-10s。
- **[LOW]** `src-tauri/src/lib.rs:117` — `persist_sessions` 每次落盘 `std::thread::spawn` 新 OS 线程。**建议**：改 `tokio::task::spawn_blocking` 复用线程池。
- **[LOW]** `src-tauri/src/lib.rs:1492` — `context_memory_source` `unwrap_or_default` 吞 IO 错误，面板显示"0 entries"用户无法察觉真实原因。

---

## 4. 修复优先级建议

1. **立即（安全）**：`store/mod.rs:562` embedding key 明文落盘 → `redacted_settings` 补清 + keyring 迁移 + 单测。
2. **近期（文档系统性漂移）**：7+ 处文档称 TTS"预留占位"但已实现双后端 → 全量订正 README/IMPLEMENTATION/modules/14/15/17/19/mvp-design + TODO.md 标 `[x]`。
3. **近期（布局债）**：`pack/mod.rs` 2834 行拆分；`lib.rs::send` slash 分流下沉；Mutex unwrap 系统性改造。
4. **按需清理**：前端重复 hook 抽取（useClickOutside/useCopyToClipboard）、a11y 补全、PomodoroCard 轮询节流、各 listen 链补 catch、`.cargo/config.toml` 跨项目 target-dir。
5. **GPT-SoVITS env**（非 Demiurge 代码）：降级 conda ffmpeg 到 7.x 或装匹配 FFmpeg 8 的 torchcodec，恢复 `/tts`。
