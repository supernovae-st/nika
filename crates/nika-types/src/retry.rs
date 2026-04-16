// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Retry configuration and error classification.
//!
//! `ErrorCategory` drives retry/alerting WITHOUT parsing NIKA-XXX string prefixes.
//! Pattern: Tonic `Status::code()`.

use serde::{Deserialize, Serialize};

/// Retry configuration for operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RetryConfig {
    /// Maximum number of attempts (including the initial one).
    pub max_attempts: u32,
    /// Base delay for exponential backoff in milliseconds.
    pub backoff_base_ms: u64,
    /// Maximum backoff delay in milliseconds.
    pub backoff_max_ms: u64,
    /// Jitter ratio (0.0 = no jitter, 1.0 = full jitter).
    pub jitter_ratio: f32,
}

impl RetryConfig {
    /// Create a new retry config.
    #[must_use]
    pub fn new(max_attempts: u32) -> Self {
        Self {
            max_attempts,
            backoff_base_ms: 1000,
            backoff_max_ms: 30_000,
            jitter_ratio: 0.25,
        }
    }

    /// No retries (single attempt).
    #[must_use]
    pub fn none() -> Self {
        Self {
            max_attempts: 1,
            backoff_base_ms: 0,
            backoff_max_ms: 0,
            jitter_ratio: 0.0,
        }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self::new(3)
    }
}

/// Classification of errors for retry decisions.
///
/// Drives retry/alerting WITHOUT parsing NIKA-XXX string prefixes.
/// Pattern: Tonic `Status::code()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ErrorCategory {
    /// Transient — may succeed on retry (network timeout, 503).
    Transient,
    /// Permanent — will never succeed (400, schema error).
    Permanent,
    /// User fault — bad input, fix the request.
    UserFault,
    /// Provider fault — provider-side issue.
    ProviderFault,
    /// Internal — engine bug.
    Internal,
    /// Rate limited — retry after delay.
    RateLimit,
    /// Budget exceeded — stop, don't retry.
    Budget,
    /// Policy violation — blocked by rules.
    Policy,
}

impl ErrorCategory {
    /// Whether this category is retryable.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Transient | Self::RateLimit | Self::ProviderFault
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_retry_config() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.backoff_base_ms, 1000);
        assert_eq!(config.backoff_max_ms, 30_000);
    }

    #[test]
    fn no_retry_config() {
        let config = RetryConfig::none();
        assert_eq!(config.max_attempts, 1);
    }

    #[test]
    fn retryable_categories() {
        assert!(ErrorCategory::Transient.is_retryable());
        assert!(ErrorCategory::RateLimit.is_retryable());
        assert!(ErrorCategory::ProviderFault.is_retryable());
        assert!(!ErrorCategory::Permanent.is_retryable());
        assert!(!ErrorCategory::UserFault.is_retryable());
        assert!(!ErrorCategory::Budget.is_retryable());
        assert!(!ErrorCategory::Policy.is_retryable());
        assert!(!ErrorCategory::Internal.is_retryable());
    }

    #[test]
    fn retry_config_serde_roundtrip() {
        let config = RetryConfig::new(5);
        let json = serde_json::to_string(&config).expect("serialize");
        let back: RetryConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(config, back);
    }

    #[test]
    fn error_category_serde_roundtrip() {
        let cat = ErrorCategory::RateLimit;
        let json = serde_json::to_string(&cat).expect("serialize");
        assert_eq!(json, "\"rate_limit\"");
        let back: ErrorCategory = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cat, back);
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn retry_types_are_send_sync() {
        _assert_send_sync::<RetryConfig>();
        _assert_send_sync::<ErrorCategory>();
    }
}
