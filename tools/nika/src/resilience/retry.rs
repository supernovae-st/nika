//! Retry with exponential backoff
//!
//! Provides retry logic with configurable exponential backoff for transient failures.
//!
//! # Example
//!
//! ```rust,ignore
//! use nika::resilience::{RetryConfig, RetryPolicy};
//!
//! let config = RetryConfig::default();
//! let policy = RetryPolicy::new(config);
//!
//! let result = policy.execute(|| async {
//!     // Operation that might fail transiently
//!     make_api_call().await
//! }).await;
//! ```

use std::future::Future;
use std::time::Duration;

use anyhow::Result;

use crate::error::NikaError;

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (not counting initial attempt)
    pub max_retries: u32,
    /// Initial delay before first retry
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Multiplier for exponential backoff (e.g., 2.0 doubles delay each time)
    pub backoff_multiplier: f64,
    /// Optional jitter factor (0.0 to 1.0) to add randomness
    pub jitter: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
            jitter: 0.1,
        }
    }
}

impl RetryConfig {
    /// Create a new config with specified max retries
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// Set initial delay
    pub fn with_initial_delay(mut self, delay: Duration) -> Self {
        self.initial_delay = delay;
        self
    }

    /// Set max delay cap
    pub fn with_max_delay(mut self, delay: Duration) -> Self {
        self.max_delay = delay;
        self
    }

    /// Set backoff multiplier
    pub fn with_backoff_multiplier(mut self, multiplier: f64) -> Self {
        self.backoff_multiplier = multiplier;
        self
    }

    /// Set jitter factor (0.0 to 1.0)
    pub fn with_jitter(mut self, jitter: f64) -> Self {
        self.jitter = jitter.clamp(0.0, 1.0);
        self
    }
}

/// Retry policy that executes operations with exponential backoff
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    config: RetryConfig,
}

impl RetryPolicy {
    /// Create a new retry policy with the given configuration
    pub fn new(config: RetryConfig) -> Self {
        Self { config }
    }

    /// Create a retry policy with default configuration
    pub fn with_defaults() -> Self {
        Self::new(RetryConfig::default())
    }

    /// Calculate delay for a given attempt (0-indexed)
    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        let base_delay = self.config.initial_delay.as_millis() as f64
            * self.config.backoff_multiplier.powi(attempt as i32);

        let capped_delay = base_delay.min(self.config.max_delay.as_millis() as f64);

        // Apply jitter if configured
        let jittered_delay = if self.config.jitter > 0.0 {
            let jitter_range = capped_delay * self.config.jitter;
            let jitter_offset = rand::random::<f64>() * jitter_range * 2.0 - jitter_range;
            (capped_delay + jitter_offset).max(0.0)
        } else {
            capped_delay
        };

        Duration::from_millis(jittered_delay as u64)
    }

    /// Execute an operation with retry logic
    ///
    /// The operation will be retried if it returns an error that is considered
    /// retryable (transient failures like network errors, rate limits).
    pub async fn execute<F, Fut, T>(&self, operation: F) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    // Check if error is retryable
                    if !Self::is_retryable(&e) {
                        return Err(e);
                    }

                    last_error = Some(e);

                    // Don't sleep after the last attempt
                    if attempt < self.config.max_retries {
                        let delay = self.calculate_delay(attempt);
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            NikaError::RetryExhausted {
                attempts: self.config.max_retries + 1,
                last_error: "Unknown error".to_string(),
            }
            .into()
        }))
    }

    /// Determine if an error is retryable
    fn is_retryable(error: &anyhow::Error) -> bool {
        // Check for specific NikaError variants
        if let Some(nika_error) = error.downcast_ref::<NikaError>() {
            matches!(
                nika_error,
                NikaError::ProviderError { .. }
                    | NikaError::McpNotConnected { .. }
                    | NikaError::McpToolCallFailed { .. }
                    | NikaError::Timeout { .. }
            )
        } else {
            // For other errors, check if they look like transient failures
            let msg = error.to_string().to_lowercase();
            msg.contains("timeout")
                || msg.contains("rate limit")
                || msg.contains("connection")
                || msg.contains("temporary")
                || msg.contains("unavailable")
                || msg.contains("503")
                || msg.contains("429")
                || msg.contains("502")
                || msg.contains("504")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_delay, Duration::from_millis(100));
        assert_eq!(config.max_delay, Duration::from_secs(10));
        assert!((config.backoff_multiplier - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_retry_config_builder() {
        let config = RetryConfig::default()
            .with_max_retries(5)
            .with_initial_delay(Duration::from_millis(50))
            .with_max_delay(Duration::from_secs(5))
            .with_backoff_multiplier(1.5)
            .with_jitter(0.2);

        assert_eq!(config.max_retries, 5);
        assert_eq!(config.initial_delay, Duration::from_millis(50));
        assert_eq!(config.max_delay, Duration::from_secs(5));
        assert!((config.backoff_multiplier - 1.5).abs() < f64::EPSILON);
        assert!((config.jitter - 0.2).abs() < f64::EPSILON);
    }

    #[test]
    fn test_calculate_delay_exponential_backoff() {
        let config = RetryConfig::default()
            .with_initial_delay(Duration::from_millis(100))
            .with_backoff_multiplier(2.0)
            .with_jitter(0.0); // Disable jitter for predictable testing

        let policy = RetryPolicy::new(config);

        // Attempt 0: 100ms * 2^0 = 100ms
        assert_eq!(policy.calculate_delay(0), Duration::from_millis(100));
        // Attempt 1: 100ms * 2^1 = 200ms
        assert_eq!(policy.calculate_delay(1), Duration::from_millis(200));
        // Attempt 2: 100ms * 2^2 = 400ms
        assert_eq!(policy.calculate_delay(2), Duration::from_millis(400));
        // Attempt 3: 100ms * 2^3 = 800ms
        assert_eq!(policy.calculate_delay(3), Duration::from_millis(800));
    }

    #[test]
    fn test_calculate_delay_respects_max_delay() {
        let config = RetryConfig::default()
            .with_initial_delay(Duration::from_millis(100))
            .with_max_delay(Duration::from_millis(500))
            .with_backoff_multiplier(2.0)
            .with_jitter(0.0);

        let policy = RetryPolicy::new(config);

        // Attempt 0: 100ms
        assert_eq!(policy.calculate_delay(0), Duration::from_millis(100));
        // Attempt 1: 200ms
        assert_eq!(policy.calculate_delay(1), Duration::from_millis(200));
        // Attempt 2: 400ms
        assert_eq!(policy.calculate_delay(2), Duration::from_millis(400));
        // Attempt 3: would be 800ms but capped at 500ms
        assert_eq!(policy.calculate_delay(3), Duration::from_millis(500));
        // Attempt 4: still capped at 500ms
        assert_eq!(policy.calculate_delay(4), Duration::from_millis(500));
    }

    #[test]
    fn test_calculate_delay_with_jitter_within_bounds() {
        let config = RetryConfig::default()
            .with_initial_delay(Duration::from_millis(100))
            .with_jitter(0.5); // 50% jitter

        let policy = RetryPolicy::new(config);

        // Run multiple times to verify jitter is applied
        for _ in 0..100 {
            let delay = policy.calculate_delay(0);
            // With 50% jitter on 100ms: range is 50ms to 150ms
            assert!(delay >= Duration::from_millis(50));
            assert!(delay <= Duration::from_millis(150));
        }
    }

    #[tokio::test]
    async fn test_execute_succeeds_on_first_try() {
        let policy = RetryPolicy::with_defaults();
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result: Result<&str> = policy
            .execute(|| {
                let attempts = attempts_clone.clone();
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Ok("success")
                }
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_execute_retries_on_transient_failure() {
        let config = RetryConfig::default()
            .with_max_retries(3)
            .with_initial_delay(Duration::from_millis(1)) // Fast for tests
            .with_jitter(0.0);

        let policy = RetryPolicy::new(config);
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result: Result<&str> = policy
            .execute(|| {
                let attempts = attempts_clone.clone();
                async move {
                    let count = attempts.fetch_add(1, Ordering::SeqCst);
                    if count < 2 {
                        // Fail first 2 attempts with retryable error
                        Err(NikaError::ProviderError {
                            provider: "test".to_string(),
                            reason: "temporary failure".to_string(),
                        }
                        .into())
                    } else {
                        Ok("success after retries")
                    }
                }
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success after retries");
        assert_eq!(attempts.load(Ordering::SeqCst), 3); // Initial + 2 retries
    }

    #[tokio::test]
    async fn test_execute_exhausts_retries() {
        let config = RetryConfig::default()
            .with_max_retries(2)
            .with_initial_delay(Duration::from_millis(1))
            .with_jitter(0.0);

        let policy = RetryPolicy::new(config);
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result: Result<&str> = policy
            .execute(|| {
                let attempts = attempts_clone.clone();
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Err(NikaError::ProviderError {
                        provider: "test".to_string(),
                        reason: "always fails".to_string(),
                    }
                    .into())
                }
            })
            .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 3); // 1 initial + 2 retries
    }

    #[tokio::test]
    async fn test_execute_does_not_retry_non_retryable_error() {
        let config = RetryConfig::default()
            .with_max_retries(3)
            .with_initial_delay(Duration::from_millis(1));

        let policy = RetryPolicy::new(config);
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result: Result<&str> = policy
            .execute(|| {
                let attempts = attempts_clone.clone();
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    // ValidationError is not retryable
                    Err(NikaError::ValidationError {
                        reason: "invalid input".to_string(),
                    }
                    .into())
                }
            })
            .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 1); // Only initial attempt
    }

    #[test]
    fn test_is_retryable_provider_error() {
        let error: anyhow::Error = NikaError::ProviderError {
            provider: "test".to_string(),
            reason: "timeout".to_string(),
        }
        .into();
        assert!(RetryPolicy::is_retryable(&error));
    }

    #[test]
    fn test_is_retryable_timeout() {
        let error: anyhow::Error = NikaError::Timeout {
            operation: "api call".to_string(),
            duration_ms: 5000,
        }
        .into();
        assert!(RetryPolicy::is_retryable(&error));
    }

    #[test]
    fn test_is_retryable_validation_error_not_retryable() {
        let error: anyhow::Error = NikaError::ValidationError {
            reason: "bad input".to_string(),
        }
        .into();
        assert!(!RetryPolicy::is_retryable(&error));
    }

    #[test]
    fn test_is_retryable_string_patterns() {
        // Test various transient error messages
        let retryable_msgs = [
            "connection refused",
            "rate limit exceeded",
            "service temporarily unavailable",
            "503 Service Unavailable",
            "429 Too Many Requests",
            "timeout occurred",
        ];

        for msg in retryable_msgs {
            let error = anyhow::anyhow!(msg);
            assert!(
                RetryPolicy::is_retryable(&error),
                "Expected '{}' to be retryable",
                msg
            );
        }

        // Non-retryable errors
        let non_retryable_msgs = ["invalid API key", "permission denied", "not found"];

        for msg in non_retryable_msgs {
            let error = anyhow::anyhow!(msg);
            assert!(
                !RetryPolicy::is_retryable(&error),
                "Expected '{}' to NOT be retryable",
                msg
            );
        }
    }
}
