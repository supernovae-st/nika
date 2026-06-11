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
    // ── additive cohort 2026-06-11 · closes the vocabulary over the
    //    nika-cli display contract (§3.1 state machine + §3.3 run card
    //    refold drivers) — every state the UI can show is expressible
    //    by an engine event. MINOR-bump additive per the header law. ──
    /// A task attempt failed and a retry is scheduled (`attempt` /
    /// `max_attempts` fields carry the counter — contract §3.1 `↻`).
    TaskRetrying,
    /// A task was cancelled (operator stop · timeout · budget kill —
    /// contract §3.1 `◼`; distinct from [`EventKind::TaskFailed`]:
    /// cancellation is a decision, not a defect).
    TaskCancelled,
    /// The whole run was cancelled (terminal, but NOT a failure).
    WorkflowCancelled,
    /// An incremental spend delta (`tokens` / `usd` fields) — the live
    /// `~$` meter refolds on every one (contract §3.3 names it).
    CostIncurred,
    /// A streaming output delta arrived from an `infer`/`agent` turn
    /// (`delta` field carries the text chunk — contract §3.3 names it).
    InferChunk,
    /// A `permits:` boundary check was evaluated (`gate` + `decision`
    /// fields · `allow`/`deny`) — the declared security boundary made
    /// observable at runtime (ADR-092 · the auditable moat).
    PermitChecked,
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
            Self::TaskRetrying => "task_retrying",
            Self::TaskCancelled => "task_cancelled",
            Self::WorkflowCancelled => "workflow_cancelled",
            Self::CostIncurred => "cost_incurred",
            Self::InferChunk => "infer_chunk",
            Self::PermitChecked => "permit_checked",
        }
    }

    /// Whether this kind marks a terminal workflow state (completed,
    /// failed, or cancelled — after it, no further events for the run).
    ///
    /// ```
    /// use nika_event::EventKind;
    /// assert!(EventKind::WorkflowCompleted.is_terminal());
    /// assert!(EventKind::WorkflowCancelled.is_terminal());
    /// assert!(!EventKind::TaskStarted.is_terminal());
    /// ```
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::WorkflowCompleted | Self::WorkflowFailed | Self::WorkflowCancelled
        )
    }

    /// Whether this kind represents a failure (workflow or task).
    ///
    /// Cancellation is NOT a failure — it is a decision (operator stop ·
    /// budget kill), and renderers draw it dim, not red (contract §3.1).
    ///
    /// ```
    /// use nika_event::EventKind;
    /// assert!(EventKind::TaskFailed.is_failure());
    /// assert!(!EventKind::TaskCompleted.is_failure());
    /// assert!(!EventKind::TaskCancelled.is_failure());
    /// ```
    #[must_use]
    pub const fn is_failure(&self) -> bool {
        matches!(self, Self::WorkflowFailed | Self::TaskFailed)
    }

    /// The coarse classification — renderers and routers branch on the
    /// CLASS (7 stable groups) instead of matching every variant, so a
    /// new kind in an existing class flows through them untouched.
    ///
    /// ```
    /// use nika_event::{EventClass, EventKind};
    /// assert_eq!(EventKind::TaskRetrying.class(), EventClass::Task);
    /// assert_eq!(EventKind::CostIncurred.class(), EventClass::Cost);
    /// assert_eq!(EventKind::PermitChecked.class(), EventClass::Security);
    /// ```
    #[must_use]
    pub const fn class(&self) -> EventClass {
        match self {
            Self::WorkflowStarted
            | Self::WorkflowCompleted
            | Self::WorkflowFailed
            | Self::WorkflowCancelled => EventClass::Workflow,
            Self::TaskScheduled
            | Self::TaskStarted
            | Self::TaskCompleted
            | Self::TaskFailed
            | Self::TaskSkipped
            | Self::TaskRetrying
            | Self::TaskCancelled => EventClass::Task,
            Self::VerbInvoked | Self::ToolInvoked => EventClass::Dispatch,
            Self::CheckpointWritten => EventClass::Durability,
            Self::CostIncurred => EventClass::Cost,
            Self::InferChunk => EventClass::Stream,
            Self::PermitChecked => EventClass::Security,
        }
    }
}

/// The coarse event classification (see [`EventKind::class`]).
///
/// Closed-but-extensible like the kind enum itself: a renderer matching
/// classes carries a `_` arm and inherits future classes gracefully.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub enum EventClass {
    /// Run lifecycle (started · completed · failed · cancelled).
    Workflow,
    /// Per-task lifecycle (scheduled … cancelled).
    Task,
    /// Verb/tool dispatch surface.
    Dispatch,
    /// Durable-state writes (checkpoints).
    Durability,
    /// Incremental spend deltas.
    Cost,
    /// Streaming output deltas.
    Stream,
    /// Permits-boundary evaluations.
    Security,
}

impl fmt::Display for EventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every variant. The exhaustive match in [`all_list_stays_complete`]
    /// makes adding a variant WITHOUT extending this slice a compile error
    /// (no `_` arm — legal in the defining crate even for a `#[non_exhaustive]`
    /// enum), so the wire-format consistency check below can never silently
    /// skip a newly-added kind.
    const ALL: &[EventKind] = &[
        EventKind::WorkflowStarted,
        EventKind::WorkflowCompleted,
        EventKind::WorkflowFailed,
        EventKind::TaskScheduled,
        EventKind::TaskStarted,
        EventKind::TaskCompleted,
        EventKind::TaskFailed,
        EventKind::TaskSkipped,
        EventKind::VerbInvoked,
        EventKind::ToolInvoked,
        EventKind::CheckpointWritten,
        EventKind::TaskRetrying,
        EventKind::TaskCancelled,
        EventKind::WorkflowCancelled,
        EventKind::CostIncurred,
        EventKind::InferChunk,
        EventKind::PermitChecked,
    ];

    #[test]
    fn all_list_stays_complete() {
        // Compile-time forward-compat guard: a new variant breaks this match
        // until it is added (and then the author must extend ALL to satisfy the
        // length check, which re-arms the wire-slug test for the new variant).
        fn _exhaustive(k: EventKind) {
            match k {
                EventKind::WorkflowStarted
                | EventKind::WorkflowCompleted
                | EventKind::WorkflowFailed
                | EventKind::TaskScheduled
                | EventKind::TaskStarted
                | EventKind::TaskCompleted
                | EventKind::TaskFailed
                | EventKind::TaskSkipped
                | EventKind::VerbInvoked
                | EventKind::ToolInvoked
                | EventKind::CheckpointWritten
                | EventKind::TaskRetrying
                | EventKind::TaskCancelled
                | EventKind::WorkflowCancelled
                | EventKind::CostIncurred
                | EventKind::InferChunk
                | EventKind::PermitChecked => {}
            }
        }
        assert_eq!(ALL.len(), 17, "extend ALL when a variant is added");
    }

    /// FCI-003: the canonical wire slug has TWO independent encoders — the
    /// serde `rename_all = "snake_case"` derive (used when an `Event` is
    /// serialized) and the hand-written [`EventKind::as_str`] (used by
    /// `Display` + direct consumers). They MUST agree forever; this pins them.
    #[cfg(feature = "serde")]
    #[test]
    fn serde_wire_slug_matches_as_str_for_every_variant() {
        for k in ALL {
            let json = serde_json::to_value(k).expect("EventKind serializes");
            let serde_slug = json
                .as_str()
                .expect("EventKind must serialize as a JSON string");
            assert_eq!(
                serde_slug,
                k.as_str(),
                "wire-slug divergence for {k:?}: serde={serde_slug:?} vs as_str()={:?} \
                 — the EventKind wire format must have ONE canonical form (FCI-003)",
                k.as_str()
            );
        }
    }

    #[test]
    fn display_matches_as_str_for_every_variant() {
        for k in ALL {
            assert_eq!(k.to_string(), k.as_str());
        }
    }

    #[test]
    fn terminal_and_failure_classification() {
        // Pin the two classifiers against the full variant set so a new
        // lifecycle variant can't silently mis-classify.
        assert!(EventKind::WorkflowCompleted.is_terminal());
        assert!(EventKind::WorkflowFailed.is_terminal());
        assert!(EventKind::WorkflowCancelled.is_terminal());
        assert!(!EventKind::TaskCompleted.is_terminal());
        assert!(!EventKind::TaskCancelled.is_terminal());
        assert!(EventKind::WorkflowFailed.is_failure());
        assert!(EventKind::TaskFailed.is_failure());
        assert!(!EventKind::WorkflowCompleted.is_failure());
        // A terminal-failure is both; a task-failure is a failure but not terminal.
        assert!(EventKind::TaskFailed.is_failure() && !EventKind::TaskFailed.is_terminal());
        // Cancellation is terminal (workflow) but NEVER a failure — a
        // decision, not a defect (contract §3.1 draws it dim, not red).
        assert!(!EventKind::WorkflowCancelled.is_failure());
        assert!(!EventKind::TaskCancelled.is_failure());
        // Retrying is neither terminal nor a failure (the attempt failed;
        // the TASK has not).
        assert!(!EventKind::TaskRetrying.is_failure());
        assert!(!EventKind::TaskRetrying.is_terminal());
    }

    #[test]
    fn every_kind_has_a_class_and_classes_partition() {
        use crate::kind::EventClass;
        // Total: every kind classifies (the const fn cannot panic, but
        // this pins the MAPPING against accidental re-grouping).
        for k in ALL {
            let expected = match k.as_str() {
                s if s.starts_with("workflow_") => Some(EventClass::Workflow),
                s if s.starts_with("task_") => Some(EventClass::Task),
                "verb_invoked" | "tool_invoked" => Some(EventClass::Dispatch),
                "checkpoint_written" => Some(EventClass::Durability),
                "cost_incurred" => Some(EventClass::Cost),
                "infer_chunk" => Some(EventClass::Stream),
                "permit_checked" => Some(EventClass::Security),
                _ => None,
            };
            assert_eq!(Some(k.class()), expected, "class drift for {k:?}");
        }
    }

    #[test]
    fn contract_named_slugs_are_canonical() {
        // The nika-cli display contract §3.3 names these two event slugs
        // VERBATIM as the live-meter refold drivers; §3.1 needs the two
        // states. Pin the wire form the contract relies on.
        assert_eq!(EventKind::CostIncurred.as_str(), "cost_incurred");
        assert_eq!(EventKind::InferChunk.as_str(), "infer_chunk");
        assert_eq!(EventKind::TaskRetrying.as_str(), "task_retrying");
        assert_eq!(EventKind::TaskCancelled.as_str(), "task_cancelled");
    }
}
