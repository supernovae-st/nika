//! Benchmarks for resilience patterns
//!
//! Run with: `cargo bench --bench resilience`

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

use nika::resilience::{
    CircuitBreaker, CircuitBreakerConfig, Metrics, RateLimiter, RateLimiterConfig, RetryConfig,
    RetryPolicy,
};

// =============================================================================
// Rate Limiter Benchmarks
// =============================================================================

fn bench_rate_limiter_try_acquire(c: &mut Criterion) {
    let mut group = c.benchmark_group("rate_limiter");

    for burst in [10, 100, 1000] {
        group.bench_with_input(
            BenchmarkId::new("try_acquire", burst),
            &burst,
            |b, &burst| {
                let config = RateLimiterConfig::new(10000.0, burst);
                let limiter = RateLimiter::new("bench", config);

                b.iter(|| {
                    // Reset for each iteration to test acquire path
                    limiter.reset();
                    for _ in 0..10 {
                        black_box(limiter.try_acquire());
                    }
                });
            },
        );
    }

    group.finish();
}

fn bench_rate_limiter_refill(c: &mut Criterion) {
    let config = RateLimiterConfig::new(1_000_000.0, 100); // Very fast refill
    let limiter = RateLimiter::new("bench", config);

    // Exhaust tokens
    while limiter.try_acquire() {}

    c.bench_function("rate_limiter/refill_and_acquire", |b| {
        b.iter(|| {
            // This will trigger refill calculation
            black_box(limiter.try_acquire());
        });
    });
}

// =============================================================================
// Circuit Breaker Benchmarks
// =============================================================================

fn bench_circuit_breaker_execute_closed(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let config = CircuitBreakerConfig::default();
    let breaker = CircuitBreaker::new("bench", config);

    c.bench_function("circuit_breaker/execute_closed", |b| {
        b.to_async(&rt).iter(|| async {
            let result = breaker.execute(|| async { Ok::<_, anyhow::Error>(42) }).await;
            black_box(result)
        });
    });
}

fn bench_circuit_breaker_execute_open(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let config = CircuitBreakerConfig::default();
    let breaker = CircuitBreaker::new("bench", config);

    // Force circuit open
    breaker.force_open();

    c.bench_function("circuit_breaker/execute_open_fast_fail", |b| {
        b.to_async(&rt).iter(|| async {
            let result: Result<i32, _> =
                breaker.execute(|| async { Ok::<_, anyhow::Error>(42) }).await;
            black_box(result)
        });
    });
}

fn bench_circuit_breaker_state_check(c: &mut Criterion) {
    let config = CircuitBreakerConfig::default();
    let breaker = CircuitBreaker::new("bench", config);

    c.bench_function("circuit_breaker/state_check", |b| {
        b.iter(|| black_box(breaker.state()));
    });
}

// =============================================================================
// Retry Policy Benchmarks
// =============================================================================

fn bench_retry_calculate_delay(c: &mut Criterion) {
    let config = RetryConfig::default();
    let policy = RetryPolicy::new(config);

    c.bench_function("retry/calculate_delay", |b| {
        b.iter(|| {
            for attempt in 0..5 {
                black_box(policy.calculate_delay(attempt));
            }
        });
    });
}

fn bench_retry_execute_success(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let config = RetryConfig::default();
    let policy = RetryPolicy::new(config);

    c.bench_function("retry/execute_success", |b| {
        b.to_async(&rt).iter(|| async {
            let result = policy.execute(|| async { Ok::<_, anyhow::Error>(42) }).await;
            black_box(result)
        });
    });
}

fn bench_retry_execute_with_retries(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let config = RetryConfig::default()
        .with_max_retries(2)
        .with_initial_delay(Duration::from_micros(1))
        .with_jitter(0.0);

    c.bench_function("retry/execute_with_2_retries", |b| {
        b.to_async(&rt).iter(|| {
            // Create fresh policy for each iteration since it doesn't implement Copy
            let policy = RetryPolicy::new(config.clone());
            let counter = Arc::new(AtomicU32::new(0));

            async move {
                let result = policy
                    .execute(|| {
                        let counter = counter.clone();
                        async move {
                            let count = counter.fetch_add(1, Ordering::SeqCst);
                            if count < 2 {
                                Err(anyhow::anyhow!("transient failure"))
                            } else {
                                Ok::<_, anyhow::Error>(42)
                            }
                        }
                    })
                    .await;
                black_box(result)
            }
        });
    });
}

// =============================================================================
// Metrics Benchmarks
// =============================================================================

fn bench_metrics_record_success(c: &mut Criterion) {
    let metrics = Metrics::new("bench");

    c.bench_function("metrics/record_success", |b| {
        b.iter(|| {
            metrics.record_success(Duration::from_millis(50));
        });
    });
}

fn bench_metrics_snapshot(c: &mut Criterion) {
    let metrics = Metrics::new("bench");

    // Pre-populate with some data
    for i in 0..100 {
        metrics.record_success(Duration::from_millis(i));
    }

    c.bench_function("metrics/snapshot", |b| {
        b.iter(|| black_box(metrics.snapshot()));
    });
}

fn bench_metrics_snapshot_latency_calculation(c: &mut Criterion) {
    let mut group = c.benchmark_group("metrics/snapshot_samples");

    for samples in [10, 100, 1000] {
        group.bench_with_input(
            BenchmarkId::new("latency_calc", samples),
            &samples,
            |b, &samples| {
                let metrics = Metrics::with_max_samples("bench", samples);

                // Fill with samples
                for i in 0..samples as u64 {
                    metrics.record_success(Duration::from_micros(i * 100));
                }

                b.iter(|| black_box(metrics.snapshot()));
            },
        );
    }

    group.finish();
}

// =============================================================================
// Combined Benchmarks
// =============================================================================

fn bench_full_resilience_stack(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("resilience/full_stack_success", |b| {
        let config = RateLimiterConfig::new(100000.0, 1000);
        let limiter = RateLimiter::new("bench", config);

        let cb_config = CircuitBreakerConfig::default();
        let breaker = CircuitBreaker::new("bench", cb_config);

        let retry_config = RetryConfig::default();
        let retry = RetryPolicy::new(retry_config);

        let metrics = Metrics::new("bench");

        b.to_async(&rt).iter(|| async {
            // Simulate full resilience stack
            if !limiter.try_acquire() {
                return Err(anyhow::anyhow!("rate limited"));
            }

            let start = std::time::Instant::now();

            // Correct order: retry wraps circuit breaker
            // Note: resilience module returns NikaError, convert to anyhow for benchmark
            let result = retry
                .execute(|| breaker.execute(|| async { Ok::<_, anyhow::Error>(42) }))
                .await
                .map_err(|e| anyhow::anyhow!("{}", e));

            let latency = start.elapsed();

            match &result {
                Ok(_) => metrics.record_success(latency),
                Err(_) => metrics.record_failure(latency),
            }

            black_box(result)
        });
    });
}

// =============================================================================
// Criterion Groups
// =============================================================================

criterion_group!(
    rate_limiter_benches,
    bench_rate_limiter_try_acquire,
    bench_rate_limiter_refill,
);

criterion_group!(
    circuit_breaker_benches,
    bench_circuit_breaker_execute_closed,
    bench_circuit_breaker_execute_open,
    bench_circuit_breaker_state_check,
);

criterion_group!(
    retry_benches,
    bench_retry_calculate_delay,
    bench_retry_execute_success,
    bench_retry_execute_with_retries,
);

criterion_group!(
    metrics_benches,
    bench_metrics_record_success,
    bench_metrics_snapshot,
    bench_metrics_snapshot_latency_calculation,
);

criterion_group!(combined_benches, bench_full_resilience_stack,);

criterion_main!(
    rate_limiter_benches,
    circuit_breaker_benches,
    retry_benches,
    metrics_benches,
    combined_benches,
);
