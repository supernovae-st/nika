// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `with:` island map (spec 03 §dispatch pipeline · spec 04) — the
//! boundary render for a whole task, and the per-iteration render a
//! fan-out lane needs. Split from `task.rs` at the 1500-LOC cap: the
//! two renders + the loop-local probe that decides between them are
//! one subject, and nothing else in the pipeline reads them.

use std::collections::BTreeMap;

use nika_schema::raw::RawTask;
use nika_schema::source::Spanned;
use serde_json::Value;

use crate::errors::RuntimeError;
use crate::expr::{self, Scope};
use crate::record::TaskRecord;

/// The boundary `with:` render (spec 03 §dispatch pipeline) — ALL
/// bindings for a single-lane task · only the loop-local-free ones for
/// a fan-out task (the item/index-bound ones re-render per iteration).
/// `tasks` is the workflow's task list: a `${{ group.<name> }}` fold
/// resolves against the membership its `group:` keys declare.
pub(super) fn render_boundary_with(
    task: &RawTask,
    tasks: &[Spanned<RawTask>],
    records: &BTreeMap<String, TaskRecord>,
    inputs: &BTreeMap<String, Value>,
    consts: &BTreeMap<String, Value>,
    secrets: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, RuntimeError> {
    // `with: { tok: "${{ secrets.X }}" }` resolves here (MINOR-B); rendering
    // performs no effect, so the task context carries no permits.
    let groups = folded_groups(task, tasks);
    let scope = Scope::workflow_with_value_authorities(records, inputs, consts, secrets)
        .with_groups(&groups);
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

/// The DECLARED membership of every group this task folds (spec 03
/// §group) — group name → member ids in DECLARATION order, the same
/// derivation the checker's `fan-in` edges ride. Empty when the task
/// folds nothing: the common lane pays one island scan of its own
/// `with:` and never walks `tasks:`. Membership is declared, never
/// matched — the checker refused a fold of a group no task declares
/// (`NIKA-DAG-008`), so a name absent here is unreachable at run time
/// and would read loud (NIKA-1702) if it ever were.
fn folded_groups(task: &RawTask, tasks: &[Spanned<RawTask>]) -> BTreeMap<String, Vec<String>> {
    use nika_check::analyzer::edges::{group_members, group_refs_in_value};
    let mut names = Vec::new();
    for (_key, value) in &task.with {
        group_refs_in_value(&value.value, &mut names);
    }
    if names.is_empty() {
        return BTreeMap::new();
    }
    let members = group_members(tasks);
    names
        .into_iter()
        .filter_map(|name| {
            let ids = members
                .get(name.as_str())?
                .iter()
                .filter_map(|&i| tasks.get(i).map(|t| t.value.id.value.clone()))
                .collect();
            Some((name, ids))
        })
        .collect()
}

/// Render the task's `with:` map (spec 03 · per-iteration in fan-out ·
/// entries cannot reference each other · spec 04). `tasks` binds the
/// fan-in membership exactly as the boundary render does, so a fold
/// re-rendered inside an iteration resolves to the same array.
#[allow(clippy::too_many_arguments)] // the run-scoped reads + the loop locals
pub(super) fn render_with(
    task: &RawTask,
    tasks: &[Spanned<RawTask>],
    records: &BTreeMap<String, TaskRecord>,
    inputs: &BTreeMap<String, Value>,
    consts: &BTreeMap<String, Value>,
    secrets: &BTreeMap<String, Value>,
    item: Option<&Value>,
    index: Option<usize>,
) -> Result<BTreeMap<String, Value>, RuntimeError> {
    // `with: { tok: "${{ secrets.X }}" }` resolves here (MINOR-B); rendering
    // performs no effect, so the task context carries no permits.
    let groups = folded_groups(task, tasks);
    let scope = Scope::workflow_with_value_authorities(records, inputs, consts, secrets)
        .with_groups(&groups)
        .with_task_context(None, item, index, None);
    task.with
        .iter()
        .map(|(key, value)| Ok((key.value.clone(), expr::render_json(&value.value, &scope)?)))
        .collect()
}
