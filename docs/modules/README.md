# 模块技术原理文档（存档）

本目录是 Demiurge 的**存档级模块技术原理文档**。每一篇对应一个子系统，基于真实源码撰写，逐处引用 `文件:行号`，重点讲清楚 **数据如何流动** 与 **为什么这样设计**，供协作开发者深读、排障和扩展时查阅。

> 这些文档描述的是 **当前代码实现**。面向用户的功能介绍见仓库根 [README.md](../../README.md)；协作者视角的实现说明见 [../IMPLEMENTATION.md](../IMPLEMENTATION.md)；路线图见 [../TODO.md](../TODO.md)。

## 阅读地图

建议先读 [01-架构总览](01-architecture-overview.md) 建立全局心智模型，再按需深入各子系统。

| # | 文档 | 一句话定位 |
|---|------|-----------|
| 01 | [架构总览](01-architecture-overview.md) | 分层架构、AppState、Tauri 命令/事件桥、一次回合的端到端数据流 |
| 02 | [Agent 主循环与 Session Engine](02-agent-loop-session-engine.md) | 回合执行核心：入口互斥、流式多轮工具循环、会话持久化收敛与双发事件 |
| 03 | [上下文工程](03-context-engineering.md) | 字符预算组装 system prompt、token 预算裁剪历史、滚动摘要回流与可视化 |
| 04 | [分层长期记忆与 Dream](04-memory-system.md) | user/project/session/pack 四层 Markdown 记忆的读写、自动提取与 `/dream` 整理 |
| 05 | [Goal 持续驱动](05-goal-driving.md) | 会话级目标状态机与每回合自动续跑（完成/暂停/阻塞/超预算/超回合） |
| 06 | [多 Agent 编排](06-multi-agent-orchestration.md) | 只读子 Agent、`/ultracode` overlay、证据包/多评审、自定义 Agent/team |
| 07 | [Workflow JSON DSL 运行时](07-workflow-runtime.md) | 声明式编排运行时，journal + durable snapshot 双轨持久化与崩溃恢复 |
| 08 | [Skills 系统](08-skills-system.md) | 约定目录下 `SKILL.md` 的发现、打分与 system prompt 注入 |
| 09 | [LLM Provider 适配层](09-llm-providers.md) | `ProviderProfile` 单一能力入口、三套 schema 方言、流式归一化与 reasoning effort |
| 10 | [工具注册表与文件/编辑工具](10-tool-system-files.md) | `registry`/`execute` 统一入口、沙盒路径防逃逸、读写/编辑/搜索工具 |
| 11 | [执行类与联网类工具](11-tools-shell-web.md) | shell 三档隔离与进程树治理、Web Search/Fetch 多后端、deferred 工具代理 |
| 12 | [权限模型与安全边界](12-permission-security.md) | `PermissionMode` 决策、确认往返与 scope、Plan Mode、沙盒、审计、capabilities |
| 13 | [持久化、凭据与连接测试](13-persistence-config.md) | Settings/Session 落盘、keyring 凭据与明文迁移、不落盘的连接测试 |
| 14 | [角色包系统](14-pack-system.md) | manifest 校验、persona/avatar 注入、zip 导入安全校验与默认包落地 |
| 15 | [多模态与 Computer Use](15-multimodal-computer-use.md) | 本地 OCR（PP-OCRv5）、屏幕窗口/截图、语音 STT 接入与 TTS 预留、媒体生成 |
| 16 | [MCP 集成（stdio 第一阶段）](16-mcp-integration.md) | stdio MCP Manager、动态 `mcp__server__tool` 发现与调用、资源读取、凭据脱敏 |
| 17 | [前端架构](17-frontend-architecture.md) | App 状态编排、`api.ts` typed 桥、事件契约、核心组件与 i18n |
| 18 | [应用外壳、命令面与构建](18-app-shell-build.md) | `AppState`、Tauri 命令分类、初始化与目录布局、构建与 release profile |
| 19 | [Live2D 面板](19-live2d-panel.md) | Cubism 4/5 模型挂载、untitled-pixi-live2d-engine + Tauri asset 协议、Cubism Core 私有运行时、bundle 隔离与待打磨 |

## 文档约定

- **行号是写作时刻的快照**：源码演进后行号会漂移，引用仅作定位线索，以代码为准。
- **诚实标注状态**：预留/占位能力（如 TTS 后端、`usage_limited` 预留态、per-agent 独立硬预算等）均按当前实现如实标注，不写成已完成。
- **命名中立**：文档不把本项目描述为任何特定产品的衍生品；兼容能力统一使用项目自有的 `.demiurge/compat/` 目录约定。

## 维护提示

新增或重构子系统后，请同步更新对应模块文档与本索引；涉及对外行为变化时，一并回写 README.md / IMPLEMENTATION.md / TODO.md，保持四处一致。
