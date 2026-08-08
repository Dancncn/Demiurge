//! system IPC adapter.

use crate::*;
use tauri::State;

#[tauri::command]
pub(crate) fn open_sandbox(state: State<'_, AppState>) -> Result<(), String> {
    crate::biz::system::open_sandbox(state.inner())
}
