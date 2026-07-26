//! External-system use cases.

use demiurge_framework::remote::webdav::WebDavRemote;
pub(crate) use demiurge_framework::remote::webdav::{WebDavBackupFile, WebDavConfig};
use serde_json::json;

use crate::store::Settings;
use crate::{connection_tests, store, AppState};

pub(crate) async fn provider_check_connection(
    state: &AppState,
    settings: Settings,
) -> Result<connection_tests::ConnectionTestResult, String> {
    connection_tests::test_provider(&state.http, settings).await
}

pub(crate) async fn web_search_check_connection(
    state: &AppState,
    settings: Settings,
    provider: Option<String>,
) -> Result<connection_tests::ConnectionTestResult, String> {
    connection_tests::test_web_search(&state.http, settings, provider).await
}

pub(crate) async fn webdav_check_connection(
    state: &AppState,
    config: WebDavConfig,
) -> Result<String, String> {
    WebDavRemote::new(&state.http)
        .check_connection(&config)
        .await?;
    Ok("Connected".to_string())
}

pub(crate) async fn webdav_backup_now(
    state: &AppState,
    config: WebDavConfig,
) -> Result<String, String> {
    let settings = store::redacted_settings(&state.settings.lock().unwrap().clone());
    let sessions = state.sessions.lock().unwrap().clone();
    let sessions = store::session_store_value(&sessions)?;
    let payload = json!({
        "app": "Demiurge",
        "version": env!("CARGO_PKG_VERSION"),
        "exported_at": store::now_millis(),
        "settings": settings,
        "sessions": sessions,
    });
    let body =
        serde_json::to_vec_pretty(&payload).map_err(|error| format!("序列化备份失败：{error}"))?;
    let file_name = format!("demiurge-backup-{}.json", store::now_millis());
    WebDavRemote::new(&state.http)
        .upload_json(&config, &file_name, body)
        .await?;
    Ok(file_name)
}

pub(crate) async fn webdav_list_backups(
    state: &AppState,
    config: WebDavConfig,
) -> Result<Vec<WebDavBackupFile>, String> {
    WebDavRemote::new(&state.http).list_backups(&config).await
}

pub(crate) async fn webdav_delete_backup(
    state: &AppState,
    config: WebDavConfig,
    file_name: String,
) -> Result<(), String> {
    WebDavRemote::new(&state.http)
        .delete_backup(&config, &file_name)
        .await
}
