//! Retry logic with exponential backoff for provider requests.

use anyhow::Result;
use std::time::Duration;

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Initial backoff duration
    pub initial_backoff: Duration,
    /// Maximum backoff duration
    pub max_backoff: Duration,
    /// Backoff multiplier
    pub multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(10),
            multiplier: 2.0,
        }
    }
}

/// Execute a function with exponential backoff retry logic.
pub async fn with_retry<F, Fut, T>(config: &RetryConfig, mut f: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut attempt = 0;
    let mut backoff = config.initial_backoff;

    loop {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                attempt += 1;
                if attempt > config.max_retries {
                    return Err(e);
                }

                // Check if error is retryable
                if !is_retryable(&e) {
                    return Err(e);
                }

                tracing::warn!(
                    attempt = attempt,
                    max_retries = config.max_retries,
                    backoff_ms = backoff.as_millis(),
                    error = %e,
                    "Request failed, retrying"
                );

                tokio::time::sleep(backoff).await;

                // Exponential backoff with cap
                backoff = Duration::from_secs_f64(
                    (backoff.as_secs_f64() * config.multiplier)
                        .min(config.max_backoff.as_secs_f64()),
                );
            }
        }
    }
}

/// Determine if an error is retryable.
fn is_retryable(error: &anyhow::Error) -> bool {
    let error_str = error.to_string().to_lowercase();

    // Network errors
    if error_str.contains("connection")
        || error_str.contains("timeout")
        || error_str.contains("dns")
        || error_str.contains("network")
    {
        return true;
    }

    // HTTP status codes that are retryable
    if error_str.contains("http 429")  // Rate limit
        || error_str.contains("http 500")  // Internal server error
        || error_str.contains("http 502")  // Bad gateway
        || error_str.contains("http 503")  // Service unavailable
        || error_str.contains("http 504")
    // Gateway timeout
    {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn retry_succeeds_on_first_attempt() {
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
    async fn retry_succeeds_after_failures() {
        let config = RetryConfig::default();
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
    async fn retry_fails_after_max_attempts() {
        let config = RetryConfig {
            max_retries: 2,
            ..Default::default()
        };
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result: Result<i32> = with_retry(&config, || {
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
    async fn non_retryable_error_fails_immediately() {
        let config = RetryConfig::default();
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result: Result<i32> = with_retry(&config, || {
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
}
