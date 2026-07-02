<div align="center">

<img src="docs/assets/logo.png" width="132" alt="Demiurge" />

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

## 这是什么

Demiurge 是一个桌面伴侣 Agent 的“空引擎”。它不绑定具体角色，也不托管你的数据；你提供角色包和 LLM 端点，它负责把对话、工具、记忆、安全边界和本地桌面能力串起来。

- **本地优先**：Tauri + Rust 后端，设置、会话、角色包、记忆都保存在本机。
- **角色与引擎分离**：角色包只描述 persona、memory、头像/Live2D/未来的语音 等素材；引擎保持通用。
- **会动手**：可读写沙盒文件、编辑代码、跑 shell、联网搜索、截图/OCR、派生子 Agent、运行 workflow。
- **可控安全**：写文件、shell、打开路径、截图/OCR 等敏感操作走确认门；文件工具被限制在沙盒目录。
- **可持续推进**：`/goal` 可以设置长期目标，普通回合结束后继续自动驱动，直到完成、暂停、阻塞或预算耗尽。
- **Live2D 面板**：角色包可挂载 Cubism 4/5 模型（`untitled-pixi-live2d-engine` + PixiJS v8），在应用内渲染带 idle 物理/眨眼/呼吸的 Live2D 面板，支持缩放与拖拽。需先运行 `npm run fetch:cubism-core` 取回 Live2D Cubism Core（私有运行时，不入库），再在设置 > 人物包导入模型文件夹。

## 功能概览

### Agent Core

- 流式对话、随时中断、多轮 tool call loop。
- OpenAI-compatible、local、Anthropic、Gemini provider adapters。
- 内置 DeepSeek、DashScope、OpenAI、OpenRouter、GLM、MiniMax、xAI、Groq、Mistral、Moonshot、Perplexity、豆包、混元、阶跃星辰等供应商预设（多数走 OpenAI 兼容适配器，Anthropic / Gemini 走各自适配器）。
- 统一工具 schema，按 provider 方言输出。
- 多会话持久化、角色包切换、设置持久化。
- LLM API Key 使用系统凭据管理器保存。

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
- workflow journal/resume。
- Workflows live panel。
- `worktree_create` 隔离工作区。

### Reserved Interfaces

- Voice：语音输入（STT/ASR）已接入云端转写后端（DashScope `qwen3-asr-flash`、OpenAI 兼容 Whisper，由 `voice_stt_backend` 选择）；语音输出（TTS）已接通双后端——DashScope（默认音色 Cherry、模型 `qwen3-tts-flash`，返回音频 URL）与 GPT-SoVITS（默认 base `http://127.0.0.1:9880`，返回 base64 data URI），由 `voice_tts_backend` 选择。流式合成、播放队列、打断、语速/情感参数等待办。
- 角色包素材字段：avatar、Live2D（已实现，经 Tauri asset 协议加载）、voice（预留）等。

## 快速开始

前置依赖：

- Node.js 18+
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

## System Architecture

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

## Project Structure

```text
Demiurge/
├─ src/                         # React front-end
│  ├─ components/                # Sidebar, Composer, ToolCard, Settings, Workflows
│  ├─ lib/                       # Tauri API wrapper and shared types
│  ├─ App.tsx                    # Front-end orchestration and event binding
│  └─ style.css                  # Global UI styling
├─ src-tauri/                    # Rust/Tauri back-end
│  ├─ src/agent/                 # Agent loop, context, memory, goal, workflow
│  ├─ src/llm/                   # Provider adapters
│  ├─ src/tools/                 # Built-in tools and registry
│  ├─ src/permission/            # Confirmation and permission gate
│  ├─ src/store/                 # Settings/session persistence
│  ├─ src/pack/                  # Character pack loading
│  ├─ src/credentials.rs         # Keyring integration
│  ├─ src/connection_tests.rs    # Provider/Web Search/WebDAV connection tests
│  ├─ src/ocr.rs                 # OCR model and inference entry
│  ├─ src/media.rs               # DashScope media (image gen) + voice credential helpers
│  ├─ src/voice.rs               # Voice adapters (STT + TTS wired: DashScope / GPT-SoVITS)
│  ├─ src/companion.rs           # Companion, weather, safety detection
│  ├─ src/embed/                 # Remote embedding provider (OpenAI-compatible /v1/embeddings)
│  ├─ src/startup.rs             # OS autorun on boot
│  └─ src/mcp/                   # stdio MCP manager and dynamic tool discovery
├─ docs/                         # Design, implementation notes, roadmap
├─ packs/                        # Example character pack
├─ public/                       # Static assets
└─ package.json                  # Front-end and Tauri scripts
```

## Security Model

- 文件工具只能访问应用数据目录下的 `sandbox/`。
- 路径先做词法校验，再做 canonicalize 校验，防止 `..`、符号链接和 junction 逃逸。
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

## Documentation

- [模块技术原理文档（存档）](docs/modules/README.md) — 逐子系统的深度技术文档，从[架构总览](docs/modules/01-architecture-overview.md)开始
- [实现说明](docs/IMPLEMENTATION.md)
- [TODO / 路线图](docs/TODO.md)
- [Goal 持续驱动](docs/goal-continuous-driving.md)
- [Ultracode 多 Agent 编排](docs/ultracode-agent-orchestration.md)
- [Workflow JSON DSL](docs/workflow-json-dsl.md)
- [MVP 设计背景](docs/demiurge-mvp-design.md)

## Credits

- Provider logos are from [@lobehub/icons](https://github.com/lobehub/lobe-icons) (MIT). Brand logos remain the trademarks of their respective owners and are used only to identify the provider.

## License

Demiurge is released under the [MIT License](LICENSE). Character assets, voice assets, artwork, and persona packs based on specific works are user-managed local content and are not distributed with this repository.
