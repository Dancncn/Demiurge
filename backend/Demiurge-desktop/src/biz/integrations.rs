//! 外部 Agent 资源用例：扫描、显式导入和安全配置快照。

use std::path::PathBuf;

use crate::integrations::{
    self, ExternalProvider, IntegrationSnapshot, MarketSearchResult, MarketSkill, SkillCandidate,
};
use crate::store::Session;
use crate::{agent, AppState};

pub(crate) struct IntegrationsBiz;

impl IntegrationsBiz {
    pub fn scan(state: &AppState) -> IntegrationSnapshot {
        let sandbox = state.sandbox_dir.lock().unwrap().clone();
        let data_dir = state.data_dir.lock().unwrap().clone();
        integrations::scan(&sandbox, &data_dir)
    }

    pub fn import_skill(
        state: &AppState,
        source_path: String,
        source: Option<String>,
    ) -> Result<SkillCandidate, String> {
        let data_dir = state.data_dir.lock().unwrap().clone();
        integrations::import_skill(&data_dir, PathBuf::from(source_path).as_path(), source)
    }

    pub fn set_skill_enabled(state: &AppState, id: String, enabled: bool) -> Result<(), String> {
        let data_dir = state.data_dir.lock().unwrap().clone();
        integrations::set_skill_enabled(&data_dir, &id, enabled)
    }

    pub fn remove_skill(state: &AppState, id: String) -> Result<(), String> {
        let data_dir = state.data_dir.lock().unwrap().clone();
        integrations::remove_skill(&data_dir, &id)
    }

    pub fn session_messages(
        provider: String,
        source_path: String,
    ) -> Result<Vec<integrations::ExternalSessionMessage>, String> {
        let provider = parse_provider(&provider)?;
        integrations::load_session_messages(&provider, PathBuf::from(source_path).as_path())
    }

    pub fn import_session(
        state: &AppState,
        provider: String,
        source_path: String,
    ) -> Result<ImportedSessionBo, String> {
        let provider = parse_provider(&provider)?;
        let (meta, messages) =
            integrations::import_session(&provider, PathBuf::from(source_path).as_path())?;
        let workspace = state
            .sandbox_dir
            .lock()
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let mut session = Session::new();
        session.title = meta.title.clone();
        session.workspace_path = workspace;
        for message in &messages {
            let message = match message.role.as_str() {
                "assistant" => {
                    agent::conversation::Message::assistant_text(message.content.clone())
                }
                "system" => agent::conversation::Message::system(message.content.clone()),
                "tool" => agent::conversation::Message {
                    role: "tool".to_string(),
                    content: Some(message.content.clone()),
                    ..Default::default()
                },
                _ => agent::conversation::Message::user(message.content.clone()),
            };
            session.append_message(message);
        }
        let imported_id = session.id.clone();
        {
            let mut sessions = state.sessions.lock().unwrap();
            sessions.active = imported_id.clone();
            sessions.sessions.push(session);
        }
        state.persist_sessions();
        Ok(ImportedSessionBo {
            id: imported_id,
            title: meta.title,
            provider: provider.label().to_string(),
            source_path: meta.source_path,
            message_count: messages.len(),
        })
    }

    pub fn import_config(
        state: &AppState,
        provider: String,
        kind: String,
    ) -> Result<integrations::ImportedConfig, String> {
        let provider = parse_provider(&provider)?;
        let data_dir = state.data_dir.lock().unwrap().clone();
        integrations::import_config(&data_dir, provider, &kind)
    }

    pub async fn search_market(
        state: &AppState,
        query: String,
        limit: usize,
    ) -> Result<MarketSearchResult, String> {
        integrations::search_market(&state.http, &query, limit).await
    }

    pub async fn install_market_skill(
        state: &AppState,
        skill: MarketSkill,
    ) -> Result<SkillCandidate, String> {
        let data_dir = state.data_dir.lock().unwrap().clone();
        integrations::install_market_skill(&state.http, &data_dir, &skill).await
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ImportedSessionBo {
    pub id: String,
    pub title: String,
    pub provider: String,
    pub source_path: String,
    pub message_count: usize,
}

fn parse_provider(value: &str) -> Result<ExternalProvider, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "codex" => Ok(ExternalProvider::Codex),
        "claude" | "claude_code" | "claude-code" => Ok(ExternalProvider::Claude),
        _ => Err(format!("不支持的外部 provider：{value}")),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_provider;

    #[test]
    fn provider_parser_accepts_codex_and_claude_code_alias() {
        assert!(parse_provider("codex").is_ok());
        assert!(parse_provider("claude-code").is_ok());
        assert!(parse_provider("unknown").is_err());
    }
}
