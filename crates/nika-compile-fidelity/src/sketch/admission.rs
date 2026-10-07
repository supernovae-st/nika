// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The pure admission of a sketch and its fills, before any document is accepted: the
//! structural judge of a graph (the sketch's own laws, its reach, then the fidelity laws over the
//! document it states) and the validation of the fills against the accepted graph and each
//! builtin's contract (`nika_cap`), a refusal never repeating a value a fill proposed. Pure over
//! the request, the reader's plan and the sketch: the sketch door's orchestration calls it.

use crate::fidelity::{self, Diagnostic};
use crate::sketch::{self as ir, Fill, Sketch};
use nika_compile_reader::lexicon::Reading;
use serde_json::{Value, json};

/// The structural judge of a sketch: the sketch's own laws, then the fidelity laws over the
/// document it states before any hole is filled (the stated paths, the approval, the
/// prohibitions, no invented literal).
#[must_use]
pub fn judge_sketch(
    intent: &str,
    reading: &Reading,
    sketch: &Sketch,
    (allowed, clarified): (&[String], &[String]),
    observed: Option<&Value>,
) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = ir::structural_laws_observed(sketch, intent, allowed, observed)
        .into_iter()
        .map(|message| Diagnostic {
            kind: "sketch",
            message,
        })
        .collect();
    if out.is_empty() {
        out.extend(reach_laws(sketch));
    }
    if out.is_empty() {
        let doc = ir::document(sketch, &[]);
        fidelity::laws_observed(
            intent,
            &reading.plan,
            &doc,
            allowed,
            &[],
            clarified,
            observed,
            &mut out,
        );
    }
    out.dedup();
    out
}

/// The reach laws a graph must hold before any hole is filled, since no fill can repair them:
/// each side a builtin always reaches (`nika_cap::required_fs_directions`, e.g. a chart's write,
/// an edit's read and write) is stated in the task's `reads`/`writes`, and every path the sketch
/// itself derives for the task (the partial projection's arguments) is bound to that reach on
/// each side its effect touches (`nika_cap::unbound_fs_args`). Optional slots and inline data are
/// never required here; a filled argument is judged again at emission.
#[must_use]
pub fn reach_laws(sketch: &Sketch) -> Vec<Diagnostic> {
    let doc = ir::document(sketch, &[]);
    let mut out = Vec::new();
    // An agent's whitelist is its own; a tool whose calls can reach a file, a host or a process is
    // not yet representable in a sketch agent (its effects have no stated reach here).
    for task in sketch.tasks.iter().filter(|t| t.verb == ir::Verb::Agent) {
        for tool in task.tools.iter().flatten() {
            if !nika_cap::pure_internal_for_all_calls(tool) {
                out.push(Diagnostic {
                    kind: "sketch",
                    message: format!(
                        "`{}` lists `{tool}`, a tool with effects a sketch agent cannot yet carry: use an invoke task that states its reach, or only effect-free tools",
                        task.id
                    ),
                });
            }
        }
    }
    for task in sketch.tasks.iter().filter(|t| t.verb == ir::Verb::Invoke) {
        let Some(tool) = task.tool.as_deref() else {
            continue;
        };
        let mut push = |message: String| {
            out.push(Diagnostic {
                kind: "sketch",
                message,
            });
        };
        if let Some((read, write)) = nika_cap::required_fs_directions(tool) {
            for (needed, stated, side) in [
                (read, &task.reads, "reads"),
                (write, &task.writes, "writes"),
            ] {
                if needed && stated.is_empty() {
                    push(format!(
                        "`{}` invokes `{tool}`, which always {side} a file: state that path in its `{side}`",
                        task.id
                    ));
                }
            }
        }
        let derived = doc["tasks"][task.id.as_str()]["invoke"].get("args");
        for finding in nika_cap::unbound_fs_args(tool, derived, &task.reads, &task.writes) {
            push(format!("`{}`: {finding}", task.id));
        }
    }
    out
}

/// The fills as the compiler consumes them and the complete document they state, or every
/// refusal before any document exists: the fill laws of the accepted sketch, then each invoke's
/// builtin contract (`nika_cap`) over the arguments it would carry. A refusal never repeats a
/// value a fill proposed.
///
/// # Errors
/// Every refusal of the fill laws, then of each builtin's contract, as diagnostics.
pub fn validated(sketch: &Sketch, raw: &[Value]) -> Result<(Vec<Fill>, Value), Vec<Diagnostic>> {
    let refused = |messages: Vec<String>| -> Vec<Diagnostic> {
        messages
            .into_iter()
            .map(|message| Diagnostic {
                kind: "fill",
                message,
            })
            .collect()
    };
    let fills = ir::fills_from_json(&json!({"fills": raw})).map_err(|m| refused(vec![m]))?;
    let doc = ir::complete_document(sketch, &fills).map_err(refused)?;
    let mut findings = Vec::new();
    for (id, node) in doc["tasks"].as_object().into_iter().flatten() {
        let Some(tool) = node["invoke"]["tool"].as_str() else {
            continue;
        };
        let args = node["invoke"].get("args");
        for finding in nika_cap::builtin_shape_findings(tool, args) {
            findings.push(format!(
                "task `{id}` (`{tool}`): {}",
                redacted(&finding, &fills)
            ));
        }
        // Each filesystem argument the builtin contract names is bound to THIS task's stated
        // reach, never to the union of permits (`nika_cap::unbound_fs_args`, the effect owner).
        if let Some(task) = sketch.tasks.iter().find(|t| &t.id == id) {
            for finding in nika_cap::unbound_fs_args(tool, args, &task.reads, &task.writes) {
                findings.push(format!("task `{id}`: {finding}"));
            }
        }
    }
    if findings.is_empty() {
        Ok((fills, doc))
    } else {
        Err(refused(findings))
    }
}

/// A contract finding with every string a fill proposed (four characters or more) replaced, so
/// the record names the broken rule without repeating the refused value.
fn redacted(message: &str, fills: &[Fill]) -> String {
    fn leaves(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::String(text) if text.chars().count() >= 4 => out.push(text.clone()),
            Value::Array(items) => items.iter().for_each(|v| leaves(v, out)),
            Value::Object(map) => map.values().for_each(|v| leaves(v, out)),
            _ => {}
        }
    }
    let mut texts = Vec::new();
    for fill in fills {
        leaves(&fill.value, &mut texts);
    }
    texts.sort_by_key(|t| std::cmp::Reverse(t.len()));
    let mut out = message.to_owned();
    for text in texts {
        out = out.replace(&text, "<proposed value>");
    }
    out
}
