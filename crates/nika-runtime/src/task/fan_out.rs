// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `for_each:` fan-out (spec 03) — collection resolve, ordered fold.
//! Dispatch (`run_fan_out` · `run_iteration`) stays in the parent.

use std::collections::BTreeMap;

use futures_util::StreamExt;
use nika_schema::raw::ForEachValue;
use serde_json::Value;

use super::{RanTask, RetryStamp, RunResult, SettleAs, VAR_TYPE_CODE, runtime_error_record};
use crate::errors::RuntimeError;
use crate::expr::{self, Scope};
use crate::record::TaskErrorRecord;

/// Collection over item-free boundary bindings. Empty → `skipped`.
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
}

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
        tokens_sum: None,
        cost_sum: None,
        unpriced: None,
        recovered: 0,
        failed_items: Vec::new(),
        recovered_items: Vec::new(),
        first_recovered_from: None,
    };

    while let Some(iter_ran) = stream.next().await {
        if consume_iteration(&mut acc, iter_ran, fail_fast) {
            break;
        }
    }
    acc
}

fn consume_iteration(acc: &mut FanOutAccum, iter_ran: RanTask, fail_fast: bool) -> bool {
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
            if let Some(original) = recovered_from {
                acc.recovered += 1;
                acc.recovered_items.push(identity);
                if acc.first_recovered_from.is_none() {
                    acc.first_recovered_from = Some(original);
                }
            }
            acc.outputs.push(value);
            fold_spend(acc, tokens, cost_usd, cost_unpriced);
            false
        }
        RunResult::SkippedWithError { error, .. } => {
            acc.recovered += 1;
            acc.recovered_items.push(identity);
            acc.outputs.push(Value::Null);
            if acc.first_recovered_from.is_none() {
                acc.first_recovered_from = Some(error);
            }
            false
        }
        RunResult::Failed { error, .. } => {
            acc.outputs.push(Value::Null);
            acc.failed_items.push(identity);
            if acc.first_error.is_none() {
                acc.first_error = Some(error);
            }
            fail_fast
        }
        RunResult::PendingRecovery(pending) => {
            acc.outputs.push(Value::Null);
            acc.failed_items.push(identity);
            if acc.first_error.is_none() {
                acc.first_error = Some(pending.render_error);
            }
            fail_fast
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

fn resolve_collection(
    collection: &ForEachValue,
    scope: &Scope<'_>,
) -> Result<Vec<Value>, Box<SettleAs>> {
    let resolved = match collection {
        ForEachValue::List(value) => expr::render_json(value, scope),
        ForEachValue::Expression(text) => expr::render_json(&Value::String(text.clone()), scope),
        other => Err(RuntimeError::WhenUnsupported {
            expr: format!("for_each form not wired in the runtime yet: {other:?}"),
        }),
    };
    match resolved {
        Ok(Value::Array(items)) => Ok(items),
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
        },
        Some(error) => RunResult::Failed {
            error,
            cost_usd,
            cost_unpriced,
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

fn identity_from_note(note: &str) -> String {
    note.split_once('=')
        .map(|(_, id)| id.to_owned())
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| note.to_owned())
}

pub(super) fn budget_stop_record(denied: usize) -> TaskErrorRecord {
    TaskErrorRecord {
        code: nika_error::codes::NIKA_1704.to_string(),
        message: format!(
            "run budget (--max-cost-usd) reached — {denied} iteration(s) were not started \
             (in-flight work completed and was counted)"
        ),
        transient: false,
    }
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
        TaskErrorRecord {
            code: "NIKA-EXEC-001".to_owned(),
            message: message.to_owned(),
            transient: false,
        }
    }

    fn ran(note: &str, result: RunResult) -> RanTask {
        RanTask {
            note: note.to_owned(),
            retries: Vec::new(),
            agent_events: Vec::new(),
            decisions: Vec::new(),
            evidence: None,
            duration_ms: 0,
            result,
        }
    }

    fn failed_iter(index: usize, item: &str) -> RanTask {
        let identity = item.to_owned();
        ran(
            &iteration_note(index, &identity),
            RunResult::Failed {
                error: TaskErrorRecord {
                    code: "NIKA-EXEC-001".to_owned(),
                    message: format!("for_each item [{index}] {identity}: boom"),
                    transient: false,
                },
                cost_usd: None,
                cost_unpriced: None,
            },
        )
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
            failed_iter(0, "alpha"),
            failed_iter(1, "beta"),
            failed_iter(2, "gamma"),
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
            },
        );
        let mut stream = futures_util::stream::iter([ok, skip]);
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
                error: boom("command exited with status 1:"),
                cost_usd: None,
                cost_unpriced: None,
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
