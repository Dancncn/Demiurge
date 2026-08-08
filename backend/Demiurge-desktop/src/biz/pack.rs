//! pack IPC Adapter.

use crate::*;

pub(crate) fn list_packs(state: &AppState) -> Vec<pack::PackManifest> {
    let dir = state.packs_dir.lock().unwrap().clone();
    pack::list_packs(&dir)
}

pub(crate) fn import_pack_zip(
    state: &AppState,
    file_name: String,
    bytes: Vec<u8>,
) -> Result<pack::PackImportResult, String> {
    let dir = state.packs_dir.lock().unwrap().clone();
    let manifest = pack::import_zip(&dir, &file_name, bytes)?;
    let warnings = pack::credit_warnings(&manifest);
    Ok(pack::PackImportResult { manifest, warnings })
}

pub(crate) fn read_pack_manifest_json(state: &AppState, id: String) -> Result<String, String> {
    let dir = state.packs_dir.lock().unwrap().clone();
    pack::read_manifest_json(&dir, &id)
}

pub(crate) fn save_pack_manifest_json(
    state: &AppState,
    id: String,
    raw_json: String,
) -> Result<pack::PackManifest, String> {
    let dir = state.packs_dir.lock().unwrap().clone();
    pack::save_manifest_json(&dir, &id, &raw_json)
}

pub(crate) fn preview_pack_lorebook(
    state: &AppState,
    id: String,
    query: String,
) -> Result<String, String> {
    let packs_dir = state.packs_dir.lock().unwrap().clone();
    pack::resolve_pack_dir(&packs_dir, &id)?;
    let data_dir = state.data_dir.lock().unwrap().clone();
    let settings = state.settings.lock().unwrap().clone();
    let embed = crate::embed::provider_from_settings(&state.http, &settings);
    Ok(pack::lorebook_context(
        &packs_dir,
        &data_dir,
        &id,
        Some(&query),
        embed.as_deref(),
        settings.hybrid_weight,
    ))
}

pub(crate) fn import_pack_live2d_folder(
    state: &AppState,
    pack_id: String,
    src_dir: String,
) -> Result<pack::PackManifest, String> {
    let dir = state.packs_dir.lock().unwrap().clone();
    pack::import_live2d_folder(&dir, &pack_id, &src_dir)
}

pub(crate) fn resolve_pack_live2d_path(
    state: &AppState,
    pack_id: String,
) -> Result<String, String> {
    let dir = state.packs_dir.lock().unwrap().clone();
    pack::resolve_live2d_model_path(&dir, &pack_id)
}

pub(crate) fn pack_live2d_bundle(
    state: &AppState,
    pack_id: String,
) -> Result<pack::Live2DBundle, String> {
    let dir = state.packs_dir.lock().unwrap().clone();
    pack::live2d_bundle(&dir, &pack_id)
}

pub(crate) fn remove_pack_live2d(
    state: &AppState,
    pack_id: String,
) -> Result<pack::PackManifest, String> {
    let dir = state.packs_dir.lock().unwrap().clone();
    pack::remove_live2d(&dir, &pack_id)
}

pub(crate) fn open_pack_dir(state: &AppState, id: String) -> Result<(), String> {
    let packs_dir = state.packs_dir.lock().unwrap().clone();
    let dir = pack::resolve_pack_dir(&packs_dir, &id)?;
    tools::execute_open(&dir.to_string_lossy()).map(|_| ())
}

pub(crate) fn import_pack_lore_files(
    state: &AppState,
    id: String,
    files: Vec<pack::PackLoreFile>,
) -> Result<pack::PackManifest, String> {
    let dir = state.packs_dir.lock().unwrap().clone();
    let manifest = pack::import_pack_lore_files(&dir, &id, files)?;
    // 导入后立即重建 Lorebook 索引，让面板状态与召回立即可见新文件，不必等下次查询。
    let data_dir = state.data_dir.lock().unwrap().clone();
    let _ = pack::lorebook_rebuild_index(&dir, &data_dir, &id);
    Ok(manifest)
}

pub(crate) fn list_pack_files(
    state: &AppState,
    id: String,
    sub_dir: Option<String>,
) -> Result<Vec<pack::PackFileEntry>, String> {
    let dir = state.packs_dir.lock().unwrap().clone();
    pack::list_pack_files(&dir, &id, sub_dir.as_deref())
}

pub(crate) fn read_pack_file(
    state: &AppState,
    id: String,
    path: String,
) -> Result<pack::PackFileContent, String> {
    let dir = state.packs_dir.lock().unwrap().clone();
    pack::read_pack_file(&dir, &id, &path)
}

pub(crate) fn lorebook_index_status(
    state: &AppState,
    id: String,
) -> Result<pack::LoreIndexStatus, String> {
    let packs_dir = state.packs_dir.lock().unwrap().clone();
    let data_dir = state.data_dir.lock().unwrap().clone();
    pack::lorebook_index_status(&packs_dir, &data_dir, &id)
}

pub(crate) fn lorebook_recall_detail(
    state: &AppState,
    id: String,
    query: String,
    limit: Option<usize>,
) -> Result<pack::LoreRecallDetail, String> {
    let packs_dir = state.packs_dir.lock().unwrap().clone();
    let data_dir = state.data_dir.lock().unwrap().clone();
    let settings = state.settings.lock().unwrap().clone();
    let embed = crate::embed::provider_from_settings(&state.http, &settings);
    pack::lorebook_recall_detail(
        &packs_dir,
        &data_dir,
        &id,
        &query,
        limit.unwrap_or(50),
        embed.as_deref(),
        settings.hybrid_weight,
    )
}

pub(crate) fn lorebook_rebuild_index(
    state: &AppState,
    id: String,
) -> Result<pack::LoreIndexStatus, String> {
    let packs_dir = state.packs_dir.lock().unwrap().clone();
    let data_dir = state.data_dir.lock().unwrap().clone();
    pack::lorebook_rebuild_index(&packs_dir, &data_dir, &id)
}

pub(crate) fn embedding_probe(
    state: &AppState,
    settings: store::Settings,
) -> embed::EmbeddingProbeResult {
    embed::probe(&state.http, &settings)
}
