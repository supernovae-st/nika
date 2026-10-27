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
//! Billed-then-failed spend IS metered (2026-07-08): the verbs
//! decorate their loop-scoped errors with the incurred spend, the
//! dispatch prices it, and the attempt loop debits it PER ATTEMPT —
//! a retry storm cannot spend past the budget invisibly. An attempt
//! dropped before it returns (a `fail_fast` sibling · a `timeout:`)
//! debits every provider request it sent through the dispatch journal
//! (B7), unanswered ones unpriced, and the known cost of every tool
//! success it received through its receipts (`UnreturnedSpend`), read or
//! not. Remaining unmetered class: `on_finally` cleanups (best-effort
//! lane · outcome dropped by design).
//!
//! Paid media (C-D3) is judged before its request against what the cap
//! leaves at that instant, and the admitted call holds its floor on the
//! ledger until it returns ([`MediaHold`]): siblings in flight cannot spend
//! one remainder together. A call that billed without a figure leaves its
//! floor counted consumed, never as spend; one dropped in flight is counted
//! unpriced and its floor stays consumed.

use std::collections::BTreeMap;
use std::sync::Mutex;

/// Shared per-run spend accumulator (waves dispatch concurrently — the
/// leaf debits race; a Mutex over the tiny fold is the whole story).
pub(crate) struct RunLedger {
    budget: Option<f64>,
    /// The run's start on the kernel clock (#1247): the terminal frame's
    /// `elapsed_ms`.
    started: Option<std::time::Instant>,
    inner: Mutex<LedgerInner>,
    /// Run-owned witnesses survive dropped task/iteration/agent futures.
    stopped: Mutex<BTreeMap<String, nika_verb_invoke::StoppedCallSlot>>,
}

#[derive(Default)]
struct LedgerInner {
    spent_usd: f64,
    any_priced: bool,
    priced_calls: u32,
    unpriced_calls: u32,
    tripped: bool,
    by_source: BTreeMap<String, f64>,
    /// The floors the paid media calls in flight hold on the cap (C-D3): what
    /// the cap leaves excludes them until each call returns ([`MediaHold`]).
    held_usd: f64,
    /// The floors of paid media calls that billed without a figure: each cost
    /// at least its floor, so the cap counts them consumed, though no total
    /// claims them as spend.
    unread_usd: f64,
    /// A paid media call billed without a figure: what the run spent is no
    /// longer fully known, so the cap refuses a call billed only from what it
    /// reports.
    unread: bool,
}

impl LedgerInner {
    /// What `budget` leaves: the spend, the floors in flight and the floors
    /// billed without a figure excluded, floored at 0.
    fn left(&self, budget: f64) -> f64 {
        (budget - self.spent_usd - self.held_usd - self.unread_usd).max(0.0)
    }
}

/// A point-in-time copy for the terminal frame + [`crate::RunOutcome`].
pub(crate) struct LedgerSnapshot {
    pub stopped_calls: Vec<nika_event::settlement::StoppedCall>,
    pub spent_usd: f64,
    /// Whether ANY leaf reported real spend — the totals only ride the
    /// terminal frame when true (a mock/local run stays field-free:
    /// absent is honest, a `total_cost_usd: 0.0` we never metered is not).
    pub any_priced: bool,
    pub priced_calls: u32,
    pub unpriced_calls: u32,
    pub budget: Option<f64>,
    /// Spend per attribution key (`provider/model @ origin` · tool id).
    pub by_source: BTreeMap<String, f64>,
    /// The run's elapsed time when known (#1247 · [`RunLedger::snapshot_at`]).
    pub elapsed: Option<std::time::Duration>,
}

impl RunLedger {
    pub(crate) fn new(budget: Option<f64>) -> Self {
        Self {
            budget,
            started: None,
            inner: Mutex::new(LedgerInner::default()),
            stopped: Mutex::new(BTreeMap::new()),
        }
    }

    /// The run's start on the kernel clock.
    #[must_use]
    pub(crate) fn started_at(mut self, now: std::time::Instant) -> Self {
        self.started = Some(now);
        self
    }

    /// [`Self::snapshot`] with the run's elapsed time at `now`.
    pub(crate) fn snapshot_at(&self, now: std::time::Instant) -> LedgerSnapshot {
        let mut snapshot = self.snapshot();
        snapshot.elapsed = self
            .started
            .map(|started| now.saturating_duration_since(started));
        snapshot
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

    /// Per-invocation accounting when evidence exists, with tools kept separate.
    pub(crate) fn debit_observed(
        &self,
        source: Option<&str>,
        cost: Option<f64>,
        unpriced: bool,
        usage: Option<&crate::usage::UsageSplit>,
    ) {
        let Some(split) = usage.filter(|u| !u.inference_calls.is_empty()) else {
            self.debit(source, cost, unpriced);
            return;
        };
        let known = self.debit_calls(&split.inference_calls);
        // Dispatch cost is the same known invocation subtotal plus known tools.
        // No unknown invocation is replaced by this remainder.
        if let Some(total) = cost {
            let tools = total - known;
            if tools > 0.0 {
                let key = source.map(|s| format!("{s} (tools)"));
                self.debit(key.as_deref(), Some(tools), false);
            }
        }
    }

    /// Debit each provider request by its own evidence, once: a known estimate
    /// under its route, anything else unpriced (never a known zero). Also the
    /// fold of a dropped attempt's journaled requests. Returns the known USD.
    pub(crate) fn debit_calls(&self, calls: &[nika_types::cost::InferenceCall]) -> f64 {
        let mut known = 0.0;
        for call in calls {
            let estimate = call
                .known_estimate()
                .map(nika_types::cost::Cost::to_usd_f64);
            known += estimate.unwrap_or(0.0);
            let route = call.route.as_ref().map(nika_providers::route_label);
            self.debit(route.as_deref(), estimate, estimate.is_none());
        }
        known
    }

    /// Fold one successful dispatch — THE leaf debit site (plain tasks
    /// and fan-out iterations both flow through the attempt loop; the
    /// parent fan-out sum is presentation-only, so nothing double-counts
    /// — the Sentry hierarchy lesson).
    pub(crate) fn debit_ok(&self, ok: &crate::dispatch::DispatchOk) {
        self.debit_observed(
            ok.cost_source.as_deref(),
            ok.cost_usd,
            ok.cost_unpriced.is_some(),
            ok.usage.as_deref(),
        );
        self.count_unpriced(ok.unpriced_inner_calls);
    }

    /// Count `calls` made inside one leaf whose spend is outside every
    /// figure (C-D3 · an agent's paid media tools, a child run's unpriced
    /// calls), so the settlement never reads `priced` while it holds them.
    /// An agent leaf always carries its per-call split (the split branch of
    /// [`Self::debit_observed`] ignores its own flag), and a child call sets
    /// no own reason, so no call is counted twice.
    pub(crate) fn count_unpriced(&self, calls: u32) {
        if calls == 0 {
            return;
        }
        let mut inner = match self.inner.lock() {
            Ok(inner) => inner,
            Err(poisoned) => poisoned.into_inner(),
        };
        inner.unpriced_calls = inner.unpriced_calls.saturating_add(calls);
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

    /// Budget minus spend at this instant, floored at 0 — what a child
    /// call may still spend (spec 14 law 6 · `min(parent remaining,
    /// child declared)`). The floors of the paid media calls in flight, and
    /// of those that billed without a figure, are excluded (C-D3). `None` =
    /// this run carries no cost budget.
    pub(crate) fn remaining_usd(&self) -> Option<f64> {
        let budget = self.budget?;
        Some(self.lock().left(budget))
    }

    /// Judge one paid media call before its request against what the cap
    /// leaves at this instant (C-D3), and hold its floor while it runs.
    /// `judge` reads what is left and whether an earlier paid call billed
    /// without a figure: `Ok(Some(floor))` admits the call, holding `floor`,
    /// `Ok(None)` is no paid media call, `Err` refuses it. The judgment and
    /// the hold share one lock, so no sibling admitted in between can spend
    /// the same remainder. `Ok(None)` without a cap: nothing to judge.
    pub(crate) fn hold_media<E>(
        &self,
        judge: impl FnOnce(f64, bool) -> Result<Option<f64>, E>,
    ) -> Result<Option<MediaHold<'_>>, E> {
        let Some(budget) = self.budget else {
            return Ok(None);
        };
        let mut inner = self.lock();
        let Some(floor) = judge(inner.left(budget), inner.unread)? else {
            return Ok(None);
        };
        inner.held_usd += floor;
        Ok(Some(MediaHold {
            ledger: self,
            floor,
            armed: true,
        }))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, LedgerInner> {
        match self.inner.lock() {
            Ok(inner) => inner,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub(crate) fn snapshot(&self) -> LedgerSnapshot {
        let inner = match self.inner.lock() {
            Ok(inner) => inner,
            Err(poisoned) => poisoned.into_inner(),
        };
        LedgerSnapshot {
            stopped_calls: self.stopped_calls(),
            spent_usd: inner.spent_usd,
            any_priced: inner.any_priced,
            priced_calls: inner.priced_calls,
            unpriced_calls: inner.unpriced_calls,
            budget: self.budget,
            by_source: inner.by_source.clone(),
            elapsed: None,
        }
    }

    /// A task's shared parent; each cancellable attempt owns a fresh child.
    pub(crate) fn stopped_slot(&self, task: &str) -> nika_verb_invoke::StoppedCallSlot {
        self.stopped
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(task.to_owned())
            .or_default()
            .clone()
    }

    fn stopped_calls(&self) -> Vec<nika_event::settlement::StoppedCall> {
        let slots = self
            .stopped
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        slots
            .iter()
            .flat_map(|(task, slot)| {
                slot.snapshot().into_iter().filter_map(move |call| {
                    let outcome =
                        nika_event::settlement::StopOutcome::parse(call.outcome.as_str())?;
                    Some(nika_event::settlement::StoppedCall::new(
                        task,
                        call.tool,
                        call.call_id,
                        outcome,
                    ))
                })
            })
            .collect()
    }
}

/// A dispatch's tool receipts while it runs (the dispatch journal's twin
/// for tool spend). Dropped armed, because the dispatch was dropped before
/// it returned (a `fail_fast` sibling · a `timeout:`), it debits the known
/// cost of every tool success the dispatch received, once. A returned
/// dispatch carries that spend in its own outcome and disarms it.
pub(crate) struct UnreturnedSpend<'l> {
    armed: Option<(
        &'l RunLedger,
        nika_verb_invoke::StoppedCallSlot,
        Option<String>,
    )>,
}

impl<'l> UnreturnedSpend<'l> {
    /// Watch `receipts` (none: nothing to debit), attributed to `source`.
    #[must_use = "dropping it debits the receipts at once"]
    pub(crate) fn arm(
        ledger: &'l RunLedger,
        receipts: Option<nika_verb_invoke::StoppedCallSlot>,
        source: Option<String>,
    ) -> Self {
        Self {
            armed: receipts.map(|receipts| (ledger, receipts, source)),
        }
    }

    /// The dispatch returned: its outcome owns the spend.
    pub(crate) fn disarm(mut self) {
        self.armed = None;
    }
}

impl Drop for UnreturnedSpend<'_> {
    fn drop(&mut self) {
        if let Some((ledger, receipts, source)) = self.armed.take()
            && let Some(cost) = receipts.reported_cost_usd()
        {
            ledger.debit(source.as_deref(), Some(cost), false);
        }
    }
}

/// The floor one paid media call holds on the run's cost cap while its
/// request runs (C-D3): every sibling judged meanwhile sees the cap without
/// it, so a capped fan-out cannot launch its items past the cap together.
/// [`Self::settle`] releases it once the call returned, its outcome carrying
/// what it cost; a call that billed without a figure leaves its floor
/// counted consumed. Dropped unsettled (a `timeout:`, a `fail_fast` sibling
/// or a Stop dropped the call in flight), the request may have rendered and
/// billed: its floor stays consumed and the settlement counts one unpriced
/// call, never a free one.
#[must_use = "dropping it unsettled counts the call unpriced"]
pub(crate) struct MediaHold<'l> {
    ledger: &'l RunLedger,
    floor: f64,
    armed: bool,
}

impl MediaHold<'_> {
    /// The call returned: its outcome carries its spend for the leaf debit.
    /// `unread`: it billed without a figure (a paid call that reported no
    /// usable spend), so its floor stays consumed and the cap no longer
    /// knows what it leaves.
    pub(crate) fn settle(mut self, unread: bool) {
        self.armed = false;
        self.release(unread, false);
    }

    fn release(&self, unread: bool, unpriced_call: bool) {
        let mut inner = self.ledger.lock();
        inner.held_usd = (inner.held_usd - self.floor).max(0.0);
        if unread {
            inner.unread_usd += self.floor;
            inner.unread = true;
        }
        if unpriced_call {
            inner.unpriced_calls = inner.unpriced_calls.saturating_add(1);
        }
    }
}

impl Drop for MediaHold<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.release(true, true);
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
        assert!(!ledger.tripped(), "unmetered spend cannot cross a budget");
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
        assert!(!ledger.tripped(), "no budget → never trips");
    }

    /// C-D3 · a paid media call holds its floor while it runs: a sibling
    /// judged meanwhile sees the cap without it; settled, the floor is
    /// released (kept consumed when the call billed without a figure);
    /// dropped unsettled, it stays consumed and one unpriced call is counted.
    #[test]
    fn a_media_hold_keeps_its_floor_from_siblings_until_it_settles() {
        let ledger = RunLedger::new(Some(0.25));
        let admit = |floor: f64| {
            ledger.hold_media(|left, _| {
                if micro_usd(floor) > micro_usd(left) {
                    Err(left)
                } else {
                    Ok(Some(floor))
                }
            })
        };
        let first = admit(0.211).expect("fits").expect("held");
        let left = admit(0.211).err().expect("the first holds its floor");
        assert!((left - 0.039).abs() < 1e-12, "{left}");
        assert!((ledger.remaining_usd().expect("capped") - 0.039).abs() < 1e-12);
        first.settle(false);
        assert!((ledger.remaining_usd().expect("capped") - 0.25).abs() < 1e-12);

        let unread = admit(0.1).expect("fits").expect("held");
        unread.settle(true);
        assert!((ledger.remaining_usd().expect("capped") - 0.15).abs() < 1e-12);
        let blind = ledger
            .hold_media(|_, unread| if unread { Err(()) } else { Ok(Some(0.0)) })
            .err();
        assert_eq!(blind, Some(()), "the cap knows a call billed unread");

        drop(admit(0.05).expect("fits").expect("held"));
        let snapshot = ledger.snapshot();
        assert_eq!(
            snapshot.unpriced_calls, 1,
            "the dropped call may have billed"
        );
        assert!((ledger.remaining_usd().expect("capped") - 0.1).abs() < 1e-12);
        assert!(!ledger.tripped(), "held and unread floors are never spend");
    }

    #[test]
    fn no_cap_holds_nothing() {
        let ledger = RunLedger::new(None);
        let hold = ledger.hold_media(|_, _| Ok::<_, ()>(Some(1.0)));
        assert!(matches!(hold, Ok(None)));
    }

    #[test]
    fn budget_trips_only_when_crossed() {
        let ledger = RunLedger::new(Some(0.05));
        ledger.debit(Some("m"), Some(0.05), false);
        assert!(!ledger.tripped(), "AT the budget is not OVER it");
        ledger.debit(Some("m"), Some(0.0001), false);
        assert!(ledger.tripped(), "crossing trips");
        let snap = ledger.snapshot();
        assert!(ledger.tripped());
        assert_eq!(snap.budget, Some(0.05));
    }
}
