# 权限模型与安全边界

> 存档级技术原理文档。读者：协作开发者。
> 主要源文件：
> - `src-tauri/src/permission/mod.rs`
> - `src-tauri/src/tools/args.rs`
> - `src-tauri/src/tools/mod.rs`（风险/策略类型、沙盒路径解析、审计辅助）
> - `src-tauri/src/agent/runner.rs`（权限门在 Agent 循环中的接入点）
> - `src-tauri/src/lib.rs`（`respond_confirm` / `interrupt` / `approve_plan` 等 Tauri 命令、`PlanState`）
> - `src-tauri/src/tools/write_plan.rs`
> - `src-tauri/capabilities/default.json`、`src-tauri/tauri.conf.json`（Tauri 暴露面）

---

## 一、模块职责与定位

权限子系统是 Demiurge 的「执行门」：在 Agent 决定调用某个工具、但**尚未真正执行**之前，对该调用做一次裁决，得出三种结果之一——直接放行、直接拒绝、或弹出前端确认对话框等待用户裁决。它的核心目标是确保**有副作用的操作（写文件、shell、外部发布、系统能力）在执行前获得用户许可**，而只读探索尽量不打扰用户（`src-tauri/src/permission/mod.rs:1-2` 的模块注释即点明此意图）。

它的设计有两条贯穿始终的安全原则：

1. **作用域是结构性强制的，而非提示词约束。** 文件类工具被物理限制在沙盒目录内（`src-tauri/src/tools/mod.rs:3` 注释明确写道：「作用域是结构性强制的（文件工具被物理限制在沙盒目录），不靠提示词」）。即便模型被诱导尝试越界，`resolve_in_sandbox` 也会在文件系统层面拒绝。
2. **权限审计不落敏感参数。** 审计记录工具名、裁决结果、来源与理由，但不写入工具入参的完整内容（详见第五节）。

这一子系统与上游的「工具注册表」（提供默认策略与风险等级）和「Agent 运行循环」（在每次工具调用前调用本模块）紧密耦合，但本身不执行任何工具——它只产出 `PermissionDecision`。

---

## 二、关键类型与入口函数

### 2.1 基础枚举（定义于 `src-tauri/src/tools/mod.rs`）

| 类型 | 取值 | 说明 |
|------|------|------|
| `PermissionEffect`（`:41-45`） | `Allow` / `Deny` / `Ask` | 裁决三态。`Ask` 表示需要弹窗确认 |
| `PermissionScope`（`:50-55`） | `Once` / `Session` / `Project` / `User` | 决策的记忆范围 |
| `ToolRisk`（`:59-64`） | `ReadOnly` / `Mutating` / `External` / `Privileged` | 工具风险等级，驱动模式决策 |
| `PermissionPolicy`（`:81-103`） | `{ effect, scope, reason }` | 工具注册表里写死的**默认策略**，含两个 const 构造器 `allow()` / `ask()` |

`PermissionMode`（决策模式）定义在 `src-tauri/src/store/mod.rs:113-118`，取 `Plan` / `Default` / `Auto` / `Bypass` 四值，由 `Settings.permission_mode` 持久化。

### 2.2 权限模块自身的类型（`src-tauri/src/permission/mod.rs`）

- `PermissionDecision`（`:33-40`）：一次裁决的结果，比 `PermissionPolicy` 多了 `source`（来源）和 `mode`（当时所处模式）字段。
- `PermissionDecisionSource`（`:25-31`）：`ToolDefault` / `UserOverride` / `UnknownTool`，标记裁决「依据何来」。
- `PermissionRule`（`:54-61`）：可持久化的用户规则，带 `updated_at`。
- `PermissionResponse`（`:109-122`）：前端确认对话框的回执 `{ allow, scope }`，提供便捷构造 `deny_once()`。
- `PermissionRequest`（`:124-133`）/ `PermissionPromptPayload`（`:135-149`）：发给前端弹窗的请求体。注意 `PermissionPromptPayload` 是**发往前端的瞬态结构**，包含 `args` 字段（完整入参），但它不会写入审计文件。
- `PermissionAuditEntry`（`:63-73`）：落盘的审计条目，**不含** `args` 字段。

### 2.3 入口函数一览

| 函数 | 位置 | 职责 |
|------|------|------|
| `decide_for_mode` | `:178-240` | **主入口**：按当前 `PermissionMode` 产出裁决 |
| `decide` | `:151-176` | 规则查找：会话规则 → 项目规则 → 用户规则 → 默认策略 |
| `confirm` | `:306-344` | 发起前端确认并 await（含 5 分钟超时） |
| `remember_response` | `:242-286` | 把用户的「记住」选择按 scope 落盘 |
| `audit` | `:288-300` | 追加一条审计（不落敏感参数） |
| `panel_state` / `upsert_rule` / `reset_rule` | `:346-441` | 权限面板的读取与增删改 |

---

## 三、核心数据流与决策算法

### 3.1 一次工具调用的完整权限流（在 Agent 循环里）

权限门接入点在 `src-tauri/src/agent/runner.rs:480-533`。一次工具调用的裁决数据流如下：

```
模型返回 tool_call(name, args)
        │
        ▼
default_policy = permission_policy_for_state(state, name)   // 注册表默认策略；未知工具回退到 ask()
risk          = tool_def.risk  (未知工具 → Privileged)       // runner.rs:482-485
        │
        ▼
decision = permission::decide_for_mode(state, name, default_policy, risk)   // 主决策
permission::audit(state, name, &decision)                   // ① 先记一条「裁决」审计
        │
        ├── Allow ─────────────────────────────► allowed = true
        ├── Deny  ─────────────────────────────► allowed = false
        └── Ask   ──► permission::confirm(...) await 用户裁决
                          │
                          ├─ remember_response()  // 若 scope != Once，落盘规则
                          ├─ 用回执覆写 decision.effect/scope/source/reason
                          └─ permission::audit(...)  // ② 再记一条「用户最终裁决」审计
        │
        ▼
interrupted = state.cancel  // runner.rs:535
if !allowed && interrupted → "[Interrupted before execution]"   // 中断优先于普通拒绝
else if !allowed           → "[User denied this operation]"
else                       → tools::execute(...)               // 真正执行
```

值得注意的两个设计细节：

- **审计可能写两条。** `Ask` 路径会先记一条「需要询问」的裁决（`runner.rs:487`），用户回执后再记一条带 `UserOverride` 来源的最终裁决（`runner.rs:530`）。`Allow`/`Deny` 路径只记一条。
- **中断态与拒绝态被区分。** `runner.rs:537-540` 用 `interrupted` 标志把「执行前被用户中断」和「用户主动拒绝」拆成两种不同的工具结果文本，便于模型理解上下文。

### 3.2 `decide_for_mode` 模式状态机（`permission/mod.rs:178-240`）

这是整个子系统的核心。它先取出 `permission_mode` 和 `plan_state` 快照，再按模式分支：

```
                       ┌─────────────────────────────────────────────┐
                       │            decide_for_mode(risk)             │
                       └─────────────────────────────────────────────┘
   mode = Default ─────► decide()  // 规则链 + 默认策略，原样返回
   mode = Auto    ─────► risk == ReadOnly ? Allow(Once)
                                          : decide()
   mode = Bypass  ─────► 永远 Allow(Once)，source = UserOverride
   mode = Plan    ─────► plan.approved          ? decide()
                         risk == ReadOnly        ? Allow(Once)  // 允许只读探索
                         tool == "write_plan"    ? Allow(Once)  // 允许写受限计划
                         否则                     → Deny(Once)   // 阻断写/shell/外部/系统
```

最后无论走哪条分支，都会把 `decision.mode = Some(mode)` 写回（`:238`），这样审计与前端弹窗都能知道裁决发生在哪个模式下。

各模式的设计意图：

- **Default**：最常规的「按规矩办事」。完全委托给 `decide()`，即工具的注册表默认策略叠加用户已记住的规则。
- **Auto**：「自动驾驶但不鲁莽」。只对 `ReadOnly` 工具无条件放行，其余工具仍回到 `decide()` 走正常确认。这样模型可以自由探索（读文件、grep、list_dir）而不打扰用户，但任何有副作用的操作仍受控。
- **Bypass**：「完全信任」。无条件放行一切，来源标为 `UserOverride`、理由写明「Bypass 模式已开启」。这是最危险的模式，但**仍然会被审计**（`runner.rs:487` 的 `audit` 在所有模式下都执行）。
- **Plan**：见第四节专述。

### 3.3 `decide` 的规则查找优先级（`permission/mod.rs:151-176`）

`decide` 实现了一条**短路的优先级链**，先命中先返回：

```
1. session_permission_rules（内存，本会话）   ─┐
2. permissions.json（项目级，data_dir 下）    ─┼─► 命中即 decision_from_rule()，source = UserOverride
3. user_permissions.json（用户级，data_dir 下）─┘
4. 都未命中 ─► PermissionDecision::from_policy(default_policy)，source = ToolDefault
```

优先级语义：**范围越窄越优先**。会话内的临时决策压过项目规则，项目规则压过用户全局规则。三层规则文件都用 `HashMap<String, PermissionRule>` 序列化（`load_rules_file` / `save_rules_file`，`:499-509`），key 是工具名。

> 安全关注点：`decide_for_mode` 的 **Auto 模式对 `ReadOnly` 工具是「无条件 Allow」，绕过了 `decide()`**（`:189-196`）。也就是说，如果用户曾对某个只读工具设置了 `Deny` 规则，在 Auto 模式下该规则会被忽略。同理 Plan 模式未批准前的只读放行（`:211-218`）也绕过规则链。这是「模式优先于规则」的有意取舍，但实现者需要意识到：用户级 `Deny` 规则并非在所有模式下都生效。

### 3.4 confirm 前后端往返与超时（`permission/mod.rs:306-344`）

`confirm` 是一次跨越「Rust 异步任务 ↔ 前端 UI」的握手，机制如下：

```
   后端 confirm()                          前端                          respond_confirm 命令
   ─────────────                          ────                          ───────────────────
   id = next_id()  // "confirm_N" 自增
   (tx, rx) = oneshot::channel
   pending_confirms.insert(id, tx)  ──┐
   app.emit("tool-confirm-request",  │
            PromptPayload{id,...})  ──┼──► 收到事件，弹出确认对话框
                                      │     用户点击「允许/拒绝 + 作用域」
   timeout(300s, rx).await  ◄─────────┼──── invoke respond_confirm(id, allow, scope)
        │                             │          │
        │                             └──────────┴─► pending_confirms.remove(id) → tx.send(resp)
        ▼
   Ok(Ok(resp)) → 返回用户裁决
   超时/通道异常 → remove(id) + deny_once()   // 默认拒绝，绝不默认放行
```

- 唯一 id 由进程内原子计数器 `SEQ` 生成（`:18-22`），格式 `confirm_{n}`。
- 发送端 `oneshot::Sender` 存进 `AppState.pending_confirms`（`lib.rs:43`，类型 `Mutex<HashMap<String, oneshot::Sender<PermissionResponse>>>`）。
- 回执命令 `respond_confirm`（`lib.rs:524-533`）从 map 中**取出**对应 sender 并回填裁决。取出即移除，保证一次确认只被消费一次。
- **超时按拒绝处理**：5 分钟（`Duration::from_secs(300)`）内无回执，或通道异常，都走 `deny_once()`（`:341`）。这是「fail closed」原则——任何异常都不放行。

### 3.5 once / session / project / user 作用域的落地（`remember_response`，`:242-286`）

用户在弹窗里选择的 scope 决定决策被记忆多久：

| scope | 落地位置 | 生命周期 |
|-------|---------|---------|
| `Once` | 不落地（`:247-249` 直接返回 `Ok`） | 仅本次 |
| `Session` | `session_permission_rules`（内存 HashMap） | 本会话，重启即失 |
| `Project` | `permissions.json`（`save_project_rules`） | 跨重启，绑定该 data_dir |
| `User` | `user_permissions.json`（`save_user_rules`） | 跨重启，全局 |

`remember_response` 把回执转成 `PermissionRule`（含 `effect`、`scope`、固定 reason「用户在确认弹窗中选择记住此决策」和 `updated_at`），再按 scope 分发存储。注意它对 `Once` 有双重短路（开头 `:247` 和 match 分支 `:264`），是冗余但无害的防御。

### 3.6 interrupt 唤醒待确认项按拒绝处理（`lib.rs:506-513`）

这是 confirm 机制的关键补强。`confirm` 的 await 最长会阻塞 5 分钟；如果用户在弹窗挂起期间点了「停止」，整轮对话会被这个 await 卡住。`interrupt` 命令的处理逻辑：

```rust
fn interrupt(app, state) {
    agent::session_engine::request_interrupt(&app, state.inner());  // 设置 cancel 标志
    // 立即唤醒所有正在等待的确认（按「中断」处理）
    let mut pending = state.pending_confirms.lock().unwrap();
    for (_, tx) in pending.drain() {
        let _ = tx.send(PermissionResponse::deny_once());   // 全部按拒绝唤醒
    }
}
```

`drain()` 一次性清空 pending map 并对每个挂起的 oneshot 发送 `deny_once()`，让所有 `confirm().await` 立刻返回拒绝，不必等到超时。随后 runner 的 `interrupted = state.cancel`（`runner.rs:535`）会把结果归类为 `[Interrupted before execution]` 而非普通拒绝。

---

## 四、Plan Mode：计划状态机与 `write_plan` / `approve_plan` 边界

Plan Mode 让模型「先规划、后执行」：批准前禁止一切副作用，只允许只读探索和写一份受限计划文件。

### 4.1 `PlanState`（`lib.rs:64-78`）

```rust
pub struct PlanState {
    pub active: bool,        // 是否处于计划进行中
    pub approved: bool,      // 计划是否已批准（决定 decide_for_mode 是否解禁）
    pub path: Option<String>,    // 沙盒相对路径
    pub content: Option<String>,
    pub created_at: Option<u64>,
    pub approved_at: Option<u64>,
}
```

`reset()` 把整个状态清空。该状态存于 `AppState.plan_state`（`lib.rs:47`）。

### 4.2 状态迁移

```
                set_permission_mode(Plan)            write_plan 工具执行
   [空] ────────────────────────────────► active=true ────────────────────► active=true
                lib.rs:657-662              approved=false  write_plan.rs    approved=false
                                            approved_at=None  :40-48         path=Some(rel)
                                                                            content=Some(...)
                                                                            created_at=Some(..)
                                                       │
                          approve_plan (lib.rs:674-694)│        reject_plan (lib.rs:696-...)
                          ┌──────────────────────────┘         └──────────────► plan.reset() → [空]
                          ▼
                  active=false, approved=true, approved_at=now
                  且 settings.permission_mode 强制切回 Default 并持久化
```

关键边界：

- **`write_plan` 只能写沙盒内的受限目录**。`write_plan.rs:16-18` 把目标固定为 `sandbox/.demiurge/plans/`，文件名是 `plan-{安全化会话名}-{时间戳}.md`。会话名经过逐字符白名单过滤（仅 `[A-Za-z0-9_-]`，其余替换为 `_`，`:22-30`），防止会话名注入路径分隔符。写完后用 `strip_prefix(&sandbox)` 转成沙盒相对路径存入 `PlanState`（`:35-44`），并把反斜杠归一为 `/`。
- **`decide_for_mode` 的 Plan 分支是真正的执行边界**（`permission/mod.rs:208-236`）。在 `plan.approved == false` 时：只读工具放行、`write_plan` 放行、**其余一律 `Deny`**。这意味着 Plan Mode 的限制是在权限门强制的，并非仅靠提示词。
- **提示词只是辅助**。`runner.rs:618-620` 的 `plan_mode_overlay()` 给模型注入一段说明，引导它只做只读探索并用 `write_plan` 产出计划。但即使模型无视该提示尝试写文件，权限门仍会 `Deny`。
- **批准会自动解禁并切回 Default**。`approve_plan`（`lib.rs:674-694`）要求 `plan.path` 非空（否则报错「当前没有可批准的计划文件」），置 `approved=true`，并把 `settings.permission_mode` 强制改为 `Default` 后持久化，再 emit `permission-mode-updated` 与 `plan-updated` 事件。此后 `decide_for_mode` 的 Plan 分支里 `plan.approved` 为真会走 `decide()`——但因为模式已切回 Default，实际上后续裁决直接走 Default 路径。

---

## 五、安全与权限相关点

### 5.1 审计不落敏感参数（`permission/mod.rs:63-73, 288-300, 526-535`）

审计是「谁、什么时候、对哪个工具、做了什么裁决」的记录，**刻意不包含工具入参**：

- 落盘结构 `PermissionAuditEntry` 字段仅有 `timestamp / tool / effect / scope / source / reason / mode`，**没有 args**。
- 完整入参只出现在两处瞬态场景：发往前端弹窗的 `PermissionPromptPayload.args`（`:140`），以及 runner 里 `serde_json::to_string_pretty(&args)` 生成的 `pretty`（`runner.rs:492`）——后者也只塞进 `PermissionRequest` 给前端展示，不进审计。
- `reason` 字段是固定话术或工具默认理由，不含动态参数内容。
- 审计以 JSON Lines 追加写入 `permission_audit.jsonl`（`append_audit`，`:526-535`，`OpenOptions::create().append()`）。读取时 `load_recent_audit`（`:511-524`）按时间戳倒序取最近 N 条（面板默认 80，`panel_state` `:365`）。

这一设计的意图：审计可以安全地长期保留、可以展示给用户，而不会泄漏诸如文件内容、shell 命令、剪贴板数据等敏感载荷。

### 5.2 沙盒路径防逃逸（`tools/mod.rs:1211-1265`）

`resolve_in_sandbox` 是文件类工具共用的越界守卫，采用**词法 + 真实路径**双重校验：

```
1) 词法解析（不要求路径存在）：
   - 绝对路径直接拒绝（:1213-1215）
   - 逐 Component 折叠：Normal 入栈、CurDir 忽略、ParentDir 出栈
   - ParentDir 试图越过沙盒根（out == sandbox）→ 拒绝「路径越界」（:1223-1228）
   - 出现盘符/根等异常组件 → 拒绝「非法路径组件」（:1231）
   - 折叠后再判一次 out.starts_with(sandbox)（:1235-1237）

2) 真实路径校验（防 symlink / junction 逃逸）：
   - canonicalize(sandbox) 得到真实沙盒根（:1242-1243）
   - canonical_existing_ancestor(out)：沿父链找到最近的「已存在祖先」再 canonicalize（:1254-1265）
   - 若 canonical_ancestor 不在 canonical_sandbox 之下 → 拒绝「经链接解析后指向沙盒之外」（:1245-1247）
```

第二步是关键的纵深防御：纯词法的 `starts_with` 拦不住符号链接/junction——`std::fs` 操作会跟随 reparse point（源码注释 `:1239-1241` 明确指出）。对**尚不存在的目标文件**（写新文件场景），`canonical_existing_ancestor` 会沿父目录回溯到最近存在的祖先做规范化，因此即便目标本身不存在，其父链中若含逃逸链接也会被发现。

多个文件工具在结果路径上也用到 `strip_prefix(sandbox)`，但用途不同需区分：`grep.rs:113-115` 在结果路径上额外做一次 `strip_prefix` 越界校验，失败时直接返回 `Err`（「路径越界：结果不在沙盒内」），属于真正的越界守卫；而 `package_scripts.rs:118`（`relative_display`）与 `shell.rs:363` 只是用 `strip_prefix` 把路径格式化为沙盒相对路径用于展示，失败时经 `unwrap_or_else` 回退到完整路径显示，既不报错也不构成安全边界。

### 5.3 Tauri 暴露面（`capabilities/default.json`、`tauri.conf.json`）

Tauri v2 的能力（capability）系统决定前端 WebView 能调用哪些核心 API。Demiurge 的暴露面**非常克制**：

```json
// capabilities/default.json
{
  "windows": ["main"],
  "permissions": [
    "core:default",            // Tauri 核心默认权限集
    "core:event:allow-listen", // 允许前端监听后端 emit 的事件
    "core:event:allow-unlisten"
  ]
}
```

- 只暴露了核心默认权限和事件监听/取消监听。**没有**开放 `fs`、`shell`、`http`、`dialog` 等插件能力给前端——所有文件、shell、网络操作都走自定义 Tauri 命令（在 Rust 侧经过权限门和沙盒校验），前端无法绕过后端直接触达文件系统。
- 事件监听能力是必需的，因为权限确认走的是 `app.emit("tool-confirm-request", ...)`（`permission/mod.rs:334`）→ 前端 listen → `respond_confirm` invoke 的往返。
- `tauri.conf.json` 中 `app.security.csp` 为 `null`（`:25-27`），即未自定义内容安全策略。生产环境若加载远程内容，这一点值得收紧（当前 `frontendDist` 指向本地 `../dist`，风险有限）。

### 5.4 入参轻量校验（`tools/args.rs`）

`tools/args.rs` 提供一组工具入参的轻量校验/取值辅助，是工具实现侧的第一道输入卫生：

- `required_str` / `required_non_empty_str`（`:4-17`）：取必填字符串，缺失或空白即返回中文错误。
- `optional_str` / `optional_bool`（`:19-25`）：带默认值的可选取值。
- `optional_u64_clamped`（`:27-38`）：取数值并用 `clamp(min, max)` 钳制到合法区间，防止超大/越界数值（如分页、超时、字节数）触发资源滥用。

这些是 schema 之外的运行时防御，确保即使模型给出非法参数，工具也能优雅拒绝而非 panic。

---

## 六、与其他模块的交互边界

| 协作方 | 交互内容 | 关键调用 |
|--------|---------|---------|
| 工具注册表 `tools/mod.rs` | 提供默认策略、风险等级、工具 schema；提供沙盒解析与审计辅助 | `permission_policy_for_state` / `definition_for_state` / `registry` / `resolve_in_sandbox` |
| Agent 循环 `agent/runner.rs` | 每次工具调用前调用权限门，按裁决执行/拒绝/确认 | `decide_for_mode` → `audit` → `confirm` → `remember_response` |
| 设置/会话存储 `store/mod.rs` | 提供 `PermissionMode`、`now_millis`、持久化 settings | `state.settings.permission_mode` |
| Tauri 命令层 `lib.rs` | 回执 `respond_confirm`、中断 `interrupt`、计划 `approve_plan`/`reject_plan`/`set_permission_mode` | 见第三、四节 |
| MCP 动态工具 `mcp` | 为 MCP 工具提供权限摘要；MCP 工具按 annotation 映射风险后接入同一权限门 | `permission_summary_for_state` 内的 `crate::mcp::permission_summary`（`tools/mod.rs:1103-1108`） |
| 前端 | 监听 `tool-confirm-request` / `plan-updated` / `permission-mode-updated` 事件，渲染弹窗与权限面板 | `app.emit(...)` |

权限面板（`panel_state`，`:346-368`）把三层规则（会话+项目+用户）、最近 80 条审计、以及全部工具的默认策略（`tool_views`，`:453-467`，来自 `registry()`）一并返回给前端展示。`upsert_rule`（`:396-441`）允许用户在面板里直接编辑规则——它会校验工具存在（`definition_for_state`）、拒绝 `Once` scope（`Once` 仅对单次确认回执有效，`:400-401`），再按 scope 落盘。

---

## 七、已知限制与扩展点

1. **模式优先于用户规则的非对称性**（见 3.3 安全关注点）：Auto 模式对只读工具、Plan 未批准前对只读工具，都是无条件 `Allow`，会绕过用户设置的 `Deny` 规则。当前没有「即便 Auto/Plan 也尊重显式 Deny」的逃生通道，若未来需要支持「永久禁用某只读工具」需改造 `decide_for_mode`。

2. **审计仅追加、无轮转**：`permission_audit.jsonl` 只追加不轮转（`append_audit`），长期运行会无限增长。读取端虽只取最近 80 条，但文件本身不会被裁剪。

3. **规则文件无 schema 版本**：`permissions.json` / `user_permissions.json` 直接是 `HashMap` 序列化（`load_rules_file` 解析失败时静默回退到空 map，`:499-504`），损坏或字段演进时会静默丢弃全部规则，无迁移机制。

4. **`PermissionDecisionSource::UnknownTool` 已声明但未在主流程使用**：枚举里定义了 `UnknownTool`（`:30`，且整个枚举带 `#[allow(dead_code)]`），但 `decide`/`decide_for_mode` 对未知工具走的是 `permission_policy_for_state` 的 `ask()` 回退（`tools/mod.rs:855-859`），来源仍标 `ToolDefault`。该变体目前是预留。

5. **CSP 未设置**：`tauri.conf.json` 的 `csp: null`（见 5.3），属当前为本地 dist 加载下的可接受现状，是后续加固的扩展点。

6. **Plan Mode 解禁即整体切回 Default**：`approve_plan` 把模式强制改为 Default（`lib.rs:686-690`），因此「批准计划后仍停留在受控的非 Default 模式」这一组合当前不存在。

---

## 附：对外部 CLI 同名约定的兼容

代码中文件类工具被限制在沙盒目录，计划文件写入 `sandbox/.demiurge/plans/`（`write_plan.rs:17`）。`.demiurge/` 是本项目自有的沙盒内部目录约定；额外兼容能力统一放在 `sandbox/.demiurge/compat/` 下，避免散落多个隐藏目录。
