# 分层长期记忆与 Dream 整理

> 存档级技术原理文档。覆盖 user / project / session / pack 四层记忆的存储、读写与审计，自动记忆提取 `extract_and_update`，以及 `/dream` 手动记忆整理流程。
>
> 主要源文件：
> - `src-tauri/src/agent/memory.rs`（手动维护 API、四层 scope、自动提取、去重审计）
> - `src-tauri/src/agent/dream.rs`（`/dream` 整理与覆盖写入）
> - 相邻协作点：`src-tauri/src/agent/prompt.rs`、`src-tauri/src/agent/runner.rs`、`src-tauri/src/lib.rs`、`src-tauri/src/permission/mod.rs`

---

## 1. 模块职责与定位

Demiurge 的长期记忆刻意保持为「人类可读的 Markdown 文件」，而不是数据库或向量库。`memory.rs` 的模块注释（`src-tauri/src/agent/memory.rs:1`）明确了这一取舍：

- 自动提取（`extract_and_update`）只把稳定记忆**追加**到 project scope 的 `.demiurge/memory.md`；
- 维护 API 则把 user / project / session / pack **四个 scope** 都暴露为可编辑的 Markdown 文件。

这样设计的核心动机有三点：

1. **可解释、可审计、可手改**：记忆就是普通 Markdown，用户在设置面板里能看到具体条目、所在文件路径、重复项与统计，也能直接编辑底层文件。
2. **与 Prompt 注入解耦**：记忆文件被 `prompt.rs` 当作一个普通的上下文段落读入（见第 4 节），写入方与读取方只通过文件系统约定耦合。
3. **分层隔离作用域**：用户级偏好（跨项目）、项目级约束、单会话临时记忆、角色包专属记忆各自落在不同路径，互不污染。

`dream.rs` 的定位则是一个**轻量、用户触发**的整理器（`src-tauri/src/agent/dream.rs:1`）。注释里特别声明它「比完整后台 Auto Dream 系统刻意小」：没有后台调度、没有自动触发，只在用户输入 `/dream` 时跑一次「读取当前记忆 → 让模型合并整理 → 确认后覆盖写回 `.demiurge/memory.md`」。

> 当前状态提示：代码中**不存在**自动后台 Dream 调度器。`/dream` 是唯一入口，整理目标也只有 project scope 的 `.demiurge/memory.md` 这一个文件（其余三层不会被 `/dream` 覆盖）。

---

## 2. 四层 Scope：路径与数据结构

### 2.1 scope 定义

四层 scope 由 `scope_files()` 集中定义（`src-tauri/src/agent/memory.rs:312`），这是整个模块路径解析的唯一真相源：

| scope id | label | 存储路径 | 作用域语义 |
|----------|-------|----------|-----------|
| `user` | User | `data_dir/memory/user.md` | 跨项目、跨会话的用户级记忆 |
| `project` | Project | `sandbox_dir/.demiurge/memory.md` | 当前工作区/沙盒的项目级记忆（自动提取落盘目标） |
| `session` | Session | `sandbox_dir/.demiurge/session-memory/<session_id>.md` | 单会话临时记忆 |
| `pack` | Pack | `packs_dir/<pack_id>/memory.md` | 当前角色包专属记忆 |

其中 project 路径由 `memory_path()`（`src-tauri/src/agent/memory.rs:365`）统一拼成 `sandbox_dir/.demiurge/memory.md`。`.demiurge/` 是项目自有的工作目录约定。

`session_id` 在拼路径前会经过 `sanitize_path_segment()`（`src-tauri/src/agent/memory.rs:560`）清洗：只保留 ASCII 字母数字与 `-`、`_`，其余字符替换为 `_`，空串回退为 `"default"`。这一步是路径安全的关键——它阻止了用 `session_id` 做目录穿越（`../`）或写到沙盒外。例如测试 `adds_entries_to_user_session_and_pack_scopes`（`src-tauri/src/agent/memory.rs:632`）传入 `"session/1"`，最终落盘文件是 `session_1.md`。

`scope_files()` 的入参 `(data_dir, sandbox_dir, packs_dir, pack_id, session_id)` 全部由上层 `memory_context()`（`src-tauri/src/lib.rs:187`）从 `AppState` 里取出：`pack_id` 来自 `settings.current_pack`，`session_id` 来自当前活动会话。

### 2.2 对外暴露的类型

序列化给前端的结构（均派生 `Serialize`）：

- `MemoryEntry`（`src-tauri/src/agent/memory.rs:36`）：单条记忆。字段 `id`、`scope`、`scope_label`（序列化为 `scopeLabel`）、`kind`、`text`、`line`。`id` 形如 `"<scope>:mem-<行号>"`，例如 `project:mem-2`。
- `MemoryDuplicateGroup`（`src-tauri/src/agent/memory.rs:47`）：一个去重组，`canonical_id` 为保留项，`duplicate_ids` 为被判定重复的其余条目。
- `MemoryScopeState`（`src-tauri/src/agent/memory.rs:53`）：单 scope 的完整状态（id/label/path/entries/duplicates）。
- `MemoryPanelState`（`src-tauri/src/agent/memory.rs:62`）：审计面板顶层结构，含一个扁平化的 `entries`、`duplicates`，以及分 scope 的 `scopes`。

内部结构 `MemoryScopeFile`（`src-tauri/src/agent/memory.rs:70`）只在 Rust 侧使用，用 `&'static str` 的 id/label 加上解析好的 `PathBuf`。

### 2.3 行级 ID 设计与其副作用

`MemoryEntry.id` 由 `parse_entry_line()`（`src-tauri/src/agent/memory.rs:376`）生成为 `format!("{scope}:mem-{line_no}")`。**ID 直接绑定文件行号**，这是理解整个读写模型的关键：

- 优点：编辑/删除时无需在文件里维护稳定主键，`find_scope_file()`（`src-tauri/src/agent/memory.rs:347`）只要 `id.split_once(':')` 取出 scope 前缀就能定位文件，行号即定位锚点。
- 代价：ID **不稳定**。任何一次增删都会改变其后所有条目的行号，从而改变它们的 `id`。前端必须在每次写操作后用返回的最新 `MemoryPanelState` 全量刷新，不能缓存旧 `id` 再发第二个操作。这是「Markdown 即真相源」这一取舍的直接后果。

---

## 3. 核心数据流与算法

### 3.1 解析：Markdown 行 → MemoryEntry

`parse_entries()`（`src-tauri/src/agent/memory.rs:369`）逐行枚举，调用 `parse_entry_line()`。解析规则：

1. 行 `trim` 后必须以 `"- "` 开头，否则跳过（标题行 `# ...` 自然被忽略）。
2. 若剩余正文以 `[` 开头，按 `]` 切分出 `kind` 与 `text`，`kind` 经 `normalize_kind()` 归一；否则整行作为 `text`，`kind` 退化为 scope 名。
3. `text` 经 `sanitize_text()`（`src-tauri/src/agent/memory.rs:533`，把连续空白折叠成单空格）清洗，空则丢弃。

`normalize_kind()`（`src-tauri/src/agent/memory.rs:523`）只显式承认 `user / session / pack / preference` 四类，其余任何无法识别的 kind 都会**兜底归为 `project`**。

### 3.2 审计面板：panel_state

`panel_state()`（`src-tauri/src/agent/memory.rs:77`）是只读聚合入口，对应 Tauri 命令 `memory_panel_state`（`src-tauri/src/lib.rs:924`）。流程：

```
scope_files() ──► 逐 scope:
                    read_to_string(path)  // 文件不存在 → unwrap_or_default() 空串
                    parse_entries()       // 行 → MemoryEntry
                    audit_duplicates()    // 组内重复检测
                  ──► MemoryScopeState
                ──► 扁平化 entries / duplicates
                ──► path 字段取 project scope 路径（兼容旧前端）
```

注意顶层 `MemoryPanelState.path` 专门 `find(id == "project")` 取 project 路径（`src-tauri/src/agent/memory.rs:108`），是为兼容只认单一 project 路径的早期前端；新前端应改读 `scopes[].path`。读取对不存在的文件极其宽容：全程 `unwrap_or_default()`，缺文件等价于空 scope，不报错。

### 3.3 去重审计算法

`audit_duplicates()`（`src-tauri/src/agent/memory.rs:399`）在**单个 scope 内**做近似去重（不跨 scope）。比对前先用 `normalize_for_dedupe()`（`src-tauri/src/agent/memory.rs:537`）归一：去掉前导 `-`、剥掉 `[user]/[project]/[session]/[pack]/[preference]` 标签前缀、转小写、trim。

判重谓词 `is_duplicate_key()`（`src-tauri/src/agent/memory.rs:427`）：

```rust
a == b
  || (a.len() > 16 && b.contains(a))   // 长串 a 是 b 的子串
  || (b.len() > 16 && a.contains(b))   // 长串 b 是 a 的子串
```

即「完全相等」或「其中一方长度 > 16 且为另一方子串」才算重复。长度阈值是为了避免短词误判。算法对每条记忆找一个已存在的 canonical 组归入，否则自己成为新组的 canonical；最后只保留 `duplicate_ids` 非空的组。测试 `parses_and_audits_duplicate_memory_entries`（`src-tauri/src/agent/memory.rs:580`）验证了大小写变体被正确识别为重复。

> 复杂度提示：`audit_duplicates` 内层对每个组都重新 `entries.iter().find(canonical_id)` 再归一计算，整体是 O(n²·m) 级别。考虑到记忆文件被 `MAX_MEMORY_FILE_BYTES = 32 KiB`（`src-tauri/src/agent/memory.rs:19`）硬性封顶，规模可控。

### 3.4 手动维护：add / update / delete / dedupe

四个写操作都遵循同一套「读全文 → 改行 → 整写回 → 返回新 panel_state」的模式，全部经 `write_lines()`（`src-tauri/src/agent/memory.rs:431`）落盘（自动 `create_dir_all` 父目录、`join("\n")` 后补尾换行）。

- **add_entry**（`src-tauri/src/agent/memory.rs:121`，命令 `memory_add_entry`）：按 `scope_id` 定位文件；`text` 经 `sanitize_text` 清洗，空则报错 `Memory text cannot be empty`；若文件为空先补一行标题 `# {label} memory`；追加 `- [{kind}] {text}`。
- **update_entry**（`src-tauri/src/agent/memory.rs:153`）：经 `find_scope_file` 用 id 前缀定位 scope，解析出目标 entry，校验 `entry.line` 落在 `1..=lines.len()`，直接覆写该行为 `- [{kind}] {text}`。
- **delete_entry**（`src-tauri/src/agent/memory.rs:185`）：同样定位后 `lines.remove(entry.line - 1)`。
- **apply_dedupe**（`src-tauri/src/agent/memory.rs:210`，命令 `memory_dedupe_apply`）：遍历**全部四个 scope**，对每个文件用 `audit_duplicates` 收集所有 `duplicate_ids`，按行号过滤掉重复行后整写回。canonical 项保留，重复项删除。

注意 `update_entry` / `delete_entry` 的 scope 定位完全依赖 `id` 前缀（`find_scope_file` 在无 `:` 时兜底为 `project`），而 `add_entry` 用的是显式 `scope` 入参——这是二者签名不同的原因。

### 3.5 自动提取：extract_and_update

入口 `extract_and_update()`（`src-tauri/src/agent/memory.rs:243`）是个 async 函数，由 `runner.rs` 在「模型给出最终答复、无工具调用」的分支末尾以 `let _ =` 形式 fire-and-forget 调用（`src-tauri/src/agent/runner.rs:415`）——它的失败被刻意忽略，不影响主对话。

**触发门槛**（`src-tauri/src/agent/memory.rs:252`）：满足任一条件直接 `Ok(())` 跳过：

- `settings.auto_memory_enabled` 为 false（默认 true，见 `src-tauri/src/store/mod.rs:248`，前端开关在 `SettingsDialog.tsx:1950`）；
- 当前 provider `requires_api_key` 但 `api_key` 为空（`llm::ProviderProfile::for_kind`）；
- `cancel` 已被置位。

**提取流程**：

```
拼 turn_text = "User:\n{user}\n\nAssistant:\n{assistant}"
   └─ cap_chars 截到 MAX_INPUT_CHARS = 8000 字符
构造抽取 prompt（强约束：只留稳定信息；禁止记 secret/token/命令输出/堆栈；
   最多 3 条；每条 < 240 字；只输出 JSON {"memories":[{kind,text}]}）
llm::stream_completion(...)  // 复用主 LLM 客户端与 settings
若 cancel 或 finish_reason == "interrupted" → 放弃
parse_extraction()        // 容错解析（剥 ```json / ``` 围栏）→ MemoryExtraction
normalize_candidates()    // 去重 + 截断 → Vec<(kind, text)>
append_entries(sandbox_dir, ...)  // 仅追加到 project .demiurge/memory.md
```

**JSON 解析容错** `parse_extraction()`（`src-tauri/src/agent/memory.rs:442`）：先剥掉模型常见的 ```` ```json ```` / ```` ``` ```` 围栏再 `serde_json::from_str`。`MemoryExtraction`（`src-tauri/src/agent/memory.rs:24`）与 `MemoryCandidate` 字段全为可选/带 `#[serde(default)]`，对模型输出抗噪。

**候选归一** `normalize_candidates()`（`src-tauri/src/agent/memory.rs:455`）：

- 只取前 `MAX_MEMORIES_PER_TURN = 3` 条；
- `kind` 缺省 `"project"` 并归一，`text` 经 `sanitize_text`，空则丢；
- 用 `normalize_for_dedupe(text)` 做**本批次内**去重（`HashSet`）；
- 每条 `text` 截到 `MAX_MEMORY_CHARS = 240` 字符。

**落盘** `append_entries()`（`src-tauri/src/agent/memory.rs:474`）固定写 `sandbox_dir/.demiurge/memory.md`：

1. 若文件已 `> 32 KiB` 直接 `Ok(())` 放弃（防止记忆无限膨胀污染上下文）；
2. 读现有内容，把每行归一后塞进 `seen` 集合做**跨历史去重**；
3. 对每条候选构造 `- [{kind}] {text}`，要求其行形与裸 text 两种归一形都未出现过才加入；
4. 空文件先写标题 `# Automatic memory`，再追加；
5. 写之前再判一次总大小 `> 32 KiB` 则放弃。

> 设计要点：自动提取**只写 project scope**，绝不碰 user/session/pack。这与模块注释「自动提取仍只追加到 project scope」一致；更细粒度的自动归档被显式留作未来扩展点。

### 3.6 prompt 注入路径（读取侧）

记忆如何回流进对话：`prompt.rs` 的 `memory_section()`（`src-tauri/src/agent/prompt.rs:303`）调用 `scoped_memory_paths()`（`src-tauri/src/agent/memory.rs:299`，它是 `scope_files` 的 `(id, label, path)` 三元组投影），按 user/project/session/pack 顺序逐个 `read_limited_text` 读入，拼成 `# {label} memory ({scope})` 段落；之后**额外**读 `root/memory.md` 作为 `# Project legacy memory`（见第 6 节）。整段以 `"memories"` section（优先级 75）参与上下文预算裁剪（`src-tauri/src/agent/prompt.rs:128`）。

```
memory.rs (写)                prompt.rs (读)                 LLM
  user.md  ─┐                                                
  .demiurge/memory.md ─┼─► scoped_memory_paths() ─► memory_section() ─► 系统提示 "Memories" 段
  session-memory/*.md ─┤        + root/memory.md (legacy)
  packs/<id>/memory.md ┘
```

---

## 4. /dream 记忆整理流程

入口 `run_manual_dream()`（`src-tauri/src/agent/dream.rs:22`），在 `lib.rs` 的 slash 分流里命中 `/dream` 或 `/dream ...`（`src-tauri/src/lib.rs:310`）后调用。

### 4.1 整体状态机

```
/dream
  │ cancel.store(false)；把用户的 /dream 文本作为 user 消息入历史并持久化
  ▼
emit "开始整理长期记忆...\n\n"
  ▼
build_source_bundle()  // 汇集待整理材料（见 4.2）
  │
  ├─ 材料为空 ──► emit "没有找到可整理的记忆材料。" ──► finish ──► 返回
  ▼
构造中文整理 prompt（输出完整 memory.md；合并重复；删过时/一次性/寒暄；
   禁记 key/密码/token；相对时间改写为事实或删除；建议分四节）
  ▼
llm::stream_completion(...)   // 回调 |_| {} 不流式吐字，仅取最终 content
  │
  ├─ cancel / finish_reason == "interrupted" ──► assistant_interrupted ──► 返回
  ▼
normalize_memory_output()     // 剥 markdown 围栏、缺标题补 "# 自动记忆"
  │
  ├─ 空 ──────────────► emit "模型没有输出可用的记忆内容..." ──► finish
  ├─ > 32 KiB ────────► emit "整理后的记忆超过 32 KiB...跳过写入" ──► finish
  ├─ 与当前规范化后相同 ► emit "记忆已经足够干净..." ──► finish
  ▼
build_preview()  +  permission::audit("dream", ...)
  ▼
permission::confirm(...)  // 弹确认，Mutating，affected_paths=[".demiurge/memory.md"]
  │
  ├─ 拒绝 ──► emit "已取消写入，记忆文件保持不变。" ──► finish
  ▼
create_dir_all(parent) ──► fs::write(memory_path, next_memory)
  ▼
emit "记忆整理完成..." ──► finish
```

`emit_delta`/`finish`（`src-tauri/src/agent/dream.rs:181`、`189`）通过 `session_engine::TurnEventEmitter` 把进度作为一条 assistant 消息流式发给前端，并在结束时 `push_message` + `persist_sessions` 落库。

### 4.2 待整理材料 build_source_bundle

`build_source_bundle()`（`src-tauri/src/agent/dream.rs:233`）汇集多源：

| 来源 | 路径 | 说明 |
|------|------|------|
| 当前自动记忆 | `sandbox_dir/.demiurge/memory.md` | 整理基线，也是唯一写回目标 |
| 项目 memory.md | `sandbox_dir/memory.md` | legacy/项目根记忆 |
| 角色包 memory.md | `packs_dir/<current_pack>/memory.md` | pack scope |
| 项目 DEMIURGE.md | `sandbox_dir/DEMIURGE.md` | 项目说明文件 |
| 项目 SYSTEM.md | `sandbox_dir/SYSTEM.md` | 本项目自有的中性指令/记忆文件 |
| 会话快照 | `current_session_snapshot()` | 会话摘要 + 最近最多 12 条非 tool 消息 |

每个文件经 `read_limited_text()`（`src-tauri/src/agent/dream.rs:270`）读取：必须是文件且 `<= 32 KiB`，否则跳过。会话快照 `current_session_snapshot()`（`src-tauri/src/agent/dream.rs:200`）取 `session.summary` 与末尾 12 条消息（跳过 `role == "tool"` 与空内容），并截到 `MAX_INPUT_CHARS/3`。整个 bundle 用 `\n\n---\n\n` 连接，再整体截到 `MAX_INPUT_CHARS = 18000` 字符。

> 关于 `SYSTEM.md`：代码读取沙盒下的 `SYSTEM.md`（本项目自有的中性指令/记忆文件名，与 `DEMIURGE.md` 并列），仅作为整理材料的输入来源之一。

### 4.3 输出归一与写入闸门

`normalize_memory_output()`（`src-tauri/src/agent/dream.rs:278`）剥 ```` ```markdown ```` / ```` ```md ```` / ```` ``` ```` 围栏，空则返回空串；若正文不以 `#` 开头自动补 `# 自动记忆` 标题，保证尾换行。

在写盘前有三道闸门拦截无意义/危险写入：**空内容**、**超 `MAX_OUTPUT_BYTES = 32 KiB`**、**与当前内容规范化后相等**（`normalize_for_compare()` 折叠空白比较，`src-tauri/src/agent/dream.rs:301`）。只有都通过才进入确认环节。

### 4.4 与权限系统的交互

写回前必须经用户确认。决策由 `PermissionDecision::from_policy(PermissionPolicy::ask(...))` 构造，先 `permission::audit(state, "dream", &decision)` 记审计，再 `permission::confirm(...)`（`src-tauri/src/agent/dream.rs:135`）发出 `PermissionRequest`：

- `tool = "dream"`，`risk = ToolRisk::Mutating`；
- `affected_paths = [".demiurge/memory.md"]`；
- `preview` 由 `build_preview()`（`src-tauri/src/agent/dream.rs:305`）生成，含目标路径、当前/整理后的行数与字节数，以及截到 `PREVIEW_CHARS = 8000` 字符的整理后正文预览。

随后 `permission::remember_response(state, "dream", &response)` 记住用户选择。**只有 `response.allow` 为真才执行 `fs::write`**——这保证记忆覆盖永远不会在用户未确认时发生。

---

## 5. 与其他模块的交互边界

| 对端模块 | 交互方式 |
|----------|----------|
| `agent/runner.rs` | 在最终答复分支 fire-and-forget 调用 `extract_and_update`（`runner.rs:415`），失败被忽略 |
| `agent/prompt.rs` | 经 `scoped_memory_paths()` 读四层 + legacy，拼成 "Memories" 上下文段（`prompt.rs:303`） |
| `lib.rs` | 暴露 Tauri 命令：`memory_panel_state` / `memory_add_entry` / `memory_update_entry` / `memory_delete_entry` / `memory_dedupe_apply`（`lib.rs:923-984`）；`memory_context()` 统一供应五元组入参 |
| `lib.rs` `context_panel_state` | `context_memory_sources()`（`lib.rs:1169`）复用 `scoped_memory_paths` 并补一条 `project_legacy` 源，用于 Context 可视化的字符/token 统计 |
| `permission` | `/dream` 的 audit/confirm/remember_response 三步交互 |
| `llm` | 自动提取与 `/dream` 都经 `llm::stream_completion` 复用主 provider 与 `Settings` |
| `store::Settings` | `auto_memory_enabled`、`current_pack`、`provider`/`api_key` 控制触发与路径 |
| 前端 `SettingsDialog.tsx` | Memory 面板展示 scope/path/entries/duplicates，提供增改删与去重按钮；`auto_memory_enabled` 开关 |

---

## 6. project legacy 兼容

存在两条「project 记忆」路径，需区分清楚：

- **分层 project scope**：`sandbox_dir/.demiurge/memory.md`（`scope_files` 中的 `project`，也是自动提取与 `/dream` 的写入目标）。
- **project legacy**：`sandbox_dir/memory.md`（沙盒根目录下的旧位置）。

legacy 文件是**只读兼容加载**，不在 `scope_files` 列表里：

- `prompt.rs:316` 单独把 `root/memory.md` 读成 `# Project legacy memory` 段注入上下文；
- `lib.rs:1181` 的 `context_memory_sources` 单独把它列为 id 为 `project_legacy` 的源用于统计；
- `dream.rs:250` 把 `sandbox_dir/memory.md` 作为 `项目 memory.md` 纳入整理材料。

也就是说：旧路径的记忆**会被读取并整理进新路径**，但维护 API（add/update/delete/dedupe）和自动提取都不会向 legacy 路径写入。这是一条平滑迁移策略——老数据被持续吸收进 `.demiurge/memory.md`，而新写入统一收敛到分层位置。

---

## 7. 安全与权限相关点

1. **路径注入防护**：`sanitize_path_segment()` 清洗 `session_id`，杜绝用会话 id 做目录穿越。
2. **secret 不入记忆**：自动提取 prompt（`memory.rs:270`）与 `/dream` prompt（`dream.rs:62-64`）都显式禁止记录 API Key、密码、token、命令输出、堆栈等敏感内容。这是 prompt 层约束，非硬过滤——依赖模型遵守，无正则兜底。
3. **写入需确认**：`/dream` 覆盖写回前必须过 `permission::confirm`，`risk = Mutating`，并展示 diff 预览与受影响路径。
4. **大小硬上限**：记忆文件与 `/dream` 输出均以 32 KiB 封顶，防止上下文污染与无限膨胀。
5. **失败静默不阻塞**：`extract_and_update` 以 `let _ =` 调用，提取失败不影响主对话；`/dream` 各闸门均优雅跳过而非报错。

---

## 8. 已知限制与扩展点

- **无后台 Auto Dream**：仅 `/dream` 手动入口，无定时/事件触发的自动整理（`dream.rs:1` 注释已声明）。
- **`/dream` 只覆盖 project scope**：尽管 `build_source_bundle` 读多源材料，整理结果只写回 `.demiurge/memory.md`，user/session/pack 文件不被 `/dream` 改动。
- **自动提取只写 project**：`extract_and_update` 不会写 user/session/pack；更细粒度自动归档是显式扩展点（`memory.rs:2` 注释）。
- **行号 ID 不稳定**：增删后其后条目 id 改变，前端必须以返回的 `MemoryPanelState` 全量刷新，不能复用旧 id 连续操作。
- **去重是单 scope、近似子串**：`audit_duplicates` 不跨 scope；判重靠归一相等或长度 > 16 的子串包含，可能漏判语义重复或误判长子串。
- **secret 过滤纯靠 prompt**：没有正则/熵检测兜底，敏感信息防护依赖模型合规。
- **`/dream` 不流式**：`stream_completion` 回调为 `|_| {}`，整理过程不向 UI 吐字，只在结束时给出固定状态文案。
