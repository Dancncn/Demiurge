//! Agent 模块：会话状态、上下文管理、人格拼装，以及核心循环。
pub mod context;
pub mod conversation;
pub mod persona;

mod runner;
pub use runner::run_turn;
