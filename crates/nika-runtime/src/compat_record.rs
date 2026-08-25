// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Source-compatible public task records at the historical runtime path.
//!
//! The new `nika-dataflow` canonical types are evolvable under FCI-002. The
//! runtime types predate that contract: downstream exhaustive enum matches and
//! `TaskErrorRecord` struct literals are source API. These wrappers preserve
//! that surface while conversion at `RunOutcome` keeps one canonical engine
//! implementation.

use std::collections::BTreeMap;

use nika_types::timestamp::Timestamp;
use serde_json::Value;

/// A task's historical four-state runtime status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    /// The task completed successfully.
    Success,
    /// The task failed.
    Failure,
    /// The task was skipped.
    Skipped,
    /// The task was cancelled.
    Cancelled,
}

impl TaskStatus {
    /// The spec wire word.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Skipped => "skipped",
            Self::Cancelled => "cancelled",
        }
    }
}

impl From<nika_dataflow::TaskStatus> for TaskStatus {
    fn from(value: nika_dataflow::TaskStatus) -> Self {
        match value {
            nika_dataflow::TaskStatus::Success => Self::Success,
            nika_dataflow::TaskStatus::Failure => Self::Failure,
            nika_dataflow::TaskStatus::Skipped => Self::Skipped,
            // The wildcard includes today's Cancelled state and intentionally
            // freezes the historical four-state API; a future canonical state
            // degrades conservatively until the runtime facade gets a new major.
            _ => Self::Cancelled,
        }
    }
}

/// The historical exhaustive outcome-cause vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalCause {
    /// Successful verb completion.
    Normal,
    /// Successful recovery.
    Recovered,
    /// Unrecovered verb error.
    VerbError,
    /// Task timeout.
    Timeout,
    /// Exhausted retry policy.
    RetryExhausted,
    /// Gate decision skip.
    Gate,
    /// Error-preserving skip.
    ErrorSkip,
    /// Upstream cancellation.
    Upstream,
    /// Operator cancellation.
    Operator,
    /// Budget cancellation.
    Budget,
}

impl TerminalCause {
    /// The spec wire word.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Recovered => "recovered",
            Self::VerbError => "verb_error",
            Self::Timeout => "timeout",
            Self::RetryExhausted => "retry_exhausted",
            Self::Gate => "gate",
            Self::ErrorSkip => "error_skip",
            Self::Upstream => "upstream",
            Self::Operator => "operator",
            Self::Budget => "budget",
        }
    }
}

impl From<nika_dataflow::TerminalCause> for TerminalCause {
    fn from(value: nika_dataflow::TerminalCause) -> Self {
        match value {
            nika_dataflow::TerminalCause::Normal => Self::Normal,
            nika_dataflow::TerminalCause::Recovered => Self::Recovered,
            nika_dataflow::TerminalCause::VerbError => Self::VerbError,
            nika_dataflow::TerminalCause::Timeout => Self::Timeout,
            nika_dataflow::TerminalCause::RetryExhausted => Self::RetryExhausted,
            nika_dataflow::TerminalCause::Gate => Self::Gate,
            nika_dataflow::TerminalCause::ErrorSkip => Self::ErrorSkip,
            nika_dataflow::TerminalCause::Upstream => Self::Upstream,
            nika_dataflow::TerminalCause::Budget => Self::Budget,
            // The wildcard includes today's Operator cause. Under the same
            // compatibility policy as TaskStatus, an old reader sees a
            // conservative external-cancellation cause, never success.
            _ => Self::Operator,
        }
    }
}

/// The historical error payload; its public literal layout remains stable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskErrorRecord {
    /// The canonical wire code.
    pub code: String,
    /// Human-readable message.
    pub message: String,
    /// Retry eligibility class.
    pub transient: bool,
}

impl TaskErrorRecord {
    /// Construct the payload.
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>, transient: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            transient,
        }
    }

    /// The `${{ tasks.X.error }}` value.
    #[must_use]
    pub fn to_value(&self) -> Value {
        serde_json::json!({
            "code": self.code,
            "message": self.message,
            "transient": self.transient,
        })
    }
}

impl From<nika_dataflow::TaskErrorRecord> for TaskErrorRecord {
    fn from(value: nika_dataflow::TaskErrorRecord) -> Self {
        Self {
            code: value.code,
            message: value.message,
            transient: value.transient,
        }
    }
}

/// One task's historical result-record surface.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TaskRecord {
    /// Terminal status.
    pub status: TaskStatus,
    /// Why the task settled.
    pub cause: TerminalCause,
    /// The verb output.
    pub output: Value,
    /// Coarse runtime integrity label.
    pub integrity: nika_cap::Integrity,
    /// Error payload where the outcome law permits it.
    pub error: Option<TaskErrorRecord>,
    /// Attempts made on success/failure.
    pub attempts: Option<u32>,
    /// Original error repaired by recovery.
    pub recovered_from: Option<TaskErrorRecord>,
    /// Task-start stamp.
    pub started_at: Option<Timestamp>,
    /// Terminal-event stamp.
    pub ended_at: Option<Timestamp>,
    /// Clock-measured execution time.
    pub duration_ms: Option<u64>,
    /// Named `output:` bindings.
    pub named: BTreeMap<String, Value>,
}

impl TaskRecord {
    /// Construct a record for a task that never ran.
    #[must_use]
    pub fn unran(status: TaskStatus, cause: TerminalCause) -> Self {
        debug_assert!(legal(status, cause));
        Self {
            status,
            cause,
            output: Value::Null,
            integrity: nika_cap::Integrity::trusted(),
            error: None,
            attempts: None,
            recovered_from: None,
            started_at: None,
            ended_at: None,
            duration_ms: None,
            named: BTreeMap::new(),
        }
    }

    /// Resolve one reserved or named record field.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<Value> {
        Some(match name {
            "output" => self.output.clone(),
            "status" => Value::String(self.status.as_str().to_owned()),
            "cause" => Value::String(self.cause.as_str().to_owned()),
            "error" => self
                .error
                .as_ref()
                .map_or(Value::Null, TaskErrorRecord::to_value),
            "started_at" => stamp_value(self.started_at),
            "ended_at" => stamp_value(self.ended_at),
            "duration_ms" => self
                .duration_ms
                .map_or(Value::Null, |ms| Value::Number(ms.into())),
            other => return self.named.get(other).cloned(),
        })
    }
}

impl From<nika_dataflow::TaskRecord> for TaskRecord {
    fn from(value: nika_dataflow::TaskRecord) -> Self {
        Self {
            status: value.status.into(),
            cause: value.cause.into(),
            output: value.output,
            integrity: value.integrity,
            error: value.error.map(Into::into),
            attempts: value.attempts,
            recovered_from: value.recovered_from.map(Into::into),
            started_at: value.started_at,
            ended_at: value.ended_at,
            duration_ms: value.duration_ms,
            named: value.named,
        }
    }
}

fn stamp_value(stamp: Option<Timestamp>) -> Value {
    stamp.map_or(Value::Null, |value| Value::String(value.to_string()))
}

/// The historical runtime transition law.
#[must_use]
pub const fn legal(class: TaskStatus, cause: TerminalCause) -> bool {
    matches!(
        (class, cause),
        (
            TaskStatus::Success,
            TerminalCause::Normal | TerminalCause::Recovered
        ) | (
            TaskStatus::Failure,
            TerminalCause::VerbError | TerminalCause::Timeout | TerminalCause::RetryExhausted
        ) | (
            TaskStatus::Skipped,
            TerminalCause::Gate | TerminalCause::ErrorSkip
        ) | (
            TaskStatus::Cancelled,
            TerminalCause::Upstream | TerminalCause::Operator | TerminalCause::Budget
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn historical_exhaustive_matches_and_error_literals_compile() {
        let status = match TaskStatus::Success {
            TaskStatus::Success => "success",
            TaskStatus::Failure => "failure",
            TaskStatus::Skipped => "skipped",
            TaskStatus::Cancelled => "cancelled",
        };
        assert_eq!(status, "success");
        let error = TaskErrorRecord {
            code: "NIKA-X".to_owned(),
            message: "x".to_owned(),
            transient: false,
        };
        assert_eq!(error.code, "NIKA-X");
    }
}
