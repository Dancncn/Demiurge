//! Tauri IPC 入口。
//!
//! Controller 只保留参数接收、响应 DTO 转换与 Biz 调用，不直接实现业务规则。

pub(crate) mod agent;
pub(crate) mod agent_config;
pub(crate) mod companion;
pub(crate) mod goal;
pub(crate) mod integrations;
pub(crate) mod mcp;
pub(crate) mod media;
pub(crate) mod memory;
pub(crate) mod models;
pub(crate) mod pack;
pub(crate) mod permission;
pub(crate) mod pet;
pub(crate) mod remote;
pub(crate) mod session;
pub(crate) mod settings;
pub(crate) mod system;
pub(crate) mod voice;
pub(crate) mod window;
pub(crate) mod workflow;
pub(crate) mod workflow_entry;
pub(crate) mod workspace;

#[cfg(test)]
mod architecture_tests {
    #[test]
    fn session_controller_only_adapts_ipc_to_biz() {
        let source = include_str!("session.rs");

        assert!(source.contains("SessionBiz::"));
        assert!(!source.contains("state.sessions"));
        assert!(!source.contains("state.sandbox_dir"));
        assert!(!source.contains("sync_active_session_workspace"));
        assert!(!source.contains("persist_sessions"));
    }
}
