//! Resource limits and safety controls for workflow execution
//!
//! Provides configurable limits for:
//! - Execution timeouts
//! - Memory usage
//! - Task iterations
//! - API rate limiting

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Global resource limits for workflow execution
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Maximum execution time for entire workflow
    pub max_workflow_duration: Duration,

    /// Maximum execution time per task
    pub max_task_duration: Duration,

    /// Maximum number of retry attempts per task
    pub max_retries: usize,

    /// Maximum depth for recursive task execution
    pub max_recursion_depth: usize,

    /// Maximum number of concurrent tasks
    pub max_concurrent_tasks: usize,

    /// Maximum size for task output (in bytes)
    pub max_output_size: usize,

    /// Rate limiter for external API calls
    pub rate_limiter: Option<Arc<RateLimiter>>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_workflow_duration: Duration::from_secs(3600), // 1 hour
            max_task_duration: Duration::from_secs(300),      // 5 minutes
            max_retries: 3,
            max_recursion_depth: 10,
            max_concurrent_tasks: 10,
            max_output_size: 10 * 1024 * 1024, // 10 MB
            rate_limiter: None,
        }
    }
}

impl ResourceLimits {
    /// Create limits suitable for testing (more restrictive)
    pub fn testing() -> Self {
        Self {
            max_workflow_duration: Duration::from_secs(60), // 1 minute
            max_task_duration: Duration::from_secs(10),     // 10 seconds
            max_retries: 1,
            max_recursion_depth: 3,
            max_concurrent_tasks: 2,
            max_output_size: 1024 * 1024, // 1 MB
            rate_limiter: Some(Arc::new(RateLimiter::new(10, Duration::from_secs(60)))),
        }
    }

    /// Create limits suitable for production (balanced)
    pub fn production() -> Self {
        Self::default()
    }

    /// Create unlimited configuration (use with caution!)
    pub fn unlimited() -> Self {
        Self {
            max_workflow_duration: Duration::from_secs(86400), // 24 hours
            max_task_duration: Duration::from_secs(3600),      // 1 hour
            max_retries: 10,
            max_recursion_depth: 100,
            max_concurrent_tasks: 100,
            max_output_size: 100 * 1024 * 1024, // 100 MB
            rate_limiter: None,
        }
    }
}

/// Simple token bucket rate limiter
#[derive(Debug)]
pub struct RateLimiter {
    /// Maximum tokens in the bucket
    capacity: usize,
    /// Current tokens available
    tokens: AtomicUsize,
    /// Refill rate (tokens per refill_period)
    refill_period: Duration,
    /// Last refill time
    last_refill: std::sync::Mutex<Instant>,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(capacity: usize, refill_period: Duration) -> Self {
        Self {
            capacity,
            tokens: AtomicUsize::new(capacity),
            refill_period,
            last_refill: std::sync::Mutex::new(Instant::now()),
        }
    }

    /// Try to acquire a token, returns true if successful
    pub fn try_acquire(&self) -> bool {
        self.try_acquire_n(1)
    }

    /// Try to acquire n tokens, returns true if successful
    pub fn try_acquire_n(&self, n: usize) -> bool {
        // Refill tokens if needed
        self.refill();

        // Try to acquire tokens
        let mut current = self.tokens.load(Ordering::Relaxed);
        loop {
            if current < n {
                return false; // Not enough tokens
            }
            match self.tokens.compare_exchange_weak(
                current,
                current - n,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(actual) => current = actual,
            }
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&self) {
        let mut last_refill = self.last_refill.lock().unwrap();
        let now = Instant::now();
        let elapsed = now.duration_since(*last_refill);

        if elapsed >= self.refill_period {
            // Refill to capacity
            self.tokens.store(self.capacity, Ordering::SeqCst);
            *last_refill = now;
        }
    }

    /// Get current available tokens
    pub fn available_tokens(&self) -> usize {
        self.refill();
        self.tokens.load(Ordering::Relaxed)
    }
}

/// Circuit breaker for handling repeated failures
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    /// Number of failures before opening circuit
    failure_threshold: usize,
    /// Duration to wait before attempting to close circuit
    reset_timeout: Duration,
    /// Current state
    state: Arc<std::sync::Mutex<CircuitState>>,
}

#[derive(Debug)]
struct CircuitState {
    failures: usize,
    last_failure: Option<Instant>,
    state: BreakerState,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BreakerState {
    Closed,   // Normal operation
    Open,     // Too many failures, rejecting requests
    HalfOpen, // Testing if service recovered
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(failure_threshold: usize, reset_timeout: Duration) -> Self {
        Self {
            failure_threshold,
            reset_timeout,
            state: Arc::new(std::sync::Mutex::new(CircuitState {
                failures: 0,
                last_failure: None,
                state: BreakerState::Closed,
            })),
        }
    }

    /// Check if the circuit breaker allows a request
    pub fn can_proceed(&self) -> bool {
        let mut state = self.state.lock().unwrap();

        match state.state {
            BreakerState::Closed => true,
            BreakerState::Open => {
                // Check if we should transition to half-open
                if let Some(last_failure) = state.last_failure {
                    if last_failure.elapsed() >= self.reset_timeout {
                        state.state = BreakerState::HalfOpen;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            BreakerState::HalfOpen => true,
        }
    }

    /// Record a successful request
    pub fn record_success(&self) {
        let mut state = self.state.lock().unwrap();

        if state.state == BreakerState::HalfOpen {
            // Success in half-open state closes the circuit
            state.state = BreakerState::Closed;
            state.failures = 0;
            state.last_failure = None;
        }
    }

    /// Record a failed request
    pub fn record_failure(&self) {
        let mut state = self.state.lock().unwrap();

        state.failures += 1;
        state.last_failure = Some(Instant::now());

        match state.state {
            BreakerState::Closed => {
                if state.failures >= self.failure_threshold {
                    state.state = BreakerState::Open;
                }
            }
            BreakerState::HalfOpen => {
                // Failure in half-open immediately opens the circuit
                state.state = BreakerState::Open;
            }
            BreakerState::Open => {
                // Already open, nothing to do
            }
        }
    }

    /// Get the current state
    pub fn current_state(&self) -> BreakerState {
        self.state.lock().unwrap().state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_rate_limiter() {
        let limiter = RateLimiter::new(5, Duration::from_millis(100));

        // Should be able to acquire 5 tokens
        for _ in 0..5 {
            assert!(limiter.try_acquire());
        }

        // 6th should fail
        assert!(!limiter.try_acquire());

        // Wait for refill
        thread::sleep(Duration::from_millis(101));

        // Should be able to acquire again
        assert!(limiter.try_acquire());
    }

    #[test]
    fn test_circuit_breaker() {
        let breaker = CircuitBreaker::new(3, Duration::from_millis(100));

        // Initially closed
        assert_eq!(breaker.current_state(), BreakerState::Closed);
        assert!(breaker.can_proceed());

        // Record failures
        for _ in 0..3 {
            breaker.record_failure();
        }

        // Should be open now
        assert_eq!(breaker.current_state(), BreakerState::Open);
        assert!(!breaker.can_proceed());

        // Wait for reset timeout
        thread::sleep(Duration::from_millis(101));

        // Should be half-open
        assert!(breaker.can_proceed());

        // Success should close it
        breaker.record_success();
        assert_eq!(breaker.current_state(), BreakerState::Closed);
    }

    #[test]
    fn test_resource_limits_profiles() {
        let testing = ResourceLimits::testing();
        assert_eq!(testing.max_task_duration, Duration::from_secs(10));
        assert!(testing.rate_limiter.is_some());

        let production = ResourceLimits::production();
        assert_eq!(production.max_task_duration, Duration::from_secs(300));

        let unlimited = ResourceLimits::unlimited();
        assert_eq!(unlimited.max_task_duration, Duration::from_secs(3600));
    }
}
