use std::fs;
use std::path::Path;

use demiurge_common::settings::Settings;

pub fn load_settings(dir: &Path) -> Settings {
    let p = dir.join("settings.json");
    fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
        .unwrap_or_default()
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
