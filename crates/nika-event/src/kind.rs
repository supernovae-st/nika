// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! [`EventKind`] — the canonical engine event taxonomy.
//!
//! A closed-but-extensible enum (`#[non_exhaustive]`) covering the
//! workflow lifecycle, the per-task lifecycle, and the 4-verb dispatch
//! surface (`infer · exec · invoke · agent`, per D-2026-05-22-N18).
//!
//! Forward-compat: `#[non_exhaustive]` permits adding variants on a MINOR
//! bump without breaking downstream `match` arms (they must carry a `_`
//! arm). Per `no-legacy-no-back-compat.md` Class 1 single-canonical-enum.

use core::fmt;

/// The kind of an emitted [`crate::Event`].
///
/// Mirrors the studio journal taxonomy in spirit but scoped to the
/// **engine runtime** (the studio chronicle lives in `dx/journal/` — a
/// disjoint domain, per `journal-storage-tiers.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub enum EventKind {
    /// A workflow run has begun.
    WorkflowStarted,
    /// A workflow run finished successfully (all tasks reached a terminal state).
    WorkflowCompleted,
    /// A workflow run aborted on an unrecoverable error.
    WorkflowFailed,
    /// A task was admitted to the ready set (its dependencies are satisfied).
    TaskScheduled,
    /// A task began executing.
    TaskStarted,
    /// A task finished successfully.
    TaskCompleted,
    /// A task aborted on an error.
    TaskFailed,
    /// A task was skipped (guard false, or an upstream dependency failed).
    TaskSkipped,
    /// A verb was dispatched (`infer · exec · invoke · agent`).
    VerbInvoked,
    /// A tool was invoked under the `invoke` verb (e.g. `nika:fetch`).
    ToolInvoked,
    /// A checkpoint was written (durable run state snapshot).
    CheckpointWritten,
}

impl EventKind {
    /// The stable wire slug for this kind (`snake_case`, stable across versions).
    ///
    /// ```
    /// use nika_event::EventKind;
    /// assert_eq!(EventKind::TaskCompleted.as_str(), "task_completed");
    /// ```
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::WorkflowStarted => "workflow_started",
            Self::WorkflowCompleted => "workflow_completed",
            Self::WorkflowFailed => "workflow_failed",
            Self::TaskScheduled => "task_scheduled",
            Self::TaskStarted => "task_started",
            Self::TaskCompleted => "task_completed",
            Self::TaskFailed => "task_failed",
            Self::TaskSkipped => "task_skipped",
            Self::VerbInvoked => "verb_invoked",
            Self::ToolInvoked => "tool_invoked",
            Self::CheckpointWritten => "checkpoint_written",
        }
    }

    /// Whether this kind marks a terminal workflow state (completed or failed).
    ///
    /// ```
    /// use nika_event::EventKind;
    /// assert!(EventKind::WorkflowCompleted.is_terminal());
    /// assert!(!EventKind::TaskStarted.is_terminal());
    /// ```
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::WorkflowCompleted | Self::WorkflowFailed)
    }

    /// Whether this kind represents a failure (workflow or task).
    ///
    /// ```
    /// use nika_event::EventKind;
    /// assert!(EventKind::TaskFailed.is_failure());
    /// assert!(!EventKind::TaskCompleted.is_failure());
    /// ```
    #[must_use]
    pub const fn is_failure(&self) -> bool {
        matches!(self, Self::WorkflowFailed | Self::TaskFailed)
    }
}

impl fmt::Display for EventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
