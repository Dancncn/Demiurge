# Cross-layer Tests

该目录为未来跨前端、Tauri IPC 与后端的端到端测试预留。

现有 Node 前端与源码契约测试位于 `frontend/tests`。Rust 单元测试靠近对应
crate 的实现，跨模块架构边界测试位于
`backend/Demiurge-desktop/tests/architecture_boundaries.rs`。

在存在真实端到端测试前，不创建空测试类。
