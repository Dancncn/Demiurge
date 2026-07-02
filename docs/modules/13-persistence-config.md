# 持久化、凭据与连接测试

> 存档级技术原理文档。读者为协作开发者。
> 覆盖源文件：
> - `src-tauri/src/store/mod.rs`（设置 / 会话的数据结构与落盘）
> - `src-tauri/src/credentials.rs`（keyring 凭据读写、明文迁移与水合）
> - `src-tauri/src/connection_tests.rs`（provider / web_search 连接测试，纯探测、不落盘）
>
> 本篇侧重「持久化与凭据」视角。connection_tests 的 provider/adapter 细节与第 09 篇（LLM provider）交叉，本篇只讲它如何复用 `Settings` 并刻意绕开落盘。

---

## ① 模块职责与定位

这是整个引擎的「记忆层」。`store/mod.rs` 顶部注释直言其设计取舍（`src-tauri/src/store/mod.rs:1`）：

```rust
//! 组件 9：持久化。设置 / 多会话写入磁盘，下次启动可恢复。
//! 这就是 MVP 的全部「记忆」——不做向量 RAG。
```

三个文件分工明确：

| 文件 | 职责 | 落盘文件 |
| --- | --- | --- |
| `store/mod.rs` | `Settings` / `SessionStore` 数据结构、序列化、读写、脱敏、看板统计 | `settings.json`、`sessions.json`（兼容旧 `conversation.json`） |
| `credentials.rs` | 所有 secret 字段进出系统凭据管理器（keyring），并在启动时迁移历史明文 | 系统钥匙串（非项目文件） |
| `connection_tests.rs` | provider / web search 端点的「最小请求」连通性探测 | **不落盘**（核心设计点，见 ③） |

落盘目录由 Tauri 的 `app.path().app_data_dir()` 决定，启动时写入 `AppState.data_dir`（`src-tauri/src/lib.rs:1659`、`:1674`）。同一目录下还有其他模块的文件，构成完整的磁盘记忆：

```
<app_data_dir>/
├─ settings.json            # 非密钥设置（store/mod.rs）
├─ sessions.json            # 多会话 + active + rolling summary + goal state（store/mod.rs）
├─ conversation.json        # 旧版单会话，仅迁移时读取
├─ permissions.json         # 项目级权限规则（permission/mod.rs:480）
├─ user_permissions.json    # 用户级权限规则（permission/mod.rs:490）
├─ permission_audit.jsonl   # 权限决策审计日志，追加写（permission/mod.rs:527）
├─ sandbox/                 # 工具沙箱根
└─ packs/                   # 人格包
```

> 说明：`permissions.json` 与 `permission_audit.jsonl` 由权限模块 `permission/mod.rs` 维护，不在本篇三个主文件内。本篇在「磁盘布局」与「安全」处引用它们以补全持久化全景，详细机制见权限子系统文档。

---

## ② 关键类型 / 入口函数

### `Settings`（`store/mod.rs:224`）

运行时全量设置。类型注释点明了贯穿全篇的核心约定（`src-tauri/src/store/mod.rs:222`）：

```rust
/// 运行时设置。`api_key` 只保留在内存和前端表单里，落盘时会被清空；
/// 实际密钥由 `credentials` 模块写入系统凭据管理器。
```

字段大量使用 `#[serde(default = "...")]`，使得**旧版 `settings.json` 缺字段也能反序列化**（向前兼容是这里的第一设计原则）。`load_settings` 在解析失败时直接回退 `Settings::default()`（`store/mod.rs:419`），所以损坏的配置文件不会导致启动崩溃，只会静默重置。

需要重点区分的两类字段：

- **非密钥字段**：`provider`、`base_url`、`model`、`max_input_tokens`、`reasoning_effort`、`mcp_servers` 的非 secret 部分等。明文落 `settings.json`。
- **密钥字段（落盘时被清空）**：`api_key`、`tavily_api_key`、`brave_search_api_key`、`exa_api_key`、`webdav_password`、`media_api_key`，以及 `mcp_servers[].env[]` 中 `secret == true` 的条目。

枚举 `ProviderKind`（`store/mod.rs:120`）通过 `#[serde(rename = "...")]` 固定磁盘上的字符串值（如 `"deepseek"`、`"glm"`、`"xai"`），改 Rust 枚举名不会破坏已有配置。`ReasoningEffort`（`store/mod.rs:159`）额外提供 `parse()`，把斜杠命令里的 `low/med/high/xhigh/max/unset` 等别名归一化（`store/mod.rs:179`）。

### `Session` / `SessionStore`（`store/mod.rs:357`、`:383`）

```rust
pub struct Session {
    pub id: String,
    pub title: String,
    pub summary: Option<String>,            // rolling summary
    pub goal: Option<crate::agent::goal::GoalState>,  // goal state
    pub messages: Vec<Message>,
    pub updated_at: u64,
}
```

两个关键字段都用 `#[serde(default, skip_serializing_if = "Option::is_none")]`（`store/mod.rs:361`、`:363`）：

- `summary`：**rolling summary**。历史被裁剪/压缩后由 LLM 生成的滚动摘要，随会话一起落盘。
- `goal`：**goal state**，类型为 `agent::goal::GoalState`，承载目标、状态机、token 预算与计时，同样随会话落盘。

`skip_serializing_if` 保证空会话不会在 JSON 里写出 `"summary": null` / `"goal": null` 噪声。

`SessionStore::ensure_one()`（`store/mod.rs:391`）是健壮性闸门：保证至少有一个会话、且 `active` 始终指向存在的会话；任何加载路径都会过它一遍。

`SessionMeta`（`store/mod.rs:412`）是给前端列表用的轻量投影（不含 `messages`），避免把整段对话推给侧边栏。

### 入口函数一览

| 函数 | 位置 | 作用 |
| --- | --- | --- |
| `load_settings(dir)` | `store/mod.rs:419` | 读 `settings.json`，失败回退默认 |
| `redacted_settings(s)` | `store/mod.rs:427` | 克隆并清空所有 secret 字段（脱敏） |
| `save_settings(dir, s)` | `store/mod.rs:445` | **先脱敏再写盘**，secret 永不进 `settings.json` |
| `load_sessions(dir)` | `store/mod.rs:453` | 读 `sessions.json`；不存在则迁移旧 `conversation.json` |
| `save_sessions(dir, store)` | `store/mod.rs:484` | 整体序列化写 `sessions.json` |
| `hydrate_or_migrate_settings(dir, &mut s)` | `credentials.rs:205` | 从 keyring 水合 secret，顺带迁移历史明文 |
| `save_mcp_env_secrets(s)` | `credentials.rs:193` | 把 MCP secret env 写入 keyring |
| `test_provider(client, s)` | `connection_tests.rs:47` | provider 最小请求探测 |
| `test_web_search(client, s, p)` | `connection_tests.rs:125` | web search 探测 |

---

## ③ 核心数据流与算法

### 3.1 落盘前脱敏：`save_settings` 的双保险

`save_settings` 不直接写传入的 `Settings`，而是先过一遍 `redacted_settings`（`store/mod.rs:445`）：

```rust
pub fn save_settings(dir: &Path, s: &Settings) -> Result<(), String> {
    let p = dir.join("settings.json");
    let safe = redacted_settings(s);            // ← 清空所有 secret
    let json = serde_json::to_string_pretty(&safe)?;
    fs::write(&p, json)
}
```

`redacted_settings`（`store/mod.rs:427`）逐一 `clear()` 六个 secret 字段，并遍历 `mcp_servers[].env[]` 把 `secret == true` 的值清空。这是「持久化层」的最后一道闸：**即使上层忘了走 keyring，secret 也不会落进 `settings.json`**。两个单元测试（`store/mod.rs:538`、`:570`）专门钉死这一行为——断言写出的 JSON 不含任何 secret 明文。

注意 `redacted_settings` 也被 WebDAV 备份复用（`lib.rs:592`），所以**云端备份同样不含 secret**。

### 3.2 完整的保存链路：`save_settings` Tauri 命令

前端保存设置时调用 `save_settings` 命令（`lib.rs:540`），顺序很重要：

```rust
credentials::save_api_key(&settings.api_key)?;          // → keyring
credentials::save_web_search_api_keys(&settings)?;      // → keyring (tavily/brave/exa)
credentials::save_webdav_password(&settings.webdav_password)?;
credentials::save_media_api_key(&settings.media_api_key)?;
credentials::save_mcp_env_secrets(&settings)?;          // → keyring (MCP secret env)
*state.settings.lock().unwrap() = settings.clone();     // 内存态：保留明文 secret
store::save_settings(&dir, &settings)?;                 // 磁盘：脱敏后写盘
emit_settings_updated(&app, &settings);
```

设计要点：

1. **先写 keyring 再写磁盘**。若 keyring 写入失败（`?` 提前返回），磁盘上的旧配置不动，避免出现「磁盘脱敏成功但 secret 丢失」的状态。
2. **内存态保留明文**（`state.settings` 存的是未脱敏的 `settings`），磁盘存脱敏版。这是贯穿全篇的「内存明文 / 磁盘脱敏」二元模型——provider adapter 直接读内存里的 `Settings.api_key` 即可，不必关心 secret 来自哪。

### 3.3 secret 落 keyring：`SecretKind` 抽象

`credentials.rs` 用 `keyring::Entry` 访问系统凭据管理器（Windows 凭据管理器 / macOS Keychain / Linux Secret Service）。服务名固定为 `com.demiurge.engine`（`credentials.rs:12`）。

六个内置 secret 由枚举 `SecretKind`（`credentials.rs:20`）统一管理，每种映射一个稳定 account 名（`credentials.rs:31`）：

| SecretKind | account 常量 | 对应 `Settings` 字段 |
| --- | --- | --- |
| `Llm` | `llm_api_key` | `api_key` |
| `Tavily` | `web_search_tavily_api_key` | `tavily_api_key` |
| `Brave` | `web_search_brave_api_key` | `brave_search_api_key` |
| `Exa` | `web_search_exa_api_key` | `exa_api_key` |
| `WebDav` | `webdav_password` | `webdav_password` |
| `Media` | `media_api_key` | `media_api_key` |

`save_secret`（`credentials.rs:67`）有一个关键语义：**写入前 `trim()`，空串等价于删除凭据**。这样把字段清空再保存，会真正从钥匙串里删掉条目（`delete_credential`），而非留一个空值；`NoEntry` 错误被当成「已无此条目」吞掉，删除是幂等的。

### 3.4 MCP secret env：动态 account 名与稳定哈希

MCP server 的 secret 环境变量数量不定、名字任意，无法用固定常量。`mcp_env_account`（`credentials.rs:81`）为每个 `(server_name, env_key)` 生成确定性的 account 名：

```
mcp_env_<server_seg>_<envkey_seg>_<fnv1a_hash>
```

- `credential_segment`（`credentials.rs:91`）把名字清洗成 ASCII 小写 + 下划线、并截断（server 24 字符、env_key 32 字符），保证 account 名对各平台凭据管理器都合法、可读。
- `stable_hash_hex`（`credentials.rs:113`）是手写的 FNV-1a 64 位哈希（常量 `0xcbf29ce484222325` / `0x100000001b3`），对**原始未清洗的 `"{server}\n{env_key}"`** 求哈希。

为什么要带哈希后缀？因为清洗+截断会丢信息：两个不同的 server/env 名清洗后可能撞成同一段（如截断或非法字符归并）。哈希基于原始字符串，保证不同输入仍得到不同 account；同一输入永远得到同一 account（`credentials.rs:296` 的测试钉死了这一稳定性）。

`save_mcp_env_secrets`（`credentials.rs:193`）只对 `env.secret == true` 的条目写 keyring；非 secret env 留在 `settings.json` 明文里（它们本就是公开配置）。

### 3.5 启动水合与明文迁移：`hydrate_or_migrate_settings`

启动时序（`lib.rs:1667`）：

```
load_settings(dir)                  // settings.json，secret 字段为空（之前已脱敏）
  → hydrate_or_migrate_settings()   // 把 secret 从 keyring 填回内存
  → load_sessions(dir)
  → 写入 AppState
```

`hydrate_or_migrate_settings`（`credentials.rs:205`）对每个 secret 字段执行同一套「迁移优先、否则水合」逻辑。以 LLM key 为例（`credentials.rs:219`）：

```rust
if has_legacy_plaintext {
    save_api_key(&legacy_plaintext)?;   // 旧明文 → 搬进 keyring
    settings.api_key = legacy_plaintext;
} else if let Some(secret) = load_api_key()? {
    settings.api_key = secret;          // keyring → 水合进内存
}
```

含义：

- **迁移路径**：老版本曾把 key 明文写在 `settings.json` 里。新版本启动后 `settings.api_key` 非空即视为历史明文，将其搬进 keyring 并保留在内存。
- **水合路径**：正常情况下磁盘里 secret 为空，从 keyring 读回填进内存 `Settings`，供 provider/adapter 透明使用。

六个内置 secret 加 MCP secret env 都走这套流程（`credentials.rs:226`–`:275`）。最后有个收尾（`credentials.rs:277`）：

```rust
if has_legacy_plaintext || has_legacy_web_key || has_legacy_webdav_password
    || has_legacy_media || has_legacy_mcp_env {
    store::save_settings(dir, settings)?;   // 触发一次脱敏重写，抹掉磁盘上的历史明文
}
```

**只有发生了迁移才回写磁盘**——把曾经明文的 `settings.json` 重新脱敏落盘，完成「一次性清洗」。若纯粹是水合（磁盘本就干净），则不写盘，避免无谓 I/O。注意水合失败在启动处只打印告警、不阻断启动（`lib.rs:1668`），保证 keyring 不可用时应用仍能起来（只是没填上 secret）。

```
首次升级（磁盘有明文）              日常启动（磁盘干净）
  settings.json: api_key="sk-x"      settings.json: api_key=""
        │                                  │
   has_legacy=true                    has_legacy=false
        │                                  │
  save_api_key → keyring             load_api_key ← keyring
  内存 api_key="sk-x"                内存 api_key="sk-x"
        │                                  │
  save_settings 回写（抹明文）        不回写
```

### 3.6 rolling summary 与 goal state 的持久化

两者都挂在 `Session` 上，随 `sessions.json` 整体落盘。落盘统一经 `AppState::persist_sessions()`（`lib.rs:104`）——克隆内存 store 后调用 `store::save_sessions`。

**rolling summary** 的写入点：

- 普通对话回合中，`runner.rs` 在历史超预算被裁剪后调用 `summary::update_session_summary` 生成新摘要，再 `replace_messages_and_summary` 写回会话（`runner.rs:257`–`:267`）；`SessionTurnStore::mutate_and_persist`（`session_engine.rs:175`）每次改动后立即落盘。
- `/compact` 命令路径在 `collapse.rs:104`–`:120` 生成摘要、写入 `session.summary`、`persist_sessions()`。

读取侧：发消息时把 `session.summary` 取出（`lib.rs:1015`、`session_engine.rs:143`），喂给 prompt 的 `session_summary_section`（`prompt.rs:258`），作为被裁历史的「记忆替身」。

**goal state** 的写入点集中在 `agent/goal.rs`，每次状态变更都 `persist_sessions()`（`goal.rs:83`、`:92`、`:109` 等多处）。`set_goal`（`goal.rs:305`）把新 `GoalState` 挂到 active session；`clear_goal`（`goal.rs:329`）置空。

> **关于 token 预算的当前状态（如实标注）**：`GoalState.token_budget` 是**软约束/状态标记**，不是硬性阻断。`add_tokens`（`goal.rs:465`）在累计 token 超过 `token_budget` 时，仅把目标状态切到 `GoalStatus::BudgetLimited`（`goal.rs:486`），并不强制终止正在进行的请求。预算更多用于面板展示与状态机流转，而非运行时硬熔断。

### 3.7 连接测试：刻意不调用 save_settings

`connection_tests.rs` 是本篇里唯一**不碰磁盘**的部分，这是其核心设计：连接测试用的是前端表单里**当前编辑中、尚未保存**的 `Settings`。三个测试命令（`lib.rs:558`、`:566`、`:576`）都把前端传来的 `settings` 直接转交给 `connection_tests`，全程没有任何 `save_settings`/`save_*` 调用。

```
前端「测试连接」按钮（表单里可能含未保存的新 key）
        │  settings: Settings（含明文 secret）
        ▼
provider_check_connection / web_search_check_connection（lib.rs）
        │
        ▼
connection_tests::test_provider / test_web_search
        │  只发 HTTP 探测，读 settings.api_key 等
        ▼
ConnectionTestResult { ok, target, detail, latency_ms }   ← 不落盘、不写 keyring
```

为什么这样设计：

1. **测了再保存**。用户填入新 key 后应能先验证可用，再决定是否保存。若测试顺手落盘，会污染「未保存」语义，也可能把错误的 key 写进 keyring。
2. **无副作用、可重复**。连接测试是纯函数式探测，只产出 `ConnectionTestResult`（`connection_tests.rs:13`），不改变任何持久化状态。

#### provider 探测（与第 09 篇交叉）

`ProviderTestRequest::from_settings`（`connection_tests.rs:180`）复用 LLM 模块的 `ProviderProfile` 与 `llm::require_api_key`，据 adapter 类型映射成三种 `ProviderTestKind`（OpenAiCompatible / Anthropic / Gemini），并 `normalize_base_url` 校验 URL（`connection_tests.rs:470`，仅允许 http/https）。三种各发一个 `max_tokens: 1` 的最小请求（`connection_tests.rs:215`–`:245`），20 秒超时（`CONNECTION_TEST_TIMEOUT_SECS`，`connection_tests.rs:9`）。**持久化视角的要点**：它读的是传入 `settings` 的 secret，本地端点（`ProviderKind::Local`）允许无 key（`connection_tests.rs:587` 测试），远程 provider 缺 key 直接报错——但无论成败都不写盘。provider 协议细节见第 09 篇。

#### web search 探测

`WebSearchTestRequest::from_settings_with_env`（`connection_tests.rs:272`）体现了**「settings 优先、环境变量兜底」**的密钥来源策略：`setting_or_env`（`connection_tests.rs:492`）先看 `Settings` 字段，为空再查环境变量（如 `TAVILY_API_KEY`、`BRAVE_SEARCH_API_KEY`/`BRAVE_API_KEY`、`EXA_API_KEY`）。这与运行时 web search 工具的取值口径一致，保证「测试通过」≈「实际可用」。`auto` 模式按 Bing → DuckDuckGo 顺序回退（`connection_tests.rs:133`）。同样全程无落盘。

> WebDAV 的连接测试 `webdav_check_connection`（`lib.rs:576`）不在 `connection_tests.rs` 内，而在 `lib.rs` 直接实现（`webdav_ensure_collection`），但遵循同样的「探测不落盘」原则——它接收前端临时 `WebDavConfig`（`lib.rs:172`，含明文 password），仅做 PROPFIND/MKCOL 探测。

---

## ④ 与其他模块的交互边界

```
                   ┌─────────────────────────────────────────┐
   前端表单 ──────▶ │ lib.rs Tauri commands                    │
                   │  save_settings / *_check_connection      │
                   └───────┬───────────────┬──────────────────┘
                           │ secret         │ settings(含明文)
                           ▼                ▼
                   credentials.rs      connection_tests.rs
                   (keyring R/W)       (HTTP 探测, 无副作用)
                           │                │ 复用
                           │                ▼
                           │            llm::ProviderProfile / require_api_key（第 09 篇）
                           ▼
                   store/mod.rs save_settings → settings.json（脱敏）

   runner / collapse / goal ──persist_sessions──▶ store::save_sessions → sessions.json
```

- **LLM 模块（第 09 篇）**：`connection_tests` 复用 `ProviderProfile::for_kind`、`require_api_key`、`ProviderAdapterKind`；provider adapter 运行时直接读 `AppState.settings` 内存里的明文 `api_key`，不关心其来自迁移还是 keyring 水合。
- **MCP 模块**：`Settings.mcp_servers`（`McpServerConfig`/`McpEnvVar`）的 secret env 由 `credentials` 单独经 keyring 处理；启动 stdio server 时用内存中水合后的 env 值。
- **agent::goal / agent::collapse / agent::runner**：通过 `Session.summary` / `Session.goal` 把状态搭车进 `sessions.json`，落盘动作统一收口到 `AppState::persist_sessions()`。
- **permission 模块**：独立维护 `permissions.json` / `user_permissions.json` / `permission_audit.jsonl`（`permission/mod.rs:480`、`:490`、`:527`），与本篇共用 `data_dir` 与 `store::now_millis()`，但走各自的读写函数。
- **WebDAV 备份（`lib.rs`）**：`webdav_backup_now`（`lib.rs:585`）打包 `redacted_settings` + 全量 sessions 上云，确保备份不含 secret。

---

## ⑤ 安全与权限相关点

1. **secret 不落明文文件**：`save_settings` 强制经 `redacted_settings`（`store/mod.rs:445`），WebDAV 备份同样脱敏（`lib.rs:592`）。两条专项单测（`store/mod.rs:538`、`:570`）守护此不变量。
2. **secret 集中存系统凭据管理器**：服务名 `com.demiurge.engine`，account 名稳定（`credentials.rs:13`–`:18`），MCP env 用「清洗段 + FNV-1a 哈希」避免碰撞（`credentials.rs:81`）。
3. **空串即删除**：`save_secret` / `save_mcp_env_secret` 对空值执行 `delete_credential` 且幂等（`credentials.rs:70`、`:139`），清空字段会真正移除钥匙串条目。
4. **一次性历史明文清洗**：迁移路径把旧 `settings.json` 中的明文搬进 keyring 后回写脱敏文件（`credentials.rs:283`），消除磁盘上的历史泄露面。
5. **连接测试无持久化副作用**：避免「测试即保存」导致错误 key 入库，也避免未保存表单污染磁盘。错误响应体经 `cap_chars` 截断到 600 字符（`connection_tests.rs:10`、`:542`），防止把超长后端报文塞进 UI/日志。
6. **base_url 校验**：连接测试强制 scheme 为 http/https（`connection_tests.rs:477`），拒绝 `ftp://` 等（`connection_tests.rs:635` 测试）。
7. **审计日志**：权限决策追加写 `permission_audit.jsonl`（`permission/mod.rs:288`、`:526`），面板只回读最近 80 条（`permission/mod.rs:365`、`:511`）。

---

## ⑥ 已知限制与扩展点

- **向量 RAG 已实现**：lorebook/记忆检索走 BM25 + dense + RRF 混合召回，dense 向量由 `src-tauri/src/embed/mod.rs` 的 `RemoteEmbeddingProvider`（OpenAI 兼容 `/v1/embeddings`）提供，详见 [modules/20](./20-lorebook-vector-rag.md)。`sessions.json` 仍全量保存会话，rolling summary 是对话层的「压缩」手段；向量索引独立维护，不与 `sessions.json` 混写。`store/mod.rs:2` 头注释的历史口径（"MVP 不做向量 RAG"）已被 modules/20 的实现覆盖，以 modules/20 为准。
- **整文件覆盖写**：`save_sessions` / `save_settings` 都是 `fs::write` 全量覆盖（`store/mod.rs:487`、`:449`），非原子写、无 WAL。进程在写盘中途崩溃可能损坏文件；但 `load_*` 解析失败会回退默认/空，不会崩。
- **goal token 预算为软约束**：`token_budget` 仅切换状态到 `BudgetLimited`（`goal.rs:486`），不硬性熔断进行中的请求。若需硬约束需在 runner 侧增加拦截。
- **voice 相关字段为占位**：`voice_stt_backend` / `voice_tts_backend` 默认 `"none"`（`store/mod.rs:63`、`:67`），`voice_enabled` 默认 `false`（`:16`）；这些字段已能持久化，但其后端能力本篇范围内仅作为配置项存在，实际语音链路状态见对应模块文档，不应视为已完整接通。
- **computer_use_enabled 默认关闭**（`store/mod.rs:17`）：同样是可持久化的开关位。
- **MCP 仅本地 stdio**：`McpServerConfig` 当前面向本地 stdio server，secret env 经 keyring；远程 transport 的凭据策略需另行设计。
- **扩展新 secret 的步骤**：在 `SecretKind` 加变体 + account 常量（`credentials.rs:20`/`:31`）→ 在 `Settings` 加字段 → 在 `redacted_settings` 清空它（`store/mod.rs:427`）→ 在 `save_settings` 命令与 `hydrate_or_migrate_settings` 各加一段读写/迁移（`lib.rs:546`、`credentials.rs:219` 同形）。漏掉 `redacted_settings` 会导致明文落盘——这是最易出错处。
- **统计算法**：`compute_stats`（`store/mod.rs:671`）用 Howard Hinnant 的 `civil_from_days`（`store/mod.rs:656`）做本地日历换算，token 数按 `chars/4` 粗估；纯展示用，不参与预算决策。
