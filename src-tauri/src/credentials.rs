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

#[derive(Clone, Copy)]
enum SecretKind {
    Llm,
    Tavily,
    Brave,
    Exa,
}

impl SecretKind {
    fn account(self) -> &'static str {
        match self {
            SecretKind::Llm => API_KEY_ACCOUNT,
            SecretKind::Tavily => TAVILY_API_KEY_ACCOUNT,
            SecretKind::Brave => BRAVE_SEARCH_API_KEY_ACCOUNT,
            SecretKind::Exa => EXA_API_KEY_ACCOUNT,
        }
    }

    fn label(self) -> &'static str {
        match self {
            SecretKind::Llm => "API Key",
            SecretKind::Tavily => "Tavily API Key",
            SecretKind::Brave => "Brave Search API Key",
            SecretKind::Exa => "Exa API Key",
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

/// Load API keys from keyring, migrating legacy plaintext keys if present.
pub fn hydrate_or_migrate_settings(dir: &Path, settings: &mut Settings) -> Result<(), String> {
    let legacy_plaintext = settings.api_key.trim().to_string();
    let legacy_tavily = settings.tavily_api_key.trim().to_string();
    let legacy_brave = settings.brave_search_api_key.trim().to_string();
    let legacy_exa = settings.exa_api_key.trim().to_string();
    let has_legacy_plaintext = !legacy_plaintext.is_empty();
    let has_legacy_web_key =
        !legacy_tavily.is_empty() || !legacy_brave.is_empty() || !legacy_exa.is_empty();

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

    if has_legacy_plaintext || has_legacy_web_key {
        store::save_settings(dir, settings)?;
    }
    Ok(())
}
