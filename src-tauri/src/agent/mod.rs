//! Agent 模块：会话状态、上下文管理、人格拼装，以及核心循环。
pub mod budget;
pub mod context;
pub mod conversation;
pub mod dream;
pub mod memory;
pub mod persona;
pub mod prompt;
pub mod subagent;
pub mod summary;
pub mod ultracode;

mod runner;
pub use runner::{run_turn, run_turn_with_options, TurnOptions};
