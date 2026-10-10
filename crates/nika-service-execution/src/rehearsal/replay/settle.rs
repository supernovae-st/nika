// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! How a replay trial ended, read from the runtime's own records. A task the trial does not
//! exercise, or one whose error speaks the room's refusal, never counts against the candidate,
//! and neither does a task cancelled or skipped behind it; the first other failing task, in the
//! candidate's order, does. A run with no such failure is then read for a filter that kept
//! nothing: a `nika:jq` task that read only exercised tasks, at least one of which gave items (a
//! non-empty array, an object holding one, or text), and itself gave an empty array, an empty
//! string or null. Its verdict names what came in, so the filter is repaired from facts alone.
//! Pure: the records are read ([`settle`](fn@settle)) or rewritten as the trial's view
//! ([`settle_into`]), nothing is run.

use nika_runtime::{RunOutcome, TaskErrorRecord, TaskRecord, TaskStatus, TerminalCause};
use serde_json::Value;

use super::ReplayScreen;
use crate::rehearsal::room::OUTSIDE;

#[cfg(test)]
mod tests;

/// The code of a filter that kept nothing of the items it read.
pub const KEPT_NOTHING: &str = "NIKA-REHEARSAL-EMPTY";

/// The keys a described object shows.
const SHOWN_KEYS: usize = 12;

/// The characters of a string a description shows whole.
const SHOWN_CHARS: usize = 40;

/// How a replay trial ended, for the candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Settled {
    /// No exercised task failed, and no filter kept nothing of the items it read.
    Passed,
    /// An exercised task failed, or a filter kept nothing of the items it read.
    Failed {
        /// The task; empty when the run failed and no task recorded a failure.
        task: String,
        /// The task's error code, or [`KEPT_NOTHING`].
        code: String,
        /// The task's error message, or what came in and what went out.
        message: String,
    },
}

/// How a replay trial screened as `screen` ended in `outcome`, its tasks read in `order`.
#[must_use]
pub fn settle(outcome: &RunOutcome, order: &[String], screen: &ReplayScreen) -> Settled {
    if let Some(failed) = failure(outcome, order, screen) {
        return failed;
    }
    (order.iter())
        .find_map(|task| emptied(outcome, task, screen))
        .unwrap_or(Settled::Passed)
}

/// `outcome` rewritten as the trial's view of the run it records, its tasks read in `order`: a
/// failed task the trial does not exercise, or whose error is its room's refusal, is recorded
/// skipped by its error (the error kept), the first filter that kept nothing of the items it read
/// is recorded failed with [`KEPT_NOTHING`], and the run is `ok` when no task it exercised failed.
/// A run that failed with no task record stays failed.
pub fn settle_into(outcome: &mut RunOutcome, order: &[String], screen: &ReplayScreen) {
    let recorded = (outcome.records.values()).any(|record| record.status == TaskStatus::Failure);
    for (task, record) in &mut outcome.records {
        if record.status == TaskStatus::Failure && excused(task, record, screen) {
            record.status = TaskStatus::Skipped;
            record.cause = TerminalCause::ErrorSkip;
        }
    }
    let empty = (order.iter()).find_map(|task| emptied(outcome, task, screen));
    if let Some(Settled::Failed {
        task,
        code,
        message,
    }) = empty
        && let Some(record) = outcome.records.get_mut(&task)
    {
        record.status = TaskStatus::Failure;
        record.cause = TerminalCause::VerbError;
        record.error = Some(TaskErrorRecord::new(code, message, false));
    }
    let failing = (outcome.records.values()).any(|record| record.status == TaskStatus::Failure);
    outcome.ok = !failing && (outcome.ok || recorded);
}

/// The first task in `order` that failed and counts against the candidate; when the run failed
/// and no task recorded a failure, that failure of the run.
fn failure(outcome: &RunOutcome, order: &[String], screen: &ReplayScreen) -> Option<Settled> {
    let first = (order.iter())
        .filter_map(|task| {
            let record = outcome.records.get(task)?;
            (record.status == TaskStatus::Failure).then_some((task, record))
        })
        .find(|(task, record)| !excused(task, record, screen));
    if let Some((task, record)) = first {
        let (code, message) = (record.error.as_ref()).map_or_else(Default::default, |error| {
            (error.code.clone(), error.message.clone())
        });
        return Some(Settled::Failed {
            task: task.clone(),
            code,
            message,
        });
    }
    let recorded = (outcome.records.values()).any(|record| record.status == TaskStatus::Failure);
    (!outcome.ok && !recorded).then(|| Settled::Failed {
        task: String::new(),
        code: String::new(),
        message: "the run failed, and no task recorded a failure".to_owned(),
    })
}

/// Whether a failed task does not count against the candidate: the trial does not exercise it,
/// or the room refused what it reached for.
fn excused(task: &str, record: &TaskRecord, screen: &ReplayScreen) -> bool {
    screen.is_unexercised(task)
        || (record.error.as_ref()).is_some_and(|error| error.message.contains(OUTSIDE))
}

/// `task`, when it is a filter that ran, read only exercised tasks and kept nothing of the items
/// one of them gave.
fn emptied(outcome: &RunOutcome, task: &str, screen: &ReplayScreen) -> Option<Settled> {
    if !screen.filters.iter().any(|filter| filter == task) {
        return None;
    }
    let kept = (outcome.records.get(task)).filter(|record| record.status == TaskStatus::Success)?;
    let reads = screen.upstream.get(task)?;
    if !nothing(&kept.output) || reads.iter().any(|read| screen.is_unexercised(read)) {
        return None;
    }
    let (from, came) = reads.iter().find_map(|read| {
        let output = &outcome.records.get(read)?.output;
        came_in(output).map(|came| (read, came))
    })?;
    let plural = if came.count == 1 { "" } else { "s" };
    let message = format!(
        "{task} kept nothing: {} {}{plural} in from {from}{}, 0 out; first {} in: {}",
        came.count, came.unit, came.field, came.unit, came.first
    );
    Some(Settled::Failed {
        task: task.to_owned(),
        code: KEPT_NOTHING.to_owned(),
        message,
    })
}

/// Whether a filter's output kept nothing: an empty array, an empty string or null.
fn nothing(output: &Value) -> bool {
    output.is_null()
        || output.as_array().is_some_and(Vec::is_empty)
        || output.as_str().is_some_and(str::is_empty)
}

/// What came in of one output: its items, those of the first non-empty array an object holds,
/// or its lines of text.
struct CameIn {
    /// The object's key that holds the items, as `.key`; empty for the output itself.
    field: String,
    /// How many items or lines came in.
    count: usize,
    /// `item` or `line`.
    unit: &'static str,
    /// The first item or line, described.
    first: String,
}

/// What came in of `output`, when anything did.
fn came_in(output: &Value) -> Option<CameIn> {
    match output {
        Value::Array(items) => listed(String::new(), items),
        Value::Object(fields) => {
            (fields.iter()).find_map(|(key, value)| listed(format!(".{key}"), value.as_array()?))
        }
        Value::String(text) if !text.trim().is_empty() => Some(CameIn {
            field: String::new(),
            count: text.lines().count(),
            unit: "line",
            first: shown(text.lines().next().unwrap_or_default()),
        }),
        _ => None,
    }
}

/// The items of a non-empty array, held at `field`.
fn listed(field: String, items: &[Value]) -> Option<CameIn> {
    let first = described(items.first()?);
    Some(CameIn {
        field,
        count: items.len(),
        unit: "item",
        first,
    })
}

/// A value as an author reads it: an object's keys, each with its value's type, and a short
/// scalar's value beside its type.
fn described(value: &Value) -> String {
    let Value::Object(fields) = value else {
        return typed(value);
    };
    let mut keys: Vec<String> = (fields.iter().take(SHOWN_KEYS))
        .map(|(key, field)| format!("{key}: {}", typed(field)))
        .collect();
    if fields.len() > SHOWN_KEYS {
        keys.push("…".to_owned());
    }
    format!("{{{}}}", keys.join(", "))
}

/// A value's type, and its value when it is a short scalar.
fn typed(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(flag) => format!("boolean ({flag})"),
        Value::Number(number) => format!("number ({number})"),
        Value::String(text) => shown(text),
        Value::Array(items) => format!("array of {}", items.len()),
        Value::Object(fields) => format!("object of {} keys", fields.len()),
    }
}

/// A string's type, and its value when it is short and on one line.
fn shown(text: &str) -> String {
    if text.chars().count() <= SHOWN_CHARS && !text.chars().any(char::is_control) {
        format!("string ({text})")
    } else {
        "string".to_owned()
    }
}
