//! Immutable ownership for background work. UI selection is never an execution identity.
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionContext {
    pub session_id: String,
    pub workspace_root: PathBuf,
}

impl ExecutionContext {
    pub fn capture_selected(state: &crate::AppState) -> Result<Self, String> {
        // Session selection uses this same lock; capture its identity and root together.
        let _runtime = state.session_engine.lock().unwrap();
        let session_id = state.sessions.lock().unwrap().active.clone();
        Self::for_session(state, &session_id)
    }

    pub fn for_session(state: &crate::AppState, session_id: &str) -> Result<Self, String> {
        let configured = state
            .sessions
            .lock()
            .unwrap()
            .get(session_id)
            .map(|session| session.workspace_path.clone())
            .ok_or_else(|| "Execution session no longer exists.".to_string())?;
        if configured.trim().is_empty() {
            return Err(
                "Execution session has no explicit workspace; select a project first.".to_string(),
            );
        }
        let workspace_root = std::fs::canonicalize(configured)
            .map_err(|e| format!("Cannot resolve execution workspace: {e}"))?;
        let context = Self {
            session_id: session_id.to_string(),
            workspace_root,
        };
        context.validate(state)?;
        Ok(context)
    }

    pub fn validate_root(&self) -> Result<(), String> {
        if !self.workspace_root.is_absolute() || !self.workspace_root.is_dir() {
            return Err("Execution workspace is unavailable.".to_string());
        }
        let resolved = std::fs::canonicalize(&self.workspace_root)
            .map_err(|e| format!("Cannot verify execution workspace: {e}"))?;
        if resolved != self.workspace_root {
            return Err("Execution workspace identity changed.".to_string());
        }
        Ok(())
    }

    pub fn validate(&self, state: &crate::AppState) -> Result<(), String> {
        self.validate_root()?;
        if self.session_id.is_empty()
            || state
                .sessions
                .lock()
                .unwrap()
                .get(&self.session_id)
                .is_none()
        {
            return Err("Execution session no longer exists.".to_string());
        }
        Ok(())
    }
}
