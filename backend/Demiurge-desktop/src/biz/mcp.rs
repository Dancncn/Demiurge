//! mcp IPC Adapter.

use crate::*;
use tauri::AppHandle;
use tauri::Emitter;

pub(crate) async fn mcp_panel_state(state: &AppState) -> Result<mcp::McpPanelState, String> {
    mcp::ensure_initialized(state).await;
    Ok(mcp::panel_state(state))
}

pub(crate) async fn mcp_refresh(
    app: AppHandle,
    state: &AppState,
) -> Result<mcp::McpPanelState, String> {
    mcp::refresh_all(state).await;
    let panel = mcp::panel_state(state);
    let _ = app.emit("mcp-updated", panel.clone());
    Ok(panel)
}

pub(crate) async fn mcp_set_server_enabled(
    app: AppHandle,
    state: &AppState,
    name: String,
    enabled: bool,
) -> Result<mcp::McpPanelState, String> {
    {
        let mut settings = state.settings.lock().unwrap();
        let server = settings
            .mcp_servers
            .iter_mut()
            .find(|server| server.name == name)
            .ok_or_else(|| format!("MCP server `{name}` 不存在。"))?;
        server.enabled = enabled;
        let dir = state.data_dir.lock().unwrap().clone();
        store::save_settings(&dir, &settings)?;
    }
    mcp::disconnect_server(state, &name).await;
    mcp::ensure_initialized(state).await;
    let panel = mcp::panel_state(state);
    let _ = app.emit("mcp-updated", panel.clone());
    Ok(panel)
}
