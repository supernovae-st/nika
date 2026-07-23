// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `VirtualClock` — the pure simulated clock behind `run: { clock:
//! virtual }` (F-P3) and behind `entropy: none | seeded(N)`, which imply
//! it: a run whose journals must replay byte-identical cannot let task
//! durations ride the wall clock.
//!
//! The discipline is the FDB/VOPR one — one run = ONE clock, and under a
//! determinism declaration that clock is simulated:
//!
//! - **Monotonic** — a base [`Instant`] captured at construction plus a
//!   shared offset; `now()` moves ONLY via [`VirtualClock::advance`], so
//!   measured durations are exact (never the sub-millisecond scheduling
//!   jitter two real `Instant::now()` calls would read).
//! - **Wall** — `system_now()` reads `system_base + offset` with the
//!   base at the Unix EPOCH by default (replay-stable: a journaled wall
//!   value never depends on when the run started).
//!   [`VirtualClock::with_system_base`] moves the zero when a fixture
//!   wants a recognizable one.
//! - **`sleep` returns instantly and does NOT advance the offset** — the
//!   pure-mock choice, NOT `tokio::time::pause`. The engine never drives
//!   a scheduler; it observes a clock. The bound this choice carries,
//!   said honestly: a task `timeout:` budget races an instantly-ready
//!   timer, so under the virtual clock any deadline is already settled
//!   (the timeout class trips at dispatch). Deterministic — two runs
//!   read the same outcome — but an author who needs a REAL deadline
//!   honored against REAL work must not declare `clock: virtual`.
//!
//! Clones share the offset (`Arc`) — the run's one clock stays one
//! clock across every seam it is injected into.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use nika_kernel::clock::ClockDyn;

/// The production virtual clock (F-P3) — deterministic time for a run
/// that declares it.
#[derive(Clone)]
#[non_exhaustive]
pub struct VirtualClock {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    base: Instant,
    system_base: SystemTime,
    offset: Duration,
}

impl VirtualClock {
    /// A virtual clock at zero: monotonic base captured now (durations
    /// measure the shared offset only), wall base at the Unix epoch
    /// (replay-stable wall reads).
    #[must_use]
    pub fn new() -> Self {
        Self::with_system_base(SystemTime::UNIX_EPOCH)
    }

    /// A virtual clock whose wall zero is `system_base` (the fixture
    /// picks its epoch — the offset discipline is unchanged).
    #[must_use]
    pub fn with_system_base(system_base: SystemTime) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                base: Instant::now(),
                system_base,
                offset: Duration::ZERO,
            })),
        }
    }

    /// Advance the shared offset — the ONLY mover of virtual time.
    pub fn advance(&self, duration: Duration) {
        self.shared().offset += duration;
    }

    /// The total advance since construction.
    #[must_use]
    pub fn elapsed_total(&self) -> Duration {
        self.shared().offset
    }

    /// The shared state, poison-tolerant (a panicking holder must not
    /// wedge the run's one clock — the offset stays readable).
    fn shared(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Default for VirtualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl ClockDyn for VirtualClock {
    fn now(&self) -> Instant {
        let inner = self.shared();
        inner.base + inner.offset
    }

    fn system_now(&self) -> SystemTime {
        let inner = self.shared();
        inner.system_base + inner.offset
    }

    async fn sleep(&self, _duration: Duration) {
        // The pure mock: instant, non-advancing (never tokio::time::pause
        // — see the module doc for the deadline-race bound this carries).
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_are_exact_zero_without_advance() {
        let clock = VirtualClock::new();
        let start = clock.now();
        assert_eq!(clock.elapsed(start), Duration::ZERO);
        assert_eq!(clock.elapsed_total(), Duration::ZERO);
    }

    #[test]
    fn wall_reads_start_at_the_epoch() {
        let clock = VirtualClock::new();
        assert_eq!(clock.system_now(), SystemTime::UNIX_EPOCH);
        clock.advance(Duration::from_secs(30));
        assert_eq!(
            clock.system_now(),
            SystemTime::UNIX_EPOCH + Duration::from_secs(30)
        );
    }

    #[test]
    fn advance_is_shared_across_clones() {
        let clock = VirtualClock::new();
        let twin = clock.clone();
        clock.advance(Duration::from_millis(40));
        assert_eq!(twin.elapsed_total(), Duration::from_millis(40));
        let start = twin.now();
        clock.advance(Duration::from_millis(2));
        assert_eq!(twin.elapsed(start), Duration::from_millis(2));
    }

    #[tokio::test]
    async fn sleep_is_instant_and_non_advancing() {
        let clock = VirtualClock::new();
        clock.sleep(Duration::from_secs(60)).await;
        assert_eq!(
            clock.elapsed_total(),
            Duration::ZERO,
            "the pure mock never moves time"
        );
    }
}
