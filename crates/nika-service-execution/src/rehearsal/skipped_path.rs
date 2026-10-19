// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The skipped path, tried after a rehearsal's run (live gauntlet r02 B7, r05 B1, 2026-10-17:
//! « clean ✔ » and a rehearsal, then the first day with nothing new failed on « cannot use null
//! as … »). A run takes one path: the day a gated step is skipped, a filter after it may read its
//! null. Each `nika:jq` step the analyzer names for a skippable producer
//! ([`null_into_jq`]) is evaluated by the run's own isolated jq on that null, its other values as
//! this run gave them, then emptied (an empty list, text or object of the same kind, as on a day
//! with nothing new). It fails the rehearsal only when the null alone makes it fail: the same
//! filter passes on those values with a value in the null's place (the producer's own output
//! when this run gave one, else an empty text, list or object). A step whose input reads a value
//! this run did not give is not tried: nothing here guesses one. An evaluation runs only while
//! the run's jq deadline leaves time for it, and a bound it meets is no claim.

use std::time::{Duration, Instant};

use nika_check::analyzer::skipped_reads::{NullIntoJq, null_into_jq};
use nika_kernel::tool_executor::ToolCall;
use nika_runtime::{RunOutcome, TaskErrorRecord, TaskStatus, TerminalCause};
use nika_schema::VarDecl;
use nika_schema::expression::{Expr, scan_templates};
use nika_schema::raw::{RawAction, RawInvokeTarget, RawTask, RawWorkflow};
use serde_json::{Value, json};

use super::isolated_jq::IsolatedJq;

#[cfg(test)]
mod tests;

/// The code a filter that fails on a skipped producer's null fails its task with.
pub(super) const SKIPPED_NULL: &str = "NIKA-REHEARSAL-SKIPPED";

/// The most readers one run tries.
const TRIED: usize = 8;

/// The largest workflow whose skipped worlds are folded here: the check's own analysis cap.
const ANALYZED: usize = 2_000;

/// The time an evaluation needs left before the run's jq deadline to start.
const MARGIN: Duration = Duration::from_secs(2);

/// The characters of a filter's error a failure keeps.
const SHOWN: usize = 300;

/// How a filter's other values are given: as the run gave them, or emptied.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Others {
    Given,
    Emptied,
}

/// Try every reader the analyzer names on its producer's null, after a run that settled `ok`:
/// the first that fails on the null alone fails its task, with [`SKIPPED_NULL`], and the run.
pub(super) async fn try_skipped(workflow: &RawWorkflow, outcome: &mut RunOutcome, jq: &IsolatedJq) {
    if !outcome.ok || workflow.tasks.len() > ANALYZED {
        return;
    }
    let Ok(analyzed) = nika_check::analyze(workflow) else {
        return;
    };
    let named = null_into_jq(workflow, &analyzed.edges, &analyzed.topo_waves);
    // The run's helper and deadline, with a bound record of its own: a bound met here never
    // marks the run's.
    let jq = jq.fresh();
    for row in named.iter().take(TRIED) {
        let Some(task) = (workflow.tasks.iter()).find(|task| task.value.id.value == row.reader)
        else {
            continue;
        };
        let Some(error) = fails_on_null(workflow, &task.value, row, outcome, &jq).await else {
            continue;
        };
        if let Some(record) = outcome.records.get_mut(&row.reader) {
            record.status = TaskStatus::Failure;
            record.cause = TerminalCause::VerbError;
            record.error = Some(TaskErrorRecord::new(SKIPPED_NULL, said(row, &error), false));
            outcome.ok = false;
            return;
        }
    }
}

/// The filter's error when the producer's null alone makes it fail, as this module's header
/// says; `None` when it passes, a value is unknown, or time ran short.
async fn fails_on_null(
    workflow: &RawWorkflow,
    task: &RawTask,
    row: &NullIntoJq,
    outcome: &RunOutcome,
    jq: &IsolatedJq,
) -> Option<String> {
    let expression = expression_of(task)?;
    let input = |slot: &Value, others| input_of(workflow, task, row, outcome, slot, others);
    let Err(error) = evaluate(jq, expression, input(&Value::Null, Others::Given)?).await? else {
        return None;
    };
    // A day with nothing new: the null must fail there as well.
    if evaluate(jq, expression, input(&Value::Null, Others::Emptied)?)
        .await?
        .is_ok()
    {
        return None;
    }
    let ran = (outcome.records.get(&row.producer))
        .filter(|record| record.status == TaskStatus::Success && !record.output.is_null())
        .map(|record| record.output.clone());
    let stand_ins = ran.into_iter().chain([json!(""), json!([]), json!({})]);
    for stand_in in stand_ins {
        if evaluate(jq, expression, input(&stand_in, Others::Given)?)
            .await?
            .is_ok()
        {
            return Some(error);
        }
    }
    None
}

/// One evaluation: `Ok` when the filter gives a value, its error when it fails; `None` when
/// too little time is left or a bound ended it.
async fn evaluate(jq: &IsolatedJq, expression: &str, input: Value) -> Option<Result<(), String>> {
    // The helper's deadline is the monotonic instant its run was given: the real time left
    // bounds a child process, which an injected clock must never stretch.
    let now = Instant::now(); // seam-bypass-ok: a child process's real deadline
    if now + MARGIN > jq.deadline() {
        return None;
    }
    let call = ToolCall::new(
        "skipped-path",
        "nika:jq",
        json!({"input": input, "expression": expression}),
    );
    match jq.evaluate(&call).await {
        Ok(result) if result.is_error => Some(Err(result.content)),
        Ok(_) => Some(Ok(())),
        Err(_) => None,
    }
}

/// The literal filter of a `nika:jq` task.
fn expression_of(task: &RawTask) -> Option<&str> {
    let RawAction::Invoke(invoke) = &task.action else {
        return None;
    };
    let RawInvokeTarget::Tool(tool) = &invoke.target else {
        return None;
    };
    let expression = invoke.args.as_ref()?.value.get("expression")?.as_str()?;
    (tool.value == "nika:jq" && !expression.contains("${{")).then_some(expression)
}

/// The task's `input:` with the producer's value as `slot` and every other value read from the
/// run's records as `others` gives them; `None` when one is not known.
fn input_of(
    workflow: &RawWorkflow,
    task: &RawTask,
    row: &NullIntoJq,
    outcome: &RunOutcome,
    slot: &Value,
    others: Others,
) -> Option<Value> {
    let RawAction::Invoke(invoke) = &task.action else {
        return None;
    };
    let input = invoke.args.as_ref()?.value.get("input")?;
    let scope = Scope {
        workflow,
        task,
        row,
        outcome,
        slot,
        others,
    };
    scope.value(input, true)
}

/// What the reader's values resolve against.
struct Scope<'a> {
    workflow: &'a RawWorkflow,
    task: &'a RawTask,
    row: &'a NullIntoJq,
    outcome: &'a RunOutcome,
    slot: &'a Value,
    others: Others,
}

impl Scope<'_> {
    /// `value` resolved: a whole island by its expression, a text with none as itself, nested
    /// values each; `with` says whether `with.<key>` may be read (the verb's fields), or not
    /// (a binding's own value).
    fn value(&self, value: &Value, with: bool) -> Option<Value> {
        match value {
            Value::String(text) if !text.contains("${{") => Some(value.clone()),
            Value::String(text) => self.expr(&sole_island(text)?, with),
            Value::Array(items) => (items.iter())
                .map(|item| self.value(item, with))
                .collect::<Option<Vec<_>>>()
                .map(Value::Array),
            Value::Object(fields) => (fields.iter())
                .map(|(key, item)| Some((key.clone(), self.value(item, with)?)))
                .collect::<Option<serde_json::Map<_, _>>>()
                .map(Value::Object),
            other => Some(other.clone()),
        }
    }

    /// `with.<key>` (the null's carriers hold the slot), `tasks.<id>.output` and the steps
    /// beneath it, `const.<name>`; any other expression is unknown here.
    fn expr(&self, expr: &Expr, with: bool) -> Option<Value> {
        let mut steps = Vec::new();
        let mut at = expr;
        while let Expr::Member { base, field } = at {
            steps.push(field.as_str());
            at = base;
        }
        let Expr::Ident(root) = at else {
            return None;
        };
        steps.reverse();
        match (root.as_str(), steps.as_slice()) {
            ("with", [key])
                if with && self.row.carriers.iter().any(|held| held.as_str() == *key) =>
            {
                Some(self.slot.clone())
            }
            ("with", [key]) if with => {
                let (_, bound) = (self.task.with.iter()).find(|(name, _)| name.value == *key)?;
                self.value(&bound.value, false)
            }
            ("tasks", [id, "output", beneath @ ..]) if *id == self.row.producer => {
                beneath.is_empty().then(|| self.slot.clone())
            }
            ("tasks", [id, "output", beneath @ ..]) => {
                let mut value = self.given(id)?;
                for step in beneath {
                    value = value.get(*step)?.clone();
                }
                Some(value)
            }
            ("const", [name]) => {
                let (_, declared) =
                    (self.workflow.consts.iter()).find(|(constant, _)| constant.value == *name)?;
                match declared {
                    VarDecl::Untyped(value) => Some(value.clone()),
                    VarDecl::Typed { default, .. } => default.clone(),
                }
            }
            _ => None,
        }
    }

    /// The output this run gave `id`, as `others` says: a skipped task's is its null; a task
    /// that did not settle a success or a skip gave none.
    fn given(&self, id: &str) -> Option<Value> {
        let record = self.outcome.records.get(id)?;
        let output = match record.status {
            TaskStatus::Success => record.output.clone(),
            TaskStatus::Skipped => Value::Null,
            _ => return None,
        };
        Some(match self.others {
            Others::Given => output,
            Others::Emptied => emptied(&output),
        })
    }
}

/// The empty value of the same kind: a day with nothing new.
fn emptied(value: &Value) -> Value {
    match value {
        Value::Array(_) => json!([]),
        Value::String(_) => json!(""),
        Value::Object(_) => json!({}),
        Value::Number(_) => json!(0),
        Value::Bool(_) => json!(false),
        Value::Null => Value::Null,
    }
}

/// The expression of a text that is exactly one `${{ … }}` island.
fn sole_island(text: &str) -> Option<Expr> {
    let trimmed = text.trim();
    if !(trimmed.starts_with("${{") && trimmed.ends_with("}}")) {
        return None;
    }
    let mut islands = scan_templates(trimmed).ok()?;
    if islands.len() != 1 {
        return None;
    }
    islands.pop().map(|island| island.expr)
}

/// What the author reads of a filter that fails on its producer's null.
fn said(row: &NullIntoJq, error: &str) -> String {
    let error: String = error.chars().take(SHOWN).collect();
    let (reader, producer) = (&row.reader, &row.producer);
    format!(
        "{reader} fails the day {producer} is skipped ({}): its filter receives that null and \
         stops with « {error} »; with a value in its place, the same filter passes. Guard \
         {reader} with a `when:` that reads {producer}'s value, or give the null a default in the \
         filter (`// []`, `// \"\"`)",
        row.reasons.join(" · ")
    )
}
