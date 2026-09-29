// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `for_each:` fan-out (spec 03) — collection resolve, completion-order
//! collection, input-order fold. Dispatch (`run_fan_out` · `run_iteration`)
//! stays in the parent.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

use futures_util::StreamExt;
use nika_schema::raw::ForEachValue;
use serde_json::Value;

use super::{RanTask, RetryStamp, RunResult, SettleAs, VAR_TYPE_CODE, runtime_error_record};
use crate::errors::RuntimeError;
use crate::expr::{self, Scope};
use crate::record::TaskErrorRecord;

/// Collection over item-free boundary bindings. Empty → `skipped`. A
/// collection longer than `max_items` → refused before the first item
/// (#1510).
pub(super) fn resolve_fan_out_items(
    collection: &ForEachValue,
    max_items: Option<u32>,
    boundary_with: &BTreeMap<String, Value>,
    inputs: &BTreeMap<String, Value>,
    consts: &BTreeMap<String, Value>,
    secrets: &BTreeMap<String, Value>,
) -> Result<Vec<Value>, Box<SettleAs>> {
    let empty_records = BTreeMap::new();
    let scope = Scope::workflow_with_value_authorities(&empty_records, inputs, consts, secrets)
        .with_task_context(Some(boundary_with), None, None, None);
    let items = resolve_collection(collection, max_items, &scope)?;
    if items.is_empty() {
        return Err(Box::new(SettleAs::SkippedGate {
            note: "for_each · empty collection",
            expr: None,
        }));
    }
    Ok(items)
}

/// Per-iteration results reduced in INPUT order (spec 03 §null-at-index).
pub(super) struct FanOutAccum {
    pub(super) outputs: Vec<Value>,
    pub(super) retries: Vec<RetryStamp>,
    pub(super) agent_events: Vec<crate::agent_events::StampedAgentEvent>,
    pub(super) decisions: Vec<crate::witness::PermitDecision>,
    pub(super) first_error: Option<TaskErrorRecord>,
    pub(super) tokens_sum: Option<i64>,
    pub(super) cost_sum: Option<f64>,
    pub(super) unpriced: Option<nika_types::cost::UnpricedReason>,
    pub(super) recovered: usize,
    pub(super) failed_items: Vec<String>,
    pub(super) recovered_items: Vec<String>,
    pub(super) first_recovered_from: Option<TaskErrorRecord>,
    /// One row per consumed iteration, in input order (#1276 · #1397):
    /// index · item · status · code · message.
    pub(super) items: Vec<Value>,
}

/// Drive the batch in COMPLETION order (spec 03): under `fail_fast` the first
/// failure to complete stops it, whatever its index; the caller then drops what
/// is still in flight (`cancelled`) or queued (`never_started`). Every
/// iteration that completed before the stop is kept, folded in INPUT order.
pub(super) async fn collect_fan_out<S>(stream: &mut S, total: usize, fail_fast: bool) -> FanOutAccum
where
    S: futures_util::Stream<Item = (usize, RanTask)> + Unpin,
{
    let mut done = BTreeMap::new();
    while let Some((index, ran)) = stream.next().await {
        let failed = matches!(
            ran.result,
            RunResult::Failed { .. } | RunResult::PendingRecovery(_)
        );
        done.insert(index, ran);
        if fail_fast && failed {
            break;
        }
    }
    let mut acc = FanOutAccum {
        outputs: Vec::with_capacity(total),
        retries: Vec::new(),
        agent_events: Vec::new(),
        decisions: Vec::new(),
        first_error: None,
        tokens_sum: None,
        cost_sum: None,
        unpriced: None,
        recovered: 0,
        failed_items: Vec::new(),
        recovered_items: Vec::new(),
        first_recovered_from: None,
        items: Vec::new(),
    };
    for (index, ran) in done {
        consume_iteration(&mut acc, index, ran);
    }
    acc
}

fn consume_iteration(acc: &mut FanOutAccum, index: usize, iter_ran: RanTask) {
    acc.retries.extend(iter_ran.retries);
    acc.agent_events.extend(iter_ran.agent_events);
    acc.decisions.extend(iter_ran.decisions);
    let identity = identity_from_note(&iter_ran.note);
    match iter_ran.result {
        RunResult::Success {
            value,
            tokens,
            cost_usd,
            cost_unpriced,
            recovered_from,
            ..
        } => {
            acc.items.push(item_row(
                index,
                &identity,
                if recovered_from.is_some() {
                    "recovered"
                } else {
                    "ok"
                },
                recovered_from.as_ref(),
            ));
            if let Some(original) = recovered_from {
                acc.recovered += 1;
                acc.recovered_items.push(identity);
                if acc.first_recovered_from.is_none() {
                    acc.first_recovered_from = Some(original);
                }
            }
            acc.outputs.push(value);
            fold_spend(acc, tokens, cost_usd, cost_unpriced);
        }
        RunResult::SkippedWithError { error, .. } => {
            acc.items
                .push(item_row(index, &identity, "recovered", Some(&error)));
            acc.recovered += 1;
            acc.recovered_items.push(identity);
            acc.outputs.push(Value::Null);
            if acc.first_recovered_from.is_none() {
                acc.first_recovered_from = Some(error);
            }
        }
        RunResult::Failed { error, .. } => {
            acc.items
                .push(item_row(index, &identity, "failed", Some(&error)));
            acc.outputs.push(Value::Null);
            acc.failed_items.push(identity);
            if acc.first_error.is_none() {
                acc.first_error = Some(error);
            }
        }
        RunResult::PendingRecovery(pending) => {
            acc.items.push(item_row(
                index,
                &identity,
                "failed",
                Some(&pending.render_error),
            ));
            acc.outputs.push(Value::Null);
            acc.failed_items.push(identity);
            if acc.first_error.is_none() {
                acc.first_error = Some(pending.render_error);
            }
        }
    }
}

fn fold_spend(
    acc: &mut FanOutAccum,
    tokens: Option<i64>,
    cost_usd: Option<f64>,
    cost_unpriced: Option<nika_types::cost::UnpricedReason>,
) {
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

/// The collection, resolved and fitted to the declared fan shape. The
/// ONE emission site of the evaluation-plane code for a `for_each`
/// collection: not an array, or longer than `max_items` (#1510 — the
/// cap is the fan's `maxItems`, and a collection that exceeds it is
/// refused HERE, before the first item, never truncated in silence).
fn resolve_collection(
    collection: &ForEachValue,
    max_items: Option<u32>,
    scope: &Scope<'_>,
) -> Result<Vec<Value>, Box<SettleAs>> {
    let resolved = match collection {
        ForEachValue::List(value) => expr::render_json(value, scope),
        ForEachValue::Expression(text) => expr::render_json(&Value::String(text.clone()), scope),
        other => Err(nika_dataflow::DataflowError::WhenUnsupported {
            expr: format!("for_each form not wired in the runtime yet: {other:?}"),
        }),
    }
    .map_err(RuntimeError::from);
    match resolved {
        Ok(Value::Array(items)) => match max_items {
            Some(cap) if items.len() > cap as usize => Err(Box::new(SettleAs::FailedBeforeStart {
                stage: "for_each",
                error: TaskErrorRecord::new(
                    VAR_TYPE_CODE,
                    format!(
                        "for_each collection has {} items but `max_items: {cap}` caps the fan — \
                         refused before the first item (never a silent truncation); raise the \
                         cap or narrow the collection",
                        items.len()
                    ),
                    false,
                ),
            })),
            _ => Ok(items),
        },
        Ok(other) => Err(Box::new(SettleAs::FailedBeforeStart {
            stage: "for_each",
            error: TaskErrorRecord::new(
                VAR_TYPE_CODE,
                format!(
                    "for_each collection must be an array · got {}",
                    json_kind(&other)
                ),
                false,
            ),
        })),
        Err(err) => Err(Box::new(SettleAs::FailedBeforeStart {
            stage: "for_each",
            error: runtime_error_record(&err),
        })),
    }
}

pub(super) fn fan_out_result(
    outputs: Vec<Value>,
    tokens_sum: Option<i64>,
    (first_error, first_recovered_from): (Option<TaskErrorRecord>, Option<TaskErrorRecord>),
    spend: (Option<f64>, Option<nika_types::cost::UnpricedReason>),
) -> RunResult {
    let (cost_usd, cost_unpriced) = spend;
    match first_error {
        None => RunResult::Success {
            value: Value::Array(outputs),
            tokens: tokens_sum,
            recovered_from: first_recovered_from,
            warning: None,
            child: None,
            cost_usd,
            cost_unpriced,
            model: None,
            access: None,
        },
        Some(error) => RunResult::Failed {
            error,
            cost_usd,
            cost_unpriced,
            access: None,
            access_refused: None,
        },
    }
}

pub(super) fn fan_note(
    total: usize,
    recovered: usize,
    failed_items: &[String],
    recovered_items: &[String],
) -> String {
    if !failed_items.is_empty() {
        return format!(
            "{} of {total} items failed: {}",
            failed_items.len(),
            failed_items.join(", "),
        );
    }
    if recovered > 0 {
        let ok = total.saturating_sub(recovered);
        if recovered_items.is_empty() {
            format!("for_each · {ok}/{total} ok · {recovered} recovered")
        } else {
            format!(
                "for_each · {ok}/{total} ok · {recovered} recovered: {}",
                recovered_items.join(", "),
            )
        }
    } else {
        format!("for_each · {total} items")
    }
}

const ITEM_IDENTITY_MAX: usize = 80;

pub(super) fn item_identity(item: &Value) -> String {
    truncate_identity(&crate::record::render_value(item))
}

fn truncate_identity(raw: &str) -> String {
    if raw.chars().count() <= ITEM_IDENTITY_MAX {
        return raw.to_owned();
    }
    let mut truncated: String = raw.chars().take(ITEM_IDENTITY_MAX - 1).collect();
    truncated.push('…');
    truncated
}

pub(super) fn iteration_note(index: usize, identity: &str) -> String {
    format!("for_each[{index}]={identity}")
}

pub(super) fn stamp_iteration(ran: &mut RanTask, index: usize, item: &Value) {
    let identity = item_identity(item);
    ran.note = iteration_note(index, &identity);
    match &mut ran.result {
        RunResult::Failed { error, .. } | RunResult::SkippedWithError { error, .. } => {
            annotate_error_in_place(error, index, &identity);
        }
        RunResult::Success { recovered_from, .. } => {
            if let Some(error) = recovered_from {
                annotate_error_in_place(error, index, &identity);
            }
        }
        RunResult::PendingRecovery(pending) => {
            annotate_error_in_place(&mut pending.render_error, index, &identity);
            annotate_error_in_place(&mut pending.failed.record, index, &identity);
        }
    }
}

const ITEM_ERROR_PREFIX: &str = "for_each item [";

fn annotate_error_in_place(error: &mut TaskErrorRecord, index: usize, identity: &str) {
    if error.message.starts_with(ITEM_ERROR_PREFIX) {
        return;
    }
    error.message = format!("for_each item [{index}] {identity}: {}", error.message);
}

/// One item's terminal row (#1276 · #1397): the index and the identity the
/// iteration note carries, the status, and the recorded error's code and
/// message when there is one.
fn item_row(index: usize, identity: &str, status: &str, error: Option<&TaskErrorRecord>) -> Value {
    let mut row = serde_json::json!({ "index": index, "item": identity, "status": status });
    if let (Some(error), Some(object)) = (error, row.as_object_mut()) {
        object.insert("code".to_owned(), Value::String(error.code.clone()));
        object.insert("message".to_owned(), Value::String(error.message.clone()));
    }
    row
}

/// One flag per item, raised when its iteration begins (spec 03 · 17).
pub(super) fn unstarted(total: usize) -> Vec<AtomicBool> {
    (0..total).map(|_| AtomicBool::new(false)).collect()
}

/// Drive one iteration; its FIRST poll marks it begun (building it does not).
/// The output keeps the item's index: the batch completes out of order.
pub(super) async fn started_on_first_poll<F: std::future::Future>(
    index: usize,
    flag: Option<&AtomicBool>,
    iteration: F,
) -> (usize, F::Output) {
    if let Some(flag) = flag {
        flag.store(true, Relaxed);
    }
    (index, iteration.await)
}

/// The fan-out's item table as ONE compact JSON text (#1276 · #1397): one row
/// per item, in input order. A kept iteration has its own row; an item the
/// batch stopped without one (`fail_fast` · the budget) is `cancelled` if it
/// began, else `never_started`; neither has an output or is a billing verdict
/// (spec 17). Rendered on the terminal frame as `items`.
pub(super) fn items_json(rows: Vec<Value>, items: &[Value], started: &[AtomicBool]) -> String {
    let mut rows = rows.into_iter().peekable();
    let table: Vec<Value> = (items.iter().enumerate())
        .map(|(index, item)| {
            rows.next_if(|row| row["index"] == index)
                .unwrap_or_else(|| {
                    let began = started.get(index).is_some_and(|f| f.load(Relaxed));
                    let status = if began { "cancelled" } else { "never_started" };
                    item_row(index, &item_identity(item), status, None)
                })
        })
        .collect();
    serde_json::to_string(&table).unwrap_or_else(|_| "[]".to_owned())
}

fn identity_from_note(note: &str) -> String {
    note.split_once('=')
        .map(|(_, id)| id.to_owned())
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| note.to_owned())
}

pub(super) fn budget_stop_record(denied: usize) -> TaskErrorRecord {
    TaskErrorRecord::new(
        nika_error::codes::NIKA_1704.to_string(),
        format!(
            "run budget (--max-cost-usd) reached — {denied} iteration(s) were not started \
             (in-flight work completed and was counted)"
        ),
        false,
    )
}

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

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::task::RunResult;

    fn boom(message: &str) -> TaskErrorRecord {
        TaskErrorRecord::new("NIKA-EXEC-001", message, false)
    }

    fn ran(note: &str, result: RunResult) -> RanTask {
        RanTask {
            usage: None,
            note: note.to_owned(),
            retries: Vec::new(),
            agent_events: Vec::new(),
            decisions: Vec::new(),
            cleanup_declassified: Vec::new(),
            evidence: None,
            duration_ms: 0,
            items: None,
            result,
        }
    }

    fn failed_iter(index: usize, item: &str) -> RanTask {
        let identity = item.to_owned();
        ran(
            &iteration_note(index, &identity),
            RunResult::Failed {
                access_refused: None,
                error: TaskErrorRecord::new(
                    "NIKA-EXEC-001",
                    format!("for_each item [{index}] {identity}: boom"),
                    false,
                ),
                cost_usd: None,
                cost_unpriced: None,
                access: None,
            },
        )
    }

    fn ok_iter(index: usize, item: &str) -> RanTask {
        ran(
            &iteration_note(index, item),
            RunResult::Success {
                value: Value::String(item.into()),
                tokens: None,
                recovered_from: None,
                warning: None,
                child: None,
                cost_usd: None,
                cost_unpriced: None,
                model: None,
                access: None,
            },
        )
    }

    /// Collect a batch whose remaining iterations never complete: a
    /// collector that waited for them fails the bound instead of hanging.
    async fn collect_then_nothing(done: Vec<(usize, RanTask)>, fail_fast: bool) -> FanOutAccum {
        let mut stream = futures_util::stream::iter(done).chain(futures_util::stream::pending());
        let collected = collect_fan_out(&mut stream, 3, fail_fast);
        tokio::time::timeout(std::time::Duration::from_secs(5), collected)
            .await
            .expect("the first completed failure stops the batch")
    }

    /// The table of `alpha · beta · gamma` where `began` items were polled.
    fn table(acc: FanOutAccum, began: &[usize]) -> Vec<Value> {
        let items = ["alpha", "beta", "gamma"].map(Value::from);
        let started = unstarted(3);
        for index in began {
            started[*index].store(true, Relaxed);
        }
        serde_json::from_str(&items_json(acc.items, &items, &started)).expect("rows")
    }

    /// B10 · spec 03: iterations arrive in COMPLETION order. A later item's
    /// failure that completes first stops the batch at once, whatever its
    /// index: the rows still read in input order, the slower earlier sibling
    /// that began is `cancelled`, the queued one `never_started`, and the
    /// parent error is the failure that completed.
    #[tokio::test]
    async fn a_later_failure_that_completes_first_stops_the_batch() {
        let acc = collect_then_nothing(vec![(1, failed_iter(1, "beta"))], true).await;
        let error = acc.first_error.clone().expect("the completed failure");
        assert!(error.message.contains("beta"), "{}", error.message);
        let rows = table(acc, &[0, 1]);
        let words: Vec<&str> = rows.iter().filter_map(|r| r["status"].as_str()).collect();
        assert_eq!(words, ["cancelled", "failed", "never_started"]);
        assert_eq!(rows[1]["code"], "NIKA-EXEC-001");
    }

    /// B10 · a sibling that completed before the stop keeps its own terminal:
    /// a success is never relabelled `cancelled` because an earlier-index
    /// sibling was slower than the failure.
    #[tokio::test]
    async fn a_success_completed_before_the_stop_keeps_its_row() {
        let done = vec![(2, ok_iter(2, "gamma")), (1, failed_iter(1, "beta"))];
        let acc = collect_then_nothing(done, true).await;
        let rows = table(acc, &[0, 1, 2]);
        let words: Vec<&str> = rows.iter().filter_map(|r| r["status"].as_str()).collect();
        assert_eq!(words, ["cancelled", "failed", "ok"]);
    }

    /// B10 · whatever the completion order, the kept iterations fold in INPUT
    /// order: outputs, rows and the parent error read as the input does.
    #[tokio::test]
    async fn completed_iterations_fold_in_input_order() {
        let mut stream = futures_util::stream::iter([
            (2, ok_iter(2, "gamma")),
            (0, ok_iter(0, "alpha")),
            (1, ok_iter(1, "beta")),
        ]);
        let acc = collect_fan_out(&mut stream, 3, true).await;
        assert!(acc.first_error.is_none());
        assert_eq!(acc.outputs, ["alpha", "beta", "gamma"].map(Value::from));
        let indexes: Vec<&Value> = acc.items.iter().map(|r| &r["index"]).collect();
        assert_eq!(
            indexes,
            [0, 1, 2].map(Value::from).iter().collect::<Vec<_>>()
        );
    }

    #[test]
    fn item_identity_strings_are_bare() {
        assert_eq!(item_identity(&Value::String("gamma".into())), "gamma");
        assert_eq!(item_identity(&serde_json::json!({"k": 1})), r#"{"k":1}"#);
    }

    #[test]
    fn item_identity_truncates_huge_values() {
        let huge = "x".repeat(200);
        let id = item_identity(&Value::String(huge));
        assert_eq!(id.chars().count(), ITEM_IDENTITY_MAX);
        assert!(
            id.ends_with('…'),
            "truncated identity ends with an ellipsis: {id}"
        );
    }

    #[test]
    fn fan_note_healthy_stays_count_only() {
        assert_eq!(fan_note(3, 0, &[], &[]), "for_each · 3 items");
    }

    #[test]
    fn fan_note_names_failed_items() {
        let failed = ["beta".to_owned(), "gamma".to_owned()];
        assert_eq!(
            fan_note(3, 0, &failed, &[]),
            "2 of 3 items failed: beta, gamma"
        );
    }

    #[test]
    fn fan_note_names_recovered_items() {
        let recovered = ["gamma".to_owned()];
        assert_eq!(
            fan_note(3, 1, &[], &recovered),
            "for_each · 2/3 ok · 1 recovered: gamma"
        );
    }

    #[test]
    fn annotate_keeps_the_original_code() {
        let mut error = boom("command exited with status 1:");
        annotate_error_in_place(&mut error, 2, "gamma");
        assert_eq!(error.code, "NIKA-EXEC-001");
        assert!(
            error.message.contains("gamma"),
            "the item name is in the message: {}",
            error.message
        );
        assert!(
            error.message.contains("for_each item [2]"),
            "index + item, not a count: {}",
            error.message
        );
        annotate_error_in_place(&mut error, 9, "other");
        assert!(
            !error.message.contains("other"),
            "a second stamp must not double-prefix: {}",
            error.message
        );
    }

    #[tokio::test]
    async fn collect_fan_out_keeps_every_failed_identity() {
        let mut stream = futures_util::stream::iter([
            (0, failed_iter(0, "alpha")),
            (1, failed_iter(1, "beta")),
            (2, failed_iter(2, "gamma")),
        ]);
        let acc = collect_fan_out(&mut stream, 3, false).await;
        assert_eq!(
            acc.failed_items,
            vec!["alpha", "beta", "gamma"],
            "fail_fast:false collects every named item, not only first_error"
        );
        let first = acc.first_error.expect("first failure is the parent error");
        assert_eq!(first.code, "NIKA-EXEC-001");
        assert!(
            first.message.contains("alpha"),
            "the first error names its item: {}",
            first.message
        );
        assert_eq!(
            fan_note(3, acc.recovered, &acc.failed_items, &acc.recovered_items),
            "3 of 3 items failed: alpha, beta, gamma"
        );
        // #1276 · every failure reaches the table, with its code.
        assert_eq!(acc.items.len(), 3, "one row per item: {:?}", acc.items);
        for (index, row) in acc.items.iter().enumerate() {
            assert_eq!(row["index"], index, "{row}");
            assert_eq!(row["status"], "failed", "{row}");
            assert_eq!(row["code"], "NIKA-EXEC-001", "{row}");
        }
        assert_eq!(acc.items[1]["item"], "beta");
    }

    /// #1397 · the items a stopped batch never started are named as such,
    /// after the consumed rows, in input order.
    #[test]
    fn items_json_names_the_never_started_tail() {
        let items = [
            Value::String("alpha".into()),
            Value::String("beta".into()),
            Value::String("gamma".into()),
        ];
        let consumed = vec![item_row(0, "alpha", "ok", None)];
        let text = items_json(consumed, &items, &unstarted(3));
        let rows: Vec<Value> = serde_json::from_str(&text).expect("a JSON array");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["status"], "ok");
        assert_eq!(rows[1]["status"], "never_started");
        assert_eq!(rows[1]["item"], "beta");
        assert_eq!(rows[2]["index"], 2);
        assert!(
            rows[1].get("code").is_none(),
            "no error on a never-started row"
        );
    }

    /// B8 · spec 03/17: an item whose iteration began but left no recorded
    /// terminal is `cancelled`; one that never began stays `never_started`.
    /// Neither row carries a code, a message or an output.
    #[test]
    fn items_json_tells_a_began_item_from_one_never_started() {
        let items = [
            Value::String("alpha".into()),
            Value::String("beta".into()),
            Value::String("gamma".into()),
        ];
        let started = unstarted(3);
        started[0].store(true, Relaxed);
        started[1].store(true, Relaxed);
        let consumed = vec![item_row(0, "alpha", "failed", Some(&boom("x")))];
        let text = items_json(consumed, &items, &started);
        let rows: Vec<Value> = serde_json::from_str(&text).expect("a JSON array");
        let words: Vec<&str> = rows.iter().filter_map(|r| r["status"].as_str()).collect();
        assert_eq!(words, ["failed", "cancelled", "never_started"]);
        for row in &rows[1..] {
            assert_eq!(row.as_object().map(serde_json::Map::len), Some(3), "{row}");
        }
    }

    /// B8 · building the iteration's future is not execution: the flag rises
    /// on the first poll, never at construction.
    #[test]
    fn the_started_flag_rises_on_the_first_poll_only() {
        use std::future::Future as _;
        let flag = AtomicBool::new(false);
        let mut iteration = std::pin::pin!(started_on_first_poll(
            0,
            Some(&flag),
            std::future::pending::<()>()
        ));
        assert!(!flag.load(Relaxed), "constructed, not polled");
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(iteration.as_mut().poll(&mut cx).is_pending());
        assert!(flag.load(Relaxed), "the first poll began it");
    }

    #[tokio::test]
    async fn collect_fan_out_skip_keeps_the_item() {
        let skip = ran(
            "for_each[1]=beta",
            RunResult::SkippedWithError {
                error: boom("for_each item [1] beta: boom"),
                cost_usd: None,
                cost_unpriced: None,
            },
        );
        let ok = ran(
            "for_each[0]=alpha",
            RunResult::Success {
                value: Value::String("ok".into()),
                tokens: None,
                recovered_from: None,
                warning: None,
                child: None,
                cost_usd: None,
                cost_unpriced: None,
                model: None,
                access: None,
            },
        );
        let mut stream = futures_util::stream::iter([(0, ok), (1, skip)]);
        let acc = collect_fan_out(&mut stream, 2, false).await;
        assert!(acc.first_error.is_none(), "skip does not fail the parent");
        assert_eq!(acc.recovered_items, vec!["beta"]);
        let kept = acc
            .first_recovered_from
            .expect("skip preserves the original error");
        assert!(
            kept.message.contains("beta"),
            "the recovered witness names the skipped item: {}",
            kept.message
        );
        assert_eq!(
            fan_note(2, acc.recovered, &acc.failed_items, &acc.recovered_items),
            "for_each · 1/2 ok · 1 recovered: beta"
        );
    }

    #[test]
    fn stamp_iteration_puts_item_on_note_and_error() {
        let mut ran = ran(
            "exec · false",
            RunResult::Failed {
                access_refused: None,
                error: boom("command exited with status 1:"),
                cost_usd: None,
                cost_unpriced: None,
                access: None,
            },
        );
        stamp_iteration(&mut ran, 2, &Value::String("gamma".into()));
        assert_eq!(ran.note, "for_each[2]=gamma");
        match ran.result {
            RunResult::Failed { error, .. } => {
                assert_eq!(error.code, "NIKA-EXEC-001");
                assert_eq!(
                    error.message,
                    "for_each item [2] gamma: command exited with status 1:"
                );
            }
            RunResult::Success { .. }
            | RunResult::SkippedWithError { .. }
            | RunResult::PendingRecovery(_) => {
                panic!("expected Failed after stamping a failed iteration")
            }
        }
    }
}
