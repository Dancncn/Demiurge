use std::fs;
use std::path::Path;

use demiurge_common::conversation::Conversation;
use demiurge_core::session::{derive_title, now_millis, Session, SessionStore};
use serde_json::Value;

use super::atomic_file::{atomic_replace_bytes, atomic_write_text, backup_path};

pub fn load_sessions(dir: &Path) -> SessionStore {
    let p = dir.join("sessions.json");
    if let Ok(store) = read_session_store(&p) {
        let (store, changed) = prepare_loaded_store(store);
        if changed {
            if let Err(error) = save_sessions(dir, &store) {
                eprintln!("Loaded sessions but failed to persist event migration: {error}");
            }
        }
        return store;
    }

    let backup = backup_path(&p);
    if let Ok(recovered) = read_session_store(&backup) {
        let (recovered, _) = prepare_loaded_store(recovered);
        if p.exists() {
            let corrupt = dir.join(format!("sessions.json.corrupt-{}", now_millis()));
            if let Err(error) = fs::copy(&p, &corrupt) {
                eprintln!(
                    "Failed to preserve corrupt session file {}: {error}",
                    p.display()
                );
            }
        }
        match session_store_json(&recovered)
            .and_then(|json| atomic_replace_bytes(&p, json.as_bytes()))
        {
            Ok(()) => eprintln!("Recovered sessions from {}", backup.display()),
            Err(error) => eprintln!("Loaded session backup but failed to repair primary: {error}"),
        }
        return recovered;
    }

    // 迁移：旧版单会话
    let mut store = SessionStore::default();
    let legacy = dir.join("conversation.json");
    if let Some(conv) = fs::read_to_string(&legacy)
        .ok()
        .and_then(|s| serde_json::from_str::<Conversation>(&s).ok())
    {
        if !conv.messages.is_empty() {
            let title = derive_title(&conv.messages);
            let mut s = Session::new();
            s.title = title;
            s.messages = conv.messages;
            store.active = s.id.clone();
            store.sessions.push(s);
        }
    }
    prepare_loaded_store(store).0
}

pub fn save_sessions(dir: &Path, store: &SessionStore) -> Result<(), String> {
    let p = dir.join("sessions.json");
    let json = session_store_json(store)?;
    atomic_write_text(&p, &json, true)
}

fn read_session_store(path: &Path) -> Result<SessionStore, String> {
    let raw =
        fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("Failed to parse {}: {e}", path.display()))
}

fn prepare_loaded_store(mut store: SessionStore) -> (SessionStore, bool) {
    store.ensure_one();
    let mut changed = false;
    for session in &mut store.sessions {
        if session.seed_legacy_events() {
            changed = true;
        }
        let previous_messages = session.messages.clone();
        let previous_summary = session.summary.clone();
        session.rebuild_projection();
        if session.messages != previous_messages || session.summary != previous_summary {
            changed = true;
        }
    }
    (store, changed)
}

fn session_store_json(store: &SessionStore) -> Result<String, String> {
    serde_json::to_string_pretty(&session_store_value(store)?).map_err(|e| e.to_string())
}

pub fn session_store_value(store: &SessionStore) -> Result<Value, String> {
    let mut value = serde_json::to_value(store).map_err(|e| e.to_string())?;
    let sessions = value
        .get_mut("sessions")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "Serialized session store is missing sessions".to_string())?;
    for (session_value, session) in sessions.iter_mut().zip(&store.sessions) {
        let messages = session_value
            .get_mut("messages")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| format!("Serialized session {} is missing messages", session.id))?;
        for (message_value, message) in messages.iter_mut().zip(&session.messages) {
            *message_value = message.history_value();
        }
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::load_sessions;

    #[test]
    fn legacy_sessions_are_seeded_once_and_rebuilt_from_the_event_source() {
        let root = std::env::temp_dir().join(format!(
            "demiurge_session_event_seed_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("sessions.json"),
            r#"{
                "active": "legacy",
                "sessions": [{
                    "id": "legacy",
                    "title": "旧会话",
                    "summary": "legacy summary",
                    "messages": [{"role":"user","content":"legacy message"}],
                    "updated_at": 123
                }]
            }"#,
        )
        .unwrap();

        let loaded = load_sessions(&root);
        let session = loaded.get("legacy").unwrap();
        assert_eq!(
            session.messages[0].content.as_deref(),
            Some("legacy message")
        );
        assert_eq!(session.summary.as_deref(), Some("legacy summary"));
        assert_eq!(session.events.len(), 1);
        assert_eq!(session.events[0].seq, 1);

        let persisted: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(root.join("sessions.json")).unwrap()).unwrap();
        assert_eq!(
            persisted["sessions"][0]["events"].as_array().unwrap().len(),
            1
        );
        assert_eq!(
            persisted["sessions"][0]["events"][0]["source"],
            "legacy_seed"
        );

        let loaded_again = load_sessions(&root);
        assert_eq!(loaded_again.get("legacy").unwrap().events.len(), 1);
        assert_eq!(loaded_again.get("legacy").unwrap().events[0].seq, 1);

        let _ = fs::remove_dir_all(root);
    }
}
