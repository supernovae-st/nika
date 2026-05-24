// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-clock` — the production [`Clock`] implementation for the Nika diamond.
//!
//! This crate sits at **L1** (effect crate): it implements the L0.5
//! [`nika_kernel::Clock`] trait using real system time. Every crate that
//! needs the wall/monotonic clock or `sleep` injects `&dyn Clock` and
//! receives [`SystemClock`] in production, a mock in tests — the kernel
//! contract keeps the engine hermetic (Invariant #27).
//!
//! ```rust
//! use nika_clock::SystemClock;
//! use nika_kernel::Clock;
//!
//! let clock = SystemClock;
//! let start = clock.now();
//! // ... work ...
//! let _elapsed = clock.elapsed(start); // monotonic, never negative
//! ```
//!
//! # Why a whole crate for a 3-method impl
//!
//! L1 isolation: `SystemClock` is the **only** place `tokio::time` and
//! `std::time::{Instant, SystemTime}` are touched on the production path.
//! Pure crates (L0) and the kernel (L0.5) stay clock-free; tests use the
//! mock. This is the effect-crate discipline — one trait impl per crate.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::time::{Duration, Instant, SystemTime};

use nika_kernel::Clock;

/// Production clock backed by `std::time` (monotonic + wall) and
/// `tokio::time::sleep` (async). Zero-size — no allocation, no state,
/// trivially `Copy`/`Default`.
///
/// Satisfies both [`nika_kernel::Clock`] and the generated
/// `nika_kernel::ClockDyn` (object-safe `Send` variant) via the
/// `trait_variant` blanket impl, so `&dyn ClockDyn` fan-out works.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn system_now(&self) -> SystemTime {
        SystemTime::now()
    }

    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }

    // `elapsed` uses the trait default (`now().duration_since(since)`).
}
