//! memory IPC adapter.

use crate::biz::memory::ContextPanelState;
use crate::*;
use tauri::State;

#[tauri::command]
pub(crate) fn memory_panel_state(state: State<'_, AppState>) -> agent::memory::MemoryPanelState {
    crate::biz::memory::memory_panel_state(state.inner())
}

#[tauri::command]
pub(crate) fn memory_add_entry(
    state: State<'_, AppState>,
    scope: String,
    kind: String,
    text: String,
) -> Result<agent::memory::MemoryPanelState, String> {
    crate::biz::memory::memory_add_entry(state.inner(), scope, kind, text)
}

#[tauri::command]
pub(crate) fn memory_update_entry(
    state: State<'_, AppState>,
    id: String,
    kind: String,
    text: String,
) -> Result<agent::memory::MemoryPanelState, String> {
    crate::biz::memory::memory_update_entry(state.inner(), id, kind, text)
}

#[tauri::command]
pub(crate) fn memory_delete_entry(
    state: State<'_, AppState>,
    id: String,
) -> Result<agent::memory::MemoryPanelState, String> {
    crate::biz::memory::memory_delete_entry(state.inner(), id)
}

#[tauri::command]
pub(crate) fn memory_dedupe_apply(
    state: State<'_, AppState>,
) -> Result<agent::memory::MemoryPanelState, String> {
    crate::biz::memory::memory_dedupe_apply(state.inner())
}

#[tauri::command]
pub(crate) fn memory_migrate_namespace(
    state: State<'_, AppState>,
    from_ns: String,
    to_ns: String,
) -> Result<agent::memory::MemoryPanelState, String> {
    crate::biz::memory::memory_migrate_namespace(state.inner(), from_ns, to_ns)
}

#[tauri::command]
pub(crate) fn context_panel_state(state: State<'_, AppState>) -> ContextPanelState {
    crate::biz::memory::context_panel_state(state.inner())
}

#[tauri::command]
pub(crate) fn skill_panel_state(
    state: State<'_, AppState>,
    query: Option<String>,
) -> agent::skills::SkillPanelState {
    crate::biz::memory::skill_panel_state(state.inner(), query)
}

#[tauri::command]
pub(crate) fn open_skills_dir(state: State<'_, AppState>) -> Result<(), String> {
    crate::biz::memory::open_skills_dir(state.inner())
}
