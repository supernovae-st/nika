// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The values a rehearsal room refuses before any run: a host's screen applies this law last.
//!
//! Evaluated fields can construct values synchronously without a rendered-byte cap.
//! A write's arguments are rendered before its filesystem byte reservation; output and model
//! values are checked at their own stages. Three source forms can amplify referenced values:
//! two template islands or more in one string, a CEL list holding a value reference, and a JSON
//! array or object holding a template that reads a value. Each is refused before any room, from
//! the parsed form the shared scanner gives, never by evaluating it; a template the scanner
//! cannot read is refused too. Constants, input defaults, keys, metadata and the bytes a
//! rehearsal copies are data, never scanned. This closes these forms; it bounds neither memory
//! as a whole nor the size of one value read whole. Every refusal it states is a
//! [`Refusal::DataBounds`](super::Refusal::DataBounds).

use nika_schema::expression::{Expr, expr_refs, scan_templates};
use nika_schema::raw::{ForEachValue, RawAction, RawTask, RawWorkflow};
use nika_schema::types::OnErrorAction;
use serde_json::Value;

#[cfg(test)]
mod tests;

/// Refuse the three amplifying forms described above in evaluated values: every task,
/// whatever its condition or branch, then the workflow's outputs and its model.
///
/// # Errors
/// The words of the first refused value, in those of its field.
pub fn evaluated(workflow: &RawWorkflow) -> Result<(), String> {
    for task in &workflow.tasks {
        task_values(&task.value)?;
    }
    for (name, declared) in &workflow.outputs {
        let at = format!("outputs.{}", name.value);
        text(&at, &declared.value().value, false)?;
    }
    if let Some(model) = &workflow.model {
        text("model", &model.value, false)?;
    }
    Ok(())
}

/// The evaluated values of one task: each binding, its condition, its fan-out collection, each
/// argument and its recovery value. The `with` and `args` maps are envelopes: each of their
/// values is evaluated on its own.
fn task_values(task: &RawTask) -> Result<(), String> {
    let id = task.id.value.as_str();
    for (name, value) in &task.with {
        root(&format!("task {id} with.{}", name.value), &value.value)?;
    }
    if let Some(condition) = task.when.as_ref().and_then(|gate| gate.value.as_expr()) {
        text(&format!("task {id} when"), condition, false)?;
    }
    if let Some(fan) = &task.for_each {
        let at = format!("task {id} for_each");
        match &fan.value {
            ForEachValue::Expression(source) => text(&at, source, false)?,
            ForEachValue::List(list) => container(&at, list)?,
            _ => {
                return Err(bound(
                    &at,
                    "holds a collection form the screen does not know",
                ));
            }
        }
    }
    if let RawAction::Invoke(invoke) = &task.action
        && let Some(args) = &invoke.args
    {
        match &args.value {
            Value::Object(fields) => {
                for (name, value) in fields {
                    root(&format!("task {id} args.{name}"), value)?;
                }
            }
            other => root(&format!("task {id} args"), other)?,
        }
    }
    if let Some(policy) = &task.on_error
        && let OnErrorAction::Recover(value) = &policy.value.action
    {
        root(&format!("task {id} on_error.recover"), &value.value)?;
    }
    Ok(())
}

/// One evaluated value: a string is scanned, an array or an object is a container the run
/// builds, and any other scalar is data.
fn root(at: &str, value: &Value) -> Result<(), String> {
    match value {
        Value::String(source) => text(at, source, false),
        Value::Array(_) | Value::Object(_) => container(at, value),
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(()),
    }
}

/// The values, never the keys, of an array or an object the run builds: each string is scanned
/// as a part of it.
fn container(at: &str, value: &Value) -> Result<(), String> {
    match value {
        Value::String(source) => text(at, source, true),
        Value::Array(items) => items.iter().try_for_each(|item| container(at, item)),
        Value::Object(fields) => fields.values().try_for_each(|item| container(at, item)),
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(()),
    }
}

/// One evaluated string, scanned by the shared scanner: refused when the scan fails, when it
/// holds two islands or more, when an island builds a list holding a value reference, or, as a
/// part of an array or an object, when an island reads a value.
fn text(at: &str, source: &str, contained: bool) -> Result<(), String> {
    let islands = scan_templates(source).map_err(|error| {
        bound(
            at,
            &format!("holds a template the screen cannot read ({error})"),
        )
    })?;
    if islands.len() > 1 {
        let joined = format!("joins {} template islands in one value", islands.len());
        return Err(bound(at, &joined));
    }
    for island in &islands {
        if builds_list(&island.expr) {
            return Err(bound(at, "builds a list of values"));
        }
        if contained && !expr_refs(&island.expr).is_empty() {
            return Err(bound(at, "builds an array or an object of values"));
        }
    }
    Ok(())
}

/// Whether `expr` builds a list holding a value reference, wherever the list sits: under a
/// call, a method, a relation, a branch, a member or an index.
fn builds_list(expr: &Expr) -> bool {
    match expr {
        Expr::List(_) => !expr_refs(expr).is_empty(),
        Expr::Or(lhs, rhs) | Expr::And(lhs, rhs) | Expr::Relation { lhs, rhs, .. } => {
            builds_list(lhs) || builds_list(rhs)
        }
        Expr::Not(inner)
        | Expr::SizeCall(inner)
        | Expr::SizeMethod(inner)
        | Expr::HasCall(inner) => builds_list(inner),
        Expr::Ternary { cond, then, else_ } => {
            builds_list(cond) || builds_list(then) || builds_list(else_)
        }
        Expr::StringMethod { base, arg, .. } => builds_list(base) || builds_list(arg),
        Expr::Member { base, .. } => builds_list(base),
        Expr::Index { base, index } => builds_list(base) || builds_list(index),
        Expr::Ident(_) | Expr::Lit(_) => false,
    }
}

/// A value the run would build before the room's write budget sees it, refused in the words of
/// its field.
fn bound(at: &str, what: &str) -> String {
    format!(
        "{at} {what}: the run builds it before the room's write budget sees it, and a rehearsal \
         vouches for no bound on it"
    )
}
