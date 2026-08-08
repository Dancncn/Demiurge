//! system IPC Adapter.

use crate::*;

pub(crate) fn open_sandbox(state: &AppState) -> Result<(), String> {
    let dir = state.sandbox_dir.lock().unwrap().clone();
    tools::execute_open(&dir.to_string_lossy()).map(|_| ())
}
