//! Demiurge 核心业务状态与纯规则。
//!
//! 禁止依赖 Tauri、文件系统、网络客户端或具体 Provider。

pub mod goal;
pub mod session;
