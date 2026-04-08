// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MockClock — deterministic time for tests.

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

/// A mock clock that advances only when explicitly told to.
///
/// All calls to `now()` return the same instant until `advance()` is called.
/// `sleep()` completes immediately (no actual waiting).
#[derive(Clone)]
pub struct MockClock {
    inner: Arc<Mutex<MockClockInner>>,
}

struct MockClockInner {
    base: Instant,
    offset: Duration,
}

impl MockClock {
    /// Create a new MockClock starting at the current real time.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MockClockInner {
                base: Instant::now(),
                offset: Duration::ZERO,
            })),
        }
    }

    /// Advance the clock by the given duration.
    pub fn advance(&self, duration: Duration) {
        self.inner.lock().offset += duration;
    }

    /// Total time elapsed since creation.
    pub fn elapsed_total(&self) -> Duration {
        self.inner.lock().offset
    }
}

impl Default for MockClock {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl nika_kernel::clock::Clock for MockClock {
    fn now(&self) -> Instant {
        let inner = self.inner.lock();
        inner.base + inner.offset
    }

    async fn sleep(&self, _duration: Duration) {
        // Mock clock: sleep completes instantly.
        // Call advance() explicitly to simulate time passing.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::clock::Clock;

    #[tokio::test]
    async fn mock_clock_starts_at_zero_offset() {
        let clock = MockClock::new();
        assert_eq!(clock.elapsed_total(), Duration::ZERO);
    }

    #[tokio::test]
    async fn mock_clock_advance() {
        let clock = MockClock::new();
        let t1 = clock.now();
        clock.advance(Duration::from_secs(5));
        let t2 = clock.now();
        assert_eq!(t2.duration_since(t1), Duration::from_secs(5));
    }

    #[tokio::test]
    async fn mock_clock_sleep_is_instant() {
        let clock = MockClock::new();
        let before = std::time::Instant::now();
        clock.sleep(Duration::from_secs(100)).await;
        assert!(before.elapsed() < Duration::from_millis(10));
    }

    #[tokio::test]
    async fn mock_clock_elapsed_default() {
        let clock = MockClock::new();
        let start = clock.now();
        let elapsed = clock.elapsed(start);
        assert_eq!(elapsed, Duration::ZERO);
    }

    #[tokio::test]
    async fn mock_clock_is_clone() {
        let c1 = MockClock::new();
        let c2 = c1.clone();
        c1.advance(Duration::from_secs(1));
        // Clones share state
        assert_eq!(c2.elapsed_total(), Duration::from_secs(1));
    }
}
