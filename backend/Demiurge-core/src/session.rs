use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use demiurge_common::conversation::Message;
use serde::{Deserialize, Serialize};

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

static SEQ: AtomicU64 = AtomicU64::new(1);

/// 生成全局唯一会话 id（时间戳 + 自增序号，避免同一毫秒碰撞）。
pub fn new_session_id() -> String {
    format!("s_{}_{}", now_millis(), SEQ.fetch_add(1, Ordering::Relaxed))
}

/// 一段会话。
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Session {
    pub id: String,
    pub title: String,
    /// 与该多轮会话绑定的项目根目录。旧版 sessions.json 没有此字段时自动为空，
    /// 启动/选中会话时再安全迁移到默认 sandbox。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub workspace_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<crate::goal::GoalState>,
    pub messages: Vec<Message>,
    pub updated_at: u64,
}

impl Session {
    pub fn new() -> Self {
        Session {
            id: new_session_id(),
            title: "新对话".to_string(),
            workspace_path: String::new(),
            summary: None,
            goal: None,
            messages: Vec::new(),
            updated_at: now_millis(),
        }
    }
}

/// 会话集合 + 当前活动会话 id。
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct SessionStore {
    pub active: String,
    pub sessions: Vec<Session>,
}

impl SessionStore {
    /// 保证至少有一个会话且 active 指向有效会话。
    pub fn ensure_one(&mut self) {
        if self.sessions.is_empty() {
            let s = Session::new();
            self.active = s.id.clone();
            self.sessions.push(s);
        }
        if !self.sessions.iter().any(|s| s.id == self.active) {
            self.active = self.sessions[0].id.clone();
        }
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Session> {
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    pub fn get(&self, id: &str) -> Option<&Session> {
        self.sessions.iter().find(|s| s.id == id)
    }
}

/// 用首条用户消息生成标题（截断）。
pub fn derive_title(messages: &[Message]) -> String {
    let first = messages
        .iter()
        .find(|m| m.role == "user")
        .and_then(|m| m.content.as_deref())
        .unwrap_or("")
        .trim();
    if first.is_empty() {
        return "新对话".to_string();
    }
    let t: String = first.chars().take(24).collect();
    if first.chars().count() > 24 {
        format!("{t}…")
    } else {
        t
    }
}

// ---------------- Session stats (dashboard) ----------------

const HEATMAP_DAYS: i64 = 126; // 18 weeks

#[derive(Serialize, Clone)]
pub struct DayCell {
    pub date: String,
    pub count: u32,
    pub level: u8,
}

#[derive(Serialize, Clone)]
pub struct StatsPanel {
    pub sessions: usize,
    pub messages: usize,
    pub est_tokens: u64,
    pub active_days: usize,
    pub current_streak: usize,
    pub longest_streak: usize,
    pub peak_hour: Option<u32>,
    pub model: String,
    pub heatmap_days: usize,
    pub heatmap: Vec<DayCell>,
}

fn level_for(count: u32) -> u8 {
    match count {
        0 => 0,
        1 => 1,
        2 => 2,
        3 | 4 => 3,
        _ => 4,
    }
}

/// Howard Hinnant civil_from_days: days since 1970-01-01 -> (year, month, day).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Aggregate dashboard stats from all sessions. `offset` is the client timezone
/// offset in minutes (JS `Date.getTimezoneOffset()`), used to bucket by local day/hour.
pub fn compute_stats(store: &SessionStore, offset: i64, model: String) -> StatsPanel {
    let offset_ms = offset * 60_000;
    let now_local = now_millis() as i64 - offset_ms;
    let today = now_local.div_euclid(86_400_000);

    let mut messages = 0usize;
    let mut est_tokens = 0u64;
    let mut day_counts: std::collections::HashMap<i64, u32> = std::collections::HashMap::new();
    let mut hour_counts = [0u32; 24];

    for s in &store.sessions {
        for m in &s.messages {
            if m.role == "user" || m.role == "assistant" {
                messages += 1;
            }
            if let Some(c) = &m.content {
                est_tokens += (c.chars().count() as u64) / 4;
            }
            if let Some(tcs) = &m.tool_calls {
                for tc in tcs {
                    est_tokens += (tc.function.arguments.chars().count() as u64) / 4;
                }
            }
        }
        let local = s.updated_at as i64 - offset_ms;
        let day = local.div_euclid(86_400_000);
        *day_counts.entry(day).or_insert(0) += 1;
        let hour = (local.rem_euclid(86_400_000) / 3_600_000) as usize;
        if hour < 24 {
            hour_counts[hour] += 1;
        }
    }

    let active_days = day_counts.len();

    // current streak: consecutive active days ending exactly at today
    let mut current_streak = 0usize;
    let mut d = today;
    while day_counts.contains_key(&d) {
        current_streak += 1;
        d -= 1;
    }

    // longest streak: longest run of consecutive day indices
    let mut days: Vec<i64> = day_counts.keys().copied().collect();
    days.sort_unstable();
    let mut longest_streak = 0usize;
    let mut run = 0usize;
    let mut prev: Option<i64> = None;
    for &day in &days {
        run = if prev == Some(day - 1) { run + 1 } else { 1 };
        if run > longest_streak {
            longest_streak = run;
        }
        prev = Some(day);
    }

    let peak_hour = if hour_counts.iter().all(|&c| c == 0) {
        None
    } else {
        hour_counts
            .iter()
            .enumerate()
            .max_by_key(|(_, &c)| c)
            .map(|(h, _)| h as u32)
    };

    let mut heatmap = Vec::with_capacity(HEATMAP_DAYS as usize);
    for i in (0..HEATMAP_DAYS).rev() {
        let day = today - i;
        let count = day_counts.get(&day).copied().unwrap_or(0);
        let (y, mo, dd) = civil_from_days(day);
        heatmap.push(DayCell {
            date: format!("{y:04}-{mo:02}-{dd:02}"),
            count,
            level: level_for(count),
        });
    }

    StatsPanel {
        sessions: store.sessions.len(),
        messages,
        est_tokens,
        active_days,
        current_streak,
        longest_streak,
        peak_hour,
        model,
        heatmap_days: HEATMAP_DAYS as usize,
        heatmap,
    }
}

#[cfg(test)]
mod tests {
    use demiurge_common::conversation::Message;

    use super::{derive_title, Session, SessionStore};

    #[test]
    fn legacy_session_without_workspace_path_remains_compatible() {
        let session = serde_json::from_str::<Session>(
            r#"{
                "id": "legacy-session",
                "title": "旧会话",
                "messages": [],
                "updated_at": 123
            }"#,
        )
        .unwrap();

        assert!(session.workspace_path.is_empty());
        assert!(serde_json::to_value(&session)
            .unwrap()
            .get("workspace_path")
            .is_none());
    }

    #[test]
    fn session_store_keeps_one_valid_active_session() {
        let mut store = SessionStore::default();
        store.ensure_one();
        assert_eq!(store.sessions.len(), 1);
        assert_eq!(store.active, store.sessions[0].id);
    }

    #[test]
    fn title_comes_from_the_first_user_message() {
        let messages = vec![
            Message::assistant_text("ignored"),
            Message::user("abcdefghijklmnopqrstuvwxyz"),
        ];
        assert_eq!(derive_title(&messages), "abcdefghijklmnopqrstuvwx…");
    }
}
