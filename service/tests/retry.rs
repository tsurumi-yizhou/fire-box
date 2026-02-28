//! Integration tests for retry logic.

use firebox_service::providers::retry::{RetryConfig, with_retry};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

#[tokio::test]
async fn test_retry_succeeds_immediately() {
    let config = RetryConfig::default();
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    let result = with_retry(&config, || {
        let count = call_count_clone.clone();
        async move {
            count.fetch_add(1, Ordering::SeqCst);
            Ok::<_, anyhow::Error>(42)
        }
    })
    .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_retry_succeeds_after_transient_failures() {
    let config = RetryConfig {
        max_retries: 3,
        initial_backoff: Duration::from_millis(10),
        max_backoff: Duration::from_secs(1),
        multiplier: 2.0,
    };
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    let result = with_retry(&config, || {
        let count = call_count_clone.clone();
        async move {
            let current = count.fetch_add(1, Ordering::SeqCst);
            if current < 2 {
                anyhow::bail!("HTTP 503 Service unavailable")
            } else {
                Ok::<_, anyhow::Error>(42)
            }
        }
    })
    .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
    assert_eq!(call_count.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn test_retry_fails_after_max_attempts() {
    let config = RetryConfig {
        max_retries: 2,
        initial_backoff: Duration::from_millis(10),
        max_backoff: Duration::from_secs(1),
        multiplier: 2.0,
    };
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    let result: Result<i32, _> = with_retry(&config, || {
        let count = call_count_clone.clone();
        async move {
            count.fetch_add(1, Ordering::SeqCst);
            anyhow::bail!("HTTP 503 Service unavailable")
        }
    })
    .await;

    assert!(result.is_err());
    assert_eq!(call_count.load(Ordering::SeqCst), 3); // Initial + 2 retries
}

#[tokio::test]
async fn test_retry_non_retryable_error_fails_immediately() {
    let config = RetryConfig::default();
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    let result: Result<i32, _> = with_retry(&config, || {
        let count = call_count_clone.clone();
        async move {
            count.fetch_add(1, Ordering::SeqCst);
            anyhow::bail!("HTTP 400 Bad request")
        }
    })
    .await;

    assert!(result.is_err());
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_retry_rate_limit_error() {
    let config = RetryConfig {
        max_retries: 2,
        initial_backoff: Duration::from_millis(10),
        max_backoff: Duration::from_secs(1),
        multiplier: 2.0,
    };
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    let result = with_retry(&config, || {
        let count = call_count_clone.clone();
        async move {
            let current = count.fetch_add(1, Ordering::SeqCst);
            if current < 1 {
                anyhow::bail!("HTTP 429 Rate limit exceeded")
            } else {
                Ok::<_, anyhow::Error>(42)
            }
        }
    })
    .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
    assert_eq!(call_count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn test_retry_connection_error() {
    let config = RetryConfig {
        max_retries: 2,
        initial_backoff: Duration::from_millis(10),
        max_backoff: Duration::from_secs(1),
        multiplier: 2.0,
    };
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    let result = with_retry(&config, || {
        let count = call_count_clone.clone();
        async move {
            let current = count.fetch_add(1, Ordering::SeqCst);
            if current < 1 {
                anyhow::bail!("Connection timeout")
            } else {
                Ok::<_, anyhow::Error>(42)
            }
        }
    })
    .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
    assert_eq!(call_count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn test_retry_exponential_backoff() {
    let config = RetryConfig {
        max_retries: 3,
        initial_backoff: Duration::from_millis(10),
        max_backoff: Duration::from_millis(100),
        multiplier: 2.0,
    };

    let start = std::time::Instant::now();
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    let _: Result<i32, _> = with_retry(&config, || {
        let count = call_count_clone.clone();
        async move {
            count.fetch_add(1, Ordering::SeqCst);
            anyhow::bail!("HTTP 503 Service unavailable")
        }
    })
    .await;

    let elapsed = start.elapsed();

    // Should have waited: 10ms + 20ms + 40ms = 70ms minimum
    assert!(
        elapsed >= Duration::from_millis(70),
        "Should have exponential backoff delays"
    );
    assert_eq!(call_count.load(Ordering::SeqCst), 4); // Initial + 3 retries
}
