# 执行类与联网类工具

> 存档级技术原理文档。读者为协作开发者。
> 覆盖源文件：
> `src-tauri/src/tools/shell.rs`、`web_search.rs`、`web_fetch.rs`、`web_common.rs`、`http_get.rs`、`package_scripts.rs`、`open_path.rs`、`clipboard.rs`、`system_info.rs`、`tool_search.rs`、`execute_tool.rs`。
> 注册表与分发位于 `src-tauri/src/tools/mod.rs`，入参校验位于 `src-tauri/src/tools/args.rs`。

---

## 1. 模块职责与定位

工具子系统的总体设计原则写在 `src-tauri/src/tools/mod.rs:1`：

> 「每个工具 = 名称 + 描述 + 输入 JSON Schema + 权限/风险/并发/输出策略 + execute。作用域是结构性强制的（文件工具被物理限制在沙盒目录），不靠提示词。」

本文聚焦其中两类「会真正与外界交互」的工具：

| 分类 | 工具 | 共同特征 |
| --- | --- | --- |
| 执行类 | `shell` | 启动本机子进程；`Privileged` + `Ask` 确认门；分三档隔离 |
| 联网类 | `web_search`、`web_fetch`、`http_get` | 走 `state.http`（reqwest）发请求；`External` 风险；多为 `Allow`（无需逐次确认） |
| 系统边界类 | `package_scripts`、`open_path`、`clipboard`、`system_info` | 触及本机系统但边界各异（只读/确认门/平台命令） |
| 元工具 | `tool_search`、`execute_tool` | 不直接干活，负责 deferred 工具的「发现 + 代理执行」 |

所有工具在 `registry()`（`mod.rs:173`）中声明元数据，在 `execute()`（`mod.rs:874`）的 `match` 分支里路由到各模块的 `run()`。这是一个刻意保持显式的设计：注释（`mod.rs:872`）说明 MVP 用 `async fn` + `match` 充当统一执行入口，「避免为了少数异步工具引入 async-trait 依赖」。新增工具只需在 `registry()` 加一项 + 在 `execute()` 加一条分支。

每个工具的四维策略由 `mod.rs:57-78` 的枚举刻画：

- `ToolRisk`：`ReadOnly | Mutating | External | Privileged`
- `ToolConcurrency`：`ParallelSafe | SerialOnly`
- `ToolOutputPolicy`：`Inline | TruncateForUi`
- `PermissionPolicy`：`Allow | Deny | Ask`（`mod.rs:80`）

本文涉及的工具元数据如下表（取自 `registry()`）：

| 工具 | risk | concurrency | permission | 入口函数 |
| --- | --- | --- | --- | --- |
| `shell` | Privileged | SerialOnly | Ask | `shell::run` / `shell::preview` |
| `web_search` | External | ParallelSafe | Allow | `web_search::run` |
| `web_fetch` | External | ParallelSafe | Allow | `web_fetch::run` |
| `http_get` | External | ParallelSafe | Allow | `http_get::run` |
| `package_scripts` | ReadOnly | ParallelSafe | Allow | `package_scripts::run` |
| `clipboard` | Privileged | SerialOnly | Ask | `clipboard::run` |
| `open_path` | Privileged | SerialOnly | Ask | `open_path::run`（deferred）|
| `system_info` | ReadOnly | ParallelSafe | Allow | `system_info::run` |
| `tool_search` | ReadOnly | ParallelSafe | Allow | `tool_search::run` |
| `execute_tool` | Privileged | SerialOnly | Ask | `execute_tool::run` |

---

## 2. shell：风险分类、policy state 与三档隔离

`shell` 是整个子系统中安全约束最复杂的工具。它的设计目标是：**让 Agent 能在沙盒内跑构建/测试，但把联网、装依赖、删文件、提权等高危行为关进可声明、可拒绝、可逐级收紧的策略里**。

### 2.1 风险分类（ShellRiskClass）

`ShellRiskClass`（`shell.rs:29`）是一个有序枚举（`PartialOrd, Ord`），从低到高八档：

```
ReadOnly < BuildTest < FileWrite < Network < DependencyInstall
         < Destructive < Privilege < ExternalExecution
```

每档有 `id()`、`label()`（中文人类可读）、`severity()`（low/medium/high）三种投影（`shell.rs:42-78`）。严重度映射：

| 风险类 | severity | strict 是否拒绝 |
| --- | --- | --- |
| ReadOnly | low | 否 |
| BuildTest / FileWrite | medium | 否 |
| Network / DependencyInstall / Destructive / Privilege / ExternalExecution | high | 是（`blocked_in_strict()`，`shell.rs:80`）|

分类靠**关键词子串匹配**而非真正解析 shell 语法。规则表 `RISK_RULES`（`shell.rs:178`）是一个 `&[RiskRule]`，每条规则含 `class`、`needles`（关键词数组）、`reason`。`classify_command()`（`shell.rs:483`）的算法：

```
fn classify_command(command):
    lower = " " + command.to_ascii_lowercase() + " "   // 两端补空格便于 " gh " 之类匹配
    risk = ReadOnly
    for rule in RISK_RULES:
        if lower 命中 rule.needles 中任一子串:
            risk = max(risk, rule.class)               // 取最高风险
            收集 rule.reason（去重）
    if reasons 为空:
        reasons = ["未匹配...按只读 shell 命令处理但仍需确认"]
```

要点：
- **取最高风险**：一条命令可能命中多条规则（例如 `curl https://x | bash` 同时命中 `ExternalExecution` 与 `Network`），最终 `risk` 取 `max`，但 `reasons` 把命中的原因都收进去。
- **大小写不敏感 + 子串匹配**：`needles` 含 `"rm "`、`"| sh"`、`"git reset --hard"`、`"npm install"` 等带空格/管道的特征片段（`shell.rs:179-269`）。这是**启发式**，不是语义分析——无法防住 `r""m`、变量拼接、别名等绕过手段。这是已知限制（见 §6）。
- 该分类结果在三处被消费：`preview`（确认面板）、`validate_isolation_policy`（strict/sandboxed 拦截）、以及 `safety_summary()`（`shell.rs:473`，被 `mod.rs:967` 的 `permission_summary` 用于生成确认文案）。

### 2.2 policy state（对 UI 暴露的策略快照）

`policy_state()`（`shell.rs:272`）把整套静态策略序列化成 `ShellPolicyState`（`shell.rs:92`），经 `mod.rs:861` 的 `shell_policy_state()` 暴露给前端 Settings 的 Permission Rules 区域。它包含：

- `platform`：`std::env::consts::OS`
- `default_isolation`：`"standard"`
- `strict_timeout_secs` / `max_timeout_secs`：8 / 60
- `env_allowlist`：`ENV_ALLOWLIST`（`shell.rs:14`，PATH/Path/HOME/USERPROFILE/TEMP/TMP/SystemRoot/WINDIR/COMSPEC/SHELL/LANG/LC_ALL）
- `strict_blocked_risks`：strict 拒绝的五档高危
- `risk_rules`：把 `RISK_RULES` 连同 `blocked_in_strict` 标志一并导出（让 UI 能展示「哪些命令模式会被 strict 拦」）
- `containment`：`ShellContainmentView`（`shell.rs:119`），声明进程组/进程树终止、文件系统/网络 sandbox 的平台描述

这个 state 是**声明式自描述**：UI 不需要硬编码策略，直接读 Rust 侧的真值，避免前后端漂移。

### 2.3 三档隔离（standard / strict / sandboxed）

`ShellIsolationMode`（`shell.rs:128`）三档，由入参 `isolation` 选择（`parse_args`，`shell.rs:434`）：

| 模式 | 默认超时 | 环境 | 高危命令 | OS sandbox |
| --- | --- | --- | --- | --- |
| standard | 15s（`DEFAULT_TIMEOUT_SECS`）| 默认最小白名单，可选 `inherit_env=true` 继承全量 | 允许（仍需确认）| 无 |
| strict | 8s（`STRICT_TIMEOUT_SECS`）| 强制 `env_clear` + 白名单；禁止 `inherit_env=true` | 拒绝五档 high 风险 | 无 |
| sandboxed | 8s | 同 strict | 同 strict | 要求平台 wrapper 可用，否则 fail closed |

`blocks_high_risk()`（`shell.rs:144`）对 strict 与 sandboxed 都返回 true，因此 sandboxed 在策略层面**复用 strict 的全部约束**（测试 `sandboxed_isolation_reuses_strict_policy`，`shell.rs:851` 验证此点），只是额外叠加 OS 级文件系统/网络隔离。

`parse_args` 中的隔离相关逻辑：
- 默认超时按 `blocks_high_risk()` 选 8s 或 15s，再经 `optional_u64_clamped`（`args.rs:27`）夹到 `[1, 60]`。
- `inherit_env` 默认 `false`；若 strict/sandboxed 且 `inherit_env=true`，直接报错拒绝（`shell.rs:457`）。

`validate_isolation_policy()`（`shell.rs:503`）是策略闸门：
1. 若当前模式 `blocks_high_risk()` 且命令分类 `blocked_in_strict()` → 拒绝执行，错误信息带风险标签和原因。
2. 若是 `Sandboxed` → 调 `ensure_sandbox_runtime_available()` 检查 wrapper 在 PATH 中可用。

`preview`（`shell.rs:345`）和 `run`（`shell.rs:377`）都会先 `classify_command` 再 `validate_isolation_policy`，因此确认面板展示的策略与实际执行一致。

### 2.4 进程构建与环境处理

`run()` 的核心流程（`shell.rs:377-426`）：

```
sandbox = state.sandbox_dir.lock()
cwd = resolve_in_sandbox(sandbox, req.cwd)   // 越界/符号链接逃逸检查，见 §5
检查 cwd 存在且是目录
profile = classify_command(req.command)
validate_isolation_policy(req, profile)      // 闸门
cmd = build_shell_command(req, sandbox, cwd)
if !inherit_env || blocks_high_risk():
    cmd.env_clear().envs(safe_env())          // 清空后只注入白名单
spawn 子进程：stdin=null, stdout/stderr=piped
轮询 try_wait()，超时则 terminate_process_tree
```

- **shell 选择**：`shell_command_spec()` 在 Windows 上用 `bash -lc <command>`（`shell.rs:577`），非 Windows 用 `sh -lc <command>`（`shell.rs:585`）。注意 Windows 下假定 `bash` 在 PATH（典型为 Git Bash / WSL），而非 `cmd.exe`。
- **环境白名单**：`safe_env()`（`shell.rs:548`）从 `ENV_ALLOWLIST` 中 `std::env::var` 读取当前进程的对应变量，过滤掉不存在的，组成 `BTreeMap`。这样即便不开 strict，默认也不把形如 `*_TOKEN`/`*_KEY` 的凭据环境变量传给子进程（测试 `safe_env_excludes_secret_like_variables`，`shell.rs:880`）。
- `stdin(Stdio::null())`：子进程无法从 stdin 读取，避免交互式命令挂起。

### 2.5 进程树 containment 与超时

这是 shell 安全的运行时支柱。`containment_view()`（`shell.rs:314`）构造的 `ShellContainmentView`（结构体定义见 `shell.rs:119`）声明 `process_group: true`、`kill_process_tree_on_timeout: true`。

**启动时设进程组**（`apply_platform_process_containment`）：
- Windows（`shell.rs:710`）：`creation_flags(CREATE_NEW_PROCESS_GROUP = 0x0200)`。
- Unix（`shell.rs:717`）：`process_group(0)`，让子进程成为新进程组组长。

**超时轮询**（`shell.rs:403-425`）：以 `Instant::now() + timeout_secs` 为 deadline，每 50ms `try_wait()` 一次。命令正常结束 → `wait_with_output()` 收集输出并 `format_output`。到达 deadline → `terminate_process_tree(child)` + `child.wait()`，返回超时错误。

**终止整棵进程树**（`terminate_process_tree`）：
- Windows（`shell.rs:730`）：`taskkill /PID <pid> /T /F`（`/T` 杀子树，`/F` 强制），再 `child.kill()` 兜底。
- Unix（`shell.rs:742`）：先向**进程组**（`-<pid>`）发 `SIGTERM`，等 200ms，若仍存活则发 `SIGKILL`，最后 `child.kill()` 兜底。

进程组是这里的关键：因为启动时把子进程设为新进程组组长，所以一个负 PID 信号能命中 `sh -lc` 派生出来的所有孙进程，避免「父进程被杀、子进程变孤儿继续跑」。

### 2.6 输出截断

`format_output()`（`shell.rs:768`）拼接 `$ command` + `退出码` + stdout/stderr 段；两者都空时写「（无输出）」。`truncate()`（`shell.rs:791`）按**字符数**（非字节）截断到 `OUTPUT_LIMIT = 12_000`，超出追加「…输出已截断」。注意这是 shell 工具自己的输出 cap，与联网工具的 `context_max_characters` 是各自独立的常量。

### 2.7 sandboxed 模式：sandbox-exec / bubblewrap / Windows fail-closed

仅当 `isolation == Sandboxed` 时，`build_shell_command`（`shell.rs:559`）把 base command 包进 OS sandbox wrapper。`platform_sandbox_runtime()`（`shell.rs:619`）：Linux → `"bwrap"`，macOS → `"sandbox-exec"`，其余 → `None`。

**Linux/WSL — bubblewrap**（`bubblewrap_spec`，`shell.rs:627`）构造的 `bwrap` 参数：

```
--die-with-parent          # 父进程死则子进程死
--unshare-net              # 断网（network sandbox 的实现）
--ro-bind / /              # 根只读绑定
--bind <sandbox> <sandbox> # 沙盒目录可写
--bind <temp> <temp>       # 临时目录可写
--dev /dev  --proc /proc
--chdir <cwd>
<program> <args...>        # 例如 sh -lc "npm test"
```

效果：进程能读整个文件系统（只读），但只能写沙盒目录和临时目录，且完全断网。测试 `bubblewrap_spec_unshares_network_and_binds_sandbox`（`shell.rs:907`）验证 `--unshare-net`、`--die-with-parent`、sandbox 绑定与末位命令。

**macOS — sandbox-exec**（`sandbox_exec_spec`，`shell.rs:658`）内联生成一段 Seatbelt profile：

```
(version 1)
(deny default)
(allow process*)
(allow file-read*)
(allow file-write* (subpath "<sandbox>") (subpath "<temp>"))
(deny network*)
```

即默认拒绝、允许进程创建与全盘读、写限定在沙盒/临时目录、拒绝网络。路径经 `sandbox_profile_path()`（`shell.rs:678`）转义反斜杠与引号。测试 `sandbox_exec_spec_denies_network_and_limits_writes`（`shell.rs:927`）验证 `(deny network*)`、`(allow file-write*` 与沙盒路径。

**Windows / 其他平台 — fail closed**：`platform_sandbox_runtime()` 返回 `None`，`ensure_sandbox_runtime_available()`（`shell.rs:521`）直接返回错误「当前平台不支持 shell sandboxed isolation」。即便 wrapper 名存在但不在 PATH（`command_available`，`shell.rs:684`，Windows 下额外尝试 `.exe`），也会拒绝。`platform_filesystem_sandbox()` / `platform_network_sandbox()`（`shell.rs:328`、`shell.rs:337`）对 Windows 明确返回 `"unsupported on native Windows; process-tree containment only"` / `"policy_only"`。

> 设计意图：sandboxed 是「能则用，不能则拒」——绝不在不支持的平台静默降级为无隔离执行，避免给出虚假的安全感。

---

## 3. 联网工具：web_search / web_fetch / http_get + web_common

三个联网工具都不走确认门（`web_search`/`web_fetch`/`http_get` 均为 `Allow`），但都标 `External` 风险，且共用 `web_common.rs` 的解析/清洗/截断/来源输出逻辑。设计目标是**让来源提醒、截断标记和 Exa 边缘行为不在两个 adapter 间漂移**（IMPLEMENTATION.md:370 的明确陈述）。

### 3.1 web_common.rs：共享基础设施

| 能力 | 函数 | 说明 |
| --- | --- | --- |
| 统一来源类型 | `WebSource{title,url,snippet}`（`web_common.rs:9`）| search/fetch 共用 |
| 安全 push | `push_web_source`（`web_common.rs:26`）| 清洗 URL（`clean_extracted_url` 去尾随标点）、只收 http(s) URL、title 缺失时回退 `title_from_url` |
| 去重 | `dedupe_sources_by_url`（`web_common.rs:45`）| 按 URL 保序去重 |
| 来源行渲染 | `append_source_lines`（`web_common.rs:50`）| `numbered` 控制 `1.` vs `- `，可带 `max_chars` 提前截断 |
| 来源计数 | `source_link_count`（`web_common.rs:73`，`pub(crate)`）| 只统计 `Links:`/`Sources:` 块内的 markdown http 链接行 |
| 选项解析 | `parse_choice` / `parse_optional_choice`（`web_common.rs:102`、`web_common.rs:119`）| 大小写不敏感枚举校验 |
| payload 解析 | `parse_json_payloads`（`web_common.rs:130`）| 先整体当 JSON，否则按 SSE 逐行取 `data:`（跳过 `[DONE]`）|
| 文本段收集 | `collect_text_segments`（`web_common.rs:153`）| 递归（深度≤8）从 `text/content/markdown/answer/result` 字段抽含 http 的字符串 |
| HTML 处理 | `looks_like_html` / `extract_title` / `html_to_text` / `clean_html_inline`（`web_common.rs:188`-`231`）| 正则去 script/style、块级标签转换行、抽 `<title>` |
| 实体解码 | `decode_html_entities`（`web_common.rs:245`）| 手写映射 `&amp; &quot; &#39; &apos; &lt; &gt; &nbsp;` |
| 域名规范化 | `normalize_domain_list` / `domain_matches`（`web_common.rs:281`、`web_common.rs:296`）| 去 scheme/www/尾斜杠，`domain_matches` 支持子域后缀匹配 |
| 截断 | `cap_chars_with_marker` / `cap_chars_with_flag`（`web_common.rs:380`、`web_common.rs:389`）| 按字符数截断，可返回是否截断标志 |
| 密钥读取 | `env_first` / `settings_secret`（`web_common.rs:308`、`web_common.rs:315`）| settings 优先、env 兜底 |

**SSE/JSON 双模解析**是这里的关键设计：Exa MCP 既可能返回纯 JSON，也可能返回 `text/event-stream`。`parse_json_payloads` 先尝试 `serde_json::from_str` 整体解析，失败则按行扫描 `data:` 前缀逐条 parse（测试 `parses_plain_json_and_sse_payloads`，`web_common.rs:401`）。

### 3.2 Exa MCP 外壳（call_exa_mcp）

`call_exa_mcp()`（`web_common.rs:332`）是 search 和 fetch 共用的 Exa 调用通道。它不走标准 MCP client，而是**直接对 Exa 的 HTTP MCP endpoint 发 JSON-RPC `tools/call`**：

```
endpoint = env_first(["EXA_MCP_URL"]) 或默认 "https://mcp.exa.ai/mcp"
body = { jsonrpc:"2.0", id:<request_id>, method:"tools/call",
         params:{ name:<tool_name>, arguments:<arguments> } }
POST，Accept: application/json, text/event-stream
若有 exa_api_key（settings 优先，EXA_API_KEY 兜底，web_common.rs:328）→ bearer_auth
返回原始响应文本（由调用方再解析）
```

`web_search` 调用 `web_search_exa` 方法（`request_id="demiurge-web-search"`），`web_fetch` 调用 `get_contents` 方法（`request_id="demiurge-web-fetch"`）。两者把返回文本各自交给 `extract_exa_results` / `extract_exa_document` 解析。

### 3.3 web_search：六种 source

`web_search::run`（`web_search.rs:61`）流程：

```
解析 Args（含 query/allowed_domains/blocked_domains/num_results/
          context_max_characters/source/livecrawl/search_type）
query 至少 2 字符；allowed/blocked 不能同时非空
limit = num_results.clamp(1,20)，默认 8
context_max = clamp(1000,50000)，默认 10000
adapter = Adapter::parse(source 或 settings.web_search_provider 或 WEB_SEARCH_ADAPTER)
按 adapter 执行 → filter_domains → dedupe → truncate(limit) → format_results
```

**adapter 优先级**（`web_search.rs:81-94`）：入参 `source` > settings `web_search_provider`（经 `non_empty` 过滤掉空/`auto`）> 环境变量 `WEB_SEARCH_ADAPTER`。`Adapter::parse`（`web_search.rs:46`）接受 `auto/bing/duckduckgo|ddg/tavily/brave/exa`。

| adapter | 后端 | 需要 key | 抽取函数 |
| --- | --- | --- | --- |
| `Auto`（默认）| Bing HTML，空/失败 fallback DuckDuckGo（`web_search.rs:110`）| 否 | — |
| `Bing` | `www.bing.com/search` HTML 结果页 | 否 | `extract_bing_results`（正则解析 `b_algo`）|
| `DuckDuckGo` | `api.duckduckgo.com` Instant Answer JSON | 否 | `extract_duckduckgo_results` |
| `Tavily` | POST 到 Tavily endpoint | 可选 | `extract_tavily_results` |
| `Brave` | `api.search.brave.com/res/v1/llm/context` | 必需 | `extract_brave_results` |
| `Exa` | `call_exa_mcp("web_search_exa")` | 可选 | `extract_exa_results` |

**Bing 解析与重定向解码**：`extract_bing_results`（`web_search.rs:287`）用正则切 `<li class="b_algo">` 块，再抽 `<h2><a href>` 标题/链接和 `b_lineclamp`/`b_caption` 摘要。Bing 的结果链接常是 `bing.com/ck/...?u=<base64>` 跳转链接，`resolve_bing_url`（`web_search.rs:631`）识别出后取 `u` 参数，`decode_bing_redirect`（`web_search.rs:656`）跳过前 2 字符再 `decode_base64_url`（`web_search.rs:669`，手写 URL-safe base64 解码，无外部依赖）还原真实 URL。测试 `extracts_bing_results_and_decodes_entities`（`web_search.rs:703`）。

**Tavily**：endpoint 取 `env_first(["TAVILY_SEARCH_URL","TAVILY_ENDPOINT_URL"])`，默认值为 `https://tavily.claude-code-best.win/search`（`web_search.rs:188`）。请求体含 `query/search_depth/max_results`，allowed/blocked 转成 `include_domains`/`exclude_domains`。key 经 `bearer_auth` + `x-api-key` 双注入。

> 命名说明：默认 Tavily endpoint 域名 `tavily.claude-code-best.win` 是一个外部代理服务域名。这是为兼容外部检索代理而硬编码的默认值，可由 `TAVILY_SEARCH_URL`/`TAVILY_ENDPOINT_URL` 覆盖，与本项目自身定位无关。

**Brave**：用 `X-Subscription-Token` 头传 key，请求 LLM context 端点。`extract_brave_results`（`web_search.rs:377`）按 JSON pointer 路径 `/grounding/generic`、`/grounding/map`、`/grounding/poi`、`/web/results`、`/results` 依次取，全空时 `collect_result_values` 整树兜底递归。

**通用结果抽取**：`collect_result_values`（`web_search.rs:422`，深度≤8 递归）+ `push_result_from_value`（`web_search.rs:442`）用多别名键（url/link/href/website/sourceUrl…，title/name/heading/source，content/snippet/description/summary/text/raw_content）从任意结构里捞出来源。这让一套代码能容忍不同 provider 的 JSON 形状差异。

**Exa 文本兜底**：`extract_exa_results`（`web_search.rs:396`）先 `parse_json_payloads`，无结构化 payload 时退化到 `extract_results_from_text`（`web_search.rs:499`）——用 markdown 链接正则、`Title:/URL:/Content:` 标签行扫描、裸 URL 正则三重手段从纯文本里抠结果。测试 `extracts_exa_sse_text_results`（`web_search.rs:801`）。

**域名过滤**：`filter_domains`（`web_search.rs:591`）用 `reqwest::Url::parse` 取 host，`domain_matches` 做子域后缀匹配；URL 无法 parse 的结果被丢弃。

**输出格式**：`format_results`（`web_search.rs:616`）输出 `Web search results for query: "..."` + `Links:` 编号列表 + 末尾 `SOURCE_REMINDER_EN`，整体经 `context_max` 截断。`SOURCE_REMINDER_EN`（`web_common.rs:4`）强制要求模型在回答里用 markdown 超链接附来源。

### 3.4 web_fetch：direct / exa

`web_fetch::run`（`web_fetch.rs:34`）：`source` 取 `direct`/`exa`（默认 direct），`livecrawl` 取 `fallback`/`always`/`never`。**路由规则**（`web_fetch.rs:45`）：`source=="exa"` 或显式指定了 `livecrawl` → 走 `fetch_exa`；否则 `fetch_direct`。即「设了 livecrawl 就自动走 Exa」。

- `fetch_direct`（`web_fetch.rs:53`）：reqwest GET，User-Agent `"Demiurge WebFetch"`。按 content-type / `looks_like_html` 分三路：HTML → `extract_title` + `html_to_text`；JSON → pretty-print；其他 → `clean_plain_text_preserve_lines`。记录 `final_url`（跟随重定向后的真实 URL）。
- `fetch_exa`（`web_fetch.rs:112`）：`call_exa_mcp("get_contents", {ids:[url], livecrawl, contextMaxCharacters})`，`extract_exa_document`（`web_fetch.rs:143`）递归（深度≤8）收集 title 和 markdown/content/text/summary/raw_content 字段拼正文。source 标记为 `"exa-livecrawl"`。

正文经 `cap_chars_with_flag` 截断到 `context_max`（默认 20000，最大 80000），`truncated` 标志写进 `FetchDocument`。`format_document`（`web_fetch.rs:205`）输出 Title/URL/Source adapter/Truncated/Content/Sources 块 + 来源提醒，最终用 `context_max + 600` 再 cap 一次（给头部元信息留余量）。

### 3.5 http_get：轻量 GET

`http_get::run`（`http_get.rs:20`）是 `web_fetch direct` 的极简版，定位为「不需要来源引用、不需要深抽取」的场景（registry 描述 `mod.rs:419` 明确建议「需要深度网页抽取或来源引用时优先用 web_fetch」）。它额外暴露 `accept`（自定义 Accept 头）。`normalize_body`（`http_get.rs:85`）按 content-type 做 JSON pretty / HTML→text / 纯文本三路处理，输出含 `Status` 和 `Content-Type` 元信息。default cap 12000、最大 50000。

### 3.6 URL 规范化与协议白名单

`web_fetch::normalize_url`（`web_fetch.rs:222`）和 `http_get::normalize_url`（`http_get.rs:66`）逻辑一致：
- 空 → 报错。
- 已带 `http://`/`https://` → 保留。
- 含 `://` 但不是 http(s)（如 `file://`、`ftp://`）→ **拒绝**（「只支持公开 http/https URL」）。
- 无 scheme → 补 `https://`。
- 最后用 `reqwest::Url::parse` 校验，scheme 仍必须是 http/https。

测试 `normalizes_urls_and_rejects_non_http`（`web_fetch.rs:253`、`http_get.rs:107`）验证 `file:///tmp/a` 被拒。这是联网工具的协议边界：**不会去读本地文件或非 http 协议**，避免被用作本地文件读取的旁路。

---

## 4. 系统边界工具：package_scripts / open_path / clipboard / system_info

### 4.1 package_scripts（只生成，不执行）

`package_scripts::run`（`package_scripts.rs:5`）读沙盒内 `package.json` 的 `scripts` 字段，列出脚本或为指定 script 生成「建议 shell 命令」，**但绝不执行**（输出明确写「package_scripts does not execute scripts. Use shell if you want to run」）。

- 路径经 `resolve_in_sandbox`（`package_scripts.rs:51`），传目录时自动拼 `package.json`。
- `detect_package_manager`（`package_scripts.rs:84`）按 lockfile 推断：`pnpm-lock.yaml`→pnpm、`yarn.lock`→yarn、`bun.lockb`/`bun.lock`→bun，否则 npm。
- `suggested_command`（`package_scripts.rs:96`）按包管理器拼命令；`quote_script_name`（`package_scripts.rs:106`）对含特殊字符的脚本名做 POSIX 单引号转义。

边界设计：它是 `ReadOnly`/`Allow`，因为只读 JSON。真正执行仍必须经 `shell` 的确认门和隔离策略——这把「了解项目能做什么」和「真去做」分成两步，前者免确认、后者受控。

### 4.2 open_path（确认门 + 硬性协议白名单）

`open_path`（`open_path.rs:9`）是 deferred 工具（见 §7），标 `Privileged`/`Ask`。它用系统默认处理器打开文件/应用/URL：Windows `cmd /C start "" <target>`、macOS `open`、其他 Unix `xdg-open`。

关键安全设计（文件头注释 `open_path.rs:1` 明确）：**即便用户点了确认，也不放行高危目标**。`validate`（`open_path.rs:34`）：
- 拒绝 UNC/网络路径（`\\` 或 `//` 开头）——防触发远端可执行。
- 带 scheme 的 URL 只允许 `ALLOWED_SCHEMES = ["http","https","file","mailto"]`（`open_path.rs:7`）；其余协议（`ms-msdt:`、`search-ms:`、自定义协议等）一律拒绝。
- `url_scheme`（`open_path.rs:52`）特意区分 Windows 盘符：单字母 + `:`（如 `C:\...`）不算 scheme，避免误拒本地路径。

这是「确认门之外再加一道硬闸」的纵深防御：确认门防误操作，协议白名单防社工攻击诱导用户确认危险协议。

### 4.3 clipboard（只读、平台命令、超时）

`clipboard::run`（`clipboard.rs:23`）当前**只支持 `action=read`**（`write` 等其他动作直接报错，测试 `rejects_unsupported_actions`，`clipboard.rs:145`）。标 `Privileged`/`Ask`，因为剪贴板可能含密钥/私聊。

`platform_clipboard_commands`（`clipboard.rs:101`）按 OS 选命令：Windows `powershell Get-Clipboard -Raw`、macOS `pbpaste`、Linux 依次尝试 `wl-paste -n`→`xclip`→`xsel`（多候选 fallback）。`run_clipboard_command`（`clipboard.rs:72`）在**独立线程**里跑命令并通过 channel + `recv_timeout(3s)` 实现超时（`TIMEOUT_SECS=3`），避免剪贴板命令挂死阻塞工具调用。输出按字符数截断到 `max_characters`（默认 4000，最大 20000）。

### 4.4 system_info（零依赖、手算 UTC）

`system_info::run`（`system_info.rs:4`）返回 UTC 时间、OS、架构、当前工作目录。**无外部时间库依赖**：`civil_from_epoch`（`system_info.rs:27`）用 Howard Hinnant 的 `civil_from_days` 算法手算从 UNIX 秒到年月日时分秒。输出末尾提示「时间为 UTC，本地时间请按时区换算」。`ReadOnly`/`Allow`/`Inline`。

> 注意：`cwd` 来自 `std::env::current_dir()`（进程工作目录），不是沙盒目录。

---

## 5. 安全与权限边界（汇总）

| 边界 | 实现位置 | 机制 |
| --- | --- | --- |
| 文件路径越界 | `resolve_in_sandbox`（`mod.rs:1211`）| 词法折叠 `.`/`..` 拒绝越过沙盒根；再对沙盒根与「最近存在祖先」分别 `canonicalize`，挡 junction/符号链接逃逸 |
| shell 命令风险 | `classify_command` + `validate_isolation_policy`（`shell.rs`）| 关键词分类 + strict/sandboxed 拒绝五档高危 |
| shell 进程逃逸 | 进程组 + `terminate_process_tree`（`shell.rs:706`、`shell.rs:726`）| 超时杀整棵进程树 |
| shell 凭据泄露 | `safe_env`（`shell.rs:548`）| 默认只传白名单环境变量；strict/sandboxed 强制 `env_clear` |
| shell OS 级隔离 | sandbox-exec / bubblewrap（`shell.rs:658`、`shell.rs:627`）| 限制写路径 + 断网；Windows fail closed |
| 联网协议 | `normalize_url`（`web_fetch.rs:222`、`http_get.rs:66`）| 只允许 http/https |
| open_path 协议 | `ALLOWED_SCHEMES`（`open_path.rs:7`）| 只允许 http/https/file/mailto，拒绝 UNC |
| 子 Agent 工具面 | `SUBAGENT_READONLY_TOOL_NAMES`（`mod.rs:154`）| 只读子 Agent 拿不到 `shell`/`clipboard`/写入类（测试 `mod.rs:1308`）|
| 确认门 | `PermissionPolicy::ask` + `confirmation_preview`（`mod.rs:1110`）| shell/clipboard/open_path/execute_tool 等执行前展示 preview |

确认门的数据流：`runner` 在执行前调 `permission_summary_for_state`（`mod.rs:1103`）生成一行摘要，并对部分工具调 `confirmation_preview`（`mod.rs:1110`）生成详细预览。shell 的 preview 由 `shell::preview`（`shell.rs:345`）生成，包含命令、cwd、超时、风险分类、隔离模式与平台 containment 描述——确保用户在确认前看到的策略与实际执行一致。

---

## 6. tool_search / execute_tool：deferred 发现与代理执行

为减少固定 tools JSON 对上下文窗口的占用（IMPLEMENTATION.md:275），主 schema 只放 `CORE_TOOL_NAMES`（`mod.rs:116`）；低频工具留在 `DEFERRED_TOOL_NAMES`（`mod.rs:145`）池里：`open_path`、`screen_list_windows`、`screen_capture_region`、`screen_capture_window`、`screen_ocr_region`、`screen_ocr_window`。

这两个元工具实现「发现 → 代理执行」的两段式访问：

```
模型不知道 deferred 工具的 schema
        │
        ▼
tool_search(query)  ── 搜本地注册表元数据，打分排序，返回名称+描述+params
        │
        ▼
execute_tool(tool_name, args)  ── 校验是 deferred，再 match 路由到真实实现
```

**tool_search::run**（`tool_search.rs:10`）：对 `deferred_definitions()`（`mod.rs:743`，即 registry 中 `is_deferred_tool` 为真的项）做关键词打分——名称命中每词 +5，name+description+parameters 拼成的 haystack 命中每词 +1，按分降序、同分按名称升序排（`tool_search.rs:34`）。`limit` 默认 8、最大 20。输出末尾提示用 `execute_tool` 调用。它是 `ReadOnly`/`Allow`，只读本地元数据，不触网。

**execute_tool::run**（`execute_tool.rs:11`）：
1. `is_deferred_tool`（`mod.rs:168`）校验——若是已加载的 core tool 则报错「请直接调用」，杜绝用 execute_tool 绕过 core tool 的正常分发。
2. `match` 路由到真实实现：`open_path` → `open_path::run`；`screen_*` → `screen::*`。未支持的 deferred 名返回错误（`execute_tool.rs:27`）。

它是 `Privileged`/`Ask`，因为最终会触发打开路径、读屏、OCR 等系统能力。`execute_tool::preview`（`execute_tool.rs:31`）在确认门展示「将执行 deferred tool `<name>`，参数：<pretty json>」。

> 注意：`execute_tool` 自己并不做 `open_path` 的协议校验——校验在 `open_path::run` 内部完成（§4.2），因此无论直接调还是经 execute_tool 代理，安全闸门都生效。

---

## 7. 与其他模块的交互边界

- **AppState**：所有联网工具用 `state.http`（共享 reqwest client）；`shell`/`package_scripts` 用 `state.sandbox_dir`；联网工具的 key 经 `state.settings`（`web_search_provider`、`tavily_api_key`、`brave_search_api_key`、`exa_api_key`，定义于 `src-tauri/src/store/mod.rs:262`）。密钥水合/keyring 落盘由 `src-tauri/src/credentials.rs` 处理（settings 优先、env 兜底）。
- **runner（`src-tauri/src/agent/runner.rs`）**：执行后调 `source_link_count`（`runner.rs:105`）对 `web_search`/`web_fetch` 结果计来源链接数，生成 `source_quality_hint`（strong≥3 / limited≥1 / none=0），提示模型是否需要换查询或换 provider。
- **mcp（`src-tauri/src/mcp.rs`）**：`execute()`（`mod.rs:874`）先判 `is_mcp_tool_name` 走 MCP 分发；`mcp_read_resource` 走标准 MCP client。而 Exa 的调用**不走** MCP client，是 `web_common::call_exa_mcp` 自己拼的 JSON-RPC HTTP 请求。
- **connection_tests（`src-tauri/src/connection_tests.rs`）**：复用 settings/env 的 provider 与 key 解析逻辑做连接测试（`connection_tests.rs:279`）。
- **Settings UI**：通过 `shell_policy_state()`（`mod.rs:861`）读取 shell 策略快照渲染 Permission Rules 区域。

---

## 8. 已知限制与扩展点

1. **shell 风险分类是启发式子串匹配**，非真正 shell 语法解析（`classify_command`，`shell.rs:483`）。变量拼接、别名、字符串拆分（如 `r''m`）等可绕过分类。strict/sandboxed 的关键词拦截因此是「降低风险」而非「形式化保证」；OS sandbox（bubblewrap/sandbox-exec）才是真正的强隔离边界。
2. **Windows 无 OS 级 sandbox**：`sandboxed` 在 Windows 直接 fail closed（`platform_sandbox_runtime` 返回 None），无文件系统/网络隔离的等价实现。Windows 上只有进程树 containment + 关键词策略 + 环境白名单。
3. **Windows shell 依赖 `bash` 在 PATH**（`shell_command_spec`，`shell.rs:577`），而非 `cmd.exe`。无 bash 环境会 spawn 失败。
4. **clipboard 只读**：`action` 只支持 `read`，write 未实现（`clipboard.rs:32`）。
5. **Bing/DuckDuckGo 解析依赖页面/接口结构**：`extract_bing_results` 用正则匹配 `b_algo`/`b_caption` 等 class（`web_search.rs:287`），上游改版会导致解析退化（结果为空时 Auto 会 fallback 到 DuckDuckGo）。
6. **Tavily 默认 endpoint 为外部代理域名**（`tavily.claude-code-best.win`，`web_search.rs:188`），生产部署应通过 `TAVILY_SEARCH_URL` 指向受信端点。
7. **截断按字符数（chars）而非字节**：所有 cap 函数（`shell::truncate`、`cap_chars_with_marker`）用 `chars().count()`/`chars().take()`，对多字节 UTF-8 友好，但与 token 数无直接对应。
8. **扩展点**：新增联网 provider 只需在 `Adapter`（`web_search.rs:36`）加分支 + 一个 `extract_*` 函数，复用 `web_common` 的去重/过滤/输出；新增 deferred 工具只需进 `DEFERRED_TOOL_NAMES` + `registry()` + `execute_tool::run` 的 match。
