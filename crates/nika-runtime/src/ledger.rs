// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run's spend ledger — REAL metered spend folded at the leaf
//! (attempt-loop success · fan-out iterations debit individually, the
//! parent's sum is presentation-only, so nothing double-counts), and
//! the `--max-cost-usd` budget gate.
//!
//! Budget semantics (the industry's block-before shape, 2026 survey):
//! once the ledger trips, the run stops ADMITTING new work — in-flight
//! work completes and counts (a cancelled provider call would spend
//! real money and drop it from the record — the one thing this ledger
//! must never do). Unstarted tasks settle `cancelled`; the terminal
//! frame carries spent-vs-budget (NIKA-1704).
//!
//! Precision stance: sums are `f64` BY CHOICE — the trace wire is f64
//! JSON and these are display/gate-grade totals (~1e-13 relative error
//! at run scale), not invoices. The nano-USD [`nika_types::cost::Cost`]
//! newtype exists for billing-grade ledgers; migrating this fold to it
//! is deliberate future work, not an oversight. The budget comparison
//! rounds both sides to micro-USD so accumulation dust one ULP over the
//! nominal budget can never print « spent $X of $X — exceeded ».
//!
//! Known unmetered classes (documented, not silent): a provider
//! round-trip that BILLS but whose verb then fails (schema repair
//! exhausted · an agent erroring after N absorbed turns) reports no
//! usage on the error path today — invisible to totals AND to the
//! gate; threading usage through verb errors is the committed
//! follow-up. `on_finally` cleanups ride the best-effort lane
//! (outcome dropped by design) and never debit.

use std::collections::BTreeMap;
use std::sync::Mutex;

/// Shared per-run spend accumulator (waves dispatch concurrently — the
/// leaf debits race; a Mutex over the tiny fold is the whole story).
pub(crate) struct RunLedger {
    budget: Option<f64>,
    inner: Mutex<LedgerInner>,
}

#[derive(Default)]
struct LedgerInner {
    spent_usd: f64,
    any_priced: bool,
    priced_calls: u32,
    unpriced_calls: u32,
    tripped: bool,
    by_source: BTreeMap<String, f64>,
}

/// A point-in-time copy for the terminal frame + [`crate::RunOutcome`].
pub(crate) struct LedgerSnapshot {
    pub spent_usd: f64,
    /// Whether ANY leaf reported real spend — the totals only ride the
    /// terminal frame when true (a mock/local run stays field-free:
    /// absent is honest, a `total_cost_usd: 0.0` we never metered is not).
    pub any_priced: bool,
    pub priced_calls: u32,
    pub unpriced_calls: u32,
    pub tripped: bool,
    pub budget: Option<f64>,
    /// Spend per attribution key (`provider/model` · tool id).
    pub by_source: BTreeMap<String, f64>,
}

impl RunLedger {
    pub(crate) fn new(budget: Option<f64>) -> Self {
        Self {
            budget,
            inner: Mutex::new(LedgerInner::default()),
        }
    }

    /// Fold ONE leaf outcome. `cost` = metered spend (absent stays
    /// absent); `unpriced` = the leaf carried an [`UnpricedReason`] —
    /// counted so the terminal frame can say « N calls not in the total ».
    ///
    /// [`UnpricedReason`]: nika_types::cost::UnpricedReason
    pub(crate) fn debit(&self, source: Option<&str>, cost: Option<f64>, unpriced: bool) {
        // A poisoned lock = a sibling panicked mid-fold (test harness
        // class). The data is a plain accumulator with no invariant a
        // partial write could break — recover it and KEEP COUNTING:
        // dropping a debit (or failing the gate open) is the one thing
        // this ledger must never do.
        let mut inner = match self.inner.lock() {
            Ok(inner) => inner,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(c) = cost {
            inner.spent_usd += c;
            inner.any_priced = true;
            inner.priced_calls = inner.priced_calls.saturating_add(1);
            if let Some(key) = source {
                *inner.by_source.entry(key.to_owned()).or_insert(0.0) += c;
            }
            // Micro-USD rounding on BOTH sides: accumulation dust one
            // ULP over the nominal budget must never trip (and then
            // print « spent $X of $X — exceeded »). Spending EXACTLY
            // the budget does not trip — crossing it does.
            if let Some(budget) = self.budget
                && micro_usd(inner.spent_usd) > micro_usd(budget)
            {
                inner.tripped = true;
            }
        }
        if unpriced {
            inner.unpriced_calls = inner.unpriced_calls.saturating_add(1);
        }
    }

    /// Whether the budget has been crossed — the admission gates (wave
    /// members · fan-out iterations) consult this at pull time. Poison
    /// recovers via `into_inner` — the gate NEVER fails open.
    pub(crate) fn tripped(&self) -> bool {
        match self.inner.lock() {
            Ok(inner) => inner.tripped,
            Err(poisoned) => poisoned.into_inner().tripped,
        }
    }

    pub(crate) fn snapshot(&self) -> LedgerSnapshot {
        let inner = match self.inner.lock() {
            Ok(inner) => inner,
            Err(poisoned) => poisoned.into_inner(),
        };
        LedgerSnapshot {
            spent_usd: inner.spent_usd,
            any_priced: inner.any_priced,
            priced_calls: inner.priced_calls,
            unpriced_calls: inner.unpriced_calls,
            tripped: inner.tripped,
            budget: self.budget,
            by_source: inner.by_source.clone(),
        }
    }
}

/// Whole micro-USD (1e-6) — the budget-comparison grain.
fn micro_usd(usd: f64) -> i64 {
    // REASON: budgets/spend are operator-scale dollars (≪ the i64 micro
    // range); a non-finite value cannot reach here (the CLI rejects
    // NaN/inf budgets · provider costs are validated finite).
    #[allow(clippy::cast_possible_truncation)]
    {
        (usd * 1_000_000.0).round() as i64
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn absent_cost_never_debits_and_never_trips() {
        // The no-fake-zero law at the ledger: an unpriced leaf adds NO
        // dollars (not 0.0-with-priced-flag) and can never trip a budget.
        let ledger = RunLedger::new(Some(0.0));
        ledger.debit(Some("ollama/llama3.2"), None, true);
        let snap = ledger.snapshot();
        assert!(!snap.any_priced, "no priced call happened");
        assert!((snap.spent_usd - 0.0).abs() < f64::EPSILON);
        assert_eq!(snap.unpriced_calls, 1);
        assert!(!snap.tripped, "unmetered spend cannot cross a budget");
    }

    #[test]
    fn debits_accumulate_and_attribute_by_source() {
        let ledger = RunLedger::new(None);
        ledger.debit(Some("openai/gpt-4o-mini"), Some(0.01), false);
        ledger.debit(Some("openai/gpt-4o-mini"), Some(0.02), false);
        ledger.debit(Some("nika:image_generate"), Some(0.04), false);
        let snap = ledger.snapshot();
        assert!((snap.spent_usd - 0.07).abs() < 1e-12);
        assert_eq!(snap.priced_calls, 3);
        assert!((snap.by_source["openai/gpt-4o-mini"] - 0.03).abs() < 1e-12);
        assert!((snap.by_source["nika:image_generate"] - 0.04).abs() < 1e-12);
        assert!(!snap.tripped, "no budget → never trips");
    }

    #[test]
    fn budget_trips_only_when_crossed() {
        let ledger = RunLedger::new(Some(0.05));
        ledger.debit(Some("m"), Some(0.05), false);
        assert!(!ledger.tripped(), "AT the budget is not OVER it");
        ledger.debit(Some("m"), Some(0.0001), false);
        assert!(ledger.tripped(), "crossing trips");
        let snap = ledger.snapshot();
        assert!(snap.tripped);
        assert_eq!(snap.budget, Some(0.05));
    }
}
