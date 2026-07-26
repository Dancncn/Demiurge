//! remote IPC adapter.

use crate::biz::remote::{WebDavBackupFile, WebDavConfig};
use crate::store::Settings;
use crate::*;
use tauri::State;

#[tauri::command]
pub(crate) async fn provider_check_connection(
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<connection_tests::ConnectionTestResult, String> {
    crate::biz::remote::provider_check_connection(state.inner(), settings).await
}

#[tauri::command]
pub(crate) async fn web_search_check_connection(
    state: State<'_, AppState>,
    settings: Settings,
    provider: Option<String>,
) -> Result<connection_tests::ConnectionTestResult, String> {
    crate::biz::remote::web_search_check_connection(state.inner(), settings, provider).await
}

#[tauri::command]
pub(crate) async fn webdav_check_connection(
    state: State<'_, AppState>,
    config: WebDavConfig,
) -> Result<String, String> {
    crate::biz::remote::webdav_check_connection(state.inner(), config).await
}

#[tauri::command]
pub(crate) async fn webdav_backup_now(
    state: State<'_, AppState>,
    config: WebDavConfig,
) -> Result<String, String> {
    crate::biz::remote::webdav_backup_now(state.inner(), config).await
}

#[tauri::command]
pub(crate) async fn webdav_list_backups(
    state: State<'_, AppState>,
    config: WebDavConfig,
) -> Result<Vec<WebDavBackupFile>, String> {
    crate::biz::remote::webdav_list_backups(state.inner(), config).await
}

#[tauri::command]
pub(crate) async fn webdav_delete_backup(
    state: State<'_, AppState>,
    config: WebDavConfig,
    file_name: String,
) -> Result<(), String> {
    crate::biz::remote::webdav_delete_backup(state.inner(), config, file_name).await
}
