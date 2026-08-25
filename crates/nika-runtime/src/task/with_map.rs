// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `with:` island map (spec 03 §dispatch pipeline · spec 04) — the
//! boundary render for a whole task, and the per-iteration render a
//! fan-out lane needs. Split from `task.rs` at the 1500-LOC cap: the
//! two renders + the loop-local probe that decides between them are
//! one subject, and nothing else in the pipeline reads them.

use std::collections::BTreeMap;

use nika_schema::raw::RawTask;
use serde_json::Value;

use crate::errors::RuntimeError;
use crate::expr::{self, Scope};
use crate::record::TaskRecord;

/// The boundary `with:` render (spec 03 §dispatch pipeline) — ALL
/// bindings for a single-lane task · only the loop-local-free ones for
/// a fan-out task (the item/index-bound ones re-render per iteration).
pub(super) fn render_boundary_with(
    task: &RawTask,
    records: &BTreeMap<String, TaskRecord>,
    inputs: &BTreeMap<String, Value>,
    consts: &BTreeMap<String, Value>,
    secrets: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, RuntimeError> {
    // `with: { tok: "${{ secrets.X }}" }` resolves here (MINOR-B); rendering
    // performs no effect, so the task context carries no permits.
    let scope = Scope::workflow_with_value_authorities(records, inputs, consts, secrets);
    let fan_out = task.for_each.is_some();
    task.with
        .iter()
        .filter(|(_key, value)| !(fan_out && references_loop_locals(&value.value)))
        .map(|(key, value)| Ok((key.value.clone(), expr::render_json(&value.value, &scope)?)))
        .collect()
}

/// Whether a JSON value's `${{ }}` islands reference the `for_each`
/// loop-locals (`item` / `index`) — those bindings are per-iteration.
fn references_loop_locals(value: &Value) -> bool {
    use nika_schema::expression::{NamespaceRef, expr_refs, scan_templates};
    match value {
        Value::String(s) => {
            let Ok(islands) = scan_templates(s) else {
                return false;
            };
            islands.iter().any(|island| {
                expr_refs(&island.expr)
                    .into_iter()
                    .any(|r| matches!(r, NamespaceRef::Item | NamespaceRef::Index))
            })
        }
        Value::Array(items) => items.iter().any(references_loop_locals),
        Value::Object(map) => map.values().any(references_loop_locals),
        _ => false,
    }
}

/// Render the task's `with:` map (spec 03 · per-iteration in fan-out ·
/// entries cannot reference each other · spec 04).
pub(super) fn render_with(
    task: &RawTask,
    records: &BTreeMap<String, TaskRecord>,
    inputs: &BTreeMap<String, Value>,
    consts: &BTreeMap<String, Value>,
    secrets: &BTreeMap<String, Value>,
    item: Option<&Value>,
    index: Option<usize>,
) -> Result<BTreeMap<String, Value>, RuntimeError> {
    // `with: { tok: "${{ secrets.X }}" }` resolves here (MINOR-B); rendering
    // performs no effect, so the task context carries no permits.
    let scope = Scope::workflow_with_value_authorities(records, inputs, consts, secrets)
        .with_task_context(None, item, index, None);
    task.with
        .iter()
        .map(|(key, value)| Ok((key.value.clone(), expr::render_json(&value.value, &scope)?)))
        .collect()
}
