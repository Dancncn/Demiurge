# 实现说明（给协作者）

面向想读懂 / 扩展代码的人。设计动机见 [demiurge-mvp-design.md](./demiurge-mvp-design.md)，待办见 [TODO.md](./TODO.md)。

## 总览

```
React UI (webview)  ──invoke 命令──▶  Rust 内核 (Tauri)  ──HTTP 流──▶  LLM 端点
       ▲                                    │
       └──────── emit 事件（流式/工具/确认）──┘
```

- **后端 = Rust**，承载全部 agent 逻辑；**前端 = React**，只做展示与交互。
- 二者通过 **Tauri 命令**（前端→后端，请求/响应）与 **Tauri 事件**（后端→前端，单向推送）通信。

## Rust 内核（`src-tauri/src/`）

| 模块 | 职责 | 关键入口 |
|---|---|---|
| `lib.rs` | 全局状态 `AppState`、命令注册、Tauri builder、`setup()` 初始化路径/持久化 | `run()` |
| `agent/runner.rs` | **Agent 循环**：调 LLM → 执行工具 → 喂回 → 重复 | `run_turn()` |
| `agent/conversation.rs` | OpenAI 兼容消息结构（role / tool_calls / tool_result） | `Message` |
| `agent/context.rs` | 上下文裁剪（先截老工具输出，再丢更老回合） | `trim()` |
| `agent/persona.rs` | system prompt = 引擎基础指令 + 角色包人格 | `assemble()` |
| `llm/mod.rs` | OpenAI 兼容流式客户端，SSE 解析 + 流式 tool_calls 累积；含 `ProviderProfile` 地基 | `stream_completion()` |
| `tools/mod.rs` | 工具注册表 + metadata（risk/concurrency/permission/output policy）+ 统一执行入口 + 沙盒路径解析 | `registry()` / `execute()` |
| `tools/*.rs` | 各内置工具实现 | `open_path` / `read_file` / `write_file` / `web_search` / `system_info` / `glob` / `grep` / `git_status` |
| `permission/mod.rs` | confirm 工具的前端确认往返（oneshot 通道） | `confirm()` |
| `pack/mod.rs` | 角色包清单加载、首启动落地默认包 | `load_pack()` / `ensure_default()` |
| `store/mod.rs` | 设置 / 多会话 JSON 落盘（含旧版迁移） | `Settings` / `Session` / `SessionStore` |

数据目录（Tauri `app_data_dir`）下：`settings.json`、`sessions.json`（多会话 + 活动会话）、`sandbox/`、`packs/`。
（旧版单会话 `conversation.json` 在首次启动时会自动迁移成一条会话。）

### Agent 循环（`run_turn`）
1. 追加用户消息、持久化。
2. 拼 `system + 裁剪后的历史`，流式调 LLM；正文增量通过 `assistant-delta` 事件实时推给前端。
3. 若返回 `tool_calls`：把带 tool_calls 的 assistant 消息入历史，逐个工具——
   - `emit tool-start` → 权限门（auto 直接放行 / confirm 走前端确认）→ 执行 → `emit tool-end`；
   - 把结果作为 `tool` 消息喂回（**每个 tool_call 都必须有对应结果**，否则下一轮 400）。
   - 然后回到第 2 步，让模型基于结果继续。
4. 无 tool_calls → 最终答复，`emit assistant-done`。
5. 全程检查 `cancel` 标志；用户中断会唤醒待确认项并尽快收尾。

### 前后端事件协议
后端 emit（`src-tauri` → 前端 `src/lib/api.ts` 监听）：

| 事件 | 载荷 | 含义 |
|---|---|---|
| `assistant-start` | — | 一次 LLM 调用开始 |
| `assistant-delta` | `string` | 正文增量 token |
| `assistant-done` | `string` | 最终答复（完整正文） |
| `assistant-interrupted` | — | 本轮被用户中断 |
| `tool-start` | `{tool_call_id, name, args, description?, risk?, permission_effect?, concurrency?}` | 工具开始执行；前端用 `tool_call_id` 关联卡片 |
| `tool-end` | `{tool_call_id, name, ok, result}` | 工具结束（ok=是否放行） |
| `tool-confirm-request` | `{id, tool, args, description?, risk?, effect?, scope?, reason?, summary?}` | 请前端弹确认框；前端回 `respond_confirm(id, allow)` |

前端命令（`invoke`）：`send` / `interrupt` / `respond_confirm` / `get_settings` / `save_settings` / `list_packs` / `list_sessions` / `get_history` / `new_session` / `select_session` / `delete_session` / `open_sandbox`。
Agent 循环在开始时**捕获活动会话 id**，整轮写入都落到这一段对话——即便用户中途切换会话也不会串台。

## 安全模型要点
- **沙盒**：`read_file` / `write_file` 限定在 `app_data_dir/sandbox`。路径先做词法 `..` 折叠并 `starts_with` 校验，
  再对「目标最近存在的祖先」做 `canonicalize` 二次校验，挡住 junction / 符号链接逃逸。
- **搜索工具**：`glob` / `grep` 只遍历沙盒内文件，拒绝绝对路径与 `..` 越界输入，并对返回条数、单文件大小和行长度设上限。
- **open_path**：标为 confirm（执行前用户确认），并拒绝 UNC 路径与非安全 URL 协议（只放行 http/https/file/mailto）。
- **权限门**：confirm 类工具经 `oneshot` 通道等待前端裁决；`interrupt` 会立即唤醒所有待确认项按拒绝处理。
  当前已有 `PermissionDecision` / `PermissionPromptPayload` 骨架，事件会携带风险、说明和决策原因；持久化 allow/deny 规则后续再接入。
- **API Key**：MVP 以明文存 `settings.json`（仅本机）。后续可改 Windows 凭据管理器（keyring）。

## 前端（`src/`）
- `App.tsx`：编排——持有状态、注册事件、渲染布局。把后端事件流翻译成展示项 `DisplayItem[]`。
- `components/`：`Sidebar` / `Composer` / `MessageList`（含用户气泡、助手消息、思考态）/ `Markdown`（GFM + KaTeX + 代码块复制 + 流式防闪烁）/ `ToolCard` / `ConfirmDialog` / `SettingsDialog` / `Icons`。
- `lib/api.ts`：Tauri 命令 + 事件订阅的类型化封装。`lib/types.ts`：与 Rust 对应的类型。
- 视觉为 ChatGPT 风浅色主题（`src/style.css`）。

## 构建
- 标准流程：`npm install` → `npm run tauri dev` / `npm run tauri build`。克隆即可独立编译，无需额外配置。
- （本地可选加速）`src-tauri/.cargo/config.toml` 可把 `target-dir` 指向另一个 Tauri 项目的 target，
  复用其已编译依赖树省一次冷编译。该文件含绝对路径、**仅本机有效，已加入 `.gitignore` 不入库**——
  公开仓库的克隆者走标准独立编译，不受影响。
- crate 源码本就全局缓存在 `~/.cargo/registry`，跨项目共享；只有「已编译产物」需共享 target-dir 才能复用。

## 如何扩展

**加一个工具**：在 `tools/mod.rs` 的 `registry()` 加一项（名称/描述/JSON Schema，并补齐 `ToolRisk` / `ToolConcurrency` / `PermissionPolicy` / `ToolOutputPolicy`），在 `execute()` 加一条分支，新建 `tools/<name>.rs` 实现。confirm 类会自动走确认门。

**加一个角色包**：在数据目录 `packs/<id>/` 放 `manifest.json` + `persona.md`（见设计文档格式），在应用里选择即可。

**换 LLM 端点**：设置里改 `base_url` + `model`（任意 OpenAI 兼容端点）。无需改代码。
