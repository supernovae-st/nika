// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Clock trait — time abstraction for deterministic tests.
//!
//! Production: `SystemClock` wraps `tokio::time`.
//! Tests: `MockClock` with `advance(Duration)`.
//!
//! Design decision #2: Clock has `async fn sleep` (the only async method);
//! `now()` and `system_now()` are sync. `trait_variant` generates `ClockDyn`
//! for object-safe usage.

use std::time::{Duration, Instant, SystemTime};

/// Async-safe time abstraction.
///
/// # Examples
///
/// ```rust,ignore
/// fn elapsed_check(clock: &impl Clock) {
///     let start = clock.now();
///     // ... do work ...
///     let dur = clock.elapsed(start);
/// }
/// ```
#[trait_variant::make(ClockDyn: Send)]
pub trait Clock: Send + Sync {
    /// Current monotonic instant.
    fn now(&self) -> Instant;

    /// Current wall-clock time.
    fn system_now(&self) -> SystemTime;

    /// Async sleep for the given duration.
    ///
    /// Production: delegates to `tokio::time::sleep`.
    /// Tests: returns immediately (mock clock advances manually).
    ///
    /// CANCEL SAFETY: cancel-safe. Dropping the future wakes the caller
    /// immediately and releases the timer. No side effects to roll back.
    async fn sleep(&self, duration: Duration);

    /// Elapsed since the given instant.
    fn elapsed(&self, since: Instant) -> Duration {
        self.now().duration_since(since)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal impl to test the trait API surface.
    struct TestClock(Instant);

    impl Clock for TestClock {
        fn now(&self) -> Instant {
            self.0
        }

        fn system_now(&self) -> SystemTime {
            SystemTime::now()
        }

        async fn sleep(&self, _duration: Duration) {
            // no-op in tests
        }

        // elapsed uses default impl
    }

    #[test]
    fn clock_elapsed_default_impl() {
        let base = Instant::now();
        let clock = TestClock(base + Duration::from_millis(100));
        let elapsed = clock.elapsed(base);
        assert_eq!(elapsed, Duration::from_millis(100));
    }

    #[test]
    fn clock_now_returns_instant() {
        let base = Instant::now();
        let clock = TestClock(base);
        assert_eq!(clock.now(), base);
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn test_clock_is_send_sync() {
        _assert_send_sync::<TestClock>();
    }
}
