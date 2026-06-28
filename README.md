# Demiurge

轻量、开源的桌面 Agent 引擎。

Demiurge 是一个本地桌面应用：你配置自己的大模型端点，加载自己的角色包，然后让一个能调用工具的 Agent 在你的电脑上帮你聊天、读项目、搜索、编辑、执行命令和持续推进任务。它强调本地可控、权限可见、角色与引擎分离。

## 已实现能力

- 流式 Agent 对话：支持中断、多轮工具调用、工具结果回填和最终回答。
- 多 provider：OpenAI-compatible、local、Anthropic、Gemini，统一工具 schema 适配。
- 安全工具系统：工具带 risk/concurrency/permission/output metadata，敏感操作走确认门。
- 文件与代码工具：`read_file`、`write_file`、`edit_file`、`multi_edit`、`apply_patch`、`undo_edit`、`glob`、`grep`、`git_status`。
- Shell 执行：沙盒 cwd、超时、输出截断和执行前预览。
- Web Search：Bing、DuckDuckGo fallback，以及 Tavily、Brave、Exa 可选 adapter，支持域名过滤和 Sources 提醒。
- 上下文工程：项目指令、角色设定、运行环境、会话摘要、长期记忆、token-aware 裁剪和 `/compact`。
- Goal 持续驱动：`/goal`、token budget、pause/resume/continue、自动续跑和模型 `goal` 工具。
- 多 Agent 编排：`/ultracode`、只读 `agent_spawn`、fork context、workflow journal/resume、JSON workflow DSL、Workflows live panel。
- Computer Use 首层能力：窗口列表、屏幕截图、区域/窗口 OCR、OCR 模型下载入口。
- 语音预留接口：TTS/ASR adapter 已留出接口，后续可接 GPT-SoVITS、CosyVoice 或其他服务。
- API Key 安全存储：LLM 密钥走系统凭据管理器，不写入 `settings.json`。
- 多会话与角色包：会话、设置和角色包本地持久化。

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

首次启动后，在设置里选择 provider，填写 `base_url`、`model` 和 API Key。OpenAI-compatible 默认适合 DeepSeek 等兼容端点；local 适合 LM Studio、Ollama OpenAI-compatible、vLLM 等本地服务。

## 常用命令

- `/compact [keep=N]`：把较早对话折叠进 rolling summary。
- `/dream`：触发记忆整理。
- `/goal <objective> [+500k]`：设置持续目标和可选 token budget。
- `/goal status`：查看当前目标状态。
- `/goal pause` / `/goal resume` / `/goal continue`：控制目标续跑。
- `/ultracode <task>`：开启多 Agent 编排提示。
- `/workflows`：查看 workflow runs。
- `/workflow resume <run_id>`：用 journal 恢复 workflow 上下文。

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

角色包只描述角色设定。头像、语音、Live2D 等字段已为后续扩展预留，具体素材由用户本地管理。

## 项目结构

```text
Demiurge/
├─ src/                    # React 前端
│  ├─ components/           # 聊天、设置、工具卡片、workflow 面板等组件
│  ├─ lib/                  # Tauri API 封装与共享类型
│  ├─ App.tsx               # 前端状态编排与事件订阅
│  └─ style.css             # 全局样式
├─ src-tauri/               # Rust/Tauri 后端
│  ├─ src/agent/            # Agent loop、上下文、记忆、Goal、workflow
│  ├─ src/llm/              # Provider adapters
│  ├─ src/tools/            # 内置工具实现与注册表
│  ├─ src/permission/       # 工具确认与权限门
│  ├─ src/store/            # settings/session 持久化
│  ├─ src/pack/             # 角色包加载
│  ├─ src/credentials.rs    # keyring 凭据管理
│  ├─ src/ocr.rs            # OCR 模型与推理入口
│  └─ src/voice.rs          # 语音 adapter 预留接口
├─ docs/                    # 设计、实现说明、路线图和功能文档
├─ packs/                   # 仓库内示例角色包
├─ public/                  # 静态资源
└─ package.json             # 前端与 Tauri 脚本
```

## 安全边界

- 文件工具限定在沙盒目录，路径同时做词法校验和 canonicalize 校验。
- 写文件、shell、open_path、截图/OCR 等敏感工具会请求确认。
- shell 限制工作目录、超时和输出长度。
- 子 Agent 默认只读，不允许写文件、跑 shell 或递归派生。
- LLM API Key 存在系统凭据管理器中。
- Web Search 外部 adapter key 当前通过环境变量读取，后续可接入同一设置界面。

## 文档

- [实现说明](docs/IMPLEMENTATION.md)
- [TODO / 路线图](docs/TODO.md)
- [Goal 持续驱动](docs/goal-continuous-driving.md)
- [Ultracode 多 Agent 编排](docs/ultracode-agent-orchestration.md)
- [Workflow JSON DSL](docs/workflow-json-dsl.md)
- [MVP 设计背景](docs/demiurge-mvp-design.md)

## 验证

```bash
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

## 许可

代码采用 [MIT](LICENSE)。具体角色素材、语音、美术和基于特定作品的人格设定由用户自行管理，不随本仓库分发。
