//! Tests for Metrics Module

use firebox_service::middleware::metrics::{
    MetricsCollector, MetricsSnapshot, ProviderMetrics, RequestTimer,
};
use std::time::Duration;

// MetricsSnapshot tests
#[test]
fn metrics_snapshot_default() {
    let snapshot = MetricsSnapshot::default();
    assert_eq!(snapshot.window_start_ms, 0);
    assert_eq!(snapshot.window_end_ms, 0);
    assert_eq!(snapshot.requests_total, 0);
    assert_eq!(snapshot.requests_failed, 0);
    assert_eq!(snapshot.prompt_tokens_total, 0);
    assert_eq!(snapshot.completion_tokens_total, 0);
    assert_eq!(snapshot.latency_avg_ms, 0);
    assert!((snapshot.cost_total - 0.0).abs() < 0.001);
}

#[test]
fn metrics_snapshot_with_values() {
    let snapshot = MetricsSnapshot {
        window_start_ms: 1000,
        window_end_ms: 2000,
        requests_total: 100,
        requests_failed: 5,
        prompt_tokens_total: 10000,
        completion_tokens_total: 5000,
        latency_avg_ms: 250,
        cost_total: 1.50,
    };

    assert_eq!(snapshot.window_start_ms, 1000);
    assert_eq!(snapshot.requests_total, 100);
    assert_eq!(snapshot.requests_failed, 5);
    assert_eq!(snapshot.prompt_tokens_total, 10000);
    assert_eq!(snapshot.completion_tokens_total, 5000);
    assert_eq!(snapshot.latency_avg_ms, 250);
    assert!((snapshot.cost_total - 1.50).abs() < 0.001);
}

#[test]
fn metrics_snapshot_clone() {
    let snapshot = MetricsSnapshot {
        window_start_ms: 1000,
        window_end_ms: 2000,
        requests_total: 50,
        requests_failed: 2,
        prompt_tokens_total: 5000,
        completion_tokens_total: 2500,
        latency_avg_ms: 200,
        cost_total: 0.75,
    };

    let cloned = snapshot.clone();
    assert_eq!(snapshot.requests_total, cloned.requests_total);
}

#[test]
fn metrics_snapshot_debug() {
    let snapshot = MetricsSnapshot::default();
    let debug_str = format!("{:?}", snapshot);
    assert!(debug_str.contains("MetricsSnapshot"));
}

#[test]
fn metrics_snapshot_success_rate() {
    let snapshot = MetricsSnapshot {
        window_start_ms: 0,
        window_end_ms: 1000,
        requests_total: 100,
        requests_failed: 10,
        prompt_tokens_total: 0,
        completion_tokens_total: 0,
        latency_avg_ms: 0,
        cost_total: 0.0,
    };

    let success_count = snapshot.requests_total - snapshot.requests_failed;
    let success_rate = success_count as f64 / snapshot.requests_total as f64;
    assert!((success_rate - 0.9).abs() < 0.001);
}

#[test]
fn metrics_snapshot_all_failed() {
    let snapshot = MetricsSnapshot {
        window_start_ms: 0,
        window_end_ms: 1000,
        requests_total: 50,
        requests_failed: 50,
        prompt_tokens_total: 0,
        completion_tokens_total: 0,
        latency_avg_ms: 100,
        cost_total: 0.0,
    };

    assert_eq!(snapshot.requests_failed, snapshot.requests_total);
}

#[test]
fn metrics_snapshot_zero_tokens() {
    let snapshot = MetricsSnapshot {
        window_start_ms: 0,
        window_end_ms: 1000,
        requests_total: 10,
        requests_failed: 0,
        prompt_tokens_total: 0,
        completion_tokens_total: 0,
        latency_avg_ms: 50,
        cost_total: 0.0,
    };

    assert_eq!(snapshot.prompt_tokens_total, 0);
    assert_eq!(snapshot.completion_tokens_total, 0);
}

// ProviderMetrics tests
#[test]
fn provider_metrics_default() {
    let metrics = ProviderMetrics::default();
    assert_eq!(metrics.provider_id, "");
    assert!(metrics.model_id.is_none());
    assert_eq!(metrics.requests_total, 0);
    assert_eq!(metrics.requests_failed, 0);
}

#[test]
fn provider_metrics_with_provider() {
    let metrics = ProviderMetrics {
        provider_id: "openai".to_string(),
        model_id: None,
        requests_total: 100,
        requests_failed: 5,
        prompt_tokens_total: 10000,
        completion_tokens_total: 5000,
        cost_total: 1.50,
    };

    assert_eq!(metrics.provider_id, "openai");
    assert!(metrics.model_id.is_none());
}

#[test]
fn provider_metrics_with_model() {
    let metrics = ProviderMetrics {
        provider_id: "openai".to_string(),
        model_id: Some("gpt-4".to_string()),
        requests_total: 50,
        requests_failed: 2,
        prompt_tokens_total: 5000,
        completion_tokens_total: 2500,
        cost_total: 0.75,
    };

    assert_eq!(metrics.model_id, Some("gpt-4".to_string()));
}

#[test]
fn provider_metrics_clone() {
    let metrics = ProviderMetrics {
        provider_id: "anthropic".to_string(),
        model_id: Some("claude-3".to_string()),
        requests_total: 30,
        requests_failed: 1,
        prompt_tokens_total: 3000,
        completion_tokens_total: 1500,
        cost_total: 0.50,
    };

    let cloned = metrics.clone();
    assert_eq!(metrics.provider_id, cloned.provider_id);
    assert_eq!(metrics.model_id, cloned.model_id);
}

#[test]
fn provider_metrics_debug() {
    let metrics = ProviderMetrics {
        provider_id: "dashscope".to_string(),
        model_id: None,
        requests_total: 0,
        requests_failed: 0,
        prompt_tokens_total: 0,
        completion_tokens_total: 0,
        cost_total: 0.0,
    };

    let debug_str = format!("{:?}", metrics);
    assert!(debug_str.contains("dashscope"));
}

#[test]
fn provider_metrics_multiple_providers() {
    let providers = vec!["openai", "anthropic", "copilot", "dashscope", "llamacpp"];

    for provider in providers {
        let metrics = ProviderMetrics {
            provider_id: provider.to_string(),
            model_id: None,
            requests_total: 10,
            requests_failed: 0,
            prompt_tokens_total: 1000,
            completion_tokens_total: 500,
            cost_total: 0.10,
        };
        assert_eq!(metrics.provider_id, provider);
    }
}

// MetricsCollector tests
#[test]
fn metrics_collector_new() {
    let _collector = MetricsCollector::new();
    // Just verify it can be created
}

#[test]
fn metrics_collector_record_success() {
    let collector = MetricsCollector::new();

    collector.record_success(
        100, // prompt tokens
        50,  // completion tokens
        Duration::from_millis(200),
        0.05, // cost in cents
    );

    let snapshot = collector.snapshot(0, 1000);
    assert_eq!(snapshot.requests_total, 1);
    assert_eq!(snapshot.requests_failed, 0);
    assert_eq!(snapshot.prompt_tokens_total, 100);
    assert_eq!(snapshot.completion_tokens_total, 50);
}

#[test]
fn metrics_collector_record_multiple_success() {
    let collector = MetricsCollector::new();

    collector.record_success(100, 50, Duration::from_millis(200), 0.05);
    collector.record_success(200, 100, Duration::from_millis(300), 0.10);
    collector.record_success(150, 75, Duration::from_millis(250), 0.075);

    let snapshot = collector.snapshot(0, 1000);
    assert_eq!(snapshot.requests_total, 3);
    assert_eq!(snapshot.requests_failed, 0);
    assert_eq!(snapshot.prompt_tokens_total, 450);
    assert_eq!(snapshot.completion_tokens_total, 225);
}

#[test]
fn metrics_collector_record_failure() {
    let collector = MetricsCollector::new();

    collector.record_failure(Duration::from_millis(100));

    let snapshot = collector.snapshot(0, 1000);
    assert_eq!(snapshot.requests_total, 1);
    assert_eq!(snapshot.requests_failed, 1);
}

#[test]
fn metrics_collector_mixed_success_failure() {
    let collector = MetricsCollector::new();

    collector.record_success(100, 50, Duration::from_millis(200), 0.05);
    collector.record_failure(Duration::from_millis(100));
    collector.record_success(200, 100, Duration::from_millis(300), 0.10);
    collector.record_failure(Duration::from_millis(150));
    collector.record_failure(Duration::from_millis(50));

    let snapshot = collector.snapshot(0, 1000);
    assert_eq!(snapshot.requests_total, 5);
    assert_eq!(snapshot.requests_failed, 3);
    assert_eq!(snapshot.prompt_tokens_total, 300);
    assert_eq!(snapshot.completion_tokens_total, 150);
}

#[test]
fn metrics_collector_latency_average() {
    let collector = MetricsCollector::new();

    // Record requests with different latencies
    collector.record_success(100, 50, Duration::from_millis(100), 0.01);
    collector.record_success(100, 50, Duration::from_millis(200), 0.01);
    collector.record_success(100, 50, Duration::from_millis(300), 0.01);

    let snapshot = collector.snapshot(0, 1000);
    // Average should be (100 + 200 + 300) / 3 = 200ms
    assert_eq!(snapshot.latency_avg_ms, 200);
}

#[test]
fn metrics_collector_cost_accumulation() {
    let collector = MetricsCollector::new();

    collector.record_success(100, 50, Duration::from_millis(100), 0.05);
    collector.record_success(100, 50, Duration::from_millis(100), 0.10);
    collector.record_success(100, 50, Duration::from_millis(100), 0.15);

    let snapshot = collector.snapshot(0, 1000);
    // Total cost should be 0.05 + 0.10 + 0.15 = 0.30
    assert!((snapshot.cost_total - 0.30).abs() < 0.01);
}

#[test]
fn metrics_collector_record_with_breakdown() {
    let collector = MetricsCollector::new();

    collector.record_success_with_breakdown(
        "openai",
        Some("gpt-4"),
        100,
        50,
        Duration::from_millis(200),
        0.05,
    );

    let snapshot = collector.snapshot(0, 1000);
    assert_eq!(snapshot.requests_total, 1);

    let provider_metrics = collector.get_provider_metrics();
    assert!(!provider_metrics.is_empty());
}

#[test]
fn metrics_collector_record_failure_with_breakdown() {
    let collector = MetricsCollector::new();

    collector.record_failure_with_breakdown(
        "anthropic",
        Some("claude-3"),
        Duration::from_millis(100),
    );

    let snapshot = collector.snapshot(0, 1000);
    assert_eq!(snapshot.requests_failed, 1);

    let provider_metrics = collector.get_provider_metrics();
    let anthropic_metrics = provider_metrics
        .iter()
        .find(|m| m.provider_id == "anthropic")
        .unwrap();
    assert_eq!(anthropic_metrics.requests_failed, 1);
}

#[test]
fn metrics_collector_get_provider_metrics() {
    let collector = MetricsCollector::new();

    collector.record_success_with_breakdown(
        "openai",
        Some("gpt-4"),
        100,
        50,
        Duration::from_millis(200),
        0.05,
    );

    collector.record_success_with_breakdown(
        "anthropic",
        Some("claude-3"),
        150,
        75,
        Duration::from_millis(250),
        0.075,
    );

    let provider_metrics = collector.get_provider_metrics();
    assert_eq!(provider_metrics.len(), 2);
}

#[test]
fn metrics_collector_reset() {
    let collector = MetricsCollector::new();

    // Record some data
    collector.record_success(100, 50, Duration::from_millis(200), 0.05);
    collector.record_failure(Duration::from_millis(100));

    // Reset
    collector.reset();

    // Verify all zeros
    let snapshot = collector.snapshot(0, 1000);
    assert_eq!(snapshot.requests_total, 0);
    assert_eq!(snapshot.requests_failed, 0);
    assert_eq!(snapshot.prompt_tokens_total, 0);
}

#[test]
fn metrics_collector_snapshot_window() {
    let collector = MetricsCollector::new();

    collector.record_success(100, 50, Duration::from_millis(200), 0.05);

    let snapshot = collector.snapshot(1000, 2000);
    assert_eq!(snapshot.window_start_ms, 1000);
    assert_eq!(snapshot.window_end_ms, 2000);
}

#[test]
fn metrics_collector_no_requests_latency() {
    let collector = MetricsCollector::new();

    let snapshot = collector.snapshot(0, 1000);
    assert_eq!(snapshot.latency_avg_ms, 0);
}

// RequestTimer tests
#[test]
fn request_timer_success() {
    let collector = MetricsCollector::new();

    {
        let timer = RequestTimer::new(&collector);
        timer.success(100, 50, 0.05);
    }

    let snapshot = collector.snapshot(0, 1000);
    assert_eq!(snapshot.requests_total, 1);
    assert_eq!(snapshot.requests_failed, 0);
}

#[test]
fn request_timer_failure() {
    let collector = MetricsCollector::new();

    {
        let timer = RequestTimer::new(&collector);
        timer.failure();
    }

    let snapshot = collector.snapshot(0, 1000);
    assert_eq!(snapshot.requests_total, 1);
    assert_eq!(snapshot.requests_failed, 1);
}

#[test]
fn request_timer_drop_without_recording() {
    let collector = MetricsCollector::new();

    {
        let _timer = RequestTimer::new(&collector);
        // Don't call success or failure - should record as failure on drop
    }

    let snapshot = collector.snapshot(0, 1000);
    assert_eq!(snapshot.requests_failed, 1);
}

#[test]
fn request_timer_measures_latency() {
    let collector = MetricsCollector::new();

    {
        let timer = RequestTimer::new(&collector);
        std::thread::sleep(Duration::from_millis(50));
        timer.success(100, 50, 0.05);
    }

    let snapshot = collector.snapshot(0, 1000);
    // Latency should be at least 50ms
    assert!(snapshot.latency_avg_ms >= 50);
}

// Edge cases
#[test]
fn metrics_collector_zero_tokens() {
    let collector = MetricsCollector::new();

    collector.record_success(0, 0, Duration::from_millis(100), 0.0);

    let snapshot = collector.snapshot(0, 1000);
    assert_eq!(snapshot.prompt_tokens_total, 0);
    assert_eq!(snapshot.completion_tokens_total, 0);
}

#[test]
fn metrics_collector_very_large_values() {
    let collector = MetricsCollector::new();

    collector.record_success(1_000_000, 500_000, Duration::from_millis(1000), 100.0);

    let snapshot = collector.snapshot(0, 1000);
    assert_eq!(snapshot.prompt_tokens_total, 1_000_000);
    assert_eq!(snapshot.completion_tokens_total, 500_000);
}

#[test]
fn metrics_collector_very_fast_request() {
    let collector = MetricsCollector::new();

    collector.record_success(100, 50, Duration::from_nanos(100), 0.001);

    let snapshot = collector.snapshot(0, 1000);
    // Very fast requests may have 0ms latency
    assert!(snapshot.latency_avg_ms == 0);
}

#[test]
fn metrics_collector_very_slow_request() {
    let collector = MetricsCollector::new();

    collector.record_success(100, 50, Duration::from_secs(60), 1.0);

    let snapshot = collector.snapshot(0, 1000);
    // 60 seconds = 60000ms
    assert!(snapshot.latency_avg_ms >= 60000);
}

#[test]
fn metrics_snapshot_high_failure_rate() {
    let snapshot = MetricsSnapshot {
        window_start_ms: 0,
        window_end_ms: 1000,
        requests_total: 1000,
        requests_failed: 999,
        prompt_tokens_total: 100,
        completion_tokens_total: 50,
        latency_avg_ms: 100,
        cost_total: 0.01,
    };

    let failure_rate = snapshot.requests_failed as f64 / snapshot.requests_total as f64;
    assert!(failure_rate > 0.99);
}

#[test]
fn provider_metrics_without_model() {
    let metrics = ProviderMetrics {
        provider_id: "llamacpp".to_string(),
        model_id: None,
        requests_total: 10,
        requests_failed: 0,
        prompt_tokens_total: 1000,
        completion_tokens_total: 500,
        cost_total: 0.0, // Local models are free
    };

    assert!(metrics.model_id.is_none());
    assert!((metrics.cost_total - 0.0).abs() < 0.001);
}

#[test]
fn metrics_collector_many_providers() {
    let collector = MetricsCollector::new();

    for i in 0..100 {
        collector.record_success_with_breakdown(
            &format!("provider-{}", i),
            Some(&format!("model-{}", i)),
            100,
            50,
            Duration::from_millis(100),
            0.01,
        );
    }

    let provider_metrics = collector.get_provider_metrics();
    assert_eq!(provider_metrics.len(), 100);
}

#[test]
fn request_timer_repeated_use() {
    let collector = MetricsCollector::new();

    for _ in 0..10 {
        let timer = RequestTimer::new(&collector);
        timer.success(100, 50, 0.01);
    }

    let snapshot = collector.snapshot(0, 1000);
    assert_eq!(snapshot.requests_total, 10);
    assert_eq!(snapshot.requests_failed, 0);
}
