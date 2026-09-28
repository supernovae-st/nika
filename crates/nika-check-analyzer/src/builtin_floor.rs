// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The static builtin floor and the unpriced-cloud class: what the catalog
//! says a workflow's priced `invoke:` tasks must spend on their cheapest path,
//! and whether a seat is a third-party cloud route with no price. Descended
//! verbatim from `nika-runtime`'s `admit.rs` (2026-09-28 · B9 phase C): pure
//! reads of the workflow and the catalog, which this plane already judges
//! against, moved so the runtime's 15k wall has room for the dispatch guard
//! that shares them. The runtime's launch gates still own every refusal.

use nika_schema::raw::{ForEachValue, RawAction, RawTask, RawWorkflow};
use serde_json::Value;

/// A recognized third-party cloud seat with no snapshot row. Unknown
/// providers stay unknown (never promoted to cloud — but NOT spared:
/// the resolved-id walk's unresolvable arm refuses them under a cap
/// through `nika_providers::resolve_refusal`, #1368). Mock and local
/// are the sparing arms — unpriced, never this class.
#[must_use]
pub fn unpriced_cloud_seat(model: &str) -> bool {
    if model == "mock" || model.starts_with("mock/") {
        return false;
    }
    let provider = model.split_once('/').map_or(model, |(p, _)| p);
    let Some(entry) = nika_catalog::find_provider(provider) else {
        return false;
    };
    let local = entry
        .tags
        .iter()
        .any(|tag| matches!(tag, nika_catalog::Tag::Local))
        || entry
            .data_policy
            .is_some_and(|policy| policy.zdr == "local");
    if local {
        return false;
    }
    nika_catalog::find_pricing_for(model).is_none()
}

/// Unavoidable catalog spend of priced `invoke:` tasks (cheapest path:
/// `when:` closed → $0 · first-try · known `n:` · known `for_each`
/// length). Templated provider/`n` and expression `for_each` stay off
/// this floor — the mid-run ledger still owns what statics cannot see.
#[must_use]
pub fn priced_builtin_floor(wf: &RawWorkflow) -> f64 {
    wf.tasks.iter().map(|t| invoke_static_floor(&t.value)).sum()
}

fn invoke_static_floor(task: &RawTask) -> f64 {
    if task.when.is_some() {
        return 0.0;
    }
    let RawAction::Invoke(inv) = &task.action else {
        return 0.0;
    };
    let Some(tool) = inv.tool() else {
        return 0.0;
    };
    let Some(args) = inv.args.as_ref() else {
        return 0.0;
    };
    let Some(provider) = static_provider(&args.value) else {
        return 0.0;
    };
    let Some(per) = nika_catalog::builtin_provider_floor_usd(&tool.value, provider) else {
        return 0.0;
    };
    per * static_n(&args.value) * static_iterations(task)
}

fn static_provider(args: &Value) -> Option<&str> {
    if let Some(provider) = args.get("provider").and_then(Value::as_str) {
        return static_literal(provider);
    }
    let model = args.get("model").and_then(Value::as_str)?;
    let model = static_literal(model)?;
    model.contains("grok-imagine").then_some("xai")
}

fn static_literal(s: &str) -> Option<&str> {
    (!s.contains("${{")).then_some(s)
}

fn static_n(args: &Value) -> f64 {
    #[allow(clippy::cast_precision_loss)] // image `n:` is capped at 10
    args.get("n")
        .and_then(Value::as_u64)
        .map_or(1.0, |n| n.max(1) as f64)
}

fn static_iterations(task: &RawTask) -> f64 {
    match task.for_each.as_ref().map(|f| &f.value) {
        None => 1.0,
        Some(ForEachValue::List(arr)) => {
            #[allow(clippy::cast_precision_loss)] // literal list length is a task count
            {
                arr.as_array().map_or(1, Vec::len) as f64
            }
        }
        // Unknown count: cheapest path cannot claim a floor (NIKA-1704).
        Some(ForEachValue::Expression(_)) => 0.0,
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and runtime ship together"
        )]
        _ => unreachable!("unsupported for_each form"),
    }
}
