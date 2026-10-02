# 首批复杂度与执行边界优化验收

日期：2026-10-02。首批优化基线：`9c72d3f4214d13eb5f2152f87ef8eb44acebf3ac`，提交为 `df9840c`；以下首批记录保留其当时验收结果。随后同步最新 main `bc763c4` 的结果见本文末尾“main 同步后的合并验收”。结论限定于列出的自动化覆盖范围，不等于所有桌面能力或外部服务已经端到端验证。

## 实际完成的改变

| 范围 | 实现和行为变化 | 入口 |
| --- | --- | --- |
| Goal 与流事件 | 回答拥有独立 `response_id`；Goal 可以在同一个外层 turn 内显示多次回答，旧回答/旧 turn 不得重开；事件发射器捕获身份，不跟随新的活动 turn | [session_engine.rs](../backend/Demiurge-desktop/src/agent/session_engine.rs)、[agentEventReducer.ts](../frontend/src/lib/agentEventReducer.ts) |
| 前端消息 | 集中历史重建、回答/工具关联、帧缓冲、权威正文修复、错误去重和缺失回答恢复；工具前导不再误判成最终回答 | [messageProjection.ts](../frontend/src/lib/messageProjection.ts) |
| Workflow | 启动时固定 session、规范项目根、渲染后的定义和独立取消令牌，贯穿 Prompt、模板、文件工具、日志、快照、重试与恢复 | [execution_context.rs](../backend/Demiurge-desktop/src/agent/execution_context.rs)、[workflow_runtime.rs](../backend/Demiurge-desktop/src/agent/workflow_runtime.rs) |
| 工具执行 | 完整封装真实 target 解析、权限、确认、取消再检查、执行、审计及配对结果；模型保留完整结果，UI 只截断预览 | [tool_call.rs](../backend/Demiurge-desktop/src/agent/tool_call.rs) |
| 权限确认与停止 | 确认绑定正在运行的 session/turn；取消后不能注册等待器或重新弹出旧确认；回复只消费一次，超时/发射失败/future 释放均清理；摘要或路由取消按 interrupted 结束 | [permission/mod.rs](../backend/Demiurge-desktop/src/permission/mod.rs)、[confirmationState.ts](../frontend/src/lib/confirmationState.ts) |
| 权限/Shell 设置 | 模块拥有草稿、加载、刷新、保存、重置和异步失效；保留原作用域身份，跨 tab 保留草稿，失败保留旧视图并显示错误 | [PermissionSettings.tsx](../frontend/src/features/settings/PermissionSettings.tsx)、[permissionSettingsModel.ts](../frontend/src/features/settings/permissionSettingsModel.ts) |
| 会话保存 | 在同一个 sessions 锁内克隆快照并分配序号，防止旧内容因晚分配序号覆盖新内容；只有成功落盘才推进写入序号 | [starter/state.rs](../backend/Demiurge-desktop/src/starter/state.rs) |

主协调文件从 App 1984 行降至 1622 行、SettingsDialog 5030 行降至 4652 行、runner 862 行降至 568 行。行数只用于说明职责迁出的位置；真正的变化是调用者不再协调消息内部 refs、工具事务步骤或权限设置请求生命周期。新增行为测试和模块代码不计作代码量节省。

## 先复现，再修复的证据

以下问题在修复前都运行了失败测试，修复后对应测试通过：

- Goal 同一外层 turn 的第二次回答被第一次 done 封锁。
- Workflow 在切换项目后把 journal/state 写入新的全局项目目录。
- 用户批准后发生取消，仍调用真实文件写入工具。
- 已取消的执行仍注册新的确认并接受允许回复。
- 旧快照暂停、新快照先落盘、旧任务恢复后，真实 `sessions.json` 从 newer 被覆盖成 older。

独立复查另发现摘要取消误报模型错误、停止后迟到确认重新弹窗；两条均已修正。确认 owner gate 与等待器被纳入同一可测产品函数，测试只替换原生事件发射适配器，不绕过身份校验。旧的消息正文源码正则检查由消息投影行为测试替代，其余既有架构接线检查保留。

## 最终验收结果

| 门禁 | 结果 | 范围 |
| --- | --- | --- |
| `npm test` | 78 通过；0 失败、0 跳过 | 前端逻辑、异步行为与现有源码/资源契约检查 |
| `npm run build` | 通过 | 当前 TS5 类型检查和 Vite 生产构建，3421 modules；保留既有 Live2D 大 chunk 提示 |
| `cargo fmt --manifest-path backend/Cargo.toml --all -- --check` | 通过 | 全工作区 Rust 格式 |
| `cargo test --manifest-path backend/Cargo.toml --workspace --all-targets --locked --offline` | 352 通过；0 失败、0 忽略 | Desktop 330、架构边界 4、Common 5、Core 6、Framework 7 |
| 同上，加 `--all-features` | 352 通过；0 失败、0 忽略 | 验证可选 feature 编译和测试；不代表本地 embedding 预留实现已经可用 |
| Edge 无头浏览器 smoke | 9/9 通过；页面异常 0、未知 IPC 0 | 实际 App、Composer、消息模块、设置页面和浏览器音频；只替换 Tauri 原生适配器 |
| `git diff --check` | 通过 | 当前 Git 换行配置下检查，无空白错误 |
| 独立代码复查 | 无剩余可复现阻断项 | Workflow 根与取消、恢复、锁顺序、工具事务、确认等待器及 runner 取消路径 |

浏览器测试可复做命令、工具路径和每个场景见 [browser/README.md](../frontend/tests/browser/README.md)。9 个场景覆盖：Goal 连续回答与旧回答拒收；错误事件与 IPC 拒绝的两种顺序；缺失最终事件的历史恢复；停止后迟到合成不播放；真实 HTMLAudioElement 播放暂停；跨会话确认隔离；同会话新 turn 拒绝旧确认；权限草稿跨 tab 保留及刷新失败保留旧规则。

Rust 新行为测试还覆盖：真实临时文件的允许/拒绝/路径错误/取消及结果配对；deferred target 审计；确认重复回复、迟到回复、超时与释放清理；项目 A/B 文件、Prompt、模板和日志归属；定义文件删除后的捕获定义；旧快照和错项目快照；真实本地 HTTP 无后续 SSE 数据时取消子 Agent；乱序写盘、I/O 失败后重试同一快照及损坏主文件的备份恢复。

## 兼容性与实现边界

- Unified agent event 新增 `response_id`，前端仍识别没有此字段的旧回答事件。确认新增 `turn_id`，前端拒绝没有执行归属的确认；前后端应成套发布。
- Workflow 快照 schema 升至 2，保留 schema 1 的读取能力。旧快照/旧日志缺少可靠 owner 或捕获定义时不能直接恢复或重试；重新启动一个运行。复制到其他项目的快照不能获得新的执行归属。重启后的运行仍是 StaleRunning，不自动重新执行。
- 主聊天继续串行，未扩展多主会话并发；Workflow 只读工具的项目根已经固定。正在进行的 HTTP 工具需要等待返回或超时，不承诺所有副作用可立即取消或回滚。
- 会话保存仍为后台异步快照，返回不代表落盘完成；尚未新增自动重试、关闭 flush 或数据库迁移，退出前未提交的数据仍可能丢失。测试中的“失败重试”证明同一快照可重新提交，不表示生产已经安排自动重试。
- 浏览器 smoke 使用真实前端但模拟原生适配器；没有执行打包后的 Tauri 原生启动/Workflow 派发整链，也没有验证真实第三方模型、凭据、录音、OCR 或强制退出恢复。因此本轮通过的是上述自动化变更验收，不是全部产品的发布认证。
- 设置目前拆出权限/Shell 域；Provider、角色包、记忆等剩余域，以及导航快照完整封装、AppState/AppHandle 依赖继续收缩，仍是后续独立批次。

## TypeScript 7 结论

固定提交、相同检查输入、预热后交替测量：TS5.9.3 → TS7.0.2，类型检查中位数 **5.222s → 0.760s（6.88 倍）**；类型检查 + Vite 的构建有效负载 **19.904s → 15.339s（节省 22.9%）**。TS7 主要优化前置类型检查，不能消除 Vite、Rust 或安装包阶段耗时。

原 tsconfig 不能直接升级：需要处理移除的 baseUrl、paths 相对前缀、字体副作用导入声明和默认全局类型行为。本轮没有升级默认依赖；在临时兼容配置下，优化后的全部 71 个前端 src 文件也已通过 TS7 单次 noEmit 检查。原始数据、官方来源、配置差异与未测场景见 [TS7-FEASIBILITY.md](TS7-FEASIBILITY.md)。建议把正式升级作为一项独立可回退变更。

## main 同步后的合并验收

同日追加。首批优化开始前没有先把最新 main 合入 Dan；已推送的 `df9840c` 基于 `9c72d3f`。本次重新获取远端后，将 `origin/main` 的 `bc763c4c33eeaf4d9bf52f9dd613774f5b7d5027` 合入 Dan，保留图片输入、视觉路由、Minecraft 场景与此前执行边界优化。合并方向是 main → Dan；向 main 合入 Dan 是后续操作。

四处显式冲突已按双方意图解决。前端场景标签和静默回复过滤迁入 MessageProjection，多模态发送与权限设置模块均保留。调用链复查还发现并修复以下兼容问题：

- Fork 子 Agent 继承图片时选择视觉模型，禁用未经确认支持图片的文本 fallback。
- Minecraft 输入固定 session 与来源，不自动续跑桌面 Goal；排队后项目不匹配即拒绝执行，不重绑到新项目。
- 主 runner 返回明确的完成、取消、预算耗尽和步数上限结果。Minecraft 只消费本次完成结果，不从全会话最后一条 assistant 猜测回复；显式 Interrupted 也阻止 Goal 续跑。
- Minecraft 外发在所属 turn 释放 busy 前完成，每段重查取消与执行身份；自动回复接入真实 MCP 工具的权限规则、确认和审计，未批准的 Plan 与拒绝规则阻止外发。批准后取消仍不能继续发送。

| 合并后门禁 | 结果 |
| --- | --- |
| 完整 `npm test` | **82 通过**；0 失败、0 跳过 |
| `npm run build` | 通过；3423 modules；仅保留既有 Live2D 大 chunk 提示 |
| Rust 默认配置、全工作区、全部 target、锁定依赖、离线测试 | **377 通过**：Desktop 353、架构边界 4、Common 7、Core 6、Framework 7；0 失败、0 忽略 |
| 同上并启用 `--all-features` | **377 通过**；0 失败、0 忽略 |
| Rust 全工作区格式检查、Git 空白检查 | 通过；同时修正 main 新测试的一处格式差异 |
| 真实 Edge smoke | **10/10 通过**；页面异常 0、未知 IPC 0；新增实际 Composer 图片附件经多模态 IPC 发送及历史重载 |
| 独立复查与联动核对 | 未发现剩余阻断问题；同 turn 的回答结束后仍可接收合法外发确认，完成/取消后拒绝迟到确认 |

定向回归包括真实子 Agent → 本地 HTTP 503：请求携带图片并使用配置的视觉模型，失败只记录一次请求，没有转向文本模型。Minecraft 测试覆盖来源与项目归属、明确完成结果、分段取消、失败传播、权限拒绝及批准后取消；外部发送使用替身，没有连接实际游戏服务器或真实 MCP 子进程。已发送分段不能撤回，取消检查只能阻止后续发送，不能保证撤销已经在途的外部操作。

main 新增的 `check-no-ocr-link` 是仅用于验证的链接替身；因此全部 feature 测试不代表 OCR 推理可用，也不能作为可运行安装包配置。默认配置测试单独通过；两组均未运行真实 OCR/LLM/Minecraft/Tauri 打包端到端。TS7 的性能测量仍对应原固定基线，未在本次包含 Minecraft 的合并版本重测，不将旧数据冒称为新版本性能。
