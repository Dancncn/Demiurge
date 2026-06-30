# Skills 系统：Markdown 能力发现与注入

> 存档级技术原理文档
> 主源文件：`src-tauri/src/agent/skills.rs`
> 衔接点：`src-tauri/src/agent/prompt.rs:262`（`skills_section`）、`src-tauri/src/lib.rs:344`（slash 分流）、`src-tauri/src/lib.rs:1211`（`skill_panel_state` Tauri 命令）

## 一、模块职责与定位

Skills 系统是 Demiurge 的「轻量能力扩展」机制：开发者或用户在若干约定目录里放置 `SKILL.md` Markdown 文件，引擎在**每一轮对话组装 system prompt 时**自动发现这些文件、根据当前用户输入打分挑选最相关的若干条，把它们的正文与引用资料注入到 prompt 的 `Skills` 区块。

与 MCP（外部进程协议）和自定义 Agent（独立运行时）不同，Skill 不引入任何额外运行时——它**纯粹是注入到 system prompt 的文本片段**。一个 Skill 的全部「执行力」来自它写进 prompt 的指导语，以及它在 frontmatter 里声明的「需要哪些工具 / 需要哪些权限」这类元数据。因此 Skill 的设计目标是：

- **零部署成本**：放一个 Markdown 文件即生效，无需注册、编译或重启。
- **就地发现**：覆盖五类作用域目录，从全局应用数据到仓库内、再到项目内兼容目录（`.demiurge/compat/`）。
- **预算友好**：发现到的 Skill 可能很多，但注入到 prompt 的数量、单条正文长度、引用长度和总长度都有硬上限，避免上下文膨胀。
- **安全注入**：引用文件（references）只能是 skill 目录内的安全相对路径，杜绝目录穿越和绝对路径读取。

整个模块是**无状态、纯函数式**的：没有缓存、没有后台索引，每次 `context_for_turn` / `panel_state` 调用都重新扫描磁盘。这是刻意的简化——发现成本低（仅读目录 + 读受限大小的文本），换来「文件改了立刻生效」的直观语义。

### 关键常量（`skills.rs:8-13`）

| 常量 | 值 | 含义 |
| --- | --- | --- |
| `MAX_SKILL_FILE_BYTES` | 64 KiB | 单个 `SKILL.md` 允许读取的最大字节数，超限直接跳过 |
| `MAX_REFERENCE_FILE_BYTES` | 32 KiB | 单个 reference 文件允许读取的最大字节数 |
| `MAX_SELECTED_SKILLS` | 4 | 单轮最多注入的 Skill 数量 |
| `MAX_SKILL_BODY_CHARS` | 4 000 | 单条 Skill 正文注入时的字符上限 |
| `MAX_REFERENCE_CHARS` | 2 000 | 单条 reference 注入时的字符上限 |
| `MAX_CONTEXT_CHARS` | 14 000 | 整个 Skills 区块渲染后的字符上限 |

注意这些是 Skills 模块内部的「软封顶」，渲染结果交给 `prompt.rs` 后还会再受 prompt 总预算（`settings.max_context_chars`）的二次裁剪（详见第四节）。

## 二、关键类型与入口函数

### 2.1 数据类型

```rust
// skills.rs:15-29
pub struct SkillDefinition {
    pub id: String,                       // sanitize_id("{scope}-{name}")，面板/去重用
    pub name: String,                     // frontmatter name，缺省回退为目录名
    pub description: String,
    pub scope: SkillScope,                // 作用域来源
    pub skill_dir: PathBuf,               // SKILL.md 所在目录（references 解析基准）
    pub skill_path: PathBuf,              // SKILL.md 完整路径
    pub body: String,                     // frontmatter 之后的 Markdown 正文（已 trim）
    pub triggers: Vec<String>,            // triggers + keywords 合并去重
    pub declared_tool_needs: Vec<String>, // tools + declared_tool_needs 合并去重
    pub required_permissions: Vec<String>,
    pub references: Vec<String>,          // 已过滤为安全相对路径
    pub always_include: bool,
}
```

`SkillScope`（`skills.rs:31-53`）是六值枚举：`Global / Project / Repository / Pack / Compat / Legacy`，其 `label()` 返回小写串（`global`/`project`/…），既用于面板序列化（`#[serde(rename_all = "snake_case")]`），也用于排序与拼 `id`。

派生类型：

- `SkillSummary`（`skills.rs:55-68`）——面向前端面板的可序列化视图，额外带 `selected: bool` 和 `match_score: i32`。
- `SkillPanelState`（`skills.rs:70-74`）——`{ skills, diagnostics }`，即 `skill_panel_state` 命令的返回值。
- `SkillContext`（`skills.rs:76-79`）——只含 `text: String`，最终注入 prompt 的字符串。
- `SkillCatalog`（`skills.rs:81-85`）——发现阶段的中间产物 `{ skills, diagnostics }`。
- `SkillFrontMatter`（`skills.rs:87-105`）——`serde` 反序列化目标，多数字段是 `serde_yaml::Value`（默认 `Null`），以便后续用 `yaml_strings` 做宽松归一化。

### 2.2 三个对外入口

| 函数 | 位置 | 用途 |
| --- | --- | --- |
| `context_for_turn` | `skills.rs:111` | prompt 组装时调用，返回注入文本 |
| `panel_state` | `skills.rs:124` | 设置面板/前端检索，返回全量列表 + 选中标记 + 打分 |
| `slash_response` | `skills.rs:161` | `/skills`、`/skill` slash 命令的文本响应 |

三者都建立在两个内部核心函数之上：`discover`（发现）与 `select`（打分挑选）。区别在于「之后干什么」：

```
context_for_turn = discover → select → render_context（拼 prompt 文本）
panel_state      = discover → select → 对全量 skills 标记 selected/score → 排序
slash_response   = 解析 query → panel_state → format_panel（拼人类可读文本）
```

## 三、核心数据流与算法

### 3.1 发现阶段：`discover`（`skills.rs:183-223`）

`discover(sandbox, data_dir, packs_dir, pack_id)` 按**固定顺序**扫描五类目录：

```rust
// skills.rs:185-191
for (scope, base) in [
    (SkillScope::Global,     data_dir.join("skills")),
    (SkillScope::Project,    sandbox.join(".demiurge").join("skills")),
    (SkillScope::Repository, sandbox.join("skills")),
    (SkillScope::Pack,       packs_dir.join(pack_id).join("skills")),
    (SkillScope::Compat,     sandbox.join(".demiurge").join("compat").join("skills")),
] {
    discover_skill_dir(&mut catalog, scope, &base);
}
```

| 顺序 | scope | 基准目录 | 语义 |
| --- | --- | --- | --- |
| 1 | Global | `{app_data_dir}/skills/` | 用户级、跨项目通用能力 |
| 2 | Project | `{sandbox}/.demiurge/skills/` | 项目私有（本引擎约定的隐藏目录） |
| 3 | Repository | `{sandbox}/skills/` | 随仓库提交、对协作者可见 |
| 4 | Pack | `{packs_dir}/{pack_id}/skills/` | 绑定当前角色包（persona）的能力 |
| 5 | Compat | `{sandbox}/.demiurge/compat/skills/` | 项目内兼容能力入口 |

`sandbox` / `data_dir` / `packs_dir` 三个根来自 `AppState`（`lib.rs:58-60` 的 `data_dir` / `sandbox_dir` / `packs_dir`），`pack_id` 取自 `settings.current_pack`（`lib.rs:191` 等处）。兼容目录仅作为额外本地能力入口，引擎不对该来源做任何特殊优待。

> **重要：发现顺序不等于优先级。** 上面的 `for` 顺序只决定「先把谁 push 进 catalog」。`discover` 末尾（`skills.rs:220-221`）会用 `(scope.label(), name)` 重新整体排序，所以最终列表的物理顺序是按 scope 标签字母序再按名字排。真正影响「哪条被注入」的是后续 `select` 的打分，而非发现顺序。同名 Skill 也**不会**互相覆盖——不同 scope 的同名 Skill 会因 `id = "{scope}-{name}"` 不同而共存。

#### 单目录扫描：`discover_skill_dir`（`skills.rs:225-255`）

对每个基准目录，逻辑是：

1. 若目录不存在则直接返回（不报错，缺目录是正常情况）。
2. **直接子文件**：若 `base/SKILL.md` 存在，按该目录本身作为一个 Skill 读入。这允许「整个 skills 目录就是一条 Skill」的扁平布局。
3. **一级子目录**：遍历 `base` 下每个子目录，若子目录内有 `SKILL.md` 就读入。这是常规布局（`skills/<name>/SKILL.md`）。

注意只下探**一级**子目录，不递归。读取失败（如 frontmatter 非法）不会中断扫描，而是把错误信息 push 到 `catalog.diagnostics`（`skills.rs:234`、`:252`），随后会在 prompt 末尾或面板里以诊断条目展示。

#### Legacy 兼容（`skills.rs:195-217`）

在五类目录之后，`discover` 再尝试三个**单文件**遗留路径：

| label | 相对路径 |
| --- | --- |
| Project skills | `.demiurge/skills.md` |
| Repository skills | `skills/README.md` |
| Compat skills | `.demiurge/compat/skills.md` |

命中后构造一条 `SkillScope::Legacy` 的 `SkillDefinition`，整文件作为 `body`，且**强制 `always_include = true`**（`skills.rs:214`）——即只要存在就总会进入候选并优先注入。这是为了让早期的「单文件笔记式」技能不至于因没有 frontmatter triggers 而永远 0 分落选。

### 3.2 解析阶段：`read_skill` 与 frontmatter

`read_skill`（`skills.rs:257-315`）是把磁盘上的 `SKILL.md` 变成 `SkillDefinition` 的核心：

1. **受限读取**：`read_limited_text(path, MAX_SKILL_FILE_BYTES)`（`skills.rs:570-576`）先 `metadata` 检查是否为普通文件且 ≤ 64 KiB，超限或非文件返回 `None` → 整条跳过并记诊断。
2. **切分 frontmatter**：`split_frontmatter`（`skills.rs:317-335`）。
3. **YAML 反序列化**：`serde_yaml::from_str::<SkillFrontMatter>`，失败则返回 `Err`（被上层转为诊断）。
4. **字段归一化**（见下）。
5. **正文**：frontmatter 之后的内容 `.trim()` 作为 `body`。

#### `split_frontmatter` 细节（`skills.rs:317-335`）

- 先剥离 UTF-8 BOM（`\u{feff}`）。
- 必须以 `---\n` 或 `---\r\n` 起头才认为有 frontmatter；否则返回 `(None, 原文)`，整文件即正文（兼容无 frontmatter 的纯 Markdown）。
- 找下一处 `\n---` 作为结束；找不到则**报错**（「starts with --- but has no closing ---」）。
- 结束标记之后再宽松剥离 `---\r\n` / `---\n` / `---` 前缀，剩余即正文。同时兼容 LF 与 CRLF。

#### frontmatter 字段与归一化

`SKILL.md` 的 YAML frontmatter 支持以下字段（`SkillFrontMatter`，`skills.rs:87-105`）：

| YAML 键 | 去向 | 备注 |
| --- | --- | --- |
| `name` | `name` | trim 后非空则用，否则回退**目录名**（`skills.rs:268-280`） |
| `description` | `description` | trim |
| `triggers` | `triggers` | 与 `keywords` 合并后 sort+dedup |
| `keywords` | `triggers` | `triggers` 的别名 |
| `tools` | `declared_tool_needs` | 与 `declared_tool_needs` 合并后 sort+dedup |
| `declared_tool_needs` | `declared_tool_needs` | `tools` 的别名 |
| `required_permissions` | `required_permissions` | 不排序去重 |
| `references` | `references` | 经 `is_safe_relative` 过滤后保留 |
| `always_include` | `always_include` | bool，默认 `false` |

所有「列表型」字段都经过 `yaml_strings`（`skills.rs:337-354`）做**极宽松**的归一化，使写法非常自由：

- `Null` → 空。
- `String` → 走 `split_csv_like`（`skills.rs:356-363`），按逗号与换行切分、trim、去空。于是 `triggers: review, evidence` 与多行写法等价。
- `Sequence` → 递归展开每个元素（所以 `tools: [grep, read_file]` 列表写法可行）。
- `Mapping` → 取所有 key 字符串（容忍误把列表写成映射）。
- `Bool` / `Number` → 转成字符串。
- `Tagged` → 递归其内层值。

`declared_tool_needs` 与 `required_permissions` 当前**仅是声明性元数据**：它们被渲染进 prompt（让模型知道该 Skill「想用什么工具、需要什么权限」），并展示在面板里，但 Skills 模块本身**不会**据此去校验或自动授予权限——真正的权限裁决发生在权限模块。换言之，这些字段是给模型和用户看的提示，不是硬约束。

### 3.3 选择阶段：`match_score` 与 `select`

#### 打分函数 `match_score`（`skills.rs:386-423`）

输入 Skill 与用户文本，先各自 `normalize`（`skills.rs:425-434`：转小写、非字母数字替换为空格、压缩空白）。空查询时：`always_include` 给 1 分，否则 0 分。非空查询时累加：

| 命中规则 | 加分 |
| --- | --- |
| 整个 `name`（归一化后）作为子串出现在 query 中 | +8 |
| `name` 中每个 ≥3 字符的词出现在 query | +2 / 词 |
| `description` 中每个 ≥4 字符的词出现在 query | +1 / 词 |
| trigger 为 `*` 或 `always` | +2 |
| trigger（整串）作为子串出现在 query | +10 |
| 否则：trigger 中每个 ≥4 字符的词出现在 query | +2 / 词 |

可见 **trigger 整串命中权重最高（+10）**，其次是完整 name 命中（+8），description 词命中最弱（+1）。这条权重梯度鼓励作者用精确的 trigger 短语来「钩住」特定用户意图。

> 算法是纯字符串包含匹配（ASCII 小写化），**对中文等非 ASCII 文本不友好**：`normalize` 会把非 ASCII 字母数字字符当作分隔符替换为空格，因此中文 trigger 实际无法形成有效词去命中中文 query。这是当前实现的已知局限（见第六节）。

#### 挑选函数 `select`（`skills.rs:365-384`）

```
1. 对每条 skill 计算 match_score
2. 保留 (always_include || score > 0) 的条目
3. 按以下 key 升序排序：
     (!always_include, Reverse(score), scope.label(), name)
   ⇒ always_include 优先 → 高分优先 → scope 字母序 → name 字母序
4. truncate 到 MAX_SELECTED_SKILLS(=4)
```

也就是说：**所有 `always_include` 的 Skill 永远排在最前**（哪怕 0 分），其后才是按分数从高到低的命中项；并列时用 scope 标签和 name 做稳定排序。最后只取前 4 条。`select` 返回 `Vec<(&SkillDefinition, i32)>`，分数随条目一起带出，供渲染时显示。

> 注意一个边界：若 `always_include` 的 Skill 数量本身就 ≥ 4，则它们会占满全部名额，普通命中项即便高分也会被挤出。这是「always_include 优先级绝对高于分数」的直接后果。

### 3.4 渲染阶段：`render_context`（`skills.rs:452-499`）

把选中的 Skill 列表拼成注入文本。每条 Skill 产出一个 `## {name} [{scope} / score={score}]` 标题块，依次包含：

```
## Web Research [global / score=12]
Description: ...
Declared tool needs: web_fetch, web_search   ← 仅在非空时
Required permissions: ...                     ← 仅在非空时
Source: /path/to/SKILL.md

### SKILL.md
<body，经 cap_chars(MAX_SKILL_BODY_CHARS=4000) 截断>

### References                                ← 仅在有可读 reference 时
<render_references 输出>
```

若 `catalog.diagnostics` 非空，最后追加一个 `## Skill diagnostics` 块（最多 5 条，`skills.rs:487-497`）。整体 `join("\n\n")` 后再过一次 `cap_chars(MAX_CONTEXT_CHARS=14000)`。

`cap_chars`（`skills.rs:578-586`）按 **Unicode 字符数**（非字节）截断，超限时尾部追加 `\n[truncated]` 标记，并预留该标记长度。

#### references 注入：`render_references`（`skills.rs:501-518`）

对每个 reference：

1. **二次安全校验** `is_safe_relative`（解析时已过滤一次，这里再防御性校验一次）。
2. 以 `skill.skill_dir.join(reference)` 解析为实际路径——**基准是 SKILL.md 所在目录**，这正是 `skill_dir` 字段存在的意义。
3. `read_limited_text(..., MAX_REFERENCE_FILE_BYTES=32 KiB)`，读不到（不存在 / 超大 / 非文件）就静默跳过。
4. 产出 `#### {reference}\n{cap_chars(text, 2000)}`。

### 3.5 面板与 slash 输出

`panel_state`（`skills.rs:124-159`）：先 `discover + select`，再对**全量** `catalog.skills` 逐条生成 `SkillSummary`——选中项沿用 `select` 算出的分数，未选中项即时调用 `match_score(skill, query)` 补算分数并标 `selected=false`。最后用 `(selected desc, match_score desc, scope.label asc, name asc)` 排序。这样前端既能看到「这轮会真正注入的 4 条」，也能看到其余 Skill 各自的分数。

`slash_response`（`skills.rs:161-181`）处理 `/skills` 和 `/skill`：

- 从 `AppState` 取 sandbox/data/packs/settings。
- 用 `strip_prefix("/skills")`，失败再 `strip_prefix("/skill")`，剩余 trim 作为 query；为空则 `None`。
- 调 `panel_state`，再 `format_panel`（`skills.rs:520-568`）拼成人类可读文本。

`format_panel` 输出形如：

```
Skills matching `web`:
- `Web Research` [global score=12] selected - Search and fetch current web sources.
  triggers: search, web
  tools: web_fetch, web_search
...
Diagnostics:
- <最多 10 条>
```

skills 为空时给出一段引导文案，列出五类可放置目录（`skills.rs:527`）。列表最多展示 40 条，诊断最多 10 条。

> **关于 `//skill`**：当前代码**不存在** `//skill`（双斜杠）处理分支。`lib.rs:344-348` 的分流条件只匹配 `/skills`、`/skills `、`/skill`、`/skill `；`skills.rs:168-169` 也只 strip `/skills`/`/skill` 前缀。文档中若提及 `//skill` 应理解为对单斜杠 `/skill` 的笔误，引擎并无双斜杠语义。

## 四、与其他模块的交互边界

### 4.1 prompt 组装衔接

Skills 通过 `prompt.rs:262-270` 的 `skills_section` 接入 system prompt——它只是把 `context_for_turn(...).text` 取出。该 section 在 `build_ordered_sections`（`prompt.rs:103-145`）里注册为：

```rust
section("skills", "Skills", 60, skills_section(root, data_dir, packs_dir, &settings.current_pack, user_text))
```

`user_text` 是**本轮用户输入**，正是 `match_score` 的打分依据——这意味着 Skill 的选择是「逐轮、随输入变化」的。

各 section 带 `priority`，由 `assemble_drafts`（`prompt.rs:156-235`）按预算装配。Skills 的优先级是 **60**，在同批 section 中相对偏低：

| section | priority |
| --- | --- |
| safety_rules | 95 |
| pack_persona | 90 |
| tools | 85 |
| project_instructions | 80 |
| memories | 75 |
| current_goal | 70 |
| conversation_summary | 65 |
| **skills** | **60** |
| environment | 55 |

`assemble_drafts` 按 priority 从高到低分配剩余字符预算（`settings.max_context_chars` 扣除 base 后）。**这是 Skills 区块的二次裁剪**：即使 `render_context` 已把文本压到 ≤14 000 字符，若装配到 Skills 时预算所剩不多，整块可能被截断（`truncate_with_note`）甚至完全丢弃（`included=false`）。由于优先级仅高于 environment，Skills 是较早被牺牲的区块之一。这是有意取舍：安全规则、persona、工具定义、项目指令等「硬约束」比「能力提示」更不可省。

数据流总览：

```
用户输入 user_text
   │
   ▼
prompt::build_* ──► build_ordered_sections ──► skills_section
   │                                                │
   │                                                ▼
   │                          skills::context_for_turn(sandbox,data,packs,pack_id,user_text)
   │                                                │
   │                          discover ─► select(打分挑≤4) ─► render_context(≤14k 字符)
   │                                                │
   ▼                                                ▼
assemble_drafts(按 priority+预算裁剪) ◄──── "Skills" section 文本(priority=60)
   │
   ▼
最终 system prompt
```

### 4.2 slash 命令分流

`lib.rs:344` 的分支在用户消息进入正常 LLM 流程**之前**拦截 `/skills`/`/skill`，调用 `slash_response` 得到文本后直接 `events.assistant_done(body)` 回显，不触发模型推理。这是一个纯查询型 slash，用于让用户检查「当前会被推荐/注入哪些 Skill」。

### 4.3 前端面板命令

`skill_panel_state`（`lib.rs:1211-1221`）是 Tauri 命令，注册于 `lib.rs:1731` 的 invoke handler 列表。它接收可选 `query`，trim 后转给 `skills::panel_state`，供设置页的 Skills 面板渲染列表、分数与选中态。配套还有 `open_skills_dir`（`lib.rs:1225` 起），用于在文件管理器中打开全局 `skills/` 目录（不存在时先 `create_dir_all`）。

## 五、安全与权限相关点

1. **references 路径沙箱化**——核心安全保证由 `is_safe_relative`（`skills.rs:608-619`）提供：
   - 空串、纯空白 → 拒绝。
   - 绝对路径（含 Windows 盘符如 `C:/...`）→ 拒绝（`path.is_absolute()`）。
   - 路径组件必须全部是 `Component::Normal` 或 `Component::CurDir`（`.`）；出现 `..`（`ParentDir`）、根（`RootDir`）、前缀（`Prefix`）即拒绝。
   这样 reference 只能指向 `skill_dir` 内部，无法穿越到 skill 目录之外。测试 `rejects_unsafe_reference_paths`（`skills.rs:728-736`）覆盖了 `../secret.md`、`refs/../../secret.md`、`C:/secret.md` 等。该校验做**两道**：解析时过滤一次（`skills.rs:296-299`），渲染时再校验一次（`skills.rs:504`）。

2. **大小封顶防 DoS / 防上下文爆炸**——`read_limited_text` 对 `SKILL.md`（64 KiB）和 reference（32 KiB）都先查 `metadata().len()` 再读，避免被超大文件拖垮；`cap_chars` 与 `MAX_SELECTED_SKILLS` 进一步限制注入体量。

3. **声明性权限不等于授权**——`required_permissions` / `declared_tool_needs` 只被渲染进 prompt 与面板，Skills 模块**不执行任何授权动作**。真正的工具调用仍走权限模块裁决。读者不应误以为写了 `required_permissions: read_only` 就限制了 Skill 的实际能力。

4. **解析失败不致命**——非法 frontmatter / 不可读文件只生成诊断条目，不会 panic 也不会阻断其他 Skill 的发现。

## 六、已知限制与扩展点

- **匹配算法对非 ASCII 不友好**：`normalize` 把非 ASCII 字母数字一律当分隔符，导致中文 name/trigger/description 难以与中文 query 形成有效子串/词命中。中文场景下基本只能依赖 `always_include` 或 ASCII trigger。可考虑引入 Unicode 分词或保留 CJK 字符。
- **无缓存、全量重扫**：每轮 `context_for_turn` 都重新遍历五类目录并读文件。Skill 极多时有 I/O 开销，但当前规模下可接受；如需优化可加 mtime 缓存。
- **只下探一级子目录**：`discover_skill_dir` 不递归，深层嵌套的 `SKILL.md` 不会被发现。
- **`always_include` 可挤占名额**：≥4 个 `always_include` Skill 会占满 `MAX_SELECTED_SKILLS`，使高分命中项落选（见 3.3）。
- **打分纯子串、无 IDF/语义**：常见短词可能误命中；没有同义词、词干或语义相似度。属可替换的策略点。
- **`declared_tool_needs` / `required_permissions` 尚未接入权限闭环**：当前是纯提示性元数据，未与权限模块联动做自动放行或拦截。这是一个明确的预留扩展点。
- **Legacy 单文件来源固定**：`.demiurge/skills.md`、`skills/README.md`、`.demiurge/compat/skills.md` 三条路径写死，且强制 `always_include`。

---

### 附：一条 SKILL.md 的最小示例（取自模块测试 `skills.rs:642-654`）

```markdown
---
name: Evidence Review
description: Review code with evidence.
triggers:
  - review
  - evidence
tools: [grep, read_file]
required_permissions: read_only
references:
  - refs/checklist.md
---
Use evidence before conclusions.
```

解析结果：`triggers = ["evidence", "review"]`（sort+dedup）、`declared_tool_needs = ["grep", "read_file"]`、`references = ["refs/checklist.md"]`（安全相对路径通过），正文为最后一行。
