//! Demiurge 引擎 —— Tauri v2 入口：全局状态、命令、构建器。
mod agent;
mod biz;
mod companion;
mod connection_tests;
mod controller;
mod credentials;
mod embed;
mod integrations;
mod llm;
pub mod mcp;
mod media;
mod minecraft;
mod model_routing;
mod models;
mod ocr;
mod pack;
mod permission;
mod pet;
mod pomodoro;
mod starter;
mod startup;
mod store;
mod tools;
mod usage;
mod verification;
mod voice;
mod workspace;

pub(crate) use starter::state::{AppState, PlanState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    starter::run();
}
