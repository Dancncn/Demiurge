//! settings IPC Adapter.

use crate::store::Settings;
use crate::*;
use tauri::{AppHandle, Manager};

pub(crate) fn get_settings(state: &AppState) -> Settings {
    state.settings.lock().unwrap().clone()
}

pub(crate) fn save_settings(
    app: AppHandle,
    state: &AppState,
    settings: Settings,
) -> Result<(), String> {
    let packs_dir = state.packs_dir.lock().unwrap().clone();
    pack::resolve_pack_dir(&packs_dir, &settings.current_pack)?;
    if !settings.current_pet.is_empty() {
        let pets_dir = state.pets_dir.lock().unwrap().clone();
        crate::pet::resolve_dir(&pets_dir, &settings.current_pet)?;
    }
    let current_launch_on_startup = state.settings.lock().unwrap().launch_on_startup;
    if settings.launch_on_startup != current_launch_on_startup {
        startup::apply_launch_on_startup(settings.launch_on_startup)?;
    }
    credentials::save_api_key(&settings.api_key)?;
    credentials::save_web_search_api_keys(&settings)?;
    credentials::save_webdav_password(&settings.webdav_password)?;
    credentials::save_media_api_key(&settings.media_api_key)?;
    credentials::save_embedding_api_key(&settings.embedding_api_key)?;
    credentials::save_mcp_env_secrets(&settings)?;
    *state.settings.lock().unwrap() = settings.clone();
    let dir = state.data_dir.lock().unwrap().clone();
    store::save_settings(&dir, &settings)?;
    crate::biz::window::sync_desktop_companion_window(&app, &settings)?;
    crate::biz::window::emit_settings_updated(&app, &settings);
    let mcp_app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = mcp_app.state::<AppState>();
        crate::mcp::ensure_initialized(state.inner()).await;
    });
    Ok(())
}
