//! Request/response metrics collection.
//!
//! Tracks:
//! - Request counts (total, success, failed) per provider/model
//! - Token usage (prompt, completion, total) per provider/model
//! - Latency
//! - Cost estimates per provider/model
//! - Per-minute ring buffer for range queries (last 24 h)

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock as AsyncRwLock;

/// Metrics snapshot for a time window.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetricsSnapshot {
    pub window_start_ms: u64,
    pub window_end_ms: u64,
    pub requests_total: u64,
    pub requests_failed: u64,
    pub prompt_tokens_total: u64,
    pub completion_tokens_total: u64,
    pub latency_avg_ms: u64,
    pub cost_total: f64,
}

/// Metrics breakdown by provider and model.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderMetrics {
    pub provider_id: String,
    pub model_id: Option<String>,
    pub requests_total: u64,
    pub requests_failed: u64,
    pub prompt_tokens_total: u64,
    pub completion_tokens_total: u64,
    pub cost_total: f64,
}

/// Aggregated metrics collector.
#[derive(Debug, Default)]
pub struct MetricsCollector {
    requests_total: AtomicU64,
    requests_success: AtomicU64,
    requests_failed: AtomicU64,
    prompt_tokens: AtomicU64,
    completion_tokens: AtomicU64,
    latency_sum_ms: AtomicU64,
    latency_count: AtomicU64,
    cost_total_microcents: AtomicU64, // Stored as microcents (millionths of a dollar) for precision

    // Per-provider/model breakdown - use async RwLock to avoid blocking
    provider_metrics: AsyncRwLock<HashMap<String, ProviderMetricsInner>>,
}

#[derive(Debug, Default)]
struct ProviderMetricsInner {
    requests_total: AtomicU64,
    requests_failed: AtomicU64,
    prompt_tokens: AtomicU64,
    completion_tokens: AtomicU64,
    cost_total_microcents: AtomicU64, // Stored as microcents (millionths of a dollar)
}

impl Clone for ProviderMetricsInner {
    fn clone(&self) -> Self {
        Self {
            requests_total: AtomicU64::new(self.requests_total.load(Ordering::Relaxed)),
            requests_failed: AtomicU64::new(self.requests_failed.load(Ordering::Relaxed)),
            prompt_tokens: AtomicU64::new(self.prompt_tokens.load(Ordering::Relaxed)),
            completion_tokens: AtomicU64::new(self.completion_tokens.load(Ordering::Relaxed)),
            cost_total_microcents: AtomicU64::new(
                self.cost_total_microcents.load(Ordering::Relaxed),
            ),
        }
    }
}

impl MetricsCollector {
    /// Create a new metrics collector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a successful request.
    pub fn record_success(
        &self,
        prompt_tokens: u32,
        completion_tokens: u32,
        latency: Duration,
        cost_usd: f64,
    ) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        self.requests_success.fetch_add(1, Ordering::Relaxed);
        self.prompt_tokens
            .fetch_add(prompt_tokens as u64, Ordering::Relaxed);
        self.completion_tokens
            .fetch_add(completion_tokens as u64, Ordering::Relaxed);
        self.latency_sum_ms
            .fetch_add(latency.as_millis() as u64, Ordering::Relaxed);
        self.latency_count.fetch_add(1, Ordering::Relaxed);
        // Store cost as microcents (millionths of a dollar) for 6 decimal places of precision
        let cost_microcents = (cost_usd * 1_000_000.0).round() as u64;
        self.cost_total_microcents
            .fetch_add(cost_microcents, Ordering::Relaxed);
    }

    /// Record a successful request with provider/model breakdown.
    pub async fn record_success_with_breakdown(
        &self,
        provider_id: &str,
        model_id: Option<&str>,
        prompt_tokens: u32,
        completion_tokens: u32,
        latency: Duration,
        cost_usd: f64,
    ) {
        // Record per-provider breakdown first (while holding lock)
        let key = format!("{}:{}", provider_id, model_id.unwrap_or(""));
        let mut lock = self.provider_metrics.write().await;
        let metrics = lock.entry(key).or_default();

        metrics.requests_total.fetch_add(1, Ordering::Relaxed);
        metrics
            .prompt_tokens
            .fetch_add(prompt_tokens as u64, Ordering::Relaxed);
        metrics
            .completion_tokens
            .fetch_add(completion_tokens as u64, Ordering::Relaxed);
        let cost_microcents = (cost_usd * 1_000_000.0).round() as u64;
        metrics
            .cost_total_microcents
            .fetch_add(cost_microcents, Ordering::Relaxed);
        drop(lock); // Release lock before recording global metrics

        // Record global metrics (atomic operations, no lock needed)
        self.record_success(prompt_tokens, completion_tokens, latency, cost_usd);
    }

    /// Record a failed request.
    pub fn record_failure(&self, latency: Duration) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        self.requests_failed.fetch_add(1, Ordering::Relaxed);
        self.latency_sum_ms
            .fetch_add(latency.as_millis() as u64, Ordering::Relaxed);
        self.latency_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a failed request with provider breakdown.
    pub async fn record_failure_with_breakdown(
        &self,
        provider_id: &str,
        model_id: Option<&str>,
        latency: Duration,
    ) {
        // Record per-provider breakdown first (while holding lock)
        let key = format!("{}:{}", provider_id, model_id.unwrap_or(""));
        let mut lock = self.provider_metrics.write().await;
        let metrics = lock.entry(key).or_default();

        metrics.requests_total.fetch_add(1, Ordering::Relaxed);
        metrics.requests_failed.fetch_add(1, Ordering::Relaxed);
        drop(lock); // Release lock before recording global metrics

        // Record global metrics (atomic operations, no lock needed)
        self.record_failure(latency);
    }

    /// Get current metrics snapshot.
    pub fn snapshot(&self, window_start_ms: u64, window_end_ms: u64) -> MetricsSnapshot {
        let requests_total = self.requests_total.load(Ordering::Relaxed);
        let requests_failed = self.requests_failed.load(Ordering::Relaxed);
        let prompt_tokens = self.prompt_tokens.load(Ordering::Relaxed);
        let completion_tokens = self.completion_tokens.load(Ordering::Relaxed);
        let latency_sum = self.latency_sum_ms.load(Ordering::Relaxed);
        let latency_count = self.latency_count.load(Ordering::Relaxed);
        let cost_microcents = self.cost_total_microcents.load(Ordering::Relaxed);

        let latency_avg_ms = if latency_count > 0 {
            latency_sum / latency_count
        } else {
            0
        };

        MetricsSnapshot {
            window_start_ms,
            window_end_ms,
            requests_total,
            requests_failed,
            prompt_tokens_total: prompt_tokens,
            completion_tokens_total: completion_tokens,
            latency_avg_ms,
            cost_total: (cost_microcents as f64) / 1_000_000.0,
        }
    }

    /// Get metrics breakdown by provider.
    pub async fn get_provider_metrics(&self) -> Vec<ProviderMetrics> {
        let lock = self.provider_metrics.read().await;
        let result = lock
            .iter()
            .map(|(key, metrics)| {
                let parts: Vec<&str> = key.split(':').collect();
                let provider_id = parts.first().unwrap_or(&"").to_string();
                let model_id = parts
                    .get(1)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());

                ProviderMetrics {
                    provider_id,
                    model_id,
                    requests_total: metrics.requests_total.load(Ordering::Relaxed),
                    requests_failed: metrics.requests_failed.load(Ordering::Relaxed),
                    prompt_tokens_total: metrics.prompt_tokens.load(Ordering::Relaxed),
                    completion_tokens_total: metrics.completion_tokens.load(Ordering::Relaxed),
                    cost_total: (metrics.cost_total_microcents.load(Ordering::Relaxed) as f64)
                        / 1_000_000.0,
                }
            })
            .collect();
        drop(lock);
        result
    }

    /// Reset all counters.
    pub async fn reset(&self) {
        self.requests_total.store(0, Ordering::Relaxed);
        self.requests_success.store(0, Ordering::Relaxed);
        self.requests_failed.store(0, Ordering::Relaxed);
        self.prompt_tokens.store(0, Ordering::Relaxed);
        self.completion_tokens.store(0, Ordering::Relaxed);
        self.latency_sum_ms.store(0, Ordering::Relaxed);
        self.latency_count.store(0, Ordering::Relaxed);
        self.cost_total_microcents.store(0, Ordering::Relaxed);
        self.provider_metrics.write().await.clear();
    }
}

/// RAII guard for timing requests.
pub struct RequestTimer<'a> {
    start: Instant,
    collector: &'a MetricsCollector,
    recorded: bool,
}

impl<'a> RequestTimer<'a> {
    /// Start a new request timer.
    pub fn new(collector: &'a MetricsCollector) -> Self {
        Self {
            start: Instant::now(),
            collector,
            recorded: false,
        }
    }

    /// Record success and stop timing.
    pub fn success(mut self, prompt_tokens: u32, completion_tokens: u32, cost_cents: f64) {
        let latency = self.start.elapsed();
        self.collector
            .record_success(prompt_tokens, completion_tokens, latency, cost_cents);
        self.recorded = true;
    }

    /// Record failure and stop timing.
    pub fn failure(mut self) {
        let latency = self.start.elapsed();
        self.collector.record_failure(latency);
        self.recorded = true;
    }
}

impl<'a> Drop for RequestTimer<'a> {
    fn drop(&mut self) {
        // If not explicitly recorded, record as failure
        if !self.recorded {
            let latency = self.start.elapsed();
            self.collector.record_failure(latency);
        }
    }
}

// Global metrics collector - use LazyLock for HashMap initialization
static COLLECTOR: LazyLock<MetricsCollector> = LazyLock::new(|| MetricsCollector {
    requests_total: AtomicU64::new(0),
    requests_success: AtomicU64::new(0),
    requests_failed: AtomicU64::new(0),
    prompt_tokens: AtomicU64::new(0),
    completion_tokens: AtomicU64::new(0),
    latency_sum_ms: AtomicU64::new(0),
    latency_count: AtomicU64::new(0),
    cost_total_microcents: AtomicU64::new(0),
    provider_metrics: AsyncRwLock::new(HashMap::new()),
});

// ---------------------------------------------------------------------------
// Per-minute ring buffer for range queries (last 24 h = 1440 buckets)
// ---------------------------------------------------------------------------

const RING_BUFFER_MINUTES: usize = 1440; // 24 hours

/// A single minute-granularity metrics bucket.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MinuteBucket {
    /// Unix timestamp (ms) of the start of this minute.
    pub timestamp_ms: u64,
    pub requests_total: u64,
    pub requests_failed: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cost_total: f64,
}

/// The global per-minute ring buffer.
static RING_BUFFER: LazyLock<AsyncRwLock<VecDeque<MinuteBucket>>> =
    LazyLock::new(|| AsyncRwLock::new(VecDeque::with_capacity(RING_BUFFER_MINUTES)));

/// Record a single request into the current minute's ring-buffer bucket.
pub async fn record_minute_bucket(
    requests: u64,
    failed: u64,
    prompt_tokens: u64,
    completion_tokens: u64,
    cost_total: f64,
) {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    // Round down to the start of the current minute.
    let minute_start = (now_ms / 60_000) * 60_000;

    let mut buf = RING_BUFFER.write().await;

    // Either update the last bucket (same minute) or push a new one.
    if let Some(last) = buf.back_mut()
        && last.timestamp_ms == minute_start
    {
        last.requests_total += requests;
        last.requests_failed += failed;
        last.prompt_tokens += prompt_tokens;
        last.completion_tokens += completion_tokens;
        last.cost_total += cost_total;
        return;
    }

    // New minute: evict oldest entry if at capacity.
    if buf.len() == RING_BUFFER_MINUTES {
        buf.pop_front();
    }
    buf.push_back(MinuteBucket {
        timestamp_ms: minute_start,
        requests_total: requests,
        requests_failed: failed,
        prompt_tokens,
        completion_tokens,
        cost_total,
    });
}

/// Return all minute buckets whose `timestamp_ms` falls within `[start_ms, end_ms]`.
pub async fn get_metrics_range(start_ms: u64, end_ms: u64) -> Vec<MinuteBucket> {
    let buf = RING_BUFFER.read().await;
    buf.iter()
        .filter(|b| b.timestamp_ms >= start_ms && b.timestamp_ms <= end_ms)
        .cloned()
        .collect()
}

/// Get the global metrics collector.
pub fn global_collector() -> &'static MetricsCollector {
    &COLLECTOR
}

/// Start timing a request.
pub fn start_request_timer() -> RequestTimer<'static> {
    RequestTimer::new(global_collector())
}

/// Get current metrics snapshot.
pub fn get_snapshot() -> MetricsSnapshot {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    global_collector().snapshot(0, now.as_millis() as u64)
}

/// Get metrics breakdown by provider from the global collector.
pub async fn get_provider_metrics() -> Vec<ProviderMetrics> {
    global_collector().get_provider_metrics().await
}

/// Reset all counters in the global collector.
pub async fn reset_global_metrics() {
    global_collector().reset().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_collector() {
        let collector = MetricsCollector::new();

        collector.record_success(100, 50, Duration::from_millis(200), 0.05);
        collector.record_success(200, 100, Duration::from_millis(300), 0.10);
        collector.record_failure(Duration::from_millis(50));

        let snapshot = collector.snapshot(0, 1000);

        assert_eq!(snapshot.requests_total, 3);
        assert_eq!(snapshot.requests_failed, 1);
        assert_eq!(snapshot.prompt_tokens_total, 300);
        assert_eq!(snapshot.completion_tokens_total, 150);
        assert!((snapshot.cost_total - 0.15).abs() < 0.01);
    }
}
