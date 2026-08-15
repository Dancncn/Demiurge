use std::fs;
use std::path::Path;

use demiurge_common::settings::{
    Settings, DEFAULT_MAX_CONTEXT_CHARS, DEFAULT_MAX_INPUT_TOKENS, DEFAULT_RESERVED_OUTPUT_TOKENS,
};

pub fn load_settings(dir: &Path) -> Settings {
    let p = dir.join("settings.json");
    let mut settings = fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
        .unwrap_or_default();
    // Migrate only the exact former defaults. A user who explicitly chose a
    // smaller budget must keep that choice; the new defaults are for fresh
    // installs and old untouched settings files.
    if settings.context_budget_auto
        && settings.max_context_chars == 24_000
        && settings.max_input_tokens == 32_000
        && settings.reserved_output_tokens == 4_000
    {
        settings.max_context_chars = DEFAULT_MAX_CONTEXT_CHARS;
        settings.max_input_tokens = DEFAULT_MAX_INPUT_TOKENS;
        settings.reserved_output_tokens = DEFAULT_RESERVED_OUTPUT_TOKENS;
    }
    settings
}

pub fn redacted_settings(s: &Settings) -> Settings {
    let mut safe = s.clone();
    safe.api_key.clear();
    safe.tavily_api_key.clear();
    safe.brave_search_api_key.clear();
    safe.exa_api_key.clear();
    safe.webdav_password.clear();
    safe.media_api_key.clear();
    safe.embedding_api_key.clear();
    for server in &mut safe.mcp_servers {
        for env in &mut server.env {
            if env.secret {
                env.value.clear();
            }
        }
    }
    safe
}

pub fn save_settings(dir: &Path, s: &Settings) -> Result<(), String> {
    let p = dir.join("settings.json");
    let safe = redacted_settings(s);
    let json = serde_json::to_string_pretty(&safe).map_err(|e| e.to_string())?;
    fs::write(&p, json).map_err(|e| e.to_string())
}
