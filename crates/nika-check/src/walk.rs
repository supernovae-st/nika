// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Walking the workflow's text surfaces — the traversal half of the hint
//! lane, split out of `hints.rs` under the ADR-023 1,500-LOC ceiling.
//!
//! These functions answer structural questions by VISITING the workflow
//! (which outputs are referenced anywhere · which strings live inside a
//! `for_each` island · what an action's text fields are · whether an arg
//! resolves to a literal). They emit no hints and hold no policy, which
//! is exactly why they belong beside the hint lane rather than inside it:
//! a traversal is reusable, a hint is a judgement.

use std::collections::BTreeSet;

use nika_schema::expression::{bare_task_refs, scan_templates, task_output_paths};
use nika_schema::raw::{RawAction, RawWorkflow};

// The static-value resolver descended to the analysis substrate
// (`static_ref.rs`) with the thinking-seat law (2026-08-25 · the 15k
// wall). Re-exported here so the in-crate lanes keep their historical
// `walk::` / `crate::` call sites — one resolver, every lane, no drift.
pub(crate) use nika_check_analyzer::{bare_static_ref, static_literal_of};

/// Task ids whose output is referenced ANYWHERE (any `tasks.X.output…`
/// chain in any island, or an envelope `outputs:` entry).
pub(crate) fn consumed_outputs(wf: &RawWorkflow) -> BTreeSet<String> {
    let mut consumed = BTreeSet::new();
    for_each_island_text(wf, &mut |text| {
        if let Ok(islands) = scan_templates(text) {
            for island in islands {
                for (target, _) in task_output_paths(&island.expr) {
                    consumed.insert(target);
                }
            }
        }
    });
    consumed
}

/// `(output name, task id)` for every `outputs:` binding that
/// references a BARE task envelope (`tasks.X` — no field hop). Scoped
/// to `outputs:` deliberately: a bare envelope in a gate or a prompt is
/// legitimate plumbing; bound into the workflow's public contract it is
/// the golden-drift trap.
pub(crate) fn envelope_bound_outputs(wf: &RawWorkflow) -> Vec<(String, String)> {
    let mut bound = Vec::new();
    for (name, decl) in &wf.outputs {
        if let Ok(islands) = scan_templates(&decl.value().value) {
            for island in islands {
                for id in bare_task_refs(&island.expr) {
                    bound.push((name.value.clone(), id));
                }
            }
        }
    }
    bound
}

/// Task ids referenced with a DEEP path (`tasks.X.output.field…`).
pub(crate) fn deeply_referenced(wf: &RawWorkflow) -> BTreeSet<String> {
    let mut deep = BTreeSet::new();
    for_each_island_text(wf, &mut |text| {
        if let Ok(islands) = scan_templates(text) {
            for island in islands {
                for (target, path) in task_output_paths(&island.expr) {
                    if !path.is_empty() {
                        deep.insert(target);
                    }
                }
            }
        }
    });
    deep
}

/// Visit every expression-bearing text in the workflow (the same surface
/// the dataflow typer walks: verbs · `when:` · `with:` · `for_each` ·
/// `on_finally` · envelope `outputs:`).
fn for_each_island_text(wf: &RawWorkflow, visit: &mut dyn FnMut(&str)) {
    for task in &wf.tasks {
        let t = &task.value;
        visit_action(&t.action, visit);
        if let Some(when) = &t.when
            && let Some(expr) = when.value.as_expr()
        {
            visit(expr);
        }
        if let Some(f) = &t.for_each
            && let nika_schema::raw::ForEachValue::Expression(src) = &f.value
        {
            visit(src);
        }
        for (_, v) in &t.with {
            visit_json(&v.value, visit);
        }
    }
    for (_, decl) in &wf.outputs {
        visit(&decl.value().value);
    }
}

fn visit_action(action: &RawAction, visit: &mut dyn FnMut(&str)) {
    match action {
        RawAction::Exec(a) => {
            for fragment in a.command.text_fragments() {
                visit(fragment);
            }
            if let Some(stdin) = &a.stdin {
                visit(&stdin.value);
            }
            for (_, v) in &a.env {
                visit(&v.value);
            }
        }
        RawAction::Invoke(a) => {
            if let Some(args) = &a.args {
                visit_json(&args.value, visit);
            }
        }
        RawAction::Infer(a) => {
            visit(&a.prompt.value);
            if let Some(system) = &a.system {
                visit(&system.value);
            }
        }
        RawAction::Agent(a) => {
            visit(&a.prompt.value);
            if let Some(system) = &a.system {
                visit(&system.value);
            }
        }
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown action: {other:?}"),
    }
}

pub(crate) fn visit_json(value: &serde_json::Value, visit: &mut dyn FnMut(&str)) {
    match value {
        serde_json::Value::String(s) => visit(s),
        serde_json::Value::Array(items) => {
            for item in items {
                visit_json(item, visit);
            }
        }
        serde_json::Value::Object(map) => {
            for item in map.values() {
                visit_json(item, visit);
            }
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
    }
}

/// Resolve an invoke arg to a STATIC string when it is a plain literal
/// or a bare authority ref [`static_literal_of`] resolves to a string
/// literal — the shapes a scaffold ships. Anything dynamic (task refs ·
/// concatenations) resolves to `None`: analysis never guesses.
fn static_string_arg(wf: &RawWorkflow, value: &serde_json::Value) -> Option<String> {
    let s = value.as_str()?;
    let trimmed = s.trim();
    if !trimmed.contains("${{") {
        return Some(trimmed.to_owned());
    }
    static_literal_of(wf, trimmed)?.as_str().map(str::to_owned)
}

/// Every `nika:read` whose `path` arg resolves STATICALLY — the pure
/// half of the missing-input lint (V-arc F1 2026-07-09): the analyzer
/// names the (task · path) pairs, the CALLER decides what existence
/// means on its side of the I/O boundary (the CLI checks the local
/// filesystem; a server might check an artifact store).
#[must_use]
pub fn static_read_paths(wf: &RawWorkflow) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for task in &wf.tasks {
        let RawAction::Invoke(invoke) = &task.value.action else {
            continue;
        };
        if invoke.tool().map(|t| t.value.as_str()) != Some("nika:read") {
            continue;
        }
        let Some(args) = &invoke.args else { continue };
        let Some(path_val) = args.value.get("path") else {
            continue;
        };
        if let Some(path) = static_string_arg(wf, path_val) {
            out.push((task.value.id.value.clone(), path));
        }
    }
    out
}
