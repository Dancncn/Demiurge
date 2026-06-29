# 多 Agent 编排：子 Agent、Ultracode 与自定义 Agent

> 存档级技术原理文档。读者：协作开发者。
> 覆盖源文件：
> - `src-tauri/src/agent/subagent.rs`（只读子 Agent 运行时）
> - `src-tauri/src/tools/agent_spawn.rs`（`agent_spawn` 工具参数解析层）
> - `src-tauri/src/agent/ultracode.rs`（`/ultracode` 临时 overlay）
> - `src-tauri/src/agent/custom.rs`（`.demiurge/agents/*.json` 自定义 Agent / team）
> 相邻协作：`src-tauri/src/tools/mod.rs`（工具注册表与只读执行分支）、`src-tauri/src/agent/runner.rs`（主 loop 消费自定义 Agent）、`src-tauri/src/agent/budget.rs`（token 预算原语）、`src-tauri/src/lib.rs`（`/ultracode` 入口）。

---

## 1. 模块职责与定位

这一子系统实现 Demiurge 的「主 Agent + 只读子 Agent」编排范式，核心约束贯穿全部代码：

- **主 Agent 是唯一可以改文件、跑 shell、做最终回复的角色**（`ultracode.rs:12` 明确写入 overlay）。
- **子 Agent 只读**：只能用收敛后的只读工具收集证据，最终输出回传给主 Agent，而非直接给用户（`subagent.rs:387-389`）。

它由四块协作组成：

| 子模块 | 职责 | 关键入口 |
| --- | --- | --- |
| `agent_spawn.rs` | 把 LLM 工具调用参数（`Value`）反序列化成 `SubagentRequest`，做 `reviewer_count` 钳制 | `run()` |
| `subagent.rs` | 只读子 Agent 的完整运行时：上下文构建、工具收敛、预算、evidence packet 校验、多评审 + judge 合成 | `run()` / `run_reviewer_panel()` |
| `ultracode.rs` | `/ultracode` 的临时 system overlay 文本，不进入长期历史 | `overlay()` |
| `custom.rs` | `.demiurge/agents/*.json` 模板/团队的发现、校验、合并与运行时统计 | `resolve_selected()` / `load_agent()` / `validate_raw()` |

需要区分两条不同的编排路径，它们共享自定义 Agent 定义，但运行方式完全不同：

1. **子 Agent 路径**（`agent_spawn` 工具）：主 Agent 在一个 turn 内调用 `agent_spawn`，派生一个**独立的只读 LLM 子循环**（`subagent::run`），子 Agent 自己持有 6 步上限的工具循环。自定义 Agent 在这里被当作**模板**（`agent_name`/`agent_type`）注入子 Agent 的 prompt 与工具白名单。
2. **主 turn 选定 Agent 路径**（`resolve_selected`）：用户在 UI 选择若干 Agent 模板/团队，`runner.rs` 在**主 Agent 自己的 turn** 上叠加它们的 prompt overlay、收敛主工具集、收紧预算。这里没有派生子进程，只是约束主 loop。

两条路径都消费同一份 `.demiurge/agents/*.json`，理解差异是读懂本模块的关键。

---

## 2. 关键类型与入口函数

### 2.1 `SubagentRequest`（subagent.rs:28-38）

```rust
pub struct SubagentRequest {
    pub prompt: String,
    pub label: Option<String>,
    pub agent_type: Option<String>,     // 软类型，如 Explorer/Reviewer，也尝试匹配模板
    pub agent_name: Option<String>,     // 精确匹配 .demiurge/agents/*.json，优先级更高
    pub context_mode: SubagentContextMode,
    pub max_total_tokens: Option<usize>,
    pub output_format: SubagentOutputFormat,
    pub reviewer_count: usize,
}
```

两个枚举携带子 Agent 的核心行为开关：

- `SubagentContextMode`（subagent.rs:60-65）：`Brief` / `Recent` / `Fork`。`parse()`（subagent.rs:314-322）做别名归一：`"fork"|"full"→Fork`、`"recent"→Recent`、其它（含 `None`）→`Brief`。注意 `"full"` 是 `Fork` 的别名。
- `SubagentOutputFormat`（subagent.rs:40-58）：`Plain` / `EvidencePacket`。`parse()` 接受 `"evidence"|"evidence_packet"|"structured"|"json"→EvidencePacket`、空/`"plain"|"text"→Plain`，其它返回 `Err`。

### 2.2 `agent_spawn.rs` 解析层

`agent_spawn::run`（agent_spawn.rs:18-36）是 `agent_spawn` 工具的执行分支（在 `tools/mod.rs:898` 注册）。它只做三件事：

1. `serde_json::from_value` 把工具参数解析为内部 `Args`；
2. `reviewer_count = args.reviewer_count.unwrap_or(1).clamp(1, 5)`（agent_spawn.rs:21）——同样的钳制在 `subagent::run` 入口（subagent.rs:329）再做一遍，属于双保险；
3. 构造 `SubagentRequest` 转交 `subagent::run`。

`agent_spawn` 工具本身在注册表里是 `ToolRisk::External` + `SerialOnly` + `PermissionPolicy::ask`（tools/mod.rs:511-531），即每次派生子 Agent 都要用户确认，且不并行。

### 2.3 `AgentFile` / `AgentDefinitionInfo` / `ResolvedAgents`（custom.rs）

- `AgentFile`（custom.rs:35-52）：`.demiurge/agents/*.json` 的直接反序列化形态，字段大量 `#[serde(default)]`，`kind` 默认 `Template`（custom.rs:40, 19-21）。
- `AgentKind`（custom.rs:12-17）：`Template`（单个角色模板）或 `Team`（成员组合），`snake_case` 序列化。
- `AgentDefinitionInfo`（custom.rs:54-67）：校验/裁剪后的运行时形态，额外带 `invalid_tools`（被丢弃的未知工具名）、`path`、`runtime` 统计。
- `ResolvedAgents`（custom.rs:109-118）：`resolve_selected` 的合并产物，含 prompt overlay、并集后的 `allowed_tools`、四个取最小值的预算字段。

---

## 3. 核心数据流与算法

### 3.1 子 Agent 一次运行的整体流程

```
agent_spawn(args)                         主 Agent turn 内的工具调用
   │  解析 + clamp reviewer_count
   ▼
subagent::run(state, req)
   │  reviewer_count > 1 ? ──是──► run_reviewer_panel ──► (见 3.6)
   │  否
   ▼
 加载 settings / 活动会话 / persona / summary
 解析模板：agent_name 优先，回退 agent_type           (subagent.rs:348-352)
 token_budget = req.max_total_tokens
                ?? 模板 budget.max_total_tokens        (subagent.rs:353-360)
 拼装 user prompt（模板块 + 任务 + 只读约束 + 输出契约）
 按 context_mode 构建 (tool_schema, msgs)              (subagent.rs:417-472)
   │
   ▼ 最多 MAX_SUBAGENT_STEPS(=6) 轮：                  (subagent.rs:474)
   ├─ cancel? ──► 返回 "[子 Agent 已被用户中断]"
   ├─ budget.is_exhausted()? ──► 返回 "[已达 token 硬预算]"
   ├─ llm::stream_completion(...)                       (subagent.rs:486)
   ├─ 记账 token（usage 优先，否则估算）                 (subagent.rs:496-500)
   ├─ finish_reason == "interrupted"? ──► 返回当前内容
   ├─ tool_calls 为空 → finalize 输出（plain 直接返回 /
   │                    evidence 做校验，失败则回灌错误继续）(subagent.rs:510-521)
   └─ 否则逐个执行 tool_call：
        ├─ submit_evidence_packet → canonicalize 后终止  (subagent.rs:537-540)
        ├─ READ_ONLY_TOOLS 内 → execute_subagent_readonly
        └─ 不在白名单 → 返回错误字符串（拒绝执行）       (subagent.rs:546-548)
   ▼
 6 轮用尽仍无最终回答 → "子 Agent 达到内部工具轮次上限…"   (subagent.rs:559)
```

### 3.2 只读工具收敛：两份白名单的真实关系（重要）

这里有一个**容易踩坑且与现有文档不符**的细节：代码里存在**两份**只读工具白名单。

1. `subagent.rs:17-26` 的私有 `READ_ONLY_TOOLS`（8 项）：
   `read_file, glob, grep, git_status, system_info, web_fetch, web_search, context_inspect`
2. `tools/mod.rs:154-166` 的公开 `SUBAGENT_READONLY_TOOL_NAMES`（11 项）：
   `read_file, list_dir, glob, grep, git_status, system_info, http_get, web_fetch, web_search, package_scripts, context_inspect`

两者**并不一致**：`SUBAGENT_READONLY_TOOL_NAMES` 比 `READ_ONLY_TOOLS` 多了 `list_dir`、`http_get`、`package_scripts`。它们在运行时承担不同职责：

- **schema 生成**与**调用门禁**都用 `subagent.rs` 的 `READ_ONLY_TOOLS`：
  - `subagent_tool_schema`（subagent.rs:86-101）只为 `readonly_tool_names` 生成 schema，而 `readonly_tool_names` 默认就是 `READ_ONLY_TOOLS`（subagent.rs:412-416）。
  - 工具调用分发时 `if READ_ONLY_TOOLS.contains(&name) { execute_subagent_readonly } else { 拒绝 }`（subagent.rs:541-548）。
- `tools/mod.rs` 的 `execute_subagent_readonly`（tools/mod.rs:915-938）是真正的执行函数，它**先用 `SUBAGENT_READONLY_TOOL_NAMES` 做二次门禁**（tools/mod.rs:920-922），再 `match` 到具体 `run`。

**净效果**：由于 `READ_ONLY_TOOLS ⊂ SUBAGENT_READONLY_TOOL_NAMES`，子 Agent 实际能调用的工具被收敛到**较小的 8 项**（`subagent.rs` 的那份），因为 schema 里根本不会出现 `list_dir/http_get/package_scripts`，且分发门禁也只认那 8 项。`SUBAGENT_READONLY_TOOL_NAMES` 里多出的三项目前是**死路**——执行函数支持它们，但子 Agent 永远拿不到它们的 schema、也过不了 `subagent.rs:541` 的门禁。这是两份常量未同步的结果，扩展时应统一来源（见第 6 节）。

模板还可以**进一步收窄**工具集：若 `agent_name`/`agent_type` 命中的模板声明了 `allowed_tools`，会先用 `READ_ONLY_TOOLS` 过滤掉非只读项（subagent.rs:401-411），过滤后非空才取代默认全集（subagent.rs:412-416）。也就是说模板**只能减少、不能扩大**子 Agent 的工具面。

### 3.3 三种 `context_mode` 的上下文构建

`context_mode` 决定子 Agent 看到多少父会话上下文，是本模块的上下文工程核心（subagent.rs:417-472）。

| 模式 | 系统提示 | 父上下文注入方式 | 最近消息保留数 |
| --- | --- | --- | --- |
| `Brief` | `build_for_input` + 只读约束追加文本 | 作为文本块 `## 父会话上下文` 插入 user prompt（`parent_context_block`） | 8（subagent.rs:706） |
| `Recent` | 同 `Brief` | 同 `Brief`，但保留更多最近消息 | 18（subagent.rs:707） |
| `Fork` | `build_for_input`（无额外约束追加文本） | **直接 clone 父 `session.messages`** 拼到消息序列，并先做 `repair_unpaired_tool_calls` | 18（仅在文本摘录处用到，fork 实际用全量消息） |

`Brief`/`Recent` 走 `parent_context_block`（subagent.rs:691-716）：拼 `### 会话摘要` + `### 最近消息摘录`，每条经 `compact_message`（subagent.rs:747-764，单条上限 900 字符，附 `[tool_calls: ...]` 名称），整块再经 `cap_chars` 截到 `MAX_PARENT_CONTEXT_CHARS = 10_000`（subagent.rs:13, 715）。这是**摘要式继承**——父历史以压缩文本形态出现在子 prompt 里。

`Fork` 走完全不同的路径（subagent.rs:418-442）：把父会话的**原始 `Message` 序列**直接 clone 进子消息列表，让子 Agent 在「同一条对话」语境里继续。这是「最大化父上下文继承」的模式，代价是必须修复 tool_call 配对。

### 3.4 Fork 模式的未配对 tool_call placeholder 修复

`Fork` 的危险在于：主 Agent 当前 turn **正是因为调用 `agent_spawn` 才触发子 Agent**，此刻父 `session.messages` 里很可能存在一条 `assistant` 消息带着 `agent_spawn` 这个 tool_call，但对应的 `tool_result` 还没写回（因为子 Agent 还在跑）。把这种「悬空 tool_call」原样发给 provider 会因 tool_call / tool_result 不配对而被拒（典型 400）。

`repair_unpaired_tool_calls`（subagent.rs:718-745）解决这个问题：

```
1. 收集已存在的 tool_result 的 tool_call_id 集合（existing_results）
2. 顺序遍历消息：
   - 对每条带 tool_calls 的消息，找出 id 不在 existing_results 的调用
   - 原消息保留，紧跟其后插入一条 placeholder tool_result：
       Message::tool_result(id, name, FORK_PLACEHOLDER_RESULT)
3. 用修复后的序列替换原序列
```

`FORK_PLACEHOLDER_RESULT = "Fork started - processing in background"`（subagent.rs:14）。这条固定占位文本让每个悬空 tool_call 都获得配对的 result，从而通过 provider 的结构校验，同时语义上告知模型「该分支仍在后台处理」。单测 `repairs_unpaired_tool_calls_with_placeholder`（subagent.rs:880-899）正是用一个未配对的 `agent_spawn` 调用验证它会补出第二条 placeholder 消息。

### 3.5 evidence_packet 输出契约与结构化校验

当 `output_format == EvidencePacket`，子 Agent 的交付不再是自由文本，而是经过 schema 约束 + 后端校验的结构化证据包。

**数据结构**（subagent.rs:67-84）：

```rust
struct EvidencePacket {
    verdict: String,            // 一句话结论
    confidence_score: u8,       // 0-100
    findings: Vec<EvidenceFinding>,   // 至少 1 条
    uncertainties: Vec<String>, // 可空
    next_actions: Vec<String>,  // 可空
}
struct EvidenceFinding { claim, evidence, reasoning, severity }
// severity ∈ {info, low, medium, high, critical}
```

**两条交付通道**：

1. **工具通道（首选）**：当 provider 支持 tools 且为 evidence 模式，`append_submit_evidence_tool`（subagent.rs:103-149）把一个 `submit_evidence_packet` 工具按 provider dialect（OpenAI / Anthropic / Gemini 三种 schema 形态）追加进 tool schema，参数 schema 由 `evidence_packet_parameters`（subagent.rs:151-200）给出（带 `additionalProperties:false`、枚举 severity、`minItems:1` 等约束）。子 Agent 调用该工具即触发 `canonicalize_evidence_value`（subagent.rs:537-540），通过即终止返回。
2. **prose 兜底通道**：若子 Agent 不调工具而直接给文本（`tool_calls` 为空，subagent.rs:510-521），`finalize_subagent_output` 对 evidence 模式调 `canonicalize_evidence_text`，后者用 `extract_json_value`（subagent.rs:227-245）尝试三级解析：①整体当 JSON；②剥 ```json 围栏（`extract_fenced_json`）；③取首个 `{` 到末个 `}` 的子串。

**校验**：`canonicalize_evidence_value`（subagent.rs:218-225）先 `serde_json::from_value` 成 `EvidencePacket`，再 `validate_evidence_packet`（subagent.rs:256-312），通过后输出 `Evidence packet (validated)\n```json\n...\n````。校验项包括：verdict 非空、`confidence_score ≤ 100`、findings 非空且每条四字段非空、severity 合法。

**handoff_format 驱动的额外校验**（subagent.rs:287-305）：模板的 `handoff_format` 文本会被小写后做关键词触发：
- 含 `"next action"` 或 `"suggest"` → 要求 `next_actions` 非空；
- 含 `"uncert"` → 要求 `uncertainties` 非空；
- 含 `"risk"` → 要求至少一条 finding 的 severity 为非 `info`。

**失败重试**：prose 兜底通道若校验失败，不直接报错，而是把错误回灌成一条 user 消息「Your previous evidence packet failed validation: ...」并 `continue` 下一轮（subagent.rs:513-519），让子 Agent 在 6 步上限内自我纠正。但工具通道（subagent.rs:538）校验失败会直接 `?` 向上传播错误终止——这是两条通道在错误处理上的不对称点。

### 3.6 reviewer_count 多评审 + judge / synthesizer

`reviewer_count > 1` 时走 `run_reviewer_panel`（subagent.rs:562-610）：

```
                         max_total_tokens (总硬预算 T)
                              │ 切分 (subagent.rs:567-573)
          ┌───────────────────┼────────────────────────┐
   synthesis = T/(N+1)   per_reviewer = (T - synthesis)/N (各取 max(1))
          │                                              │
          ▼                                              ▼
  ┌─ Reviewer 1 (lens=correctness) ─ evidence packet ─┐
  ├─ Reviewer 2 (lens=evidence)     ─ evidence packet ─┤  每个都是
  ├─ Reviewer 3 (lens=risk)         ─ evidence packet ─┤  reviewer_count=1
  ├─ Reviewer 4 (lens=completeness) ─ evidence packet ─┤  的递归 run()
  └─ Reviewer 5 (lens=simplicity)   ─ evidence packet ─┘  (Box::pin 递归)
          │
          ▼  synthesize_reviewer_outputs (subagent.rs:632-673)
   judge/synthesizer 子 Agent（agent_type="synthesizer", brief, evidence）
   prompt：偏好具体证据而非多数投票，保留分歧为 uncertainties，不得编造证据
          │
          ▼
   "Multi-reviewer synthesis (judge round)" + 合成包 + 各 reviewer 原始包
```

要点：

- **lens 视角**固定为 `["correctness","evidence","risk","completeness","simplicity"]`（subagent.rs:574-580），按 reviewer 序号取，超出则用 `"review"`。每个 reviewer 的 prompt 显式要求「独立判断，不要假设其他 reviewer 的结论正确」（subagent.rs:598-604），实现对抗式多视角审查。
- **每个 reviewer 强制 `output_format = EvidencePacket`**（subagent.rs:592），无论上层请求是 plain 还是 evidence。
- **预算硬切分**：总预算先扣出 judge 的份额（`T/(N+1)`），余下均分给 N 个 reviewer，保证「总预算硬上限」不被多评审放大（subagent.rs:567-573）。
- **judge 子 Agent**（subagent.rs:648-666）清空 `agent_name`、强制 `agent_type="synthesizer"`、`context_mode=Brief`、`reviewer_count=1`，prompt 注入所有 reviewer 的 evidence packet，要求合成单一最终包。
- 两处递归都用 `Box::pin(run(...))`（subagent.rs:605, 667）以避免 async 递归的无限大小 future。

### 3.7 max_total_tokens 硬预算

预算原语在 `agent/budget.rs` 的 `TokenBudgetState`（budget.rs:24-76）：

- `record_usage_or_estimate`（budget.rs:63-75）：provider 返回 `usage` 时用精确值（`used_exact`），否则用本地启发式估算（`used_estimated`）。`estimate_text_tokens`（budget.rs:77-89）按「ascii 字符 /4 向上取整 + 非 ascii 字符数」估算，对中文友好。
- `is_exhausted`（budget.rs:49-53）：`used_total() >= total` 即耗尽。

子 Agent 循环里的预算流（subagent.rs:479-556）：
1. 每轮开头先查 `is_exhausted`，命中直接返回硬停文案（subagent.rs:479-484）；
2. 每次模型调用后 `record_usage_or_estimate`（subagent.rs:496-500）；
3. 每次工具调用后用 `record_estimated` 把「参数 + 结果」文本估算计入（subagent.rs:549-554）；
4. 最终输出经 `with_budget_footer`（subagent.rs:675-689）追加 `Subagent token budget: used=.., remaining=..` 页脚。

预算来源优先级（subagent.rs:353-360）：`req.max_total_tokens`（显式参数）→ 模板 `budget.max_total_tokens` → 无预算（`None`，即不限）。

> **历史状态说明**：现有文档 `docs/ultracode-agent-orchestration.md:45` 把「per-agent 硬预算追踪」列为后续工作（"把 budget step 从 journal 标记升级为硬预算"）。实际上子 Agent 维度的硬预算**已经实现**（上述 `TokenBudgetState` + `is_exhausted` 早停 + footer 上报）。该旧文档此处已过时。

### 3.8 `/ultracode` 临时 overlay 注入

`ultracode::overlay(task)`（ultracode.rs:1-28）只是一个**纯文本生成函数**，产出一段 10 条运行原则 + 上下文工程约定的编排手册，把用户任务嵌入 `当前任务：{task}`。它在 `lib.rs:414-435` 被 `/ultracode [task]` 命令调用：

```
/ultracode <task>
  → task = 去前缀去空白
  → run_id = workflow_journal::new_run_id()
  → overlay = ultracode::overlay(&task)
  → run_turn_with_options(.., TurnOptions {
        system_overlay: Some(overlay),   // 仅本轮注入
        workflow_run_id: Some(run_id),   // 写 journal
        agent_names: Vec::new(),
        token_budget: None,
    })
```

关键设计：overlay 通过 `TurnOptions.system_overlay` 注入，在 `runner.rs:247` 经 `apply_system_overlay` 叠加到 system prompt，**只作用于当前 turn，不写入会话历史**。这避免大段编排手册污染后续普通对话的 prompt（与旧文档 `ultracode-agent-orchestration.md:36` 描述一致）。overlay 文本里第 8/9 条还引导主 Agent 用 `tool_search`+`execute_tool` 访问 deferred 工具、用 `worktree_create` 隔离大改动。

### 3.9 自定义 Agent 的发现 / 校验 / 合并

**存储位置**：`AGENTS_DIR = ".demiurge/agents"`（custom.rs:7），基目录是 `state.sandbox_dir`（custom.rs:121-124），即每次发现都在沙盒根下。运行时统计落 `.demiurge/agent_stats.json`（custom.rs:8）。

**发现**：`list_definitions`（custom.rs:217-237）遍历目录里的 `*.json`，逐个 `definition_from_path`（custom.rs:357-396）。后者反序列化 `AgentFile`，`name` 缺失时回退文件名 stem，并把 `allowed_tools` 按 `valid_tool_names`（来自 `registry_for_state`，含 MCP 工具）分流为 `allowed_tools` / `invalid_tools`，prompt/handoff 分别截到 `MAX_TEXT_CHARS=16_000` / 其半值。

**查找**：`find_agent_path`（custom.rs:423-451）支持两种匹配——文件名 stem 命中（含 `sanitize_name` 归一），或读入 JSON 后 `name` 字段命中。`sanitize_name`（custom.rs:537-550）把非 `[A-Za-z0-9_-]` 字符替换为 `-` 并去首尾 `-`。

**校验**：`validate_agent_json_with_tools`（custom.rs:458-526）区分 errors（阻断保存）与 warnings（仅提示）：
- errors：name 必填；`reserved_output_tokens >= max_input_tokens`；`max_steps == 0`。
- warnings：template 缺 prompt；team 缺 members；template 带了 members（会被忽略）；未知工具名（会被忽略）。

注意校验用的工具表分两套：UI 保存走 `valid_tool_names`（含 MCP，custom.rs:552-557），裸 `validate_raw` 走 `core_tool_names`（仅核心，custom.rs:559-564）。

**合并 `resolve_selected`（custom.rs:297-349）**——主 turn 选定 Agent 路径的核心：

```
BFS 展开（VecDeque + seen 去重 + 深度 ≤ MAX_TEAM_DEPTH=8）：
  - 队列初始为用户选中的 names
  - Team 类型则把 members 入队（depth+1），实现 team 嵌套展开
  - 每个去重后的定义 load_agent 加入 definitions
合并产物：
  - allowed_tools：所有定义的并集（去重，保序）
  - max_input_tokens / reserved_output_tokens / max_steps / max_total_tokens：
        各取所有定义中的最小值（min_assign, custom.rs:351-355）——“最严格者胜”
  - prompt_overlay：build_overlay 拼接每个定义的 description/tools/members/budget/prompt/handoff
```

预算取最小值是安全侧策略：多个 Agent 组合时，最紧的预算约束生效，避免组合放大资源消耗。`build_overlay`（custom.rs:566-602）产出的文本整体截到 `MAX_TEXT_CHARS`。

---

## 4. 与其他模块的交互边界

```
                       用户 / UI
                          │
        ┌─────────────────┼──────────────────────┐
   /ultracode          选中 Agent              聊天工具调用
   (lib.rs:414)        (UI → agent_names)      agent_spawn
        │                  │                       │
   ultracode::overlay  runner.run_turn_with_options   tools::execute
        │                  │  (runner.rs:146)         (mod.rs:898)
        └──► system_overlay │                          │
                            ▼                           ▼
                   custom::resolve_selected     agent_spawn::run
                   (runner.rs:157)              → subagent::run
                     ├ 收紧 settings 预算 (158-166)        │
                     ├ 收敛主工具集 (193-199)        ┌──────┴────────┐
                     ├ overlay 叠加 (246)        子 Agent loop   reviewer panel
                     └ runtime 统计 (176,357,345)  (≤6 步)        + judge
                                                       │
                                                  execute_subagent_readonly
                                                  (mod.rs:915, 只读门禁)
```

具体边界：

- **runner.rs ↔ custom.rs**：`run_turn_with_options` 调 `resolve_selected`，用其 `max_input_tokens`/`reserved_output_tokens` 收紧 `settings`（runner.rs:158-166），用 `max_steps` 钳制主 loop（runner.rs:167-170），用 `max_total_tokens` 建主 turn 预算（runner.rs:171-175），把 `prompt_overlay` 叠进 system（runner.rs:246, 279）。运行统计由 `record_runtime_start`/`record_runtime_usage`/`record_runtime_error`（runner.rs:176, 357, 345）写回 `.demiurge/agent_stats.json`。
- **subagent.rs ↔ llm**：`stream_completion`（subagent.rs:486）发起子 Agent 模型调用；`ProviderProfile::for_kind`（subagent.rs:396）决定 tool schema dialect 与是否支持 tools。
- **subagent.rs ↔ tools/mod.rs**：schema 经 `schemas_json_for_names`（tools/mod.rs:771），只读执行经 `execute_subagent_readonly`（tools/mod.rs:915）。
- **subagent.rs ↔ prompt.rs**：`prompt::build_for_input`（subagent.rs:419, 444）复用主 system prompt 组装（persona/记忆/摘要/环境）。
- **subagent.rs ↔ pack/store**：读 persona（subagent.rs:337）与活动会话（subagent.rs:341-345）。

---

## 5. 安全与权限相关点

1. **子 Agent 写隔离的双层门禁**：schema 层只暴露只读工具（subagent.rs:434, 463 + `READ_ONLY_TOOLS`），执行层 `execute_subagent_readonly` 用 `SUBAGENT_READONLY_TOOL_NAMES` 二次拒绝（tools/mod.rs:920-922），且 `subagent.rs:546-548` 对非白名单工具直接返回错误字符串而非执行。即使模型「越权」调用非只读工具，也只会收到一条错误反馈。
2. **不可递归派生**：子 Agent 工具集不含 `agent_spawn`，prompt 也显式禁止「再次派生子 Agent」（subagent.rs:388, ultracode.rs:12-14）。
3. **派生需用户确认**：`agent_spawn` 是 `PermissionPolicy::ask`（tools/mod.rs:515），`permission_summary`（tools/mod.rs:1043-1056）会说明「将额外调用模型服务、可能发送项目上下文」。这点很重要：fork 模式会把父会话原文发给模型服务。
4. **沙盒路径**：自定义 Agent 目录、统计文件都基于 `sandbox_dir`（custom.rs:121, 129），文件名经 `safe_agent_file_name`/`sanitize_name` 清洗（custom.rs:528-550），防目录穿越。
5. **取消传播**：主/子循环都在每轮检查 `state.cancel`（subagent.rs:475, 584），reviewer panel 中断会把已完成部分保留并尽快返回。
6. **输出截断**：父上下文 `MAX_PARENT_CONTEXT_CHARS=10_000`、模板文本 `MAX_TEXT_CHARS=16_000`、单条消息 900 字符、错误记录 800 字符（custom.rs:281），均防止上下文/统计无限膨胀。

---

## 6. 已知限制与扩展点

1. **两份只读白名单未同步（应统一来源）**：`subagent.rs::READ_ONLY_TOOLS`（8 项）与 `tools/mod.rs::SUBAGENT_READONLY_TOOL_NAMES`（11 项）不一致，导致 `list_dir`/`http_get`/`package_scripts` 在 `execute_subagent_readonly` 里有执行分支却永远无法被子 Agent 触达。建议让 `subagent.rs` 直接引用 `tools::SUBAGENT_READONLY_TOOL_NAMES`，删除本地副本，使 schema、门禁、执行三者同源。
2. **evidence 两条通道错误处理不对称**：工具通道（subagent.rs:538）校验失败直接 `?` 终止 run，而 prose 兜底通道（subagent.rs:513-519）会回灌错误重试。若希望工具通道也能纠错，需要把 `submit_evidence_packet` 的校验失败也转成 user 反馈 + `continue`。
3. **`MAX_SUBAGENT_STEPS=6` 与模板 `budget.max_steps` 未打通**：子 Agent 的步数上限是常量（subagent.rs:12），模板里的 `max_steps`（custom.rs:30）只在主 turn 路径经 `resolve_selected` 生效，**不影响子 Agent 循环**。子 Agent 只消费模板的 `max_total_tokens`（subagent.rs:357-359）。
4. **fork 模式成本**：fork 把父会话全量消息 clone 进子请求，token 成本最高；预算估算虽计入，但若父历史很长，单次请求即可能接近 provider 上限（fork 路径未再做历史裁剪，仅 brief/recent 才走文本截断）。
5. **运行时统计的归属粒度**：`record_runtime_usage` 把整轮 token 平摊记到**所有**选中 definitions 上（custom.rs:253-269），多 Agent 组合时无法区分各自真实消耗。
6. **`agent_type` 软匹配**：`agent_type` 既是软标签也会尝试当模板名匹配（subagent.rs:351-352），若恰好与某 `.demiurge/agents/*.json` 同名会被当模板加载，命名需注意避免意外命中。

---

## 7. 现有文档与代码不符（顺手发现）

| 文档 | 文档原文 | 代码实际 |
| --- | --- | --- |
| `docs/ultracode-agent-orchestration.md:10` | 子 Agent 只读工具列为 `read_file、glob、grep、git_status、system_info、web_search` | 实际 `READ_ONLY_TOOLS` 还含 `web_fetch`、`context_inspect`（subagent.rs:17-26），共 8 项 |
| `docs/ultracode-agent-orchestration.md:45` | 把「per-agent 硬预算追踪」列为后续工作 | 子 Agent 硬预算已实现：`TokenBudgetState` + `is_exhausted` 早停 + footer（subagent.rs:479-484, 675-689） |
| `docs/ultracode-agent-orchestration.md:47` | 把「judge panel：多 Reviewer 独立打分再合成」列为后续工作 | 已实现 `run_reviewer_panel` + `synthesize_reviewer_outputs`（subagent.rs:562-673） |
| `docs/ultracode-agent-orchestration.md:52` | 当前边界称「还没有 per-agent 硬预算追踪…judge…」 | 同上，二者均已落地，该「当前边界」段落已过时 |
