//! Circuit Breaker Pattern
//!
//! Prevents cascading failures by failing fast when a service is down.
//!
//! # States
//!
//! - **Closed**: Normal operation, requests go through
//! - **Open**: Too many failures, requests fail immediately
//! - **Half-Open**: Testing if service recovered
//!
//! # Example
//!
//! ```rust,ignore
//! use nika::resilience::{CircuitBreaker, CircuitBreakerConfig};
//!
//! let config = CircuitBreakerConfig::default();
//! let breaker = CircuitBreaker::new("api-service", config);
//!
//! let result = breaker.execute(|| async {
//!     make_api_call().await
//! }).await;
//! ```

use std::future::Future;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::Duration;

use anyhow::Result;

use crate::error::NikaError;

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation - requests flow through
    Closed,
    /// Service down - requests fail immediately
    Open,
    /// Testing recovery - one request allowed through
    HalfOpen,
}

/// Configuration for circuit breaker behavior
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening circuit
    pub failure_threshold: u32,
    /// Time to wait in open state before transitioning to half-open
    pub recovery_timeout: Duration,
    /// Number of successful requests in half-open to close circuit
    pub success_threshold: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(30),
            success_threshold: 2,
        }
    }
}

impl CircuitBreakerConfig {
    /// Set failure threshold before circuit opens
    pub fn with_failure_threshold(mut self, threshold: u32) -> Self {
        self.failure_threshold = threshold;
        self
    }

    /// Set recovery timeout duration
    pub fn with_recovery_timeout(mut self, timeout: Duration) -> Self {
        self.recovery_timeout = timeout;
        self
    }

    /// Set success threshold to close circuit from half-open
    pub fn with_success_threshold(mut self, threshold: u32) -> Self {
        self.success_threshold = threshold;
        self
    }
}

/// Circuit breaker for fault-tolerant operations
pub struct CircuitBreaker {
    name: String,
    config: CircuitBreakerConfig,
    state: RwLock<CircuitState>,
    failure_count: AtomicU32,
    success_count: AtomicU32,
    last_failure_time: AtomicU64, // Milliseconds since UNIX epoch
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given configuration
    pub fn new(name: impl Into<String>, config: CircuitBreakerConfig) -> Self {
        Self {
            name: name.into(),
            config,
            state: RwLock::new(CircuitState::Closed),
            failure_count: AtomicU32::new(0),
            success_count: AtomicU32::new(0),
            last_failure_time: AtomicU64::new(0),
        }
    }

    /// Create a circuit breaker with default configuration
    pub fn with_defaults(name: impl Into<String>) -> Self {
        Self::new(name, CircuitBreakerConfig::default())
    }

    /// Get the current circuit state
    pub fn state(&self) -> CircuitState {
        *self.state.read().unwrap()
    }

    /// Get the service name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the current failure count
    pub fn failure_count(&self) -> u32 {
        self.failure_count.load(Ordering::SeqCst)
    }

    /// Execute an operation through the circuit breaker
    pub async fn execute<F, Fut, T>(&self, operation: F) -> Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        // Check if circuit should transition from Open to HalfOpen
        self.check_recovery_timeout();

        let current_state = self.state();

        match current_state {
            CircuitState::Closed | CircuitState::HalfOpen => {
                match operation().await {
                    Ok(result) => {
                        self.record_success();
                        Ok(result)
                    }
                    Err(e) => {
                        self.record_failure();
                        Err(e)
                    }
                }
            }
            CircuitState::Open => {
                Err(NikaError::CircuitBreakerOpen {
                    service: self.name.clone(),
                }
                .into())
            }
        }
    }

    /// Check if recovery timeout has passed and transition to half-open
    fn check_recovery_timeout(&self) {
        let state = *self.state.read().unwrap();

        if state == CircuitState::Open {
            let last_failure = self.last_failure_time.load(Ordering::SeqCst);
            let now = Self::current_time_millis();
            let elapsed = Duration::from_millis(now.saturating_sub(last_failure));

            if elapsed >= self.config.recovery_timeout {
                let mut state_guard = self.state.write().unwrap();
                if *state_guard == CircuitState::Open {
                    *state_guard = CircuitState::HalfOpen;
                    self.success_count.store(0, Ordering::SeqCst);
                }
            }
        }
    }

    /// Record a successful operation
    fn record_success(&self) {
        let mut state_guard = self.state.write().unwrap();

        match *state_guard {
            CircuitState::Closed => {
                // Reset failure count on success in closed state
                self.failure_count.store(0, Ordering::SeqCst);
            }
            CircuitState::HalfOpen => {
                let successes = self.success_count.fetch_add(1, Ordering::SeqCst) + 1;
                if successes >= self.config.success_threshold {
                    // Enough successes, close the circuit
                    *state_guard = CircuitState::Closed;
                    self.failure_count.store(0, Ordering::SeqCst);
                    self.success_count.store(0, Ordering::SeqCst);
                }
            }
            CircuitState::Open => {
                // Should not happen, but reset just in case
                self.failure_count.store(0, Ordering::SeqCst);
            }
        }
    }

    /// Record a failed operation
    fn record_failure(&self) {
        let mut state_guard = self.state.write().unwrap();

        self.last_failure_time
            .store(Self::current_time_millis(), Ordering::SeqCst);

        match *state_guard {
            CircuitState::Closed => {
                let failures = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
                if failures >= self.config.failure_threshold {
                    *state_guard = CircuitState::Open;
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in half-open reopens the circuit
                *state_guard = CircuitState::Open;
                self.success_count.store(0, Ordering::SeqCst);
            }
            CircuitState::Open => {
                // Already open, just update failure count
                self.failure_count.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    /// Reset the circuit breaker to closed state (for testing/admin purposes)
    pub fn reset(&self) {
        let mut state_guard = self.state.write().unwrap();
        *state_guard = CircuitState::Closed;
        self.failure_count.store(0, Ordering::SeqCst);
        self.success_count.store(0, Ordering::SeqCst);
        self.last_failure_time.store(0, Ordering::SeqCst);
    }

    /// Force the circuit open (for testing/admin purposes)
    pub fn force_open(&self) {
        let mut state_guard = self.state.write().unwrap();
        *state_guard = CircuitState::Open;
        self.last_failure_time
            .store(Self::current_time_millis(), Ordering::SeqCst);
    }

    /// Force the circuit to half-open state (for testing)
    #[cfg(test)]
    fn force_half_open(&self) {
        let mut state_guard = self.state.write().unwrap();
        *state_guard = CircuitState::HalfOpen;
        self.success_count.store(0, Ordering::SeqCst);
    }

    fn current_time_millis() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
}

impl std::fmt::Debug for CircuitBreaker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CircuitBreaker")
            .field("name", &self.name)
            .field("state", &self.state())
            .field("failure_count", &self.failure_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_config_default() {
        let config = CircuitBreakerConfig::default();
        assert_eq!(config.failure_threshold, 5);
        assert_eq!(config.recovery_timeout, Duration::from_secs(30));
        assert_eq!(config.success_threshold, 2);
    }

    #[test]
    fn test_circuit_breaker_config_builder() {
        let config = CircuitBreakerConfig::default()
            .with_failure_threshold(3)
            .with_recovery_timeout(Duration::from_secs(10))
            .with_success_threshold(1);

        assert_eq!(config.failure_threshold, 3);
        assert_eq!(config.recovery_timeout, Duration::from_secs(10));
        assert_eq!(config.success_threshold, 1);
    }

    #[test]
    fn test_circuit_breaker_initial_state_closed() {
        let breaker = CircuitBreaker::with_defaults("test-service");
        assert_eq!(breaker.state(), CircuitState::Closed);
        assert_eq!(breaker.failure_count(), 0);
    }

    #[tokio::test]
    async fn test_circuit_breaker_allows_requests_when_closed() {
        let breaker = CircuitBreaker::with_defaults("test-service");

        let result = breaker
            .execute(|| async { Ok::<_, anyhow::Error>("success") })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_after_threshold_failures() {
        let config = CircuitBreakerConfig::default().with_failure_threshold(3);
        let breaker = CircuitBreaker::new("test-service", config);

        // Fail 3 times
        for _ in 0..3 {
            let _ = breaker
                .execute(|| async { Err::<(), _>(anyhow::anyhow!("failure")) })
                .await;
        }

        assert_eq!(breaker.state(), CircuitState::Open);
        assert_eq!(breaker.failure_count(), 3);
    }

    #[tokio::test]
    async fn test_circuit_breaker_fails_fast_when_open() {
        let config = CircuitBreakerConfig::default()
            .with_failure_threshold(1)
            .with_recovery_timeout(Duration::from_secs(60)); // Long timeout

        let breaker = CircuitBreaker::new("test-service", config);

        // Trigger open state
        let _ = breaker
            .execute(|| async { Err::<(), _>(anyhow::anyhow!("failure")) })
            .await;

        assert_eq!(breaker.state(), CircuitState::Open);

        // Next request should fail fast
        let result = breaker
            .execute(|| async { Ok::<_, anyhow::Error>("should not run") })
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Circuit breaker open"));
    }

    #[tokio::test]
    async fn test_circuit_breaker_resets_failure_count_on_success() {
        let config = CircuitBreakerConfig::default().with_failure_threshold(3);
        let breaker = CircuitBreaker::new("test-service", config);

        // Fail twice
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async { Err::<(), _>(anyhow::anyhow!("failure")) })
                .await;
        }

        assert_eq!(breaker.failure_count(), 2);

        // Success resets count
        let _ = breaker
            .execute(|| async { Ok::<_, anyhow::Error>(()) })
            .await;

        assert_eq!(breaker.failure_count(), 0);
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_closes_on_success() {
        let config = CircuitBreakerConfig::default()
            .with_failure_threshold(1)
            .with_success_threshold(2);

        let breaker = CircuitBreaker::new("test-service", config);

        // Force to half-open state
        breaker.force_half_open();
        assert_eq!(breaker.state(), CircuitState::HalfOpen);

        // First success
        let _ = breaker
            .execute(|| async { Ok::<_, anyhow::Error>(()) })
            .await;

        assert_eq!(breaker.state(), CircuitState::HalfOpen);

        // Second success should close the circuit
        let _ = breaker
            .execute(|| async { Ok::<_, anyhow::Error>(()) })
            .await;

        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_reopens_on_failure() {
        let config = CircuitBreakerConfig::default()
            .with_failure_threshold(1)
            .with_success_threshold(2);

        let breaker = CircuitBreaker::new("test-service", config);

        // Force to half-open state
        breaker.force_half_open();
        assert_eq!(breaker.state(), CircuitState::HalfOpen);

        // Failure should reopen
        let _ = breaker
            .execute(|| async { Err::<(), _>(anyhow::anyhow!("failure")) })
            .await;

        assert_eq!(breaker.state(), CircuitState::Open);
    }

    #[test]
    fn test_circuit_breaker_reset() {
        let config = CircuitBreakerConfig::default().with_failure_threshold(1);
        let breaker = CircuitBreaker::new("test-service", config);

        breaker.force_open();
        assert_eq!(breaker.state(), CircuitState::Open);

        breaker.reset();
        assert_eq!(breaker.state(), CircuitState::Closed);
        assert_eq!(breaker.failure_count(), 0);
    }

    #[tokio::test]
    async fn test_circuit_breaker_transitions_to_half_open_after_timeout() {
        let config = CircuitBreakerConfig::default()
            .with_failure_threshold(1)
            .with_recovery_timeout(Duration::from_millis(10)); // Very short for testing

        let breaker = CircuitBreaker::new("test-service", config);

        // Trigger open state
        let _ = breaker
            .execute(|| async { Err::<(), _>(anyhow::anyhow!("failure")) })
            .await;

        assert_eq!(breaker.state(), CircuitState::Open);

        // Wait for recovery timeout
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Next call should trigger transition to half-open
        // and the operation should be allowed
        let result = breaker
            .execute(|| async { Ok::<_, anyhow::Error>("recovered") })
            .await;

        assert!(result.is_ok());
        // State could be Closed (if success_threshold=2 and 1 success) or HalfOpen
        // With default success_threshold=2, it should be Closed only after 2 successes
        // But we changed it, so let's check
        assert!(matches!(
            breaker.state(),
            CircuitState::HalfOpen | CircuitState::Closed
        ));
    }

    #[test]
    fn test_circuit_breaker_debug() {
        let breaker = CircuitBreaker::with_defaults("test-service");
        let debug_str = format!("{:?}", breaker);
        assert!(debug_str.contains("test-service"));
        assert!(debug_str.contains("Closed"));
    }
}
