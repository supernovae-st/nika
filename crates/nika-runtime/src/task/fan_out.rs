// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `for_each:` fan-out machinery (spec 03 · closed at v1) — the
//! PURE side of the lane: collection resolution over the item-free
//! boundary bindings · INPUT-order accumulation of the buffered
//! iteration stream · the terminal fold. The dispatching methods
//! (`run_fan_out` · `run_iteration`) stay on the runtime in the parent
//! module — this module never touches the seams.

use std::collections::BTreeMap;

use futures_util::StreamExt;
use nika_schema::raw::ForEachValue;
use serde_json::Value;

use super::{RanTask, RetryStamp, RunResult, SettleAs, VAR_TYPE_CODE, runtime_error_record};
use crate::errors::RuntimeError;
use crate::expr::{self, Scope};
use crate::record::TaskErrorRecord;

/// The pre-fan-out surface (spec 03 §`for_each`): the collection reads
/// local names — the item-free boundary bindings, never the global tasks
/// namespace (empty records = defense-in-depth; the checker already
/// refused any tasks.* here). An empty collection settles `skipped`.
pub(super) fn resolve_fan_out_items(
    collection: &ForEachValue,
    boundary_with: &BTreeMap<String, Value>,
    inputs: &BTreeMap<String, Value>,
    consts: &BTreeMap<String, Value>,
    secrets: &BTreeMap<String, Value>,
) -> Result<Vec<Value>, Box<SettleAs>> {
    let empty_records = BTreeMap::new();
    let scope = Scope {
        records: &empty_records,
        inputs,
        consts,
        secrets,
        with_ns: Some(boundary_with),
        item: None,
        index: None,
        permits: None,
    };
    let items = resolve_collection(collection, &scope)?;
    // Empty collection → the task is `skipped` (spec 03).
    if items.is_empty() {
        return Err(Box::new(SettleAs::SkippedGate {
            note: "for_each · empty collection",
            expr: None,
        }));
    }
    Ok(items)
}

/// The settled accumulation of a `for_each` fan-out — the per-iteration
/// results reduced in INPUT order (positions stay aligned · spec 03
/// §null-at-index).
pub(super) struct FanOutAccum {
    /// One value per iteration (null at a skipped/failed index).
    pub(super) outputs: Vec<Value>,
    /// Every retry scheduled across all iterations (`TaskRetrying`).
    pub(super) retries: Vec<RetryStamp>,
    /// The agent decisions across all iterations, in order.
    pub(super) agent_events: Vec<crate::agent_events::StampedAgentEvent>,
    /// NEP-0007 · the per-lane permit decisions, folded in lane order.
    pub(super) decisions: Vec<crate::witness::PermitDecision>,
    /// The FIRST iteration error (the one the task reports on failure).
    pub(super) first_error: Option<TaskErrorRecord>,
    /// A selected execution route from any iteration. Harness receipts
    /// take precedence because replaying the aggregate would repeat an
    /// effect even when another iteration supplied the reported error.
    pub(super) access_receipt: Option<crate::dispatch::AccessReceipt>,
    /// Per-iteration token spend SUMMED onto the parent (a 50-infer fan-out
    /// must never report zero to the cost meter) · None until any reports.
    pub(super) tokens_sum: Option<i64>,
    /// Per-iteration USD spend SUMMED the same way (same-model iterations ·
    /// per-turn pricing sums exactly) · None until any priced call reports.
    pub(super) cost_sum: Option<f64>,
    /// The FIRST unpriced reason across iterations (they share one model,
    /// so the first is the class) — rides the parent's terminal frame.
    pub(super) unpriced: Option<nika_types::cost::UnpricedReason>,
    /// Iterations that did NOT truly succeed — a `recover:` fallback stood
    /// in, an `on_error: skip` nulled the index, or a swallowed failure
    /// held its slot (`fail_fast: false`). The fan's own note carries the
    /// count (V7-1 · wave-3 Marta: 2 of 3 items died at their timeout
    /// under `recover: null` and the card said `✔ 3 items · 5/5 done` —
    /// she found out by counting the trace).
    pub(super) recovered: usize,
    /// The FIRST error an iteration was repaired from — the fan's
    /// `recovered_from` (spec 13 §payload). Mirrors `first_error`: one
    /// witness, kept, rather than N discarded.
    ///
    /// It used to be counted and dropped. `recovered_from` was
    /// destructured, tested with `is_some()`, and thrown away for the
    /// counter below, so the tally survived as prose in the note while
    /// the machine-readable `cause` reported `normal` on a fan whose
    /// iterations had died.
    pub(super) first_recovered_from: Option<TaskErrorRecord>,
}

/// Drain the buffered iteration stream, reducing it to a [`FanOutAccum`] in
/// INPUT order. On `fail_fast`, the FIRST failure stops the drain: dropping
/// the stream cancels in-flight iterations at their await points and unspawned
/// ones never start (spec 03 · `fail_fast: true` default).
pub(super) async fn collect_fan_out<S>(stream: &mut S, total: usize, fail_fast: bool) -> FanOutAccum
where
    S: futures_util::Stream<Item = RanTask> + Unpin,
{
    let mut acc = FanOutAccum {
        outputs: Vec::with_capacity(total),
        retries: Vec::new(),
        agent_events: Vec::new(),
        decisions: Vec::new(),
        first_error: None,
        access_receipt: None,
        tokens_sum: None,
        cost_sum: None,
        unpriced: None,
        recovered: 0,
        first_recovered_from: None,
    };

    while let Some(iter_ran) = stream.next().await {
        acc.retries.extend(iter_ran.retries);
        acc.agent_events.extend(iter_ran.agent_events);
        acc.decisions.extend(iter_ran.decisions);
        match iter_ran.result {
            // Per-call warnings do not aggregate into the fan-out.
            RunResult::Success {
                value,
                tokens,
                cost_usd,
                cost_unpriced,
                ref recovered_from,
                access_receipt,
                ..
            } => {
                retain_effect_receipt(&mut acc.access_receipt, access_receipt);
                // `recovered_from` distinguishes a fallback from honest output.
                if let Some(original) = recovered_from {
                    acc.recovered += 1;
                    // The payload names the first recovery witness.
                    if acc.first_recovered_from.is_none() {
                        acc.first_recovered_from = Some(original.clone());
                    }
                }
                acc.outputs.push(value);
                if let Some(n) = tokens {
                    acc.tokens_sum = Some(acc.tokens_sum.unwrap_or(0).saturating_add(n));
                }
                if let Some(c) = cost_usd {
                    acc.cost_sum = Some(acc.cost_sum.unwrap_or(0.0) + c);
                }
                if acc.unpriced.is_none() {
                    acc.unpriced = cost_unpriced;
                }
            }
            // `on_error: skip` contributes null and retains alignment.
            RunResult::SkippedWithError { access_receipt, .. } => {
                retain_effect_receipt(&mut acc.access_receipt, access_receipt);
                acc.recovered += 1;
                acc.outputs.push(Value::Null);
            }
            RunResult::Failed {
                error,
                access_receipt,
                ..
            } => {
                retain_effect_receipt(&mut acc.access_receipt, access_receipt);
                acc.outputs.push(Value::Null);
                if acc.first_error.is_none() {
                    acc.first_error = Some(error);
                }
                if fail_fast {
                    break;
                }
            }
            // An iteration never parks; pending recovery becomes failure.
            RunResult::PendingRecovery(pending) => {
                retain_effect_receipt(
                    &mut acc.access_receipt,
                    pending.failed.access_receipt.map(|receipt| *receipt),
                );
                acc.outputs.push(Value::Null);
                if acc.first_error.is_none() {
                    acc.first_error = Some(pending.render_error);
                }
                if fail_fast {
                    break;
                }
            }
        }
    }
    acc
}

/// Retain one typed witness that makes replaying the whole aggregate
/// unsafe. A harness receipt outranks an API/local receipt even when it
/// came from a later iteration or a successful sibling.
pub(super) fn retain_effect_receipt(
    retained: &mut Option<crate::dispatch::AccessReceipt>,
    candidate: Option<crate::dispatch::AccessReceipt>,
) {
    let Some(candidate) = candidate.map(crate::dispatch::AccessReceipt::into_representative) else {
        return;
    };
    let replace = retained.is_none()
        || (!retained
            .as_ref()
            .is_some_and(crate::dispatch::AccessReceipt::selected_harness)
            && candidate.selected_harness());
    if replace {
        *retained = Some(candidate);
    }
}

/// Resolve the `for_each:` collection (the ONLY once-evaluated body
/// expression · spec 03) — an array of items, or the settle verdict
/// for the failure lanes (boxed: the error lane stays pointer-thin).
fn resolve_collection(
    collection: &ForEachValue,
    scope: &Scope<'_>,
) -> Result<Vec<Value>, Box<SettleAs>> {
    let resolved = match collection {
        ForEachValue::List(value) => expr::render_json(value, scope),
        ForEachValue::Expression(text) => expr::render_json(&Value::String(text.clone()), scope),
        // #[non_exhaustive] · a future collection form fails loudly.
        other => Err(RuntimeError::WhenUnsupported {
            expr: format!("for_each form not wired in the runtime yet: {other:?}"),
        }),
    };
    match resolved {
        Ok(Value::Array(items)) => Ok(items),
        // Non-array collection = evaluation error (spec 03 · the
        // NIKA-VAR-006 class).
        Ok(other) => Err(Box::new(SettleAs::FailedBeforeStart {
            stage: "for_each",
            error: TaskErrorRecord {
                code: VAR_TYPE_CODE.to_owned(),
                message: format!(
                    "for_each collection must be an array · got {}",
                    json_kind(&other)
                ),
                transient: false,
            },
        })),
        Err(err) => Err(Box::new(SettleAs::FailedBeforeStart {
            stage: "for_each",
            error: runtime_error_record(&err),
        })),
    }
}

/// Reduce a drained fan-out to its terminal [`RunResult`]. The leaf
/// iterations already debited the ledger — the aggregate spend here is
/// presentation-only (never re-debited). OBS-E warnings stay per-call
/// (no single aggregate warning channel).
pub(super) fn fan_out_result(
    outputs: Vec<Value>,
    tokens_sum: Option<i64>,
    (first_error, first_recovered_from): (Option<TaskErrorRecord>, Option<TaskErrorRecord>),
    spend: (
        Option<f64>,
        Option<nika_types::cost::UnpricedReason>,
        Option<crate::dispatch::AccessReceipt>,
    ),
) -> RunResult {
    let (cost_usd, cost_unpriced, access_receipt) = spend;
    match first_error {
        None => RunResult::Success {
            value: Value::Array(outputs),
            tokens: tokens_sum,
            // `settle` reads THIS to choose the cause: `Some` gives
            // `success/recovered` + the payload's original error, `None`
            // gives `success/normal`. Hardcoding `None` here is what made
            // a fan of dead iterations report `normal`.
            recovered_from: first_recovered_from,
            warning: None,
            // per-iteration child rows stay per-call — no aggregate row
            child: None,
            cost_usd,
            cost_unpriced,
            // the aggregate is N calls · per-iteration models stay
            // per-call — no single model names the fold
            model: None,
            access_receipt,
        },
        Some(error) => RunResult::Failed {
            error,
            cost_usd,
            cost_unpriced,
            access_receipt,
        },
    }
}

/// The fan's terminal note — the honest tally (V7-1): a fan whose K
/// iterations were repaired (`recover:` fallback · `on_error: skip`)
/// SAYS so on its own row. A green `✔ N items` over silently-nulled
/// work taught wave-3 Marta to distrust the card (she counted the
/// trace by hand); a healthy fan keeps its historical row byte-stable
/// (no zero-count tail — calm stays calm).
pub(super) fn fan_note(total: usize, recovered: usize) -> String {
    if recovered > 0 {
        format!(
            "for_each · {ok}/{total} ok · {recovered} recovered",
            ok = total.saturating_sub(recovered),
        )
    } else {
        format!("for_each · {total} items")
    }
}

/// The fan-out budget-starvation error — iterations the ledger refused
/// to admit (NIKA-1704 · the workflow-level abort follows at the wave
/// boundary).
pub(super) fn budget_stop_record(denied: usize) -> TaskErrorRecord {
    TaskErrorRecord {
        code: nika_error::codes::NIKA_1704.to_string(),
        message: format!(
            "run budget (--max-cost-usd) reached — {denied} iteration(s) were not started \
             (in-flight work completed and was counted)"
        ),
        transient: false, // spending more will not help
    }
}

/// JSON value kind word (error messages).
fn json_kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
