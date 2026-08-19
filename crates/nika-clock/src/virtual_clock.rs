// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `VirtualClock` — the frozen simulated clock behind `run: { clock:
//! virtual }` (F-P3) and behind `entropy: none | seeded(N)`, which imply
//! it: a run whose journals must replay byte-identical cannot let task
//! durations ride the wall clock.
//!
//! The discipline is the FDB/VOPR one — one run = ONE clock, and under a
//! determinism declaration that clock is FROZEN. The engine observes the
//! clock; nothing drives it, and nothing may pretend otherwise:
//!
//! - **Monotonic** — `now()` returns the base [`Instant`] captured at
//!   construction, forever, so measured durations are exact ZERO (never
//!   the sub-millisecond scheduling jitter two real `Instant::now()`
//!   calls would read). There is NO time mover — an `advance` seam with
//!   zero production callers was removed (it documented itself as the
//!   engine of virtual time while virtual time never moved).
//! - **Wall** — `system_now()` reads the base at the Unix EPOCH by
//!   default (replay-stable: a journaled wall value never depends on
//!   when the run started). [`VirtualClock::with_system_base`] moves the
//!   zero when a fixture wants a recognizable one.
//! - **`sleep` returns instantly** — the pure-mock choice, NOT
//!   `tokio::time::pause`. The bound this choice carries, said honestly:
//!   a task `timeout:` budget races an instantly-ready timer, so under
//!   the virtual clock any deadline is already settled (the timeout
//!   class trips at dispatch). Deterministic — two runs read the same
//!   outcome — but an author who needs a REAL deadline honored against
//!   REAL work must not declare `clock: virtual`.
//!
//! Clones read the same frozen bases (`Copy`) — the run's one clock
//! stays one clock across every seam it is injected into.

use std::time::{Duration, Instant, SystemTime};

use nika_kernel::clock::ClockDyn;

/// The production virtual clock (F-P3) — deterministic frozen time for a
/// run that declares it.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct VirtualClock {
    base: Instant,
    system_base: SystemTime,
}

impl VirtualClock {
    /// A virtual clock at zero: monotonic base captured now (durations
    /// measure as exact zero), wall base at the Unix epoch (replay-stable
    /// wall reads).
    #[must_use]
    pub fn new() -> Self {
        Self::with_system_base(SystemTime::UNIX_EPOCH)
    }

    /// A virtual clock whose wall zero is `system_base` (the fixture
    /// picks its epoch — the frozen discipline is unchanged).
    #[must_use]
    pub fn with_system_base(system_base: SystemTime) -> Self {
        Self {
            base: Instant::now(),
            system_base,
        }
    }
}

impl Default for VirtualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl ClockDyn for VirtualClock {
    fn now(&self) -> Instant {
        self.base
    }

    fn system_now(&self) -> SystemTime {
        self.system_base
    }

    async fn sleep(&self, _duration: Duration) {
        // The pure mock: instant (never tokio::time::pause — see the
        // module doc for the deadline-race bound this carries).
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_are_exact_zero_forever() {
        let clock = VirtualClock::new();
        let start = clock.now();
        assert_eq!(clock.elapsed(start), Duration::ZERO);
        assert_eq!(clock.now(), start, "frozen: now() never moves");
    }

    #[test]
    fn wall_reads_stay_at_the_epoch() {
        let clock = VirtualClock::new();
        assert_eq!(clock.system_now(), SystemTime::UNIX_EPOCH);
        let recognisable = VirtualClock::with_system_base(
            SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
        );
        assert_eq!(
            recognisable.system_now(),
            SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
            "the fixture-picked zero, frozen"
        );
    }

    #[test]
    fn clones_read_the_same_frozen_bases() {
        let clock = VirtualClock::new();
        let twin = clock;
        assert_eq!(twin.now(), clock.now());
        assert_eq!(twin.system_now(), clock.system_now());
    }

    #[tokio::test]
    async fn sleep_is_instant_and_time_stays_frozen() {
        let clock = VirtualClock::new();
        let start = clock.now();
        clock.sleep(Duration::from_secs(60)).await;
        assert_eq!(clock.elapsed(start), Duration::ZERO, "nothing moves time");
        assert_eq!(clock.system_now(), SystemTime::UNIX_EPOCH);
    }
}
