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
//! RUSTFLAGS="--cfg loom" cargo test --locked -p nika-types --lib loom_cancel
//! ```
//!
//! Only the cancellation primitive and the separate payload are instrumented.
//! This model does not prove scheduler progress, process shutdown or effects.

use crate::cancel::CancelCtx;
use loom::{
    sync::Arc,
    sync::atomic::{AtomicUsize, Ordering},
    thread,
};

/// A clone's cancellation is visible after its thread is joined. The join
/// synchronizes this assertion; it does not test publication by the flag.
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

/// Three clones share cancellation state after the writer is joined.
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

/// Observing cancellation publishes a preceding relaxed payload write.
/// The assertion races with the writer, before join can synchronize it.
/// Weakening either `CancelCtx` ordering to Relaxed must fail this model.
#[test]
fn cancellation_publishes_preceding_payload() {
    loom::model(|| {
        let ctx = CancelCtx::new();
        let writer_ctx = ctx.clone();
        let payload = Arc::new(AtomicUsize::new(0));
        let writer_payload = payload.clone();

        let writer = thread::spawn(move || {
            writer_payload.store(1, Ordering::Relaxed);
            writer_ctx.cancel();
        });

        if ctx.is_cancelled() {
            assert_eq!(
                payload.load(Ordering::Relaxed),
                1,
                "observed cancellation before its preceding payload"
            );
        }

        writer.join().unwrap();
    });
}
