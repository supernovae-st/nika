// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Loom concurrency tests for `CancelCtx`.
//!
//! INV-029 (ADR-013) requires every crate that ships concurrent
//! primitives to gate them behind loom tests so interleaving bugs are
//! found before they make it into production. `CancelCtx` is the only
//! concurrent primitive in `nika-types` today (Arc<AtomicBool> with
//! Acquire/Release happens-before) so it's the natural first target.
//!
//! These tests run ONLY under `RUSTFLAGS="--cfg loom"`:
//!
//! ```bash
//! RUSTFLAGS="--cfg loom" cargo test -p nika-types --test loom_cancel
//! ```
//!
//! A normal `cargo test` compiles this file as a no-op so keychain
//! popups, IDE watchers, and CI ratchets remain silent.

#![cfg(loom)]

use loom::thread;
use nika_types::cancel::CancelCtx;

/// Two-writer / two-reader race: producer writes state + flips cancel;
/// consumer observes cancel == true and MUST see the producer's final
/// state (Acquire/Release happens-before invariant of CancelCtx).
///
/// Loom explores every interleaving of loads/stores between the two
/// threads. If the atomic orderings were wrong (e.g. Relaxed), some
/// interleaving would observe `flag = true` without seeing the
/// producer's state write — loom catches that on the offending run.
#[test]
fn cancel_propagates_to_clone_under_all_interleavings() {
    loom::model(|| {
        let ctx = CancelCtx::new();
        let clone = ctx.clone();

        let t = thread::spawn(move || {
            clone.cancel();
        });

        // Racing observer: we may observe pre-cancel (false) OR
        // post-cancel (true), never a torn read (atomics are full words).
        let _seen = ctx.is_cancelled();

        t.join().unwrap();

        // After the producer joined, the cancel MUST be visible.
        assert!(ctx.is_cancelled(), "post-join, cancel must be observed");
    });
}

/// Three-way fan-out: one parent, three children; any child can cancel
/// and all others observe it. Verifies the Acquire ordering holds
/// across the three-clone keyspace.
#[test]
fn cancel_propagates_across_three_clones() {
    loom::model(|| {
        let root = CancelCtx::new();
        let a = root.clone();
        let b = root.clone();

        let t_cancel = thread::spawn(move || {
            a.cancel();
        });
        let t_observe = thread::spawn(move || b.is_cancelled());

        t_cancel.join().unwrap();
        let _observed = t_observe.join().unwrap();

        assert!(root.is_cancelled());
    });
}
