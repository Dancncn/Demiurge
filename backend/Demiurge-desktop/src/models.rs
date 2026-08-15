//! Model catalog and pricing snapshots.
//!
//! The catalog is a replaceable integration seam: OpenRouter is refreshed from
//! its public model API, then the snapshot is used for context hints and local
//! usage-cost projection. It never changes the deterministic agent loop.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::AppState;

const CATALOG_FILE: &str = "model_catalog.json";
const OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";
const MAX_CATALOG_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelCatalogEntry {
    pub id: String,
    pub name: String,
    pub context_length: u64,
    pub input_cost_per_million: Option<f64>,
    pub output_cost_per_million: Option<f64>,
    pub cache_read_cost_per_million: Option<f64>,
    pub cache_write_cost_per_million: Option<f64>,
    #[serde(default)]
    pub supported_parameters: Vec<String>,
    pub source: String,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ModelCatalog {
    pub source: String,
    pub fetched_at: Option<u64>,
    pub models: Vec<ModelCatalogEntry>,
}

#[derive(Deserialize)]
struct OpenRouterResponse {
    data: Vec<OpenRouterModel>,
}

#[derive(Deserialize)]
struct OpenRouterModel {
    id: String,
    name: Option<String>,
    context_length: Option<u64>,
    pricing: Option<Value>,
    supported_parameters: Option<Vec<String>>,
}

pub fn load(state: &AppState) -> ModelCatalog {
    let dir = state.data_dir.lock().unwrap().clone();
    load_from_path(&dir.join(CATALOG_FILE))
}

pub async fn refresh_openrouter(state: &AppState) -> Result<ModelCatalog, String> {
    let response = state
        .http
        .get(OPENROUTER_MODELS_URL)
        .query(&[
            ("output_modalities", "text"),
            ("supported_parameters", "tools"),
            ("sort", "newest"),
        ])
        .send()
        .await
        .map_err(|e| format!("OpenRouter 模型目录请求失败：{e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "OpenRouter 模型目录返回 HTTP {}",
            response.status()
        ));
    }
    let body = response
        .text()
        .await
        .map_err(|e| format!("读取 OpenRouter 模型目录失败：{e}"))?;
    if body.len() > MAX_CATALOG_BYTES {
        return Err("OpenRouter 模型目录超过本地安全大小上限".to_string());
    }
    let parsed: OpenRouterResponse =
        serde_json::from_str(&body).map_err(|e| format!("解析 OpenRouter 模型目录失败：{e}"))?;
    let fetched_at = now_millis();
    let models = parsed
        .data
        .into_iter()
        .filter(|model| !model.id.trim().is_empty())
        .map(|model| ModelCatalogEntry {
            id: model.id.clone(),
            name: model.name.unwrap_or_else(|| model.id.clone()),
            context_length: model.context_length.unwrap_or_default(),
            input_cost_per_million: price_per_million(model.pricing.as_ref(), "prompt"),
            output_cost_per_million: price_per_million(model.pricing.as_ref(), "completion"),
            cache_read_cost_per_million: price_per_million(model.pricing.as_ref(), "cache_read"),
            cache_write_cost_per_million: price_per_million(model.pricing.as_ref(), "cache_write"),
            supported_parameters: model.supported_parameters.unwrap_or_default(),
            source: "openrouter".to_string(),
            updated_at: fetched_at,
        })
        .collect::<Vec<_>>();
    let catalog = ModelCatalog {
        source: "openrouter".to_string(),
        fetched_at: Some(fetched_at),
        models,
    };
    let dir = state.data_dir.lock().unwrap().clone();
    fs::create_dir_all(&dir).map_err(|e| format!("创建模型目录失败：{e}"))?;
    let raw =
        serde_json::to_string_pretty(&catalog).map_err(|e| format!("序列化模型目录失败：{e}"))?;
    crate::store::atomic_write_text(&dir.join(CATALOG_FILE), &raw, true)
        .map_err(|e| format!("保存模型目录失败：{e}"))?;
    Ok(catalog)
}

/// Return a pricing row for a provider/model pair, if the catalog has one.
/// OpenRouter variants keep their full id (including `:free`) so a free route
/// never accidentally inherits the paid price of its base model.
pub fn pricing_for(data_dir: &Path, provider: &str, model: &str) -> Option<ModelCatalogEntry> {
    if !provider.eq_ignore_ascii_case("openrouter") {
        return None;
    }
    let catalog = load_from_path(&data_dir.join(CATALOG_FILE));
    catalog.models.into_iter().find(|entry| {
        entry.id.eq_ignore_ascii_case(model.trim())
            || entry
                .id
                .eq_ignore_ascii_case(model.trim_start_matches("openrouter/"))
    })
}

fn load_from_path(path: &Path) -> ModelCatalog {
    let Ok(metadata) = fs::metadata(path) else {
        return ModelCatalog::default();
    };
    if metadata.len() as usize > MAX_CATALOG_BYTES {
        return ModelCatalog::default();
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn price_per_million(pricing: Option<&Value>, key: &str) -> Option<f64> {
    let value = pricing?.get(key)?;
    let per_token = value
        .as_str()
        .and_then(|raw| raw.parse::<f64>().ok())
        .or_else(|| value.as_f64())?;
    if per_token.is_finite() && per_token >= 0.0 {
        Some(per_token * 1_000_000.0)
    } else {
        None
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openrouter_prices_convert_to_per_million() {
        let value = serde_json::json!({"prompt": "0.0000025", "completion": "0.00001"});
        assert_eq!(price_per_million(Some(&value), "prompt"), Some(2.5));
        assert_eq!(price_per_million(Some(&value), "completion"), Some(10.0));
    }

    #[test]
    fn malformed_or_oversized_catalog_fails_closed_to_empty() {
        let root = std::env::temp_dir().join(format!("demiurge_models_{}", now_millis()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(CATALOG_FILE), "not-json").unwrap();
        assert!(load_from_path(&root.join(CATALOG_FILE)).models.is_empty());
        let _ = fs::remove_dir_all(root);
    }
}
