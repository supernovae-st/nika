// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `DeclaredClock` — the `run: { clock: … }` declaration AS a clock (F-P3).
//!
//! The composer resolves the declaration once and injects the ONE clock
//! everywhere the run reads time (the runtime core · the builtin
//! dispatcher): [`DeclaredClock::System`] is the production wall/monotonic
//! pair, [`DeclaredClock::Virtual`] the pure simulated clock. An enum, not a
//! flag — the two clocks read in one home (the stamper-family
//! precedent), and a third clock lands here when the contract grows one.

use std::time::{Duration, Instant, SystemTime};

use nika_kernel::clock::ClockDyn;

use crate::SystemClock;
use crate::virtual_clock::VirtualClock;

/// The run's clock as declared (F-P3 · `run: { clock: system | virtual }`).
#[derive(Clone)]
#[non_exhaustive]
pub enum DeclaredClock {
    /// The production wall/monotonic clock (the status quo).
    System(SystemClock),
    /// The pure simulated clock (deterministic durations · instant
    /// sleeps — see [`VirtualClock`] for the deadline-race bound).
    Virtual(VirtualClock),
}

impl DeclaredClock {
    /// The system clock (production default).
    #[must_use]
    pub fn system() -> Self {
        Self::System(SystemClock)
    }

    /// A virtual clock at zero (the determinism seam).
    #[must_use]
    pub fn r#virtual() -> Self {
        Self::Virtual(VirtualClock::new())
    }

    /// The virtual half, when virtual — the composer/fixture surface that
    /// inspects the declaration (never a downcast at a call site). The
    /// clock it returns is frozen: nothing advances virtual time.
    #[must_use]
    pub fn as_virtual(&self) -> Option<&VirtualClock> {
        match self {
            Self::Virtual(clock) => Some(clock),
            _ => None,
        }
    }
}

impl ClockDyn for DeclaredClock {
    fn now(&self) -> Instant {
        match self {
            Self::System(clock) => clock.now(),
            Self::Virtual(clock) => clock.now(),
        }
    }

    fn system_now(&self) -> SystemTime {
        match self {
            Self::System(clock) => clock.system_now(),
            Self::Virtual(clock) => clock.system_now(),
        }
    }

    async fn sleep(&self, duration: Duration) {
        match self {
            Self::System(clock) => clock.sleep(duration).await,
            Self::Virtual(clock) => clock.sleep(duration).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_enum_delegates_each_variant() {
        let system = DeclaredClock::system();
        assert!(system.as_virtual().is_none());
        system.sleep(Duration::ZERO).await;

        let virtual_clock = DeclaredClock::r#virtual();
        let clock = virtual_clock.as_virtual().expect("the virtual half");
        let start = clock.now();
        virtual_clock.sleep(Duration::from_secs(60)).await;
        assert_eq!(
            clock.elapsed(start),
            Duration::ZERO,
            "frozen: the sleep did not move virtual time"
        );
        assert_eq!(virtual_clock.system_now(), SystemTime::UNIX_EPOCH);
    }
}
