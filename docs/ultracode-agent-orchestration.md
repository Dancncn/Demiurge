# Ultracode Multi-Agent Orchestration

Demiurge 的 Ultracode 集成采用渐进式落地：先提供可运行的只读子 Agent 编排，再扩展到可恢复 workflow、React live panel 和隔离工作区。

## 已实现

- `/ultracode [task]`：显式启用本轮多 Agent 编排模式。该指令只作为临时 system overlay 注入，不会把完整编排手册长期写入会话历史。
- `agent_spawn` 工具：主 Agent 可派生只读子 Agent，用于代码探索、方案复核、风险审查、反例验证和遗漏检查。
- 子 Agent 上下文继承：子 Agent 会继承当前角色包、长期记忆、会话摘要和最近消息摘录。
- 子 Agent 工具收敛：子 Agent 只暴露 `read_file`、`glob`、`grep`、`git_status`、`system_info`、`web_fetch`、`web_search`、`context_inspect` 这 8 个只读工具（schema 过滤与执行门禁都用 `subagent.rs` 的 `READ_ONLY_TOOLS`），不能写文件、跑 shell、截图或递归派生子 Agent。
- 工具 schema 过滤：子 Agent 请求只携带只读工具 schema，减少无关工具定义占用上下文，也降低误调用风险。
- `context_mode=fork`：子 Agent 继承父会话消息，并对未配对的 tool call 插入固定 placeholder，避免 fork 请求破坏 tool_result 配对。
- `/compact [keep=N]`：手动触发上下文折叠，把旧消息压缩进 rolling summary。
- `context_inspect` / `context_collapse`：模型可检查上下文压力，并在确认后触发折叠。
- Deferred Tool Search：主 Agent 默认只加载 core tools；截图/OCR/打开路径等低频能力通过 `tool_search` 发现、`execute_tool` 代理执行，保持 tools schema 稳定。
- Workflow journal：`/ultracode` 会写 `.demiurge/workflow-runs/<run_id>/journal.jsonl`；`/workflows` 可列出 run，`/workflow resume <run_id>` 可用 journal 恢复。
- Workflow JSON DSL：沙盒 `.demiurge/workflows/*.json` 可定义 `log`、`phase`、`agent`、`parallel`、`pipeline`、`budget` step，由 Rust 原生运行时执行。
- Workflows live panel：顶部 `Workflows` 入口可查看定义、启动 workflow、实时查看 run/agent/log 状态，并支持 stop/resume。
- `worktree_create`：在沙盒 Git 仓库下创建隔离 worktree，用于大改动或实验分支。
- Goal 持续驱动：`/goal` 可把长任务持续推进；普通 `/ultracode`、workflow 或手动 Agent 回合结束后，若 goal 仍 active，会继续自动调度下一轮。

## 使用方式

直接在聊天中输入：

```text
/ultracode 分析 agent 模块的上下文裁剪逻辑，并找出可以优化的点
```

主 Agent 会根据任务复杂度决定是否调用 `agent_spawn`。小任务会直接完成；复杂任务通常会拆成 Explorer、Reviewer、Verifier、Critic 等互补视角，然后由主 Agent 合成结果、修改代码并运行验证。

## 上下文工程优化

参考 Ultracode 与 fork-subagent 的设计，本次落地了三条低风险优化：

1. 编排提示临时注入：`/ultracode` 的规则不进入长期会话历史，避免后续普通对话被大型 workflow 提示污染。
2. 子 Agent 继承摘要而非全量历史：默认 `brief` 模式只传会话摘要和少量最近消息，必要时可用 `recent` 传更多近期上下文。
3. 子 Agent 工具 schema 精简：只发送只读工具定义，减少每次 LLM 请求的固定 token 成本。
4. Fork 模式修复 tool pairing：`fork` 模式继承父消息时会补固定 placeholder tool_result，使当前 turn 中尚未完成的 `agent_spawn` 不会让子请求 400。
5. 主 tools schema 稳定：低频工具进入 deferred 池，不随发现动态注入 schema，而是经 `execute_tool` 代理执行。
6. Journal-first resume：长任务的关键事件落盘成 JSONL，恢复时作为临时 overlay 注入，不污染普通会话 prompt。

后续可继续参考 Ultracode 做：

- per-agent budget：run 级硬预算与消耗追踪已落地（`budget` step 会在预算耗尽时中止后续 `agent` step 并记录用量，子 Agent 也支持 `max_total_tokens` 硬停机）；后续可做的是 per-agent 独立硬预算——让每个子 Agent 各自带独立上限，而非共享 run 预算的剩余额度。
- cross-process recovery：应用重启后从 journal 恢复 live run 进度，而不是仅恢复为 overlay。
- judge panel：同一设计让多个 Reviewer 独立打分，再由主 Agent 合成取舍。
- context compaction policy：把大块工具结果压成结构化 evidence packet，再进入后续主上下文。

## 当前边界

当前版本仍不是完整的通用 workflow runtime。它已经有 fork-style 子 Agent、deferred tools、上下文折叠、journal/resume、Rust 原生 JSON workflow DSL、live panel、worktree 创建和 Goal 持续驱动骨架；但还没有 JavaScript workflow DSL、后台 LocalAgentTask 式长任务、coordinator/team/swarm/mailbox、per-agent 独立硬预算上限（目前子 Agent 共享 run 级预算的剩余额度）或跨进程 live 进度恢复（已能恢复 durable snapshot，但不会重新拉起后台 live 执行）。主 Agent 仍是唯一做最终判断和答复的角色。
