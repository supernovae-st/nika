// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Clock trait — time abstraction for deterministic tests.
//!
//! Wire points: retry/backoff, structured retry, scheduler, rate limiter.

use std::time::{Duration, Instant};

/// Async-safe time abstraction.
///
/// Production: `SystemClock` wraps `tokio::time`.
/// Tests: `MockClock` with `advance(Duration)` for deterministic timing.
#[async_trait::async_trait]
pub trait Clock: Send + Sync {
    /// Current monotonic instant.
    fn now(&self) -> Instant;

    /// Async sleep for the given duration.
    async fn sleep(&self, duration: Duration);

    /// Elapsed since the given instant.
    fn elapsed(&self, since: Instant) -> Duration {
        self.now().duration_since(since)
    }
}
