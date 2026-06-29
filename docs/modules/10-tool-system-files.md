# 工具注册表与文件/编辑工具

> 存档级技术原理文档。读者为协作开发者。
> 覆盖源文件：`src-tauri/src/tools/mod.rs`、`args.rs`、`read_file.rs`、`write_file.rs`、`edit_file.rs`、`glob.rs`、`grep.rs`、`list_dir.rs`、`git_status.rs`、`write_plan.rs`。

## 1. 模块职责与定位

`tools` 模块是 Agent 与「外部世界」之间唯一受控的能力出口。它把每一种工具抽象成一个**带元数据的声明**（名称 + 描述 + 输入 JSON Schema + 风险/并发/权限/输出策略），并提供一个统一的异步分发入口 `execute()`。模块顶部的注释一句话点明了设计哲学（`src-tauri/src/tools/mod.rs:1`）：

> 作用域是结构性强制的（文件工具被物理限制在沙盒目录），不靠提示词。

这句话是整个模块的核心立场：**安全边界由 Rust 代码强制，而不是寄希望于模型遵守 system prompt**。文件类工具（`read_file`/`write_file`/`edit_file`/`multi_edit`/`apply_patch`/`undo_edit`/`glob`/`grep`/`list_dir`/`write_plan`）全部通过同一个沙盒路径解析函数 `resolve_in_sandbox`（`mod.rs:1211`）把任何相对路径钉死在沙盒目录内，逃逸尝试在代码层被直接拒绝。

本模块在整个 Agent 回合中的位置：

```text
LLM 返回 tool_calls
        │
        ▼
runner.rs (顺序遍历每个 tool_call)
   ├─ tools::definition_for_state  → 取风险/并发/输出策略元数据
   ├─ tools::confirmation_preview  → 生成 diff/动作预览（写类工具）
   ├─ tools::affected_paths        → 列出将被影响的路径
   ├─ permission gate（Allow / Deny / Ask）
   └─ tools::execute(state, name, args)  ← 本模块的执行入口
        │
        ▼
   具体工具 run()  → Result<String, String>
        │
        ▼
   tool_result 消息回填给模型
```

## 2. 关键类型与入口函数

### 2.1 工具元数据类型（`mod.rs:38`–`114`）

每个工具用 `ToolDefinition`（`mod.rs:105`）描述，字段含义：

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `name` | `&'static str` | 工具唯一名，模型按此名调用 |
| `description` | `&'static str` | 给模型看的能力说明（同时也是中文/英文混合的用户可读文案） |
| `risk` | `ToolRisk` | `ReadOnly` / `Mutating` / `External` / `Privileged` |
| `concurrency` | `ToolConcurrency` | `ParallelSafe` / `SerialOnly` |
| `permission` | `PermissionPolicy` | `effect`(Allow/Deny/Ask) + `scope` + `reason` |
| `output_policy` | `ToolOutputPolicy` | `Inline` / `TruncateForUi` |
| `parameters` | `serde_json::Value` | 标准 JSON Schema，直接进 tools 数组发给 LLM |

四个枚举（`ToolRisk` `mod.rs:59`、`ToolConcurrency` `mod.rs:68`、`ToolOutputPolicy` `mod.rs:75`、`PermissionEffect`/`PermissionScope` `mod.rs:41`/`50`）都派生 `Serialize`，因此可原样作为事件字段传到前端。`PermissionPolicy`（`mod.rs:80`）提供两个 `const fn` 构造器 `allow(reason)` 与 `ask(reason)`（`mod.rs:88`/`96`），让注册表里能用编译期常量声明每个工具的默认权限。

**设计要点**：这些元数据是**声明式**的。新增工具时「只需在 `registry()` 里加一项 + 在 `execute()` 里加一条分支」（`mod.rs:172` 注释），元数据与执行逻辑解耦。

### 2.2 `registry()`：工具定义表（`mod.rs:173`–`725`）

`registry()` 返回一个 `Vec<ToolDefinition>`，是全部内置工具的唯一真相源。注意它是一个**纯函数，每次调用都重新构造整个 Vec**（`definition_for`、`permission_policy_for`、`deferred_definitions` 等都各自调一次 `registry()`，`mod.rs:733`/`849`/`743`）。对于一次工具调用这点开销可忽略，但说明注册表是无状态、可随时重建的。

`registry()` 之外还有 `registry_for_state()`（`mod.rs:727`），它在内置工具之上追加 `crate::mcp::tool_definitions(state)`——即把当前连接的 MCP server 暴露的工具动态并入注册表。因此「真正可用的工具集」是 `内置工具 ∪ MCP 工具`。

### 2.3 三套 schema 方言（`mod.rs:756`–`842`）

工具 schema 要发给不同 provider，`schemas_json_for_defs`（`mod.rs:791`）按 `ToolSchemaDialect` 分派：

- `OpenAiCompatible` → `openai_schemas_json`（`mod.rs:799`）：`{"type":"function","function":{name,description,parameters}}`
- `Anthropic` → `anthropic_schemas_json`（`mod.rs:816`）：`{name,description,input_schema}`（注意键名是 `input_schema`，这是对外部 CLI/SDK 同名约定的兼容）
- `Gemini` → `gemini_schemas_json`（`mod.rs:830`）：包进 `[{"function_declarations":[...]}]`

`main_schemas_json_for_state`（`mod.rs:764`）是主回合实际发送的 schema 来源——它用 `registry_for_state` 的全集（内置 + MCP）。这里有个**值得注意的细节**：`main_schemas_json_for`（不带 state，`mod.rs:760`）按 `CORE_TOOL_NAMES` 过滤，只发 core pool；但 `main_schemas_json_for_state` 实际发的是 `registry_for_state` 全集（含 deferred 工具），并未按 `CORE_TOOL_NAMES` 过滤。runner 的取值逻辑是：`allowed_tool_names` 为空时调用 `main_schemas_json_for_state`（`runner.rs:196`），非空时按名字过滤（`runner.rs:198`）。也就是说，core/deferred 的「池划分」是否生效，取决于 runner 传入的 `allowed_tool_names`（见 §2.5 与 §6 的限制说明）。

### 2.4 `execute()`：统一分发（`mod.rs:874`–`913`）

```rust
pub async fn execute(state, name, args) -> Result<String, String> {
    if crate::mcp::is_mcp_tool_name(name) {           // MCP 工具走 MCP 客户端
        return crate::mcp::call_tool(state, name, args).await;
    }
    match name {
        "read_file"   => read_file::run(state, args),
        "write_file"  => write_file::run(state, args),
        "edit_file"   => edit_file::run(state, args),
        "multi_edit"  => edit_file::multi_run(state, args),
        "apply_patch" => edit_file::patch_run(state, args),
        "undo_edit"   => edit_file::undo(state, args),
        "glob"        => glob::run(state, args),
        "grep"        => grep::run(state, args),
        "list_dir"    => list_dir::run(state, args),
        "git_status"  => git_status::run(state, args),
        "write_plan"  => write_plan::run(state, args),
        ...
        other => Err(format!("未实现的工具：{other}")),
    }
}
```

注释解释了为何用「async 函数 + match」而非 trait object（`mod.rs:872`）：「避免为了少数异步工具引入 async-trait 依赖」。绝大多数文件工具是同步的 `fn run(...) -> Result<String, String>`，只有 `web_fetch`/`http_get`/`web_search`/`agent_spawn`/`context_collapse`/`execute_tool`/`mcp_read_resource` 是 `.await`。

`execute_subagent_readonly`（`mod.rs:915`）是只读子 Agent 的受限入口：先用 `SUBAGENT_READONLY_TOOL_NAMES`（`mod.rs:154`）白名单挡掉一切写类/特权工具，再走一个**更小的 match**。这是「子 Agent 物理上不能写文件」的强制点——即便子 Agent 模型试图调用 `write_file`，分发表里根本没有这条分支。

### 2.5 core / deferred 池划分

三张静态名单（`mod.rs:116`–`166`）：

- `CORE_TOOL_NAMES`（`mod.rs:116`）：26 个常用工具，含全部文件/编辑/搜索工具。
- `DEFERRED_TOOL_NAMES`（`mod.rs:145`）：6 个低频工具——`open_path` 与 5 个 `screen_*`（截图/OCR）。
- `SUBAGENT_READONLY_TOOL_NAMES`（`mod.rs:154`）：11 个只读工具，子 Agent 专用。

`is_deferred_tool`（`mod.rs:168`）判断是否在 deferred 池。划分的目的（与 `docs/IMPLEMENTATION.md:275` 一致）：**主 tools schema 只放高频 core 工具，低频工具留在 deferred pool，靠 `tool_search` 发现、`execute_tool` 代理执行**，从而减少固定 tools JSON 对上下文的占用。

发现/执行链路：

```text
模型不知道某能力是否存在
   → tool_search(query)            tool_search.rs:10
       打分匹配 deferred_definitions()，返回名称+描述+params
   → execute_tool(tool_name,args)  execute_tool.rs:11
       先校验 is_deferred_tool() 才执行；core 工具会被拒绝并提示"直接调用"
```

`tool_search` 的打分（`tool_search.rs:17`）：名字命中 +5，整体 haystack（name+description+params）命中 +1，按分降序、同分按名字升序。`execute_tool` 的 match（`execute_tool.rs:20`）只接了 `open_path` + 5 个 `screen_*`，与 `DEFERRED_TOOL_NAMES` 完全对应。

## 3. 沙盒路径解析：词法校验 + canonicalize 双层防护

这是整个文件工具体系的安全基石，位于 `resolve_in_sandbox`（`mod.rs:1211`）与辅助函数 `canonical_existing_ancestor`（`mod.rs:1254`）。所有文件类工具都调它把相对路径转成绝对路径。

### 3.1 为何需要两层

单纯做字符串 `starts_with(sandbox)` 是不够的：

1. `..` 可以在词法上越界（`a/../../etc/passwd`）。
2. **符号链接 / Windows junction / reparse point** 在词法上看起来在沙盒内，但 `std::fs` 操作会**跟随 reparse point**，实际落到沙盒外（`mod.rs:1241` 注释明确指出这点）。

因此 `resolve_in_sandbox` 分两步：

**第一步：词法解析（`mod.rs:1217`–`1237`）**

```rust
let rel_path = Path::new(rel);
if rel_path.is_absolute() { return Err("路径必须是相对沙盒目录的相对路径…"); }

let mut out = sandbox.to_path_buf();
for comp in rel_path.components() {
    match comp {
        Component::Normal(p) => out.push(p),
        Component::CurDir => {}                       // "." 忽略
        Component::ParentDir => {                      // ".."
            if out == *sandbox {                       // 已在根，再上一级 = 越界
                return Err("路径越界：不允许访问沙盒目录之外");
            }
            out.pop();
        }
        _ => return Err("非法路径组件"),                // 盘符/根等异常组件直接拒绝
    }
}
if !out.starts_with(sandbox) { return Err("路径越界…"); }
```

这一步**手动折叠 `.` 与 `..`**，且在 `out == sandbox` 时遇到 `..` 立即拒绝——即不允许任何中间态越过沙盒根。注意它没有用 `Path::canonicalize`，因为目标文件可能尚不存在（写新文件场景），必须能对不存在的路径做越界判断。

**第二步：真实路径校验（`mod.rs:1242`–`1247`）**

```rust
let canonical_sandbox = std::fs::canonicalize(sandbox)?;
let canonical_ancestor = canonical_existing_ancestor(&out)?;
if !canonical_ancestor.starts_with(&canonical_sandbox) {
    return Err("路径越界：经链接解析后指向沙盒之外");
}
```

`canonical_existing_ancestor`（`mod.rs:1254`）从目标路径出发**逐级向上找到第一个真实存在的祖先**，对它做 `canonicalize`（会解析掉路径中所有 reparse point / 符号链接），再判包含关系：

```rust
fn canonical_existing_ancestor(p: &Path) -> Result<PathBuf, String> {
    let mut cur: &Path = p;
    loop {
        if cur.exists() {
            return std::fs::canonicalize(cur);   // 解析全部 reparse point
        }
        match cur.parent() {
            Some(parent) => cur = parent,
            None => return Err("无法解析路径"),
        }
    }
}
```

**为何只 canonicalize「最近存在的祖先」而非目标本身**：写文件时目标尚不存在，`canonicalize` 会失败；但只要其已存在的父链路里有一条逃逸链接，就会在祖先 canonicalize 时被解析出来并被拦截（`mod.rs:1252` 注释）。这套组合同时挡住了 `../`、绝对路径、符号链接、Windows junction 三类逃逸，且兼容「写尚不存在路径」的用例。

### 3.2 数据流图

```text
rel = "notes/../sub/file.txt"   sandbox = C:\sb
        │
        ▼  第一步：词法折叠 . / ..  （不允许越过根）
out = C:\sb\sub\file.txt   (lexical, 可能不存在)
        │  out.starts_with(sandbox) ✔
        ▼  第二步：canonicalize 沙盒 + 最近存在祖先
canonical_sandbox = \\?\C:\sb
canonical_ancestor = canonicalize(C:\sb\sub)  // file.txt 尚不存在
        │  ancestor.starts_with(canonical_sandbox) ?
        ├─ 是 → Ok(out)
        └─ 否（sub 是指向 C:\其它 的 junction）→ Err 越界
```

## 4. 文件读写工具

### 4.1 read_file（`read_file.rs`）

`run`（`read_file.rs:6`）：取 `path` → `resolve_in_sandbox` → `metadata` 校验是文件 → 大小不超过 `MAX_READ = 256 KiB`（`read_file.rs:4`）→ `read_to_string`。

- **256 KiB 上限**的理由（`read_file.rs:4` 注释）：避免把超大文件灌进上下文。
- 只接受 UTF-8 文本：`read_to_string` 失败时返回「可能不是 UTF-8 文本」（`read_file.rs:24`）。
- `sandbox_dir` 通过 `state.sandbox_dir.lock().unwrap().clone()` 取出（`read_file.rs:8`），`AppState.sandbox_dir` 是 `Mutex<PathBuf>`（`lib.rs:59`），运行期可被切换（`lib.rs:1675`）。

### 4.2 write_file（`write_file.rs`）

`run`（`write_file.rs:4`）：取 `path` + `content` → 解析沙盒路径 → `create_dir_all(parent)` 自动建父目录（`write_file.rs:11`）→ `std::fs::write` 整体覆盖 → 返回「已写入 N 字节」。

注意 `write_file` **不做大小上限校验、不读旧内容、不记录 undo**——它是「创建或覆盖」语义，注册表里标注「不可逆，需用户确认」（`mod.rs:302`），其风险是 `Mutating`、权限是 `ask`。可逆性由 `edit_file` 体系提供（见 §5）。

## 5. 编辑工具体系（`edit_file.rs`）

`edit_file.rs` 实现四个对外工具：`edit_file`、`multi_edit`、`apply_patch`、`undo_edit`。它们共享一套「plan → preview / write」两阶段流水线，核心常量在 `edit_file.rs:6`–`10`：

| 常量 | 值 | 含义 |
| --- | --- | --- |
| `MAX_EDIT` | 256 KiB | 单文件可编辑大小上限 |
| `MAX_PREVIEW_CHARS` | 12000 | diff 预览字符上限，超出截断 |
| `MAX_UNDO_ENTRIES` | 20 | undo 栈深度 |
| `MAX_MULTI_EDITS` | 20 | 单次 multi_edit 的 edit 数上限 |
| `MAX_PATCH_HUNKS` | 20 | 单次 apply_patch 的 hunk 数上限 |

### 5.1 两阶段流水线：plan → write

所有写类编辑都先「规划」（读旧内容、算出新内容，**不落盘**），再「写入」。这让 preview 和真正执行复用同一套规划逻辑，保证 confirm 对话框里展示的 diff 与最终写入完全一致。

- `edit_file`：`run`（`edit_file.rs:57`）= `parse_edit_args` → `plan_edit` → `write_planned_edit`。`preview`（`:46`）走到 `plan_edit` 即止，调 `build_preview`。
- `multi_edit`：`multi_run`（`:73`）→ `plan_multi_edit`（`:312`）→ 逐个 `write_planned_edit`。
- `apply_patch`：`patch_run`（`:98`）→ `plan_patch`（`:333`）→ 逐个 `write_planned_edit`。

`PlannedEdit`（`edit_file.rs:30`）= `{ rel, before, after, replacements }`，是规划阶段的产物，承载 diff 所需的全部信息。

### 5.2 edit_file 的精确替换语义（`apply_edit`，`edit_file.rs:392`）

```rust
let count = original.matches(&req.old_string).count();
if count == 0 { return Err("未找到 old_string，未做任何修改"); }
if !req.replace_all && count > 1 {
    return Err("old_string 出现 N 次，不唯一；请提供更具体上下文或设置 replace_all=true");
}
let updated = if req.replace_all {
    original.replace(&req.old_string, &req.new_string)        // 全部
} else {
    original.replacen(&req.old_string, &req.new_string, 1)    // 第一处
};
```

- **唯一性约束**：默认要求 `old_string` 在文件中唯一出现，否则报错。这避免「替错位置」，迫使模型提供足够上下文。`replace_all=true` 才允许多处替换。
- 入参校验（`validate_edit_args`，`edit_file.rs:159`）：`old_string` 非空、且与 `new_string` 不同（相同视为无意义编辑）。

### 5.3 apply_patch 的结构化行 hunk（`apply_hunk_to_text`，`edit_file.rs:359`）

`PatchHunk`（`edit_file.rs:38`）= `{ rel, start_line(1-based), old_lines, new_lines }`。算法：

```rust
let had_trailing_newline = original.ends_with('\n');   // 保留末尾换行
let mut lines = original.lines().map(ToString::to_string).collect::<Vec<_>>();
let start = hunk.start_line - 1;                        // 转 0-based
let end = start + hunk.old_lines.len();
if start > lines.len() || end > lines.len() { return Err("hunk 越界…"); }
if lines[start..end] != hunk.old_lines[..] { return Err("hunk 不匹配…"); }
lines.splice(start..end, hunk.new_lines.clone());      // 整段替换
let mut next = lines.join("\n");
if had_trailing_newline { next.push('\n'); }
```

两道校验保证安全：**越界检查**（start/end 不能超过实际行数）+ **内容匹配检查**（`old_lines` 必须与目标行逐行全等）。这等价于 `git apply` 的「上下文必须匹配才打补丁」，但用结构化数组而非 unified diff 文本表达。末尾换行被显式保留，避免 patch 引入无意义的行尾差异。

### 5.4 同文件多次编辑的累积语义（`plan_multi_edit` / `plan_patch`）

`multi_edit` 与 `apply_patch` 都用 `HashMap<rel, planned_index>` 记录「某文件是否已在本批次出现」（`edit_file.rs:315`/`336`）。

- 首次遇到某文件：从磁盘读 `before`，规划出 `after`。
- 再次遇到同文件：**在前一个 hunk/edit 的 `after` 之上继续应用**（`edit_file.rs:319`/`340`），并累加 `replacements`，而非各自基于磁盘原文。

这点对 `apply_patch` 尤其重要——注册表描述明确说「后续 start_line 基于前一个 hunk 应用后的内容」（`mod.rs:374`）。测试 `patch_applies_multiple_hunks_to_same_file_in_order`（`edit_file.rs:849`）验证了这一链式行为。

**原子性**：所有规划在写入前完成（`plan_*` 全程不落盘）。任一 edit/hunk 预检失败，函数提前返回 Err，**一个文件都不会被写**。测试 `multi_edit_failure_writes_nothing`（`edit_file.rs:725`）与 `patch_mismatch_writes_nothing`（`edit_file.rs:878`）锁定了这个保证。但要注意：写入阶段是 `for edit in &planned { write_planned_edit(...)?  }`（`edit_file.rs:79`/`103`），多文件场景下若第一个文件写盘成功、第二个 `std::fs::write` 因 IO 错误失败，则**已写的文件不会回滚**——原子性是「预检层面的全有全无」，不是「写盘层面的事务」。

### 5.5 undo_edit：进程内撤销栈

撤销能力建立在 `AppState.edit_undo_stack: Mutex<Vec<EditUndoEntry>>`（`lib.rs:49`）之上，`EditUndoEntry`（`edit_file.rs:12`）= `{ id, path, before, after, created_at, replacements }`。

- 每次成功写入都 `push_undo_entry`（`edit_file.rs:425`）：id 形如 `edit_{millis}_{序号}`，栈超过 `MAX_UNDO_ENTRIES=20` 时从头部 `drain` 丢弃最旧记录（`edit_file.rs:445`）。
- `undo`（`edit_file.rs:128`）：取栈顶 `latest_undo_entry` → 读当前磁盘内容 → `ensure_undo_safe` → 写回 `entry.before` → 弹栈。
- **漂移检测** `ensure_undo_safe`（`edit_file.rs:462`）：若当前文件内容 ≠ 编辑后内容 `entry.after`，说明文件被后续外部修改，**拒绝撤销**并报「无法安全撤销」。测试 `undo_refuses_when_file_drifted`（`edit_file.rs:638`）覆盖此场景。
- 弹栈时再次确认栈顶 id 一致（`edit_file.rs:139`），否则把记录推回——防止并发下误删别人的记录。

**关键限制**：undo 栈是**进程内内存态**，应用重启即清空；且只能从栈顶逐条撤销（无法跳着撤）。`undo_edit` 注册表描述称「撤销本进程内最近一次成功的 edit_file 修改」（`mod.rs:392`）。`write_file` 不进栈，故 `write_file` 的覆盖**不可被 undo_edit 撤销**。

### 5.6 行级 diff 预览（`build_preview`，`edit_file.rs:522`）

预览不是真正的 LCS diff，而是**逐行索引对齐**：把 `before`/`after` 各自 `lines()`，按下标 `idx` 对比，下标相同位置不同就输出 `- 旧行` / `+ 新行`。输出形如：

```text
--- path
+++ path
@@ 替换 N 处 @@
- old line
+ new line
```

特点与边界：
- 这是**位置对齐**而非内容对齐，插入/删除行会导致后续所有行「看起来都变了」，diff 可能偏大。这是为 confirm 预览服务的轻量近似，不追求最小 diff。
- 超过 `MAX_PREVIEW_CHARS=12000` 字符即截断并标注（`edit_file.rs:548`）。
- 若按行未发现整行差异（`changed == 0`），输出提示「可能是行尾或空白字符变化」（`edit_file.rs:555`）——覆盖纯空白/换行变更。
- `multi`/`patch` 预览（`build_multi_preview` `:472`、`build_patch_preview` `:498`）在头部汇总「文件数/edit 数/替换数」，再逐文件拼接 `build_preview`，整体也受 `MAX_PREVIEW_CHARS` 截断。

## 6. 搜索与目录工具

### 6.1 glob（`glob.rs`，globset + walkdir）

`run`（`glob.rs:10`）流程：

1. `validate_pattern`（`glob.rs:72`）：pattern 不能是绝对路径、不能含 `..`。
2. 解析 `base`（默认沙盒根）经 `resolve_in_sandbox`，校验存在且是目录。
3. 用 `globset::GlobSetBuilder` 编译 pattern（`glob.rs:32`）。
4. `WalkDir::new(base_path).follow_links(false)`（`glob.rs:40`）**关闭符号链接跟随**——这是又一道防逃逸（不让遍历顺着链接走出沙盒）。
5. 只收文件（`is_file`），对每个文件用相对沙盒根的路径 `set.is_match(rel)`，命中即收集，路径分隔符统一替换成 `/`（`glob.rs:53`）。
6. 上限：`DEFAULT_LIMIT=200`，`MAX_LIMIT=500`（`glob.rs:7`），达到 limit 即截断并标注。

**注意匹配语义**：`set.is_match(rel)` 中的 `rel` 是「相对沙盒根」的完整相对路径（`strip_prefix(&sandbox)`，`glob.rs:50`），不是相对 `base`。因此即使指定了 `base`，pattern 仍按沙盒根视角匹配（如 `**/*.rs` 能匹配 `base` 子树内文件，因为 `**` 跨多级目录）。

### 6.2 grep（`grep.rs`，regex + walkdir）

`run`（`grep.rs:11`）流程：

1. `build_matcher`（`grep.rs:87`）：默认把 `query` 当字面量（`regex::escape`），`regex=true` 时按正则；`case_sensitive=false`（默认）时 `case_insensitive(true)`。
2. `path` 解析沙盒路径（默认根），支持文件或目录两种输入（`grep.rs:37`/`44`）。
3. 目录用 `WalkDir(...).follow_links(false)` 遍历（`grep.rs:45`，同样关闭链接跟随）。
4. 每个文件 `search_file`（`grep.rs:99`）：跳过非文件、跳过 `> MAX_FILE_BYTES=256 KiB` 的文件、`read_to_string` 失败（非 UTF-8）即跳过并计入 `skipped`。
5. 命中行格式 `rel:行号: 行摘要`（行号 1-based），单行超过 `MAX_LINE_CHARS=240` 字符截断加 `…`（`trim_line`，`grep.rs:130`）。
6. 上限：`DEFAULT_LIMIT=100`，`MAX_LIMIT=300`（`grep.rs:6`）。`remaining = limit - 已收集` 逐文件递减（`grep.rs:60`），达到 limit 即停。
7. 末尾汇总扫描/跳过文件数，并在截断/有跳过时分别加提示。

`grep` 不做 `.gitignore` 过滤，也不区分二进制/文本（靠「非 UTF-8 即跳过」近似排除二进制）。

### 6.3 list_dir（`list_dir.rs`）

`run`（`list_dir.rs:16`）只列**直接子项**（非递归，用 `fs::read_dir` 而非 `WalkDir`）。

- `include_hidden=false`（默认）时跳过以 `.` 开头的项（`list_dir.rs:47`）。
- 每项分类 `dir`/`file`/`other`，文件附带字节大小（`list_dir.rs:53`–`64`）。
- **排序**（`list_dir.rs:67`–`78`）：先按类型（dir=0 < file=1 < other=2），同类型按**小写名字**升序。
- **截断**（`list_dir.rs:80`–`82`）：先记录 `total`，再 `truncate(limit)`，输出头部显示真实总数 `(N entries)`，被截断时追加 `…已达到 limit=N，结果已截断`。`DEFAULT_LIMIT=200`，`MAX_LIMIT=500`（`list_dir.rs:6`）。
- 输出整体 `trim_end`（`list_dir.rs:106`）。测试 `lists_direct_children_sorted_by_kind_and_name`（`list_dir.rs:123`）与 `reports_truncation`（`list_dir.rs:142`）锁定排序与截断行为。

### 6.4 git_status（`git_status.rs`）

`run`（`git_status.rs:9`）在沙盒目录执行 `git status --short --branch`（只读）。实现细节：

- 在**独立线程**里跑 `Command`，主线程用 `mpsc::recv_timeout(5s)` 等结果（`git_status.rs:13`/`21`），超时返回「git status 超时」。这是因为该函数是同步的，用线程 + 通道做超时是避免阻塞。
- 非零退出时若 stderr 含 `not a git repository`（或中文同义）则友好返回「不是 Git 仓库」（`git_status.rs:28`），否则把 stderr 当错误返回。
- stdout 为空返回「Git 工作区干净」。
- 依赖系统 `git` 可执行文件在 PATH 中。

## 7. write_plan：Plan Mode 计划写入（`write_plan.rs`）

`run`（`write_plan.rs:6`）是受最强约束的写工具：**只能写入沙盒 `.demiurge/plans/`**，路径完全由代码构造，不接受调用方传入路径（`write_plan.rs:17`）。

流程：

1. 校验 `content` 非空（`write_plan.rs:7`–`14`）。
2. `plans_dir = sandbox/.demiurge/plans`，`create_dir_all`。
3. 文件名 `plan-{safe_session}-{millis}.md`，其中 `safe_session` 把活动会话 id 里非 `[A-Za-z0-9_-]` 的字符替换成 `_`（`write_plan.rs:22`–`30`）——防注入/路径穿越。
4. 写盘后更新 `AppState.plan_state`（`write_plan.rs:40`–`48`）：`active=true`、`approved=false`、记录 `path`/`content`/`created_at`。

这把「写计划」与「Plan Mode 状态机」绑定：计划写入后处于「待批准」态，用户批准（`approve_plan`，见 `IMPLEMENTATION.md:297`）后才离开 plan 模式进入执行。`write_plan` 的风险是 `Mutating`、权限 `ask`，注册表描述明确限定写入目录（`mod.rs:287`）。

## 8. 与其他模块的交互边界

```text
                         ┌─────────────────────────────┐
   AppState (lib.rs) ───►│ sandbox_dir / edit_undo_stack│
                         │ plan_state / sessions / mcp  │
                         └──────────────┬──────────────┘
                                        │ (Mutex 取值)
runner.rs ──tool_calls──► tools::execute ─┬─► read/write/edit/glob/grep/list_dir/git_status/write_plan
   │                                      └─► mcp::call_tool（MCP 工具）
   │
   ├─ definition_for_state ──► registry_for_state = registry() ∪ mcp::tool_definitions
   ├─ confirmation_preview ──► edit_file::preview / multi_preview / patch_preview / undo_preview …
   ├─ affected_paths       ──► 供 UI 高亮将被改动的路径
   └─ permission gate      ──► permission::decide_for_mode（用 ToolDefinition.permission/risk）
```

- **与 runner / session_engine**：runner 顺序遍历 tool_calls，每个工具先发 `ToolStartEvent`（`runner.rs:459`），该事件携带 `risk`/`permission_effect`/`concurrency`/`output_policy`/`preview`/`affected_paths`（`session_engine.rs:95`–`100`）。这些元数据**透传到前端**用于展示与决策。
- **与 permission 模块**：`permission_policy_for_state`（`mod.rs:855`）给出默认策略，`permission::decide_for_mode`（`runner.rs:486`）结合当前模式（如 Plan/strict）得出最终 Allow/Deny/Ask。未知工具默认 `ask`（`mod.rs:852`）——fail-safe。
- **与 mcp 模块**：MCP 工具名通过 `is_mcp_tool_name` 识别并改走 MCP 客户端；MCP 工具的定义也并入注册表（`registry_for_state`），统一参与 schema 生成与权限展示。
- **与前端 confirm 流程**：`confirmation_preview`（`mod.rs:1110`）为 `edit_file`/`multi_edit`/`apply_patch`/`undo_edit`/`shell`/`execute_tool`/`worktree_create`/`screen_*` 生成可读预览；其余工具返回 `None`（无预览）。
- **`args.rs` 校验辅助**：所有工具的入参校验复用 `args.rs` 的 `required_non_empty_str`/`required_str`/`optional_bool`/`optional_u64_clamped`（后者把数值上限钳进 `[min,max]`，是 limit 参数防滥用的统一手段）。

## 9. 安全与权限要点小结

| 防护点 | 位置 | 作用 |
| --- | --- | --- |
| 词法路径折叠 + 越界拒绝 | `mod.rs:1217` | 拦 `..`、绝对路径、异常组件 |
| canonicalize 祖先校验 | `mod.rs:1242` | 拦符号链接 / junction / reparse 逃逸 |
| `follow_links(false)` | `glob.rs:40`、`grep.rs:45` | 遍历不顺着链接走出沙盒 |
| 文件大小上限 256 KiB | `read_file.rs:4`、`edit_file.rs:6`、`grep.rs:8` | 限上下文膨胀 / 拒大文件 |
| edit 唯一性约束 | `edit_file.rs:397` | 避免误替换 |
| patch 越界 + 内容匹配校验 | `edit_file.rs:368`/`377` | 上下文不符就拒打补丁 |
| 预检全有全无 | `plan_*` 不落盘 | 任一失败不写任何文件 |
| undo 漂移检测 | `edit_file.rs:462` | 文件被外改则拒绝撤销 |
| 子 Agent 只读白名单 | `mod.rs:920` | 子 Agent 物理上无写工具分支 |
| write_plan 目录固定 | `write_plan.rs:17` | 只能写 `.demiurge/plans/` |
| 会话 id 字符过滤 | `write_plan.rs:22` | 防文件名注入 |
| 未知工具默认 ask | `mod.rs:852` | fail-safe 权限 |

## 10. 已知限制与扩展点

1. **`concurrency` 当前是元数据，不是调度器**。runner 用 `for tc in &turn.tool_calls` **顺序执行**所有工具（`runner.rs:440`），`ToolConcurrency::ParallelSafe` 仅作为透传给前端的标签（`runner.rs:466` → `session_engine.rs:97`），后端并未据此并行执行同一回合内的多个工具。provider 侧的 `parallel_tool_calls`（`llm/openai.rs:113`）控制的是模型一次能否返回多个 tool_call，与后端是否并行执行无关。若未来要并行，需要在 runner 引入按 `concurrency` 分组的调度。

2. **`output_policy` 在本模块只是声明**。`ToolOutputPolicy::TruncateForUi` 同样作为元数据透传到前端（`runner.rs:467`），本模块工具自身的输出截断是各自硬编码的（如 grep `MAX_LINE_CHARS`、list_dir `limit`、edit 预览 `MAX_PREVIEW_CHARS`）。「按 output_policy 统一裁剪给 UI」的逻辑在前端/事件层，而非工具实现内。

3. **`registry()` 每次全量重建**。`definition_for` / `permission_policy_for` 等都各自重建整个 Vec（`mod.rs:734` 等）。当前规模无性能问题，但若注册表显著增大可考虑 `OnceLock` 缓存。

4. **core/deferred 池在主回合的实际过滤取决于 runner 入参**。`main_schemas_json_for_state`（`mod.rs:764`）发的是 `registry_for_state` 全集而非 `CORE_TOOL_NAMES` 子集；只有 `main_schemas_json_for`（不带 state）才按 core 名单过滤。因此 deferred 工具是否真的「不进主 schema」依赖 runner 传入的 `allowed_tool_names`（`runner.rs:194`–`198`）。文档 `IMPLEMENTATION.md:275` 描述的「主 schema 只放 core」是设计意图，代码里这条过滤的生效路径需结合 runner 一并理解。

5. **多文件写入非事务**。§5.4 已述：预检原子，但写盘阶段多文件中途 IO 失败不回滚已写文件。

6. **undo 栈不持久化**、深度上限 20、只能栈顶撤销；`write_file` 覆盖不可 undo。

7. **glob pattern 按沙盒根视角匹配**，指定 `base` 只缩小遍历范围、不改变匹配基准（§6.1），使用 `**` 才能跨目录命中。

8. **`git_status` 依赖系统 git**、5 秒超时；非 git 目录返回友好提示而非错误。

**扩展新工具的标准动作**：① 在 `registry()`（`mod.rs:173`）加 `ToolDefinition`；② 在 `execute()`（`mod.rs:878`）加 match 分支；③ 写类工具按需在 `confirmation_preview`（`mod.rs:1110`）加预览、在 `affected_paths`（`mod.rs:1158`）加路径提取、在 `permission_summary`（`mod.rs:940`）加确认文案；④ 若属低频工具，加入 `DEFERRED_TOOL_NAMES`；⑤ 若允许子 Agent 使用且只读，加入 `SUBAGENT_READONLY_TOOL_NAMES` 并在 `execute_subagent_readonly` 加分支。
