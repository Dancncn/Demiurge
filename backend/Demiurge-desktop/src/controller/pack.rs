//! pack IPC adapter.

use crate::*;
use tauri::State;

#[tauri::command]
pub(crate) fn list_packs(state: State<'_, AppState>) -> Vec<pack::PackManifest> {
    crate::biz::pack::list_packs(state.inner())
}

#[tauri::command]
pub(crate) fn import_pack_zip(
    state: State<'_, AppState>,
    file_name: String,
    bytes: Vec<u8>,
) -> Result<pack::PackImportResult, String> {
    crate::biz::pack::import_pack_zip(state.inner(), file_name, bytes)
}

#[tauri::command]
pub(crate) fn read_pack_manifest_json(
    state: State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    crate::biz::pack::read_pack_manifest_json(state.inner(), id)
}

#[tauri::command]
pub(crate) fn save_pack_manifest_json(
    state: State<'_, AppState>,
    id: String,
    raw_json: String,
) -> Result<pack::PackManifest, String> {
    crate::biz::pack::save_pack_manifest_json(state.inner(), id, raw_json)
}

#[tauri::command]
pub(crate) fn preview_pack_lorebook(
    state: State<'_, AppState>,
    id: String,
    query: String,
) -> Result<String, String> {
    crate::biz::pack::preview_pack_lorebook(state.inner(), id, query)
}

#[tauri::command]
pub(crate) fn import_pack_live2d_folder(
    state: State<'_, AppState>,
    pack_id: String,
    src_dir: String,
) -> Result<pack::PackManifest, String> {
    crate::biz::pack::import_pack_live2d_folder(state.inner(), pack_id, src_dir)
}

#[tauri::command]
pub(crate) fn resolve_pack_live2d_path(
    state: State<'_, AppState>,
    pack_id: String,
) -> Result<String, String> {
    crate::biz::pack::resolve_pack_live2d_path(state.inner(), pack_id)
}

#[tauri::command]
pub(crate) fn pack_live2d_bundle(
    state: State<'_, AppState>,
    pack_id: String,
) -> Result<pack::Live2DBundle, String> {
    crate::biz::pack::pack_live2d_bundle(state.inner(), pack_id)
}

#[tauri::command]
pub(crate) fn remove_pack_live2d(
    state: State<'_, AppState>,
    pack_id: String,
) -> Result<pack::PackManifest, String> {
    crate::biz::pack::remove_pack_live2d(state.inner(), pack_id)
}

#[tauri::command]
pub(crate) fn open_pack_dir(state: State<'_, AppState>, id: String) -> Result<(), String> {
    crate::biz::pack::open_pack_dir(state.inner(), id)
}

#[tauri::command]
pub(crate) fn import_pack_lore_files(
    state: State<'_, AppState>,
    id: String,
    files: Vec<pack::PackLoreFile>,
) -> Result<pack::PackManifest, String> {
    crate::biz::pack::import_pack_lore_files(state.inner(), id, files)
}

#[tauri::command]
pub(crate) fn list_pack_files(
    state: State<'_, AppState>,
    id: String,
    sub_dir: Option<String>,
) -> Result<Vec<pack::PackFileEntry>, String> {
    crate::biz::pack::list_pack_files(state.inner(), id, sub_dir)
}

#[tauri::command]
pub(crate) fn read_pack_file(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> Result<pack::PackFileContent, String> {
    crate::biz::pack::read_pack_file(state.inner(), id, path)
}

#[tauri::command]
pub(crate) fn lorebook_index_status(
    state: State<'_, AppState>,
    id: String,
) -> Result<pack::LoreIndexStatus, String> {
    crate::biz::pack::lorebook_index_status(state.inner(), id)
}

#[tauri::command]
pub(crate) fn lorebook_recall_detail(
    state: State<'_, AppState>,
    id: String,
    query: String,
    limit: Option<usize>,
) -> Result<pack::LoreRecallDetail, String> {
    crate::biz::pack::lorebook_recall_detail(state.inner(), id, query, limit)
}

#[tauri::command]
pub(crate) fn lorebook_rebuild_index(
    state: State<'_, AppState>,
    id: String,
) -> Result<pack::LoreIndexStatus, String> {
    crate::biz::pack::lorebook_rebuild_index(state.inner(), id)
}

#[tauri::command]
pub(crate) fn embedding_probe(
    state: State<'_, AppState>,
    settings: store::Settings,
) -> embed::EmbeddingProbeResult {
    crate::biz::pack::embedding_probe(state.inner(), settings)
}
