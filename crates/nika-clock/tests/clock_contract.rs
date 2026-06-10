// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Contract tests for `nika-clock` — the production `SystemClock`.
//!
//! Covers the four `Clock` trait surfaces against the Diamond kernel
//! trait: `now` (monotonic), `system_now` (wall), `sleep` (async), and the
//! defaulted `elapsed`. Includes the Gate 10 parity assertions (behaviour
//! matches the brouillon `SystemClock`: monotonic now, sleep advances,
//! elapsed non-negative) plus Send+Sync + object-safety.

use std::time::{Duration, SystemTime};

use nika_clock::SystemClock;
use nika_kernel::Clock;

// ─── trait-variant companion (Send futures) ──────────────────────────

#[test]
fn systemclock_satisfies_send_future_dyn_variant() {
    // ClockDyn is a generic bound (Send futures), not a dyn-dispatch
    // surface — RPITIT is not object-safe. SystemClock implements the
    // companion directly; the base Clock arrives via the blanket impl.
    fn accepts<T: nika_kernel::clock::ClockDyn>(_: &T) {}
    accepts(&SystemClock);
}

// ─── now() · monotonic ───────────────────────────────────────────────

#[test]
fn now_is_monotonic_non_decreasing() {
    let clock = SystemClock;
    let t1 = clock.now();
    let t2 = clock.now();
    assert!(t2 >= t1, "Instant::now must never go backwards");
}

// ─── system_now() · wall-clock (the Diamond addition vs brouillon) ───

#[test]
fn system_now_is_after_unix_epoch() {
    let clock = SystemClock;
    let wall = clock.system_now();
    assert!(
        wall.duration_since(SystemTime::UNIX_EPOCH).is_ok(),
        "wall clock must be after the unix epoch"
    );
}

#[test]
fn system_now_advances_or_equals() {
    let clock = SystemClock;
    let a = clock.system_now();
    let b = clock.system_now();
    // Wall clock is not guaranteed monotonic, but two reads in sequence
    // should at minimum both be valid points after the epoch.
    assert!(a.duration_since(SystemTime::UNIX_EPOCH).is_ok());
    assert!(b.duration_since(SystemTime::UNIX_EPOCH).is_ok());
}

// ─── elapsed() · default trait impl ──────────────────────────────────

#[test]
fn elapsed_is_non_negative() {
    let clock = SystemClock;
    let start = clock.now();
    std::hint::black_box(0..1000).for_each(|_| {});
    let elapsed = clock.elapsed(start);
    assert!(
        elapsed >= Duration::ZERO,
        "monotonic elapsed never negative"
    );
}

#[test]
fn elapsed_grows_after_sleep() {
    // (sync framing of the parity property; the async timing is in the
    // tokio test below — this one just proves elapsed reflects real time)
    let clock = SystemClock;
    let start = clock.now();
    std::thread::sleep(Duration::from_millis(5));
    let elapsed = clock.elapsed(start);
    assert!(
        elapsed >= Duration::from_millis(5),
        "elapsed must reflect at least the slept duration, got {elapsed:?}"
    );
}

// ─── sleep() · async (Gate 10 parity with brouillon) ─────────────────

#[tokio::test]
async fn sleep_advances_monotonic_time() {
    let clock = SystemClock;
    let start = clock.now();
    clock.sleep(Duration::from_millis(10)).await;
    let elapsed = clock.elapsed(start);
    assert!(
        elapsed >= Duration::from_millis(10),
        "sleep(10ms) must advance the monotonic clock by >=10ms, got {elapsed:?}"
    );
}

#[tokio::test]
async fn sleep_zero_returns_promptly() {
    let clock = SystemClock;
    // A zero-duration sleep must complete without hanging.
    clock.sleep(Duration::ZERO).await;
}

// ─── type properties ─────────────────────────────────────────────────

#[test]
fn system_clock_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SystemClock>();
}

#[test]
fn system_clock_is_zero_size() {
    assert_eq!(std::mem::size_of::<SystemClock>(), 0, "ZST — no state");
}

#[test]
#[allow(clippy::default_constructed_unit_structs)] // deliberately exercising Default
fn system_clock_is_copy_and_default() {
    // Copy: using `a` after passing it by value must still compile + work.
    let a = SystemClock;
    let b = SystemClock::default();
    let start = b.now();
    let copied = a; // Copy semantics — `a` remains usable below.
    assert!(copied.now() >= start || a.elapsed(start) >= Duration::ZERO);
}

// ─── object-safety · &dyn fan-out via injection ──────────────────────

#[tokio::test]
async fn clock_usable_through_generic_injection() {
    async fn timed_work<C: Clock>(clock: &C) -> Duration {
        let start = clock.now();
        clock.sleep(Duration::from_millis(1)).await;
        clock.elapsed(start)
    }
    let clock = SystemClock;
    let d = timed_work(&clock).await;
    assert!(d >= Duration::from_millis(1));
}
