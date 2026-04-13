// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `MockClock` — deterministic time for tests.

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use parking_lot::Mutex;

use nika_kernel::Clock;

/// A controllable clock for deterministic test timing.
///
/// `advance()` manually moves time forward. `sleep()` returns immediately.
/// Clones share state via `Arc`.
#[derive(Clone)]
pub struct MockClock {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    base: Instant,
    system_base: SystemTime,
    offset: Duration,
}

impl MockClock {
    /// Create a new mock clock starting from the current wall time.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                base: Instant::now(),
                system_base: SystemTime::now(),
                offset: Duration::ZERO,
            })),
        }
    }

    /// Advance time by the given duration.
    pub fn advance(&self, duration: Duration) {
        self.inner.lock().offset += duration;
    }

    /// Total offset since creation.
    #[must_use]
    pub fn elapsed_total(&self) -> Duration {
        self.inner.lock().offset
    }
}

impl Default for MockClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for MockClock {
    fn now(&self) -> Instant {
        let inner = self.inner.lock();
        inner.base + inner.offset
    }

    fn system_now(&self) -> SystemTime {
        let inner = self.inner.lock();
        inner.system_base + inner.offset
    }

    async fn sleep(&self, _duration: Duration) {
        // No-op: mock clock does not actually wait.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_clock_starts_at_zero_offset() {
        let clock = MockClock::new();
        assert_eq!(clock.elapsed_total(), Duration::ZERO);
    }

    #[test]
    fn advance_moves_time_forward() {
        let clock = MockClock::new();
        let t0 = clock.now();
        clock.advance(Duration::from_secs(5));
        let t1 = clock.now();
        assert_eq!(t1.duration_since(t0), Duration::from_secs(5));
    }

    #[test]
    fn elapsed_total_accumulates() {
        let clock = MockClock::new();
        clock.advance(Duration::from_millis(100));
        clock.advance(Duration::from_millis(200));
        assert_eq!(clock.elapsed_total(), Duration::from_millis(300));
    }

    #[test]
    fn clone_shares_state() {
        let clock1 = MockClock::new();
        let clock2 = clock1.clone();
        clock1.advance(Duration::from_secs(1));
        assert_eq!(clock2.elapsed_total(), Duration::from_secs(1));
    }

    #[test]
    fn elapsed_default_impl_works() {
        let clock = MockClock::new();
        let start = clock.now();
        clock.advance(Duration::from_millis(42));
        assert_eq!(clock.elapsed(start), Duration::from_millis(42));
    }

    #[test]
    fn system_now_advances_with_advance() {
        let clock = MockClock::new();
        let before = clock.system_now();
        clock.advance(Duration::from_secs(10));
        let after = clock.system_now();
        assert!(after > before);
        let diff = after.duration_since(before).unwrap();
        assert_eq!(diff, Duration::from_secs(10));
    }

    #[tokio::test]
    async fn sleep_returns_immediately() {
        let clock = MockClock::new();
        clock.sleep(Duration::from_secs(999)).await;
        // If we got here, sleep didn't actually wait.
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn mock_clock_is_send_sync() {
        _assert_send_sync::<MockClock>();
    }
}
