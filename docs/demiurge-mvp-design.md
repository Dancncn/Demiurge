# Demiurge — 设计与技术路线

> 本文是 Demiurge 的设计文档 / 技术路线。MVP 已按此实现，实现细节见
> [IMPLEMENTATION.md](./IMPLEMENTATION.md)，后续计划见 [TODO.md](./TODO.md)。
> 标注 ✅ 的为已落地，🔜 为已设计待实现。

**Demiurge** 是一个轻量、开源的桌面伴侣 Agent 引擎。它加载 *角色包（character pack）*，
用一个会自主调用工具的 agent 循环驱动角色，使其既能「入戏聊天」，也能在用户机器上执行
受限的任务。引擎本身通用、不内置任何角色——角色由用户自备的角色包提供。

### 定位
- **轻量。** Tauri + Rust 内核——小二进制、低内存、不打包 JS 运行时。目标是能和游戏一起
  跑在 8GB 笔记本上，区别于此领域常见的 Electron/Python 伴侣。
- **自带大脑（Bring-your-own-brain）。** 无托管后端、无中继。用户把引擎指向自己的 LLM 端点
  （DeepSeek API，或本地 LM Studio 等 OpenAI 兼容端点）。
- **角色与引擎分离。** 引擎通用，可加载任意角色。具体角色（人格，未来还有头像 / 语音 / Live2D）
  以单独的角色包形式由用户导入，引擎本身保持纯净通用。

## 技术栈

| 层 | 选型 | 说明 |
|---|---|---|
| 内核 / 后端 | **Rust（Tauri 2.x）** ✅ | 全部 agent 逻辑、工具、持久化、系统访问 |
| UI / 前端 | **React 18 + TypeScript + Tailwind CSS 4** ✅ | 跑在系统 webview；聊天显示、确认弹窗、角色容器。视觉为 ChatGPT 风浅色主题 |
| LLM | **OpenAI 兼容适配器**，流式 ✅ | 默认 **DeepSeek**（在线，自带 Key）；换任意 OpenAI 兼容端点（如 LM Studio）只改 `base_url` + `model` |
| 开发工具 | Vite + npm ✅ | 仅开发期（包管理 + 打包），**不随产品分发** |
| Python | **无** ✅ | 仅在后期作为外部 TTS 进程经 HTTP 调用出现 |

需厘清的几点：
- 没有「TS 后端」。后端 = Rust；TS 只是 UI 层。两者经 Tauri 命令 / 事件通信。
- Tauri **不**打包 JS 运行时。agent 循环用 **Rust** 写（无 JS sidecar、无额外运行时、无 IPC 开销）。
- 引擎不需要 Python。本地 GPT-SoVITS / CosyVoice 是用户自运行的独立进程，经 HTTP 访问；TTS 已推迟，故 MVP 无 Python。

## 已锁定的两个关键决策
（原计划「编码前先锁定 MVP 工具集 + 默认 provider」，现已锁定：）
- **默认 provider / model**：DeepSeek `deepseek-chat`，`base_url = https://api.deepseek.com/v1`。**不做 Ollama**（按需求取舍）。
- **MVP 工具集**：见下方表格，已实现。

## Agent 内核（MVP — 10 个组件，均 ✅）
Owner 标记：**[R]** Rust 内核 · **[F]** 前端。

1. **LLM 适配器** [R] —— OpenAI 兼容、流式客户端。循环调用的「大脑」。
2. **会话状态** [R] —— role / tool_use / tool_result 的消息历史；每轮发送的就是它。
3. **Agent 循环** [R] —— 输入 + 上下文 → 调 LLM → 若请求工具则执行 → 把 tool_result 喂回 → 重复，直到最终答复。整个系统的心脏。
4. **人格拼装** [R] —— system prompt = 引擎基础指令（工具规则、确认规则、输出格式）+ 角色包人格。角色包从这里插入。
5. **工具注册表 + 统一接口** [R] —— 每个工具 = 名称 + 描述 + 输入 JSON Schema + execute。循环遍历这张表。
6. **工具执行** [R] —— 真实触碰系统 / 文件；输出与错误原样回写为 tool_result。未实现或失败即报错，不吞、不 mock。
7. **权限门** [R + F] —— 工具分自动执行 / 需确认；不可逆操作（删、覆盖、发送、网络写）执行前先弹确认框。
8. **上下文管理** [R] —— 历史超阈值时：先砍老工具输出，再折叠更老的回合。全天聊天 → 这直接决定 API 账单，非可选。
9. **持久化** [R] —— 会话落盘，下次启动恢复。这就是 MVP 的全部「记忆」——不做向量 RAG。
10. **流式 + 中断** [R 发流 · F 显示 + 中断] —— 逐 token 推到 UI，用户可中断。让它「活」起来，也为后续逐句 TTS 铺路。

## MVP 工具集
一小撮有边界的工具，够把循环跑起来。示例性质——循环比具体工具更重要。

| 工具 | 动作 | 默认 |
|---|---|---|
| `open_path` | 用系统默认处理器打开文件 / 应用 / URL | **confirm**¹ |
| `read_file` | 读取沙盒目录内的文件 | auto |
| `write_file` | 在沙盒目录内创建 / 覆盖文件 | **confirm** |
| `web_search` | 联网搜索，返回带来源链接的结果摘要（Bing + DuckDuckGo fallback，支持域名过滤与 context cap） | auto |
| `system_info` | 读取时间 / 系统 / 架构等基础状态 | auto |

作用域是结构性强制的（如文件工具被物理限制在沙盒目录），不靠提示词。

> ¹ 设计稿原定 `open_path` 为 auto；安全审查发现它会以系统默认语义启动任意可执行 / 协议处理器，
> 一次提示注入即可静默触发，故改为 **confirm** 并叠加校验（拒绝 UNC / 危险协议）。详见 IMPLEMENTATION.md。

## 权限模型
- **auto** —— 只读 / 幂等 / 低风险 → 不打断直接跑。
- **confirm** —— 任何不可逆或有副作用（覆盖、删除、发送、写出沙盒、网络写）→ 执行前弹前端确认框。
- 限制住在权限层，不在人格提示词里。只读工具就是没有写能力，无论对话怎么要求。

## 角色包（MVP 格式）
文本版 MVP 的最小清单；格式为可成长而设计。

```json
{
  "id": "cyrene",
  "name": "Cyrene",
  "persona": "persona.md",
  "avatar": "avatar.png"
}
```

预留（对应轨道落地时再加，无需重写内核）：🔜 Live2D 模型路径、表情 / 情绪映射、TTS 后端 + 音色、问候 / 待机台词。

## MVP 范围外
推迟——已为其设计，后续经「角色包字段 + 适配器」加入：
- 🔜 TTS / ASR 适配器（语音输入 / 输出）
- 🔜 桌宠视觉外壳：透明置顶窗口、Live2D、点击穿透、主动说话、屏幕感知

不需要——**不要**搭：
- 向量 / 长期记忆 RAG（持久化已覆盖 MVP）
- 完整工作流运行时（多 Agent 只读编排已有第一版；完整 journal / worktree / panel 仍不进 MVP）

## 仓库结构（现状）
```
demiurge/
├─ src-tauri/            # Rust 内核
│  ├─ src/
│  │  ├─ agent/          # 循环、会话状态、上下文管理、人格拼装
│  │  ├─ llm/            # OpenAI 兼容适配器（流式）
│  │  ├─ tools/          # 注册表 + 接口 + 内置工具
│  │  ├─ permission/     # auto vs confirm 门控
│  │  ├─ pack/           # 角色包加载 + 清单
│  │  └─ store/          # 会话 / 设置持久化
│  │                     # （.cargo/config.toml 为本地编译加速，已 gitignore，不入库）
│  └─ tauri.conf.json
├─ src/                  # React + TypeScript + Tailwind UI
│  ├─ components/        # Sidebar / Composer / MessageList / Markdown / 对话框 / 图标
│  └─ lib/               # api（Tauri 命令/事件封装）+ types
├─ public/               # 静态资源（应用内头像等）
├─ packs/                # 本地角色包；.gitignore（默认包除外，作格式参考）
└─ docs/                 # 设计 / 实现 / TODO
```

## 参考
- **Open-LLM-VTuber**、**Live2DPet** —— agent 如何被托进一个角色 / 桌宠外壳（本项目最终形态的参考）。
