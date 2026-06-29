# 角色包系统

> 存档级技术原理文档。覆盖角色包的清单校验、persona 注入、头像 data URL 生成、zip 导入安全校验、默认包落地，以及角色包作为 memory / skills 作用域载体的衔接逻辑。
>
> 主要源文件：
> - `src-tauri/src/pack/mod.rs`
> - `src-tauri/src/agent/persona.rs`
> - 衔接点：`src-tauri/src/agent/prompt.rs`、`src-tauri/src/agent/memory.rs`、`src-tauri/src/agent/skills.rs`、`src-tauri/src/lib.rs`

---

## 1. 模块职责与定位

角色包（Pack）是 Demiurge 用来描述「这台桌面伴侣此刻扮演谁」的最小可分发单元。一个角色包在磁盘上就是 `packs/<id>/` 下的一个目录，至少包含 `manifest.json`（清单）与一份 persona 正文（默认 `persona.md`），可选地携带头像、`memory.md`、`skills/` 子目录。

模块边界划得很清楚，分成两层：

- **`pack/mod.rs`**：纯文件系统层。负责清单的解析与校验、persona 正文读取、头像编码成 data URL、zip 包的导入与安全落地、首启动时落地默认包。它**不**关心这些内容如何拼进 system prompt。
- **`agent/persona.rs`**：仅持有「引擎基础指令」常量 `ENGINE_BASE`（`src-tauri/src/agent/persona.rs:4`），它是与具体角色无关的通用规则。注意：本文件**不读取角色包**，它和角色包是 prompt 装配阶段才汇合的两块输入。

这种拆分的设计意图是：角色包是可被用户替换、可被第三方制作分发的「皮」，而引擎规则是不可被角色包覆盖的「骨」。两者在 `agent/prompt.rs` 中以不同优先级合成，保证再花哨的角色设定也无法改写工具/安全约束。

模块顶部注释明确把当前定位写成「MVP 文本版清单，格式预留可成长字段（Live2D / TTS / 表情等）」（`src-tauri/src/pack/mod.rs:1`）。也就是说，清单结构本身就是为将来扩展预留的，现阶段只落地了 `id` / `name` / `persona` / `avatar` 四个字段。

---

## 2. 关键类型与入口函数

### 2.1 清单与运行时类型

```rust
// src-tauri/src/pack/mod.rs:12
pub struct PackManifest {
    pub id: String,
    pub name: String,
    pub persona: String,              // persona 文件名（相对包目录）
    pub avatar: Option<String>,       // 可选头像相对路径
    pub avatar_data_url: Option<String>, // 运行时填充，rename = "avatarDataUrl"
}
```

`avatar_data_url` 是一个**派生字段**：磁盘清单里不存在它（`#[serde(... skip_serializing_if = "Option::is_none")]`），而是在 `read_manifest_with_avatar` 阶段把磁盘头像读出来、编码成 data URL 后填回内存对象，供前端直接 `<img src>` 渲染。这避免了前端再去走一次 Tauri 命令读图片字节，也避免暴露本地绝对路径。

```rust
// src-tauri/src/pack/mod.rs:28
pub struct Pack {
    pub manifest: PackManifest, // #[allow(dead_code)]，目前只用 persona_text
    pub persona_text: String,
}
```

### 2.2 对外入口（被 `lib.rs` 的 Tauri 命令调用）

| 函数 | 位置 | 职责 | 调用方 |
| --- | --- | --- | --- |
| `ensure_default` | `pack/mod.rs:47` | 首启动落地 `packs/default` | `lib.rs:1665`（setup） |
| `list_packs` | `pack/mod.rs:62` | 枚举所有合法角色包 | Tauri 命令 `list_packs`（`lib.rs:777`） |
| `load_pack` | `pack/mod.rs:85` | 按 id 读清单 + persona 正文 | runner / subagent / context panel |
| `import_zip` | `pack/mod.rs:97` | 导入并安全落地 zip 角色包 | Tauri 命令 `import_pack_zip`（`lib.rs:783`） |

`persona.rs` 侧的入口仅有 `engine_base()`（`src-tauri/src/agent/persona.rs:17`），返回 `&'static str`。

`packs_dir` 的真实根目录在应用启动时确定为 `app_data_dir()/packs`（`src-tauri/src/lib.rs:1663`），随后 `ensure_default` 在其下创建 `default` 子目录。`AppState` 用 `Mutex<PathBuf>` 持有该路径（`src-tauri/src/lib.rs:60`）。

---

## 3. 核心数据流与算法

### 3.1 清单的三段式校验

校验被有意拆成三个正交函数，分别负责「身份」「相对路径安全」「最终落地存在性」，这样在不同阶段（解析时 / 解压前 / 解压后）可以按需复用：

1. **身份校验** `validate_manifest_identity`（`pack/mod.rs:174`）
   - `id` 不能为空、不能有首尾空白（`id != manifest.id` 的对比就是在拒绝「trim 后才合法」的输入）。
   - `id` 只允许 ASCII 字母数字与 `-` `_`（`pack/mod.rs:179`）。这是**第一道目录穿越防线**：因为 `id` 会直接拼成落地目录名 `packs/<id>`，所以 `.`、`/`、`\` 全部被禁，`../bad` 这类输入在解析阶段就被打回（对应测试 `validates_manifest_identity_and_paths`，`pack/mod.rs:393`）。
   - `name` 不能为空白。

2. **相对路径校验** `validate_manifest_paths` → `validate_relative_file`（`pack/mod.rs:191`、`pack/mod.rs:202`）
   - 对 `persona` 必校验，对 `avatar` 在存在时校验。
   - 拒绝绝对路径；逐 `Path::components()` 检查，只允许 `Normal` 与 `CurDir`（`.`），任何 `ParentDir`（`..`）、`RootDir`、`Prefix`（Windows 盘符）都返回「包含非法路径组件」。这是**第二道穿越防线**，作用在清单里声明的相对文件名上。
   - 对 `avatar` 还会调 `avatar_mime` 校验扩展名白名单（仅 png/jpg/jpeg/webp/gif，`pack/mod.rs:238`），不在白名单（如 `avatar.svg`）直接报错。

3. **落地存在性校验** `validate_extracted_pack`（`pack/mod.rs:333`）：仅在 zip 导入解压后调用，确认 `manifest.json`、`persona`、（若声明了）`avatar` 三者真的落到了临时目录里。

`parse_manifest`（`pack/mod.rs:167`）= `serde_json::from_str` + 身份校验，是清单进入系统的统一入口；`read_manifest_with_avatar`（`pack/mod.rs:153`）= 读盘 + `parse_manifest` + 路径校验 + 头像编码，是「从已落地目录加载清单」的统一入口。

### 3.2 persona 注入：从磁盘到 system prompt

`load_pack`（`pack/mod.rs:85`）读清单后，用 `resolve_pack_file(dir, manifest.persona, "persona")`（`pack/mod.rs:220`）把相对 persona 文件名安全拼成绝对路径，再 `read_to_string` 得到正文。`resolve_pack_file` 内部会再跑一次 `validate_relative_file`，即使清单是从可信目录读出来的，也不省略这道校验。

persona 正文进入 prompt 的完整链路：

```
runner.rs:183 / subagent.rs:337 / lib.rs:1027
  pack::load_pack(packs_dir, settings.current_pack).persona_text
        │  （load 失败时 runner/subagent 回退为空串，见 runner.rs:185）
        ▼
prompt::build_with_report_for_input(..., persona_text, ...)   prompt.rs:77
        ▼
build_ordered_sections → section("pack_persona", "Pack Persona", 90, persona_section(persona_text))   prompt.rs:115
        ▼
assemble_drafts(persona::engine_base(), drafts, max_context_chars)   prompt.rs:100
```

几个关键设计点：

- **persona 是一个「分区草稿」，优先级 90**（`prompt.rs:115`）。`engine_base()` 不参与分区竞争——它作为 `assemble_drafts` 的 `base` 永远整段置顶（`prompt.rs:156`、`prompt.rs:200`），不会被裁剪。各分区按优先级降序争夺剩余字符预算（`prompt.rs:162`），不够时高优先级分区先入选、临界分区被 `truncate_with_note` 截断（`prompt.rs:189`）。安全规则（priority 95）与工具说明（85）都高于或接近 persona，意味着**在上下文吃紧时角色设定可能先于安全/工具被牺牲**，而非反过来。
- `persona_section` 仅做 `trim()`（`prompt.rs:254`），不加任何包装标题——标题 `Pack Persona` 由分区框架统一加。
- persona 正文是**纯文本注入**，不做模板替换、不解析 frontmatter。角色包作者写什么就注入什么（受预算裁剪约束）。

### 3.3 头像 data URL 生成

`avatar_data_url`（`pack/mod.rs:225`）：

```
avatar_mime(path)  →  None 则报错（理论上不会，路径校验已过白名单）
fs::read(path)     →  空文件报「avatar 文件为空」
format!("data:{mime};base64,{}", STANDARD.encode(bytes))
```

只有 `manifest.avatar` 声明了、且文件真实存在时才生成（`read_manifest_with_avatar` 内 `if avatar_path.exists()`，`pack/mod.rs:160`）。生成的 data URL 写入内存 `avatar_data_url` 字段返回前端。注意头像没有大小上限校验（`MAX_IMPORT_BYTES` 只在 zip 导入阶段约束整包），一张巨大的头像会被整体 base64 进 panel 状态——见 §6 已知限制。

### 3.4 zip 导入流水线（安全校验的核心）

`import_zip`（`pack/mod.rs:97`）是整个模块安全性最集中的地方，按以下顺序设防：

```
┌─ 入口快速失败 ───────────────────────────────────────┐
│ bytes 为空           → "角色包 zip 为空"             │  pack/mod.rs:102
│ bytes > 25 MiB       → "过大"（MAX_IMPORT_BYTES）    │  pack/mod.rs:105
│ 文件名非 .zip 结尾   → 拒绝                          │  pack/mod.rs:111
└──────────────────────────────────────────────────────┘
        ▼
ZipArchive::new(Cursor::new(bytes))                      pack/mod.rs:116
        ▼
find_manifest_entry  ── 必须恰好一个 manifest.json       pack/mod.rs:118 / 253
        │  0 个 → "缺少 manifest.json"
        │  >1 个 → "只能包含一个 manifest.json"
        ▼
prefix = manifest_path 去掉 "manifest.json" 后缀         pack/mod.rs:119
        （支持包根带一层目录，如 demo/manifest.json → prefix="demo/"）
        ▼
read_zip_text(manifest)  ── size > 256 KiB 直接拒绝      pack/mod.rs:123 / 280
        ▼
parse_manifest + validate_manifest_paths                 pack/mod.rs:124-125
        ▼
dest = packs_dir/<id>;  已存在 → 拒绝（防重复 id 覆盖） pack/mod.rs:127
        ▼
temp = packs_dir/.import-<id>-<随机 session id>          pack/mod.rs:131
        ▼
extract_archive(prefix → temp)                           pack/mod.rs:141 / 289
   └─ validate_extracted_pack(temp)                      pack/mod.rs:142 / 333
   └─ fs::rename(temp → dest)  ── 原子落地              pack/mod.rs:144
   └─ read_manifest_with_avatar(dest)  ── 回读带头像    pack/mod.rs:145
        ▼
任一步出错 → fs::remove_dir_all(temp)  ── 清理临时目录  pack/mod.rs:147
```

每一项校验对应一类威胁：

| 校验 | 防御目标 | 源码位置 |
| --- | --- | --- |
| `bytes.is_empty()` | 空包 | `pack/mod.rs:102` |
| `> MAX_IMPORT_BYTES`（压缩前 25 MiB） | 超大包 / 内存放大 | `pack/mod.rs:105` |
| 单一 `manifest.json` | 多清单歧义 / 混入伪装清单 | `pack/mod.rs:266` |
| `manifest.json` 解压前 `size > 256 KiB` 拒绝 | 解析阶段内存炸弹 | `pack/mod.rs:280` |
| `dest.exists()` 拒绝 | 重复 id 覆盖既有包 | `pack/mod.rs:128` |
| `normalized_zip_name` + `validate_relative_file` | **zip-slip（`../` 逃逸）** | `pack/mod.rs:303`、`pack/mod.rs:311`、`pack/mod.rs:351` |
| `extract_archive` 累计 `files > 100` | 文件数炸弹 | `pack/mod.rs:313`（`MAX_IMPORT_FILES`） |
| `extract_archive` 累计 `total > 25 MiB` | **解压炸弹（zip-bomb）** | `pack/mod.rs:317` |

**zip-slip 防御细节**：`normalized_zip_name`（`pack/mod.rs:351`）先把 `\` 统一成 `/`（处理 Windows 制作的 zip），去掉尾部 `/` 后调 `validate_relative_file` 拒绝任何 `..` 组件，再去掉前导 `./`。`extract_archive` 里在 `dest.join(rel)` 之前，对去 prefix 后的相对路径 `rel` 再跑一次 `validate_relative_file`（`pack/mod.rs:311`），双重保险。对应测试 `rejects_zip_slip_entries`（`pack/mod.rs:427`）构造了 `../persona.md` 条目并断言导入失败。

**解压计数是「解压后体积」的累计**：用 `file.size()`（解压后大小）`saturating_add` 累加（`pack/mod.rs:316`），所以即便压缩比极高，解压超过 25 MiB 也会中途失败，这正是抵御 zip-bomb 的关键——只校验压缩前的 `bytes.len()` 是不够的。

**临时目录 + 原子 rename 的设计意图**：先解压到 `.import-<id>-<random>`，全部校验通过后才 `fs::rename` 到正式 `packs/<id>`。这样导入要么完整成功、要么完全不留痕（出错路径 `remove_dir_all(temp)`，`pack/mod.rs:148`），不会出现「解压一半、`packs/<id>` 里是残包」的中间态。随机后缀用 `crate::store::new_session_id()`（`pack/mod.rs:134`）避免并发导入撞名。

### 3.5 默认包落地

`ensure_default`（`pack/mod.rs:47`）在 `packs/default` 下幂等地写 `manifest.json` 与 `persona.md`（`!exists()` 才写，不覆盖用户已改的内容）。默认清单 `DEFAULT_MANIFEST`（`pack/mod.rs:35`）的 `name` 是中立的 `"Demiurge"`，persona `DEFAULT_PERSONA`（`pack/mod.rs:41`）是一段通用的「桌面伴侣」人设，注释强调「通用、不绑定任何特定角色」（`pack/mod.rs:34`）。这保证了首启动即有一个可用 `current_pack=default`，runner 不会因为找不到包而拿到空 persona。

### 3.6 列举与排序

`list_packs`（`pack/mod.rs:62`）遍历 `packs_dir` 下所有子目录，对每个目录尝试 `read_manifest_with_avatar`，失败的目录（含临时 `.import-*` 目录、无清单目录）被静默跳过。结果按 `name` 再按 `id` 排序（`pack/mod.rs:76`），保证 UI 列表稳定。

---

## 4. 与其他模块的交互边界

```
                ┌────────────────────────────────────────────┐
                │  packs/<current_pack>/                       │
                │    manifest.json / persona.md                │
                │    memory.md          ← memory.rs 的 pack 作用域 │
                │    skills/*/SKILL.md  ← skills.rs 的 Pack 作用域 │
                │    avatar.png         ← data URL              │
                └────────────────────────────────────────────┘
                       │persona            │memory.md         │skills/
                       ▼                   ▼                  ▼
   pack::load_pack   memory::scope_files   skills::discover
                       │                   │                  │
                       └──────────┬────────┴──────────────────┘
                                  ▼
                    agent/prompt.rs build_ordered_sections
                       pack_persona(90) / memories(75) / skills(60) ...
                                  ▼
                    assemble_drafts(engine_base, ...) → system prompt
```

角色包不是只有 persona 一块内容会进 prompt，它同时是 memory 和 skills 的一个**作用域载体**，三条线在 `prompt.rs` 汇合，但 key 都是 `settings.current_pack`：

- **persona**：`load_pack().persona_text` → `pack_persona` 分区（见 §3.2）。
- **pack memory**：`memory::scope_files` 把 `packs_dir.join(pack_id).join("memory.md")` 作为 `id="pack"` 作用域（`src-tauri/src/agent/memory.rs:339`）。`prompt::memory_section` 通过 `scoped_memory_paths`（`memory.rs:299`）读取 user/project/session/pack 四层记忆，pack 层即角色包自带的 `memory.md`。Settings 的记忆面板也能对 pack scope 做增删改查与去重（命令在 `lib.rs:925` 一带）。
- **pack skills**：`skills::discover` 把 `packs_dir.join(pack_id).join("skills")` 作为 `SkillScope::Pack`（`src-tauri/src/agent/skills.rs:189`），与 global / project / repository / `.claude` 兼容目录并列发现（`skills.rs:185`）。`prompt::skills_section` 经 `skills::context_for_turn`（`skills.rs:111`）按当前用户输入选择性注入。

**衔接含义**：一个第三方角色包 zip 完全可以同时携带 `persona.md`、`memory.md` 和 `skills/<name>/SKILL.md`。导入后，只要把 `current_pack` 切到该 id，这三类内容会自动随角色一起进入 prompt——角色包因此不只是「换人设」，而是「换一整套人设 + 预置记忆 + 专属技能」。需要注意：`import_zip` 的解压只做相对路径与体积/数量校验，并不强制要求或特殊处理 `memory.md`/`skills/`，它们就是普通文件，被原样落地后由 memory/skills 模块在 prompt 装配时各自发现。

调用 `load_pack` 的三个上游：`runner.rs:183`（主对话）、`subagent.rs:337`（只读子 Agent）、`lib.rs:1027`（Context 可视化面板）。前两者在 `load_pack` 失败时回退为空 persona 继续运行（`runner.rs:185`、`subagent.rs:339`），保证角色包损坏不会让对话彻底不可用；context panel 用 `unwrap_or_default()` 同样容错（`lib.rs:1029`）。

---

## 5. 安全与权限相关点

1. **目录穿越的两层防御**：`id` 字符白名单（拼目录名用）+ 相对路径组件校验（拼包内文件用），加上 zip 条目名归一化后的再校验。三处都用同一个 `validate_relative_file`，逻辑集中、不易出现某条路径漏校验。
2. **资源耗尽防御**：压缩前 25 MiB、解压后累计 25 MiB、文件数 100、清单 256 KiB，四道闸覆盖空包 / 超大包 / 解压炸弹 / 解析炸弹。
3. **原子性与无残留**：临时目录 + `rename` + 失败清理，避免半成品包污染 `packs/`。
4. **重复 id 拒绝而非覆盖**：`dest.exists()` 即报错（`pack/mod.rs:128`），不会静默覆盖用户已有同名包。
5. **头像不落地为可执行/可注入内容**：头像只被读成 base64 data URL，扩展名白名单排除了 SVG（可含脚本）等。
6. **角色包无法越权改写引擎规则**：`engine_base()` 作为不参与裁剪的 `base` 置顶，安全规则分区优先级（95）高于 persona（90），结构上保证角色设定不能凌驾于安全/工具约束之上。
7. **不暴露本地绝对路径**：头像走 data URL 而非文件路径回前端。

---

## 6. 已知限制与扩展点

- **清单是 MVP 文本版，多数「成长字段」尚未落地**。源码注释列出的 Live2D / TTS / 表情等（`pack/mod.rs:1`）目前在 `PackManifest` 中**没有任何对应字段**，属预留方向而非已实现能力。请勿在文档中把它们描述为已支持。
- **语音（TTS/ASR）后端未接通**。角色包当前无法携带语音配置；`voice.rs` 的语音命令面板在设置中可见但后端未接入（与 `docs/IMPLEMENTATION.md:101` 的现状一致）。角色包与语音的衔接是未来扩展点，现状为空。
- **头像无大小上限**。`MAX_IMPORT_BYTES` 只约束 zip 导入；对一个已落地包，`avatar_data_url` 会把整张图 base64 进内存/panel 状态，超大头像可能放大 IPC 负载。
- **`manifest.id` 与目录名强绑定**。导入后 id 不可改名（改名等于新包），且大小写/同名冲突依赖文件系统语义。
- **`Pack.manifest` 字段当前未被消费**（`#[allow(dead_code)]`，`pack/mod.rs:29`），上游只取 `persona_text`；将来若 prompt 要用 `name`/`avatar` 需打通这条路径。
- **自动记忆归档不会写入 pack 作用域**。自动 memory extraction 优先写 project scope（`docs/IMPLEMENTATION.md:238`）；pack 层 `memory.md` 目前主要靠角色包自带或用户在记忆面板手动维护。
- **扩展建议**：新增清单字段时应同步扩展三段式校验（尤其涉及路径或外链的字段必须过 `validate_relative_file` 或同等白名单），并保持「角色包不能覆盖引擎/安全规则」的分区优先级不变。
