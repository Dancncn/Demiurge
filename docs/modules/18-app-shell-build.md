# 应用外壳、命令面与构建

> 存档级技术原理文档。读者为协作开发者。
> 覆盖源文件：
> - `src-tauri/src/lib.rs`（全局状态 `AppState`、所有 `#[tauri::command]` 命令面、`run()` 构建器与 `setup`、WebDAV/上下文聚合等辅助逻辑）
> - `src-tauri/src/main.rs`（Tauri v2 二进制入口，仅转调 `demiurge_lib::run()`）
> - `src-tauri/tauri.conf.json`（窗口、bundle、CSP 配置）
> - `src-tauri/Cargo.toml`（crate 布局与 release profile）
> - `src-tauri/build.rs`（`tauri_build::build()`）
> - `scripts/tauri.mjs`（dev 端口选择与配置覆盖包装器）
> - `vite.config.ts`、`package.json`（前端构建与脚本）
>
> 本篇是「壳层」视角：讲清进程如何启动、目录如何布局、前端通过哪些命令与 Rust 内核交互、`send` 如何分发斜杠命令、`context_panel_state` 如何聚合预算视图，以及发布包如何被裁剪。各业务子系统（agent 循环、goal、workflow、memory、permission、MCP、OCR、media）只在「交互边界」处引用，细节见对应专篇。

---

## ① 模块职责与定位

`lib.rs` 是整个引擎的**装配点（composition root）**。它本身几乎不实现业务逻辑，而是承担四件事：

1. **声明全局状态** `AppState`（`lib.rs:36`），把所有子系统需要共享的句柄、锁与目录集中在一处，由 Tauri 的 `manage()` 注入为托管状态。
2. **暴露命令面**：约 60 个 `#[tauri::command]` 函数，是 WebView 前端调用 Rust 内核的**唯一**入口（IPC 边界）。
3. **应用初始化** `run()` + `setup()`（`lib.rs:1642`、`:1650`）：建窗口、解析数据目录、加载持久化状态、迁移凭据、水合 workflow run。
4. **少量壳层自有逻辑**：WebDAV 备份的 HTTP/PROPFIND 细节、`context_panel_state` 预算聚合、`/effort` 斜杠命令处理，这些都不值得单独成模块，就近放在壳层。

`main.rs` 极薄（`main.rs:1-6`）：

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() { demiurge_lib::run() }
```

第一行的 `windows_subsystem = "windows"` 属性**仅在发布构建生效**（`not(debug_assertions)`），让发布版不弹出额外的控制台窗口；debug 构建保留控制台以便看 `eprintln!` 输出。这是 Tauri v2 的标准双 crate 布局：二进制 crate `main.rs` 只转调库 crate 的 `run()`，库名在 `Cargo.toml:11` 定义为 `demiurge_lib`，并以 `crate-type = ["staticlib", "cdylib", "rlib"]`（`Cargo.toml:12`）同时支持桌面静态链接与移动端动态库。`run()` 上的 `#[cfg_attr(mobile, tauri::mobile_entry_point)]`（`lib.rs:1641`）为移动端入口预留，但当前 bundle 目标只有 Windows NSIS。

---

## ② 关键类型 / 入口函数

### `AppState`（`lib.rs:36`）

进程内的唯一共享状态，被 `tauri::Builder::manage()` 接管后，每个命令通过 `State<'_, AppState>` 借用。字段可分三类：

| 类别 | 字段 | 说明 |
| --- | --- | --- |
| 共享资源 | `http: reqwest::Client` | 单一复用的 HTTP 客户端（连接池 + TLS 会话复用），120s 超时。`provider_check_connection`、WebDAV、voice、media 全部复用它 |
| 受锁状态 | `settings: Mutex<Settings>` | 运行时全量设置（脱敏后落盘，密钥走 keyring，见第 13 篇） |
| | `sessions: Mutex<SessionStore>` | 多会话集合 + 当前 active 会话 |
| | `pending_confirms: Mutex<HashMap<String, oneshot::Sender<PermissionResponse>>>` | 待确认工具调用：调用 id → oneshot 发送端，前端回执时回填 |
| | `session_permission_rules: Mutex<HashMap<String, PermissionRule>>` | 本会话内的临时权限规则 |
| | `plan_state: Mutex<PlanState>` | 计划模式的计划文件状态机 |
| | `edit_undo_stack: Mutex<Vec<EditUndoEntry>>` | 进程内 `edit_file` 撤销栈，供 `undo_edit` 安全回退 |
| | `workflow_runs` / `workflow_cancels` | workflow run 进度与取消标志（`Arc<AtomicBool>`） |
| | `session_engine: Mutex<SessionEngineState>` | 当前/上一轮 turn 的运行态（看板用） |
| | `mcp: McpManager` | MCP 连接管理器（内部自带锁） |
| | `ocr: OcrState` | OCR 模型句柄与状态 |
| 原子标志 | `cancel: AtomicBool` | 用户中断标志，被 agent 循环各处轮询 |
| | `busy: AtomicBool` | 防并发：标记是否正在处理一轮对话 |
| 目录 | `data_dir` / `sandbox_dir` / `packs_dir`（均 `Mutex<PathBuf>`） | 启动时为空，`setup()` 拿到 `AppHandle` 后才填充 |

为什么目录字段用 `Mutex<PathBuf>` 而不是直接 `PathBuf`？因为 `AppState::new()`（`lib.rs:81`）在 `manage()` 时就要构造，而 `app_data_dir()` 必须等到 `setup()` 拿到 `AppHandle` 才能解析。先用空 `PathBuf` 占位、`setup` 时再写入，是 Tauri 下「构造早于路径可知」这一时序矛盾的标准解法（`lib.rs:1674-1676`）。

`AppState::persist_sessions()`（`lib.rs:104`）是被反复调用的便捷方法：克隆出 `data_dir` 与 `sessions` 后调 `store::save_sessions` 落盘，几乎每个改动会话的命令末尾都会调它。

### `PlanState`（`lib.rs:64`）

计划模式的小状态机：`active`（计划模式开启中）、`approved`（已批准）、`path`/`content`（计划文件）、`created_at`/`approved_at`。`reset()` 整体清空。它由三个命令驱动：`set_permission_mode(Plan)` 激活、`approve_plan` 批准并自动切回 `Default` 模式、`reject_plan` 重置。详见 ③ 的状态流。

### `run()`（`lib.rs:1642`）

构建器组装顺序：
1. 构造带 120s 超时的 `reqwest::Client`（`lib.rs:1643`）。
2. `.manage(AppState::new(http))` 注入托管状态。
3. `.setup(...)` 闭包做一次性初始化（见 ③）。
4. `.invoke_handler(tauri::generate_handler![...])` 注册全部命令（`lib.rs:1684-1749`）。
5. `.run(tauri::generate_context!())` 启动事件循环；`generate_context!` 在编译期把 `tauri.conf.json` 嵌入二进制。

---

## ③ 核心数据流与算法

### 3.1 应用初始化与目录布局（`setup`，`lib.rs:1650-1682`）

```
run()
 └─ setup(app):
     ├─ get_webview_window("main")
     │    ├─ set_size(1811 × 1213)   // DEFAULT_WINDOW_WIDTH/HEIGHT 常量(lib.rs:32-33)
     │    └─ center()
     ├─ dir = app.path().app_data_dir()         // 操作系统级 app 数据目录
     │    └─ create_dir_all(dir)
     ├─ sandbox = dir/sandbox   → create_dir_all
     ├─ packs   = dir/packs     → create_dir_all → pack::ensure_default(packs)
     ├─ settings = store::load_settings(dir)
     │    └─ credentials::hydrate_or_migrate_settings(dir, &mut settings)  // keyring 水合/明文迁移
     ├─ sessions = store::load_sessions(dir)
     ├─ 写入 AppState：data_dir / sandbox_dir / packs_dir / settings / sessions
     ├─ agent::workflow_runtime::hydrate_persisted_runs(state)  // 从 journal 恢复 run 列表
     └─ persist_sessions()    // 迁移/初始化后保证落盘一次
```

值得注意的设计点：

- **窗口尺寸双重设置**。`tauri.conf.json` 已声明 `width: 1811, height: 1213`（`tauri.conf.json:17-18`），`setup` 又用 `PhysicalSize` 显式 `set_size` 并 `center` 一次。conf 里的尺寸是逻辑像素（受 DPI 缩放影响），而 `setup` 用的是**物理像素**，目的是在高 DPI 屏上拿到确定的初始物理尺寸；两处数值同源于常量 `DEFAULT_WINDOW_WIDTH/HEIGHT`。
- **凭据迁移容错**。`hydrate_or_migrate_settings` 失败时只 `eprintln!` 警告（`lib.rs:1668-1670`），不阻断启动——keyring 不可用时仍能用内存态运行。
- **目录布局**最终形成（结合第 13/14 篇）：

```
<app_data_dir>/                 // app.path().app_data_dir()
├─ settings.json                // 脱敏设置
├─ sessions.json                // 多会话 + active + summary + goal
├─ permissions.json / user_permissions.json / permission_audit.jsonl
├─ skills/                      // 全局技能目录（open_skills_dir 按需建）
├─ sandbox/                     // 工具沙箱根（open_sandbox 打开）
│   └─ .demiurge/ ...            // 项目本地配置与兼容目录
└─ packs/                       // 人格包，ensure_default 保证至少有 default
```

### 3.2 命令面分类

全部命令在 `generate_handler!`（`lib.rs:1684-1749`）一次性注册。按业务域分类如下（函数名 → 行号）：

| 分类 | 命令 | 关键说明 |
| --- | --- | --- |
| 发送/对话 | `send`(:292)、`send_with_agents`(:457)、`interrupt`(:505) | 见 3.3 |
| Turn 运行态 | `session_engine_state`(:515) | 暴露 `busy`/当前 turn 给看板 |
| 权限回执 | `respond_confirm`(:523) | 取出 oneshot 回填裁决 |
| 设置 | `get_settings`(:535)、`save_settings`(:540) | save 先把各类密钥写 keyring，再脱敏落盘并 emit `settings-updated` |
| 连接测试 | `provider_check_connection`(:558)、`web_search_check_connection`(:566)、`webdav_check_connection`(:575) | 纯探测、不落盘（见第 13 篇） |
| 权限模式/计划 | `set_permission_mode`(:643)、`plan_state`(:668)、`approve_plan`(:673)、`reject_plan`(:696) | 见 3.4 |
| 权限规则 | `permission_panel_state`(:707)、`shell_policy_state`(:712)、`permission_reset_rule`(:717)、`permission_upsert_rule`(:726) | 转调 `permission` 模块 |
| WebDAV 备份 | `webdav_backup_now`(:584)、`webdav_list_backups`(:616)、`webdav_delete_backup`(:625) | 见 3.5 |
| MCP | `mcp_panel_state`(:734)、`mcp_refresh`(:740)、`mcp_set_server_enabled`(:751) | 改动后 emit `mcp-updated` |
| 人格包 | `list_packs`(:776)、`import_pack_zip`(:782) | 转调 `pack` 模块 |
| 自定义 Agent | `agent_panel_state`(:792)、`agent_template_json`(:797)、`agent_validate_json`(:802)、`agent_read_file`(:807)、`agent_save_file`(:815)、`agent_delete_file`(:825) | 转调 `agent::custom` |
| Goal 驱动 | `goal_panel_state`(:833)、`goal_pause`(:838)、`goal_resume`(:848)、`goal_continue`(:869)、`goal_clear`(:890) | resume/continue 用 `busy.swap` 防并发 |
| 记忆 | `memory_panel_state`(:923)、`memory_add_entry`(:929)、`memory_update_entry`(:949)、`memory_delete_entry`(:969)、`memory_dedupe_apply`(:978) | 统一经 `memory_context()` 取 5 元组路径 |
| 会话管理 | `list_sessions`(:988)、`session_stats`(:995)、`get_history`(:1003)、`new_session`(:1335)、`select_session`(:1350)、`delete_session`(:1362)、`rename_session`(:1384) | 改动后 `persist_sessions` |
| 上下文面板 | `context_panel_state`(:1012) | 见 3.6 |
| 技能 | `skill_panel_state`(:1210)、`open_skills_dir`(:1224) | 可带 `query` 做匹配打分 |
| 沙箱 | `open_sandbox`(:1403) | 调 `tools::execute_open` 用系统文件管理器打开 |
| OCR | `ocr_image_bytes`(:1409)、`ocr_model_status`(:1602)、`ocr_download_models`(:1607) | 转调 `ocr` 模块 |
| 媒体 | `media_generate_image`(:1417)、`media_synthesize_speech`(:1425) | 转调 `media` 模块 |
| Workflow | `workflow_panel_state`(:1616)、`workflow_run`(:1621)、`workflow_stop`(:1636) | run 在 `async_runtime::spawn` 后台执行 |
| 语音 | `voice::voice_status`、`voice::voice_transcribe`、`voice::voice_synthesize` | STT 已接通云端；TTS 已接通 dashscope + gpt-sovits 双后端（见 ⑤） |

设计共性：**绝大多数命令是薄转调**，把 `state.inner()` 与若干锁里克隆出的值传给对应模块函数；壳层只负责取锁、克隆、emit 事件。写类命令的通用尾声是「改内存 → `store::save_*` 落盘 → `app.emit(...)` 通知前端」。

### 3.3 `send` 的斜杠命令分发（`lib.rs:292-455`）

`send` 是对话主入口，它不是简单地把文本丢给 agent 循环，而是先做**斜杠命令路由**。结构是一条长 `if/else if` 链，对 `text.trim()` 做前缀匹配：

```
send(text)
 ├─ begin_turn(Send)                        // 登记 turn 运行态
 ├─ events = TurnEventEmitter
 ├─ 分发：
 │   ├─ "/dream"     → agent::dream::run_manual_dream        (should_drive_goal=true)
 │   ├─ "/compact"   → agent::collapse::run_manual_compact   (should_drive_goal=true)
 │   ├─ "/goal"      → agent::goal::handle_slash
 │   │     ├─ Respond(body)            → events.assistant_done(body)        // 直接回话
 │   │     └─ Query{overlay, stored}   → run_turn_with_options(system_overlay, stored_user_text)
 │   ├─ "/skills"|"/skill" → agent::skills::slash_response → assistant_done
 │   ├─ "/effort"    → handle_effort_slash → assistant_done                  // 壳层自有
 │   ├─ "/workflows" → 列出 workflow runs → assistant_done
 │   ├─ "/workflow resume <id>" → resume_overlay → run_turn_with_options(workflow_run_id)
 │   ├─ "/ultracode <task>" → ultracode::overlay + new_run_id → run_turn_with_options
 │   └─ 其它（普通消息） → agent::run_turn(text)
 ├─ if 成功 && should_drive_goal && !cancel → goal::drive_after_turn       // 自动续跑目标
 ├─ status = cancel? Interrupted : ok? Completed : Failed
 └─ finish_turn(turn, status, error)
```

关键设计：

- **turn 生命周期与分发解耦**。无论走哪条分支，`begin_turn`(`lib.rs:296`) / `finish_turn`(`lib.rs:453`) 都包住整个过程，由 `session_engine` 统一登记运行态。`TurnEntrypoint` 当前只有 `Send` 与 `SendWithAgents` 两种（`session_engine.rs:29-32`）；`TurnStatus` 有 `Running/Cancelling/Completed/Interrupted/Failed`（`session_engine.rs:19-25`）。
- **两类斜杠命令**：一类是「**即时应答**型」（`/effort`、`/skills`、`/workflows`），直接 `events.assistant_done(body)` 写一条助手消息就结束，**不进 LLM**；另一类是「**改写本轮**型」（`/goal Query`、`/workflow resume`、`/ultracode`），构造一个 `system_overlay`（临时系统提示叠加）后走 `agent::run_turn_with_options`，让本轮 LLM 在叠加约束下运行。
- **`should_drive_goal` 闸门**。只有「会推进目标」的分支把它置 `true`；纯查询型（如 `/effort`、`/skills`）保持 `false`，避免在用户只是查状态时触发 `goal::drive_after_turn` 的自动续跑。
- `send_with_agents`（`lib.rs:457`）是 `send` 的简化版：不做斜杠分发，直接带 `agent_names` 走 `run_turn_with_options`，用于前端显式指定参与 agent 的场景。

`interrupt`（`lib.rs:505`）除了调 `request_interrupt` 置 `cancel` 标志外，还有一个**关键副作用**：立即 `drain()` 所有 `pending_confirms`，给每个等待中的确认发 `PermissionResponse::deny_once()`（`lib.rs:509-512`）。这是因为权限确认弹窗的 `await` 最长会等 5 分钟，若不主动唤醒，用户点「中断」后整轮仍会被卡住。

### 3.4 计划模式状态流（`set_permission_mode` / `approve_plan` / `reject_plan`）

```
set_permission_mode(Plan)        approve_plan()                 reject_plan()
   plan.active = true               需 plan.path.is_some()         plan.reset()
   plan.approved = false            plan.active = false            emit plan-updated
   plan.approved_at = None          plan.approved = true
   emit permission-mode-updated     plan.approved_at = now
   emit plan-updated                settings.mode = Default       // 自动切回
                                     emit permission-mode-updated
                                     emit plan-updated
```

设计意图：进入 Plan 模式时只「武装」状态（`active=true, approved=false`），实际计划文件由 agent 在循环中写入 `plan.path`；批准时校验必须已有计划文件（`lib.rs:677`），批准后**自动把权限模式降回 `Default`** 让后续工具调用恢复正常确认流程。每次状态变更都 emit 两类事件让前端同步 UI。

### 3.5 WebDAV 备份（`lib.rs:584-1600`）

这是壳层里少见的「实打实业务逻辑」，因为它只是一组 HTTP 调用、不值得单独成模块。核心：

- **备份内容**（`webdav_backup_now`，`lib.rs:584`）：把 `redacted_settings`（脱敏后的设置，**不含密钥**）+ 全量 `sessions` + 版本号 + 时间戳打成 JSON，文件名 `demiurge-backup-{millis}.json`，PUT 到 WebDAV 集合。
- **URL 拼装**：`webdav_collection_url`(:1442) 强制 `http(s)://` 前缀并规整斜杠；`webdav_file_url`(:1455) 在拼接前先过 `validate_backup_file_name`。
- **安全校验** `validate_backup_file_name`（`lib.rs:1460`）：要求文件名以 `demiurge-backup-` 开头、`.json` 结尾，且**不含 `/` `\` `..`**——这是防路径穿越的关键，list/delete/parse 三处都依赖它。
- **集合自建** `webdav_ensure_collection`（`lib.rs:1508`）：先 `PROPFIND` 探测，失败再 `MKCOL`；把 405（已存在）也当成功。
- **列表解析** `parse_webdav_backup_files`（`lib.rs:1530`）：用一组**容忍命名空间前缀**的正则（`<[^:>/]*:?response>` 等）从 PROPFIND 的 XML 里抽 `href`/`getlastmodified`/`getcontentlength`，再 `xml_unescape` + 自实现 `percent_decode` 还原文件名，并再次用 `validate_backup_file_name` 过滤掉非备份文件。这里**手写正则解析 XML 而非引入 XML 库**，是为了不为单一功能拉重依赖。

> 注意：当前命令面**只有备份/列举/删除，没有 `webdav_restore`**。从备份恢复尚未在 Rust 侧实现命令。

### 3.6 `context_panel_state` 上下文预算聚合（`lib.rs:1012-1099`）

这是壳层最复杂的只读命令，把分散在多个模块的「上下文占用」信息聚合成前端可视化的预算面板。数据流：

```
context_panel_state:
 1. 克隆 settings；取 active 会话的 messages + summary
 2. persona_text = pack::load_pack(packs_dir, current_pack).persona_text
 3. prompt_build = agent::prompt::build_with_report(state, settings, persona, summary)
        → 得到 system prompt 文本 + 分段报告(PromptSectionReport)
 4. tools_schema = profile.supports_tools ? tools::main_schemas_json_for(dialect) : empty
 5. budget = agent::budget::history_budget(settings, prompt_text, tools_schema, messages)
        → ContextBudget{ max_input_tokens, reserved_output_tokens,
                         system_tokens, tools_tokens, history_tokens, history_budget_tokens }
 6. 派生指标：
        input_budget_used      = system + tools + history
        input_budget_remaining = max_input - used
        projected_total        = used + reserved_output
        history_remaining      = history_budget - history          (saturating_sub)
        history_over_budget    = history - history_budget          (saturating_sub)
 7. 组装 budget_items / history_buckets / memory_sources / prompt_sections
```

几个细节：

- **token 估算是启发式的**，统一走 `agent::budget::estimate_text_tokens` / `estimate_message_tokens`（见第 04/09 篇所述预算模块），不是真实 tokenizer。所以面板数值是「估算」而非精确计费。
- 所有减法都用 `saturating_sub`（`lib.rs:1056-1066`），保证历史超预算时不会下溢成巨大数字，而是钳到 0 / 给出 `over_budget`。
- `context_history_buckets`（`lib.rs:1136`）把消息按 `system/user/assistant/tool/other` 五桶统计条数与 token；`context_memory_sources`（`lib.rs:1169`）枚举各 scope 的 memory 文件（user/project/session/pack）外加一个 `project_legacy`（`sandbox/memory.md`，旧版项目记忆位置）。这两个函数都有单元测试（`lib.rs:1234-1332`）固化行为。
- `budget_items` 给四项（system / tools / history / output_reserve）配上 `limit_tokens` 与人类可读 `detail`，是前端进度条的直接数据源。

---

## ④ 与其他模块的交互边界

```
                         WebView (React, src/)
                              │  invoke(...) / listen(...)
                              ▼
              ┌──────────────────────────────────┐
              │   lib.rs  命令面 (IPC 边界)         │
              │   + AppState (manage)             │
              └──────────────────────────────────┘
   send/turn │   persist │ panel_state │ emit 事件
        ┌─────┴────┐  ┌───┴───┐  ┌────┴─────┐
        ▼          ▼  ▼       ▼  ▼          ▼
   agent::*    store    permission   mcp / pack / ocr / media / voice
 (run_turn,  (settings, (rules,      (各自管理子状态)
  session_    sessions, audit)
  engine,     stats)
  goal,
  budget,
  prompt)
```

- **agent**：`send`/`send_with_agents` 是 agent 循环的唯一触发点；`session_engine` 管 turn 运行态；`goal::drive_after_turn` 在轮末自动续跑；`prompt`/`budget` 被 `context_panel_state` 复用做预算可视化。
- **store**：所有持久化（settings/sessions）经它落盘；`session_stats`、`redacted_settings`、`now_millis` 都来自这里。
- **credentials**：`save_settings` 把 api_key / web_search / webdav / media / mcp_env 等密钥写 keyring；`setup` 启动时水合/迁移。
- **permission / mcp / pack / ocr / media / voice**：壳层只做转调与事件 emit，子状态由各模块自管（`McpManager`、`OcrState` 直接是 `AppState` 字段）。
- **前端事件**：壳层通过 `app.emit` 推送 `settings-updated`、`permission-mode-updated`、`plan-updated`、`mcp-updated` 等，配合命令的「拉取式」`*_panel_state`，构成「命令拉 + 事件推」的双通道同步模型。

---

## ⑤ 安全与权限相关点

- **密钥不落盘**。`save_settings`（`lib.rs:540`）先把所有 secret 写 keyring，再让 `store::save_settings` 落盘脱敏后的 `settings.json`；WebDAV 备份也用 `redacted_settings`（`lib.rs:592`）确保导出文件不含密钥。
- **CSP 关闭**。`tauri.conf.json:26` 设 `"csp": null`。这放宽了 WebView 的内容安全策略（便于内联资源、动态加载），但意味着不依赖 CSP 兜底，安全边界主要落在「命令面只暴露受控命令 + 工具层权限确认」。
- **路径穿越防护**：WebDAV `validate_backup_file_name`（`lib.rs:1460`）拒绝含 `/` `\` `..` 的名字；`webdav_collection_url` 强制协议前缀。
- **工具权限确认**：`pending_confirms` + `respond_confirm` + `interrupt` 三者构成确认回路，确认弹窗以 oneshot 异步等待用户裁决，中断时强制 `deny_once`（见 3.3）。
- **并发保护**：`busy: AtomicBool` 防止并发 turn（`goal_resume`/`goal_continue` 用 `busy.swap(true)` 抢占，`lib.rs:854`、`:875`）；`cancel: AtomicBool` 是全局中断闸。
- **项目本地兼容目录**：兼容能力统一收敛到 `.demiurge/compat/`，环境变量统一使用 `DEMIURGE_*` 前缀，降低跨工具约定带来的命名噪声。

---

## ⑥ 已知限制与扩展点

- **TTS 双后端已接通，缺流式/队列/打断**。`voice_synthesize`（`voice.rs:193-249`）按 `voice_tts_backend` 分派：dashscope 分支复用 `media::synthesize_speech`（默认音色 Cherry、模型 `qwen3-tts-flash`，返回音频 URL），gpt-sovits 分支走 `synthesize_with_gpt_sovits`（默认 base `http://127.0.0.1:9880`，返回 base64 data URI）；未实现流式合成、播放队列、打断、语速/情感参数、连接测试与失败降级。STT（`voice_transcribe`）已接通云端 Whisper 形态接口（DashScope `qwen3-asr-flash` 或当前 provider 的 OpenAI 兼容 `whisper-1`）。
- **WebDAV 无恢复命令**。只有备份/列举/删除，缺 `restore`（见 3.5）。
- **token 预算为估算**，非真实 tokenizer，面板数值仅供参考（见 3.6）。`context_panel_state` 里 `history_over_budget_tokens` 仅作展示，**裁剪/硬约束**的实际生效逻辑在 `agent::budget`/agent 循环侧，本壳层只读不裁。
- **窗口尺寸双写**。conf 逻辑像素与 `setup` 物理像素并存（见 3.1），改默认尺寸需同时改常量与 conf。
- **dev 端口扩展点**：`scripts/tauri.mjs` 通过 `DEMIURGE_PREFERRED_DEV_PORT` 环境变量可覆盖首选端口 38741（见后文构建链）。

---

## ⑦ 构建链：dev 端口选择、Vite、release profile

### `scripts/tauri.mjs` —— dev 包装器

`npm run tauri`（`package.json:12`）实际执行 `node scripts/tauri.mjs`，它在 `tauri dev` 之外多做一层**动态端口协商**：

```
tauri.mjs dev:
 1. host = TAURI_DEV_HOST || 127.0.0.1
 2. chooseDevPort(host):
       preferred = DEMIURGE_PREFERRED_DEV_PORT || 38741
       canListen(preferred)? → 用它
       否则在 49152–65535 随机尝试 100 次，再顺序扫描
 3. 写 .tauri-dev/tauri.dev.conf.json，覆盖 build.devUrl = http://host:port
 4. runTauri(["dev","--config",生成的配置])
       并把 DEMIURGE_DEV_HOST/DEMIURGE_DEV_PORT 注入子进程环境
```

设计动机：固定端口在多实例 / 端口占用时会冲突，所以先探测首选 38741、不可用则退到临时端口区间，并把最终端口经环境变量传给 Vite。`DEMIURGE_TAURI_DRY_RUN=1` 时只生成配置不启动（`tauri.mjs:89`），便于测试。非 `dev` 子命令则原样透传给 `@tauri-apps/cli`（`tauri.mjs:104`）。

### `vite.config.ts`

- 端口来源链：`DEMIURGE_DEV_PORT ?? PORT ?? 38741`（`vite.config.ts:6`），与 `tauri.mjs` 注入的环境变量对接；host 同理。
- `strictPort: true`：端口已由包装器选好，Vite 不得再自行漂移。
- `clearScreen: false`：把清屏权让给 Tauri，避免吞掉 Rust 报错（`vite.config.ts:14-15`）。
- `watch.ignored: ["**/src-tauri/**"]`：前端 HMR 不监听 Rust 源码，避免误触发。
- 插件：`@vitejs/plugin-react` + `@tailwindcss/vite`；`@` 别名指向 `./src`。
- `tauri.conf.json` 侧 `beforeDevCommand: "npm run dev"` / `beforeBuildCommand: "npm run build"`，`frontendDist: "../dist"`，把前端构建产物喂给 Tauri 打包。`npm run build` = `tsc --noEmit && vite build`（`package.json:10`），即**先类型检查再产出**。

### `build.rs` 与 release profile

- `build.rs` 仅 `tauri_build::build()`（`build.rs:1-3`），由 `tauri-build` 在编译期生成上下文与权限 schema。
- **release profile（`Cargo.toml:37-42`）**针对桌面分发体积优化：

| 设置 | 值 | 意图 |
| --- | --- | --- |
| `opt-level` | `"s"` | 优化**体积**而非纯速度 |
| `lto` | `true` | 链接期优化，跨 crate 内联、去冗余 |
| `strip` | `true` | 剥离符号表，减小二进制 |
| `codegen-units` | `1` | 单代码生成单元，最大化优化（牺牲编译并行度） |
| `panic` | `"abort"` | panic 直接 abort，去掉 unwind 表/栈展开代码 |

注意 `panic = "abort"` 的影响：内核里**不能依赖 panic 被 `catch_unwind` 捕获**，所以错误处理普遍用 `Result<_, String>` 而非靠 panic 兜底——这与命令面几乎全部返回 `Result<_, String>` 的风格一致。
- bundle 目标只有 `nsis`（`tauri.conf.json:31`）即 Windows 安装包；`webviewInstallMode: downloadBootstrapper` 表示安装时按需下载 WebView2 运行时而非内嵌。
