# Goal 持续驱动

> 存档级技术原理文档。读者为协作开发者。
> 主要源码：`src-tauri/src/agent/goal.rs`、`src-tauri/src/tools/goal_tool.rs`。
> 关联源码：`src-tauri/src/lib.rs`(slash/命令入口与续跑挂载)、`src-tauri/src/agent/runner.rs`(回合执行与 token 记账)、`src-tauri/src/agent/budget.rs`(token 估算)、`src-tauri/src/agent/prompt.rs`(目标上下文注入)、`src-tauri/src/tools/mod.rs`(goal 工具注册)、`src/App.tsx`(前端历史过滤)。

## 1. 模块职责与定位

Goal 持续驱动让用户用一句 `/goal <目标>` 设定一个跨越多个回合的长期目标，之后引擎在每个普通回合结束后**自动续跑**，反复把目标重新注入模型上下文并推动其继续工作，直到满足终止条件之一：目标完成、被用户暂停、连续阻塞、token 预算耗尽、达到最大续跑回合数，或被用户取消。

它本质上是一个**挂在回合执行链路尾部的调度器**，自身不直接调用 LLM，而是通过既有的 `run_turn_with_options`（`src-tauri/src/agent/runner.rs:146`）发起新回合。状态全部存放在 `Session.goal` 字段中，随 session 持久化，因此目标是**按会话隔离**的：每个会话最多一个活跃目标。

设计动机：把"反复让模型继续干活"这件事从前端轮询/用户手动催促，下沉为后端在一次命令调用内部的同步循环，使得续跑对前端透明（前端只看到模型连续产出，看不到内部续跑消息），同时把预算、回合数、阻塞计数等护栏集中在一处管理。

## 2. 关键类型与入口函数

### 2.1 状态类型

`GoalStatus`（`goal.rs:13-23`）是七态枚举，序列化为 snake_case：

| 枚举 | 序列化值（`status_value`，`goal.rs:713`） | 展示标签（`status_label`，`goal.rs:701`） | 含义 |
|---|---|---|---|
| `Active` | `active` | Active | 正在续跑或可被续跑 |
| `Paused` | `paused` | Paused | 用户暂停，计时停止 |
| `Blocked` | `blocked` | Blocked | 同一阻塞连续 3 次确认后冻结 |
| `BudgetLimited` | `budget_limited` | Budget Limited | token 预算耗尽 |
| `UsageLimited` | `usage_limited` | Usage Limited | **预留态，当前代码无任何路径会设置它**（见 §6） |
| `MaxTurns` | `max_turns` | Max Turns Reached | 续跑回合数达到 `MAX_GOAL_TURNS` |
| `Complete` | `complete` | Complete | 已完成 |

`GoalState`（`goal.rs:25-41`）是持久化的真实状态，关键字段：

- `objective: String` —— 目标文本。
- `status: GoalStatus`。
- `token_budget: Option<usize>` —— 用户指定的 token 预算上限；`None` 表示无预算约束。
- `tokens_used: usize` —— 累计消耗（精确 usage 或本地估算，见 §3.4）。
- `start_time` / `paused_at` / `accumulated_active_ms` —— 用于计算"活跃时间"，暂停时不计时。
- `blocked_attempts: usize` + `last_block_reason: Option<String>` —— 连续阻塞计数与上次阻塞原因。
- `turns_executed: usize` —— 续跑回合计数（受 `MAX_GOAL_TURNS` 约束）。
- `budget_limit_notified: bool`（`#[serde(default)]`，`goal.rs:39-40`）—— 预算提醒是否已发出，保证只提醒一次。

`GoalPanelState`（`goal.rs:43-63`）是给前端 UI 的派生只读视图，由 `panel_state_from_goal`（`goal.rs:274`）从 `GoalState` 计算得到，额外提供 `token_remaining`、`elapsed`、`can_pause/can_resume/can_continue/can_clear` 等 UI 决策字段。注意 `can_clear` 恒为 `true`（`goal.rs:295`）。

### 2.2 常量

- `BLOCKED_CONSECUTIVE_THRESHOLD: usize = 3`（`goal.rs:9`）—— 同因阻塞达到 3 次才真正进入 `Blocked`。
- `MAX_GOAL_TURNS: usize = 150`（`goal.rs:10`）—— 单段续跑的回合上限。
- `MAX_OBJECTIVE_CHARS: usize = 4000`（`goal.rs:11`）—— 目标文本字符上限。

### 2.3 入口函数

- `handle_slash`（`goal.rs:73`）—— 解析并分发 `/goal …` 子命令，由 `lib.rs:316-343` 在 `send` 命令中调用。
- `drive_after_turn`（`goal.rs:170`）—— **续跑调度器核心**，回合成功结束后调用（`lib.rs:441`、`lib.rs:488`、`lib.rs:918`）。
- `goal_tool::run`（`goal_tool.rs:11`）—— 模型侧 `goal` 工具的执行体，经 `tools/mod.rs:901` 分发。
- token 记账：`add_provider_usage` / `add_estimated_tokens`（`goal.rs:445`、`goal.rs:457`），由 `runner.rs` 调用。
- 上下文注入：`build_goal_context_block`（`goal.rs:513`），由 `prompt.rs:88` 拼进每次请求的 system prompt。

## 3. 核心数据流与算法

### 3.1 设置目标：slash 命令与预算解析

`/goal <objective>` 进入 `handle_slash` 的兜底分支（`goal.rs:148-166`）：

1. 字符数超过 `MAX_OBJECTIVE_CHARS` 直接报错，建议把细节写进文件再用短目标引用。
2. `parse_objective_and_budget`（`goal.rs:725`）从原始文本中拆出 token 预算并剥离预算片段，得到纯目标文本。
3. 空目标报错。
4. `set_goal`（`goal.rs:299`）把目标写进当前 active session 的 `goal` 字段，状态重置为 `Active`，所有计数器归零。
5. `increment_turns`（`goal.rs:436`）把 `turns_executed` 置 1（设置目标本身算作第一个回合）。
6. 返回 `GoalSlashOutcome::Query`，`lib.rs` 据此立即发起一次带目标 overlay 的真实回合，回合结束后再触发 `drive_after_turn` 续跑。

**预算解析 `parse_token_budget`（`goal.rs:731-751`）** 依次尝试三个正则（不区分大小写）：

| 正则 | 匹配示例 | 说明 |
|---|---|---|
| `^\s*\+(\d+(?:\.\d+)?)\s*([kmb])\b` | `+500k do it` | 行首 `+数值单位` |
| `\s\+(\d+(?:\.\d+)?)\s*([kmb])\s*[.!?]?\s*$` | `do it +2.5M` | 行尾 `+数值单位`（需前导空白） |
| `\b(?:use\|spend)\s+(\d+(?:\.\d+)?)\s*([kmb])\s*tokens?\b` | `use 2m tokens` / `spend 3.5m tokens` | 自然语言写法 |

单位倍率：`k`=1e3、`m`=1e6、`b`=1e9（`goal.rs:741-746`），结果 `(value*multiplier).round() as usize`。因此 `+2.5M` → 2_500_000。注意第二个正则要求 `+` 前有空白，故裸 `500k`（无 `+`、非 `use/spend` 句式）解析为 `None`（见测试 `goal.rs:779`）。`strip_token_budget`（`goal.rs:753`）用对应的三个正则把预算片段替换为空格，剩余即目标文本。

### 3.2 状态机全貌

```
                 /goal <objective>            (set_goal)
   (无目标) ───────────────────────────────▶ Active
                                                │
            drive_after_turn 每回合推进         │
   ┌────────────────────────────────────────────┤
   │                                            │
   │  turns_executed >= 150                      │ tokens_used >= budget
   │  (mark_max_turns)                           │ (add_tokens 内联)
   ▼                                            ▼
 MaxTurns                                   BudgetLimited
   │  /goal continue                            │  drive 注入一次预算总结
   │  (continue_from_max_turns,turns归零)         │  然后停住等用户
   └──────────────▶ Active ◀────────────────────┘

 Active ──/goal pause──▶ Paused ──/goal resume──▶ Active
 Active ──goal工具 status=blocked × 同因3次──▶ Blocked
 Active/任意 ──/goal complete 或 goal工具 status=complete──▶ Complete
```

各转换的真实实现：

- `pause_goal`（`goal.rs:341`）：仅 `Active` 可暂停；把当前活跃片段累加进 `accumulated_active_ms`，记录 `paused_at`，停止计时。
- `resume_goal`（`goal.rs:358`）：仅 `Paused` 可恢复；重置 `start_time`，并**清零 `blocked_attempts` 与 `last_block_reason`**（恢复视为重新尝试）。
- `continue_from_max_turns`（`goal.rs:375`）：仅 `MaxTurns` 可续；**把 `turns_executed` 归零**并回到 `Active`，同时清空阻塞计数。这是 `/goal continue` 的语义。
- `complete_goal`（`goal.rs:393`）：任意状态可完成，结账活跃时间后置 `Complete`。
- `mark_max_turns`（`goal.rs:648`）、`mark_budget_notified`（`goal.rs:660`）为内部辅助。

所有写操作都经 `mutate_active_goal`（`goal.rs:669`）在 `state.sessions` 锁内完成，并刷新 `session.updated_at`，保证一致性。

> 关于 `/goal resume` 与 `MaxTurns`：`handle_slash` 的 `resume` 分支（`goal.rs:99-122`）会先判断目标是否处于 `MaxTurns`，若是则提示改用 `/goal continue` 重置计数器，而不会错误地走 `resume_goal`（后者只接受 `Paused`）。

### 3.3 续跑调度 `drive_after_turn`

`drive_after_turn`（`goal.rs:170-261`）是一个 `loop`，在**一次命令调用内部同步循环**，每轮做：

1. 若 `state.cancel` 被置位（用户取消）→ 立即返回 `Ok(())`。
2. 取当前 active goal，无目标→返回。
3. 按 `status` 分派：
   - **`Active`**：
     - 若 `turns_executed >= MAX_GOAL_TURNS`(150)：`mark_max_turns`，发出一条 `assistant_done` 文案提示用 `/goal continue` 重置，返回。
     - 否则 `increment_turns` 得到本轮号 `turns`，用 `build_continuation_prompt` 生成续跑 overlay，发 `goal-progress` 事件（含 status / message / turns / tokens / budget），然后以**内部用户文本 `[Goal continuation #N]`** 调用 `run_turn_with_options`。循环继续（即同一命令内可连续推进多个续跑回合，直到状态不再是 `Active` 或命中护栏）。
   - **`BudgetLimited`**：
     - 若 `budget_limit_notified` 已为真→直接返回（不再骚扰）。
     - 否则 `mark_budget_notified`，发 `goal-progress`，用 `build_budget_limit_prompt` 注入**一次性预算总结提示**（要求模型停止实质工作、给出已完成/待办/阻塞总结），调用一次 `run_turn_with_options` 后 `return Ok(())`（不再循环）。
   - **其它状态**（`Paused`/`Blocked`/`MaxTurns`/`Complete`/`UsageLimited`）：`_ => return Ok(())`，不续跑。

```
run_turn_with_options(普通回合)  ──成功且未取消──▶ drive_after_turn
                                                       │loop
                              ┌────────────────────────┤
                              │ status==Active & turns<150
                              ▼
            increment_turns → emit goal-progress
                              → run_turn_with_options("[Goal continuation #N]", overlay)
                              └──────────────▲────────┘ (回到 loop 顶)
```

注意续跑的 `TurnOptions.token_budget` 传 `None`（`goal.rs:221`），即续跑不施加"单回合硬预算"；预算护栏只通过 `tokens_used` 与 `token_budget` 的软比较实现（§3.4）。

### 3.4 token 记账与预算护栏

记账发生在 `runner.rs` 的回合执行过程中，对象是当前 session 的 goal：

1. **精确优先**：模型回合返回后，`runner.rs:377` 调用 `add_provider_usage`（`goal.rs:445`）。它取 `usage.total_or_sum()`（`src-tauri/src/llm/mod.rs:23`，优先 `total_tokens`，否则 `input+output`），若 provider 给出 usage 即按精确值累加，并返回 `true`（`exact_usage_recorded`）。
2. **估算兜底**：仅当 provider 未返回 usage（`exact_usage_recorded==false`）时，在最终答复分支用 `add_estimated_tokens`（`goal.rs:457`）补记用户输入与助手输出（`runner.rs:400-403`）；工具调用路径则对 `tc.function.arguments` 与截断后的工具结果做估算补记（`runner.rs:583-584`）。
3. **估算函数** `budget::estimate_text_tokens`（`budget.rs:77-89`）：`ascii.div_ceil(4) + non_ascii.max(1)`，即 ASCII 约 4 字符/token、非 ASCII（如中文）约 1 字符/token，是粗粒度启发式而非真实分词。

累加发生在 `add_tokens`（`goal.rs:465-488`），关键约束：

- 只有 `goal.status == GoalStatus::Active` 时才累加（`goal.rs:476`），暂停/已完成等状态不计费。
- 累加后立即软比较：若 `tokens_used >= token_budget` 则状态切到 `BudgetLimited`（`goal.rs:481-487`）。这是**唯一**进入 `BudgetLimited` 的路径。

因此预算是"事后软触发"：模型仍会把触发那一刻的回合跑完，下次 `drive_after_turn` 才发现状态已是 `BudgetLimited` 并注入一次性总结提示。预算**不是硬中断**，也不依赖任何 provider 侧限流。

### 3.5 连续 3 次同因才 Blocked

模型通过 `goal` 工具上报阻塞时（`status="blocked"`），进入 `record_blocked_attempt`（`goal.rs:408-434`）：

1. 仅 `Active` 状态接受阻塞上报，否则返回 `None`（工具层据此报错"Goal is not in a state that accepts blocked attempts."）。
2. 把 reason 归一化（trim + 小写）。若与 `last_block_reason` 归一化后**不同**，则**重置 `blocked_attempts = 0`**（换了阻塞原因，重新计数）。
3. 记录新 reason，`blocked_attempts += 1`。
4. 当 `blocked_attempts >= BLOCKED_CONSECUTIVE_THRESHOLD`(3) 时才真正置 `Blocked`。
5. 返回 `(当前状态, attempts)`。

工具层（`goal_tool.rs:62-83`）据返回值给模型不同反馈：达到阈值则"已标记 blocked"，否则"已记录第 N 次，需连续 3 次同因"。

设计动机：避免模型遇到一次困难就轻易放弃；只有**反复、同因**的阻塞才被视为真正卡死。任何一次 `resume_goal` / `continue_from_max_turns` 或更换阻塞原因都会清零该计数。

### 3.6 续跑 overlay 与目标上下文注入

两条注入路径并存：

- **每回合常驻上下文**：`prompt.rs:88` 调用 `build_goal_context_block`（`goal.rs:513`），把 `<active-goal status=… elapsed=… tokens=… budget=… turns=…>目标文本</active-goal>` 拼进 system prompt。无目标时返回空串。
- **续跑专用 overlay**：`build_continuation_prompt`（`goal.rs:533`）生成 `<goal-steering type="continuation">`，包含目标、活跃时间、token 用量、续跑回合数，以及两段强约束指令——**Completion Audit**（要求基于权威证据严格证明完成、不得擅自缩小目标范围）与 **Blocked Audit**（重申"首次遇阻不得标记 blocked，需同因连续 3 回合"）。该 overlay 经 `TurnOptions.system_overlay` 通过 `apply_system_overlay`（`runner.rs:247`）叠加到 system prompt。

另有 `build_budget_limit_prompt`（`goal.rs:586`，预算总结提示）与 `build_objective_updated_prompt`（`goal.rs:615`，目标被替换时提示）。

### 3.7 `[Goal continuation #N]` 不入前端历史

续跑回合通过 `run_turn_with_options` 写入 session 时，`stored_user_text` 被设为 `[Goal continuation #N]`（`goal.rs:211-218`）；其它内部回合用 `[Goal resumed]`、`[Goal continued]`、`[Goal budget limit]`。这些文本会作为 user 消息**真实写入 session 历史**（`runner.rs:225`），但前端做两层屏蔽：

- **实时态**：前端只在用户主动发送时乐观插入 user 项（`App.tsx:469`）；续跑由后端驱动、不经前端发送路径，因此实时不会出现 user 气泡。
- **重建态**：刷新/切换会话时 `buildHistory`（`App.tsx:107-143`）重建历史，对 user 消息显式过滤 `text.startsWith("[Goal ")`（`App.tsx:118`），凡以 `[Goal ` 开头的内部消息一律不渲染为 user 气泡。

净效果：用户只看到模型连续产出的助手消息与工具调用，看不到驱动这些产出的内部续跑提示。注意被过滤的仅是 user 角色的内部触发消息，模型的回复仍正常显示。

## 4. 与其他模块的交互边界

| 边界 | 方向 | 说明 |
|---|---|---|
| `lib.rs::send` / `send_with_agents` | 调用 goal | slash 分发到 `handle_slash`；回合成功且未取消时调用 `drive_after_turn`（`lib.rs:441/488`）。`/dream`、`/compact`、`/ultracode`、`/workflows`、普通消息成功后都会触发续跑检查（置 `should_drive_goal=true`）。 |
| `lib.rs::goal_pause/goal_resume/goal_continue/goal_clear` | 命令→goal | 面板按钮对应的 Tauri 命令；`goal_resume`/`goal_continue` 经 `run_goal_control_turn`（`lib.rs:897`）发起一次内部回合再续跑。这两个命令用 `state.busy` 互斥防并发。 |
| `runner.rs` | 回合→goal | 回合内做 token 记账（`add_provider_usage`/`add_estimated_tokens`）。 |
| `prompt.rs:88` | prompt→goal | 每次请求注入 `<active-goal>` 上下文块。 |
| `tools/mod.rs` | 工具注册/分发 | `goal` 工具定义于 `tools/mod.rs:555-581`，分发于 `tools/mod.rs:901`。 |
| `store`(Session) | 持久化 | `GoalState` 存于 `Session.goal`，随 `persist_sessions` 落盘；按会话隔离。 |
| 前端 `App.tsx` / `GoalBar` | goal→UI | 消费 `goal-progress` 事件与 `GoalPanelState`，并过滤 `[Goal ` 内部消息（`App.tsx:118/875`）。 |

## 5. 安全与权限相关点

- **模型工具能力被严格限制**。`goal` 工具（`goal_tool.rs`）只暴露两个 action：`get`（只读快照）与 `update`（仅 `complete` / `blocked`）。`snapshot`（`goal_tool.rs:88`）返回目标的只读字段。工具定义的权限声明为 `PermissionPolicy::allow("只更新当前会话的 goal 状态，不访问外部资源。")`（`tools/mod.rs:560`），`risk` 为 `Mutating` 但 `ParallelSafe`。
- **模型不能**：创建目标、替换目标文本、设置/修改预算、暂停、恢复、清除、从 `MaxTurns` 续跑。这些状态转换全部保留给用户的 slash 命令（`handle_slash`）或面板命令（`goal_pause` 等）。也就是说"目标的生命周期归用户，目标的进度推进/收尾归模型"。
- **action 推断**：`goal_tool.rs:17-25` 在缺省 `action` 时做兜底——有 `status` 则当 `update`，否则当 `get`，降低模型误用门槛；但非 `get`/`update` 的 action 直接报错（`goal_tool.rs:38`）。
- **blocked 防滥用**：连续 3 次同因阈值（§3.5）使模型无法因单次困难就让目标停摆。
- **预算与回合护栏**：`token_budget`（软触发 `BudgetLimited`）与 `MAX_GOAL_TURNS=150`（硬触发 `MaxTurns`）共同防止失控的无限续跑。`drive_after_turn` 每轮检查 `state.cancel`，用户随时可中断。
- **目标长度护栏**：`MAX_OBJECTIVE_CHARS=4000` 防止超长目标污染上下文。

## 6. 已知限制与扩展点

- **`UsageLimited` 状态为预留未接通**。枚举（`goal.rs:20`）与 `status_label`/`status_value`（`goal.rs:707/719`）都有它，但代码中**没有任何路径会把状态置为 `UsageLimited`**（`add_tokens` 只会置 `BudgetLimited`）。它显然是为"provider 侧用量限流（如 5 小时配额）"预留的状态，目前不会触发，前端的 `can_continue` 也只对 `MaxTurns` 为真（`goal.rs:294`），并未覆盖 `UsageLimited`。
- **token 记账是启发式估算 + 精确优先的混合**。当 provider 返回 usage 时用精确值；否则退化为 `estimate_text_tokens` 的粗粒度估算（ASCII÷4、非 ASCII×1），与真实分词存在偏差。预算是"软触发"，不会硬性中断正在进行的回合。
- **预算耗尽后只提醒一次**。`budget_limit_notified` 保证 `BudgetLimited` 状态只注入一次总结提示，之后 `drive_after_turn` 在该状态直接返回，需用户介入（`/goal continue` 不接受 `BudgetLimited`，实际只能重新 `/goal <objective>` 或 `/goal complete`/`clear`）。
- **从 `BudgetLimited` 没有"重置预算继续"的命令**。`continue_from_max_turns` 只接受 `MaxTurns`；要在预算耗尽后继续，当前只能重新设置目标（会清零 `tokens_used`）。这是一个潜在扩展点。
- **替换已有 active goal 无 UI 二次确认**：`set_goal` 直接覆盖。
- **续跑在单次命令调用内同步循环**。`drive_after_turn` 的 `loop` 会在一次 `send` 调用里连续跑多个续跑回合，期间该命令不返回；好处是对前端透明，代价是长目标会让单次命令耗时很长（受 150 回合上限兜底）。

## 7. 顺手发现：现有文档与代码不符

详见结构化输出的 `existingDocIssues`。核心是旧文档 `docs/goal-continuous-driving.md` 第 11、38 行的描述与当前代码不一致（`/goal continue` 的可用前置状态、前端状态栏的实现情况）。
