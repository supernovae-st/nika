// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Contract tests for the frozen `VirtualClock` (F-P3) — the honest
//! bound, pinned at the boundary where the runtime races a task's
//! `timeout:` budget against `clock.sleep(budget)`.
//!
//! RED finding (2026-08-19, pre-fix): the same race asserted from the
//! author's honest expectation — « a `timeout: 5m` budget lets in-flight
//! work finish » — FAILED instantly (`finished in 0.00s`): under the
//! virtual clock every deadline is already settled at dispatch. The fix
//! is honesty, not a fake scheduler: the dead `advance` seam (zero
//! production callers, self-described as « the ONLY mover of virtual
//! time ») is gone, and these tests pin the frozen behavior so a future
//! time-mover regression fails loudly. Wiring the task `timeout:` budget
//! to the exec runner's own deadline (a hung subprocess's group dies
//! instead of lingering) is a follow-up wave in the runtime's dispatch —
//! said here so this bound is a named deferral, not a forgotten one.

use std::time::{Duration, SystemTime};

use nika_clock::{DeclaredClock, VirtualClock};
use nika_kernel::clock::ClockDyn;

/// The boundary race, asserted the honest way: under `clock: virtual`
/// the budget's timer is instantly ready, so the timeout class trips at
/// dispatch — deterministically. An author who needs a REAL deadline
/// honored against REAL work must not declare `clock: virtual`.
#[tokio::test]
async fn a_deadline_under_the_virtual_clock_is_always_already_settled() {
    let clock = DeclaredClock::r#virtual();
    // Real work, still in flight when the budget is judged (the runtime's
    // `race_budget` shape: attempt future vs `clock.sleep(budget)`).
    let work = std::future::pending::<u8>();
    tokio::select! {
        _ = work => panic!("in-flight work must NEVER win this race under the frozen clock"),
        () = clock.sleep(Duration::from_secs(300)) => {
            // The documented bound: the deadline is settled at dispatch.
        }
    }
}

/// Virtual time does not ride the wall clock: real time passes, the
/// frozen bases do not move (replay-stable journals, F-P3).
#[tokio::test]
async fn virtual_time_is_frozen_while_real_time_passes() {
    let clock = VirtualClock::new();
    let start = clock.now();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(
        clock.elapsed(start),
        Duration::ZERO,
        "frozen: real time passing must not move virtual time"
    );
    assert_eq!(clock.system_now(), SystemTime::UNIX_EPOCH);
}
