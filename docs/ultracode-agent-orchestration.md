# Ultracode Multi-Agent Orchestration

Demiurge 的 Ultracode 集成采用渐进式落地：先提供可运行的只读子 Agent 编排，再逐步扩展到可恢复 workflow、TUI 面板和隔离工作区。

## 已实现

- `/ultracode [task]`：显式启用本轮多 Agent 编排模式。该指令只作为临时 system overlay 注入，不会把完整编排手册长期写入会话历史。
- `agent_spawn` 工具：主 Agent 可派生只读子 Agent，用于代码探索、方案复核、风险审查、反例验证和遗漏检查。
- 子 Agent 上下文继承：子 Agent 会继承当前角色包、长期记忆、会话摘要和最近消息摘录。
- 子 Agent 工具收敛：子 Agent 只暴露 `read_file`、`glob`、`grep`、`git_status`、`system_info`、`web_search`，不能写文件、跑 shell、截图或递归派生子 Agent。
- 工具 schema 过滤：子 Agent 请求只携带只读工具 schema，减少无关工具定义占用上下文，也降低误调用风险。

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

后续可继续参考 Ultracode 做：

- fork-style placeholder tool result：让父上下文在子 Agent 执行期间保持稳定，进一步提升 prompt cache 命中率。
- workflow journal：把多 Agent 计划、阶段、子任务和结果写入可恢复日志，支持中断后继续。
- worktree isolation：对子 Agent 或并行实现分支分配临时 worktree，避免文件修改竞争。
- judge panel：同一设计让多个 Reviewer 独立打分，再由主 Agent 合成取舍。
- context compaction policy：把大块工具结果压成结构化 evidence packet，再进入后续主上下文。

## 当前边界

当前版本不是完整的 Ultracode workflow runtime。它没有 JavaScript workflow DSL、阶段面板、预算追踪、恢复日志或并行 worktree。主 Agent 仍是唯一执行修改和最终回复的角色；子 Agent 是只读研究员。
