use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::agent::conversation::Message;
use crate::llm;
use crate::store::{now_millis, Settings};
use crate::AppState;

const WEATHER_CACHE_TTL_MS: u64 = 30 * 60 * 1000;
const MAX_COMPANION_MEMORY_INPUT_CHARS: usize = 6_000;
const MAX_COMPANION_MEMORY_ITEMS: usize = 4;
const MAX_COMPANION_MEMORY_TEXT_CHARS: usize = 220;

#[derive(Clone, Debug, Serialize)]
pub struct CompanionPanelState {
    pub enabled: bool,
    pub privacy: CompanionPrivacyState,
    pub user_state: CompanionUserState,
    pub weather: Option<WeatherCard>,
    pub weather_cache: WeatherCacheState,
    pub weather_error: Option<String>,
    pub suggestions: Vec<CompanionSuggestion>,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompanionPrivacyState {
    pub weather_enabled: bool,
    pub provider: String,
    pub location_mode: String,
    pub city: String,
    pub note: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct WeatherCacheState {
    pub entries: usize,
    pub active_city: Option<String>,
    pub active_cached: bool,
    pub expires_at: Option<u64>,
    pub ttl_ms: u64,
    pub last_error: Option<String>,
    pub location_cached: bool,
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
    pub uv_index: Option<f32>,
    pub air_quality_index: Option<u16>,
    pub pm2_5: Option<f32>,
    pub day_temperature_min_c: Option<f32>,
    pub day_temperature_max_c: Option<f32>,
    pub commute_precipitation_probability: Option<u8>,
    pub severe_weather: bool,
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
pub struct HighRiskDetection {
    pub kind: String,
    pub severity: String,
    pub support_message: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duplicate_memory_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duplicate_memory_text: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompanionMemoryQueueState {
    pub path: String,
    pub pending_count: usize,
    pub items: Vec<CompanionMemoryQueueItem>,
}

#[derive(Deserialize)]
struct CompanionMemoryExtraction {
    #[serde(default)]
    memories: Vec<CompanionMemoryCandidate>,
}

#[derive(Deserialize)]
struct CompanionMemoryCandidate {
    scope: Option<String>,
    kind: Option<String>,
    text: Option<String>,
    reason: Option<String>,
}

#[derive(Clone)]
struct CachedWeather {
    card: WeatherCard,
    expires_at: u64,
}

#[derive(Clone)]
struct CachedLocation {
    city: String,
    expires_at: u64,
}

static WEATHER_CACHE: OnceLock<Mutex<HashMap<String, CachedWeather>>> = OnceLock::new();
static LOCATION_CACHE: OnceLock<Mutex<Option<CachedLocation>>> = OnceLock::new();

pub fn clear_weather_cache() -> usize {
    let mut cache = WEATHER_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap();
    let count = cache.len();
    cache.clear();
    if let Ok(mut location) = LOCATION_CACHE.get_or_init(|| Mutex::new(None)).lock() {
        *location = None;
    }
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

pub fn detect_high_risk_expression(text: &str) -> Option<HighRiskDetection> {
    let normalized = text.to_ascii_lowercase();
    let compact = text.split_whitespace().collect::<String>();
    if contains_any(
        &normalized,
        &[
            "kill myself",
            "end my life",
            "suicide",
            "self harm",
            "hurt myself",
            "don't want to live",
            "do not want to live",
        ],
    ) || contains_any(
        &compact,
        &[
            "自杀",
            "轻生",
            "想死",
            "不想活",
            "结束生命",
            "伤害自己",
            "自残",
            "活不下去",
        ],
    ) {
        return Some(HighRiskDetection {
            kind: "self_harm_or_crisis".to_string(),
            severity: "high".to_string(),
            support_message: crisis_support_message(),
        });
    }
    if contains_any(
        &normalized,
        &[
            "replace therapy",
            "instead of therapy",
            "instead of a doctor",
            "stop my medication",
            "diagnose me",
        ],
    ) || contains_any(
        &compact,
        &[
            "替代心理治疗",
            "替代医生",
            "不用看医生",
            "停药",
            "诊断我",
            "代替咨询师",
        ],
    ) {
        return Some(HighRiskDetection {
            kind: "medical_or_therapy_substitution".to_string(),
            severity: "medium".to_string(),
            support_message: professional_boundary_message(),
        });
    }
    None
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn crisis_support_message() -> String {
    "我听见你现在可能很难受。先别一个人扛着：如果你有马上伤害自己的风险，请立刻联系当地紧急服务，或去最近的急诊/安全地点；如果你在美国或加拿大，可以拨打或短信 988。也请尽快联系一个现实里可信任的人，让 TA 陪你待一会儿。\n\n我可以继续陪你把当下这几分钟撑过去，但我不能替代紧急救助或专业支持。".to_string()
}

fn professional_boundary_message() -> String {
    "这类问题我可以做信息整理、陪你准备就医/咨询时要说的重点，但不能替代医生、心理治疗师或紧急干预，也不建议自行停药或把诊断交给聊天来决定。更稳妥的下一步是联系合格专业人士；如果风险正在升高，请优先联系当地紧急服务或可信任的人。".to_string()
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
    enqueue_memory_queue_item(
        data_dir,
        source_session,
        &suggestion.reason,
        "user",
        &suggestion.kind,
        &suggestion.text,
    )
}

pub fn enqueue_memory_queue_item(
    data_dir: &Path,
    source_session: &str,
    reason: &str,
    scope: &str,
    kind: &str,
    text: &str,
) -> Result<CompanionMemoryQueueState, String> {
    let scope = normalize_memory_scope(scope);
    let kind = normalize_memory_kind(kind);
    let text = sanitize_memory_text(text, MAX_COMPANION_MEMORY_TEXT_CHARS);
    if text.is_empty() {
        return Ok(memory_queue_state(data_dir));
    }
    let reason = sanitize_memory_text(reason, 180);
    let mut items = read_memory_queue(data_dir);
    let dedupe_key = memory_queue_dedupe_key(&scope, &kind, &text);
    if let Some(existing) = items.iter_mut().find(|item| {
        item.status == "pending"
            && memory_queue_dedupe_key(&item.scope, &item.kind, &item.text) == dedupe_key
    }) {
        existing.reason = reason;
        existing.source_session = source_session.to_string();
    } else {
        let seq = items.len() + 1;
        let created_at = now_millis();
        items.push(CompanionMemoryQueueItem {
            id: format!("cmq_{created_at}_{seq}"),
            source_session: source_session.to_string(),
            reason,
            scope,
            kind,
            text,
            created_at,
            status: "pending".to_string(),
            saved_memory_id: None,
            duplicate_memory_id: None,
            duplicate_memory_text: None,
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

pub async fn extract_memory_to_queue(
    client: &reqwest::Client,
    settings: &Settings,
    data_dir: &Path,
    source_session: &str,
    user_text: &str,
    assistant_text: &str,
    cancel: &AtomicBool,
) -> Result<CompanionMemoryQueueState, String> {
    let profile = llm::ProviderProfile::for_kind(settings.provider);
    if !settings.companion_enabled
        || !settings.companion_memory_extraction_enabled
        || (profile.requires_api_key && settings.api_key.trim().is_empty())
        || cancel.load(Ordering::Relaxed)
    {
        return Ok(memory_queue_state(data_dir));
    }

    let turn_text = cap_text(
        &format!("User:\n{user_text}\n\nAssistant:\n{assistant_text}"),
        MAX_COMPANION_MEMORY_INPUT_CHARS,
    );
    let prompt = format!(
        r#"Extract only stable companion-memory candidates from this conversation turn.
Allowed categories:
- stress sources the user explicitly described;
- sleep/work rhythm or break preferences;
- preferred names or forms of address;
- reminder preferences and disliked reminder styles;
- encouragement style that is likely to help the user.

Do not extract sensitive crisis/medical/therapy-risk content, secrets, transient tasks, guesses, or anything uncertain.
Every item must be user-reviewable before it is saved.
Return at most 4 items. Use scope "user" unless the note is clearly only session-local.
Return JSON only:
{{"memories":[{{"scope":"user|session","kind":"preference|boundary|routine|stress|encouragement","text":"...","reason":"why this looks durable"}}]}}

Conversation:
{turn_text}"#
    );

    let messages = vec![
        Message::system(
            "You extract companion memory candidates for user review. Output JSON only.",
        ),
        Message::user(prompt),
    ];
    let turn =
        llm::stream_completion(client, settings, &messages, &json!([]), |_| {}, cancel).await?;
    if cancel.load(Ordering::Relaxed) || turn.finish_reason == "interrupted" {
        return Ok(memory_queue_state(data_dir));
    }

    let extraction = parse_companion_memory_extraction(&turn.content)?;
    let candidates = normalize_companion_memory_candidates(extraction.memories);
    for candidate in candidates {
        let _ = enqueue_memory_queue_item(
            data_dir,
            source_session,
            &candidate
                .reason
                .unwrap_or_else(|| "LLM companion memory extraction.".to_string()),
            candidate.scope.as_deref().unwrap_or("user"),
            candidate.kind.as_deref().unwrap_or("preference"),
            candidate.text.as_deref().unwrap_or_default(),
        )?;
    }
    Ok(memory_queue_state(data_dir))
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

fn parse_companion_memory_extraction(content: &str) -> Result<CompanionMemoryExtraction, String> {
    let trimmed = content.trim();
    let json_text = if let Some(inner) = trimmed.strip_prefix("```json") {
        inner.trim().trim_end_matches("```").trim()
    } else if let Some(inner) = trimmed.strip_prefix("```") {
        inner.trim().trim_end_matches("```").trim()
    } else {
        trimmed
    };
    serde_json::from_str::<CompanionMemoryExtraction>(json_text)
        .map_err(|e| format!("Failed to parse companion memory extraction result: {e}"))
}

fn normalize_companion_memory_candidates(
    candidates: Vec<CompanionMemoryCandidate>,
) -> Vec<CompanionMemoryCandidate> {
    let mut out = Vec::new();
    for candidate in candidates.into_iter().take(MAX_COMPANION_MEMORY_ITEMS) {
        let text = sanitize_memory_text(
            candidate.text.as_deref().unwrap_or_default(),
            MAX_COMPANION_MEMORY_TEXT_CHARS,
        );
        if text.is_empty() {
            continue;
        }
        out.push(CompanionMemoryCandidate {
            scope: Some(normalize_memory_scope(
                candidate.scope.as_deref().unwrap_or("user"),
            )),
            kind: Some(normalize_memory_kind(
                candidate.kind.as_deref().unwrap_or("preference"),
            )),
            text: Some(text),
            reason: Some(sanitize_memory_text(
                candidate
                    .reason
                    .as_deref()
                    .unwrap_or("LLM companion memory extraction."),
                180,
            )),
        });
    }
    out
}

fn normalize_memory_scope(scope: &str) -> String {
    match scope.trim().to_ascii_lowercase().as_str() {
        "session" => "session".to_string(),
        _ => "user".to_string(),
    }
}

fn normalize_memory_kind(kind: &str) -> String {
    match kind.trim().to_ascii_lowercase().as_str() {
        "boundary" | "routine" | "stress" | "encouragement" => kind.trim().to_ascii_lowercase(),
        _ => "preference".to_string(),
    }
}

fn sanitize_memory_text(text: &str, max_chars: usize) -> String {
    cap_text(
        &text.split_whitespace().collect::<Vec<_>>().join(" "),
        max_chars,
    )
}

fn cap_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        text.chars().take(max_chars).collect()
    }
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
    let (weather, weather_error) = if settings.companion_enabled
        && settings.weather_enabled
        && settings.weather_location_mode != "off"
    {
        match fetch_weather(state, &settings).await {
            Ok(card) => (Some(card), None),
            Err(err) => (None, Some(err)),
        }
    } else {
        (None, None)
    };
    let suggestions = build_suggestions(&settings, weather.as_ref(), recent_interaction_at);
    CompanionPanelState {
        enabled: settings.companion_enabled,
        privacy: CompanionPrivacyState {
            weather_enabled: settings.weather_enabled,
            provider: weather_provider_label(&settings.weather_provider).to_string(),
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
        weather_cache: weather_cache_state(&settings, weather_error.clone()),
        weather_error,
        suggestions,
        updated_at: now_millis(),
    }
}

async fn fetch_weather(state: &AppState, settings: &Settings) -> Result<WeatherCard, String> {
    let city = resolve_weather_city(&state.http, settings).await?;
    let city = city.trim();
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

    let place = weather_provider(settings)
        .geocode(&state.http, city)
        .await?;
    let mut card = weather_provider(settings)
        .forecast(&state.http, &place)
        .await?;
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

#[derive(Clone, Copy)]
enum WeatherProvider {
    OpenMeteo,
}

impl WeatherProvider {
    async fn geocode(&self, client: &reqwest::Client, city: &str) -> Result<GeocodePlace, String> {
        match self {
            WeatherProvider::OpenMeteo => geocode_city(client, city).await,
        }
    }

    async fn forecast(
        &self,
        client: &reqwest::Client,
        place: &GeocodePlace,
    ) -> Result<WeatherCard, String> {
        match self {
            WeatherProvider::OpenMeteo => forecast(client, place).await,
        }
    }
}

fn weather_provider(settings: &Settings) -> WeatherProvider {
    match settings.weather_provider.trim() {
        "open_meteo" | "" => WeatherProvider::OpenMeteo,
        _ => WeatherProvider::OpenMeteo,
    }
}

fn weather_provider_label(value: &str) -> &'static str {
    match value.trim() {
        "open_meteo" | "" => "Open-Meteo",
        _ => "Open-Meteo",
    }
}

async fn resolve_weather_city(
    client: &reqwest::Client,
    settings: &Settings,
) -> Result<String, String> {
    if settings.weather_location_mode == "auto" && settings.weather_city.trim().is_empty() {
        return coarse_location_city(client).await;
    }
    Ok(settings.weather_city.trim().to_string())
}

#[derive(Deserialize)]
struct IpLocationResponse {
    #[serde(default)]
    city: String,
    #[serde(default)]
    region: String,
    #[serde(default)]
    country_name: String,
}

async fn coarse_location_city(client: &reqwest::Client) -> Result<String, String> {
    let now = now_millis();
    if let Some(cached) = LOCATION_CACHE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap()
        .clone()
    {
        if cached.expires_at > now {
            return Ok(cached.city);
        }
    }
    let response = client
        .get("https://ipapi.co/json/")
        .send()
        .await
        .map_err(|e| format!("Coarse weather location failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Coarse weather location failed: HTTP {}",
            response.status()
        ));
    }
    let body = response
        .json::<IpLocationResponse>()
        .await
        .map_err(|e| format!("Coarse weather location parse failed: {e}"))?;
    let city = if body.city.trim().is_empty() {
        return Err("Coarse weather location did not return a city.".to_string());
    } else if body.region.trim().is_empty() {
        format!("{}, {}", body.city.trim(), body.country_name.trim())
    } else {
        format!("{}, {}", body.city.trim(), body.region.trim())
    };
    *LOCATION_CACHE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap() = Some(CachedLocation {
        city: city.clone(),
        expires_at: now + WEATHER_CACHE_TTL_MS,
    });
    Ok(city)
}

fn weather_cache_state(settings: &Settings, last_error: Option<String>) -> WeatherCacheState {
    let cache = WEATHER_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap();
    let cache_key = settings.weather_city.trim().to_ascii_lowercase();
    let active = if cache_key.is_empty() && settings.weather_location_mode == "auto" {
        cache
            .values()
            .filter(|cached| cached.expires_at > now_millis())
            .max_by_key(|cached| cached.card.fetched_at)
    } else if cache_key.is_empty() {
        None
    } else {
        cache.get(&cache_key)
    };
    let location_cached = LOCATION_CACHE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap()
        .as_ref()
        .map(|cached| cached.expires_at > now_millis())
        .unwrap_or(false);
    WeatherCacheState {
        entries: cache.len(),
        active_city: active.map(|cached| cached.card.city.clone()),
        active_cached: active
            .map(|cached| cached.expires_at > now_millis())
            .unwrap_or(false),
        expires_at: active.map(|cached| cached.expires_at),
        ttl_ms: WEATHER_CACHE_TTL_MS,
        last_error,
        location_cached,
    }
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
    #[serde(default)]
    daily: Option<ForecastDaily>,
    #[serde(default)]
    hourly: Option<ForecastHourly>,
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

#[derive(Deserialize)]
struct ForecastDaily {
    #[serde(default)]
    temperature_2m_max: Vec<f32>,
    #[serde(default)]
    temperature_2m_min: Vec<f32>,
    #[serde(default)]
    uv_index_max: Vec<f32>,
}

#[derive(Deserialize)]
struct ForecastHourly {
    #[serde(default)]
    precipitation_probability: Vec<Option<u8>>,
}

#[derive(Deserialize)]
struct AirQualityResponse {
    #[serde(default)]
    current: Option<AirQualityCurrent>,
}

#[derive(Deserialize)]
struct AirQualityCurrent {
    #[serde(default)]
    us_aqi: Option<u16>,
    #[serde(default)]
    pm2_5: Option<f32>,
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
            (
                "daily",
                "temperature_2m_max,temperature_2m_min,uv_index_max".to_string(),
            ),
            ("hourly", "precipitation_probability".to_string()),
            ("forecast_days", "1".to_string()),
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
    let air = air_quality(client, place).await.ok().flatten();
    let daily = body.daily.as_ref();
    let hourly = body.hourly.as_ref();
    let commute_precipitation_probability = hourly.and_then(|hourly| {
        hourly
            .precipitation_probability
            .iter()
            .take(24)
            .flatten()
            .max()
            .copied()
    });
    let day_temperature_min_c = daily.and_then(|daily| daily.temperature_2m_min.first().copied());
    let day_temperature_max_c = daily.and_then(|daily| daily.temperature_2m_max.first().copied());
    let uv_index = daily.and_then(|daily| daily.uv_index_max.first().copied());
    let severe_weather = matches!(body.current.weather_code, 95..=99)
        || body.current.wind_speed_10m.unwrap_or_default() >= 45.0
        || body.current.apparent_temperature >= 38.0
        || body.current.apparent_temperature <= -8.0;
    Ok(WeatherCard {
        city: place_name,
        country: place.country.clone(),
        temperature_c: body.current.temperature_2m,
        apparent_temperature_c: body.current.apparent_temperature,
        precipitation_mm: body.current.precipitation,
        humidity_percent: body.current.relative_humidity_2m,
        wind_speed_kmh: body.current.wind_speed_10m,
        uv_index,
        air_quality_index: air.as_ref().and_then(|air| air.us_aqi),
        pm2_5: air.as_ref().and_then(|air| air.pm2_5),
        day_temperature_min_c,
        day_temperature_max_c,
        commute_precipitation_probability,
        severe_weather,
        weather_code: body.current.weather_code,
        condition: weather_condition(body.current.weather_code).to_string(),
        advice: Vec::new(),
        source: "Open-Meteo".to_string(),
        cached: false,
        fetched_at: now_millis(),
    })
}

async fn air_quality(
    client: &reqwest::Client,
    place: &GeocodePlace,
) -> Result<Option<AirQualityCurrent>, String> {
    let response = client
        .get("https://air-quality-api.open-meteo.com/v1/air-quality")
        .query(&[
            ("latitude", place.latitude.to_string()),
            ("longitude", place.longitude.to_string()),
            ("current", "us_aqi,pm2_5".to_string()),
            ("timezone", "auto".to_string()),
        ])
        .send()
        .await
        .map_err(|e| format!("Weather air-quality failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Weather air-quality failed: HTTP {}",
            response.status()
        ));
    }
    let body = response
        .json::<AirQualityResponse>()
        .await
        .map_err(|e| format!("Weather air-quality parse failed: {e}"))?;
    Ok(body.current)
}

fn build_suggestions(
    settings: &Settings,
    weather: Option<&WeatherCard>,
    recent_interaction_at: u64,
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
                priority: 2,
                text: advice.clone(),
            });
        }
    }
    items.extend(proactive_reminder_candidates(
        settings,
        weather,
        recent_interaction_at,
    ));
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

fn proactive_reminder_candidates(
    settings: &Settings,
    weather: Option<&WeatherCard>,
    recent_interaction_at: u64,
) -> Vec<CompanionSuggestion> {
    let mut items = Vec::new();
    if settings.companion_focus == "focusing" {
        items.push(CompanionSuggestion {
            kind: "reminder_policy".to_string(),
            priority: 1,
            text: "专注状态下，主动提醒只留在卡片里，默认不发桌面通知。".to_string(),
        });
        return items;
    }
    if !settings.companion_do_not_disturb.trim().is_empty() {
        items.push(CompanionSuggestion {
            kind: "reminder_policy".to_string(),
            priority: 1,
            text: format!(
                "免打扰偏好是 {}；桌面通知需要另外授权。",
                settings.companion_do_not_disturb.trim()
            ),
        });
    }
    let now = now_millis();
    let inactive_for_ms = now.saturating_sub(recent_interaction_at);
    if recent_interaction_at > 0 && inactive_for_ms >= 4 * 60 * 60 * 1000 {
        items.push(CompanionSuggestion {
            kind: "reminder_time".to_string(),
            priority: 1,
            text: "有一阵没互动了，卡片里只留一个轻量候选：回来时先做一个小起手动作。".to_string(),
        });
    }
    if matches!(
        weather.map(|card| card.weather_code),
        Some(61..=67 | 80..=82 | 95..=99)
    ) {
        items.push(CompanionSuggestion {
            kind: "reminder_weather".to_string(),
            priority: 1,
            text: "天气候选只在卡片展示；出门前再确认降雨和通勤即可。".to_string(),
        });
    }
    items
}

fn weather_advice(card: &WeatherCard) -> Vec<String> {
    let mut advice = Vec::new();
    if card.severe_weather {
        advice.push("天气有偏极端信号，出门前看一眼本地预警和交通信息。".to_string());
    }
    if card.precipitation_mm > 0.1 || matches!(card.weather_code, 51..=67 | 80..=82 | 95..=99) {
        advice.push("可能有降雨，出门前看一眼伞和通勤路况。".to_string());
    }
    if card.commute_precipitation_probability.unwrap_or_default() >= 60 {
        advice.push("今天降雨概率偏高，通勤时间可以多留一点缓冲。".to_string());
    }
    if card.apparent_temperature_c >= 32.0 {
        advice.push("体感温度偏高，补水和避开暴晒会更舒服。".to_string());
    } else if card.apparent_temperature_c <= 3.0 {
        advice.push("体感温度偏低，出门多加一层，别让冷风偷走精力。".to_string());
    }
    if let (Some(max), Some(min)) = (card.day_temperature_max_c, card.day_temperature_min_c) {
        if max - min >= 10.0 {
            advice.push("昼夜温差有点大，外套可以按晚间温度准备。".to_string());
        }
    }
    if card.uv_index.unwrap_or_default() >= 6.0 {
        advice.push("紫外线偏强，长时间在户外的话留意防晒。".to_string());
    }
    if card.air_quality_index.unwrap_or_default() >= 101 || card.pm2_5.unwrap_or_default() >= 35.0 {
        advice.push("空气质量对敏感人群不太友好，户外活动可以适当降强度。".to_string());
    }
    if card.humidity_percent.unwrap_or_default() >= 80 && card.apparent_temperature_c >= 28.0 {
        advice.push("湿度偏高，体感会更闷，户外活动节奏可以放慢一点。".to_string());
    }
    if card.wind_speed_kmh.unwrap_or_default() >= 30.0 {
        advice.push("风力有点明显，轻便物品和外套帽子留意一下。".to_string());
    }
    advice.truncate(5);
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
        return format!(
            "天气 provider：{}。仅发送手动城市给地理编码与天气服务；缓存只保留城市级天气，30 分钟过期。",
            weather_provider_label(&settings.weather_provider)
        );
    }
    "粗略定位会通过 IP 定位服务估算城市，只缓存城市级信息 30 分钟；关闭天气或清理缓存会停止/清除位置缓存。".to_string()
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
            uv_index: Some(7.0),
            air_quality_index: Some(80),
            pm2_5: Some(20.0),
            day_temperature_min_c: Some(25.0),
            day_temperature_max_c: Some(36.0),
            commute_precipitation_probability: Some(20),
            severe_weather: false,
            weather_code: 0,
            condition: "晴".to_string(),
            advice: vec!["体感温度偏高，补水和避开暴晒会更舒服。".to_string()],
            source: "Open-Meteo".to_string(),
            cached: false,
            fetched_at: 1,
        };
        let suggestions = build_suggestions(&settings(), Some(&weather), now_millis());
        assert!(suggestions.len() <= 4);
        assert!(suggestions.iter().any(|item| item.kind == "mood"));
        assert!(suggestions.iter().any(|item| item.kind == "weather"));
    }

    #[test]
    fn proactive_candidates_stay_card_scoped_and_low_frequency() {
        let mut settings = settings();
        settings.companion_focus = "available".to_string();
        settings.companion_do_not_disturb = "23:00-08:00".to_string();
        let candidates = proactive_reminder_candidates(
            &settings,
            None,
            now_millis().saturating_sub(5 * 60 * 60 * 1000),
        );
        assert!(candidates.iter().any(|item| item.kind == "reminder_policy"));
        assert!(candidates.iter().any(|item| item.kind == "reminder_time"));
        assert!(candidates
            .iter()
            .all(|item| item.text.contains("卡片") || item.text.contains("通知")));
    }

    #[test]
    fn weather_advice_includes_refined_daily_signals() {
        let card = WeatherCard {
            city: "Hangzhou".to_string(),
            country: "China".to_string(),
            temperature_c: 30.0,
            apparent_temperature_c: 34.0,
            precipitation_mm: 0.0,
            humidity_percent: Some(85),
            wind_speed_kmh: Some(10.0),
            uv_index: Some(8.0),
            air_quality_index: Some(120),
            pm2_5: Some(42.0),
            day_temperature_min_c: Some(18.0),
            day_temperature_max_c: Some(31.0),
            commute_precipitation_probability: Some(70),
            severe_weather: false,
            weather_code: 1,
            condition: "多云".to_string(),
            advice: Vec::new(),
            source: "Open-Meteo".to_string(),
            cached: false,
            fetched_at: 1,
        };
        let advice = weather_advice(&card);
        assert!(advice.iter().any(|item| item.contains("紫外线")));
        assert!(advice.iter().any(|item| item.contains("空气质量")));
        assert!(advice.iter().any(|item| item.contains("通勤")));
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
    fn parses_llm_companion_memory_candidates_for_review() {
        let raw = r#"{"memories":[
          {"scope":"user","kind":"stress","text":"Deadlines are a recurring stress source.","reason":"User described this as recurring."},
          {"scope":"session","kind":"encouragement","text":"Short, practical encouragement helps.","reason":"User preferred low-pressure support."}
        ]}"#;
        let extraction = parse_companion_memory_extraction(raw).unwrap();
        let candidates = normalize_companion_memory_candidates(extraction.memories);
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].scope.as_deref(), Some("user"));
        assert_eq!(candidates[0].kind.as_deref(), Some("stress"));
        assert_eq!(candidates[1].scope.as_deref(), Some("session"));
        assert_eq!(candidates[1].kind.as_deref(), Some("encouragement"));
    }

    #[test]
    fn detects_high_risk_expressions_without_memory_payload() {
        let crisis = detect_high_risk_expression("我真的不想活了").unwrap();
        assert_eq!(crisis.kind, "self_harm_or_crisis");
        assert_eq!(crisis.severity, "high");
        assert!(crisis.support_message.contains("紧急"));

        let boundary = detect_high_risk_expression("你可以替代心理治疗吗").unwrap();
        assert_eq!(boundary.kind, "medical_or_therapy_substitution");
        assert!(boundary.support_message.contains("不能替代"));
    }

    #[test]
    fn privacy_note_does_not_claim_auto_location() {
        let mut settings = Settings::default();
        settings.weather_enabled = true;
        settings.weather_location_mode = "auto".to_string();
        assert!(privacy_note(&settings).contains("粗略定位"));
    }
}
