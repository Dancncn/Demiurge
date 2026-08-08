//! 桌宠目录用例编排。

use crate::pet::{self, InstalledPet, ResolvedPet};
use crate::AppState;
use tauri::{AppHandle, Emitter};

pub(crate) fn list(state: &AppState) -> Vec<InstalledPet> {
    let dir = state.pets_dir.lock().unwrap().clone();
    pet::list(&dir)
}

pub(crate) fn import(
    app: &AppHandle,
    state: &AppState,
    file_name: String,
    bytes: Vec<u8>,
) -> Result<InstalledPet, String> {
    let dir = state.pets_dir.lock().unwrap().clone();
    let installed = pet::import(&dir, &file_name, bytes)?;
    let _ = app.emit("pet-catalog-updated", pet::list(&dir));
    Ok(installed)
}

pub(crate) fn remove(app: &AppHandle, state: &AppState, id: String) -> Result<(), String> {
    let dir = state.pets_dir.lock().unwrap().clone();
    pet::remove(&dir, &id)?;
    let mut settings = state.settings.lock().unwrap().clone();
    if settings.current_pet == id {
        settings.current_pet.clear();
        crate::biz::settings::save_settings(app.clone(), state, settings)?;
    }
    let _ = app.emit("pet-catalog-updated", pet::list(&dir));
    Ok(())
}

pub(crate) fn resolve(state: &AppState, id: String) -> Result<ResolvedPet, String> {
    let dir = state.pets_dir.lock().unwrap().clone();
    pet::resolve(&dir, &id)
}
