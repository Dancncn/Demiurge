//! 追加式模型使用量日志。
//!
//! 使用量是报告投影，不参与会话状态机决策。每次模型请求追加一行 JSONL，
//! 汇总时从日志重放，因此掉线、重启和审计都不会依赖内存计数器。

use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::llm::Usage;
use crate::models;
use crate::AppState;

const MAX_USAGE_LINE_BYTES: usize = 256 * 1024;
static NEXT_USAGE_ID: AtomicU64 = AtomicU64::new(1);
static USAGE_WRITE_FAILURES: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: String,
    pub session_id: String,
    pub provider: String,
    pub model: String,
    pub purpose: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    #[serde(default)]
    pub cache_read_tokens: u64,
    #[serde(default)]
    pub cache_creation_tokens: u64,
    #[serde(default)]
    pub cost_usd: Option<f64>,
    #[serde(default)]
    pub pricing_source: Option<String>,
    pub latency_ms: u64,
    pub status: String,
    pub created_at: u64,
}

#[derive(Clone, Debug)]
pub struct UsageRecordInput<'a> {
    pub session_id: &'a str,
    pub provider: &'a str,
    pub model: &'a str,
    pub purpose: &'a str,
    pub usage: Option<Usage>,
    pub fallback_total_tokens: u64,
    pub latency_ms: u64,
    pub status: &'a str,
}

#[derive(Clone, Debug, Serialize)]
pub struct UsageBucket {
    pub key: String,
    pub requests: u64,
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cost_usd: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct UsageSummary {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub interrupted_requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub total_tokens: u64,
    pub cache_hit_rate: f64,
    pub average_latency_ms: u64,
    pub total_cost_usd: f64,
    pub priced_requests: u64,
    pub unpriced_requests: u64,
    pub providers: Vec<UsageBucket>,
    pub models: Vec<UsageBucket>,
    pub daily: Vec<UsageBucket>,
    pub recent_records: Vec<UsageRecord>,
    /// Diagnostics are process-safe counters, not model usage records. They
    /// make a broken JSONL store visible without changing the record format.
    pub log_read_errors: u64,
    pub malformed_lines: u64,
    pub oversized_lines: u64,
    pub write_failures: u64,
}

pub fn record(state: &AppState, input: UsageRecordInput<'_>) -> Result<(), String> {
    let usage = input.usage.unwrap_or_default();
    let input_tokens = usage.input_tokens.unwrap_or_default() as u64;
    let output_tokens = usage.output_tokens.unwrap_or_default() as u64;
    let cache_read_tokens = usage.cache_read_tokens.unwrap_or_default() as u64;
    let cache_creation_tokens = usage.cache_creation_tokens.unwrap_or_default() as u64;
    let total_tokens = usage
        .total_or_sum()
        .map(|value| value as u64)
        .unwrap_or(input.fallback_total_tokens);
    let dir = state.data_dir.lock().unwrap().clone();
    let pricing = models::pricing_for(&dir, input.provider, input.model);
    let cost_usd = pricing.as_ref().and_then(|price| {
        let fresh_input = input_tokens.saturating_sub(cache_read_tokens);
        let input_cost = price
            .input_cost_per_million
            .map(|value| fresh_input as f64 * value / 1_000_000.0)
            .unwrap_or(0.0);
        let output_cost = price
            .output_cost_per_million
            .map(|value| output_tokens as f64 * value / 1_000_000.0)
            .unwrap_or(0.0);
        let cache_read_cost = price
            .cache_read_cost_per_million
            .map(|value| cache_read_tokens as f64 * value / 1_000_000.0)
            .unwrap_or(0.0);
        let cache_creation_cost = price
            .cache_write_cost_per_million
            .map(|value| cache_creation_tokens as f64 * value / 1_000_000.0)
            .unwrap_or(0.0);
        if price.input_cost_per_million.is_some()
            || price.output_cost_per_million.is_some()
            || price.cache_read_cost_per_million.is_some()
            || price.cache_write_cost_per_million.is_some()
        {
            Some(input_cost + output_cost + cache_read_cost + cache_creation_cost)
        } else {
            None
        }
    });
    let record = UsageRecord {
        id: format!(
            "usage-{}-{}-{}",
            now_millis(),
            std::process::id(),
            NEXT_USAGE_ID.fetch_add(1, Ordering::Relaxed)
        ),
        session_id: input.session_id.to_string(),
        provider: input.provider.to_string(),
        model: input.model.to_string(),
        purpose: input.purpose.to_string(),
        input_tokens,
        output_tokens,
        total_tokens,
        cache_read_tokens,
        cache_creation_tokens,
        cost_usd,
        pricing_source: pricing.map(|value| value.source),
        latency_ms: input.latency_ms,
        status: input.status.to_string(),
        created_at: now_millis(),
    };
    let line = serde_json::to_string(&record)
        .map_err(|e| usage_write_error(format!("序列化用量日志失败：{e}")))?;
    let _guard = state.usage_log_lock.lock().unwrap();
    fs::create_dir_all(&dir)
        .map_err(|e| usage_write_error(format!("创建用量日志目录失败：{e}")))?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("usage.jsonl"))
        .map_err(|e| usage_write_error(format!("打开用量日志失败：{e}")))?;
    writeln!(file, "{line}").map_err(|e| usage_write_error(format!("追加用量日志失败：{e}")))?;
    file.flush()
        .map_err(|e| usage_write_error(format!("刷新用量日志失败：{e}")))
}

pub fn summary(state: &AppState, start_at: Option<u64>, end_at: Option<u64>) -> UsageSummary {
    let dir = state.data_dir.lock().unwrap().clone();
    let _guard = state.usage_log_lock.lock().unwrap();
    summary_from_path(&dir.join("usage.jsonl"), start_at, end_at)
}

fn summary_from_path(path: &Path, start_at: Option<u64>, end_at: Option<u64>) -> UsageSummary {
    let (records, diagnostics) = read_records(path);
    let filtered_records = records
        .into_iter()
        .filter(|record| {
            start_at.is_none_or(|start| record.created_at >= start)
                && end_at.is_none_or(|end| record.created_at <= end)
        })
        .collect::<Vec<_>>();
    let mut recent_records = filtered_records.clone();
    recent_records.sort_by_key(|record| Reverse(record.created_at));
    recent_records.truncate(200);
    let mut summary = UsageSummary {
        total_requests: 0,
        successful_requests: 0,
        failed_requests: 0,
        interrupted_requests: 0,
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
        total_tokens: 0,
        cache_hit_rate: 0.0,
        average_latency_ms: 0,
        total_cost_usd: 0.0,
        priced_requests: 0,
        unpriced_requests: 0,
        providers: Vec::new(),
        models: Vec::new(),
        daily: Vec::new(),
        recent_records,
        log_read_errors: diagnostics.read_errors,
        malformed_lines: diagnostics.malformed_lines,
        oversized_lines: diagnostics.oversized_lines,
        write_failures: USAGE_WRITE_FAILURES.load(Ordering::Relaxed),
    };
    let mut provider_buckets = BTreeMap::<String, UsageBucket>::new();
    let mut model_buckets = BTreeMap::<String, UsageBucket>::new();
    let mut daily_buckets = BTreeMap::<String, UsageBucket>::new();
    let mut latency_total = 0u64;
    for record in filtered_records {
        summary.total_requests += 1;
        if record.status == "success" {
            summary.successful_requests += 1;
        } else if record.status == "interrupted" {
            summary.interrupted_requests += 1;
        } else {
            summary.failed_requests += 1;
        }
        summary.input_tokens = summary.input_tokens.saturating_add(record.input_tokens);
        summary.output_tokens = summary.output_tokens.saturating_add(record.output_tokens);
        summary.cache_read_tokens = summary
            .cache_read_tokens
            .saturating_add(record.cache_read_tokens);
        summary.cache_creation_tokens = summary
            .cache_creation_tokens
            .saturating_add(record.cache_creation_tokens);
        summary.total_tokens = summary.total_tokens.saturating_add(record.total_tokens);
        if let Some(cost) = record.cost_usd {
            summary.total_cost_usd += cost;
            summary.priced_requests += 1;
        } else {
            summary.unpriced_requests += 1;
        }
        latency_total = latency_total.saturating_add(record.latency_ms);
        add_bucket(&mut provider_buckets, record.provider.clone(), &record);
        add_bucket(&mut model_buckets, record.model.clone(), &record);
        add_bucket(&mut daily_buckets, day_key(record.created_at), &record);
    }
    summary.average_latency_ms = latency_total
        .checked_div(summary.total_requests)
        .unwrap_or_default();
    if summary.input_tokens > 0 {
        summary.cache_hit_rate = summary.cache_read_tokens as f64 / summary.input_tokens as f64;
    }
    summary.providers = provider_buckets.into_values().collect();
    summary.models = model_buckets.into_values().collect();
    summary.daily = daily_buckets.into_values().collect();
    summary
}

fn add_bucket(buckets: &mut BTreeMap<String, UsageBucket>, key: String, record: &UsageRecord) {
    let bucket = buckets.entry(key.clone()).or_insert_with(|| UsageBucket {
        key,
        requests: 0,
        total_tokens: 0,
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
        cost_usd: 0.0,
    });
    bucket.requests += 1;
    bucket.total_tokens = bucket.total_tokens.saturating_add(record.total_tokens);
    bucket.input_tokens = bucket.input_tokens.saturating_add(record.input_tokens);
    bucket.output_tokens = bucket.output_tokens.saturating_add(record.output_tokens);
    bucket.cache_read_tokens = bucket
        .cache_read_tokens
        .saturating_add(record.cache_read_tokens);
    bucket.cache_creation_tokens = bucket
        .cache_creation_tokens
        .saturating_add(record.cache_creation_tokens);
    bucket.cost_usd += record.cost_usd.unwrap_or(0.0);
}

#[derive(Default)]
struct UsageLogDiagnostics {
    read_errors: u64,
    malformed_lines: u64,
    oversized_lines: u64,
}

fn usage_write_error(message: String) -> String {
    USAGE_WRITE_FAILURES.fetch_add(1, Ordering::Relaxed);
    message
}

fn read_records(path: &Path) -> (Vec<UsageRecord>, UsageLogDiagnostics) {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return (Vec::new(), UsageLogDiagnostics::default());
        }
        Err(_) => {
            return (
                Vec::new(),
                UsageLogDiagnostics {
                    read_errors: 1,
                    ..UsageLogDiagnostics::default()
                },
            );
        }
    };
    let mut records = Vec::new();
    let mut diagnostics = UsageLogDiagnostics::default();
    for line in BufReader::new(file).lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => {
                diagnostics.read_errors = diagnostics.read_errors.saturating_add(1);
                continue;
            }
        };
        if line.len() > MAX_USAGE_LINE_BYTES {
            diagnostics.oversized_lines = diagnostics.oversized_lines.saturating_add(1);
            continue;
        }
        match serde_json::from_str::<UsageRecord>(&line) {
            Ok(record) => records.push(record),
            Err(_) => {
                diagnostics.malformed_lines = diagnostics.malformed_lines.saturating_add(1);
            }
        }
    }
    (records, diagnostics)
}

fn day_key(timestamp: u64) -> String {
    // 用 UTC 日期做稳定聚合；UI 可以按本地时区显示，原始 timestamp 保留在日志中。
    let days = timestamp / 86_400_000;
    format!("day-{days}")
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
    fn usage_summary_replays_append_only_records() {
        let root = std::env::temp_dir().join(format!("demiurge_usage_{}", now_millis()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("usage.jsonl");
        let records = [
            UsageRecord {
                id: "a".into(),
                session_id: "s".into(),
                provider: "deepseek".into(),
                model: "chat".into(),
                purpose: "agent_turn".into(),
                input_tokens: 10,
                output_tokens: 5,
                total_tokens: 15,
                cache_read_tokens: 4,
                cache_creation_tokens: 0,
                cost_usd: None,
                pricing_source: None,
                latency_ms: 20,
                status: "success".into(),
                created_at: 86_400_000,
            },
            UsageRecord {
                id: "b".into(),
                session_id: "s".into(),
                provider: "deepseek".into(),
                model: "chat".into(),
                purpose: "agent_turn".into(),
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                cost_usd: None,
                pricing_source: None,
                latency_ms: 10,
                status: "failed".into(),
                created_at: 86_400_001,
            },
        ];
        fs::write(
            &path,
            records
                .iter()
                .map(|v| serde_json::to_string(v).unwrap() + "\n")
                .collect::<String>(),
        )
        .unwrap();
        let summary = summary_from_path(&path, None, None);
        assert_eq!(summary.total_requests, 2);
        assert_eq!(summary.successful_requests, 1);
        assert_eq!(summary.total_tokens, 15);
        assert_eq!(summary.cache_read_tokens, 4);
        assert!((summary.cache_hit_rate - 0.4).abs() < f64::EPSILON);
        assert_eq!(summary.recent_records[0].id, "b");
        assert_eq!(summary.providers[0].requests, 2);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn usage_summary_reports_corrupt_and_oversized_lines() {
        let root =
            std::env::temp_dir().join(format!("demiurge_usage_diagnostics_{}", now_millis()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("usage.jsonl");
        fs::write(
            &path,
            format!("not-json\n{}\n", "x".repeat(MAX_USAGE_LINE_BYTES + 1)),
        )
        .unwrap();

        let summary = summary_from_path(&path, None, None);
        assert_eq!(summary.total_requests, 0);
        assert_eq!(summary.malformed_lines, 1);
        assert_eq!(summary.oversized_lines, 1);
        assert_eq!(summary.log_read_errors, 0);
        let _ = fs::remove_dir_all(root);
    }
}
