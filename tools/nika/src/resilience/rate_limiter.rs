//! Rate Limiting with Token Bucket Algorithm
//!
//! Prevents overwhelming services with too many requests.
//!
//! # Token Bucket Algorithm
//!
//! - Tokens are added to a bucket at a steady rate (refill_rate)
//! - Each request consumes one or more tokens
//! - Bucket has maximum capacity (burst limit)
//! - If bucket is empty, requests are blocked or rejected
//!
//! # Example
//!
//! ```rust,ignore
//! use nika::resilience::{RateLimiter, RateLimiterConfig};
//!
//! // Allow 10 requests per second with burst of 20
//! let config = RateLimiterConfig::new(10, 20);
//! let limiter = RateLimiter::new("api", config);
//!
//! // Acquire a permit (waits if necessary)
//! limiter.acquire().await?;
//!
//! // Or try without waiting
//! if limiter.try_acquire() {
//!     // proceed
//! }
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::Result;

use crate::error::NikaError;

/// Configuration for rate limiter
#[derive(Debug, Clone)]
pub struct RateLimiterConfig {
    /// Requests allowed per second
    pub rate_per_second: f64,
    /// Maximum burst capacity (tokens)
    pub burst_capacity: u32,
    /// Maximum time to wait for a token
    pub max_wait: Duration,
}

impl RateLimiterConfig {
    /// Create a new rate limiter config
    ///
    /// # Arguments
    /// * `rate_per_second` - Steady state requests per second
    /// * `burst_capacity` - Maximum tokens for burst handling
    pub fn new(rate_per_second: f64, burst_capacity: u32) -> Self {
        Self {
            rate_per_second,
            burst_capacity,
            max_wait: Duration::from_secs(30),
        }
    }

    /// Set maximum wait time for acquiring a token
    pub fn with_max_wait(mut self, max_wait: Duration) -> Self {
        self.max_wait = max_wait;
        self
    }
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        Self {
            rate_per_second: 10.0,
            burst_capacity: 20,
            max_wait: Duration::from_secs(30),
        }
    }
}

/// Rate limiter using token bucket algorithm
pub struct RateLimiter {
    name: String,
    config: RateLimiterConfig,
    /// Available tokens (scaled by 1000 for precision)
    tokens: AtomicU64,
    /// Last refill time in milliseconds since UNIX epoch
    last_refill: AtomicU64,
}

impl RateLimiter {
    const SCALE: u64 = 1000; // Token precision scale

    /// Create a new rate limiter
    pub fn new(name: impl Into<String>, config: RateLimiterConfig) -> Self {
        let tokens = (config.burst_capacity as u64) * Self::SCALE;
        Self {
            name: name.into(),
            config,
            tokens: AtomicU64::new(tokens),
            last_refill: AtomicU64::new(Self::current_time_millis()),
        }
    }

    /// Create a rate limiter with default configuration
    pub fn with_defaults(name: impl Into<String>) -> Self {
        Self::new(name, RateLimiterConfig::default())
    }

    /// Get the rate limiter name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get current available tokens (approximate)
    pub fn available_tokens(&self) -> f64 {
        self.refill_tokens();
        (self.tokens.load(Ordering::SeqCst) as f64) / (Self::SCALE as f64)
    }

    /// Try to acquire a token without waiting
    ///
    /// Returns true if a token was acquired, false otherwise
    pub fn try_acquire(&self) -> bool {
        self.try_acquire_tokens(1)
    }

    /// Try to acquire multiple tokens without waiting
    pub fn try_acquire_tokens(&self, count: u32) -> bool {
        self.refill_tokens();

        let required = (count as u64) * Self::SCALE;
        let mut current = self.tokens.load(Ordering::SeqCst);

        loop {
            if current < required {
                return false;
            }

            match self.tokens.compare_exchange_weak(
                current,
                current - required,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return true,
                Err(new) => current = new,
            }
        }
    }

    /// Acquire a token, waiting if necessary
    ///
    /// Returns an error if waiting would exceed max_wait
    pub async fn acquire(&self) -> Result<()> {
        self.acquire_tokens(1).await
    }

    /// Acquire multiple tokens, waiting if necessary
    pub async fn acquire_tokens(&self, count: u32) -> Result<()> {
        let start = std::time::Instant::now();

        loop {
            if self.try_acquire_tokens(count) {
                return Ok(());
            }

            // Check if we've exceeded max wait time
            if start.elapsed() >= self.config.max_wait {
                return Err(NikaError::RateLimitExceeded {
                    resource: self.name.clone(),
                    reason: format!(
                        "waited {}ms for {} tokens",
                        self.config.max_wait.as_millis(),
                        count
                    ),
                }
                .into());
            }

            // Calculate wait time based on refill rate
            let tokens_needed = count as f64;
            let tokens_per_ms = self.config.rate_per_second / 1000.0;
            let wait_ms = (tokens_needed / tokens_per_ms).ceil() as u64;

            // Wait a fraction of the calculated time, then retry
            let sleep_time = Duration::from_millis(wait_ms.max(1).min(100));
            tokio::time::sleep(sleep_time).await;
        }
    }

    /// Refill tokens based on elapsed time
    fn refill_tokens(&self) {
        let now = Self::current_time_millis();
        let last = self.last_refill.load(Ordering::SeqCst);
        let elapsed_ms = now.saturating_sub(last);

        if elapsed_ms == 0 {
            return;
        }

        // Calculate tokens to add
        let tokens_to_add = (elapsed_ms as f64 * self.config.rate_per_second / 1000.0
            * Self::SCALE as f64) as u64;

        if tokens_to_add == 0 {
            return;
        }

        // Try to update last_refill time
        if self
            .last_refill
            .compare_exchange_weak(last, now, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            // Add tokens up to capacity
            let max_tokens = (self.config.burst_capacity as u64) * Self::SCALE;
            let mut current = self.tokens.load(Ordering::SeqCst);

            loop {
                let new_tokens = (current + tokens_to_add).min(max_tokens);
                if new_tokens == current {
                    break;
                }

                match self.tokens.compare_exchange_weak(
                    current,
                    new_tokens,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                ) {
                    Ok(_) => break,
                    Err(new) => current = new,
                }
            }
        }
    }

    /// Reset the rate limiter to full capacity
    pub fn reset(&self) {
        let max_tokens = (self.config.burst_capacity as u64) * Self::SCALE;
        self.tokens.store(max_tokens, Ordering::SeqCst);
        self.last_refill
            .store(Self::current_time_millis(), Ordering::SeqCst);
    }

    fn current_time_millis() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
}

impl std::fmt::Debug for RateLimiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimiter")
            .field("name", &self.name)
            .field("available_tokens", &self.available_tokens())
            .field("rate_per_second", &self.config.rate_per_second)
            .field("burst_capacity", &self.config.burst_capacity)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_config_default() {
        let config = RateLimiterConfig::default();
        assert!((config.rate_per_second - 10.0).abs() < f64::EPSILON);
        assert_eq!(config.burst_capacity, 20);
        assert_eq!(config.max_wait, Duration::from_secs(30));
    }

    #[test]
    fn test_rate_limiter_config_new() {
        let config = RateLimiterConfig::new(5.0, 10);
        assert!((config.rate_per_second - 5.0).abs() < f64::EPSILON);
        assert_eq!(config.burst_capacity, 10);
    }

    #[test]
    fn test_rate_limiter_config_builder() {
        let config = RateLimiterConfig::new(5.0, 10).with_max_wait(Duration::from_secs(5));
        assert_eq!(config.max_wait, Duration::from_secs(5));
    }

    #[test]
    fn test_rate_limiter_initial_tokens() {
        let config = RateLimiterConfig::new(10.0, 20);
        let limiter = RateLimiter::new("test", config);

        // Should start with full burst capacity
        let available = limiter.available_tokens();
        assert!(
            (available - 20.0).abs() < 1.0,
            "expected ~20 tokens, got {}",
            available
        );
    }

    #[test]
    fn test_rate_limiter_try_acquire_success() {
        let config = RateLimiterConfig::new(10.0, 20);
        let limiter = RateLimiter::new("test", config);

        // Should succeed with full bucket
        assert!(limiter.try_acquire());

        let available = limiter.available_tokens();
        assert!(
            (available - 19.0).abs() < 1.0,
            "expected ~19 tokens, got {}",
            available
        );
    }

    #[test]
    fn test_rate_limiter_try_acquire_multiple() {
        let config = RateLimiterConfig::new(10.0, 20);
        let limiter = RateLimiter::new("test", config);

        // Acquire 10 tokens
        assert!(limiter.try_acquire_tokens(10));

        let available = limiter.available_tokens();
        assert!(
            (available - 10.0).abs() < 1.0,
            "expected ~10 tokens, got {}",
            available
        );
    }

    #[test]
    fn test_rate_limiter_try_acquire_exhausts_bucket() {
        let config = RateLimiterConfig::new(10.0, 5);
        let limiter = RateLimiter::new("test", config);

        // Exhaust the bucket
        for _ in 0..5 {
            assert!(limiter.try_acquire());
        }

        // Next acquire should fail
        assert!(!limiter.try_acquire());
    }

    #[test]
    fn test_rate_limiter_try_acquire_tokens_fails_when_not_enough() {
        let config = RateLimiterConfig::new(10.0, 5);
        let limiter = RateLimiter::new("test", config);

        // Can't acquire more than burst capacity
        assert!(!limiter.try_acquire_tokens(10));

        // Can acquire exactly burst capacity
        assert!(limiter.try_acquire_tokens(5));
    }

    #[tokio::test]
    async fn test_rate_limiter_acquire_success() {
        let config = RateLimiterConfig::new(10.0, 20);
        let limiter = RateLimiter::new("test", config);

        let result = limiter.acquire().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_rate_limiter_acquire_waits_for_refill() {
        let config = RateLimiterConfig::new(100.0, 5) // 100 per second = fast refill
            .with_max_wait(Duration::from_secs(1));
        let limiter = RateLimiter::new("test", config);

        // Exhaust bucket
        for _ in 0..5 {
            limiter.try_acquire();
        }

        assert!(!limiter.try_acquire());

        // Acquire should wait and succeed (refills 100/sec)
        let start = std::time::Instant::now();
        let result = limiter.acquire().await;
        let elapsed = start.elapsed();

        assert!(result.is_ok());
        // Should have waited some time for refill
        assert!(
            elapsed >= Duration::from_millis(1),
            "expected some wait time, got {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_rate_limiter_acquire_fails_after_max_wait() {
        let config = RateLimiterConfig::new(0.1, 1) // Very slow: 0.1 per second
            .with_max_wait(Duration::from_millis(50));
        let limiter = RateLimiter::new("test", config);

        // Exhaust bucket
        limiter.try_acquire();

        // Acquire should fail after max_wait
        let result = limiter.acquire().await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.to_string().contains("Rate limit exceeded"));
    }

    #[test]
    fn test_rate_limiter_reset() {
        let config = RateLimiterConfig::new(10.0, 5);
        let limiter = RateLimiter::new("test", config);

        // Exhaust bucket
        for _ in 0..5 {
            limiter.try_acquire();
        }

        assert!(!limiter.try_acquire());

        // Reset should restore full capacity
        limiter.reset();

        assert!(limiter.try_acquire());
        let available = limiter.available_tokens();
        assert!(
            (available - 4.0).abs() < 1.0,
            "expected ~4 tokens after reset+acquire, got {}",
            available
        );
    }

    #[tokio::test]
    async fn test_rate_limiter_refill_over_time() {
        let config = RateLimiterConfig::new(1000.0, 10); // 1000 per second = fast
        let limiter = RateLimiter::new("test", config);

        // Exhaust bucket
        for _ in 0..10 {
            limiter.try_acquire();
        }

        let before = limiter.available_tokens();
        assert!(before < 1.0, "expected near 0 tokens, got {}", before);

        // Wait for refill
        tokio::time::sleep(Duration::from_millis(10)).await;

        let after = limiter.available_tokens();
        assert!(
            after > before,
            "expected tokens to increase, before={}, after={}",
            before,
            after
        );
    }

    #[test]
    fn test_rate_limiter_debug() {
        let limiter = RateLimiter::with_defaults("test");
        let debug_str = format!("{:?}", limiter);
        assert!(debug_str.contains("test"));
        assert!(debug_str.contains("rate_per_second"));
    }
}
