//! 本地文件持久化 Adapter。

mod atomic_file;
mod session;
mod settings;

pub use atomic_file::{atomic_write, atomic_write_text, backup_path};
pub use session::{load_sessions, save_sessions, session_store_value};
pub use settings::{load_settings, redacted_settings, save_settings};

#[cfg(test)]
mod tests {
    use std::fs;

    use demiurge_common::conversation::{Message, ToolExecutionRecord, ToolExecutionStatus};
    use demiurge_common::mcp::{McpEnvVar, McpServerConfig, McpTransportKind};
    use demiurge_common::settings::Settings;
    use demiurge_core::session::{new_session_id, Session, SessionStore};

    use super::{atomic_write_text, backup_path, load_sessions, save_sessions, save_settings};

    fn temp_dir(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("demiurge_{label}_{}", new_session_id()))
    }

    #[test]
    fn sessions_round_trip_local_execution_metadata() {
        let root = temp_dir("session_metadata");
        fs::create_dir_all(&root).unwrap();
        let mut session = Session::new();
        session.messages.push(Message::tool_result_with_execution(
            "call-1",
            "edit_file",
            "updated",
            ToolExecutionRecord {
                status: ToolExecutionStatus::Ok,
                error: None,
                duration_ms: 17,
                affected_paths: vec!["src/main.rs".to_string()],
            },
        ));
        let id = session.id.clone();
        let store = SessionStore {
            active: id.clone(),
            sessions: vec![session],
        };

        save_sessions(&root, &store).unwrap();
        let loaded = load_sessions(&root);
        let execution = loaded.get(&id).unwrap().messages[0]
            .tool_execution
            .as_ref()
            .unwrap();
        assert_eq!(execution.status, ToolExecutionStatus::Ok);
        assert_eq!(execution.duration_ms, 17);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_primary_recovers_from_the_atomic_backup() {
        let root = temp_dir("session_recovery");
        fs::create_dir_all(&root).unwrap();
        let mut session = Session::new();
        session.title = "recover me".to_string();
        let id = session.id.clone();
        let first = SessionStore {
            active: id.clone(),
            sessions: vec![session],
        };
        save_sessions(&root, &first).unwrap();
        let mut second = first.clone();
        second.sessions[0].title = "newer snapshot".to_string();
        save_sessions(&root, &second).unwrap();
        fs::write(root.join("sessions.json"), b"{not-json").unwrap();

        let recovered = load_sessions(&root);
        assert_eq!(recovered.get(&id).unwrap().title, "recover me");
        assert!(serde_json::from_str::<SessionStore>(
            &fs::read_to_string(root.join("sessions.json")).unwrap()
        )
        .is_ok());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn atomic_write_keeps_the_previous_version_without_temp_files() {
        let root = temp_dir("atomic");
        let path = root.join("memory.md");
        atomic_write_text(&path, "first", true).unwrap();
        atomic_write_text(&path, "second", true).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        assert_eq!(fs::read_to_string(backup_path(&path)).unwrap(), "first");
        assert!(!fs::read_dir(&root).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        }));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn settings_repository_redacts_all_secrets() {
        let root = temp_dir("settings");
        fs::create_dir_all(&root).unwrap();
        let settings = Settings {
            api_key: "sk-secret".to_string(),
            tavily_api_key: "search-secret".to_string(),
            media_api_key: "media-secret".to_string(),
            mcp_servers: vec![McpServerConfig {
                name: "secret-server".to_string(),
                enabled: true,
                transport: McpTransportKind::Stdio,
                command: "cmd".to_string(),
                args: Vec::new(),
                env: vec![McpEnvVar {
                    key: "API_TOKEN".to_string(),
                    value: "mcp-secret".to_string(),
                    secret: true,
                }],
            }],
            ..Settings::default()
        };

        save_settings(&root, &settings).unwrap();
        let raw = fs::read_to_string(root.join("settings.json")).unwrap();
        assert!(!raw.contains("sk-secret"));
        assert!(!raw.contains("search-secret"));
        assert!(!raw.contains("media-secret"));
        assert!(!raw.contains("mcp-secret"));
        let _ = fs::remove_dir_all(root);
    }
}
