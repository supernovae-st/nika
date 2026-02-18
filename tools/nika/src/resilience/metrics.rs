//! Performance Metrics Collection
//!
//! Lightweight metrics for observability without external dependencies.
//!
//! # Features
//!
//! - Request counting (success/failure)
//! - Latency tracking with percentiles
//! - Circuit breaker state transitions
//! - Thread-safe atomic counters
//!
//! # Example
//!
//! ```rust,ignore
//! use nika::resilience::{Metrics, MetricsSnapshot};
//! use std::time::Duration;
//!
//! let metrics = Metrics::new("api");
//! metrics.record_success(Duration::from_millis(50));
//! metrics.record_failure(Duration::from_millis(100));
//!
//! let snapshot = metrics.snapshot();
//! println!("Requests: {}, Failures: {}", snapshot.total_requests, snapshot.failures);
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Performance metrics collector
pub struct Metrics {
    name: String,
    /// Total successful requests
    successes: AtomicU64,
    /// Total failed requests
    failures: AtomicU64,
    /// Total retries attempted
    retries: AtomicU64,
    /// Circuit breaker trips (opened)
    circuit_trips: AtomicU64,
    /// Rate limit hits
    rate_limits: AtomicU64,
    /// Latency samples for percentile calculation (recent window)
    latencies: RwLock<Vec<u64>>, // microseconds
    /// Maximum samples to keep
    max_samples: usize,
    /// Start time for uptime calculation
    start_time: Instant,
}

impl Metrics {
    const DEFAULT_MAX_SAMPLES: usize = 1000;

    /// Create a new metrics collector
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            successes: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            retries: AtomicU64::new(0),
            circuit_trips: AtomicU64::new(0),
            rate_limits: AtomicU64::new(0),
            latencies: RwLock::new(Vec::with_capacity(Self::DEFAULT_MAX_SAMPLES)),
            max_samples: Self::DEFAULT_MAX_SAMPLES,
            start_time: Instant::now(),
        }
    }

    /// Create metrics with custom sample size
    pub fn with_max_samples(name: impl Into<String>, max_samples: usize) -> Self {
        Self {
            name: name.into(),
            successes: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            retries: AtomicU64::new(0),
            circuit_trips: AtomicU64::new(0),
            rate_limits: AtomicU64::new(0),
            latencies: RwLock::new(Vec::with_capacity(max_samples)),
            max_samples,
            start_time: Instant::now(),
        }
    }

    /// Get the metrics name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Record a successful operation
    pub fn record_success(&self, latency: Duration) {
        self.successes.fetch_add(1, Ordering::SeqCst);
        self.record_latency(latency);
    }

    /// Record a failed operation
    pub fn record_failure(&self, latency: Duration) {
        self.failures.fetch_add(1, Ordering::SeqCst);
        self.record_latency(latency);
    }

    /// Record a retry attempt
    pub fn record_retry(&self) {
        self.retries.fetch_add(1, Ordering::SeqCst);
    }

    /// Record circuit breaker trip (opened)
    pub fn record_circuit_trip(&self) {
        self.circuit_trips.fetch_add(1, Ordering::SeqCst);
    }

    /// Record rate limit hit
    pub fn record_rate_limit(&self) {
        self.rate_limits.fetch_add(1, Ordering::SeqCst);
    }

    /// Record latency sample
    fn record_latency(&self, latency: Duration) {
        let micros = latency.as_micros() as u64;
        let mut latencies = self.latencies.write().unwrap();

        if latencies.len() >= self.max_samples {
            // Remove oldest sample (FIFO behavior)
            latencies.remove(0);
        }
        latencies.push(micros);
    }

    /// Get a snapshot of current metrics
    pub fn snapshot(&self) -> MetricsSnapshot {
        let successes = self.successes.load(Ordering::SeqCst);
        let failures = self.failures.load(Ordering::SeqCst);
        let retries = self.retries.load(Ordering::SeqCst);
        let circuit_trips = self.circuit_trips.load(Ordering::SeqCst);
        let rate_limits = self.rate_limits.load(Ordering::SeqCst);
        let uptime = self.start_time.elapsed();

        // Calculate latency statistics
        let latencies = self.latencies.read().unwrap();
        let latency_stats = Self::calculate_latency_stats(&latencies);

        MetricsSnapshot {
            name: self.name.clone(),
            total_requests: successes + failures,
            successes,
            failures,
            retries,
            circuit_trips,
            rate_limits,
            latency_stats,
            uptime,
        }
    }

    /// Reset all metrics (useful for testing or periodic resets)
    pub fn reset(&self) {
        self.successes.store(0, Ordering::SeqCst);
        self.failures.store(0, Ordering::SeqCst);
        self.retries.store(0, Ordering::SeqCst);
        self.circuit_trips.store(0, Ordering::SeqCst);
        self.rate_limits.store(0, Ordering::SeqCst);
        self.latencies.write().unwrap().clear();
    }

    /// Calculate latency statistics from samples
    fn calculate_latency_stats(samples: &[u64]) -> LatencyStats {
        if samples.is_empty() {
            return LatencyStats::default();
        }

        let mut sorted: Vec<u64> = samples.to_vec();
        sorted.sort_unstable();

        let count = sorted.len();
        let sum: u64 = sorted.iter().sum();
        let min = sorted[0];
        let max = sorted[count - 1];
        let avg = sum / count as u64;

        // Percentiles
        let p50 = sorted[count * 50 / 100];
        let p95 = sorted[(count * 95 / 100).min(count - 1)];
        let p99 = sorted[(count * 99 / 100).min(count - 1)];

        LatencyStats {
            min: Duration::from_micros(min),
            max: Duration::from_micros(max),
            avg: Duration::from_micros(avg),
            p50: Duration::from_micros(p50),
            p95: Duration::from_micros(p95),
            p99: Duration::from_micros(p99),
            sample_count: count,
        }
    }
}

impl std::fmt::Debug for Metrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let snapshot = self.snapshot();
        f.debug_struct("Metrics")
            .field("name", &self.name)
            .field("total_requests", &snapshot.total_requests)
            .field("failures", &snapshot.failures)
            .field("success_rate", &format!("{:.1}%", snapshot.success_rate()))
            .finish()
    }
}

/// Snapshot of metrics at a point in time
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    /// Metrics name
    pub name: String,
    /// Total requests (success + failures)
    pub total_requests: u64,
    /// Successful requests
    pub successes: u64,
    /// Failed requests
    pub failures: u64,
    /// Retry attempts
    pub retries: u64,
    /// Circuit breaker trips
    pub circuit_trips: u64,
    /// Rate limit hits
    pub rate_limits: u64,
    /// Latency statistics
    pub latency_stats: LatencyStats,
    /// Time since metrics creation
    pub uptime: Duration,
}

impl MetricsSnapshot {
    /// Calculate success rate as percentage (0.0 to 100.0)
    pub fn success_rate(&self) -> f64 {
        if self.total_requests == 0 {
            return 100.0; // No requests = 100% success
        }
        self.successes as f64 / self.total_requests as f64 * 100.0
    }

    /// Calculate requests per second
    pub fn requests_per_second(&self) -> f64 {
        if self.uptime.as_secs_f64() == 0.0 {
            return 0.0;
        }
        self.total_requests as f64 / self.uptime.as_secs_f64()
    }

    /// Check if error rate exceeds threshold
    pub fn error_rate_exceeds(&self, threshold: f64) -> bool {
        if self.total_requests == 0 {
            return false;
        }
        let error_rate = self.failures as f64 / self.total_requests as f64;
        error_rate > threshold
    }
}

/// Latency statistics
#[derive(Debug, Clone, Default)]
pub struct LatencyStats {
    /// Minimum latency
    pub min: Duration,
    /// Maximum latency
    pub max: Duration,
    /// Average latency
    pub avg: Duration,
    /// 50th percentile (median)
    pub p50: Duration,
    /// 95th percentile
    pub p95: Duration,
    /// 99th percentile
    pub p99: Duration,
    /// Number of samples
    pub sample_count: usize,
}

impl LatencyStats {
    /// Check if p95 latency exceeds threshold
    pub fn p95_exceeds(&self, threshold: Duration) -> bool {
        self.p95 > threshold
    }

    /// Check if p99 latency exceeds threshold
    pub fn p99_exceeds(&self, threshold: Duration) -> bool {
        self.p99 > threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_new() {
        let metrics = Metrics::new("test");
        assert_eq!(metrics.name(), "test");

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.total_requests, 0);
        assert_eq!(snapshot.successes, 0);
        assert_eq!(snapshot.failures, 0);
    }

    #[test]
    fn test_metrics_record_success() {
        let metrics = Metrics::new("test");

        metrics.record_success(Duration::from_millis(50));
        metrics.record_success(Duration::from_millis(100));

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.successes, 2);
        assert_eq!(snapshot.failures, 0);
        assert_eq!(snapshot.total_requests, 2);
    }

    #[test]
    fn test_metrics_record_failure() {
        let metrics = Metrics::new("test");

        metrics.record_failure(Duration::from_millis(100));
        metrics.record_success(Duration::from_millis(50));

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.successes, 1);
        assert_eq!(snapshot.failures, 1);
        assert_eq!(snapshot.total_requests, 2);
    }

    #[test]
    fn test_metrics_record_retry() {
        let metrics = Metrics::new("test");

        metrics.record_retry();
        metrics.record_retry();
        metrics.record_retry();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.retries, 3);
    }

    #[test]
    fn test_metrics_record_circuit_trip() {
        let metrics = Metrics::new("test");

        metrics.record_circuit_trip();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.circuit_trips, 1);
    }

    #[test]
    fn test_metrics_record_rate_limit() {
        let metrics = Metrics::new("test");

        metrics.record_rate_limit();
        metrics.record_rate_limit();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.rate_limits, 2);
    }

    #[test]
    fn test_metrics_latency_stats() {
        let metrics = Metrics::new("test");

        // Record some latencies
        for i in 1..=100 {
            metrics.record_success(Duration::from_millis(i));
        }

        let snapshot = metrics.snapshot();
        let stats = &snapshot.latency_stats;

        assert_eq!(stats.sample_count, 100);
        assert_eq!(stats.min, Duration::from_millis(1));
        assert_eq!(stats.max, Duration::from_millis(100));

        // p50 should be around 50ms
        assert!(
            stats.p50 >= Duration::from_millis(49) && stats.p50 <= Duration::from_millis(51),
            "p50 was {:?}",
            stats.p50
        );

        // p95 should be around 95ms
        assert!(
            stats.p95 >= Duration::from_millis(94) && stats.p95 <= Duration::from_millis(96),
            "p95 was {:?}",
            stats.p95
        );
    }

    #[test]
    fn test_metrics_max_samples() {
        let metrics = Metrics::with_max_samples("test", 10);

        // Record more than max samples
        for i in 1..=20 {
            metrics.record_success(Duration::from_millis(i));
        }

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.latency_stats.sample_count, 10);

        // Should keep the most recent samples (11-20)
        assert_eq!(snapshot.latency_stats.min, Duration::from_millis(11));
        assert_eq!(snapshot.latency_stats.max, Duration::from_millis(20));
    }

    #[test]
    fn test_metrics_reset() {
        let metrics = Metrics::new("test");

        metrics.record_success(Duration::from_millis(50));
        metrics.record_failure(Duration::from_millis(100));
        metrics.record_retry();

        metrics.reset();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.total_requests, 0);
        assert_eq!(snapshot.retries, 0);
        assert_eq!(snapshot.latency_stats.sample_count, 0);
    }

    #[test]
    fn test_metrics_snapshot_success_rate() {
        let metrics = Metrics::new("test");

        // 8 successes, 2 failures = 80% success rate
        for _ in 0..8 {
            metrics.record_success(Duration::from_millis(10));
        }
        for _ in 0..2 {
            metrics.record_failure(Duration::from_millis(10));
        }

        let snapshot = metrics.snapshot();
        assert!((snapshot.success_rate() - 80.0).abs() < 0.001);
    }

    #[test]
    fn test_metrics_snapshot_success_rate_no_requests() {
        let metrics = Metrics::new("test");
        let snapshot = metrics.snapshot();
        assert!((snapshot.success_rate() - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_metrics_snapshot_error_rate_exceeds() {
        let metrics = Metrics::new("test");

        // 5 successes, 5 failures = 50% error rate
        for _ in 0..5 {
            metrics.record_success(Duration::from_millis(10));
        }
        for _ in 0..5 {
            metrics.record_failure(Duration::from_millis(10));
        }

        let snapshot = metrics.snapshot();
        assert!(snapshot.error_rate_exceeds(0.49));
        assert!(!snapshot.error_rate_exceeds(0.51));
    }

    #[test]
    fn test_latency_stats_threshold_checks() {
        let stats = LatencyStats {
            min: Duration::from_millis(10),
            max: Duration::from_millis(200),
            avg: Duration::from_millis(50),
            p50: Duration::from_millis(50),
            p95: Duration::from_millis(150),
            p99: Duration::from_millis(180),
            sample_count: 100,
        };

        assert!(stats.p95_exceeds(Duration::from_millis(100)));
        assert!(!stats.p95_exceeds(Duration::from_millis(200)));

        assert!(stats.p99_exceeds(Duration::from_millis(100)));
        assert!(!stats.p99_exceeds(Duration::from_millis(200)));
    }

    #[test]
    fn test_metrics_debug() {
        let metrics = Metrics::new("test");
        metrics.record_success(Duration::from_millis(50));

        let debug_str = format!("{:?}", metrics);
        assert!(debug_str.contains("test"));
        assert!(debug_str.contains("total_requests"));
    }

    #[test]
    fn test_latency_stats_default() {
        let stats = LatencyStats::default();
        assert_eq!(stats.min, Duration::ZERO);
        assert_eq!(stats.max, Duration::ZERO);
        assert_eq!(stats.sample_count, 0);
    }

    #[test]
    fn test_metrics_snapshot_requests_per_second() {
        let metrics = Metrics::new("test");

        // Record some requests
        for _ in 0..10 {
            metrics.record_success(Duration::from_millis(10));
        }

        // Sleep briefly to have non-zero uptime
        std::thread::sleep(Duration::from_millis(100));

        let snapshot = metrics.snapshot();
        let rps = snapshot.requests_per_second();

        // Should have recorded 10 requests over ~100ms = ~100 rps
        // Allow wide tolerance due to timing variability
        assert!(rps > 10.0 && rps < 1000.0, "RPS was {}", rps);
    }
}
