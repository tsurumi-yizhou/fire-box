/// Real-time metrics collection for the gateway.
///
/// Tracks token usage, request counts, active connections, latencies,
/// broken down per model, per provider, and per app. All counters use
/// atomic operations for lock-free updates on the hot path.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;

// ─── Atomic counter helper ──────────────────────────────────────────────────

/// A lightweight atomic counter that serializes to a plain u64.
#[derive(Debug, Default)]
pub struct Counter(AtomicU64);

impl Counter {
    pub fn inc(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
    pub fn add(&self, n: u64) {
        self.0.fetch_add(n, Ordering::Relaxed);
    }
    pub fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
    pub fn dec(&self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

// ─── Per-entity metrics ─────────────────────────────────────────────────────

/// Metrics for a single model / provider / app.
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct EntityMetrics {
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub errors: u64,
}

// ─── Mutable interior bucket (behind RwLock) ────────────────────────────────

#[derive(Debug, Default)]
struct EntityBucket {
    requests: Counter,
    input_tokens: Counter,
    output_tokens: Counter,
    errors: Counter,
}

impl EntityBucket {
    fn snapshot(&self) -> EntityMetrics {
        EntityMetrics {
            requests: self.requests.get(),
            input_tokens: self.input_tokens.get(),
            output_tokens: self.output_tokens.get(),
            errors: self.errors.get(),
        }
    }
}

// ─── Top-level Metrics ──────────────────────────────────────────────────────

/// Central metrics collector.  
/// Global counters are updated atomically; per-entity maps are behind a
/// `RwLock` so readers never block writers for long.
#[derive(Debug)]
pub struct Metrics {
    // Global counters
    pub total_requests: Counter,
    pub active_connections: Counter,
    pub total_input_tokens: Counter,
    pub total_output_tokens: Counter,
    pub total_errors: Counter,

    // Per-entity breakdown
    per_model: RwLock<HashMap<String, EntityBucket>>,
    per_provider: RwLock<HashMap<String, EntityBucket>>,
    per_app: RwLock<HashMap<String, EntityBucket>>,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            total_requests: Counter::default(),
            active_connections: Counter::default(),
            total_input_tokens: Counter::default(),
            total_output_tokens: Counter::default(),
            total_errors: Counter::default(),
            per_model: RwLock::new(HashMap::new()),
            per_provider: RwLock::new(HashMap::new()),
            per_app: RwLock::new(HashMap::new()),
        }
    }

    // ── Recording helpers ───────────────────────────────────────────────

    /// Record a new request starting.
    pub async fn record_request(&self, model: &str, provider: &str, app_id: Option<&str>) {
        self.total_requests.inc();

        {
            let mut map = self.per_model.write().await;
            map.entry(model.to_owned()).or_default().requests.inc();
        }
        {
            let mut map = self.per_provider.write().await;
            map.entry(provider.to_owned()).or_default().requests.inc();
        }
        if let Some(app) = app_id {
            let mut map = self.per_app.write().await;
            map.entry(app.to_owned()).or_default().requests.inc();
        }
    }

    /// Record token usage after a response completes.
    pub async fn record_tokens(
        &self,
        model: &str,
        provider: &str,
        app_id: Option<&str>,
        input_tokens: u64,
        output_tokens: u64,
    ) {
        self.total_input_tokens.add(input_tokens);
        self.total_output_tokens.add(output_tokens);

        {
            let mut map = self.per_model.write().await;
            let b = map.entry(model.to_owned()).or_default();
            b.input_tokens.add(input_tokens);
            b.output_tokens.add(output_tokens);
        }
        {
            let mut map = self.per_provider.write().await;
            let b = map.entry(provider.to_owned()).or_default();
            b.input_tokens.add(input_tokens);
            b.output_tokens.add(output_tokens);
        }
        if let Some(app) = app_id {
            let mut map = self.per_app.write().await;
            let b = map.entry(app.to_owned()).or_default();
            b.input_tokens.add(input_tokens);
            b.output_tokens.add(output_tokens);
        }
    }

    /// Record an error.
    pub async fn record_error(&self, model: &str, provider: &str, app_id: Option<&str>) {
        self.total_errors.inc();

        {
            let mut map = self.per_model.write().await;
            map.entry(model.to_owned()).or_default().errors.inc();
        }
        {
            let mut map = self.per_provider.write().await;
            map.entry(provider.to_owned()).or_default().errors.inc();
        }
        if let Some(app) = app_id {
            let mut map = self.per_app.write().await;
            map.entry(app.to_owned()).or_default().errors.inc();
        }
    }

    // ── Snapshot for IPC / GUI ──────────────────────────────────────────

    /// Produce a serialisable snapshot of all metrics.
    pub async fn snapshot(&self) -> MetricsSnapshot {
        let per_model = {
            let map = self.per_model.read().await;
            map.iter().map(|(k, v)| (k.clone(), v.snapshot())).collect()
        };
        let per_provider = {
            let map = self.per_provider.read().await;
            map.iter().map(|(k, v)| (k.clone(), v.snapshot())).collect()
        };
        let per_app = {
            let map = self.per_app.read().await;
            map.iter().map(|(k, v)| (k.clone(), v.snapshot())).collect()
        };

        MetricsSnapshot {
            total_requests: self.total_requests.get(),
            active_connections: self.active_connections.get(),
            total_input_tokens: self.total_input_tokens.get(),
            total_output_tokens: self.total_output_tokens.get(),
            total_errors: self.total_errors.get(),
            per_model,
            per_provider,
            per_app,
        }
    }
}

/// Serialisable metrics snapshot — returned via IPC to the native GUI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub total_requests: u64,
    pub active_connections: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_errors: u64,
    pub per_model: HashMap<String, EntityMetrics>,
    pub per_provider: HashMap<String, EntityMetrics>,
    pub per_app: HashMap<String, EntityMetrics>,
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_record_and_snapshot() {
        let m = Metrics::new();
        m.record_request("gpt-5", "OpenAI", Some("com.test.app"))
            .await;
        m.record_tokens("gpt-5", "OpenAI", Some("com.test.app"), 100, 50)
            .await;
        m.record_error("gpt-5", "OpenAI", None).await;

        let snap = m.snapshot().await;
        assert_eq!(snap.total_requests, 1);
        assert_eq!(snap.total_input_tokens, 100);
        assert_eq!(snap.total_output_tokens, 50);
        assert_eq!(snap.total_errors, 1);
        assert_eq!(snap.per_model["gpt-5"].requests, 1);
        assert_eq!(snap.per_provider["OpenAI"].input_tokens, 100);
        assert_eq!(snap.per_app["com.test.app"].output_tokens, 50);
    }

    #[tokio::test]
    async fn test_active_connections() {
        let m = Metrics::new();
        m.active_connections.inc();
        m.active_connections.inc();
        assert_eq!(m.active_connections.get(), 2);
        m.active_connections.dec();
        assert_eq!(m.active_connections.get(), 1);
    }
}
