# Workflow JSON DSL 运行时与持久化恢复

> 存档级技术原理文档。读者为协作开发者。本文聚焦“为什么这样设计”与“数据如何流动”，所有结论均以真实源码为依据。
>
> 主要源文件：
> - `src-tauri/src/agent/workflow_runtime.rs`（DSL 解析、执行语义、live panel、durable snapshot、启动水合、恢复 overlay）
> - `src-tauri/src/agent/workflow_journal.rs`（`journal.jsonl` 追加写、tail 读取、run 目录约定）
> - `src-tauri/src/tools/worktree.rs`（`worktree_create` 隔离工作区）

---

## 一、模块职责与定位

本子系统是 Demiurge 的“声明式多 Agent 编排引擎”。它把一份 JSON 文件（DSL）解释成一棵步骤树，按既定语义（顺序 / 并发 / 分阶段 / 预算约束）逐步执行，每一步都会同时做两件事：

1. **更新内存中的实时进度**（`WorkflowRunProgress`），并向前端推送 `workflow-updated` 事件，驱动 Workflows live panel；
2. **把不可变事件追加进 `journal.jsonl`**（事件流），并把可变的整体进度写成 `state.json`（durable snapshot）。

这种“事件流 + 快照”的双轨持久化是整个设计的核心理由：进程崩溃或正常退出后，下一次启动可以从磁盘恢复出曾经存在的 run（即使没有 live task 挂在上面），用户还能用 `/workflow resume <run_id>` 把历史上下文回灌给主 Agent 继续推进。

与之配套的 `worktree_create` 工具属于“执行侧的隔离能力”：当某个步骤要做大改动或并行实验时，可在沙盒 Git 仓库下开一个独立 worktree，避免污染主沙盒工作区。它本身不属于 workflow 运行时，但常被 Ultracode/workflow 场景调用，故一并归档于此。

DSL 文件位置约定（`workflow_runtime.rs:18`）：

```text
<sandbox>/.demiurge/workflows/<name>.json     # workflow 定义
<sandbox>/.demiurge/workflow-runs/<run_id>/   # 每个 run 的运行期目录
        ├── journal.jsonl                      # 事件流（append-only）
        └── state.json                         # durable snapshot（原子替换）
```

---

## 二、关键类型与入口函数

### 2.1 DSL 数据结构

`WorkflowFile`（`workflow_runtime.rs:85-90`）是顶层定义，含可选 `name`、可选 `description` 和必填 `steps`。

`WorkflowStep`（`workflow_runtime.rs:92-118`）用 serde 内部标签 `#[serde(tag = "type", rename_all = "snake_case")]` 区分六种 step：

| `type` | 字段 | 语义 |
| --- | --- | --- |
| `log` | `message` | 追加一条 panel/journal 日志 |
| `phase` | `name`, `steps` | 命名阶段，包裹一组子步骤 |
| `agent` | `prompt`, `label?`, `agent_type?`, `agent?`, `context_mode?` | 跑一个只读子 Agent |
| `parallel` | `items` | 并发执行子步骤（上限 8） |
| `pipeline` | `items` | 顺序执行子步骤 |
| `budget` | `total?` | 设定本 run 的 token 预算（`None` = 无限制） |

> 说明：`agent` step 有两个看似重复的字段 `agent_type` 与 `agent`。它们在 `run_agent_step` 里分别填入 `SubagentRequest.agent_type` 与 `SubagentRequest.agent_name`（`workflow_runtime.rs:463-475`），用途不同——前者是 Agent 类别 / 模板标识，后者是具名 Agent。现有的 `docs/workflow-json-dsl.md` 仅文档化了 `agent_type`，遗漏了 `agent` 字段（见文末“现有文档与代码不符”）。

### 2.2 运行期进度类型

`WorkflowRunProgress`（`workflow_runtime.rs:39-56`）是内存与磁盘共享的进度结构，关键字段：

- `run_id` / `name` / `status`（`WorkflowStatus`）；
- `cancel_requested`：是否已请求取消，用于恢复时区分 Killed vs StaleRunning；
- `current_phase`、`agents: Vec<WorkflowAgentProgress>`、`logs: Vec<String>`；
- `journal_path`：journal 绝对路径；
- `started_at` / `updated_at`（毫秒）；
- `budget: budget::TokenBudgetState`；
- `steps_total` / `steps_done`：进度计数。

`WorkflowStatus`（`workflow_runtime.rs:58-67`）是状态机的核心枚举，序列化为 snake_case：`running` / `stale_running` / `done` / `failed` / `killed` / `journaled`。其中 `StaleRunning` 与 `Journaled` 是“恢复语义”专属状态，见第三节。

`WorkflowRunStateFile`（`workflow_runtime.rs:79-83`）是 snapshot 的磁盘包装，带 `schema_version`（当前常量 `RUN_STATE_SCHEMA_VERSION = 1`，`workflow_runtime.rs:20`）。版本不匹配时直接丢弃该 snapshot（`read_run_state_file`，`workflow_runtime.rs:811-813`）。

### 2.3 入口函数一览

| 函数 | 位置 | 作用 |
| --- | --- | --- |
| `launch` | `workflow_runtime.rs:235` | 加载定义、生成 run_id、登记内存 run、写 `workflow_started` 事件 |
| `run_launched` | `workflow_runtime.rs:281` | 异步驱动整棵 step 树，收尾打 Done/Failed/Killed |
| `run_step` | `workflow_runtime.rs:342` | 递归执行单个 step（返回 `StepFuture`，boxed 递归 future） |
| `stop` | `workflow_runtime.rs:325` | 置取消标志 + 标 Killed + 写 `workflow_killed` |
| `panel_state` | `workflow_runtime.rs:127` | 聚合内存 + snapshot + journal 三来源给前端 |
| `hydrate_persisted_runs` | `workflow_runtime.rs:167` | 启动时把磁盘 snapshot 灌回内存 |
| `resume_overlay` | `workflow_runtime.rs:185` | journal 优先 + snapshot 兜底的恢复提示 |

Tauri command 绑定在 `lib.rs`：`workflow_panel_state`（`lib.rs:1616`）、`workflow_run`（`lib.rs:1621`，`launch` 后 `spawn` 出 `run_launched`）、`workflow_stop`（`lib.rs:1636`）。启动水合在 `setup` 钩子里调用（`lib.rs:1679`）。`/workflow resume <run_id>` 与 `/workflows` 斜杠命令在 `lib.rs:356-413` 处理。

---

## 三、核心数据流与算法

### 3.1 定义加载与名称解析

`load_workflow`（`workflow_runtime.rs:519`）→ `find_workflow_path`（`workflow_runtime.rs:541`）的解析顺序值得注意，它兼顾“按文件名”和“按定义内 name”两种引用方式：

1. 先按请求名与其 `sanitize_name` 结果匹配文件 stem；
2. 再逐个解析 JSON，匹配文件内 `name`（原值或 sanitize 后）；
3. 都不中则回退到 `<sanitize_name(requested)>.json`，由后续 `read_to_string` 报错。

`sanitize_name`（`workflow_runtime.rs:571`）把非 `[A-Za-z0-9_-]` 字符替换成 `-` 并修剪首尾 `-`。这是路径安全的第一道闸：避免 `..`、路径分隔符等注入。

`list_definitions`（`workflow_runtime.rs:204`）扫描 `.demiurge/workflows/*.json`，解析失败的文件被静默跳过（`serde_json::from_str(...).ok()`），不会让面板崩溃。

### 3.2 step 执行语义

`run_step`（`workflow_runtime.rs:342`）是递归异步函数，返回 `StepFuture<'a> = Pin<Box<dyn Future...>>`（`workflow_runtime.rs:24`）——之所以手动 box，是因为 Rust 的 async fn 不能直接自递归。

每次进入 `run_step` 先做协作式取消检查 `is_cancelled`（`workflow_runtime.rs:350`），命中则直接 `Ok(())` 返回（不报错），把“取消”表达成“安静地不再往下做”。各 step 语义：

- **`log`**：`push_log` 进面板 + 写 `log` 事件。
- **`phase`**：`set_phase` 设置 `current_phase` → 写 `phase_started` → 递归子步骤 → 写 `phase_done`。注意 phase 自身不重置 phase，子 step 通过参数 `phase: Option<String>` 继承父阶段名。
- **`agent`**：转交 `run_agent_step`（见 3.3）。
- **`parallel`**（`workflow_runtime.rs:393-408`）：先做并发上限校验 `items.len() > MAX_PARALLEL_ITEMS`（常量 `= 8`，`workflow_runtime.rs:19`），超限直接返回 `Err`。随后把每个 item 映射成 future，用 `futures_util::future::join_all` 并发 await，再逐个 `result?` 传播错误——**任一子步骤失败则整个 parallel 失败**。
- **`pipeline`**（`workflow_runtime.rs:409-413`）：纯 `for` 顺序执行，遇错即 `?` 短路。与 `parallel` 的唯一区别就是不并发。
- **`budget`**（`workflow_runtime.rs:414-429`）：`set_budget` 用 `TokenBudgetState::new(total)` 重置预算 → panel 日志 → 写 `budget` 事件。

每个 step 正常结束后调用 `mark_step_done`（`workflow_runtime.rs:716`）把 `steps_done` 自增（`saturating_add(1).min(steps_total)`，防越界）。

`steps_total` 由 `count_steps`（`workflow_runtime.rs:726`）在 `launch` 阶段一次算出：`log/agent/budget` 各计 1；`phase` 计 `1 + 子树`；`parallel/pipeline` 计 `1 + 子树`。即容器节点本身也占一个计数槽。

并发模型的关键点：

```
parallel(items)         pipeline(items)
   ├ join_all              for item in items
   ├ 全部并发跑               └ 顺序、遇错短路
   └ 逐个 result? 传播错误
```

> 并发上限为何是 8？这是一个保守的资源/限速保护常量（`MAX_PARALLEL_ITEMS`），防止一份 DSL 误声明几十个并行子 Agent 同时打爆下游 LLM 限流。它是硬上限：超限不截断、不排队，而是直接让该 step 失败，迫使作者显式拆分。

### 3.3 agent step 与预算约束

`run_agent_step`（`workflow_runtime.rs:436`）的执行链：

1. `next_agent_id`（`workflow_runtime.rs:596`）按现有 agents 的 `max(id)+1` 分配自增 id（不依赖全局计数器，恢复后仍单调）。
2. **预算前置闸**（`workflow_runtime.rs:448-453`）：若 `workflow_budget(...).is_exhausted()`，写 `token_budget_exhausted` 事件并 `Err` 中止——**这是真实存在的硬约束**，并非纯标记。
3. `push_agent` 登记一个 `Running` 的 `WorkflowAgentProgress`，写 `agent_started`（payload 含 `agent_id/label/phase/prompt/agent`）。
4. 解析 `context_mode`：`SubagentContextMode::parse`（`subagent.rs:314`）把字符串映射为 `Fork`(`fork`/`full`) / `Recent`(`recent`) / 其余一律 `Brief`。
5. 构造 `SubagentRequest`（`workflow_runtime.rs:464-474`），其中 `max_total_tokens = workflow_budget(...).remaining()` —— **把 run 级剩余预算下传给子 Agent**，`reviewer_count` 固定 1，`output_format` 固定 `Plain`。调用 `subagent::run`（`subagent.rs:324`）。
6. 成功：`record_budget_estimate` 把结果文本估算的 token 计入预算（见下），`update_agent` 标 `Done` 并把结果裁到 1200 字符（`cap_chars`，`workflow_runtime.rs:861`），写 `agent_done`。
7. 失败：`update_agent` 标 `Failed`，写 `agent_failed`，向上 `Err`。

**预算记账**（`budget.rs`）：`TokenBudgetState`（`budget.rs:24-76`）三元组 `total / used_exact / used_estimated`。workflow 路径走的是“估算”侧——`record_budget_estimate`（`workflow_runtime.rs:675`）仅在 `budget.total.is_some()` 时用 `estimate_text_tokens`（`budget.rs:77-89`，ASCII 按 ⌈n/4⌉、非 ASCII 每字符≈1 token 的启发式）累加 `used_estimated`，并写 `token_budget_used` 事件。`is_exhausted`（`budget.rs:49`）判据是 `used_total() >= total`。

> 历史状态澄清：旧文档 `docs/workflow-json-dsl.md:58` 写“budget hard enforcement is reserved for a later pass（硬约束留待后续）”。**该说法已过时**：当前代码在每个 agent step 前做 `is_exhausted` 拦截，并把剩余额度作为 `max_total_tokens` 下传，已构成有效硬约束。需注意其约束粒度是“step 之间”而非“token 流式实时”，且基于启发式估算（非 provider 精确 usage）——估算偏差可能让实际消耗略微越线。

### 3.4 live panel：`workflow-updated` 推送

几乎所有改变 run 状态的辅助函数（`push_log` / `push_agent` / `update_agent` / `set_phase` / `set_budget` / `mark_step_done` / `record_budget_estimate`）末尾都调用 `emit_update`（`workflow_runtime.rs:856`）：

```rust
fn emit_update(app, state) {
    persist_all_run_snapshots(state);          // 先落盘所有 run 的 state.json
    app.emit("workflow-updated", panel_state(state));  // 再推送完整面板
}
```

设计取舍：**每次状态变更都全量落盘 + 全量推送**。优点是恢复点极密、前端逻辑简单（拿到的总是完整 `WorkflowPanelState`）；代价是写放大——高频步骤会反复重写 `state.json`。这在桌面单用户场景下可接受，但属于已知的可优化点（第六节）。

`panel_state`（`workflow_runtime.rs:127`）的三来源合并顺序（用 run_id 去重，先到先得）：

1. 内存 `workflow_runs`（活跃 + 已水合）；
2. 磁盘 snapshot（`list_persisted_run_states`，补内存里没有的）；
3. journal 目录（`workflow_journal::list`，仅有 journal 没 snapshot 的，状态标 `Journaled`，name 填 `"journal"`）。

最后按 `updated_at` 倒序。这保证“只要磁盘上留过痕迹的 run，面板里就能看到”。

日志环形裁剪：`push_log` 把 `logs` 截到最多 80 条（`workflow_runtime.rs:745-748`），超出从头 drain，防止长 run 无限膨胀。

### 3.5 journal：append-only 事件流

`workflow_journal::append`（`workflow_journal.rs:23`）→ `append_in_root`（`workflow_journal.rs:33`）每条事件写一行 JSON：

```json
{"ts": <millis>, "run_id": "...", "event": "...", "payload": {...}}
```

用 `OpenOptions::create(true).append(true)` 打开，保证并发追加不互相覆盖（这也是 `parallel` 多个子 Agent 同时写同一 journal 仍安全的原因——OS 层 append 语义）。所有 `append` 调用点都用 `let _ = ...` 忽略写失败，体现“journal 是尽力而为的辅助轨，不应阻塞主流程”的设计意图。

事件类型全集（grep 自 `workflow_runtime.rs`）：`workflow_started`、`workflow_done`、`workflow_failed`、`workflow_killed`、`log`、`phase_started`、`phase_done`、`agent_started`、`agent_done`、`agent_failed`、`budget`、`token_budget_used`、`token_budget_exhausted`。

`run_dir`（`workflow_journal.rs:98`）= `root/.demiurge/workflow-runs/<sanitize_run_id(run_id)>`。`sanitize_run_id`（`workflow_journal.rs:102`）同样把非 `[A-Za-z0-9_-]` 替换为 `_`，单测验证 `"wf_1/../x"` → `"wf_1____x"`（`workflow_journal.rs:120`）——这是防路径穿越的第二道闸。

`new_run_id`（`workflow_journal.rs:19`）= `"wf_" + new_session_id() 去掉 "s_" 前缀`，与会话 id 体系同源、保证全局唯一与时间单调。

### 3.6 durable snapshot：state.json 原子写

`write_run_state_in_root`（`workflow_runtime.rs:766`）采用经典的“temp + rename”原子替换：

```
1. 序列化 WorkflowRunStateFile{schema_version, run} → state.json.tmp
2. 若 state.json 已存在则删除
3. rename(state.json.tmp → state.json)
```

`rename` 在同目录内是原子的，因此任何时刻读到的 `state.json` 要么是旧完整版要么是新完整版，不会读到半截。`Journaled` 状态的 run 不写 snapshot（`persist_all_run_snapshots`，`workflow_runtime.rs:759`），因为它本就没有内存进度可落。

> 注意 Windows 上 `fs::rename` 目标已存在会失败，所以这里先 `remove_file` 再 rename（`workflow_runtime.rs:778-781`）——这一步本身不是原子的（删除与重命名之间有窗口），是该实现的细微取舍点。

### 3.7 状态机与恢复语义

整个 run 的状态流转：

```
            launch
              │
              ▼
          [Running] ──stop()──► [Killed]      (cancel_requested=true, 写 workflow_killed)
              │
   run_launched 收尾：
     ├ 正常结束 ─────► [Done]   (写 workflow_done)
     ├ 中途取消 ─────► [Killed] (run_launched 内检测到 is_cancelled)
     └ 返回 Err ─────► [Failed] (写 workflow_failed)

  ── 进程退出后再启动 ──（normalize_restored_run）
     [Running] + cancel_requested ─► [Killed]        ("...stopping when Demiurge exited.")
     [Running] + 无 cancel        ─► [StaleRunning]  ("...running...; no live task is attached.")
```

`normalize_restored_run`（`workflow_runtime.rs:820`）是恢复语义的关键：从磁盘读回的 run 若仍是 `Running`，说明进程在它跑到一半时退出了，但内存里早已没有驱动它的 task。于是按 `cancel_requested` 把它“修正”为 `Killed` 或 `StaleRunning`，并补一段说明性 error。`StaleRunning` 因此专指“曾经在跑、现已无 live task 挂载”的孤儿 run——这正是 `/workflow resume` 的典型目标。

**启动水合** `hydrate_persisted_runs`（`workflow_runtime.rs:167`）在 `lib.rs:1679` 的 setup 钩子里调用：读全部 snapshot（经 normalize），按 run_id 去重塞回内存 `workflow_runs`，再按 `updated_at` 倒序。这样应用一启动，Workflows 面板就能显示上次遗留的 run。

### 3.8 `/workflow resume`：journal 优先 + snapshot 兜底

`workflow_runtime::resume_overlay`（`workflow_runtime.rs:185`）的两段式回退是本模块最体现设计意图的地方：

```
resume_overlay(run_id):
  ┌─ 优先：workflow_journal::resume_overlay  ── 读 journal.jsonl 尾部 40 行
  │       成功 → 返回含 ```jsonl tail``` 的 overlay
  └─ 失败（无 journal 等）：
          读 state.json snapshot
            有 → 返回含 ```json snapshot``` 的 overlay（提示“没有 journal tail，但有 durable snapshot”）
            无 → 把 journal 的原始错误透传出去
```

journal tail（`workflow_journal.rs:78-96`）取最后 40 行（`lines().rev().take(40)` 再翻回正序），因为“最近发生的事”对“接着干”最有价值，而完整 journal 可能很长。两条路径生成的 overlay 文案都引导主 Agent：先复盘“已完成 / 未完成 / 下一步”，**不要重复已完成的安全操作**——这是为了让恢复后的执行幂等友好。

overlay 文本以 `system_overlay` 形式注入 `TurnOptions`（`lib.rs:401-412`），并带上 `workflow_run_id`，使续跑的 turn 仍归属同一 run、继续往同一 journal 写事件。

> 命名兼容说明：overlay 文案中出现的 "Ultracode workflow run" 字样（`workflow_runtime.rs:196`、`workflow_journal.rs:92`）是本项目内部对“多 Agent 编排会话”的称呼，与外部产品无关。

### 3.9 worktree_create：隔离工作区

`worktree::create`（`worktree.rs:12`）是供 Agent 在“需要隔离大改动/实验分支”时调用的工具（`ultracode.rs:20` 的系统提示里建议先用它）。流程：

1. `sanitize_label`（`worktree.rs:58`，规则同前）清洗 label，空则报错；
2. 在 `<sandbox>/.demiurge/worktrees/<label>` 建目录，若已存在则报错（不覆盖）；
3. 分支名默认 `demiurge/<label>`，可由 `branch` 参数覆盖；
4. 执行 `git worktree add -b <branch> <path>`，`current_dir` 设为沙盒根；失败把 stderr 透传；
5. 成功返回 JSON：`worktree_path` / `branch` / 一段 notice，提示“这是独立 worktree，子任务操作前应重新读取文件，路径与主沙盒不同”。

工具注册见 `tools/mod.rs:614-629`：`risk=Mutating`、`concurrency=SerialOnly`、`permission=ask(...)`（执行前需用户确认）、`output_policy=Inline`。`preview`（`worktree.rs:49`）提供执行前的 dry-run 文案。

> 设计意图：worktree 让并行/实验性 Agent 改动落在独立分支与独立工作目录，既不污染主沙盒，也便于事后 review/丢弃。它与 workflow 的 `parallel` 配合，是“隔离并发实现”的物理基础。

---

## 四、与其他模块的交互边界

| 边界 | 接口 | 说明 |
| --- | --- | --- |
| 子 Agent 执行 | `subagent::run(state, SubagentRequest)`（`subagent.rs:324`） | agent step 的实际算力来源；workflow 只传 prompt/label/agent_type/agent_name/context_mode/remaining 预算 |
| 预算 | `budget::TokenBudgetState` / `estimate_text_tokens`（`budget.rs`） | run 级预算的数据结构与估算函数；workflow 走估算侧 |
| 持久化路径 | `workflow_journal::run_dir / JOURNAL_DIR`（`workflow_journal.rs`） | snapshot 与 journal 共用同一 run 目录 |
| 会话续跑 | `agent::run_turn_with_options` + `TurnOptions`（`lib.rs:401`） | `/workflow resume` 把 overlay 与 run_id 注入续跑 turn |
| 应用状态 | `crate::AppState`（`lib.rs:50`：`workflow_runs`、`workflow_cancels`、`sandbox_dir`） | 内存 run 列表与取消标志位表 |
| 前端 | Tauri event `workflow-updated` + command `workflow_panel_state/run/stop` | live panel 数据通道 |
| 工具系统 | `tools::worktree`（`tools/mod.rs:614/904/1061`） | `worktree_create` 注册、执行、preview |

取消机制细节：`AppState.workflow_cancels: Mutex<HashMap<run_id, Arc<AtomicBool>>>`。`launch` 时插入 flag（`workflow_runtime.rs:266-270`），`stop` 置位，`is_cancelled`（`workflow_runtime.rs:586`）在每个 step 入口轮询，`run_launched` 收尾移除 flag（`workflow_runtime.rs:322`）。这是协作式（cooperative）取消：正在 await 的子 Agent 不会被强杀，只是不再启动后续步骤。

---

## 五、安全与权限相关点

1. **路径穿越防护**：`sanitize_name`（workflow 名）、`sanitize_run_id`（run 目录）、`sanitize_label`（worktree label）三处统一白名单清洗 `[A-Za-z0-9_-]`，杜绝 `..`、分隔符注入。单测 `sanitizes_run_ids_for_paths` 明确覆盖 `wf_1/../x`。
2. **沙盒边界**：所有读写都基于 `state.sandbox_dir`，definitions/runs/worktrees 全在 `.demiurge/` 下，不逸出沙盒。
3. **agent step 只读定位**：`run_agent_step` 走 `subagent::run`，原文档将其描述为“read-only subagent”；实际写权限取决于子 Agent 的工具权限策略（本模块不额外授权）。
4. **worktree_create 需用户确认**：`PermissionPolicy::ask`（`tools/mod.rs:619`），因为它会真实执行 `git worktree add` 改动仓库引用。
5. **预算闸**：`is_exhausted` 前置拦截避免失控的 token 消耗，但为启发式估算，非精确硬隔离（见 3.3）。
6. **写失败不致命**：journal/snapshot 写错误一律被吞（`let _`），保证持久化故障不会中断或污染主执行流——代价是可能静默丢事件。

---

## 六、已知限制与扩展点

- **预算约束粒度**：仅在 agent step 之间检查，基于文本估算（`estimate_text_tokens`），不接 provider 真实 usage，存在轻微越界可能。可扩展为接入 `TokenBudgetState::record_usage_or_estimate`（`budget.rs:63`）的精确路径。
- **写放大**：`emit_update` 每次状态变更全量重写所有 run 的 `state.json` 并全量推送面板。高频长 run 下可优化为按 run 增量写 / 防抖推送。
- **snapshot 原子替换的删除窗口**：`remove_file` 与 `rename` 之间存在非原子窗口（`workflow_runtime.rs:778-781`），极端情况下崩溃可能留下无 `state.json`（仍可靠 journal 恢复）。
- **`parallel` 失败语义粗粒度**：任一子步骤失败即整体失败，且 `join_all` 会等全部完成再传播错误，不支持“快速失败取消其余”。
- **DSL 无变量/数据传递**：step 之间不能显式传递上游 Agent 输出，只能靠 prompt 文本约定与 phase 划分；这是当前 DSL 的表达力上限。
- **`agent` 字段文档缺口**：见下节。

---

## 附：分析中发现的现有文档与代码不符

1. `docs/workflow-json-dsl.md:58` 称 budget “Hard enforcement is reserved for a later pass”，但 `workflow_runtime.rs:448-453` 已实现 `is_exhausted` 前置拦截并把 `remaining()` 下传作 `max_total_tokens`，硬约束实际已生效（仅为估算粒度）。
2. `docs/workflow-json-dsl.md:55` 描述 `agent` step 字段时只列出 `prompt/label/agent_type/context_mode`，遗漏了 `WorkflowStep::Agent` 中真实存在的 `agent`（→ `SubagentRequest.agent_name`）字段（`workflow_runtime.rs:106`、`workflow_runtime.rs:467`）。
