//! 桌面端用例编排。
//!
//! Biz 隐藏一次用户操作涉及的锁顺序、回滚、持久化和跨领域协作；
//! Controller 只负责 IPC 参数与响应适配。

pub(crate) mod agent;
pub(crate) mod agent_config;
pub(crate) mod companion;
pub(crate) mod goal;
pub(crate) mod mcp;
pub(crate) mod media;
pub(crate) mod memory;
pub(crate) mod pack;
pub(crate) mod permission;
pub(crate) mod remote;
pub(crate) mod session;
pub(crate) mod settings;
pub(crate) mod system;
pub(crate) mod window;
pub(crate) mod workflow_entry;
