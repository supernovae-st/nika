// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MCP Retry Logic with Exponential Backoff
//!
//! Provides retry capabilities for transient MCP failures like timeouts and
//! connection issues. Uses `backon` crate for exponential backoff with jitter.
//!
//! ## Retryable Errors
//!
//! - `McpTimeout` - Server took too long to respond
//! - `McpNotConnected` - Connection dropped, can reconnect
//! - `McpToolError` - Tool call failed (may be transient)
//! - `McpToolCallFailed` - Tool execution failed
//!
//! ## Non-Retryable Errors
//!
//! - `McpResourceNotFound` - 404-like, permanent
//! - `McpValidationFailed` - Client error, won't change on retry
//! - `McpSchemaError` - Schema mismatch, permanent
//! - `McpNotConfigured` - Config error, permanent

use std::time::Duration;

use backon::{ExponentialBuilder, Retryable};

use crate::error::McpError;

/// Default configuration for MCP retries.
pub const DEFAULT_MAX_RETRIES: usize = 3;
pub const DEFAULT_INITIAL_DELAY: Duration = Duration::from_millis(100);
pub const DEFAULT_MAX_DELAY: Duration = Duration::from_secs(5);

/// Configuration for MCP retry behavior.
#[derive(Debug, Clone)]
pub struct McpRetryConfig {
    /// Maximum number of retry attempts (default: 3)
    pub max_retries: usize,
    /// Initial delay before first retry (default: 100ms)
    pub initial_delay: Duration,
    /// Maximum delay between retries (default: 5s)
    pub max_delay: Duration,
    /// Whether to add jitter to delays (default: true)
    pub jitter: bool,
}

impl Default for McpRetryConfig {
    fn default() -> Self {
        Self {
            max_retries: DEFAULT_MAX_RETRIES,
            initial_delay: DEFAULT_INITIAL_DELAY,
            max_delay: DEFAULT_MAX_DELAY,
            jitter: true,
        }
    }
}

impl McpRetryConfig {
    /// Create a new retry config with custom values.
    pub fn new(max_retries: usize, initial_delay: Duration, max_delay: Duration) -> Self {
        Self {
            max_retries,
            initial_delay,
            max_delay,
            jitter: true,
        }
    }

    /// Disable jitter (useful for testing).
    pub fn without_jitter(mut self) -> Self {
        self.jitter = false;
        self
    }
}

/// Determine if an MCP error is retryable.
///
/// Returns true for transient errors that may succeed on retry.
/// For tool errors, checks the error code — client errors like
/// InvalidParams (-32602) are NOT retried since they'll always fail.
pub fn is_retryable_mcp_error(error: &McpError) -> bool {
    match error {
        McpError::McpTimeout { .. }
        | McpError::McpNotConnected { .. }
        | McpError::McpProtocolError { .. } => true,

        // Tool call failures have no error code — cannot distinguish transient
        // from permanent. Only retry if the reason suggests transience.
        McpError::McpToolCallFailed { reason, .. } => {
            let r = reason.to_lowercase();
            r.contains("timeout") || r.contains("unavailable") || r.contains("overloaded")
        }

        // Tool errors: only retry if the error code is retryable (server-side)
        // InvalidParams, MethodNotFound etc. will always fail on retry
        McpError::McpToolError { error_code, .. } => {
            error_code.as_ref().is_none_or(|code| code.is_retryable())
        }

        // Start errors: NOT retryable — if the server binary doesn't exist,
        // retrying just wastes 30s per reconnect attempt
        McpError::McpStartError { .. } => false,

        _ => false,
    }
}

/// Execute an async operation with MCP retry logic.
///
/// Uses exponential backoff with jitter. Only retries on retryable errors
/// (timeouts, connection issues, transient tool failures).
///
/// # Arguments
///
/// * `config` - Retry configuration
/// * `operation` - Async closure to execute
///
/// # Returns
///
/// The result of the operation if successful, or the last error if all retries fail.
///
/// # Example
///
/// ```ignore
/// use nika_mcp::retry::{retry_mcp_call, McpRetryConfig};
///
/// let result = retry_mcp_call(
///     McpRetryConfig::default(),
///     || async {
///         client.call_tool("my_tool", params.clone()).await
///     },
/// ).await?;
/// ```
pub async fn retry_mcp_call<F, Fut, T>(config: McpRetryConfig, operation: F) -> Result<T, McpError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, McpError>>,
{
    let mut builder = ExponentialBuilder::default()
        .with_min_delay(config.initial_delay)
        .with_max_delay(config.max_delay)
        .with_max_times(config.max_retries)
        .with_factor(2.0);

    if config.jitter {
        builder = builder.with_jitter();
    }

    operation.retry(builder).when(is_retryable_mcp_error).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_is_retryable_mcp_error_timeout() {
        let err = McpError::McpTimeout {
            name: "test".to_string(),
            operation: "call_tool".to_string(),
            timeout_secs: 30,
        };
        assert!(is_retryable_mcp_error(&err));
    }

    #[test]
    fn test_is_retryable_mcp_error_not_connected() {
        let err = McpError::McpNotConnected {
            name: "test".to_string(),
        };
        assert!(is_retryable_mcp_error(&err));
    }

    #[test]
    fn test_is_retryable_mcp_error_tool_error() {
        let err = McpError::McpToolError {
            tool: "test".to_string(),
            reason: "transient failure".to_string(),
            error_code: None,
        };
        assert!(is_retryable_mcp_error(&err));
    }

    #[test]
    fn test_is_retryable_mcp_error_tool_call_failed() {
        let err = McpError::McpToolCallFailed {
            tool: "test".to_string(),
            reason: "timeout".to_string(),
        };
        assert!(is_retryable_mcp_error(&err));
    }

    #[test]
    fn test_is_not_retryable_resource_not_found() {
        let err = McpError::McpResourceNotFound {
            uri: "test://resource".to_string(),
        };
        assert!(!is_retryable_mcp_error(&err));
    }

    #[test]
    fn test_is_not_retryable_validation_failed() {
        let err = McpError::McpValidationFailed {
            tool: "test".to_string(),
            details: "invalid params".to_string(),
            missing: vec![],
            suggestions: vec![],
        };
        assert!(!is_retryable_mcp_error(&err));
    }

    #[test]
    fn test_is_not_retryable_schema_error() {
        let err = McpError::McpSchemaError {
            tool: "test".to_string(),
            reason: "invalid schema".to_string(),
        };
        assert!(!is_retryable_mcp_error(&err));
    }

    #[test]
    fn test_is_not_retryable_not_configured() {
        let err = McpError::McpNotConfigured {
            name: "test".to_string(),
        };
        assert!(!is_retryable_mcp_error(&err));
    }

    #[test]
    fn test_default_config() {
        let config = McpRetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_delay, Duration::from_millis(100));
        assert_eq!(config.max_delay, Duration::from_secs(5));
        assert!(config.jitter);
    }

    #[test]
    fn test_config_without_jitter() {
        let config = McpRetryConfig::default().without_jitter();
        assert!(!config.jitter);
    }

    #[tokio::test]
    async fn test_retry_mcp_call_success_first_try() {
        let config = McpRetryConfig::default().without_jitter();

        let result: Result<i32, McpError> = retry_mcp_call(config, || async { Ok(42) }).await;

        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_mcp_call_success_after_retries() {
        let config = McpRetryConfig::new(3, Duration::from_millis(1), Duration::from_millis(10))
            .without_jitter();

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = Arc::clone(&attempts);

        let result: Result<i32, McpError> = retry_mcp_call(config, || {
            let attempts = Arc::clone(&attempts_clone);
            async move {
                let current = attempts.fetch_add(1, Ordering::SeqCst);
                if current < 2 {
                    Err(McpError::McpTimeout {
                        name: "test".to_string(),
                        operation: "call".to_string(),
                        timeout_secs: 30,
                    })
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
    async fn test_retry_mcp_call_exhausted() {
        let config = McpRetryConfig::new(2, Duration::from_millis(1), Duration::from_millis(10))
            .without_jitter();

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = Arc::clone(&attempts);

        let result: Result<i32, McpError> = retry_mcp_call(config, || {
            let attempts = Arc::clone(&attempts_clone);
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err(McpError::McpTimeout {
                    name: "test".to_string(),
                    operation: "call".to_string(),
                    timeout_secs: 30,
                })
            }
        })
        .await;

        assert!(result.is_err());
        // Initial attempt + 2 retries = 3 total attempts
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_mcp_call_non_retryable_error_no_retry() {
        let config = McpRetryConfig::new(3, Duration::from_millis(1), Duration::from_millis(10))
            .without_jitter();

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = Arc::clone(&attempts);

        let result: Result<i32, McpError> = retry_mcp_call(config, || {
            let attempts = Arc::clone(&attempts_clone);
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                // Non-retryable error - should not retry
                Err(McpError::McpResourceNotFound {
                    uri: "test://resource".to_string(),
                })
            }
        })
        .await;

        assert!(result.is_err());
        // Only 1 attempt for non-retryable errors
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }
}
