//! 外部资源 IPC Controller：只做参数适配和 Biz 调用。

use serde::Serialize;
use tauri::State;

use crate::biz::integrations::{ImportedSessionBo, IntegrationsBiz};
use crate::integrations::{self, MarketSearchResult, MarketSkill, SkillCandidate};
use crate::{usage, AppState};

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ImportedSessionResDto {
    id: String,
    title: String,
    provider: String,
    source_path: String,
    message_count: usize,
}

impl From<ImportedSessionBo> for ImportedSessionResDto {
    fn from(value: ImportedSessionBo) -> Self {
        Self {
            id: value.id,
            title: value.title,
            provider: value.provider,
            source_path: value.source_path,
            message_count: value.message_count,
        }
    }
}

#[tauri::command]
pub(crate) fn integration_scan(state: State<'_, AppState>) -> integrations::IntegrationSnapshot {
    IntegrationsBiz::scan(state.inner())
}

#[tauri::command]
pub(crate) fn integration_import_skill(
    state: State<'_, AppState>,
    source_path: String,
    source: Option<String>,
) -> Result<SkillCandidate, String> {
    IntegrationsBiz::import_skill(state.inner(), source_path, source)
}

#[tauri::command]
pub(crate) fn integration_set_skill_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    IntegrationsBiz::set_skill_enabled(state.inner(), id, enabled)
}

#[tauri::command]
pub(crate) fn integration_remove_skill(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    IntegrationsBiz::remove_skill(state.inner(), id)
}

#[tauri::command]
pub(crate) fn integration_session_messages(
    provider: String,
    source_path: String,
) -> Result<Vec<integrations::ExternalSessionMessage>, String> {
    IntegrationsBiz::session_messages(provider, source_path)
}

#[tauri::command]
pub(crate) fn integration_import_session(
    state: State<'_, AppState>,
    provider: String,
    source_path: String,
) -> Result<ImportedSessionResDto, String> {
    IntegrationsBiz::import_session(state.inner(), provider, source_path).map(Into::into)
}

#[tauri::command]
pub(crate) fn integration_import_config(
    state: State<'_, AppState>,
    provider: String,
    kind: String,
) -> Result<integrations::ImportedConfig, String> {
    IntegrationsBiz::import_config(state.inner(), provider, kind)
}

#[tauri::command]
pub(crate) async fn integration_market_search(
    state: State<'_, AppState>,
    query: String,
    limit: usize,
) -> Result<MarketSearchResult, String> {
    IntegrationsBiz::search_market(state.inner(), query, limit).await
}

#[tauri::command]
pub(crate) async fn integration_market_install(
    state: State<'_, AppState>,
    skill: MarketSkill,
) -> Result<SkillCandidate, String> {
    IntegrationsBiz::install_market_skill(state.inner(), skill).await
}

#[tauri::command]
pub(crate) fn usage_summary(
    state: State<'_, AppState>,
    start_at: Option<u64>,
    end_at: Option<u64>,
) -> usage::UsageSummary {
    usage::summary(state.inner(), start_at, end_at)
}
