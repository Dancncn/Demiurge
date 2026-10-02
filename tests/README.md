# Cross-layer Tests

Node 前端行为与源码契约测试位于 `frontend/tests`。Rust 单元测试靠近对应
crate 的实现，跨模块架构边界测试位于
`backend/Demiurge-desktop/tests/architecture_boundaries.rs`。

真实浏览器烟测位于 `frontend/tests/browser`，启动实际 App、Composer、设置和
语音队列，仅用 Tauri mocks 替换原生 IPC、事件与窗口适配器。覆盖 Goal 续跑、
错误竞态、历史恢复、停止播放、会话与确认隔离、权限草稿和刷新失败，以及真实图片附件的多模态发送。
运行方式见 [浏览器烟测说明](../frontend/tests/browser/README.md)。

这套烟测使用受控后端响应，不是打包 Tauri 应用与真实 Rust/LLM 的端到端测试。
当前没有这类完整端到端测试。
