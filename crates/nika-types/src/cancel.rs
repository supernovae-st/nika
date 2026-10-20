// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cancellation context — structured cancellation for v0.95.
//!
//! `CancelCtx` is a cooperative cancellation token backed by one shared
//! atomic state. All clones share it. When any clone calls `cancel()`,
//! all clones observe `is_cancelled() == true`.
//!
//! A [`CancelCtx::child`] is cancelled with the context it derives from, or
//! on its own; its own cancellation leaves that context alone. A context can
//! also guard one effect ([`CancelCtx::begin`]): the effect begins only while
//! the context is not cancelled, and [`CancelCtx::cancel_begun`] says
//! whether it had begun, exactly, because both read one atomic.
//!
//! Today: used as a forward-compat field on `InferRequest.cancel`.
//! v0.95: wired to DAG propagation — parent cancel cascades to children.
//!
//! Uses `std::sync::atomic` (not `tokio_util::CancellationToken`) to keep
//! the kernel free of tokio dependency. L0.5 must remain runtime-agnostic.
//! See ADR-016.

// Loom shadow: when `cfg(loom)` is active (test-only), we use loom's
// instrumented atomics + Arc so the same `CancelCtx` can run under
// `loom::model(...)` for interleaving-exhaustive concurrent testing.
// Production + non-loom tests use the stdlib primitives.
#[cfg(not(loom))]
use alloc::sync::Arc;
#[cfg(not(loom))]
use core::sync::atomic::{AtomicU8, Ordering};

#[cfg(loom)]
use loom::sync::Arc;
#[cfg(loom)]
use loom::sync::atomic::{AtomicU8, Ordering};

/// The state bit [`CancelCtx::cancel`] sets.
const CANCELLED: u8 = 1;
/// The state bit [`CancelCtx::begin`] sets.
const BEGUN: u8 = 2;

/// Opaque cancellation context.
///
/// Passed through `InferRequest.cancel` to allow cooperative cancellation.
/// All clones share state: cancelling one cancels all.
///
/// # Examples
///
/// ```rust
/// use nika_types::cancel::CancelCtx;
///
/// let ctx = CancelCtx::new();
/// let clone = ctx.clone();
/// assert!(!ctx.is_cancelled());
///
/// clone.cancel();
/// assert!(ctx.is_cancelled());
///
/// // A child is cancelled with its parent; its own cancel stays its own.
/// let run = CancelCtx::new();
/// let call = run.child();
/// call.cancel();
/// assert!(!run.is_cancelled());
/// run.cancel();
/// assert!(run.child().is_cancelled());
/// ```
#[derive(Debug, Clone)]
pub struct CancelCtx {
    /// [`CANCELLED`] and [`BEGUN`], on one atomic so that [`Self::begin`]
    /// and [`Self::cancel_begun`] order exactly.
    state: Arc<AtomicU8>,
    /// The context this one derives from ([`Self::child`]): its cancellation
    /// is this one's too.
    parent: Option<Arc<CancelCtx>>,
}

impl CancelCtx {
    /// Create a new cancellation context (not cancelled).
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(AtomicU8::new(0)),
            parent: None,
        }
    }

    /// A context cancelled with this one, or on its own: cancelling the
    /// child leaves this one alone. A run gives each call it waits for a
    /// child, so the run's stop reaches every call while a call abandoned
    /// alone stops alone.
    #[must_use]
    pub fn child(&self) -> Self {
        Self {
            state: Arc::new(AtomicU8::new(0)),
            parent: Some(Arc::new(self.clone())),
        }
    }

    /// Check whether cancellation has been requested.
    ///
    /// Uses `Ordering::Acquire`: pairs with the `Release` store in
    /// [`Self::cancel`] to establish a happens-before edge, so any data a
    /// producer writes *before* signalling cancel is visible to consumers
    /// that observe `is_cancelled() == true`. Zero cost on x86-64 TSO;
    /// required for ADR-016 v0.95 DAG cancel propagation, where the parent
    /// writes final state then flips the flag.
    /// A child also reads the context it derives from.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.state.load(Ordering::Acquire) & CANCELLED != 0
            || self.parent.as_deref().is_some_and(Self::is_cancelled)
    }

    /// Request cancellation. All clones of this context will observe it.
    ///
    /// Uses `Ordering::Release` so any data written before this call is
    /// visible to any consumer that observes `is_cancelled() == true` via
    /// [`Self::is_cancelled`].
    pub fn cancel(&self) {
        self.state.fetch_or(CANCELLED, Ordering::Release);
    }

    /// Begin the one effect this context guards (a request written to a
    /// server) unless the context is already cancelled: `true` lets the
    /// caller go ahead. Exact against a concurrent [`Self::cancel_begun`] on
    /// the same context: one of them comes first, so a cancel that found the
    /// effect not begun is never followed by it.
    #[must_use]
    pub fn begin(&self) -> bool {
        if self.parent.as_deref().is_some_and(Self::is_cancelled) {
            return false;
        }
        let mut current = self.state.load(Ordering::Acquire);
        loop {
            if current & CANCELLED != 0 {
                return false;
            }
            match self.state.compare_exchange(
                current,
                current | BEGUN,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(actual) => current = actual,
            }
        }
    }

    /// [`Self::cancel`], saying whether [`Self::begin`] had already let the
    /// guarded effect go ahead on this context. `false` means it never will.
    #[must_use]
    pub fn cancel_begun(&self) -> bool {
        self.state.fetch_or(CANCELLED, Ordering::AcqRel) & BEGUN != 0
    }
}

impl Default for CancelCtx {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[cfg(not(loom))]
mod tests {
    use super::*;

    #[test]
    fn cancel_ctx_starts_not_cancelled() {
        let ctx = CancelCtx::new();
        assert!(!ctx.is_cancelled());
    }

    #[test]
    fn cancel_ctx_cancel_propagates_to_clones() {
        let ctx = CancelCtx::new();
        let clone = ctx.clone();
        ctx.cancel();
        assert!(ctx.is_cancelled());
        assert!(clone.is_cancelled());
    }

    #[test]
    fn cancel_ctx_default_is_not_cancelled() {
        let ctx = CancelCtx::default();
        assert!(!ctx.is_cancelled());
    }

    #[test]
    fn cancel_ctx_multiple_clones_share_state() {
        let ctx = CancelCtx::new();
        let c1 = ctx.clone();
        let c2 = ctx.clone();
        let c3 = c1.clone();

        // Cancel from a deeply-cloned instance
        c3.cancel();

        assert!(ctx.is_cancelled());
        assert!(c1.is_cancelled());
        assert!(c2.is_cancelled());
        assert!(c3.is_cancelled());
    }

    #[test]
    fn cancel_ctx_idempotent_cancel() {
        let ctx = CancelCtx::new();
        ctx.cancel();
        ctx.cancel(); // double cancel is fine
        assert!(ctx.is_cancelled());
    }

    #[test]
    fn a_child_is_cancelled_with_its_parent_and_never_cancels_it() {
        let run = CancelCtx::new();
        let call = run.child();
        let twin = call.clone();
        call.cancel();
        assert!(twin.is_cancelled(), "clones of a child share its state");
        assert!(!run.is_cancelled(), "a child's cancel stays its own");
        let other = run.child();
        let grandchild = other.child();
        assert!(!grandchild.is_cancelled());
        run.cancel();
        assert!(other.is_cancelled());
        assert!(
            grandchild.is_cancelled(),
            "the root's cancel reaches every descendant"
        );
    }

    #[test]
    fn an_effect_begins_only_before_a_cancel_and_the_cancel_says_so() {
        let begun = CancelCtx::new();
        assert!(begun.begin());
        assert!(begun.begin(), "a begun effect stays begun");
        assert!(!begun.is_cancelled(), "beginning cancels nothing");
        assert!(begun.cancel_begun(), "the cancel finds it begun");
        assert!(begun.is_cancelled());
        assert!(begun.cancel_begun(), "a second cancel still finds it begun");

        let refused = CancelCtx::new();
        assert!(!refused.cancel_begun(), "nothing had begun");
        assert!(refused.is_cancelled(), "cancel_begun cancels");
        assert!(!refused.begin(), "a cancelled context begins nothing");
        assert!(!refused.cancel_begun(), "and the refusal left it unbegun");

        let plain = CancelCtx::new();
        plain.cancel();
        assert!(!plain.begin());
    }

    #[test]
    fn a_child_begins_nothing_once_its_parent_is_cancelled() {
        let run = CancelCtx::new();
        let call = run.child();
        run.cancel();
        assert!(!call.begin());
        assert!(!call.cancel_begun(), "the call never began");
        let before = CancelCtx::new();
        let started = before.child();
        assert!(started.begin());
        before.cancel();
        assert!(started.is_cancelled());
        assert!(started.cancel_begun(), "begun before the parent's cancel");
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn cancel_ctx_is_send_sync() {
        _assert_send_sync::<CancelCtx>();
    }
}

#[cfg(test)]
#[cfg(loom)]
mod loom_cancel;
