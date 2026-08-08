//! companion IPC Adapter.

use crate::*;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tauri::AppHandle;

pub(crate) async fn companion_panel_state(
    state: &AppState,
) -> Result<companion::CompanionPanelState, String> {
    Ok(companion::panel_state(state).await)
}

pub(crate) async fn companion_clear_weather_cache(
    state: &AppState,
) -> Result<companion::CompanionPanelState, String> {
    companion::clear_weather_cache();
    Ok(companion::panel_state(state).await)
}

pub(crate) fn pomodoro_state(state: &AppState) -> pomodoro::PomodoroPanelState {
    pomodoro::panel_state(state)
}

pub(crate) fn pomodoro_start(
    app: AppHandle,
    state: &AppState,
    request: pomodoro::PomodoroStartRequest,
) -> Result<pomodoro::PomodoroPanelState, String> {
    pomodoro::start(app, state, request)
}

pub(crate) fn pomodoro_pause(
    app: AppHandle,
    state: &AppState,
) -> Result<pomodoro::PomodoroPanelState, String> {
    pomodoro::pause(app, state)
}

pub(crate) fn pomodoro_resume(
    app: AppHandle,
    state: &AppState,
) -> Result<pomodoro::PomodoroPanelState, String> {
    pomodoro::resume(app, state)
}

pub(crate) fn pomodoro_skip(
    app: AppHandle,
    state: &AppState,
    request: Option<pomodoro::PomodoroSkipRequest>,
) -> Result<pomodoro::PomodoroPanelState, String> {
    pomodoro::skip(app, state, request)
}

pub(crate) fn companion_memory_suggestions(
    state: &AppState,
) -> Vec<companion::CompanionMemorySuggestion> {
    let settings = state.settings.lock().unwrap().clone();
    companion::memory_suggestions(&settings)
}

pub(crate) fn companion_memory_queue_state(
    state: &AppState,
) -> companion::CompanionMemoryQueueState {
    companion_queue_state(state)
}

pub(crate) fn companion_enqueue_memory_suggestion(
    state: &AppState,
    id: String,
) -> Result<companion::CompanionMemoryQueueState, String> {
    let settings = state.settings.lock().unwrap().clone();
    let suggestion = companion::memory_suggestion_by_id(&settings, &id)
        .ok_or_else(|| format!("Unknown companion memory suggestion: {id}"))?;
    let data_dir = state.data_dir.lock().unwrap().clone();
    let session_id = state.sessions.lock().unwrap().active.clone();
    companion::enqueue_memory_suggestion(&data_dir, &session_id, suggestion)
        .map(|_| companion_queue_state(state))
}

pub(crate) fn companion_save_memory_queue_item(
    state: &AppState,
    id: String,
    resolution: Option<String>,
) -> Result<companion::CompanionMemoryQueueState, String> {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let item = companion::pending_memory_queue_item(&data_dir, &id)
        .ok_or_else(|| format!("Unknown pending companion memory queue item: {id}"))?;
    let (data, sandbox, packs, pack_id, session_id) = crate::biz::memory::memory_context(state);
    let panel = agent::memory::panel_state(&data, &sandbox, &packs, &pack_id, &session_id);
    let duplicate = find_similar_memory_entry(&panel, &item);
    let resolution = resolution.unwrap_or_default();
    let (saved_id, undo_record) = match (duplicate, resolution.as_str()) {
        (Some(existing), "merge") => {
            let merged = merge_memory_text(&existing.text, &item.text);
            agent::memory::update_entry(
                &data,
                &sandbox,
                &packs,
                &pack_id,
                &session_id,
                &existing.id,
                &item.kind,
                &merged,
            )?;
            let memory_id = existing.id.clone();
            (
                Some(memory_id.clone()),
                CompanionMemoryUndoRecord {
                    queue_item_id: id.clone(),
                    memory_id,
                    action: "updated".to_string(),
                    previous_kind: Some(existing.kind),
                    previous_text: Some(existing.text),
                    applied_kind: item.kind.clone(),
                    applied_text: merged,
                },
            )
        }
        (Some(existing), "replace") => {
            agent::memory::update_entry(
                &data,
                &sandbox,
                &packs,
                &pack_id,
                &session_id,
                &existing.id,
                &item.kind,
                &item.text,
            )?;
            let memory_id = existing.id.clone();
            (
                Some(memory_id.clone()),
                CompanionMemoryUndoRecord {
                    queue_item_id: id.clone(),
                    memory_id,
                    action: "updated".to_string(),
                    previous_kind: Some(existing.kind),
                    previous_text: Some(existing.text),
                    applied_kind: item.kind.clone(),
                    applied_text: item.text.clone(),
                },
            )
        }
        (Some(_), "keep_new") | (None, _) => {
            let panel = agent::memory::add_entry(
                &data,
                &sandbox,
                &packs,
                &pack_id,
                &session_id,
                &item.scope,
                &item.kind,
                &item.text,
            )?;
            let memory_id = find_saved_memory_id(&panel, &item)
                .ok_or_else(|| "Saved memory could not be located for undo.".to_string())?;
            (
                Some(memory_id.clone()),
                CompanionMemoryUndoRecord {
                    queue_item_id: id.clone(),
                    memory_id,
                    action: "created".to_string(),
                    previous_kind: None,
                    previous_text: None,
                    applied_kind: item.kind.clone(),
                    applied_text: item.text.clone(),
                },
            )
        }
        (Some(_), _) => {
            return Err("Similar memory exists; choose merge, replace, or keep_new.".to_string())
        }
    };
    if let Err(error) = store_companion_memory_undo_record(&data_dir, undo_record.clone()) {
        let rollback = rollback_companion_memory_change(
            &data,
            &sandbox,
            &packs,
            &pack_id,
            &session_id,
            &undo_record,
        );
        return Err(match rollback {
            Ok(()) => error,
            Err(rollback_error) => format!("{error}; rollback also failed: {rollback_error}"),
        });
    }
    if let Err(error) = companion::mark_memory_queue_item(&data_dir, &id, "saved", saved_id) {
        let rollback = rollback_companion_memory_change(
            &data,
            &sandbox,
            &packs,
            &pack_id,
            &session_id,
            &undo_record,
        );
        let _ = remove_companion_memory_undo_record(&data_dir, &id);
        return Err(match rollback {
            Ok(()) => error,
            Err(rollback_error) => format!("{error}; rollback also failed: {rollback_error}"),
        });
    }
    Ok(companion_queue_state(state))
}

pub(crate) fn companion_ignore_memory_queue_item(
    state: &AppState,
    id: String,
) -> Result<companion::CompanionMemoryQueueState, String> {
    let data_dir = state.data_dir.lock().unwrap().clone();
    companion::mark_memory_queue_item(&data_dir, &id, "ignored", None)?;
    Ok(companion_queue_state(state))
}

pub(crate) fn companion_save_all_memory_queue_items(
    state: &AppState,
) -> Result<companion::CompanionMemoryQueueState, String> {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let pending = companion::memory_queue_state(&data_dir)
        .items
        .into_iter()
        .filter(|item| item.status == "pending")
        .collect::<Vec<_>>();
    let (data, sandbox, packs, pack_id, session_id) = crate::biz::memory::memory_context(state);
    for item in pending {
        let panel = agent::memory::panel_state(&data, &sandbox, &packs, &pack_id, &session_id);
        let duplicate = find_similar_memory_entry(&panel, &item);
        let (saved_id, undo_record) = if let Some(existing) = duplicate {
            let merged = merge_memory_text(&existing.text, &item.text);
            agent::memory::update_entry(
                &data,
                &sandbox,
                &packs,
                &pack_id,
                &session_id,
                &existing.id,
                &item.kind,
                &merged,
            )?;
            let memory_id = existing.id.clone();
            (
                Some(memory_id.clone()),
                CompanionMemoryUndoRecord {
                    queue_item_id: item.id.clone(),
                    memory_id,
                    action: "updated".to_string(),
                    previous_kind: Some(existing.kind),
                    previous_text: Some(existing.text),
                    applied_kind: item.kind.clone(),
                    applied_text: merged,
                },
            )
        } else {
            let panel = agent::memory::add_entry(
                &data,
                &sandbox,
                &packs,
                &pack_id,
                &session_id,
                &item.scope,
                &item.kind,
                &item.text,
            )?;
            let memory_id = find_saved_memory_id(&panel, &item)
                .ok_or_else(|| "Saved memory could not be located for undo.".to_string())?;
            (
                Some(memory_id.clone()),
                CompanionMemoryUndoRecord {
                    queue_item_id: item.id.clone(),
                    memory_id,
                    action: "created".to_string(),
                    previous_kind: None,
                    previous_text: None,
                    applied_kind: item.kind.clone(),
                    applied_text: item.text.clone(),
                },
            )
        };
        if let Err(error) = store_companion_memory_undo_record(&data_dir, undo_record.clone()) {
            let rollback = rollback_companion_memory_change(
                &data,
                &sandbox,
                &packs,
                &pack_id,
                &session_id,
                &undo_record,
            );
            return Err(match rollback {
                Ok(()) => error,
                Err(rollback_error) => format!("{error}; rollback also failed: {rollback_error}"),
            });
        }
        if let Err(error) =
            companion::mark_memory_queue_item(&data_dir, &item.id, "saved", saved_id)
        {
            let rollback = rollback_companion_memory_change(
                &data,
                &sandbox,
                &packs,
                &pack_id,
                &session_id,
                &undo_record,
            );
            let _ = remove_companion_memory_undo_record(&data_dir, &item.id);
            return Err(match rollback {
                Ok(()) => error,
                Err(rollback_error) => format!("{error}; rollback also failed: {rollback_error}"),
            });
        }
    }
    Ok(companion_queue_state(state))
}

pub(crate) fn companion_ignore_all_memory_queue_items(
    state: &AppState,
) -> Result<companion::CompanionMemoryQueueState, String> {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let pending_ids = companion::memory_queue_state(&data_dir)
        .items
        .into_iter()
        .filter(|item| item.status == "pending")
        .map(|item| item.id)
        .collect::<Vec<_>>();
    for id in pending_ids {
        companion::mark_memory_queue_item(&data_dir, &id, "ignored", None)?;
    }
    Ok(companion_queue_state(state))
}

pub(crate) fn companion_undo_memory_queue_item(
    state: &AppState,
    id: String,
) -> Result<companion::CompanionMemoryQueueState, String> {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let item = companion::memory_queue_state(&data_dir)
        .items
        .into_iter()
        .find(|item| item.id == id && item.status == "saved")
        .ok_or_else(|| format!("Unknown saved companion memory queue item: {id}"))?;
    let memory_id = item
        .saved_memory_id
        .clone()
        .ok_or_else(|| "Saved memory id is not available for undo.".to_string())?;
    let journal = load_companion_memory_undo_journal(&data_dir)?;
    let undo_record = journal.records.get(&id).cloned().ok_or_else(|| {
        "This saved item predates safe undo metadata; refusing to delete existing memory."
            .to_string()
    })?;
    if undo_record.memory_id != memory_id {
        return Err("Saved memory id does not match its undo record.".to_string());
    }
    let (data, sandbox, packs, pack_id, session_id) = crate::biz::memory::memory_context(state);
    apply_companion_memory_undo(&data, &sandbox, &packs, &pack_id, &session_id, &undo_record)?;
    companion::mark_memory_queue_item(&data_dir, &id, "pending", None)?;
    if let Err(error) = remove_companion_memory_undo_record(&data_dir, &id) {
        eprintln!("Companion memory was undone but its journal cleanup failed: {error}");
    }
    Ok(companion_queue_state(state))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CompanionMemoryUndoRecord {
    queue_item_id: String,
    memory_id: String,
    action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    previous_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    previous_text: Option<String>,
    applied_kind: String,
    applied_text: String,
}

#[derive(Default, Serialize, Deserialize)]
struct CompanionMemoryUndoJournal {
    #[serde(default)]
    records: HashMap<String, CompanionMemoryUndoRecord>,
}

fn companion_memory_undo_path(data_dir: &Path) -> PathBuf {
    data_dir.join("companion-memory-undo.json")
}

fn load_companion_memory_undo_journal(
    data_dir: &Path,
) -> Result<CompanionMemoryUndoJournal, String> {
    let path = companion_memory_undo_path(data_dir);
    let primary: Result<CompanionMemoryUndoJournal, String> = fs::read_to_string(&path)
        .map_err(|error| error.to_string())
        .and_then(|raw| serde_json::from_str(&raw).map_err(|error| error.to_string()));
    if let Ok(journal) = primary {
        return Ok(journal);
    }

    let backup = store::backup_path(&path);
    if let Ok(raw) = fs::read_to_string(&backup) {
        if let Ok(journal) = serde_json::from_str::<CompanionMemoryUndoJournal>(&raw) {
            if let Err(error) = store::atomic_write_text(&path, &raw, false) {
                eprintln!(
                    "Recovered companion memory undo journal from {} but failed to repair {}: {error}",
                    backup.display(),
                    path.display()
                );
            }
            return Ok(journal);
        }
    }

    if !path.exists() && !backup.exists() {
        Ok(CompanionMemoryUndoJournal::default())
    } else {
        Err(format!(
            "Companion memory undo journal {} and its backup are unreadable or corrupt.",
            path.display()
        ))
    }
}

fn save_companion_memory_undo_journal(
    data_dir: &Path,
    journal: &CompanionMemoryUndoJournal,
) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(journal)
        .map_err(|error| format!("Failed to serialize companion memory undo journal: {error}"))?;
    store::atomic_write_text(&companion_memory_undo_path(data_dir), &raw, true)
}

fn store_companion_memory_undo_record(
    data_dir: &Path,
    record: CompanionMemoryUndoRecord,
) -> Result<(), String> {
    let mut journal = load_companion_memory_undo_journal(data_dir)?;
    journal.records.insert(record.queue_item_id.clone(), record);
    save_companion_memory_undo_journal(data_dir, &journal)
}

fn remove_companion_memory_undo_record(data_dir: &Path, queue_item_id: &str) -> Result<(), String> {
    let mut journal = load_companion_memory_undo_journal(data_dir)?;
    journal.records.remove(queue_item_id);
    save_companion_memory_undo_journal(data_dir, &journal)
}

fn rollback_companion_memory_change(
    data: &Path,
    sandbox: &Path,
    packs: &Path,
    pack_id: &str,
    session_id: &str,
    record: &CompanionMemoryUndoRecord,
) -> Result<(), String> {
    match record.action.as_str() {
        "created" => {
            let panel = agent::memory::panel_state(data, sandbox, packs, pack_id, session_id);
            if panel
                .entries
                .iter()
                .any(|entry| entry.id == record.memory_id)
            {
                agent::memory::delete_entry(
                    data,
                    sandbox,
                    packs,
                    pack_id,
                    session_id,
                    &record.memory_id,
                )?;
            }
            Ok(())
        }
        "updated" => agent::memory::update_entry(
            data,
            sandbox,
            packs,
            pack_id,
            session_id,
            &record.memory_id,
            record
                .previous_kind
                .as_deref()
                .ok_or_else(|| "Undo record is missing previous memory kind.".to_string())?,
            record
                .previous_text
                .as_deref()
                .ok_or_else(|| "Undo record is missing previous memory text.".to_string())?,
        )
        .map(|_| ()),
        action => Err(format!("Unknown companion memory undo action: {action}")),
    }
}

fn apply_companion_memory_undo(
    data: &Path,
    sandbox: &Path,
    packs: &Path,
    pack_id: &str,
    session_id: &str,
    record: &CompanionMemoryUndoRecord,
) -> Result<(), String> {
    let panel = agent::memory::panel_state(data, sandbox, packs, pack_id, session_id);
    let current = panel
        .entries
        .iter()
        .find(|entry| entry.id == record.memory_id);
    match record.action.as_str() {
        "created" => {
            let Some(current) = current else {
                return Ok(());
            };
            if current.kind != record.applied_kind || current.text != record.applied_text {
                return Err(
                    "The saved memory changed after creation; undo will not delete it.".to_string(),
                );
            }
            agent::memory::delete_entry(
                data,
                sandbox,
                packs,
                pack_id,
                session_id,
                &record.memory_id,
            )?;
            Ok(())
        }
        "updated" => {
            let previous_kind = record
                .previous_kind
                .as_deref()
                .ok_or_else(|| "Undo record is missing previous memory kind.".to_string())?;
            let previous_text = record
                .previous_text
                .as_deref()
                .ok_or_else(|| "Undo record is missing previous memory text.".to_string())?;
            let current =
                current.ok_or_else(|| "The updated memory no longer exists.".to_string())?;
            if current.kind == previous_kind && current.text == previous_text {
                return Ok(());
            }
            if current.kind != record.applied_kind || current.text != record.applied_text {
                return Err(
                    "The saved memory changed after merge/replace; undo will not overwrite it."
                        .to_string(),
                );
            }
            agent::memory::update_entry(
                data,
                sandbox,
                packs,
                pack_id,
                session_id,
                &record.memory_id,
                previous_kind,
                previous_text,
            )?;
            Ok(())
        }
        action => Err(format!("Unknown companion memory undo action: {action}")),
    }
}

fn companion_queue_state(state: &AppState) -> companion::CompanionMemoryQueueState {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let (data, sandbox, packs, pack_id, session_id) = crate::biz::memory::memory_context(state);
    let panel = agent::memory::panel_state(&data, &sandbox, &packs, &pack_id, &session_id);
    let mut queue = companion::memory_queue_state(&data_dir);
    for item in &mut queue.items {
        if item.status != "pending" {
            continue;
        }
        if let Some(existing) = find_similar_memory_entry(&panel, item) {
            item.duplicate_memory_id = Some(existing.id);
            item.duplicate_memory_text = Some(existing.text);
        }
    }
    queue
}

fn find_saved_memory_id(
    panel: &agent::memory::MemoryPanelState,
    item: &companion::CompanionMemoryQueueItem,
) -> Option<String> {
    panel
        .entries
        .iter()
        .filter(|entry| {
            entry.scope == item.scope && entry.kind == item.kind && entry.text == item.text
        })
        .max_by_key(|entry| entry.line)
        .map(|entry| entry.id.clone())
}

fn find_similar_memory_entry(
    panel: &agent::memory::MemoryPanelState,
    item: &companion::CompanionMemoryQueueItem,
) -> Option<agent::memory::MemoryEntry> {
    panel
        .entries
        .iter()
        .filter(|entry| entry.scope == item.scope)
        .find(|entry| memory_text_similar(&entry.text, &item.text))
        .cloned()
}

fn memory_text_similar(a: &str, b: &str) -> bool {
    let a_key = normalize_memory_text_key(a);
    let b_key = normalize_memory_text_key(b);
    if a_key.is_empty() || b_key.is_empty() {
        return false;
    }
    if a_key == b_key
        || (a_key.len() > 14 && b_key.contains(&a_key))
        || (b_key.len() > 14 && a_key.contains(&b_key))
    {
        return true;
    }
    let a_words = a_key
        .split_whitespace()
        .collect::<std::collections::HashSet<_>>();
    let b_words = b_key
        .split_whitespace()
        .collect::<std::collections::HashSet<_>>();
    if a_words.len() < 3 || b_words.len() < 3 {
        return false;
    }
    let intersection = a_words.intersection(&b_words).count();
    let union = a_words.union(&b_words).count().max(1);
    (intersection as f32 / union as f32) >= 0.72
}

fn normalize_memory_text_key(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('-')
        .trim()
        .trim_start_matches("[user]")
        .trim_start_matches("[project]")
        .trim_start_matches("[session]")
        .trim_start_matches("[pack]")
        .trim_start_matches("[preference]")
        .trim_start_matches("[boundary]")
        .trim_start_matches("[routine]")
        .trim_start_matches("[stress]")
        .trim_start_matches("[encouragement]")
        .trim()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn merge_memory_text(existing: &str, incoming: &str) -> String {
    if memory_text_similar(existing, incoming) {
        incoming.to_string()
    } else {
        format!("{}; {}", existing.trim(), incoming.trim())
    }
}

#[cfg(test)]
mod companion_memory_undo_tests {
    use super::*;

    fn memory_fixture() -> (PathBuf, PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "demiurge_companion_undo_{}",
            store::new_session_id()
        ));
        let data = root.join("data");
        let sandbox = root.join("sandbox");
        let packs = root.join("packs");
        fs::create_dir_all(packs.join("default")).unwrap();
        (root, data, sandbox, packs)
    }

    #[test]
    fn merge_or_replace_undo_restores_the_previous_memory() {
        let (root, data, sandbox, packs) = memory_fixture();
        let panel = agent::memory::add_entry(
            &data,
            &sandbox,
            &packs,
            "default",
            "session-1",
            "user",
            "preference",
            "Uses concise replies",
        )
        .unwrap();
        let original = panel.entries[0].clone();
        agent::memory::update_entry(
            &data,
            &sandbox,
            &packs,
            "default",
            "session-1",
            &original.id,
            "preference",
            "Uses very concise replies",
        )
        .unwrap();
        let record = CompanionMemoryUndoRecord {
            queue_item_id: "queue-1".to_string(),
            memory_id: original.id.clone(),
            action: "updated".to_string(),
            previous_kind: Some(original.kind.clone()),
            previous_text: Some(original.text.clone()),
            applied_kind: "preference".to_string(),
            applied_text: "Uses very concise replies".to_string(),
        };
        store_companion_memory_undo_record(&data, record.clone()).unwrap();

        apply_companion_memory_undo(&data, &sandbox, &packs, "default", "session-1", &record)
            .unwrap();

        let panel = agent::memory::panel_state(&data, &sandbox, &packs, "default", "session-1");
        let restored = panel
            .entries
            .iter()
            .find(|entry| entry.id == original.id)
            .unwrap();
        assert_eq!(restored.kind, original.kind);
        assert_eq!(restored.text, original.text);
        assert!(load_companion_memory_undo_journal(&data)
            .unwrap()
            .records
            .contains_key("queue-1"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn undo_refuses_to_overwrite_a_memory_changed_after_save() {
        let (root, data, sandbox, packs) = memory_fixture();
        let panel = agent::memory::add_entry(
            &data,
            &sandbox,
            &packs,
            "default",
            "session-1",
            "user",
            "preference",
            "Initial saved text",
        )
        .unwrap();
        let created = panel.entries[0].clone();
        let record = CompanionMemoryUndoRecord {
            queue_item_id: "queue-2".to_string(),
            memory_id: created.id.clone(),
            action: "created".to_string(),
            previous_kind: None,
            previous_text: None,
            applied_kind: created.kind.clone(),
            applied_text: created.text.clone(),
        };
        agent::memory::update_entry(
            &data,
            &sandbox,
            &packs,
            "default",
            "session-1",
            &created.id,
            "preference",
            "User edited this later",
        )
        .unwrap();

        let error =
            apply_companion_memory_undo(&data, &sandbox, &packs, "default", "session-1", &record)
                .unwrap_err();
        assert!(error.contains("changed after creation"));
        let panel = agent::memory::panel_state(&data, &sandbox, &packs, "default", "session-1");
        assert!(panel
            .entries
            .iter()
            .any(|entry| entry.text == "User edited this later"));

        let _ = fs::remove_dir_all(root);
    }
}
