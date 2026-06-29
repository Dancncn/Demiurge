# MCP 集成（stdio 第一阶段）

> 主源文件：`src-tauri/src/mcp/mod.rs`
> 关联文件：`src-tauri/src/tools/mod.rs`、`src-tauri/src/agent/runner.rs`、`src-tauri/src/permission/mod.rs`、`src-tauri/src/credentials.rs`、`src-tauri/src/store/mod.rs`、`src-tauri/src/lib.rs`

本模块实现了一个 **Model Context Protocol（MCP）客户端运行时**，让 Demiurge 能够把外部 MCP server 暴露的工具/资源接入到自身的工具注册表与权限体系中。第一阶段（“first slice”）刻意把传输范围收窄到**本地 stdio server**，并复用 Demiurge 既有的权限门，而不是引入新的安全模型。这一设计意图在文件头注释中写得很明确（`src-tauri/src/mcp/mod.rs:1-5`）。

---

## 一、模块职责与定位

MCP 集成在整个 Agent 引擎中承担“**外部能力适配层**”的角色：

- **协议适配**：实现 MCP 的 JSON-RPC 2.0 over stdio 子集——`initialize` 握手、`tools/list`、`tools/call`、`resources/list`、`resources/read`，以及对 server 反向请求（`roots/list`、`ping`）的应答。
- **生命周期管理**：以 `McpManager` 为核心，按配置启动/停止子进程、跟踪连接状态、提供刷新与健康可视化（stderr 尾部）。
- **动态工具发现与注册**：把 server 报告的工具映射成形如 `mcp__server__tool` 的 Demiurge 工具名，并生成与原生工具同构的 `ToolDefinition`，让它们无缝进入 runner 的调度与权限循环。
- **风险分级**：把 MCP 工具的 annotation（`readOnlyHint`/`destructiveHint`/`openWorldHint`）翻译为 Demiurge 的 `ToolRisk` 等级，从而决定并发策略与权限确认行为。
- **密钥治理**：与 `credentials` 模块协作，把标记为 `secret` 的环境变量存入操作系统 keyring，并在落盘配置时脱敏。

第一阶段的边界由 `connect_stdio_server` 强制（`src-tauri/src/mcp/mod.rs:472-474`）：`transport` 只接受 `Stdio`，其它枚举值在代码层面尚未定义——`McpTransportKind` 当前**仅有 `Stdio` 一个变体**（`src-tauri/src/mcp/mod.rs:44-48`）。因此“第一阶段”不是临时开关，而是类型系统层面的真实约束；HTTP/SSE 等远程传输属于**预留扩展点，尚未实现**。

---

## 二、关键类型与入口函数

### 2.1 配置类型（持久化在 Settings 中）

| 类型 | 位置 | 说明 |
|------|------|------|
| `McpServerConfig` | `mcp/mod.rs:59-71` | 单个 server 的配置：`name`、`enabled`、`transport`、`command`、`args`、`env`。 |
| `McpEnvVar` | `mcp/mod.rs:50-57` | 环境变量项：`key`/`value`/`secret`。`secret=true` 触发 keyring 治理。 |
| `McpTransportKind` | `mcp/mod.rs:44-48` | 仅 `Stdio`。 |

`McpServerConfig` 提供两个辅助方法：

- `normalized_name()`：调用 `normalize_segment(name, 32)`，产出可用于标识符的规整名。
- `signature()`（`mcp/mod.rs:78-80`）：把整个 config 序列化为 JSON 字符串作为“配置指纹”。这是**幂等刷新的核心**——见下文 `ensure_initialized`。

这些配置存放在 `Settings.mcp_servers`（`src-tauri/src/store/mod.rs:294-295`），因此 MCP server 列表随应用设置一起持久化。

### 2.2 运行时类型（仅存于内存）

| 类型 | 位置 | 说明 |
|------|------|------|
| `McpManager` | `mcp/mod.rs:241-244` | 全局单例，内部仅一个 `Mutex<HashMap<String, McpServerRuntime>>`，键为 server name。挂在 `AppState.mcp`（`lib.rs:53`、`lib.rs:93`）。 |
| `McpServerRuntime` | `mcp/mod.rs:172-184` | 单 server 的全部运行时状态：config、signature、status、error、server_info、instructions、capabilities、`tools`、`resources`、连接句柄 `handle`、`updated_at`。 |
| `McpClientHandle` | `mcp/mod.rs:163-170` | 连接句柄，可 `Clone`。包含 stdin writer、子进程 `Child`、待响应表 `pending`、自增请求 ID `next_id`、stderr 累积缓冲。 |
| `McpTool` / `McpResource` | `mcp/mod.rs:145-161` | 发现到的工具/资源的内部表示。 |
| `McpAnnotations` | `mcp/mod.rs:137-143` | 工具的风险提示位。 |

`McpClientHandle` 的字段混用了两种锁：`writer`/`child` 用 `tokio::sync::Mutex`（异步上下文中持有跨 await），而 `pending`/`stderr` 用 `std::sync::Mutex`（短临界区、不跨 await）。这是有意的选择——待响应表的插入/移除是同步的瞬时操作，没必要异步化。

### 2.3 对外视图类型（序列化给前端）

`McpServerView`、`McpToolView`、`McpResourceView`、`McpPanelState`（`mcp/mod.rs:92-135`）是面板状态的快照。注意 `McpServerView.stderr_tail` 在 `view()` 中实时从 handle 的 stderr 缓冲取尾部 2000 字符（`mcp/mod.rs:233-236`），这是健康诊断的主要手段。

### 2.4 公开入口函数

| 函数 | 位置 | 角色 |
|------|------|------|
| `ensure_initialized` | `mcp/mod.rs:307-374` | 幂等地把内存运行时对齐到配置。 |
| `refresh_all` | `mcp/mod.rs:376-389` | 全部断开后重新初始化。 |
| `disconnect_server` | `mcp/mod.rs:391-403` | 杀掉子进程并清理待响应。 |
| `call_tool` | `mcp/mod.rs:435-463` | 把 `mcp__*` 调用分发到对应 server。 |
| `read_resource` | `mcp/mod.rs:649-675` | 读取资源内容。 |
| `tool_definitions` | `mcp/mod.rs:409-411` | 导出已连接 server 的工具定义供注册表合并。 |
| `panel_state` | `mcp/mod.rs:405-407` | 导出面板快照。 |
| `is_mcp_tool_name` | `mcp/mod.rs:413-415` | 判定工具名是否属于 MCP（前缀 `mcp__` 且至少 3 段）。 |
| `permission_summary` | `mcp/mod.rs:417-433` | 为确认弹窗生成中文风险摘要。 |

---

## 三、核心数据流与算法

### 3.1 server 生命周期：`ensure_initialized` 的指纹对齐算法

`ensure_initialized`（`mcp/mod.rs:307-374`）是整个生命周期的中枢，被多处调用以保证“调用前一定已对齐”：

- runner 在每个 turn 开始时调用（`src-tauri/src/agent/runner.rs:156`）；
- `mcp_read_resource` 工具执行前调用（`src-tauri/src/tools/mod.rs:868`）；
- 前端打开 MCP 面板（`mcp_panel_state` 命令）时调用（`src-tauri/src/lib.rs:736`）。

它的算法是**基于配置指纹的差量对齐**，而非粗暴重连：

```
1. 读取 settings.mcp_servers，构建 desired = { name -> signature }
2. 扫描内存中现存运行时，找出 stale：
   - 配置里已删除（desired 不含该 name），或
   - signature 与运行时记录的 signature 不一致（配置被改过）
   → 对每个 stale 调用 disconnect_server（杀进程 + 从 map 移除）
3. 对每个 desired config：
   - name 为空 → 跳过
   - 已存在同名运行时 → 跳过（不重连，保持稳定）
   - enabled=false → 插入 McpServerRuntime::disabled（占位，不启动进程）
   - 否则 connect_stdio_server：成功插入 Connected 运行时，失败插入 failed 运行时
```

这套设计的关键意图是**幂等且最小扰动**：只要配置指纹未变，已连接的 server 就不会被重启，避免每个 turn 都重新拉起子进程。配置一旦改动（signature 变化），对应 server 会被先断开再按新配置重建。

状态机：

```
            enabled=false
   config ───────────────► Disabled（占位，无进程）
      │
      │ enabled=true
      ▼
 connect_stdio_server
      │
      ├── Ok ──► Connected ──(signature 变 / 删除)──► disconnect ──► (移除)
      │
      └── Err ─► Failed（error 文本可见于面板）
```

`McpServerStatus` 共有 `Disabled`/`Pending`/`Connected`/`Failed` 四态（`mcp/mod.rs:83-90`）。值得注意：**`Pending` 当前没有任何代码路径会写入**——连接是 `connect_stdio_server` 内同步 `.await` 完成的，要么直接得到 `Connected` 要么得到 `Failed`，中间不经过 `Pending`。`Pending` 属于预留态。

`refresh_all`（`mcp/mod.rs:376-389`）则是强制全量重连：先对所有 server 调 `disconnect_server`，再 `ensure_initialized`。前端的“刷新”按钮（`mcp_refresh` 命令，`lib.rs:745`）走这条路径。

### 3.2 启动 stdio server：`connect_stdio_server`

`connect_stdio_server`（`mcp/mod.rs:465-573`）执行真正的进程拉起与握手：

1. **前置校验**：`command` 非空、`transport==Stdio`；并对 Windows 上裸 `npx` 给出明确报错（`mcp/mod.rs:475-479`，`looks_like_npx` 见 `mcp/mod.rs:1137-1140`）。这是因为 Windows 下 `npx` 是 `.cmd` 脚本，必须包装成 `command="cmd", args=["/c","npx",...]` 才能被 `Command::spawn` 正确启动。
2. **拉起进程**：`Command::new(command).args(args)`，三条管道全部 piped，并把 `config.env` 逐项注入（`mcp/mod.rs:481-491`）。注意此处注入的 `env.value` 是**已经被 keyring 水合过的明文值**（见第五节）。
3. **建立句柄**：取出 stdin/stdout/stderr，构造 `McpClientHandle`，`next_id` 从 1 开始。
4. **启动后台读取器**：`spawn_stdout_reader`（解析 JSON-RPC 帧）与 `spawn_stderr_reader`（累积 stderr）。stdout reader 拿到 `sandbox_dir` 的副本，用于回应 `roots/list`。
5. **握手**：发送 `initialize` 请求（`protocolVersion = "2025-06-18"`，`MCP_PROTOCOL_VERSION`，`mcp/mod.rs:27`；clientInfo 名为 `"demiurge"`，版本取 `CARGO_PKG_VERSION`），随后发出 `notifications/initialized` 通知（`mcp/mod.rs:522-538`）。
6. **能力门控发现**：只有当 `initialize` 返回的 `capabilities` 含 `tools` 字段时才 `discover_tools`；含 `resources` 时才 `discover_resources`（`mcp/mod.rs:549-558`）。`tools` 发现失败会让整个连接失败（`?`），而 `resources` 发现失败被 `unwrap_or_default()` 容忍——资源是可选增强，工具是核心契约。

超时常量分三档（`mcp/mod.rs:28-30`）：握手 `CONNECT_TIMEOUT=30s`、一般请求 `REQUEST_TIMEOUT=60s`、工具调用 `TOOL_TIMEOUT=600s`。工具调用给到 10 分钟，是为了容纳长耗时的外部操作。

### 3.3 JSON-RPC 帧的收发与多路复用

**发送**：`send_request`（`mcp/mod.rs:763-793`）原子自增 `next_id` 拿到请求 ID，建立一个 `oneshot` 通道，把 sender 存进 `pending` 表（键为 ID），然后写帧。随后 `tokio::time::timeout` 在该通道上等待。超时则从 `pending` 移除并返回超时错误。`send_notification`（`mcp/mod.rs:795-809`）不带 ID、不等待响应。

**接收**：`spawn_stdout_reader`（`mcp/mod.rs:677-704`）以行为单位读 stdout，每行尝试解析为 JSON，交给 `handle_incoming_message`（`mcp/mod.rs:722-761`）。这是一个**双向分发器**：

```
收到一帧 message：
  ├── 有 id 且无 method ──► 这是“对我请求的响应”
  │      从 pending 取出 sender，
  │      有 error → Err(format_jsonrpc_error)
  │      否则     → Ok(result)，唤醒 send_request
  │
  └── 有 method ──► 这是“server 发起的请求/通知”
         ├── 有 id（请求）：按 method 应答
         │     "roots/list" → 返回 sandbox 目录作为唯一 root
         │     "ping"       → 返回 {}
         │     其它         → JSON-RPC -32601 method not found
         └── 无 id（通知）：忽略
```

`roots/list` 的应答把 `sandbox_dir` 转成 `file://` URI 返回（`mcp/mod.rs:743-748`、`file_uri` 见 `mcp/mod.rs:1128-1135`），向 server 声明 Demiurge 只暴露沙盒目录作为文件根，命名为 `"Demiurge sandbox"`。这是 MCP 的 roots 机制——告诉 server 客户端允许其感知的文件系统范围。

**连接断裂处理**：stdout reader 在读到 EOF（`Ok(None)`）或错误时，调用 `reject_all_pending`（`mcp/mod.rs:825-829`）把所有挂起请求以错误唤醒，避免调用方永久阻塞到超时。

### 3.4 动态工具发现与 `mcp__server__tool` 命名

`discover_tools`（`mcp/mod.rs:575-619`）请求 `tools/list`，对每个工具：

1. 取 `name`（trim），空名跳过。
2. 用 `build_mcp_tool_name(server_name, original_name)`（`mcp/mod.rs:933-937`）构造暴露名：
   - server 段经 `normalize_segment(_, 24)`，tool 段经 `normalize_segment(_, 30)`；
   - 拼成 `mcp__{server}__{tool}`，再经 `cap_tool_name` 限长 64。
3. **去重**：用 `used_names` 计数。若同一 base_name 第二次出现，追加 `_{stable_hash(original_name)}` 后缀并再次限长（`mcp/mod.rs:597-604`）。这是因为多个原始工具名经规整后可能撞车（例如 `query.table` 与 `query-table` 都规整成 `query_table`）。
4. 解析 description（`sanitize_text` 去控制字符）、`inputSchema`（`normalize_input_schema` 补全 `type:object`/`properties`）、`annotations`。

命名规整算法 `normalize_segment`（`mcp/mod.rs:948-976`）：非 `[A-Za-z0-9_-]` 字符映射为 `_`，折叠连续下划线，按**字符数**截断到 `max_chars`，trim 首尾下划线，空结果回退为 `"server"`。`cap_tool_name`（`mcp/mod.rs:939-946`）在总名超 64 字节时，取前 55 字符 + `_{8位hash}`。`stable_hash`（`mcp/mod.rs:978-982`）用 `DefaultHasher` 取低 32 位、格式化为 8 位十六进制——注意它**不保证跨 Rust 版本稳定**（`DefaultHasher` 的算法可能随标准库变化），但在单次运行内一致，足以满足去重需求。

单元测试 `normalizes_mcp_tool_name`（`mcp/mod.rs:1146-1150`）验证 `("my server","query.table") → "mcp__my_server__query_table"`。

### 3.5 注册为工具：`tool_definition`

`tool_definitions`（`mcp/mod.rs:273-280`）只导出 **`Connected`** 状态 server 的工具。每个工具经 `tool_definition`（`mcp/mod.rs:831-862`）转成与原生工具同构的 `ToolDefinition`：

- `name`：用 `Box::leak` 把暴露名泄漏成 `&'static str`。这是因为 `ToolDefinition.name` 字段是 `&'static str`（`tools/mod.rs:107`），而原生工具都是编译期常量字符串。**这意味着每次 `tool_definitions()` 被调用都会泄漏一份字符串内存**——见第六节“已知限制”。`description` 同样被 leak。
- `risk`：由 `risk_for_annotations` 决定（见 3.6）。
- `concurrency`：`read_only` 的工具标 `ParallelSafe`，否则 `SerialOnly`（`mcp/mod.rs:853-857`）。
- `permission`：固定为 `Ask`/`Once`，reason 为“MCP 工具由外部 server 提供，执行前需要确认。”（`mcp/mod.rs:833-837`）。即 **MCP 工具默认永远走确认弹窗**。
- `output_policy`：`TruncateForUi`。
- `parameters`：直接用发现到的 `input_schema`。

注册表合并点在 `tools::registry_for_state`（`src-tauri/src/tools/mod.rs:727-730`）：

```rust
pub fn registry_for_state(state: &crate::AppState) -> Vec<ToolDefinition> {
    let mut defs = registry();                       // 原生工具
    defs.extend(crate::mcp::tool_definitions(state)); // 追加 MCP 工具
    defs
}
```

由此，MCP 工具对 runner、对模型的 schema 导出、对权限查询全部透明——它们就是普通工具，只是 `name` 带 `mcp__` 前缀。

### 3.6 风险分级：annotation → ToolRisk

`risk_for_annotations`（`mcp/mod.rs:887-897`）的优先级（按从高到低短路判定）：

| annotation 命中 | 映射 ToolRisk | 含义 |
|-----------------|---------------|------|
| `destructiveHint=true` | `Mutating` | 破坏性/可变更，最高优先级 |
| `openWorldHint=true` | `External` | 访问外部世界（网络等） |
| `readOnlyHint=true` | `ReadOnly` | 只读 |
| 都未命中 | `Privileged` | **默认按特权处理**（保守兜底） |

`parse_annotations`（`mcp/mod.rs:899-915`）对缺失字段一律按 `false`，因此一个未声明任何 hint 的工具会落入 `Privileged`——这是**安全默认**：宁可过度确认，不可放行未知风险。单测 `maps_annotations_to_risk`（`mcp/mod.rs:1162-1182`）覆盖了 ReadOnly/Mutating 两条。

### 3.7 工具调用分发：`call_tool`

runner 在执行阶段调用 `tools::execute`（`src-tauri/src/tools/mod.rs:874-877`），其第一步即判定：

```rust
if crate::mcp::is_mcp_tool_name(name) {
    return crate::mcp::call_tool(state, name, args).await;
}
```

`call_tool`（`mcp/mod.rs:435-463`）流程：

1. `resolve_tool(exposed_name)`（`mcp/mod.rs:282-304`）遍历所有 `Connected` server，找到 `exposed_name` 匹配的工具，返回 `(handle, server_name, original_name)`。找不到返回“未连接或不存在”。**注意它用的是原始名 `original_name`** 去调外部 server，暴露名只是 Demiurge 内部标识。
2. 发 `tools/call` 请求，参数 `{ "name": original_name, "arguments": args }`，超时 `TOOL_TIMEOUT`（600s）。
3. 检查返回的 `isError` 字段：为 `true` 则把格式化结果作为 `Err` 返回；否则作为 `Ok`。两条路径都经 `format_mcp_tool_result`。

### 3.8 结果格式化与裁剪

`format_mcp_tool_result`（`mcp/mod.rs:1073-1113`）把 MCP 的 `content` 数组拍平为可读文本：`type:text` 项直接取 `text`，其它项 pretty-print 为 JSON；若有 `structuredContent` 追加一段“Structured content:”。全部为空时回退 pretty-print 整个 result。最终经 `cap_chars(_, RESULT_CAP_CHARS)` 限到 10 万字符（`mcp/mod.rs:32`）。

`format_mcp_resource_result`（`mcp/mod.rs:1035-1071`）类似，但处理资源的 `contents` 数组：逐项打印 `Resource: <uri> (<mime>)` 头，再附 `text`；遇到 `blob`（base64）则**不内联**，仅输出 `[base64 blob omitted; N chars]` 占位，避免把二进制塞进上下文。单测 `formats_resource_text_contents`（`mcp/mod.rs:1184-1196`）验证了 text 路径。

### 3.9 资源读取：`read_resource`

`read_resource`（`mcp/mod.rs:649-675`）按 server name 查找运行时（必须 `Connected` 且有 handle），发 `resources/read` 请求并格式化。它通过原生工具 `mcp_read_resource`（参数 `server_name`/`uri`）对模型暴露——见 `read_mcp_resource_tool`（`src-tauri/src/tools/mod.rs:865-870`），该工具名也列在 `CORE_TOOL_NAMES`（`tools/mod.rs:134`）。这是个**手动按需读取**入口：发现阶段只列资源元数据（`resources/list`），真正读内容需要模型显式调用此工具。

---

## 四、与其他模块的交互边界

```
                ┌─────────────────────────────────────────┐
                │              AppState                     │
                │   .settings(mcp_servers)  .mcp:McpManager │
                └───────────────┬───────────────────────────┘
                                │
 runner.run_turn ──ensure_initialized──► McpManager（对齐 server）
        │
        │ tools::registry_for_state ──► registry() + mcp::tool_definitions()
        │                                        （合并 MCP 工具到注册表）
        ▼
 每个 tool_call：
   permission::decide_for_mode(risk) ── Ask ──► permission::confirm（弹窗）
        │                                          summary 来自
        │                                          tools::permission_summary_for_state
        │                                              └► mcp::permission_summary
        ▼
   tools::execute ──is_mcp_tool_name?──► mcp::call_tool ──tools/call──► 子进程
                                                                          │
   credentials::save_mcp_env_secrets ◄── save_settings ── keyring ────────┘（env 注入）
```

### 4.1 与 runner

runner（`src-tauri/src/agent/runner.rs`）是唯一的工具执行驱动。它在 `run_turn` 入口调 `ensure_initialized`（runner.rs:156），随后通过 `tools::definition_for_state`/`permission_policy_for_state`/`permission_summary_for_state`/`execute` 间接触达 MCP，**不直接 import `mcp` 的执行函数**。runner 对 MCP 工具与原生工具走完全相同的代码路径：`tool_start` 事件、权限门、`execute`、`tool_end` 事件（runner.rs:440-579）。

### 4.2 与 tools 注册表

tools 模块是 MCP 与 runner 之间的“接缝”：
- `registry_for_state` 合并定义（tools/mod.rs:727-730）；
- `execute` 按 `is_mcp_tool_name` 前缀分流（tools/mod.rs:874-877）；
- `permission_summary_for_state`（tools/mod.rs:1103-1104）优先用 `mcp::permission_summary` 的 MCP 专属摘要。

### 4.3 与权限模块

`tool_definition` 把 MCP 工具的 `permission.effect` 钉死为 `Ask`，但**最终是否弹窗取决于权限模式**（`permission::decide_for_mode`，`src-tauri/src/permission/mod.rs:178-240`）：

| 模式 | 对 MCP 工具的效果 |
|------|-------------------|
| `Default` | 走 `decide`（结合用户记住的规则），通常 `Ask`。 |
| `Auto` | 仅 `ReadOnly` 风险自动放行（permission/mod.rs:189）；其余仍 `Ask`。 |
| `Bypass` | 一律放行。 |
| `Plan`（未批准） | 仅 `ReadOnly` 放行探索；其余 `Deny`（permission/mod.rs:208-235）。 |

因此 3.6 的风险分级**直接决定了** Auto/Plan 模式下哪些 MCP 工具能免确认：声明 `readOnlyHint` 的工具在 Auto 模式可静默执行，而 `Privileged`（无 annotation）工具永远需要确认。`permission_summary`（mcp/mod.rs:417-433）生成的中文摘要会带上 server 名、原始工具名、风险等级与截断到 400 字符的描述，呈现在确认弹窗里供用户判断。

### 4.4 与 lib.rs 命令层

Tauri 命令 `mcp_panel_state`（lib.rs:735-737）、`mcp_refresh`（lib.rs:744-746）、以及按 name 切换/删除 server 的命令（lib.rs:757-771）是前端入口。`save_settings`（lib.rs:540-556）在持久化前调 `credentials::save_mcp_env_secrets`（lib.rs:550），把 secret env 写入 keyring。

---

## 五、安全与权限相关点

### 5.1 secret env 走 keyring + 落盘脱敏

这是 MCP 集成最关键的安全设计，跨 `mcp`/`credentials`/`store` 三个模块：

1. **写入**：`save_settings` 时，`save_mcp_env_secrets`（`src-tauri/src/credentials.rs:193-202`）遍历所有 `secret=true` 的 env，调 `save_mcp_env_secret` 写进 OS keyring。
2. **keyring 账户命名**：`mcp_env_account`（credentials.rs:81-89）用 `mcp_env_{server段}_{key段}_{hash}` 作为账户名，其中 hash 是 `server\nkey` 的 FNV-1a 64 位（`stable_hash_hex`，credentials.rs:113-120）。加 hash 是为了在 server 名/key 被截断或规整后仍能唯一区分。单测 `mcp_env_account_is_stable_and_sanitized`（credentials.rs:292-296）验证其稳定与脱敏。
3. **落盘脱敏**：`store::save_settings`（`src-tauri/src/store/mod.rs:445-447`）调用 `redacted_settings`，对每个 `secret=true` 的 env **清空 `value`**（store/mod.rs:435-438）后才写磁盘。单测 `save_settings_does_not_persist_secret_mcp_env_values`（store/mod.rs:570-595）断言 `settings.json` 不含明文且持久化后 `env[0].value == ""`。
4. **水合**：应用启动时 `hydrate_or_migrate_settings`（credentials.rs:205-286）把 keyring 中的 secret 读回 `env.value`（credentials.rs:261-275）。还兼容**历史明文配置**：若发现旧版 settings.json 里残留明文 secret，会迁移进 keyring 并重写文件（`has_legacy_mcp_env` 分支）。
5. **进程注入**：`connect_stdio_server` 注入的是水合后的明文 `env.value`（mcp/mod.rs:487-491）——明文只活在内存与子进程环境块里，从不落盘。

```
   UI 输入 secret ──save_settings──┬──► keyring（持久、明文）
                                   └──► settings.json（value 被清空）
   应用启动 ──hydrate──► keyring → 内存 env.value（明文）
   连接 server ──► 注入子进程环境（明文）
```

### 5.2 stderr 与 description 的脱敏处理

- **stderr 容量限制**：`spawn_stderr_reader`（mcp/mod.rs:706-720）累积 stderr 到上限 `STDERR_CAP=256KB`（mcp/mod.rs:31），超限时保留尾部。面板只取尾部 2000 字符。这既防止 stderr 把内存撑爆，也保证诊断信息可见。
- **控制字符过滤**：`sanitize_text`（mcp/mod.rs:984-991）从 description/annotation title 中剔除除 `\n\r\t` 外的 C0 控制字符与 DEL，防止外部 server 通过控制字符注入污染 UI 或日志。
- **结果限长**：所有工具/资源结果经 `cap_chars` 限到 10 万字符，防止恶意/异常 server 撑爆上下文。

### 5.3 roots 仅暴露沙盒

回应 `roots/list` 时只声明 `sandbox_dir`（mcp/mod.rs:743-748），与 Demiurge 文件工具被物理限制在沙盒目录的整体安全模型一致（参见 `tools/mod.rs:1-3` 注释强调“作用域是结构性强制的”）。不过需注意：roots 只是**对 server 的声明**，stdio server 作为本地子进程实际能访问的文件系统并不受此限制——roots 是协作约定而非强制隔离。

### 5.4 默认按特权确认

如 3.6 所述，无 annotation 的工具落入 `Privileged`，叠加 `tool_definition` 固定的 `Ask` 策略，构成“**默认拒绝放行、强制确认**”的保守姿态。外部 server 必须显式声明 `readOnlyHint` 才能在 Auto/Plan 模式下获得免确认待遇。

---

## 六、已知限制与扩展点

1. **仅 stdio 传输**：`McpTransportKind` 只有 `Stdio`（mcp/mod.rs:44-48），HTTP/SSE 等远程传输是预留扩展点，**尚未实现**。`connect_stdio_server` 对非 stdio 会直接报错。

2. **`Pending` 状态未接通**：`McpServerStatus::Pending`（mcp/mod.rs:86）在当前代码里没有任何写入路径——连接是同步 await 的，结果直接是 `Connected` 或 `Failed`。若未来要做异步/后台连接，可启用该态。

3. **`ToolDefinition.name` 通过 `Box::leak` 泄漏内存**（mcp/mod.rs:839-851）。由于该字段类型是 `&'static str`，每次 `tool_definitions()` 调用都会为每个 MCP 工具泄漏一份 name + description 字符串。而 `tool_definitions` 在每个 turn（`registry_for_state`）都会被调用，因此**频繁的 turn + 大量 MCP 工具会持续累积泄漏**。这是为了让 MCP 工具复用原生工具的 `&'static str` 接口而做的妥协，属于已知技术债。

4. **`stable_hash` 非跨版本稳定**：基于 `DefaultHasher`（mcp/mod.rs:978-982），用于工具名去重后缀。它在单次运行内一致，但不保证跨 Rust 版本/重启稳定。对去重场景无影响，但不应被当作持久标识。注意 keyring 账户用的是 credentials 模块里独立实现的 FNV-1a（`stable_hash_hex`），那个是确定性的。

5. **server 健康仅靠 stderr 与状态位被动反映**：没有主动心跳/重连。子进程崩溃后，挂起请求会被 `reject_all_pending` 唤醒为错误，但运行时状态不会自动从 `Connected` 翻转为 `Failed`——下一次配置变更触发 `ensure_initialized` 或手动 `refresh_all` 才会重建。`ping` 仅作为**应答 server**的能力实现，Demiurge 自身不主动 ping server。

6. **资源不订阅变更**：只支持一次性 `resources/list` + 按需 `resources/read`，未实现 `resources/subscribe` 或 `notifications/resources/list_changed`。工具列表同理——`notifications/tools/list_changed` 不会触发重新发现。

7. **roots 静态单一**：始终只返回沙盒目录一个 root，未实现动态 roots 或 `notifications/roots/list_changed` 通知。

---

## 附：协议版本与常量速查

| 常量 | 值 | 位置 |
|------|-----|------|
| `MCP_PROTOCOL_VERSION` | `"2025-06-18"` | mcp/mod.rs:27 |
| `CONNECT_TIMEOUT` | 30s | mcp/mod.rs:28 |
| `REQUEST_TIMEOUT` | 60s | mcp/mod.rs:29 |
| `TOOL_TIMEOUT` | 600s | mcp/mod.rs:30 |
| `STDERR_CAP` | 256 KB | mcp/mod.rs:31 |
| `RESULT_CAP_CHARS` | 100,000 字符 | mcp/mod.rs:32 |
| clientInfo.name | `"demiurge"` | mcp/mod.rs:531 |
