// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cancellation context — structured cancellation for v0.95.
//!
//! `CancelCtx` is a cooperative cancellation token backed by `Arc<AtomicBool>`.
//! All clones share the same cancellation state. When any clone calls `cancel()`,
//! all clones observe `is_cancelled() == true`.
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
use core::sync::atomic::{AtomicBool, Ordering};

#[cfg(loom)]
use loom::sync::Arc;
#[cfg(loom)]
use loom::sync::atomic::{AtomicBool, Ordering};

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
/// ```
#[derive(Debug, Clone)]
pub struct CancelCtx {
    cancelled: Arc<AtomicBool>,
}

impl CancelCtx {
    /// Create a new cancellation context (not cancelled).
    #[must_use]
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
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
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Request cancellation. All clones of this context will observe it.
    ///
    /// Uses `Ordering::Release` so any data written before this call is
    /// visible to any consumer that observes `is_cancelled() == true` via
    /// [`Self::is_cancelled`].
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
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

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn cancel_ctx_is_send_sync() {
        _assert_send_sync::<CancelCtx>();
    }
}

#[cfg(test)]
#[cfg(loom)]
mod loom_cancel;
