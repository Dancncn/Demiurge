use crate::{models, AppState};

pub(crate) fn catalog(state: &AppState) -> models::ModelCatalog {
    models::load(state)
}

pub(crate) async fn refresh_openrouter(state: &AppState) -> Result<models::ModelCatalog, String> {
    models::refresh_openrouter(state).await
}
