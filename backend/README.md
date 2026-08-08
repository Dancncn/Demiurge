# Demiurge Backend

Rust/Tauri 模块化单体工作区。四个目录都是可编译的 Cargo crate；重构保持
command/event 名、参数、配置键、持久化 JSON 和现有技术栈不变。

职责域：

- `Demiurge-common`：共享 Conversation、Settings、MCP 契约。
- `Demiurge-core`：不依赖 Tauri 的 Session 与 Goal 领域状态。
- `Demiurge-framework`：原子文件、Session/Settings 持久化及 WebDAV Adapter。
- `Demiurge-desktop`：Tauri Controller、Biz、运行时装配，以及仍只服务桌面运行时的
  Agent、Tool、Provider 和系统能力。

依赖方向应保持：

```text
desktop -> core
desktop -> framework
framework -> core
common <- core/framework/desktop
```

`core` 禁止反向依赖 `desktop` 或具体 `framework` Adapter。
这一方向由 `Demiurge-desktop/tests/architecture_boundaries.rs` 自动校验。
