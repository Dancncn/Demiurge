//! mcp IPC adapter.

use crate::*;
use tauri::{AppHandle, State};

#[tauri::command]
pub(crate) async fn mcp_panel_state(
    state: State<'_, AppState>,
) -> Result<mcp::McpPanelState, String> {
    crate::biz::mcp::mcp_panel_state(state.inner()).await
}

#[tauri::command]
pub(crate) async fn mcp_refresh(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<mcp::McpPanelState, String> {
    crate::biz::mcp::mcp_refresh(app, state.inner()).await
}

#[tauri::command]
pub(crate) async fn mcp_set_server_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    enabled: bool,
) -> Result<mcp::McpPanelState, String> {
    crate::biz::mcp::mcp_set_server_enabled(app, state.inner(), name, enabled).await
}
