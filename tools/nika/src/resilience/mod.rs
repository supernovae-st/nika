//! Resilience patterns for fault-tolerant operations
//!
//! This module provides:
//! - [`retry`]: Retry with exponential backoff
//! - [`circuit_breaker`]: Circuit breaker pattern
//! - [`rate_limiter`]: Rate limiting with token bucket
//! - [`provider`]: Resilient provider wrapper
//! - [`metrics`]: Performance metrics collection

pub mod circuit_breaker;
pub mod metrics;
pub mod provider;
pub mod rate_limiter;
pub mod retry;

pub use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
pub use metrics::{LatencyStats, Metrics, MetricsSnapshot};
pub use provider::{ResilientProvider, ResilientProviderConfig};
pub use rate_limiter::{RateLimiter, RateLimiterConfig};
pub use retry::{RetryConfig, RetryPolicy};
