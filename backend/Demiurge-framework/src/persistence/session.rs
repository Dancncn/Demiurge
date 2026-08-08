use std::fs;
use std::path::Path;

use demiurge_common::conversation::Conversation;
use demiurge_core::session::{derive_title, now_millis, Session, SessionStore};
use serde_json::Value;

use super::atomic_file::{atomic_replace_bytes, atomic_write_text, backup_path};

pub fn load_sessions(dir: &Path) -> SessionStore {
    let p = dir.join("sessions.json");
    if let Ok(store) = read_session_store(&p) {
        let mut store = store;
        store.ensure_one();
        return store;
    }

    let backup = backup_path(&p);
    if let Ok(mut recovered) = read_session_store(&backup) {
        recovered.ensure_one();
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
    store.ensure_one();
    store
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
