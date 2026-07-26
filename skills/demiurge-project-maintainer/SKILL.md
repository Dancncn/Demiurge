---
name: demiurge-project-maintainer
description: 按统一的模块归属、文件命名、依赖方向、作者追溯、验证流程和变更交接规范开发、重构、审查或维护 Demiurge Tauri/Rust/React 项目。当任务涉及 Demiurge 的代码修改、新功能、缺陷修复、架构调整、文件移动、IPC 契约变更、测试补充或 AI 辅助维护时使用。
---

# Demiurge 项目维护规范

确保每次修改都容易定位、审查、测试、追溯和回滚。除非用户明确批准迁移，否则保留现有业务行为和外部契约。

## 每项任务开始时

1. 定位仓库根目录并阅读 `docs/MODULAR-ARCHITECTURE.md`。
2. 阅读目标位置附近的 README、清单文件、实现、测试，以及适用的 `AGENTS.md` 或 `CODEOWNERS`。
3. 编辑前检查 `git status`，保留与任务无关的用户修改。
4. 创建文件前，先说明代码应归属的模块及完整调用链。
5. 识别并保持以下契约稳定：
   - Tauri 命令名和事件名。
   - IPC 参数及响应的序列化格式。
   - 设置键及其默认值。
   - 持久化 JSON 字段和恢复行为。
   - Pack 格式、文件路径和 Provider 语义。

决定代码位置时，必须阅读 [references/architecture-map.md](references/architecture-map.md)。
新建或重命名文件、类型、函数时，还必须阅读
[references/naming-and-style.md](references/naming-and-style.md)。

## 确定代码归属

按照“谁拥有这项行为”放置代码，不要按照“谁最先调用它”放置代码。

- React 应用组合放在 `frontend/src/app`。
- 某项功能独有的界面和状态放在 `frontend/src/features/<feature>`。
- 无状态、跨功能复用的界面组件放在 `frontend/src/shared/components`。
- 类型化 IPC 门面和真正跨功能的辅助能力放在 `frontend/src/lib`。
- Tauri 参数、响应适配放在 `backend/Demiurge-desktop/src/controller`。
- 桌面端用例编排放在 `backend/Demiurge-desktop/src/biz`。
- 不依赖 Tauri 的共享契约放在 `backend/Demiurge-common`。
- 不依赖 Tauri 的领域状态和规则放在 `backend/Demiurge-core`。
- 持久化和外部系统适配器放在 `backend/Demiurge-framework`。
- 运行时构建、恢复、窗口初始化和命令注册放在
  `backend/Demiurge-desktop/src/starter.rs` 或 `starter/`。

不要为了预留未来位置而创建空模块、空 Trait、空 DTO、BO、DO、Query、DAO、
Provider、MQ Consumer 或占位类。只有当行为存在真实变化，或者复杂度将扩散到多个调用方时，才建立新的抽象边界。

## 保持依赖方向

遵守以下调用链：

```text
frontend -> Tauri IPC -> controller -> biz -> core/framework/desktop modules
starter  -> 装配 controller + state + adapters
```

遵守以下后端依赖方向：

```text
common <- core <- framework
common/core/framework <- desktop
```

禁止：

- Controller 直接访问状态存储字段、文件系统 API、HTTP 客户端、SQL 或供应商 SDK。
- Core 依赖 Tauri、Framework 或 Desktop。
- Framework 依赖 Desktop。
- Starter 承担领域决策。
- 前端 Feature 绕过类型化门面直接调用 Tauri。
- 新代码使用通配符导入。修改已有通配符导入时，如果不会扩大任务范围，应改成显式导入。

设计“深模块”：用小而稳定的接口隐藏校验、事务、重试、持久化、供应商差异和错误转换。
如果新增层只做参数改名和原样转发，应删除该层。

## 让文件自身说明职责

模块注释用于说明职责和约束，不写物理路径，也不手工声明作者。

Rust 示例：

```rust
//! 会话 IPC 适配器。
//!
//! 将 Tauri 参数转换为会话用例输入，并把结果映射为 DTO。
//! 不直接访问持久化或外部系统适配器。
```

TypeScript 示例：

```ts
/**
 * 负责工作区浏览和 Git 交互界面。
 * 后端能力统一通过类型化 IPC 门面访问。
 */
```

如果文件名和接口已经清楚表达职责，可以省略文件头。禁止在每个源文件中写死本机路径或 `Author:`。

使用 Git 追溯作者，使用 `CODEOWNERS` 声明维护责任：

- 提交时保持正确的 `user.name` 和 `user.email`。
- 未经用户要求，不改写作者历史，不暂存、不提交、不推送。
- 使用 IDE Code Vision 或 Git blame 查看贡献者。
- 仓库启用 `CODEOWNERS` 后，新增责任区域时同步更新归属规则。

## 完成一个纵向闭环

1. 行为发生变化时，先新增或更新能够失败的测试。
2. 只修改构成完整功能闭环的最小模块集合。
3. 保持 Controller、Listener、Task 等入口轻量。
4. 当表示形式确实不同时，在明确边界完成 DTO、BO、DO 转换。
5. 有意移动源码时，同步更新源码契约测试。
6. 职责、契约、构建命令或路径发生变化时，同步更新架构或功能文档。
7. 避免无关的格式化、重命名、依赖升级和清理。

命名必须遵守
[references/naming-and-style.md](references/naming-and-style.md)。

## 按风险完成验证

移动或创建模块后，执行确定性的结构检查：

```text
python skills/demiurge-project-maintainer/scripts/audit_structure.py .
```

修改前端时执行：

```text
npm test
npm run build
```

修改后端时执行：

```text
cd backend
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace --all-targets
```

修改 Tauri 配置、运行时装配、路径或打包逻辑时执行：

```text
npm run tauri -- build --debug --no-bundle
```

同时执行 `git diff --check`。如果某条命令无法运行，必须报告准确原因，不能用更弱的检查替代后声称验证通过。

## 每次修改都要交接

给出最终答复前，阅读
[references/review-and-handoff.md](references/review-and-handoff.md)。

必须报告：

- 先说明最终结果。
- 代码归属和修改后的调用链。
- 按职责分组的修改文件。
- 保持不变的外部契约。
- 实际执行的测试与构建命令，以及数量或结果。
- 已知警告、风险、假设和未验证范围。
- 当前 Git 分支，以及是否已经提交、是否已经推送。

只要必需测试仍然失败或必需工作尚未完成，就不能声称“已经完成”。
