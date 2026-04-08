// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Production [`Clock`] implementation via `tokio::time`.
//!
//! This crate sits at L1 in the dependency graph — it implements the
//! [`nika_kernel::clock::Clock`] trait using real system time.
//!
//! # Example
//!
//! ```rust
//! use nika_clock::SystemClock;
//! use nika_kernel::clock::Clock;
//! use std::time::Duration;
//!
//! let clock = SystemClock;
//! let start = clock.now();
//! // ... work ...
//! let elapsed = clock.elapsed(start);
//! ```

use std::time::{Duration, Instant};

use nika_kernel::clock::Clock;

/// Production clock backed by `std::time::Instant` and `tokio::time::sleep`.
///
/// Zero-size type — no allocation, no state.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

#[async_trait::async_trait]
impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_returns_monotonic_instant() {
        let clock = SystemClock;
        let t1 = clock.now();
        let t2 = clock.now();
        assert!(t2 >= t1);
    }

    #[tokio::test]
    async fn sleep_advances_time() {
        let clock = SystemClock;
        let start = clock.now();
        clock.sleep(Duration::from_millis(10)).await;
        let elapsed = clock.elapsed(start);
        assert!(elapsed >= Duration::from_millis(10));
    }

    #[test]
    fn elapsed_measures_duration() {
        let clock = SystemClock;
        let start = clock.now();
        // Spin briefly to ensure measurable elapsed time
        std::hint::black_box(0..1000).for_each(|_| {});
        let elapsed = clock.elapsed(start);
        // Should be non-negative (monotonic)
        assert!(elapsed >= Duration::ZERO);
    }

    #[test]
    fn is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SystemClock>();
    }

    #[test]
    fn is_zero_sized() {
        assert_eq!(std::mem::size_of::<SystemClock>(), 0);
    }
}
