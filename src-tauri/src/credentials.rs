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

fn api_key_entry() -> Result<Entry, String> {
    Entry::new(SERVICE, API_KEY_ACCOUNT).map_err(|e| format!("打开系统凭据管理器失败：{e}"))
}

pub fn load_api_key() -> Result<Option<String>, String> {
    let entry = api_key_entry()?;
    match entry.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("读取 API Key 失败：{e}")),
    }
}

pub fn save_api_key(secret: &str) -> Result<(), String> {
    let entry = api_key_entry()?;
    let secret = secret.trim();
    if secret.is_empty() {
        return match entry.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("删除 API Key 失败：{e}")),
        };
    }
    entry
        .set_password(secret)
        .map_err(|e| format!("保存 API Key 到系统凭据管理器失败：{e}"))
}

/// Load the API key from keyring, migrating a legacy plaintext key if present.
pub fn hydrate_or_migrate_settings(dir: &Path, settings: &mut Settings) -> Result<(), String> {
    let legacy_plaintext = settings.api_key.trim().to_string();
    if !legacy_plaintext.is_empty() {
        save_api_key(&legacy_plaintext)?;
        settings.api_key = legacy_plaintext;
        store::save_settings(dir, settings)?;
        return Ok(());
    }

    if let Some(secret) = load_api_key()? {
        settings.api_key = secret;
    }
    Ok(())
}
