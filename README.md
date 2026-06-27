<div align="center">

<img src="docs/assets/logo.png" width="128" alt="Demiurge" />

# Demiurge

**轻量、开源的桌面伴侣 Agent 引擎**

加载你自己的「角色包」，用一个会自主调用工具的 agent 循环驱动它——
既能入戏陪你聊天，也能在你的电脑上动手帮你做事。

[![License](https://img.shields.io/badge/License-MIT-black.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/Tauri-2.x-24C8DB?logo=tauri&logoColor=white)](https://tauri.app/)
[![React](https://img.shields.io/badge/React-18-20232A?logo=react&logoColor=61DAFB)](https://react.dev/)
[![TypeScript](https://img.shields.io/badge/TypeScript-3178C6?logo=typescript&logoColor=white)](https://www.typescriptlang.org/)
[![Tailwind CSS](https://img.shields.io/badge/Tailwind_CSS-4-38BDF8?logo=tailwindcss&logoColor=white)](https://tailwindcss.com/)

</div>

---

## 这是什么

Demiurge 是一个**桌面伴侣的「空引擎」**。它本身不内置任何角色——你给它一个**角色包**
（人格，未来还有头像 / 语音 / Live2D），它就化身为那个角色。底层是一个真正的 agent 循环：
角色不只是回话，还能调用工具读写文件、打开网页、联网搜索、查看系统状态，重要操作前会先征求你同意。

- **轻量**：Tauri + Rust 内核，小巧省内存，不打包 JS 运行时——能安心和游戏一起跑在轻薄本上。
- **自带大脑**：没有托管后端、不收集数据。把引擎指向你自己的 LLM 端点即可（默认 DeepSeek，或任意 OpenAI 兼容端点如 LM Studio）。
- **角色与引擎分离**：引擎通用、纯净，可加载任意角色；具体角色以单独的角色包形式由你导入。
- **会动手，且安全**：文件操作被物理限制在沙盒目录；删除 / 覆盖 / 打开应用等有副作用的操作会先弹窗确认。

## 功能

- 流式对话（逐字输出，可随时中断）
- Markdown / 代码块（带复制）/ 数学公式渲染
- 工具调用：`open_path`（打开文件 / 应用 / 网址）、`read_file` / `write_file`（沙盒内）、`web_search`、`system_info`
- 权限确认弹窗（不可逆操作执行前征求同意）
- 多会话管理（新建 / 切换 / 删除）、角色包切换、会话本地持久化（重启恢复）
- 简洁的浅色聊天界面

## 快速开始

**前置**：[Node.js](https://nodejs.org/) ≥ 18、[Rust](https://www.rust-lang.org/tools/install) 稳定版、Windows 自带的 WebView2（macOS / Linux 用系统 WebView）。

```bash
git clone <your-repo-url> demiurge
cd demiurge
npm install
npm run tauri dev      # 开发运行
npm run tauri build    # 打包安装器
```

首次启动后，点左下角设置，填入你的 **API Key**：

- 默认走 **DeepSeek**：`base_url = https://api.deepseek.com/v1`，`model = deepseek-chat`
- 想用别的？把 `base_url` + `model` 改成任意 OpenAI 兼容端点即可（例如本地的 LM Studio），无需改代码。

然后就能开始聊天了。试试「现在几点了」「帮我在沙盒里建个 notes.txt」「搜一下今天的新闻」来体验工具调用。

## 角色包

引擎不内置角色——角色以角色包形式存在。最小格式：

```
packs/<id>/
├─ manifest.json   { "id": "...", "name": "...", "persona": "persona.md" }
└─ persona.md      角色人格正文（会拼进系统提示词）
```

把你的角色包放进**应用数据目录**的 `packs/<id>/` 下，在界面里选择即可。仓库只附带一个通用的
`default` 包作为格式参考。`avatar` / Live2D / 语音等字段已为后续预留。

## 安全说明

- 文件工具（读 / 写）被**物理限制**在应用数据目录下的 `sandbox/`，越界路径（含符号链接 / junction）一律拒绝。
- `open_path`、`write_file` 等有副作用的操作会**先弹确认框**，你不点同意就不执行。
- API Key 目前以明文存于本机配置文件（仅本机使用）；后续计划改用系统凭据管理器。

## 文档

- [设计与技术路线](docs/demiurge-mvp-design.md)
- [实现说明（架构 / 事件协议 / 如何扩展）](docs/IMPLEMENTATION.md)
- [TODO / 路线图](docs/TODO.md)

## 许可

引擎代码采用 [MIT](LICENSE)。具体角色的素材（美术、语音、基于特定作品的人格）由用户自备，不随本仓库分发。
