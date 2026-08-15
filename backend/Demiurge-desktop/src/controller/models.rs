use tauri::State;

use crate::biz::models as models_biz;
use crate::models::ModelCatalog;
use crate::AppState;

#[tauri::command]
pub(crate) fn model_catalog(state: State<'_, AppState>) -> ModelCatalog {
    models_biz::catalog(state.inner())
}

#[tauri::command]
pub(crate) async fn model_catalog_refresh(
    state: State<'_, AppState>,
) -> Result<ModelCatalog, String> {
    models_biz::refresh_openrouter(state.inner()).await
}
