//! Resilience Pattern Integration Tests
//!
//! Tests for retry, circuit breaker, and rate limiter patterns.
//! Covers edge cases and state transitions.

use nika::resilience::{CircuitBreaker, CircuitBreakerConfig, CircuitState, RetryConfig, RetryPolicy};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// Retry Policy Edge Cases
// ============================================================================

/// Simple test error for testing retry logic
#[derive(Debug)]
struct TestError(String);

impl std::fmt::Display for TestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for TestError {}

#[tokio::test]
async fn test_retry_with_rate_limit_error_message() {
    let config = RetryConfig::default()
        .with_max_retries(2)
        .with_initial_delay(Duration::from_millis(1))
        .with_jitter(0.0);

    let policy = RetryPolicy::new(config);
    let attempts = Arc::new(AtomicU32::new(0));
    let attempts_clone = attempts.clone();

    let result = policy
        .execute(|| {
            let attempts = attempts_clone.clone();
            async move {
                let count = attempts.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    // Simulate rate limit error
                    Err(TestError("rate limit exceeded (429)".to_string()))
                } else {
                    Ok("success after rate limit")
                }
            }
        })
        .await;

    assert!(result.is_ok());
    assert_eq!(attempts.load(Ordering::SeqCst), 3); // Initial + 2 retries
}

#[tokio::test]
async fn test_retry_with_connection_timeout_error() {
    let config = RetryConfig::default()
        .with_max_retries(1)
        .with_initial_delay(Duration::from_millis(1))
        .with_jitter(0.0);

    let policy = RetryPolicy::new(config);
    let attempts = Arc::new(AtomicU32::new(0));
    let attempts_clone = attempts.clone();

    let result = policy
        .execute(|| {
            let attempts = attempts_clone.clone();
            async move {
                let count = attempts.fetch_add(1, Ordering::SeqCst);
                if count < 1 {
                    // Simulate timeout error
                    Err(TestError("connection timed out".to_string()))
                } else {
                    Ok("recovered from timeout")
                }
            }
        })
        .await;

    assert!(result.is_ok());
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn test_retry_503_service_unavailable() {
    let config = RetryConfig::default()
        .with_max_retries(2)
        .with_initial_delay(Duration::from_millis(1))
        .with_jitter(0.0);

    let policy = RetryPolicy::new(config);
    let attempts = Arc::new(AtomicU32::new(0));
    let attempts_clone = attempts.clone();

    let result = policy
        .execute(|| {
            let attempts = attempts_clone.clone();
            async move {
                let count = attempts.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    Err(TestError("503 Service Unavailable".to_string()))
                } else {
                    Ok("service recovered")
                }
            }
        })
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "service recovered");
}

#[tokio::test]
async fn test_retry_does_not_retry_401_unauthorized() {
    let config = RetryConfig::default()
        .with_max_retries(3)
        .with_initial_delay(Duration::from_millis(1));

    let policy = RetryPolicy::new(config);
    let attempts = Arc::new(AtomicU32::new(0));
    let attempts_clone = attempts.clone();

    let result = policy
        .execute(|| {
            let attempts = attempts_clone.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                // Auth errors should NOT be retried
                Err::<&str, _>(TestError("401 Unauthorized - Invalid API key".to_string()))
            }
        })
        .await;

    assert!(result.is_err());
    // Should only attempt once since auth errors aren't retryable
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_retry_does_not_retry_400_bad_request() {
    let config = RetryConfig::default()
        .with_max_retries(3)
        .with_initial_delay(Duration::from_millis(1));

    let policy = RetryPolicy::new(config);
    let attempts = Arc::new(AtomicU32::new(0));
    let attempts_clone = attempts.clone();

    let result = policy
        .execute(|| {
            let attempts = attempts_clone.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err::<&str, _>(TestError("400 Bad Request - Invalid parameters".to_string()))
            }
        })
        .await;

    assert!(result.is_err());
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_retry_exponential_backoff_timing() {
    // Verify delays are actually exponential
    let config = RetryConfig::default()
        .with_initial_delay(Duration::from_millis(100))
        .with_backoff_multiplier(2.0)
        .with_jitter(0.0);

    let policy = RetryPolicy::new(config);

    let delay_0 = policy.calculate_delay(0);
    let delay_1 = policy.calculate_delay(1);
    let delay_2 = policy.calculate_delay(2);
    let delay_3 = policy.calculate_delay(3);

    assert_eq!(delay_0, Duration::from_millis(100));
    assert_eq!(delay_1, Duration::from_millis(200));
    assert_eq!(delay_2, Duration::from_millis(400));
    assert_eq!(delay_3, Duration::from_millis(800));
}

// ============================================================================
// Circuit Breaker State Transitions
// ============================================================================

#[tokio::test]
async fn test_circuit_breaker_closed_to_open_transition() {
    let config = CircuitBreakerConfig::default()
        .with_failure_threshold(3)
        .with_recovery_timeout(Duration::from_secs(60));

    let breaker = CircuitBreaker::new("test-service", config);

    // Start in closed state
    assert_eq!(breaker.state(), CircuitState::Closed);

    // Fail 3 times to trigger open
    for i in 0..3 {
        let result = breaker
            .execute(|| async { Err::<(), _>(TestError(format!("failure {}", i + 1))) })
            .await;
        assert!(result.is_err());
    }

    // Should now be open
    assert_eq!(breaker.state(), CircuitState::Open);
}

#[tokio::test]
async fn test_circuit_breaker_open_rejects_requests_immediately() {
    let config = CircuitBreakerConfig::default()
        .with_failure_threshold(1)
        .with_recovery_timeout(Duration::from_secs(60));

    let breaker = CircuitBreaker::new("test-service", config);

    // Trigger open state
    let _ = breaker
        .execute(|| async { Err::<(), _>(TestError("trigger open".to_string())) })
        .await;

    assert_eq!(breaker.state(), CircuitState::Open);

    // Next request should fail fast without executing
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    let result = breaker
        .execute(|| {
            let count = call_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok::<_, TestError>("should not run")
            }
        })
        .await;

    assert!(result.is_err());
    // Operation should NOT have been called
    assert_eq!(call_count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn test_circuit_breaker_open_to_half_open_after_timeout() {
    let config = CircuitBreakerConfig::default()
        .with_failure_threshold(1)
        .with_recovery_timeout(Duration::from_millis(50));

    let breaker = CircuitBreaker::new("test-service", config);

    // Trigger open state
    let _ = breaker
        .execute(|| async { Err::<(), _>(TestError("trigger open".to_string())) })
        .await;

    assert_eq!(breaker.state(), CircuitState::Open);

    // Wait for recovery timeout
    tokio::time::sleep(Duration::from_millis(60)).await;

    // Next call should trigger transition to half-open
    let result = breaker
        .execute(|| async { Ok::<_, TestError>("recovery test") })
        .await;

    // Should succeed and state should be half-open or closed
    assert!(result.is_ok());
    assert!(matches!(
        breaker.state(),
        CircuitState::HalfOpen | CircuitState::Closed
    ));
}

#[tokio::test]
async fn test_circuit_breaker_half_open_to_closed_on_success_threshold() {
    let config = CircuitBreakerConfig::default()
        .with_failure_threshold(1)
        .with_success_threshold(2)
        .with_recovery_timeout(Duration::from_millis(10));

    let breaker = CircuitBreaker::new("test-service", config);

    // Trigger open, wait for half-open
    let _ = breaker
        .execute(|| async { Err::<(), _>(TestError("trigger open".to_string())) })
        .await;

    tokio::time::sleep(Duration::from_millis(20)).await;

    // First success
    let _ = breaker.execute(|| async { Ok::<_, TestError>(()) }).await;

    // May still be half-open
    let state_after_first = breaker.state();

    // Second success should close
    let _ = breaker.execute(|| async { Ok::<_, TestError>(()) }).await;

    let state_after_second = breaker.state();

    // After 2 successes (success_threshold=2), should be closed
    assert_eq!(state_after_second, CircuitState::Closed);
}

#[tokio::test]
async fn test_circuit_breaker_half_open_to_open_on_failure() {
    let config = CircuitBreakerConfig::default()
        .with_failure_threshold(1)
        .with_success_threshold(3)
        .with_recovery_timeout(Duration::from_millis(10));

    let breaker = CircuitBreaker::new("test-service", config);

    // Force to half-open state
    breaker.force_open();
    tokio::time::sleep(Duration::from_millis(20)).await;

    // One success (still half-open since threshold=3)
    let _ = breaker.execute(|| async { Ok::<_, TestError>(()) }).await;

    // Now fail in half-open - should reopen
    let _ = breaker
        .execute(|| async { Err::<(), _>(TestError("fail in half-open".to_string())) })
        .await;

    assert_eq!(breaker.state(), CircuitState::Open);
}

#[tokio::test]
async fn test_circuit_breaker_reset_clears_all_state() {
    let config = CircuitBreakerConfig::default().with_failure_threshold(2);

    let breaker = CircuitBreaker::new("test-service", config);

    // Record some failures
    let _ = breaker
        .execute(|| async { Err::<(), _>(TestError("fail 1".to_string())) })
        .await;
    let _ = breaker
        .execute(|| async { Err::<(), _>(TestError("fail 2".to_string())) })
        .await;

    assert_eq!(breaker.state(), CircuitState::Open);
    assert!(breaker.failure_count() >= 2);

    // Reset
    breaker.reset();

    assert_eq!(breaker.state(), CircuitState::Closed);
    assert_eq!(breaker.failure_count(), 0);
}

#[tokio::test]
async fn test_circuit_breaker_success_resets_failure_count_in_closed() {
    let config = CircuitBreakerConfig::default().with_failure_threshold(5);

    let breaker = CircuitBreaker::new("test-service", config);

    // Record 3 failures (not enough to open)
    for _ in 0..3 {
        let _ = breaker
            .execute(|| async { Err::<(), _>(TestError("fail".to_string())) })
            .await;
    }

    assert_eq!(breaker.failure_count(), 3);
    assert_eq!(breaker.state(), CircuitState::Closed);

    // One success should reset failure count
    let _ = breaker.execute(|| async { Ok::<_, TestError>(()) }).await;

    assert_eq!(breaker.failure_count(), 0);
}

// ============================================================================
// Concurrent Access Tests
// ============================================================================

#[tokio::test]
async fn test_circuit_breaker_concurrent_failures() {
    use tokio::task::JoinSet;

    let config = CircuitBreakerConfig::default()
        .with_failure_threshold(5)
        .with_recovery_timeout(Duration::from_secs(60));

    let breaker = Arc::new(CircuitBreaker::new("concurrent-test", config));

    let mut join_set = JoinSet::new();

    // Launch 10 concurrent failing requests
    for i in 0..10 {
        let breaker = Arc::clone(&breaker);
        join_set.spawn(async move {
            let _ = breaker
                .execute(|| async move { Err::<(), _>(TestError(format!("concurrent fail {}", i))) })
                .await;
        });
    }

    // Wait for all to complete
    while join_set.join_next().await.is_some() {}

    // Should be open after 5+ failures
    assert_eq!(breaker.state(), CircuitState::Open);
}

#[tokio::test]
async fn test_retry_policy_concurrent_executions() {
    use tokio::task::JoinSet;

    let config = RetryConfig::default()
        .with_max_retries(2)
        .with_initial_delay(Duration::from_millis(1))
        .with_jitter(0.0);

    let policy = Arc::new(RetryPolicy::new(config));
    let success_count = Arc::new(AtomicU32::new(0));

    let mut join_set = JoinSet::new();

    // Launch 5 concurrent operations that succeed on 2nd try
    for i in 0..5 {
        let policy = Arc::clone(&policy);
        let success_count = Arc::clone(&success_count);
        let attempt = Arc::new(AtomicU32::new(0));

        join_set.spawn(async move {
            let attempt_clone = Arc::clone(&attempt);
            let result = policy
                .execute(move || {
                    let attempt = Arc::clone(&attempt_clone);
                    async move {
                        let count = attempt.fetch_add(1, Ordering::SeqCst);
                        if count < 1 {
                            Err(TestError(format!("temporary failure {}", i)))
                        } else {
                            Ok(format!("success {}", i))
                        }
                    }
                })
                .await;

            if result.is_ok() {
                success_count.fetch_add(1, Ordering::SeqCst);
            }
        });
    }

    // Wait for all
    while join_set.join_next().await.is_some() {}

    // All should succeed after retry
    assert_eq!(success_count.load(Ordering::SeqCst), 5);
}

// ============================================================================
// Edge Cases and Error Messages
// ============================================================================

#[test]
fn test_circuit_breaker_name_preserved() {
    let breaker = CircuitBreaker::with_defaults("my-api-service");
    assert_eq!(breaker.name(), "my-api-service");
}

#[test]
fn test_retry_config_jitter_clamped() {
    let config = RetryConfig::default().with_jitter(1.5); // > 1.0
    assert!((config.jitter - 1.0).abs() < f64::EPSILON); // Should be clamped to 1.0

    let config = RetryConfig::default().with_jitter(-0.5); // < 0.0
    assert!((config.jitter - 0.0).abs() < f64::EPSILON); // Should be clamped to 0.0
}

#[tokio::test]
async fn test_circuit_breaker_error_contains_service_name() {
    let breaker = CircuitBreaker::with_defaults("payment-gateway");
    breaker.force_open();

    let result = breaker
        .execute(|| async { Ok::<_, TestError>("should fail") })
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("payment-gateway"),
        "Error should contain service name: {err}"
    );
}
