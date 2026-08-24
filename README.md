<div align="center">

<img src="assets/branding/logo.png" width="132" alt="Demiurge" />

# Demiurge

**轻量、开源、可扩展的桌面 Agent 引擎**

加载你自己的角色包，把本地桌面、项目上下文、工具系统和大模型端点接成一个可控的 Agent。<br/>
它既能像角色一样陪你聊天，也能在权限确认后读项目、搜索、编辑、执行命令、整理记忆和持续推进目标。

[![License](https://img.shields.io/badge/License-MIT-111827?style=for-the-badge)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-Core-000000?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/Tauri-2.x-24C8DB?style=for-the-badge&logo=tauri&logoColor=white)](https://tauri.app/)
[![React](https://img.shields.io/badge/React-18-20232A?style=for-the-badge&logo=react&logoColor=61DAFB)](https://react.dev/)
[![TypeScript](https://img.shields.io/badge/TypeScript-5-3178C6?style=for-the-badge&logo=typescript&logoColor=white)](https://www.typescriptlang.org/)
[![Vite](https://img.shields.io/badge/Vite-6-646CFF?style=for-the-badge&logo=vite&logoColor=white)](https://vite.dev/)
[![Tailwind CSS](https://img.shields.io/badge/Tailwind-4-38BDF8?style=for-the-badge&logo=tailwindcss&logoColor=white)](https://tailwindcss.com/)

</div>

---

> 文档状态：2026-07-26 已按当前实现复核。验证基线为前端 44 项测试、生产构建和 Rust 297 项测试通过；真正音频字节流、Computer Use 执行闭环与 Live2D 桌宠交互扩展仍在路线图中。

## 这是什么

Demiurge 是一个桌面伴侣 Agent 的“空引擎”。它不绑定具体角色，也不托管你的数据；你提供角色包和 LLM 端点，它负责把对话、工具、记忆、安全边界和本地桌面能力串起来。

- **本地优先**：Tauri + Rust 后端，设置、会话、角色包、记忆都保存在本机；每个会话可绑定一个独立项目文件夹。
- **原生桌面体验**：默认“水晶花”（`Crystal Bloom`）与经典中性主题共享 Material 布局和字体层级；吉签、陪伴、番茄钟使用独立常驻小工具窗口，Windows 主窗口控制由 Rust/Win32 可靠执行。
- **角色与引擎分离**：角色包用 manifest 2.0 描述 persona、结构化 Character Card（身份/背景/人格/说话风格/示例对话/OOC 规则）、Runtime 策略（技能绑定、memory namespace、voice、permission 偏好）与 Lorebook 知识库；引擎保持通用。
- **会动手**：可读写当前会话项目中的文件、编辑代码、跑 shell、联网搜索、截图/OCR、派生子 Agent、运行 workflow。
- **Minecraft 角色接入**：设置页可启动独立 Mineflayer MCP 项目；游戏角色沿用当前人物包、Lorebook、会话和 memory namespace，动作由 Skill 执行，Demiurge 当前 LLM 是唯一规划模型。
- **可控安全**：写文件、shell、打开路径、截图/OCR 等敏感操作走确认门；文件工具被限制在当前会话项目根；角色卡可声明 permission 偏好，在用户规则与工具默认之间形成可配置 overlay。
- **可持续推进**：`/goal` 可以设置长期目标，普通回合结束后继续自动驱动，直到完成、暂停、阻塞或预算耗尽。
- **Lorebook 向量召回**：本地 BM25 稀疏检索 + 远程 embedding 稠密检索 + RRF 混合融合，chunk 向量按 provider+维度缓存；`/recall` 与设置面板可视化命中关键词、score、索引状态。
- **Live2D 面板与独立窗口**：角色包可挂载 Cubism 4/5 模型（`untitled-pixi-live2d-engine` + PixiJS v8），支持 idle 物理/眨眼/呼吸、缩放、拖拽和可持久化的鼠标跟随开关。主面板首次访问后保持 WebGL 实例，独立透明置顶窗口隐藏后暂停 ticker、再次呼出复用模型。文件夹导入会在临时目录规范化并验证全部引用；运行时通过 Tauri asset protocol 直接加载模型资源。需先运行 `npm run fetch:cubism-core` 取回 Live2D Cubism Core（私有运行时，不入库）。

## 功能概览

### Agent Core

- 流式对话、随时中断、多轮 tool call loop。
- OpenAI-compatible、local、Anthropic、Gemini provider adapters。
- 内置 DeepSeek、DashScope、OpenAI、OpenRouter、GLM、MiniMax、xAI、Groq、Mistral、Moonshot、Perplexity、豆包、混元、阶跃星辰等供应商预设（多数走 OpenAI 兼容适配器，Anthropic / Gemini 走各自适配器）。
- 统一工具 schema，按 provider 方言输出。
- 多会话持久化、角色包切换、设置持久化。
- LLM API Key 使用系统凭据管理器保存。

### 项目工作区与对话

- 从输入区或项目面板选择本地文件夹；选择结果随当前会话持久化，切换会话时恢复对应项目。
- 项目树按目录懒加载，默认过滤依赖、构建产物与版本控制内部目录；文本预览限制为 256 KiB，并识别二进制文件。
- Git 面板展示当前分支、本地/远程分支与未提交文件；分支请求与缓存绑定项目路径，切换时由后端再次验证调用方看到的项目，脏工作区仍会二次确认。
- 文件编辑工具在消息流中显示“正在编辑/已编辑”的文件活动卡片，可展开查看受影响路径、参数、结果与差异预览。
- 三类 Provider 共用有界 SSE 语义并校验协议终止标记；前端按 session + turn 去重事件，最终正文可修复漏 delta。Markdown/ToolCard 使用稳定渲染，Mermaid 等到流完成后再执行。
- 输入区图片会作为原生多模态内容发送：OpenAI-compatible 使用 `image_url` content part，Anthropic 使用 base64 image source，Gemini 使用 `inlineData`；OCR 仅作为可选补充，不再代替视觉输入。

### Minecraft MCP

- “设置 → Minecraft”管理独立项目路径、Node 运行时、服务器、AI 游戏用户名、主用户、安全和视觉参数。
- 启用后应用自动启动 stdio MCP 子进程，设置变化自动重启，退出时终止；Mineflayer 不持有第二套 LLM 凭据。
- 公开聊天和私聊都会保留精确玩家用户名、主用户身份与 Minecraft 场景。玩家直接叫 AI 游戏用户名即可互动；未点名消息由当前模型结合语境判断。自动长期记忆由代码强制写入场景、频道、玩家用户名和主用户标识，不依赖提取模型自行补全，因此不会混淆不同玩家。
- 死亡、维度切换、重要 Skill 成败和视觉异常写入当前人物包的 namespaced 长期记忆。

### Context Engineering

- system prompt 分区：引擎规则、角色设定、项目指令、运行环境、当前目标、会话摘要、长期记忆。
- token-aware history budget。
- rolling summary。
- `/compact`、`context_inspect`、`context_collapse`。
- `/dream` 记忆整理。
- 自动长期记忆提取，写入沙盒 `.demiurge/memory.md`。

### Tools

- 文件与编辑：`read_file`、`write_file`、`edit_file`、`multi_edit`、`apply_patch`、`undo_edit`。
- 搜索导航：`glob`、`grep`、`git_status`。
- 执行：`shell`，带确认、沙盒 cwd、超时和输出截断。
- Web Search：Bing、DuckDuckGo fallback、Tavily、Brave、Exa adapter。
- Computer Use 首层能力：窗口列表、屏幕截图、区域/窗口 OCR、OCR 模型下载入口。
- Deferred tools：`tool_search` / `execute_tool` 按需发现低频工具，减少固定上下文成本。

### Agent Orchestration

- `/ultracode` 多 Agent 编排提示。
- `agent_spawn` 只读子 Agent。
- fork context，修复未配对 tool call。
- workflow JSON DSL：`agent`、`parallel`、`pipeline`、`phase`、`budget`、`log` step。
- workflow journal/resume 与 durable snapshot。
- Workflows 面板支持参数表单、内置模板、dry-run、运行日志和失败节点重试，可从 View 菜单打开。
- `worktree_create` 隔离工作区。

### 角色卡、Lorebook 与向量召回

- **结构化角色卡编辑器**：设置 > 人物包内表单化编辑 Character Card / Runtime / Lorebook / 示例对话 / OOC 规则，保留可折叠原始 JSON 回退；复用 manifest 读写命令往返。
- **角色包素材管理**：`open_pack_dir` 打开包目录、`import_pack_lore_files` 批量导入 lore、`list_pack_files` / `read_pack_file` 包内文件浏览；manifest 增 `credits` / `license` 字段，zip 导入产出授权缺失警告。
- **Lorebook RAG**：`lore/*.md`/`.txt` 按 Markdown 标题与段落分块，frontmatter `title/tags/keywords/priority` 进元数据；BM25（k1=1.2, b=0.75）+ 短语/元数据加权召回，IDF 预算并缓存；按文件签名失效与手动重建。
- **向量召回（远程优先）**：`EmbeddingProvider` trait + 远程 OpenAI 兼容 `/v1/embeddings`（DashScope `text-embedding-v3` / OpenAI `text-embedding-3-small` 等）；BM25 与稠密余弦按 RRF（k=60）融合，`hybrid_weight` 调权；chunk 向量按 provider+维度缓存，切换 model 自动重算；本地 fastembed 通过 cargo feature 预留接口、默认不打包 ONNX runtime。
- **召回可视化**：`lorebook_index_status` / `lorebook_recall_detail` / `lorebook_rebuild_index` 命令 + `/recall <query>` slash；面板展示索引状态、chunk 列表、命中关键词高亮、score 与 dense_score、embedding 降级徽标。
- **Memory namespace**：角色卡 `runtime.memory.namespace` 把 user/project 记忆隔离到带后缀文件（`user.{ns}.md` / `memory.{ns}.md`），default 走 legacy；`memory_migrate_namespace` 迁移旧记忆；`/dream` 与自动抽取写 namespaced 路径。
- **Permission overlay**：角色卡 `runtime.permissions` 只能保持或收紧工具默认权限，导入/保存会拒绝 `allow` 和未知策略，运行时再做单调收紧校验；`ask_once` 自动 session-remember，`ask_every_time` 禁止持久化。

### Voice 与素材接口

- Voice：语音输入（STT/ASR）已接入云端转写后端；语音输出支持云端与本地服务、语速/情感参数、连接测试、失败降级，以及按句切分的播放队列、静音和立即打断。STT/TTS 载荷有大小上限；当前是流式文本驱动的分句合成，不是音频字节边接收边播放。默认安装包不分发本地语音模型权重。
- 角色包素材字段：avatar、Live2D（已实现；受检路径经 Tauri asset protocol 直接加载，model3 内引用改写为完整 asset URL）、voice（预留）等。

## 快速开始

前置依赖：

- Node.js 22.6+（测试脚本使用 Node 原生 TypeScript 类型剥离）
- Rust stable
- Windows WebView2，macOS/Linux 使用系统 WebView

```bash
git clone <your-repo-url> demiurge
cd demiurge
npm install
npm run tauri dev
```

打包：

```bash
npm run tauri build
```

首次启动后，在设置里选择 provider，填写 `base_url`、`model` 和 API Key：

- DeepSeek 等在线兼容端点：选择 OpenAI-compatible。
- LM Studio、Ollama OpenAI-compatible、vLLM：选择 local，API Key 可为空。
- Anthropic / Gemini：选择对应 provider，并使用各自默认或自定义 endpoint。

## 常用命令

```text
/compact [keep=N]              折叠较早上下文
/dream                         整理长期记忆
/goal <objective> [+500k]      设置持续目标和可选 token budget
/goal status                   查看目标状态
/goal pause|resume|continue    控制目标续跑
/effort [low|medium|high|xhigh|max|auto]
                                Switch reasoning effort for supported provider/model pairs
/ultracode <task>              开启多 Agent 编排提示
/workflows                     查看 workflow runs
/workflow resume <run_id>      从 journal 恢复 workflow 上下文
```

## 角色包

角色包放在应用数据目录的 `packs/<id>/` 下。最小结构：

```text
packs/<id>/
├─ manifest.json
└─ persona.md
```

`manifest.json` 示例：

```json
{
  "id": "default",
  "name": "Default",
  "persona": "persona.md"
}
```

可选文件：

```text
memory.md        # 角色长期记忆，只读注入 prompt
assets/          # 头像、语音、Live2D 等本地素材
```

## 系统架构

```text
React Feature
  │
  ▼
类型化 IPC 门面（frontend/src/lib）
  │ invoke / listen
  ▼
Tauri Controller
  │ 参数校验与 DTO 转换
  ▼
Biz 用例编排
  ├─ Core：Session、Goal 等领域规则
  ├─ Framework：持久化、WebDAV 等基础设施适配器
  └─ Desktop Service：Agent、Tool、LLM、窗口与系统能力
        │
        ▼
本地文件 / 系统凭据 / LLM 端点 / 操作系统集成

Starter：构建 AppState、装配 Adapter、恢复运行时并注册 Tauri Command
```

## 项目结构

```text
Demiurge/
├─ frontend/                     # React/TypeScript/Vite front-end
│  ├─ src/app/                   # Application orchestration and navigation shell
│  ├─ src/features/              # Agent/chat/workspace/settings/etc. feature owners
│  ├─ src/shared/                # Stateless shared view components
│  ├─ src/lib/                   # Typed IPC facade and cross-feature contracts
│  ├─ public/                    # Front-end runtime assets
│  └─ tests/                     # Front-end logic and source-contract tests
├─ backend/                      # Rust/Tauri back-end area
│  ├─ Demiurge-common/           # Compiled shared contract crate
│  ├─ Demiurge-core/             # Compiled domain rule crate
│  ├─ Demiurge-framework/        # Compiled persistence/remote adapter crate
│  └─ Demiurge-desktop/          # Tauri controller/biz/starter and desktop services
│     ├─ src/controller/         # Lightweight IPC adapters
│     ├─ src/biz/                # Use-case orchestration
│     ├─ src/agent/              # Agent loop, context, memory, goal, workflow
│     ├─ src/llm/                # Provider adapters
│     ├─ src/tools/              # Built-in tools and registry
│     ├─ src/permission/         # Confirmation and permission gate
│     ├─ src/store/              # Settings/session persistence
│     └─ src/pack/               # Character pack loading
├─ assets/                       # Project-level source artwork and branding
├─ resources/packs/              # Versioned default pack and format example
├─ docs/                         # Design, implementation notes, roadmap
├─ scripts/                      # Build/runtime helper scripts
├─ tests/                        # Future cross-layer E2E tests
└─ package.json                  # Workspace-level commands
```

## Security Model

> 注意：2026-07-12 代码审查列出的 12 项 P1 已全部修复，包括角色包与路径边界、权限作用域、跨项目撤销、系统打开、direct 网页读取、deferred 目标授权、导航事务、分支/回合归属以及动态外部工具授权下限。P2/P3 与显式 Bypass 模式等剩余边界仍应按 [代码审查报告](docs/CODE-REVIEW-2026-07-12.md) 理解。

- 文件与 shell 工具只能访问当前会话绑定的项目根；未选择项目时回退到应用数据目录下的 `sandbox/`。
- 路径先做词法校验，再做 canonicalize 校验，防止 `..`、符号链接和 junction 逃逸。
- 回复生成期间禁止切换到另一个项目或切换 Git 分支，避免运行中的工具根目录发生漂移。
- 回合从 `begin_turn` 起固定拥有一个 session id；初始化、prompt、消息、工具内会话状态和目标续跑都复用该身份，运行/取消中的所属会话不可删除。
- 后端把 `session_id + sessions + history + workspace + goal` 作为一个导航快照返回；前端只提交最新 navigation epoch/request，并在导航期间锁定 Sidebar、Composer、Goal、语音与工作区操作。主时间线消费带 `turn.session_id` 的统一事件，确认与 Goal 事件也携带 session id。
- 权限上下文在回合开始、任何异步初始化之前捕获；裁决、审计和确认后的规则记忆始终复用该 session/workspace identity，不会因界面切换改写归属。
- Session 权限按 session id 分桶并随会话删除；Project 权限按 canonical workspace identity 存入版本化 `project_permissions.json`，不会跨项目复用。
- Project/User 权限文件在进程内串行并通过同目录临时文件原子替换；工作区身份或规则存储无法验证时拒绝自动授权。旧版无项目身份的 `permissions.json` 不会自动应用，升级后需在当前项目重新确认 Project 规则。
- direct `http_get` / `web_fetch` 只连接公开 HTTP(S) 地址：首跳和每次重定向都会重做 DNS 全答案校验，固定已验证 IP，并拒绝凭据 URL、loopback、私网、链路本地、组播、未指定和保留地址。响应按上限流式读取，不会先完整下载再截断；外部抓取 adapter 不经过本机 direct fetch。
- Session、Memory 与 Dream 的重要本地写入使用同目录临时文件和原子替换，主文件损坏时可从备份恢复并保留损坏副本。
- Live2D 导入只复制普通文件，拒绝源链接/特殊文件；model3 的 Moc、纹理、物理、Pose、DisplayInfo、UserData、表情、动作和声音引用统一拒绝绝对路径、盘符、`.`/`..` 与链接逃逸。候选目录完整验证后以备份/rename 提交，任何提交错误都会尝试恢复旧目录和 manifest。
- 角色包 id 只允许最长 128 字节的 ASCII 字母、数字、`-` 与 `_`；所有读取、修改、打开、Live2D、lore、memory 和 skills 入口都先 canonicalize `packs` 根与目标，要求目标是非链接的直接子目录。设置保存也拒绝无效或不存在的当前包。
- `execute_tool` 只负责 deferred 分发；权限规则、角色包收紧策略、风险、确认内容、affected paths 与审计都绑定内层真实 target。允许 `open_path` 不会自动允许截图/OCR，旧的宽泛 wrapper Allow 也不再命中有效 target。
- 动态外部工具的 `readOnlyHint`、`destructiveHint`、`openWorldHint` 只作为服务端提示保留；本地有效风险始终至少为 `External`/`Privileged`，默认权限保持 `Ask/Once`。Auto 会先执行本地规则链，不能用只读分类覆盖显式或默认 `Ask`/`Deny`。
- 写文件、shell、open_path、截图/OCR 等操作会先请求确认。
- shell 限制 cwd、timeout 和 output cap。
- 子 Agent 默认只读，不允许写文件、跑 shell 或递归派生。
- LLM API Key 存在系统凭据管理器中，不写入 `settings.json`。
- Web Search 外部 adapter key（Tavily/Brave/Exa）可在设置中填写并存入系统凭据管理器，运行时优先读取设置值，未配置时回退到对应环境变量。

## Development

开发运行：

```bash
npm run tauri dev
```

前端测试与构建：

```bash
npm test
npm run build
```

Rust 测试：

```bash
cargo test --manifest-path backend/Cargo.toml --workspace --all-targets
```

Tauri 打包：

```bash
npm run tauri build
```

## Documentation

- [模块化架构与迁移约束](docs/MODULAR-ARCHITECTURE.md)
- [模块技术原理文档（存档）](docs/modules/README.md) — 逐子系统的深度技术文档，从[架构总览](docs/modules/01-architecture-overview.md)开始
- [实现说明](docs/IMPLEMENTATION.md)
- [代码审查报告（2026-07-12）](docs/CODE-REVIEW-2026-07-12.md)
- [流式输出协议评估](docs/streaming-protocol-assessment.md)
- [TODO / 路线图](docs/TODO.md)
- [Goal 持续驱动](docs/goal-continuous-driving.md)
- [Ultracode 多 Agent 编排](docs/ultracode-agent-orchestration.md)
- [Workflow JSON DSL](docs/workflow-json-dsl.md)
- [MVP 设计背景](docs/demiurge-mvp-design.md)

## Credits

- Provider logos are from [@lobehub/icons](https://github.com/lobehub/lobe-icons) (MIT). Brand logos remain the trademarks of their respective owners and are used only to identify the provider.

## License

Demiurge is released under the [MIT License](LICENSE). Character assets, voice assets, artwork, and persona packs based on specific works are user-managed local content and are not distributed with this repository.
