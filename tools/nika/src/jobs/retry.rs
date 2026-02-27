//! Retry logic with exponential backoff for Jobs Daemon.
//!
//! Uses the `backon` crate for retry strategies with jitter.

use crate::jobs::config::{BackoffStrategy, RetryConfig};
use crate::jobs::error::JobsError;
use backon::{ConstantBuilder, ExponentialBuilder, FibonacciBuilder, Retryable};
use std::future::Future;
use std::time::Duration;

/// Retry executor using backon crate.
pub struct RetryExecutor {
    config: RetryConfig,
}

impl RetryExecutor {
    /// Create a new retry executor with the given configuration.
    pub fn new(config: RetryConfig) -> Self {
        Self { config }
    }

    /// Create a retry executor with default configuration.
    pub fn default_config() -> Self {
        Self::new(RetryConfig::default())
    }

    /// Execute an async function with retry logic.
    ///
    /// Returns the result if successful within max_attempts, otherwise returns
    /// the last error.
    pub async fn execute<F, Fut, T, E>(&self, operation: F) -> Result<T, JobsError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        match self.config.backoff {
            BackoffStrategy::Constant => self.execute_constant(operation).await,
            BackoffStrategy::Linear => self.execute_linear(operation).await,
            BackoffStrategy::Exponential => self.execute_exponential(operation).await,
        }
    }

    /// Execute with constant backoff strategy.
    async fn execute_constant<F, Fut, T, E>(&self, operation: F) -> Result<T, JobsError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        let mut builder = ConstantBuilder::default()
            .with_delay(self.config.initial_delay)
            .with_max_times(self.config.max_attempts as usize);

        if self.config.jitter {
            builder = builder.with_jitter();
        }

        operation
            .retry(builder)
            .await
            .map_err(|e| JobsError::ExecutionFailed {
                name: "retry".to_string(),
                reason: e.to_string(),
            })
    }

    /// Execute with linear backoff strategy.
    ///
    /// Uses Fibonacci as an approximation for linear growth.
    async fn execute_linear<F, Fut, T, E>(&self, operation: F) -> Result<T, JobsError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        let mut builder = FibonacciBuilder::default()
            .with_min_delay(self.config.initial_delay)
            .with_max_delay(self.config.max_delay)
            .with_max_times(self.config.max_attempts as usize);

        if self.config.jitter {
            builder = builder.with_jitter();
        }

        operation
            .retry(builder)
            .await
            .map_err(|e| JobsError::ExecutionFailed {
                name: "retry".to_string(),
                reason: e.to_string(),
            })
    }

    /// Execute with exponential backoff strategy.
    async fn execute_exponential<F, Fut, T, E>(&self, operation: F) -> Result<T, JobsError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        let mut builder = ExponentialBuilder::default()
            .with_min_delay(self.config.initial_delay)
            .with_max_delay(self.config.max_delay)
            .with_max_times(self.config.max_attempts as usize)
            .with_factor(2.0); // Double each retry

        if self.config.jitter {
            builder = builder.with_jitter();
        }

        operation
            .retry(builder)
            .await
            .map_err(|e| JobsError::ExecutionFailed {
                name: "retry".to_string(),
                reason: e.to_string(),
            })
    }

    /// Get the maximum number of attempts.
    pub fn max_attempts(&self) -> u32 {
        self.config.max_attempts
    }

    /// Get the backoff strategy.
    pub fn strategy(&self) -> BackoffStrategy {
        self.config.backoff
    }

    /// Check if jitter is enabled.
    pub fn has_jitter(&self) -> bool {
        self.config.jitter
    }
}

/// Calculate delay for a specific attempt using the configured strategy.
///
/// This is useful for preview/logging purposes.
pub fn calculate_delay(config: &RetryConfig, attempt: u32) -> Duration {
    if attempt == 0 {
        return Duration::ZERO;
    }

    let base_delay = match config.backoff {
        BackoffStrategy::Constant => config.initial_delay,
        BackoffStrategy::Linear => {
            Duration::from_millis(config.initial_delay.as_millis() as u64 * attempt as u64)
        }
        BackoffStrategy::Exponential => {
            let multiplier = 2u64.saturating_pow(attempt - 1);
            Duration::from_millis(config.initial_delay.as_millis() as u64 * multiplier)
        }
    };

    // Cap at max_delay
    base_delay.min(config.max_delay)
}

/// Determine if an error is retryable.
///
/// By default, most errors are retryable except for validation errors.
pub fn is_retryable_error(error: &JobsError) -> bool {
    match error {
        // Not retryable - validation/config errors
        JobsError::InvalidJobDefinition { .. } => false,
        JobsError::DuplicateJobName { .. } => false,
        JobsError::InvalidCronExpression { .. } => false,
        JobsError::TriggerConfigError { .. } => false,
        JobsError::InvalidNotificationConfig { .. } => false,
        JobsError::ConfigParseError { .. } => false,
        JobsError::InvalidConfig { .. } => false,

        // Not retryable - workflow not found
        JobsError::WorkflowNotFound { .. } => false,
        JobsError::JobNotFound { .. } => false,
        JobsError::ConfigNotFound { .. } => false,

        // Retryable - transient errors
        JobsError::DaemonStartFailed { .. } => true,
        JobsError::ExecutionFailed { .. } => true,
        JobsError::ExecutionTimeout { .. } => true,
        JobsError::WebhookServerError { .. } => true,
        JobsError::DatabaseError { .. } => true,
        JobsError::NotificationFailed { .. } => true,
        JobsError::IoError { .. } => true,
        JobsError::FileSystemError { .. } => true,

        // Special cases
        JobsError::MaxRetriesExceeded { .. } => false, // Already retried
        JobsError::JobAlreadyRunning { .. } => false,  // Should wait, not retry
        JobsError::DaemonAlreadyRunning { .. } => false,
        JobsError::DaemonNotRunning => false,
        JobsError::CannotCancelRunning { .. } => false,
        JobsError::ChannelClosed => false,

        // Database/serialization - may be retryable
        JobsError::DatabaseOpenFailed { .. } => true,
        JobsError::SerializationError { .. } => false,
        JobsError::MigrationError { .. } => false,
        JobsError::PidFileError { .. } => true,
        JobsError::WatchPathError { .. } => true,
        JobsError::ExecutionNotFound { .. } => false,
        JobsError::DaemonStopFailed { .. } => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_calculate_delay_constant() {
        let config = RetryConfig {
            backoff: BackoffStrategy::Constant,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            max_attempts: 3,
            jitter: false,
        };

        assert_eq!(calculate_delay(&config, 0), Duration::ZERO);
        assert_eq!(calculate_delay(&config, 1), Duration::from_millis(100));
        assert_eq!(calculate_delay(&config, 2), Duration::from_millis(100));
        assert_eq!(calculate_delay(&config, 3), Duration::from_millis(100));
    }

    #[test]
    fn test_calculate_delay_linear() {
        let config = RetryConfig {
            backoff: BackoffStrategy::Linear,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            max_attempts: 5,
            jitter: false,
        };

        assert_eq!(calculate_delay(&config, 0), Duration::ZERO);
        assert_eq!(calculate_delay(&config, 1), Duration::from_millis(100));
        assert_eq!(calculate_delay(&config, 2), Duration::from_millis(200));
        assert_eq!(calculate_delay(&config, 3), Duration::from_millis(300));
    }

    #[test]
    fn test_calculate_delay_exponential() {
        let config = RetryConfig {
            backoff: BackoffStrategy::Exponential,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            max_attempts: 5,
            jitter: false,
        };

        assert_eq!(calculate_delay(&config, 0), Duration::ZERO);
        assert_eq!(calculate_delay(&config, 1), Duration::from_millis(100));
        assert_eq!(calculate_delay(&config, 2), Duration::from_millis(200));
        assert_eq!(calculate_delay(&config, 3), Duration::from_millis(400));
        assert_eq!(calculate_delay(&config, 4), Duration::from_millis(800));
    }

    #[test]
    fn test_calculate_delay_respects_max() {
        let config = RetryConfig {
            backoff: BackoffStrategy::Exponential,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(5),
            max_attempts: 10,
            jitter: false,
        };

        // 1s * 2^9 = 512s, but should cap at 5s
        assert_eq!(calculate_delay(&config, 10), Duration::from_secs(5));
    }

    #[test]
    fn test_is_retryable_error_transient() {
        assert!(is_retryable_error(&JobsError::ExecutionFailed {
            name: "test".to_string(),
            reason: "timeout".to_string(),
        }));

        assert!(is_retryable_error(&JobsError::IoError {
            reason: "connection reset".to_string(),
        }));
    }

    #[test]
    fn test_is_retryable_error_permanent() {
        assert!(!is_retryable_error(&JobsError::JobNotFound {
            name: "test".to_string(),
        }));

        assert!(!is_retryable_error(&JobsError::InvalidCronExpression {
            expression: "bad".to_string(),
        }));
    }

    #[tokio::test]
    async fn test_retry_executor_success_first_try() {
        let executor = RetryExecutor::new(RetryConfig {
            max_attempts: 3,
            backoff: BackoffStrategy::Constant,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_secs(1),
            jitter: false,
        });

        let result: Result<i32, JobsError> =
            executor.execute(|| async { Ok::<_, String>(42) }).await;

        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_executor_success_after_retries() {
        let executor = RetryExecutor::new(RetryConfig {
            max_attempts: 3,
            backoff: BackoffStrategy::Constant,
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_secs(1),
            jitter: false,
        });

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = Arc::clone(&attempts);

        let result: Result<i32, JobsError> = executor
            .execute(|| {
                let attempts = Arc::clone(&attempts_clone);
                async move {
                    let current = attempts.fetch_add(1, Ordering::SeqCst);
                    if current < 2 {
                        Err("not yet".to_string())
                    } else {
                        Ok(42)
                    }
                }
            })
            .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_executor_exhausted() {
        let executor = RetryExecutor::new(RetryConfig {
            max_attempts: 2,
            backoff: BackoffStrategy::Constant,
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_secs(1),
            jitter: false,
        });

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = Arc::clone(&attempts);

        let result: Result<i32, JobsError> = executor
            .execute(|| {
                let attempts = Arc::clone(&attempts_clone);
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Err::<i32, _>("always fails".to_string())
                }
            })
            .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_retry_executor_config_accessors() {
        let config = RetryConfig {
            max_attempts: 5,
            backoff: BackoffStrategy::Linear,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            jitter: true,
        };

        let executor = RetryExecutor::new(config);

        assert_eq!(executor.max_attempts(), 5);
        assert_eq!(executor.strategy(), BackoffStrategy::Linear);
        assert!(executor.has_jitter());
    }
}
