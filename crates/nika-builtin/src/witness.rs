// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The builtin arm's per-op permit witness slot (NEP-0007 law 2 — the
//! declared v1 residual, closed 2026-08-09): the fs boundary's
//! enforcement decisions record here, granted AND refused alike, and
//! the settle spine emits one `permit_checked` frame per decision with
//! `plane: "fs"`. The record happens AT the enforcement point, never
//! dispatch-side — a pre-witness could lie against
//! canonicalize-then-confine (the witness crate's own warning).
//!
//! The collector rides a tokio task-local scoped by the runtime around
//! each attempt ([`scope_attempt_witness`]): the boundary is reached
//! through a shared, `Arc`-composed dispatcher the per-attempt witness
//! cannot be threaded through without breaking the kernel's
//! `ToolExecute` seam — the task-local is the one channel that follows
//! the attempt across `.await` points (a thread-local would not: tokio
//! tasks migrate between threads at every await). Outside a scoped
//! attempt (unit tests · the floor · non-run callers) recording is a
//! no-op: telemetry never panics a run.

use std::sync::Arc;

use nika_cap::PermitWitness;

tokio::task_local! {
    /// The current attempt's collector — set by the runtime's per-
    /// attempt scope, read at the boundary's enforcement point.
    static ATTEMPT_WITNESS: Arc<PermitWitness>;
}

/// Scope one attempt's collector over a future. The runtime wraps every
/// attempt lane with this (the main attempt · the `on_finally:` cleanup
/// · the fan-out item), so a decision taken at the fs boundary binds to
/// the task that took it when the settle spine drains the collector.
pub async fn scope_attempt_witness<F>(witness: Arc<PermitWitness>, fut: F) -> F::Output
where
    F: std::future::Future,
{
    ATTEMPT_WITNESS.scope(witness, fut).await
}

/// Record one boundary decision into the attempt's collector. A no-op
/// outside a scoped attempt (`try_with` errs on an unset slot) — the
/// witness is best-effort by law, never a run failure mode.
pub(crate) fn record_decision(
    plane: &'static str,
    gate: String,
    decision: &'static str,
    why: String,
) {
    let _ = ATTEMPT_WITNESS.try_with(|w| w.record(plane, gate, decision, why));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scoped, the boundary's decision lands in the attempt's collector.
    #[tokio::test]
    async fn a_scoped_record_reaches_the_collector() {
        let witness = Arc::new(PermitWitness::new());
        scope_attempt_witness(witness.clone(), async {
            record_decision(
                "fs",
                "fs.read ./allowed.txt".to_owned(),
                "allow",
                "the effective identity stays inside the declared set".to_owned(),
            );
        })
        .await;
        let taken = witness.take();
        assert_eq!(taken.len(), 1);
        assert_eq!(taken[0].plane, "fs");
        assert_eq!(taken[0].decision, "allow");
    }

    /// Unscoped, recording is a silent no-op — telemetry never panics.
    #[tokio::test]
    async fn an_unscoped_record_is_a_noop() {
        record_decision("fs", "gate".to_owned(), "deny", "why".to_owned());
    }

    /// Two parallel attempts never cross their collectors — the
    /// task-local binds each decision to its own attempt.
    #[tokio::test]
    async fn parallel_attempts_never_cross_their_collectors() {
        let a = Arc::new(PermitWitness::new());
        let b = Arc::new(PermitWitness::new());
        let fa = scope_attempt_witness(a.clone(), async {
            tokio::task::yield_now().await;
            record_decision("fs", "gate-a".to_owned(), "allow", "a".to_owned());
        });
        let fb = scope_attempt_witness(b.clone(), async {
            tokio::task::yield_now().await;
            record_decision("fs", "gate-b".to_owned(), "deny", "b".to_owned());
        });
        tokio::join!(fa, fb);
        let ta = a.take();
        let tb = b.take();
        assert_eq!(ta.len(), 1, "a holds exactly its own decision");
        assert_eq!(tb.len(), 1, "b holds exactly its own decision");
        assert_eq!(ta[0].gate, "gate-a");
        assert_eq!(tb[0].gate, "gate-b");
    }
}
