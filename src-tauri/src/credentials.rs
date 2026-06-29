//! System credential storage.
//!
//! Keep API keys out of `settings.json`. The runtime `Settings` still carries
//! the key in memory so the existing provider adapters do not need to know
//! where the secret came from.
use std::path::Path;

use keyring::{Entry, Error};

use crate::store::{self, Settings};

const SERVICE: &str = "com.demiurge.engine";
const API_KEY_ACCOUNT: &str = "llm_api_key";
const TAVILY_API_KEY_ACCOUNT: &str = "web_search_tavily_api_key";
const BRAVE_SEARCH_API_KEY_ACCOUNT: &str = "web_search_brave_api_key";
const EXA_API_KEY_ACCOUNT: &str = "web_search_exa_api_key";
const WEBDAV_PASSWORD_ACCOUNT: &str = "webdav_password";
const MEDIA_API_KEY_ACCOUNT: &str = "media_api_key";

#[derive(Clone, Copy)]
enum SecretKind {
    Llm,
    Tavily,
    Brave,
    Exa,
    WebDav,
    Media,
}

impl SecretKind {
    fn account(self) -> &'static str {
        match self {
            SecretKind::Llm => API_KEY_ACCOUNT,
            SecretKind::Tavily => TAVILY_API_KEY_ACCOUNT,
            SecretKind::Brave => BRAVE_SEARCH_API_KEY_ACCOUNT,
            SecretKind::Exa => EXA_API_KEY_ACCOUNT,
            SecretKind::WebDav => WEBDAV_PASSWORD_ACCOUNT,
            SecretKind::Media => MEDIA_API_KEY_ACCOUNT,
        }
    }

    fn label(self) -> &'static str {
        match self {
            SecretKind::Llm => "API Key",
            SecretKind::Tavily => "Tavily API Key",
            SecretKind::Brave => "Brave Search API Key",
            SecretKind::Exa => "Exa API Key",
            SecretKind::WebDav => "WebDAV Password",
            SecretKind::Media => "Media API Key",
        }
    }
}

fn secret_entry(kind: SecretKind) -> Result<Entry, String> {
    Entry::new(SERVICE, kind.account()).map_err(|e| format!("打开系统凭据管理器失败：{e}"))
}

fn load_secret(kind: SecretKind) -> Result<Option<String>, String> {
    let entry = secret_entry(kind)?;
    match entry.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("读取 {} 失败：{e}", kind.label())),
    }
}

fn save_secret(kind: SecretKind, secret: &str) -> Result<(), String> {
    let entry = secret_entry(kind)?;
    let secret = secret.trim();
    if secret.is_empty() {
        return match entry.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("删除 {} 失败：{e}", kind.label())),
        };
    }
    entry
        .set_password(secret)
        .map_err(|e| format!("保存 {} 到系统凭据管理器失败：{e}", kind.label()))
}

fn mcp_env_account(server_name: &str, env_key: &str) -> String {
    let raw = format!("{server_name}\n{env_key}");
    format!(
        "mcp_env_{}_{}_{}",
        credential_segment(server_name, 24),
        credential_segment(env_key, 32),
        stable_hash_hex(&raw)
    )
}

fn credential_segment(value: &str, max_len: usize) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if ch == '_' || ch == '-' {
            out.push('_');
        } else if ch.is_whitespace() || ch == '.' || ch == ':' || ch == '/' || ch == '\\' {
            out.push('_');
        }
        if out.len() >= max_len {
            break;
        }
    }
    let out = out.trim_matches('_').to_string();
    if out.is_empty() {
        "unnamed".to_string()
    } else {
        out
    }
}

fn stable_hash_hex(value: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in value.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn mcp_env_entry(server_name: &str, env_key: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, &mcp_env_account(server_name, env_key))
        .map_err(|e| format!("open MCP env credential failed: {e}"))
}

fn load_mcp_env_secret(server_name: &str, env_key: &str) -> Result<Option<String>, String> {
    let entry = mcp_env_entry(server_name, env_key)?;
    match entry.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(Error::NoEntry) => Ok(None),
        Err(e) => Err(format!(
            "read MCP env `{server_name}.{env_key}` failed: {e}"
        )),
    }
}

fn save_mcp_env_secret(server_name: &str, env_key: &str, secret: &str) -> Result<(), String> {
    let entry = mcp_env_entry(server_name, env_key)?;
    let secret = secret.trim();
    if secret.is_empty() {
        return match entry.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(e) => Err(format!(
                "delete MCP env `{server_name}.{env_key}` failed: {e}"
            )),
        };
    }
    entry
        .set_password(secret)
        .map_err(|e| format!("save MCP env `{server_name}.{env_key}` failed: {e}"))
}

pub fn load_api_key() -> Result<Option<String>, String> {
    load_secret(SecretKind::Llm)
}

pub fn save_api_key(secret: &str) -> Result<(), String> {
    save_secret(SecretKind::Llm, secret)
}

pub fn load_tavily_api_key() -> Result<Option<String>, String> {
    load_secret(SecretKind::Tavily)
}

pub fn load_brave_search_api_key() -> Result<Option<String>, String> {
    load_secret(SecretKind::Brave)
}

pub fn load_exa_api_key() -> Result<Option<String>, String> {
    load_secret(SecretKind::Exa)
}

pub fn save_web_search_api_keys(settings: &Settings) -> Result<(), String> {
    save_secret(SecretKind::Tavily, &settings.tavily_api_key)?;
    save_secret(SecretKind::Brave, &settings.brave_search_api_key)?;
    save_secret(SecretKind::Exa, &settings.exa_api_key)?;
    Ok(())
}

pub fn load_webdav_password() -> Result<Option<String>, String> {
    load_secret(SecretKind::WebDav)
}

pub fn save_webdav_password(secret: &str) -> Result<(), String> {
    save_secret(SecretKind::WebDav, secret)
}

pub fn load_media_api_key() -> Result<Option<String>, String> {
    load_secret(SecretKind::Media)
}

pub fn save_media_api_key(secret: &str) -> Result<(), String> {
    save_secret(SecretKind::Media, secret)
}

pub fn save_mcp_env_secrets(settings: &Settings) -> Result<(), String> {
    for server in &settings.mcp_servers {
        for env in &server.env {
            if env.secret {
                save_mcp_env_secret(&server.name, &env.key, &env.value)?;
            }
        }
    }
    Ok(())
}

/// Load API keys from keyring, migrating legacy plaintext keys if present.
pub fn hydrate_or_migrate_settings(dir: &Path, settings: &mut Settings) -> Result<(), String> {
    let legacy_plaintext = settings.api_key.trim().to_string();
    let legacy_tavily = settings.tavily_api_key.trim().to_string();
    let legacy_brave = settings.brave_search_api_key.trim().to_string();
    let legacy_exa = settings.exa_api_key.trim().to_string();
    let legacy_webdav_password = settings.webdav_password.trim().to_string();
    let legacy_media = settings.media_api_key.trim().to_string();
    let mut has_legacy_mcp_env = false;
    let has_legacy_plaintext = !legacy_plaintext.is_empty();
    let has_legacy_web_key =
        !legacy_tavily.is_empty() || !legacy_brave.is_empty() || !legacy_exa.is_empty();
    let has_legacy_webdav_password = !legacy_webdav_password.is_empty();
    let has_legacy_media = !legacy_media.is_empty();

    if has_legacy_plaintext {
        save_api_key(&legacy_plaintext)?;
        settings.api_key = legacy_plaintext;
    } else if let Some(secret) = load_api_key()? {
        settings.api_key = secret;
    }

    if !legacy_tavily.is_empty() {
        save_secret(SecretKind::Tavily, &legacy_tavily)?;
        settings.tavily_api_key = legacy_tavily;
    } else if let Some(secret) = load_tavily_api_key()? {
        settings.tavily_api_key = secret;
    }

    if !legacy_brave.is_empty() {
        save_secret(SecretKind::Brave, &legacy_brave)?;
        settings.brave_search_api_key = legacy_brave;
    } else if let Some(secret) = load_brave_search_api_key()? {
        settings.brave_search_api_key = secret;
    }

    if !legacy_exa.is_empty() {
        save_secret(SecretKind::Exa, &legacy_exa)?;
        settings.exa_api_key = legacy_exa;
    } else if let Some(secret) = load_exa_api_key()? {
        settings.exa_api_key = secret;
    }

    if has_legacy_webdav_password {
        save_secret(SecretKind::WebDav, &legacy_webdav_password)?;
        settings.webdav_password = legacy_webdav_password;
    } else if let Some(secret) = load_webdav_password()? {
        settings.webdav_password = secret;
    }

    if has_legacy_media {
        save_secret(SecretKind::Media, &legacy_media)?;
        settings.media_api_key = legacy_media;
    } else if let Some(secret) = load_media_api_key()? {
        settings.media_api_key = secret;
    }

    for server in &mut settings.mcp_servers {
        for env in &mut server.env {
            if !env.secret {
                continue;
            }
            let legacy_secret = env.value.trim().to_string();
            if !legacy_secret.is_empty() {
                save_mcp_env_secret(&server.name, &env.key, &legacy_secret)?;
                env.value = legacy_secret;
                has_legacy_mcp_env = true;
            } else if let Some(secret) = load_mcp_env_secret(&server.name, &env.key)? {
                env.value = secret;
            }
        }
    }

    if has_legacy_plaintext
        || has_legacy_web_key
        || has_legacy_webdav_password
        || has_legacy_media
        || has_legacy_mcp_env
    {
        store::save_settings(dir, settings)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_env_account_is_stable_and_sanitized() {
        let account = mcp_env_account("My Server", "OPENAI_API_KEY");
        assert!(account.starts_with("mcp_env_my_server_openai_api_key_"));
        assert_eq!(account, mcp_env_account("My Server", "OPENAI_API_KEY"));
    }
}
