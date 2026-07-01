use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::store::{now_millis, Settings};
use crate::AppState;

const WEATHER_CACHE_TTL_MS: u64 = 30 * 60 * 1000;

#[derive(Clone, Debug, Serialize)]
pub struct CompanionPanelState {
    pub enabled: bool,
    pub privacy: CompanionPrivacyState,
    pub user_state: CompanionUserState,
    pub weather: Option<WeatherCard>,
    pub suggestions: Vec<CompanionSuggestion>,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompanionPrivacyState {
    pub weather_enabled: bool,
    pub location_mode: String,
    pub city: String,
    pub note: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompanionUserState {
    pub mood: String,
    pub energy: String,
    pub focus: String,
    pub tone: String,
    pub do_not_disturb: String,
    pub recent_interaction_at: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct WeatherCard {
    pub city: String,
    pub country: String,
    pub temperature_c: f32,
    pub apparent_temperature_c: f32,
    pub precipitation_mm: f32,
    pub humidity_percent: Option<u8>,
    pub wind_speed_kmh: Option<f32>,
    pub weather_code: i32,
    pub condition: String,
    pub advice: Vec<String>,
    pub source: String,
    pub cached: bool,
    pub fetched_at: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompanionSuggestion {
    pub kind: String,
    pub priority: u8,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompanionMemorySuggestion {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompanionMemoryQueueItem {
    pub id: String,
    pub source_session: String,
    pub reason: String,
    pub scope: String,
    pub kind: String,
    pub text: String,
    pub created_at: u64,
    pub status: String,
    #[serde(default)]
    pub saved_memory_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompanionMemoryQueueState {
    pub path: String,
    pub pending_count: usize,
    pub items: Vec<CompanionMemoryQueueItem>,
}

#[derive(Clone)]
struct CachedWeather {
    card: WeatherCard,
    expires_at: u64,
}

static WEATHER_CACHE: OnceLock<Mutex<HashMap<String, CachedWeather>>> = OnceLock::new();

pub fn clear_weather_cache() -> usize {
    let mut cache = WEATHER_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap();
    let count = cache.len();
    cache.clear();
    count
}

pub fn prompt_context(settings: &Settings) -> String {
    if !settings.companion_enabled {
        return String::new();
    }
    let mut lines = vec![
        "Use this companion context only as a lightweight style and daily-life signal.".to_string(),
        "Do not imply medical, therapeutic, emergency, or continuous monitoring capability."
            .to_string(),
        format!("Tone preference: {}.", label_tone(&settings.companion_tone)),
        format!("User mood state: {}.", label_mood(&settings.companion_mood)),
        format!(
            "User energy state: {}.",
            label_energy(&settings.companion_energy)
        ),
        format!("Focus state: {}.", label_focus(&settings.companion_focus)),
    ];
    if !settings.companion_do_not_disturb.trim().is_empty() {
        lines.push(format!(
            "Do-not-disturb preference: {}. Avoid proactive nudges during this window unless explicitly asked.",
            settings.companion_do_not_disturb.trim()
        ));
    }
    if settings.weather_enabled && settings.weather_location_mode != "off" {
        if let Some(weather) = cached_weather_for_city(&settings.weather_city) {
            lines.push(format!(
                "Weather context: {} {}, temperature {:.0}C, feels like {:.0}C, precipitation {:.1}mm.",
                weather.city,
                weather.condition,
                weather.temperature_c,
                weather.apparent_temperature_c,
                weather.precipitation_mm
            ));
            for advice in weather.advice.iter().take(2) {
                lines.push(format!("Weather-based care hint: {advice}"));
            }
        } else if !settings.weather_city.trim().is_empty() {
            lines.push(format!(
                "Weather companion is enabled for manual city `{}`; no cached weather is available in this prompt.",
                settings.weather_city.trim()
            ));
        }
    }
    lines.join("\n")
}

pub fn memory_suggestions(settings: &Settings) -> Vec<CompanionMemorySuggestion> {
    if !settings.companion_enabled {
        return Vec::new();
    }
    let mut suggestions = Vec::new();
    suggestions.push(CompanionMemorySuggestion {
        id: "companion_tone".to_string(),
        kind: "preference".to_string(),
        text: format!(
            "Prefers {} companion tone for reminders and daily-life support.",
            label_tone(&settings.companion_tone)
        ),
        reason: "Store how the user likes to be reminded.".to_string(),
    });
    if !settings.companion_do_not_disturb.trim().is_empty() {
        suggestions.push(CompanionMemorySuggestion {
            id: "companion_dnd".to_string(),
            kind: "preference".to_string(),
            text: format!(
                "Prefers not to receive proactive companion nudges during {}.",
                settings.companion_do_not_disturb.trim()
            ),
            reason: "Store the user's interruption boundary.".to_string(),
        });
    }
    if settings.weather_enabled && settings.weather_location_mode != "off" {
        let location = settings.weather_city.trim();
        let text = if location.is_empty() {
            "Prefers weather-aware companion reminders when a manual city is configured."
                .to_string()
        } else {
            format!("Uses {location} as the manual city for weather-aware companion reminders.")
        };
        suggestions.push(CompanionMemorySuggestion {
            id: "companion_weather_city".to_string(),
            kind: "preference".to_string(),
            text,
            reason: "Store weather companion preference for future sessions.".to_string(),
        });
    }
    suggestions
}

pub fn memory_suggestion_by_id(settings: &Settings, id: &str) -> Option<CompanionMemorySuggestion> {
    memory_suggestions(settings)
        .into_iter()
        .find(|suggestion| suggestion.id == id)
}

pub fn memory_queue_state(data_dir: &Path) -> CompanionMemoryQueueState {
    let mut items = read_memory_queue(data_dir);
    items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let pending_count = items.iter().filter(|item| item.status == "pending").count();
    CompanionMemoryQueueState {
        path: memory_queue_path(data_dir).to_string_lossy().to_string(),
        pending_count,
        items,
    }
}

pub fn enqueue_memory_suggestion(
    data_dir: &Path,
    source_session: &str,
    suggestion: CompanionMemorySuggestion,
) -> Result<CompanionMemoryQueueState, String> {
    let mut items = read_memory_queue(data_dir);
    let dedupe_key = memory_queue_dedupe_key("user", &suggestion.kind, &suggestion.text);
    if let Some(existing) = items.iter_mut().find(|item| {
        item.status == "pending"
            && memory_queue_dedupe_key(&item.scope, &item.kind, &item.text) == dedupe_key
    }) {
        existing.reason = suggestion.reason;
        existing.source_session = source_session.to_string();
    } else {
        let seq = items.len() + 1;
        let created_at = now_millis();
        items.push(CompanionMemoryQueueItem {
            id: format!("cmq_{created_at}_{seq}"),
            source_session: source_session.to_string(),
            reason: suggestion.reason,
            scope: "user".to_string(),
            kind: suggestion.kind,
            text: suggestion.text,
            created_at,
            status: "pending".to_string(),
            saved_memory_id: None,
        });
    }
    write_memory_queue(data_dir, &items)?;
    Ok(memory_queue_state(data_dir))
}

pub fn mark_memory_queue_item(
    data_dir: &Path,
    id: &str,
    status: &str,
    saved_memory_id: Option<String>,
) -> Result<CompanionMemoryQueueState, String> {
    let status = match status {
        "pending" | "saved" | "ignored" => status,
        _ => return Err(format!("Unknown companion memory queue status: {status}")),
    };
    let mut items = read_memory_queue(data_dir);
    let item = items
        .iter_mut()
        .find(|item| item.id == id)
        .ok_or_else(|| format!("Unknown companion memory queue item: {id}"))?;
    item.status = status.to_string();
    item.saved_memory_id = saved_memory_id;
    write_memory_queue(data_dir, &items)?;
    Ok(memory_queue_state(data_dir))
}

pub fn pending_memory_queue_item(data_dir: &Path, id: &str) -> Option<CompanionMemoryQueueItem> {
    read_memory_queue(data_dir)
        .into_iter()
        .find(|item| item.id == id && item.status == "pending")
}

fn memory_queue_path(data_dir: &Path) -> PathBuf {
    data_dir.join("companion-memory-queue.json")
}

fn read_memory_queue(data_dir: &Path) -> Vec<CompanionMemoryQueueItem> {
    fs::read_to_string(memory_queue_path(data_dir))
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<CompanionMemoryQueueItem>>(&raw).ok())
        .unwrap_or_default()
}

fn write_memory_queue(data_dir: &Path, items: &[CompanionMemoryQueueItem]) -> Result<(), String> {
    fs::create_dir_all(data_dir).map_err(|e| format!("Failed to create data directory: {e}"))?;
    let raw = serde_json::to_string_pretty(items)
        .map_err(|e| format!("Failed to serialize companion memory queue: {e}"))?;
    fs::write(memory_queue_path(data_dir), raw)
        .map_err(|e| format!("Failed to write companion memory queue: {e}"))
}

fn memory_queue_dedupe_key(scope: &str, kind: &str, text: &str) -> String {
    format!(
        "{}:{}:{}",
        scope.trim().to_ascii_lowercase(),
        kind.trim().to_ascii_lowercase(),
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase()
    )
}

fn cached_weather_for_city(city: &str) -> Option<WeatherCard> {
    let cache_key = city.trim().to_ascii_lowercase();
    if cache_key.is_empty() {
        return None;
    }
    let now = now_millis();
    WEATHER_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap()
        .get(&cache_key)
        .and_then(|cached| {
            if cached.expires_at > now {
                let mut card = cached.card.clone();
                card.cached = true;
                Some(card)
            } else {
                None
            }
        })
}

pub async fn panel_state(state: &AppState) -> CompanionPanelState {
    let settings = state.settings.lock().unwrap().clone();
    let recent_interaction_at = {
        let sessions = state.sessions.lock().unwrap();
        sessions
            .get(&sessions.active)
            .map(|session| session.updated_at)
            .unwrap_or_default()
    };
    let weather = if settings.companion_enabled
        && settings.weather_enabled
        && settings.weather_location_mode != "off"
    {
        fetch_weather(state, &settings).await.ok()
    } else {
        None
    };
    let suggestions = build_suggestions(&settings, weather.as_ref());
    CompanionPanelState {
        enabled: settings.companion_enabled,
        privacy: CompanionPrivacyState {
            weather_enabled: settings.weather_enabled,
            location_mode: settings.weather_location_mode.clone(),
            city: settings.weather_city.clone(),
            note: privacy_note(&settings),
        },
        user_state: CompanionUserState {
            mood: settings.companion_mood.clone(),
            energy: settings.companion_energy.clone(),
            focus: settings.companion_focus.clone(),
            tone: settings.companion_tone.clone(),
            do_not_disturb: settings.companion_do_not_disturb.clone(),
            recent_interaction_at,
        },
        weather,
        suggestions,
        updated_at: now_millis(),
    }
}

async fn fetch_weather(state: &AppState, settings: &Settings) -> Result<WeatherCard, String> {
    let city = settings.weather_city.trim();
    if city.is_empty() {
        return Err("Weather city is empty.".to_string());
    }
    let cache_key = city.to_ascii_lowercase();
    let now = now_millis();
    if let Some(cached) = WEATHER_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap()
        .get(&cache_key)
        .cloned()
    {
        if cached.expires_at > now {
            let mut card = cached.card;
            card.cached = true;
            return Ok(card);
        }
    }

    let place = geocode_city(&state.http, city).await?;
    let mut card = forecast(&state.http, &place).await?;
    card.advice = weather_advice(&card);
    WEATHER_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap()
        .insert(
            cache_key,
            CachedWeather {
                card: card.clone(),
                expires_at: now + WEATHER_CACHE_TTL_MS,
            },
        );
    Ok(card)
}

#[derive(Deserialize)]
struct GeocodeResponse {
    results: Option<Vec<GeocodePlace>>,
}

#[derive(Clone, Deserialize)]
struct GeocodePlace {
    name: String,
    latitude: f64,
    longitude: f64,
    #[serde(default)]
    country: String,
    #[serde(default)]
    admin1: String,
}

async fn geocode_city(client: &reqwest::Client, city: &str) -> Result<GeocodePlace, String> {
    let response = client
        .get("https://geocoding-api.open-meteo.com/v1/search")
        .query(&[
            ("name", city),
            ("count", "1"),
            ("language", "zh"),
            ("format", "json"),
        ])
        .send()
        .await
        .map_err(|e| format!("Weather geocoding failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Weather geocoding failed: HTTP {}",
            response.status()
        ));
    }
    let body = response
        .json::<GeocodeResponse>()
        .await
        .map_err(|e| format!("Weather geocoding parse failed: {e}"))?;
    body.results
        .and_then(|mut results| results.drain(..).next())
        .ok_or_else(|| format!("No weather location found for `{city}`."))
}

#[derive(Deserialize)]
struct ForecastResponse {
    current: ForecastCurrent,
}

#[derive(Deserialize)]
struct ForecastCurrent {
    temperature_2m: f32,
    apparent_temperature: f32,
    precipitation: f32,
    #[serde(default)]
    relative_humidity_2m: Option<u8>,
    #[serde(default)]
    wind_speed_10m: Option<f32>,
    weather_code: i32,
}

async fn forecast(client: &reqwest::Client, place: &GeocodePlace) -> Result<WeatherCard, String> {
    let response = client
        .get("https://api.open-meteo.com/v1/forecast")
        .query(&[
            ("latitude", place.latitude.to_string()),
            ("longitude", place.longitude.to_string()),
            (
                "current",
                "temperature_2m,apparent_temperature,precipitation,relative_humidity_2m,wind_speed_10m,weather_code"
                    .to_string(),
            ),
            ("timezone", "auto".to_string()),
        ])
        .send()
        .await
        .map_err(|e| format!("Weather forecast failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Weather forecast failed: HTTP {}",
            response.status()
        ));
    }
    let body = response
        .json::<ForecastResponse>()
        .await
        .map_err(|e| format!("Weather forecast parse failed: {e}"))?;
    let place_name = if place.admin1.trim().is_empty() {
        place.name.clone()
    } else {
        format!("{}, {}", place.name, place.admin1)
    };
    Ok(WeatherCard {
        city: place_name,
        country: place.country.clone(),
        temperature_c: body.current.temperature_2m,
        apparent_temperature_c: body.current.apparent_temperature,
        precipitation_mm: body.current.precipitation,
        humidity_percent: body.current.relative_humidity_2m,
        wind_speed_kmh: body.current.wind_speed_10m,
        weather_code: body.current.weather_code,
        condition: weather_condition(body.current.weather_code).to_string(),
        advice: Vec::new(),
        source: "Open-Meteo".to_string(),
        cached: false,
        fetched_at: now_millis(),
    })
}

fn build_suggestions(
    settings: &Settings,
    weather: Option<&WeatherCard>,
) -> Vec<CompanionSuggestion> {
    let mut items = Vec::new();
    if !settings.companion_enabled {
        return items;
    }
    if settings.companion_focus == "focusing" {
        items.push(CompanionSuggestion {
            kind: "focus".to_string(),
            priority: 2,
            text: "专注中，主动提醒会保持克制。".to_string(),
        });
    }
    if settings.companion_energy == "low" {
        items.push(CompanionSuggestion {
            kind: "energy".to_string(),
            priority: 2,
            text: "今天可以把任务切小一点，先完成一个最轻的起手动作。".to_string(),
        });
    }
    if matches!(settings.companion_mood.as_str(), "stressed" | "down") {
        items.push(CompanionSuggestion {
            kind: "mood".to_string(),
            priority: 3,
            text: "如果压力偏高，我会优先给支持性陪伴和生活辅助，不替代专业帮助。".to_string(),
        });
    }
    if let Some(weather) = weather {
        for advice in &weather.advice {
            items.push(CompanionSuggestion {
                kind: "weather".to_string(),
                priority: 1,
                text: advice.clone(),
            });
        }
    }
    if items.is_empty() {
        items.push(CompanionSuggestion {
            kind: "check_in".to_string(),
            priority: 1,
            text: tone_default_hint(&settings.companion_tone).to_string(),
        });
    }
    items.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| a.kind.cmp(&b.kind))
    });
    items.truncate(4);
    items
}

fn weather_advice(card: &WeatherCard) -> Vec<String> {
    let mut advice = Vec::new();
    if card.precipitation_mm > 0.1 || matches!(card.weather_code, 51..=67 | 80..=82 | 95..=99) {
        advice.push("可能有降雨，出门前看一眼伞和通勤路况。".to_string());
    }
    if card.apparent_temperature_c >= 32.0 {
        advice.push("体感温度偏高，补水和避开暴晒会更舒服。".to_string());
    } else if card.apparent_temperature_c <= 3.0 {
        advice.push("体感温度偏低，出门多加一层，别让冷风偷走精力。".to_string());
    }
    if card.wind_speed_kmh.unwrap_or_default() >= 30.0 {
        advice.push("风力有点明显，轻便物品和外套帽子留意一下。".to_string());
    }
    advice
}

fn weather_condition(code: i32) -> &'static str {
    match code {
        0 => "晴",
        1..=3 => "多云",
        45 | 48 => "雾",
        51..=57 => "毛毛雨",
        61..=67 => "雨",
        71..=77 => "雪",
        80..=82 => "阵雨",
        85 | 86 => "阵雪",
        95..=99 => "雷雨",
        _ => "未知",
    }
}

fn privacy_note(settings: &Settings) -> String {
    if !settings.weather_enabled || settings.weather_location_mode == "off" {
        return "天气陪伴已关闭，不会查询城市天气。".to_string();
    }
    if settings.weather_location_mode == "manual" {
        return "天气仅使用你手动填写的城市；当前原型不做自动定位。".to_string();
    }
    "自动定位暂未启用；会继续按手动城市查询。".to_string()
}

fn tone_default_hint(tone: &str) -> &'static str {
    match tone {
        "quiet" => "我会保持安静，只在有用的时候轻轻提醒。",
        "bright" => "今天先抓一个小胜利，节奏起来就很好。",
        "wry" => "任务可以慢慢啃，先别让待办列表反过来指挥你。",
        "coach" => "先定一个 25 分钟内能完成的小目标，然后开始。",
        _ => "我会用温柔克制的方式陪你推进今天的事。",
    }
}

fn label_tone(value: &str) -> &'static str {
    match value {
        "quiet" => "quiet and restrained",
        "bright" => "bright and encouraging",
        "wry" => "lightly wry, without being mean",
        "coach" => "efficient coach",
        _ => "gentle and restrained",
    }
}

fn label_mood(value: &str) -> &'static str {
    match value {
        "good" => "good",
        "stressed" => "stressed",
        "down" => "down",
        _ => "neutral",
    }
}

fn label_energy(value: &str) -> &'static str {
    match value {
        "low" => "low",
        "high" => "high",
        _ => "normal",
    }
}

fn label_focus(value: &str) -> &'static str {
    match value {
        "focusing" => "focusing; avoid unnecessary interruptions",
        "resting" => "resting; keep responses soft and low-pressure",
        _ => "available",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            companion_enabled: true,
            companion_mood: "stressed".to_string(),
            companion_energy: "low".to_string(),
            companion_focus: "focusing".to_string(),
            ..Settings::default()
        }
    }

    #[test]
    fn suggestions_include_state_and_weather_without_overflowing() {
        let weather = WeatherCard {
            city: "Hangzhou".to_string(),
            country: "China".to_string(),
            temperature_c: 35.0,
            apparent_temperature_c: 37.0,
            precipitation_mm: 0.0,
            humidity_percent: Some(60),
            wind_speed_kmh: Some(8.0),
            weather_code: 0,
            condition: "晴".to_string(),
            advice: vec!["体感温度偏高，补水和避开暴晒会更舒服。".to_string()],
            source: "Open-Meteo".to_string(),
            cached: false,
            fetched_at: 1,
        };
        let suggestions = build_suggestions(&settings(), Some(&weather));
        assert!(suggestions.len() <= 4);
        assert!(suggestions.iter().any(|item| item.kind == "mood"));
        assert!(suggestions.iter().any(|item| item.kind == "weather"));
    }

    #[test]
    fn prompt_context_keeps_companion_boundaries() {
        let text = prompt_context(&settings());
        assert!(text.contains("Tone preference"));
        assert!(text.contains("Do not imply medical"));
        assert!(text.contains("Focus state"));
    }

    #[test]
    fn memory_suggestions_capture_stable_preferences() {
        let mut settings = settings();
        settings.companion_tone = "coach".to_string();
        settings.companion_do_not_disturb = "23:00-08:00".to_string();
        let suggestions = memory_suggestions(&settings);
        assert!(suggestions.iter().any(|item| item.id == "companion_tone"));
        assert!(suggestions.iter().any(|item| item.id == "companion_dnd"));

        settings.companion_enabled = false;
        assert!(memory_suggestions(&settings).is_empty());
    }

    #[test]
    fn memory_queue_keeps_pending_items_with_review_metadata() {
        let root = std::env::temp_dir().join(format!(
            "demiurge_companion_memory_queue_{}",
            crate::store::now_millis()
        ));
        let suggestion = CompanionMemorySuggestion {
            id: "tone".to_string(),
            kind: "preference".to_string(),
            text: "Prefers gentle reminders.".to_string(),
            reason: "Stable companion tone preference.".to_string(),
        };

        let state = enqueue_memory_suggestion(&root, "session_1", suggestion.clone()).unwrap();
        assert_eq!(state.pending_count, 1);
        let item = &state.items[0];
        assert_eq!(item.source_session, "session_1");
        assert_eq!(item.scope, "user");
        assert_eq!(item.kind, "preference");
        assert_eq!(item.status, "pending");

        let state = enqueue_memory_suggestion(&root, "session_2", suggestion).unwrap();
        assert_eq!(state.pending_count, 1);
        assert_eq!(state.items.len(), 1);
        assert_eq!(state.items[0].source_session, "session_2");

        let state = mark_memory_queue_item(&root, &state.items[0].id, "ignored", None).unwrap();
        assert_eq!(state.pending_count, 0);
        assert_eq!(state.items[0].status, "ignored");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn privacy_note_does_not_claim_auto_location() {
        let mut settings = Settings::default();
        settings.weather_enabled = true;
        settings.weather_location_mode = "auto".to_string();
        assert!(privacy_note(&settings).contains("暂未启用"));
    }
}
