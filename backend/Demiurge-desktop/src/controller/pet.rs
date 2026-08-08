//! 桌宠 IPC 适配器。

use crate::pet::{InstalledPet, ResolvedPet};
use crate::AppState;
use tauri::{AppHandle, State};

#[tauri::command]
pub(crate) fn pet_list(state: State<'_, AppState>) -> Vec<InstalledPet> {
    crate::biz::pet::list(state.inner())
}

#[tauri::command]
pub(crate) fn pet_import(
    app: AppHandle,
    state: State<'_, AppState>,
    file_name: String,
    bytes: Vec<u8>,
) -> Result<InstalledPet, String> {
    crate::biz::pet::import(&app, state.inner(), file_name, bytes)
}

#[tauri::command]
pub(crate) fn pet_remove(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    crate::biz::pet::remove(&app, state.inner(), id)
}

#[tauri::command]
pub(crate) fn pet_resolve(state: State<'_, AppState>, id: String) -> Result<ResolvedPet, String> {
    crate::biz::pet::resolve(state.inner(), id)
}
