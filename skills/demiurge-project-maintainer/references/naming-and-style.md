# 命名与源码书写规范

## Rust 文件和模块

- 模块文件使用 `snake_case.rs`，例如 `session_engine.rs`、`workflow_entry.rs`。
- 一个功能包含多个内聚实现文件时使用目录模块；`mod.rs` 只负责导出和模块级协调。
- Struct、Enum、Trait 使用 `PascalCase`。
- 函数、变量和命令名使用 `snake_case`。
- 常量使用 `SCREAMING_SNAKE_CASE`。
- 优先使用显式导入，禁止新增 `use crate::*`。

### 职责后缀

只有后缀确实表达了不同表示或职责时才使用：

| 职责 | Rust 命名 | 示例 |
|---|---|---|
| IPC 请求 | `ActionReqDto` | `RenameSessionReqDto` |
| IPC 响应 | `ActionResDto` | `SessionListResDto` |
| 领域对象 | 确有需要时使用 `DomainBo` | `SessionBo` |
| 持久化对象 | 确有需要时使用 `DomainDo` | `SessionDo` |
| 查询条件 | `DomainQuery` | `MemoryQuery` |
| 用例模块或类型 | `DomainBiz` | `SessionBiz` |
| 持久化抽象 | `DomainRepository` | `SessionRepository` |
| 具体适配器 | 能表达实现技术的名词 | `FileSessionRepository` |
| 外部系统适配器 | `ProviderRemote` | `WebDavRemote` |
| 消息入口 | `EventMessageListener` | 仅在真实存在消息能力时使用 |
| 定时任务 | `PurposeTask` | 仅在真实存在调度能力时使用 |

Rust 不采用 Java 风格的 `IService` 或 `IDomainBiz` 前缀。Trait 应描述能力，具体实现应描述所采用的技术。

### 命令命名

- 完全保留现有 Tauri 命令名。
- 新命令使用稳定的“领域 + 动作”词汇，例如：
  `session_rename`、`memory_add_entry`、`webdav_list_backups`。
- 命令包装器必须足够轻量，使参数校验和委托关系一眼可见。
- 禁止通过 IPC 暴露持久化对象。

## TypeScript 文件和符号

- React 组件：`PascalCase.tsx`。
- Hook：`useThing.ts`。
- 工具和状态模型：`camelCase.ts`。
- 测试：`<subject>.test.ts` 或 `<subject>.test.tsx`。
- Feature 目录：小写领域名词。
- Props：导出时使用 `<ComponentName>Props`，仅文件内使用时命名为 `Props`。
- 事件处理函数：局部处理函数使用 `handle<Action>`，回调属性使用 `on<Action>`。
- Boolean：使用 `is`、`has`、`can`、`should` 或能表达领域状态的前缀。
- 大型匿名对象类型应改为具名领域类型。
- Tauri 必须通过 `frontend/src/lib/api.ts` 访问，Feature 组件不得直接调用 `invoke`。

## 文件头注释

只有当文件头能补充职责或关键约束时才编写。

推荐：

```rust
//! WebDAV 外部系统适配器。
//!
//! 隐藏认证、集合创建和 XML 响应解析。
```

```ts
/**
 * 将流式 Agent 事件归并为当前会话的展示时间线。
 * 忽略来自已被替换轮次的迟到事件。
 */
```

禁止：

```text
Path: D:\Desktop\Demiurge\...
Author: Alice
Created: 2026-07-26
```

物理路径会变化，Git 已经记录作者和日期。责任归属使用 `CODEOWNERS`，不要在源码中声明排他性作者身份。

## 注释和文档

- 解释原因、约束、错误行为、并发、兼容性和安全决策。
- 不要复述语法本身。
- 移动代码时同步修正注释。
- 公共契约和容易误解的限制写在接口附近。
- 长篇设计说明应链接到架构决策文档，不在多个文件中重复粘贴。

## 错误和日志

- 在模块接口返回可以指导排查的错误。
- 只在真正了解失败操作的那一层补充一次上下文。
- 禁止记录密钥、Token、密码、原始凭据或完整的敏感载荷。
- 除非日志表示了不同的运维事件，否则不要对同一个错误既记录日志又原样向上传递。

## 测试

- 测试名描述行为，例如 `rejects_path_traversal_backup_names`。
- 通过模块公开接口测试。
- 纯单元测试靠近实现；跨模块契约和架构约束使用集成测试。
- 有意移动文件导致源码契约测试输入变化时，同步更新对应测试。
