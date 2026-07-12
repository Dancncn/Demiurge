# Demiurge 全项目代码审查报告

> 审查日期：2026-07-12
> 范围：React/TypeScript 前端、Rust/Tauri 后端、工作区与 Git、权限、工具、角色包、持久化、LLM 流式适配，以及仓库内全部 Markdown
> 状态：审查底稿已完成；12 项 P1 均已修复并按独立提交落地，本文同步记录每项验证结果

## 1. 结论摘要

本轮没有发现需要按 P0 处理的立即性故障，共确认 **12 项 P1、12 项 P2、3 项 P3**。12 项 P1 现已全部修复；前端生产构建和 Rust 255 项测试通过，专项回归覆盖了相应的跨作用域、并发、故障注入和对抗性输入路径。P2/P3 仍按后续队列处理。

优先级最高的风险集中在两条所有权链：

1. **权限所有权**：角色包权限升级、Session/Project 规则跨边界复用、deferred wrapper 共用身份与动态外部工具风险下限降级均已修复。
2. **项目所有权**：undo、分支操作、后端 turn 与前端导航快照均已绑定不可变的 workspace/session identity。

P1 发布阻断队列已经关闭。显式 Bypass 模式、stdio 子进程本身不受 roots 强隔离以及报告中的 P2/P3 仍不能被误解为强安全边界；处理未受信内容或外部服务时仍应遵循最小权限。

| 级别 | 数量 | 含义 |
|---|---:|---|
| P0 | 0 | 立即阻断发布、可直接造成广泛不可逆损失 |
| P1 | 12 | 高风险安全/数据正确性问题，应在下一次可发布构建前修复 |
| P2 | 12 | 中风险可靠性、协议一致性、性能或可恢复性问题 |
| P3 | 3 | 可访问性、窄窗口体验和格式门禁问题 |

## 2. 审查方法与验证

- 阅读当前未提交前后端差异，并把新增工作区、分支、编辑活动和流式渲染纳入审查。
- 对权限作用域、路径解析、进程启动、网络访问、撤销、持久化与事件归属做数据流追踪。
- 对三种流式适配路径检查网络分片、事件边界、流尾、错误、usage、工具参数和终止语义。
- 逐份更新仓库内 Markdown，并检查相对链接、陈旧能力描述和测试基线。
- 执行 `npm run build`：通过；Vite 对约 1.1 MB Live2D vendor chunk 给出非阻断体积警告。
- 执行 `cargo test --manifest-path src-tauri/Cargo.toml --no-fail-fast`：255 passed，0 failed。
- 执行 `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`：通过。
- Markdown 覆盖核验：35/35 文件均出现在工作区变更列表。
- 禁用名称大小写不敏感全量扫描：0 命中。
- Markdown 相对链接检查：0 个失效目标；`git diff --check` 通过。
- 未执行真实外部模型、搜索、语音、WebDAV、MCP server 或打包安装器的网络/系统契约测试。

## 3. P1：高风险发现

### P1-01 角色包可以自行放宽高危工具权限（已修复）

**修复状态**：manifest 导入、读取和保存会拒绝 `allow` / `always_allow`、未知策略与非法工具名；前端编辑器不再提供 allow；permission 运行时即使遇到绕过校验的数据，也只解析 deny/ask，并按工具默认 effect 做单调收紧。新增清单策略与单调性回归测试。

**触发条件**：导入并启用的角色包在 `runtime.permissions` 中声明 `shell: allow`、`execute_tool: allow` 等，同时其 persona 引导模型调用对应工具。

**影响**：角色包既控制提示内容又控制权限 overlay，可以绕过原本的确认框；这形成从未受信内容包到 shell、截图、打开路径等系统能力的信任倒置。

**证据**：

- `src-tauri/src/pack/manifest.rs:185-194`：可导入清单包含权限映射。
- `src-tauri/src/pack/manifest.rs:531-540`：权限偏好原样返回。
- `src-tauri/src/pack/manifest.rs:705-732`：运行时校验没有验证权限键、值或是否放宽默认策略。
- `src-tauri/src/permission/mod.rs:156-186`、`253-299`：角色包 overlay 可把值直接转换为 Allow。
- `src-tauri/src/agent/runner.rs:526-588`：Allow 决策跳过用户确认。

**建议**：角色包只允许收紧默认权限，例如 Deny/AskEveryTime；任何放宽都应由独立 UI 展示差异、显式确认，并绑定包内容指纹。未知工具和未知策略必须拒绝。

### P1-02 Session 与 Project 权限规则没有按声明作用域隔离（已修复）

**修复状态**：Session 规则改为按 `session_id -> tool` 分桶并在会话删除时清理；Project 规则按 canonical workspace identity 写入版本化 `project_permissions.json`。runner 在任何异步初始化前捕获 turn-owned `PermissionContext`，裁决、审计和确认记忆复用同一身份；设置面板修改/清除规则也携带并校验具体 session/workspace identity，拒绝晚到操作。Project/User 规则与审计写入由同一进程锁串行，规则文件通过同目录临时文件、flush/sync 和 rename 原子替换；身份、读取、解析或版本校验失败均 fail closed。旧 `permissions.json` 不读取、不迁移、不作为新存储种子，界面会提示用户按项目重新授权。新增双会话、双项目、切换中确认、边界中间态、损坏/未知版本、并发更新、旧文件、删除会话与审计兼容回归测试。

**触发条件**：在会话 A 记住 Session Allow 后切到会话 B；或在项目 A 记住 Project Allow 后切到项目 B。

**影响**：高危工具授权会跨会话/项目继续自动生效，直接破坏“一个会话绑定一个项目”的安全预期。

**证据**：

- `src-tauri/src/lib.rs:54-55`：只有全局 `HashMap<String, PermissionRule>`，没有 session id。
- `src-tauri/src/permission/mod.rs:161-177`、`343-355`：Session 只以工具名为键；Project 保存时只使用 app data。
- `src-tauri/src/permission/mod.rs:561-568`：所有 Project 规则固定写同一 `permissions.json`。
- `src-tauri/src/lib.rs:1508-1586`：新建、切换、删除会话不切换或清理专属规则桶。

**建议**：Session 规则按 `(session_id, tool)` 分桶；Project 规则按 canonical workspace identity 分桶；审计记录同时包含 session、workspace 和实际能力。

### P1-03 undo 栈未绑定工作区，可静默修改错误项目（已修复）

**修复状态**：每条 undo entry 现在保存编辑发生时的 canonical workspace root；预览和执行都会先重新规范化当前工作区并要求身份完全一致，再解析相对路径和检查文件内容漂移。新增同相对路径、同 after 内容的跨项目拒绝测试，以及同一规范化工作区可撤销测试。

**触发条件**：在项目 A 编辑相对路径 `config.txt`，切到项目 B；B 的同名文件内容恰好等于 A 编辑后的内容，然后执行 undo。

**影响**：当前内容一致性校验会通过，项目 B 文件被写入项目 A 的旧内容，造成无提示跨项目数据破坏。

**证据**：

- `src-tauri/src/lib.rs:58-59`：全局 undo 栈。
- `src-tauri/src/tools/edit_file.rs:10-18`：entry 只有相对 path 与 before/after。
- `src-tauri/src/tools/edit_file.rs:128-140`：撤销按当前 `sandbox_dir` 重新解析相对路径。
- `src-tauri/src/tools/edit_file.rs:425-449`：写入记录时没有保存工作区身份。

**建议**：entry 保存 canonical workspace/root、目标真实路径和内容摘要；撤销前要求当前工作区完全一致；工作区切换时清理或过滤栈。

### P1-04 Windows 系统打开路径存在命令解释器注入（已修复）

**修复状态**：Windows 打开目标不再经过 `cmd.exe`，改为直接调用系统 `ShellExecuteW`，把目标作为单独的 NUL 结尾 UTF-16 `lpFile` 传入且不提供命令参数；内部 NUL 会被拒绝。新增元字符、引号、空格、URL query、本地路径与 NUL 截断回归测试。

**触发条件**：打开含 `&`、`|`、`<`、`>`、`^` 等元字符的允许协议 URL 或本地路径。

**影响**：`cmd.exe` 可能把目标后半部分解释为额外命令；deferred 工具和多个直接“打开目录”IPC 都会复用该入口。

**证据**：

- `src-tauri/src/tools/open_path.rs:16-22`：Windows 使用 `cmd /C start`。
- `src-tauri/src/tools/open_path.rs:34-48`：只检查 UNC/协议，没有拒绝命令元字符。
- `src-tauri/src/lib.rs:855`、`1397`、`1611`：角色包、技能和工作区打开命令复用该逻辑。

**建议**：改用 `ShellExecuteW`、安全系统 opener 或对应桌面插件，不经过命令解释器；加入全部元字符、引号、空格、URL query 和本地路径回归测试。

### P1-05 默认 Allow 的网页读取工具可访问本机、私网和链路本地（已修复）

**修复状态**：`http_get` 与 direct `web_fetch` 统一走 `safe_http`。仅接受无 credentials 的 HTTP(S) URL；首跳和最多 10 次重定向的每一跳都独立解析 DNS，并要求全部解析结果为公网 IPv4/IPv6。客户端通过 `resolve_to_addrs` 固定已验证地址、禁用环境代理和自动重定向，响应返回后再复核实际 peer；DNS、连接、逐读和单跳总请求均有限时。新增 loopback/private/link-local/CGNAT/reserved/multicast、IPv4-mapped IPv6、另类 IPv4 记法、混合 DNS、DNS pin、禁用自动重定向，以及完整“首跳响应 → 私网 Location → 第二跳连接前拒绝”测试。`source=exa`/`livecrawl` 仍是外部服务抓取，不声称经过本机 IP pin。

**触发条件**：请求 loopback、RFC1918、IPv6 本地、链路本地/元数据地址，或公开 URL 重定向到这些地址。

**影响**：可读取本机管理端点、内网服务或云元数据；结果进入下一轮模型上下文，可能造成敏感数据外带。部分 GET 端点还可能有副作用。

**证据**：

- `src-tauri/src/tools/http_get.rs:34-82`、`web_fetch.rs:53-83`、`222-238`：仅校验 http/https scheme。
- `src-tauri/src/tools/mod.rs:400-422`：两个工具默认 Allow，却声明只访问公开 URL。
- `src-tauri/src/lib.rs:2199-2202`：共享客户端没有逐跳私网重定向策略。

**建议**：首跳与每次重定向都拒绝 loopback、私网、链路本地、组播、未指定/保留地址；连接前复核实际 DNS 解析结果；本机开发服务使用独立显式授权。在闭环前先把工具改为 Ask。

### P1-06 Live2D 导入可把包外文件移动进包内（已修复）

**修复状态**：导入在包内同父级唯一 staging 目录完成，源目录仅复制普通文件并拒绝 symlink/junction、特殊文件、源根逃逸和目录循环。统一引用解析器只接受便携 Normal component，显式拒绝绝对/根路径、Windows prefix/盘符、`.`、`..`、ADS/保留名与不可移植尾缀；canonicalize 后还必须是候选模型根内的普通文件。Moc、Textures、Physics、Pose、DisplayInfo、UserData、Expressions.File、Motions.File/Sound 全部复用该边界，非 ASCII 引用统一移动到候选目录内的 ASCII 资源目录并重写。候选模型和 manifest 完整验证后才进入串行提交；旧模型目录与 manifest 均有同父级备份，目录/清单替换或最终复核失败会回滚。读取 bundle 也使用相同引用校验并拒绝链接逃逸。新增绝对/父目录/盘符、非 ASCII 全引用类型、链接、失败保留旧文件和提交故障回滚测试。

**触发条件**：模型 JSON 的资源引用使用绝对路径或 `../`，且路径含非 ASCII 字符以触发重命名分支。

**影响**：`fs::rename` 可把项目/角色包之外的已有文件移动到包内，原位置文件消失。导入还会先删除旧模型目录，后续失败会丢失旧模型。

**证据**：

- `src-tauri/src/pack/live2d.rs:150-188`：从不可信模型 JSON 读取资源引用。
- `src-tauri/src/pack/live2d.rs:203-268`：直接 `dest.join(rel)` 后 rename，没有绝对路径/父目录/containment 校验。
- `src-tauri/src/pack/live2d.rs:367-390`：已有安全解析器，但导入规范化阶段没有复用。
- `src-tauri/src/pack/live2d.rs:73-99`：验证新模型前先删除旧目录。

**建议**：只允许 Normal path component，拒绝根、盘符、`..` 和链接逃逸；在独立临时目录完成复制、重写和全量验证，成功后原子替换。

### P1-07 deferred 工具共享一个权限身份（已修复）

**修复状态**：runner 在生成工具事件与进入权限门前调用 `authorization_target_for_state`。有效 `execute_tool` 调用会解析并再次校验 `tool_name` 必须属于 deferred 白名单，随后以真实 target 名称和内层 args 执行规则查找、角色包 overlay、风险/default policy、确认 description/summary/preview、affected paths、remember 与两次审计；模型协议中的外层 tool call/result 仍保持 `execute_tool`，不破坏配对。非法、未知或 core target 不会被解析为真实能力，继续使用外层 Privileged/Ask 兜底且执行层再次拒绝。新增解析白名单、open_path/screen metadata 映射，以及“open_path Session Allow 不命中 screen_capture_region、审计无 wrapper 身份”回归测试。旧的宽泛 `execute_tool` Allow 不再授权有效 target。

**触发条件**：用户允许 `execute_tool` 执行低敏目标并选择 Session/Project/User 记忆，之后模型用同一 wrapper 调截图、OCR 或打开路径。

**影响**：后续真实能力不再得到目标级确认；审计也只记录 wrapper，无法表达实际敏感操作。

**证据**：

- `src-tauri/src/tools/execute_tool.rs:20-28`：一个 wrapper 分派多种敏感能力。
- `src-tauri/src/tools/mod.rs:598-611`：注册表只有统一权限策略。
- `src-tauri/src/agent/runner.rs:526-565`、`permission/mod.rs:302-349`：按外层名称决策和记忆。

**建议**：权限/审计身份使用 `execute_tool:<actual_tool_name>`，并使用目标工具自己的风险、预览和 affected paths；或禁止 wrapper 使用 Once 之外的记忆作用域。

### P1-08 角色包 IPC 的 id 可重新定义信任根（已修复）

**修复状态**：新增统一 `resolve_pack_dir`：id 不能为空、含空白、超过 128 字节或包含 ASCII 字母数字、`-`、`_` 之外的字符；解析器 canonicalize `packs` 根与现有目标，要求目标是根的直接子目录，并通过 `symlink_metadata` 与 Windows reparse attribute 拒绝 symlink/junction。manifest、文件浏览/读取、lore、Live2D、打开目录、设置中的当前包、pack memory/skills 与梦境材料全部复用该边界；zip 新目标先验证 id 和 canonical 父根，临时目录不再清理碰巧同名的既有路径。无错误返回的上下文路径在校验失败时省略 pack 来源。新增直接子目录成功，以及绝对路径、盘符、UNC、父目录、嵌套、非 ASCII、目录链接、外部 memory/skills 旁路拒绝回归。

**触发条件**：IPC 传入绝对目录、不同盘符、UNC 或含 `..` 的包 id，再调用列表/读取等命令。

**影响**：后续 containment 只证明目标位于调用者指定目录，并未证明位于真正 `packs_dir`；可枚举并读取外部 md/txt/json，复用入口还可能越界修改/删除。

**证据**：

- `src-tauri/src/pack/manifest.rs:482-484`：`pack_dir` 只是 `packs_dir.join(id)`。
- `src-tauri/src/pack/files.rs:106-185`：列表/读取把传入 id 作为新的 base。
- `src-tauri/src/lib.rs:877-894`：IPC id 直接传入。

**建议**：在 IPC 边界统一验证 id，只允许 packs 根的直接子目录；canonicalize 根和目标并验证 direct child；所有 pack API 复用同一受检解析器。

### P1-09 前端会话、历史和工作区没有事务边界（已修复）

**修复状态**：新增后端 `NavigationSnapshot { session_id, sessions, history, workspace, goal }`，在同一 SessionStore 锁内捕获会话投影，workspace 按同批捕获的 session-owned path 检查；`new_session`、`select_session`、`delete_session` 直接返回快照，选择/删除的 workspace 同步失败会恢复旧 active/SessionStore，权限规则只在删除事务成功后清理。前端 `NavigationEpoch` 用单调 epoch 作废旧导航，并用同 epoch request 序号保证并发刷新最后发起者胜出；每次提交同时核对 expected/actual session id 与会话列表一致性。`navigationPending` 以同步 ref + state 锁住 Sidebar、新消息、Goal、语音和 workspace 入口。tool-end、workspace、session、goal 与 turn 结束刷新全部走受检快照。主时间线改为消费 `agent-event` 的 `turn.session_id`，确认/Goal 载荷增加 `session_id`，非当前会话事件直接丢弃。新增后端 A 历史/A workspace/A goal 绑定测试，以及前端 A/B 延迟乱序、同 epoch 旧刷新和跨会话事件测试；生产构建通过。

**触发条件**：快速点击多个会话，或 tool-end/工作区刷新与会话切换交错；慢请求后返回。

**影响**：UI 可能显示“会话 A 高亮 + 会话 B 历史 + 项目 C 工作区”，而后端 active session 又是另一状态。用户下一次发送或批准工具时可能作用于并非界面所示的项目。

**证据**：

- `src/App.tsx:511-534`：sessions/workspace 刷新无请求序号，workspace change fire-and-forget 刷会话。
- `src/App.tsx:678-697`：tool-end 启动无 session 归属的工作区刷新。
- `src/App.tsx:914-945`：会话选择/删除通过多个独立请求拼快照，无 navigation lock/epoch。
- `src/components/Sidebar.tsx:188-195`：只按 Agent busy 禁用，会话导航本身不互斥。

**建议**：引入单调 navigation epoch 和 `navigationPending`；最好由后端原子返回 `{session_id, sessions, history, workspace, goal}`，提交前核对 id；所有晚到刷新丢弃。

### P1-10 分支列表缓存没有绑定项目（已修复）

**修复状态**：`BranchSwitcher` 在项目变化时关闭菜单、清空旧列表并递增请求 generation；加载期间不渲染或点击旧项，晚到响应必须同时匹配项目路径和请求 id。分支枚举与切换 IPC 都携带 `expected_workspace_path`，后端 canonicalize 后要求它等于当前工作区；切换期间持有工作区锁，消除校验与 `git switch` 之间的切换窗口。新增后端 stale/current 路径回归测试。

**触发条件**：项目 A 加载过分支后关闭菜单，切到项目 B，再打开；B 列表返回前点击仍显示的 A 分支。

**影响**：分支名在当前项目 B 中解释；同名时可能切换 B 的错误分支，不同名时产生误导性错误。dirty 状态也可能过期。

**证据**：

- `src/components/BranchSwitcher.tsx:19-64`：branches 跨打开保留，load 开始不清空、不绑定 `workspace.path`。
- `src/components/BranchSwitcher.tsx:82-114`：切换只传 branch name，loading 时旧项仍可点击。

**建议**：项目变化时关闭菜单并清缓存；loading 时不展示/禁用旧项；响应绑定 generation；后端同时验证 expected workspace path。

### P1-11 turn 起始会话与实际写入会话可能不同（已修复）

**修复状态**：`TurnHandle` 保存 `begin_turn` 捕获的 session id，普通发送、带 Agent 发送、slash、Goal 控制与自动续跑都把它显式传给 runner；runner 在任何异步初始化前验证目标并据此构造 `SessionTurnStore`、prompt 和会话内工具上下文，不再从全局 active 推断写入目标。会话删除与 turn 登记使用同一锁序，Running/Cancelling 回合所属会话不可删除。新增可控初始化暂停测试，验证切到同项目会话 B 后，A 的 user/tool/assistant 消息仍只写入 A。

**触发条件**：会话 A 发送后，外部服务初始化 await；期间切换到使用同一工作区的会话 B。

**影响**：Turn 元数据指向 A，但消息、工具结果和回复可能写进 B；事件过滤与磁盘历史互相矛盾。

**证据**：

- `src-tauri/src/lib.rs:368`：send 开始捕获 A 并登记 turn。
- `src-tauri/src/agent/runner.rs:182-204`：长初始化后重新读取当前 active session。
- `src-tauri/src/workspace.rs:242-255`：busy 时仍允许切到同工作区会话。
- `src-tauri/src/lib.rs:1530-1545`：该切换可以成功。

**建议**：把 begin_turn 捕获的 session id 显式传给 runner；整个 turn 不再从全局 active 推断目标；用可控初始化延迟增加并发回归。

### P1-12 外部工具 read-only 注解可覆盖 Ask（已修复）

**修复状态**：协议 annotation 与本地授权风险已经分离。动态外部工具只能映射到 `External`/`Privileged`，`readOnlyHint` 仅保留给 UI 展示和并发提示；工具默认权限继续固定为 `Ask/Once`，确认摘要同时展示本地有效风险与服务端原始 hints。Auto 现在先执行完整本地规则链，不再以 `ReadOnly` 分类覆盖默认或显式 `Ask`/`Deny`；未批准 Plan 也不会把动态外部工具当作本地只读能力。新增风险映射、工具定义/视图和 Auto 会话/项目/用户规则优先级回归测试。

**触发条件**：外部工具服务恶意或错误地自报 `readOnlyHint=true`，用户处于 Auto 模式。

**影响**：有副作用的动态工具可无确认执行；协议提示性元数据被错误升级为本地授权事实。

**证据**：

- `src-tauri/src/mcp/mod.rs:833-916`：外部注解映射为 `ToolRisk::ReadOnly`，尽管默认 permission 是 Ask。
- `src-tauri/src/permission/mod.rs:189-209`：Auto 对所有 ReadOnly 直接 Allow。

**建议**：外部动态工具风险下限固定 External/Privileged；注解只影响 UI/并发提示，不能降低权限；Auto 必须先尊重显式 Ask。

## 4. P2：中风险发现

### P2-01 第三种供应商适配器仍使用脆弱的独立 SSE 拆行

**触发/影响**：EOF 最后一条 data 无 LF 时事件丢失；合法多行 data 会逐行按 JSON 解析并静默失败；命名 error 事件和顶层 error 对象没有明确错误路径，最终可能出现成功但空白的答复。

**证据**：

- `src-tauri/src/llm/gemini.rs:67-93`：自有 `Vec<u8>` 只按 LF 拆单行 data，EOF 不 flush。
- `src-tauri/src/llm/gemini.rs:223-230`：JSON 失败静默返回。
- `src-tauri/src/llm/mod.rs:520-523`：空 finish 被归一为 stop。

**建议**：复用公共 `SseDecoder`；显式处理 event/data error；JSON 格式错误返回受控诊断；增加无尾换行、多行 data、error、逐字节分片契约测试。

### P2-02 主要 SSE 适配器不验证完整终止，解码器也没有大小上限

**触发/影响**：代理在部分正文后 clean EOF，但未发 `[DONE]`、`message_stop` 或明确 finish reason；当前会把截断正文标成正常完成。端点持续发送无换行字节或不结束事件时，buffer/data_lines 又会无界增长。

**证据**：

- `src-tauri/src/llm/openai.rs:47-72`、`anthropic.rs:73-98`：EOF 后直接 finish，不要求协议终止。
- `src-tauri/src/llm/sse.rs:16-20`、`40-42`、`94-109`：行/事件缓冲无字节上限。
- `src-tauri/src/llm/mod.rs:520-523`：缺 finish reason 默认 stop。

**建议**：跟踪协议 terminal marker；用户取消除外，缺终止条件返回 incomplete/error；设置单行、单事件、累计回复硬字节上限。

### P2-03 网页工具先完整下载响应再截断

**触发/影响**：超大 Content-Length、chunked 大流或膨胀正文会先完整进入内存，`context_max_characters` 不能保护进程。与 SSRF/default Allow 组合时风险更高。

**证据**：

- `src-tauri/src/tools/http_get.rs:51-56`、`web_fetch.rs:80-102`：`resp.text().await` 后才截断。

**建议**：统一有界响应读取器；预检 Content-Length，流式读取按字节硬截止，达到上限立即关闭响应；转换前后都保留上限。

### P2-04 点击停止不能真正取消正在等待的网络或工具

**触发/影响**：服务端不再发 chunk、长 shell、MCP tool 或其他异步工具运行中点击停止；代码只在 await 返回后检查 AtomicBool，副作用仍可能继续数十秒到数分钟。

**证据**：

- `src-tauri/src/llm/openai.rs:33-49`、`anthropic.rs:58-75`、`gemini.rs:54-71`：send/next await 不可唤醒取消。
- `src-tauri/src/agent/runner.rs:581-588`：进入 `tools::execute().await` 后不再监听。
- `src-tauri/src/tools/shell.rs:377`、`mcp/mod.rs:437`：没有统一取消上下文。

**建议**：使用 `CancellationToken`/watch/Notify；网络 send/next、外部请求和工具执行使用 `tokio::select!`；shell 取消时终止整棵进程树。

### P2-05 shell 在子进程退出前不排空 stdout/stderr

**触发/影响**：命令输出超过 OS pipe 容量时，子进程阻塞写管道，无法退出；父进程只轮询 try_wait，最终把正常构建/测试误判为超时。

**证据**：

- `src-tauri/src/tools/shell.rs:395-416`：stdout/stderr pipe 后只 try_wait，退出后才 `wait_with_output`。
- `src-tauri/src/tools/shell.rs:768`：截断发生在完整收集之后。

**建议**：改异步子进程并发读取两路输出；用有界 ring buffer 持续排空；将超时、取消、进程树终止放进同一 select。

### P2-06 设置和会话持久化不是原子写，错误还可能被静默忽略

**触发/影响**：进程在 truncate/写入中退出，磁盘满或短暂 I/O 失败；整个 JSON 可损坏，下次启动静默回退默认/空会话，用户看不到保存失败。

**证据**：

- `src-tauri/src/store/mod.rs:611-646`、`638`、`677`：直接 `fs::write`，解析失败回退。
- `src-tauri/src/lib.rs:131-139`：后台保存错误被丢弃，序号仍标为已写。

**建议**：同目录临时文件 + flush/fsync + 原子替换；保留最近备份；损坏文件隔离并向 UI 报告；应用退出等待保存队列。

### P2-07 外部工具初始化存在 check-then-await 和孤儿进程风险

**触发/影响**：面板刷新与对话同时初始化，或子进程在 initialize/tools/list 阶段失败；可能启动重复、被覆盖或 manager 不再跟踪的进程。

**证据**：

- `src-tauri/src/mcp/mod.rs:341-359`：检查后释放锁再 await，无 Pending/每服务互斥。
- `src-tauri/src/mcp/mod.rs:483-524`：child 无 kill_on_drop，失败路径无显式 kill。

**建议**：每 server 使用 Connecting 状态/异步互斥；RAII 构建器只在完整成功后转移 child；启用 kill_on_drop 并在错误路径 kill + wait。

### P2-08 历史恢复把失败/拒绝工具无条件显示为成功

**触发/影响**：重开含 denied/failed 编辑的会话；文件卡片显示绿色“已编辑”，形成错误审计信息。

**证据**：

- `src/App.tsx:190-222`：`buildHistory` 对每个 tool call 写死 `status: "done"`。
- `src/components/ToolCard.tsx:120-131`：done 映射为“已编辑”。

后端历史只保存 result 文本，缺 ok/denied/duration/error metadata，前端无法可靠重建。字符串匹配不是长期方案。

**建议**：持久化结构化执行元数据或 UI event journal；迁移前将不可判断状态显示为 Unknown/Completed，不显示成功语义。

### P2-09 项目树、changes 和 preview 的旧响应可覆盖新项目

**触发/影响**：慢磁盘/网络目录中切项目或快速点文件；旧请求完成后无条件写相同 state key，可能显示 A 的树却按 B 的根读取相对路径。

**证据**：

- `src/components/WorkspaceExplorer.tsx:74-117`：directory/changes 响应无 workspace generation。
- `src/components/WorkspaceExplorer.tsx:146-166`：preview 无 request id，晚到 A 可覆盖 B。

**建议**：组件维护 generationRef/previewRequestRef；捕获 `{workspace.path, generation, relativePath}`，全部仍匹配才提交。

### P2-10 流式帧会重渲染历史 Markdown、强制平滑滚动并重复渲染 Mermaid

**触发/影响**：长会话或模型输出较长 Mermaid；每个 rAF 增量让历史消息重新解析 Markdown/KaTeX/highlight，连续重启 smooth scroll，把用户从上方阅读位置拉回；Mermaid 多个不可取消任务共享 DOM id。

**证据**：

- `src/App.tsx:1688`、`MessageList.tsx:72-118`、`174`：内联 retry handler 破坏 memo。
- `src/components/MessageList.tsx:148`：每次 items 变化都 smooth scroll。
- `src/components/MarkdownRenderer.tsx:37`、`MermaidBlock.tsx:30-45`：流式阶段每次 chart 变化都 parse/render。

**建议**：稳定 handler、memo ToolCard/历史子树；只在用户原本接近底部时自动滚；流式帧用 auto；Mermaid 在消息完成后只渲染一次。

### P2-11 最终完整正文没有成为权威值

**触发/影响**：IPC 压力、页面恢复或事件迁移漏掉任一 delta；虽然 done 带完整正文，当前只在累计内容为空时回填，界面永久保留残缺文本。主时间线的 session/turn 归属已随 P1-09 修复，不再属于本项剩余风险。

**证据**：

- `src/App.tsx:606-614`：只要已有任意正文就丢弃 done 完整文本。
- `src/App.tsx` 的 `onAssistantDone`：已有累计正文时仍保留累计值，而不是使用非空 done 正文复核/修复。

**建议**：非空 done 文本作为 canonical value，并在不一致时记录诊断；后续 reducer 还应按 turn id 处理重复与乱序事件。

### P2-12 Git 项目切到非 Git 项目后可能停在不可用 Changes 页

**触发/影响**：在 Git 项目的 Changes tab 中切换到非 Git 项目；tab state 未重置，按钮虽禁用，内容仍显示“工作区没有未提交更改”。

**证据**：

- `src/components/WorkspaceExplorer.tsx:63`、`110-117`：项目变化未重置 tab。
- `src/components/WorkspaceExplorer.tsx:314-371`：非 Git 只禁用按钮，仍按旧 tab 渲染 clean 文案。

**建议**：workspace path 变化时回到 Files；`!workspace.is_git` 时强制 Files。

## 5. P3：低风险发现

### P3-01 新增树、菜单、页签与展开卡缺少完整键盘/ARIA 语义

- 分支菜单不处理 Escape、方向键、menu/listbox/current item；trigger 缺 `aria-haspopup` 与受控 id。
- 文件树缺 tree/treeitem/`aria-expanded`；页签缺 tablist/tab/`aria-selected`。
- ToolCard 展开按钮缺 `aria-expanded`/`aria-controls`；更改圆点只有视觉信息。

证据：`BranchSwitcher.tsx:27-145`、`WorkspaceExplorer.tsx:206-324`、`ToolCard.tsx:175-207`。

### P3-02 固定 360px 项目面板在最小窗口宽度压缩聊天区

应用允许缩到 980px，同时展开 230px 侧栏和 360px 项目面板后，聊天主列约剩 370px；Composer 控制条不换行。证据：`WorkspaceExplorer.tsx:257`、`Sidebar.tsx:89`、`Composer.tsx:536`。

建议在中小宽度使用 drawer/overlay 或 `clamp()`，必要时自动折叠侧栏，并为 980/1280/1811px 建立布局回归。

### P3-03 全仓 Rust 格式门禁当前未通过（已修复）

**修复状态**：P1-06 对 `live2d.rs` 的业务修改已统一经过 rustfmt，原换行差异自然消失；`cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` 已恢复通过。

初始审查时，`cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` 曾在 `src-tauri/src/pack/live2d.rs` 报告一处纯换行格式差异。P1-06 的业务修改统一经过 rustfmt 后，该差异自然消失；当前全仓格式门禁已通过，本项关闭。

## 6. 流式协议与渲染专项结论

OpenAI-compatible 与 Anthropic 两类适配器已经把正文、思考、工具调用、usage、finish 和 error 归一成统一后端增量，前端不直接消费供应商原始 SSE。因此**只要后端适配正确，供应商切换本身不会改变 Markdown、代码块、公式、Mermaid、自动滚动或文字动画组件**。

当前差异主要在归一化前后两个边界：

1. 归一化前：第三种适配器尚未复用公共 SSE 解码器；两个主要适配器又没有验证完整终止，公共解码器无大小上限。
2. 归一化后：主时间线已按统一信封的 session id 过滤；done 完整正文仍不总是覆盖累计增量，流式 Mermaid 和历史重渲染仍存在性能问题。

所以结论不是“协议差异完全无影响”，而是“前端组件路径和会话归属已经统一，但适配器完整性与最终文本校验仍会影响用户看到的完整性与错误状态”。详细矩阵见 [流式协议评估](./streaming-protocol-assessment.md)。

## 7. 做得较好的部分

- `workspace.rs` 对相对路径先做词法检查，再 canonical containment；静态符号链接逃逸有跨平台测试。
- 工作区文件预览使用 `take` 限制实际读取量，能识别二进制和 UTF-8 截断边界。
- Git 分支切换使用参数数组和 `--`，没有把分支名拼进 shell。
- 公共 `SseDecoder` 正确处理 transport chunk、UTF-8 字节分片、CR/LF/CRLF、多行 data、注释和 EOF flush。
- 两个主要适配器分别归一化正文、思考、工具参数、usage、finish 和流内错误，原始方言没有直接泄漏给前端。
- API key、WebDAV 密码和标记 secret 的外部工具环境变量经系统凭据存储；设置/备份前有脱敏闸。
- ZIP 角色包导入限制 id、相对路径、文件数量、压缩包和解压总量，并使用临时目录；问题集中在独立 IPC 与模型文件夹导入入口。
- 普通文件工具的根目录解析既拒绝绝对路径/父目录，也检查已存在祖先真实路径。
- reduced-motion 会关闭新增流式和编辑活动动画。

## 8. 修复顺序

### 第一批：阻断信任升级与跨项目写入

1. 角色包权限只允许收紧；Session/Project 权限真正分桶。
2. undo 绑定 workspace identity；pack id 统一受检；Live2D 事务导入。
3. Windows opener 去除 `cmd`；HTTP 工具补 SSRF/重定向边界。
4. deferred 目标级授权；外部工具注解不降低风险。

### 第二批：绑定会话/项目身份

1. turn 固定 session id；前端原子 session snapshot + navigation epoch。（已完成）
2. 分支命令带 expected workspace；Explorer/preview/changes 使用 generation。
3. 时间线迁移统一事件信封（已完成）；历史工具状态结构化持久化仍待处理。

### 第三批：流式、取消与持久化可靠性

1. 所有供应商共用有界 SSE 解码；验证 terminal marker；done 作为 canonical。
2. 统一 CancellationToken；shell 并发排空输出；MCP 初始化 RAII。
3. 设置/会话原子写、备份与可见错误。
4. 前端 memo、近底部滚动策略、流结束后 Mermaid 渲染。

## 9. 建议测试矩阵

- 未受信角色包：`shell: allow`、未知工具/策略、包切换后的权限差异。
- 双会话/双项目：Session/Project Allow/Deny 互不串用；turn 延迟初始化期间切换。
- 文件系统：绝对 pack id、`..`、UNC、不同盘符、目录链接、Live2D 内部绝对/父目录引用。
- undo：两个项目同相对路径、同 after 内容，必须拒绝跨项目撤销。
- Windows opener：`& | < > ^ % ! ( )`、引号、空格、URL query、本地路径。
- HTTP：localhost、IPv4/IPv6 私网、链路本地、公开→私网重定向、DNS 重绑定、超大 chunked 响应。
- SSE：逐字节、CR/LF/CRLF、多行 data、无尾换行、error、clean EOF、缺终止标志、超大事件、取消。
- 前端竞态：A/B session/history/workspace 延迟乱序；A 项目请求晚于 B；分支旧缓存不可点击。
- 时间线：漏 delta 后 done 自愈；跨 session/turn、重复、乱序事件；denied/failed 历史保持真实状态。
- 性能/交互：500 条历史流式 render count、Mermaid 完成后仅渲染一次、用户上滚不被拉回。
- shell/MCP：大量 stdout/stderr、子孙进程、取消/超时、并发初始化、握手失败后无孤儿进程。
- 持久化：磁盘满、替换失败、进程中断、关闭时仍有保存任务。
- 980/1280/1811px 布局、Escape/方向键、屏幕阅读器与 reduced-motion。

## 10. 本轮实际改动与边界

- 已更新仓库内所有既有 Markdown，并新增本报告。
- 已把项目工作区、Git、会话绑定、编辑活动、流式动画与协议评估同步到 README、实现说明和模块文档。
- 已把历史报告明确标成历史快照，修正 Voice、向量召回和构建 warning 等陈旧描述。
- 已把本报告确认的 P1/P2 修复项同步到 [TODO](./TODO.md)。
- 本轮没有修改业务代码，也没有把发现标记为“已修复”；构建/测试通过只记录当前基线。
