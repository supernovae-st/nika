// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Recorded terminal facts for one task occurrence, in journal order.

use nika_event::{Event, EventKind};
use nika_runtime::{TaskStatus, TerminalCause, legal};
use serde_json::Value;

#[derive(Default)]
pub(super) struct TaskOutcome {
    pub(super) cause: Option<String>,
    pub(super) error_code: Option<String>,
    pub(super) error_message: Option<String>,
    pub(super) recovered_from: Option<String>,
    pub(super) recovered: bool,
}

pub(super) fn current(events: &[Event], task: &str) -> TaskOutcome {
    for (index, event) in events.iter().enumerate().rev() {
        if event.kind == EventKind::WorkflowStarted {
            break;
        }
        if event.str_field("task") != Some(task) {
            continue;
        }
        if terminal_class(event.kind).is_some() {
            // Select before parsing: bad or absent current evidence never
            // grants permission to reuse a preceding terminal observation.
            return terminal(event, &events[..index], task);
        }
        if occurrence_boundary(event.kind) {
            break;
        }
    }
    TaskOutcome::default()
}

fn terminal_class(kind: EventKind) -> Option<TaskStatus> {
    match kind {
        EventKind::TaskCompleted | EventKind::TaskCacheHit => Some(TaskStatus::Success),
        EventKind::TaskFailed => Some(TaskStatus::Failure),
        EventKind::TaskSkipped => Some(TaskStatus::Skipped),
        EventKind::TaskCancelled => Some(TaskStatus::Cancelled),
        _ => None,
    }
}

fn occurrence_boundary(kind: EventKind) -> bool {
    matches!(
        kind,
        EventKind::TaskScheduled
            | EventKind::TaskStarted
            | EventKind::TaskRetrying
            | EventKind::TaskRecovered
            | EventKind::WorkflowPaused
    )
}

fn terminal(event: &Event, preceding: &[Event], task: &str) -> TaskOutcome {
    if event.field("outcome").is_none() && event.kind == EventKind::TaskCompleted {
        return legacy_recovery(preceding, task);
    }
    recorded(event).unwrap_or_default()
}

fn recorded(event: &Event) -> Option<TaskOutcome> {
    let value: Value = serde_json::from_str(event.str_field("outcome")?).ok()?;
    let outcome = value.as_object()?;
    let payload = outcome.get("payload")?.as_object()?;
    let class = terminal_class(event.kind)?;
    if outcome.get("class")?.as_str()? != class.as_str() {
        return None;
    }
    let cause: TerminalCause = serde_json::from_value(outcome.get("cause")?.clone()).ok()?;
    if !legal(class, cause)
        || (event.kind == EventKind::TaskCacheHit && cause != TerminalCause::Normal)
    {
        return None;
    }
    let error = if class == TaskStatus::Failure || cause == TerminalCause::ErrorSkip {
        payload.get("error").and_then(Value::as_object)
    } else {
        None
    };
    let recovered = cause == TerminalCause::Recovered;
    let original = if recovered {
        payload.get("recovered_from").and_then(Value::as_object)
    } else {
        None
    };
    Some(TaskOutcome {
        cause: Some(cause.as_str().to_owned()),
        error_code: error.and_then(|e| e.get("code")).and_then(text),
        error_message: error.and_then(|e| e.get("message")).and_then(text),
        recovered_from: original.and_then(|e| e.get("code")).and_then(text),
        recovered,
    })
}

fn text(value: &Value) -> Option<String> {
    value.as_str().map(str::to_owned)
}

fn legacy_recovery(events: &[Event], task: &str) -> TaskOutcome {
    for event in events.iter().rev() {
        if event.kind == EventKind::WorkflowStarted {
            break;
        }
        if event.str_field("task") != Some(task) {
            continue;
        }
        if event.kind == EventKind::TaskRecovered {
            return TaskOutcome {
                recovered_from: event.str_field("code").map(str::to_owned),
                recovered: true,
                ..TaskOutcome::default()
            };
        }
        if terminal_class(event.kind).is_some() || occurrence_boundary(event.kind) {
            break;
        }
    }
    TaskOutcome::default()
}
