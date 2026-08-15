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
/// **engine runtime**. The studio keeps its own chronicle, in its own
/// private tree — a disjoint domain, never conflated with this one.
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
    /// A task was skipped (`when:` gate false · empty `for_each`
    /// collection · `on_error: skip` — never ran OR recovered-by-skip,
    /// by design; an upstream failure is [`EventKind::TaskCancelled`],
    /// spec 03 §task states).
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
    /// A task's `on_error.recover` repaired a failure into a success —
    /// emitted BETWEEN the (absorbed) failure and the terminal
    /// [`EventKind::TaskCompleted`], which stays the ONE success
    /// terminal (D-2026-07-08-N4: `… > task_recovered > task_completed`
    /// · additive · completed-only consumers unaffected). The `code`
    /// field carries what was recovered FROM; without this kind a
    /// repaired success is byte-identical to a clean one in the kind
    /// stream (empirically pinned by the corpus events-asserts,
    /// 2026-07-08 · engine#301).
    TaskRecovered,
    /// A task was cancelled (an upstream failure made the default gate
    /// unsatisfiable · operator stop · budget kill — contract §3.1 `◼`;
    /// distinct from [`EventKind::TaskFailed`]: cancellation is a
    /// decision, not a defect. A task-level `timeout:` is a FAILURE,
    /// not a cancellation — spec 03 §timeout · `NIKA-TIMEOUT-001`).
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
    // ── additive cohort 2026-06-12 · the agent-loop telemetry vocabulary
    //    (ADR-096) — every internal decision the `agent` verb takes is
    //    expressible as an engine event («eyes everywhere»: per AgentOps,
    //    Dong · Lu · Zhu 2024 · arxiv.org/abs/2411.05285, agent traces
    //    must expose DECISIONS, not just I/O). The L2 loop reports through
    //    its observer seam; the L3 runtime maps observer payloads onto
    //    these kinds 1:1. MINOR-bump additive per the header law. ──
    /// The agent's per-turn tool routing decided which definitions the
    /// model sees (`offered` / `universe` / per-source counts — the
    /// MCP-Zero-style active-discovery surface made observable).
    AgentToolsSelected,
    /// A corrective reflection message was injected into the transcript
    /// (`reason` field · `repeated_actions`/`error_streak` — the
    /// Reflexion-style verbal feedback, bounded by config).
    AgentNudge,
    /// The agent loop detected a no-progress action cycle past the stall
    /// threshold and stopped (`period` + `repeats` fields carry the
    /// evidence — the failure class TRAIL calls repetitive-action loops).
    AgentStalled,
    /// An `agent:compose` intrinsic draft was statically checked
    /// (`valid` + `violations` fields — generation is not permission:
    /// every self-composed workflow carries its check verdict).
    AgentComposeChecked,
    /// A per-turn budget snapshot (`turn` + `total_tokens` + optional
    /// `budget` fields — the spend curve is observable while the loop
    /// runs, not just at its end).
    AgentBudgetCheckpoint,
    // ── additive cohort 2026-07-05 · ADR-099 durable-lite resume —
    //    the skip is VISIBLE, never silent. MINOR-bump additive per
    //    the header law. ──
    /// A task was skipped under `--resume` because its identity matched
    /// a journaled success (`task` + `def_hash` + `input_hash` +
    /// `output` fields — the rehydration record · spec vocabulary
    /// `task.cache_hit` · ADR-099 §2). Downstream observes `status:
    /// success` exactly as if it ran live; the task-state enum stays
    /// CLOSED — the cache/live distinction rides the event stream only.
    TaskCacheHit,
    /// The run paused on a blocking `nika:prompt` with no usable
    /// `default:` under a non-interactive surface (`task` + the prompt
    /// payload fields · spec vocabulary `workflow.paused` · ADR-099
    /// rider). Terminal for THIS invocation (the process exits cleanly ·
    /// run state `paused`) — `--resume` re-arms the prompt; a paused
    /// trace can be resumed any number of times.
    WorkflowPaused,
    // ── additive cohort 2026-07-20 · the verifiable-run seal (S2 · the
    //    signed-journal wave). MINOR-bump additive per the header law. ──
    /// The journal's terminal integrity seal (`seal_format` + `covers`
    /// (head · events · semantic hash · engine · key identity) + `sig`
    /// fields) — one ed25519 signature binding the whole chain to the
    /// run-key that minted it, emitted as the LAST line of a signed run
    /// (S2 · `seal_format: 1` · the evidence pack's integrity root).
    RunSealed,
    // ── additive cohort 2026-07-23 · F-O1 PR-3 (NEP-0004 law 5) · the
    //    declassify door is receipt-recorded. MINOR-bump additive per
    //    the header law. ──
    /// A task-level `declassify:` entry lifted one binding from untrusted
    /// to trusted for THIS task (`task` + `from` + `because` +
    /// `value_digest` fields — the taint-lift evidence · NEP-0004 law 5 ·
    /// the only door through the permit re-gate; emitted once per entry
    /// between `task_started` and the terminal frame, so the receipt
    /// commits to WHAT was lifted and WHY).
    Declassify,
    // ── additive cohort 2026-07-29 · F-P4 (NEP-0013) · the human
    //    approval is a bounded capability, attested like every other
    //    boundary decision. MINOR-bump additive per the header law. ──
    /// A `nika:prompt` approval ticket was DECIDED (`step` + `mode` +
    /// `decision` (`allow`/`deny`/`dedup`) + `shown_hash` + `digest` +
    /// `run_nonce` + `ttl_seconds` fields — the WYSIWYS attestation: the
    /// journal swears WHICH content hash was answered, under WHICH ticket
    /// digest, with what TTL remaining · NEP-0013 law 4 · the same
    /// conformance-not-wire-bump posture as `permit_checked`, so
    /// `trace_format` stays 2). Emitted between `task_started` and the
    /// terminal frame; a blocking prompt that pauses the run carries its
    /// mint on the `workflow_paused` frame instead (no decision yet).
    ApprovalDecided,
    // ── additive cohort 2026-07-29 · F-P8 (SMSR · signed memory) · a
    //    rejected memory entry is NAMED, never silently filtered. MINOR-bump
    //    additive per the header law. ──
    /// A memory-store entry failed recall verification (`store` + `entry` +
    /// `reason` fields — `unsigned` · `malformed` · `key_mismatch` ·
    /// `store_mismatch` · `bad_signature` · the reason set grows
    /// additively with `nika_store::RejectReason`, e.g.
    /// `unsupported_version` · `name_mismatch`): the SMSR no-provenance-
    /// free-filter theorem made observable — an entry that cannot prove
    /// its provenance is REJECTED at recall, and the rejection is
    /// journaled (the seal's `covers["memory"]` pins the count beside the
    /// admitted set; the events land BEFORE the seal, so the chain covers
    /// the names). Diagnostic, like the agent-telemetry cohort: the recall
    /// verdict itself rides the recall API.
    MemoryEntryRejected,
    // ── additive cohort 2026-08-06 · ADR-111 outbound pause delivery —
    //    the question is heard, and the hearing is journaled. MINOR-bump
    //    additive per the header law. ──
    /// The pause event's outbound delivery landed — the engine sent the
    /// `workflow_paused` payload (one HTTP POST · a `CloudEvents`
    /// envelope · ADR-111) to the
    /// operator-configured `NIKA_NOTIFY_URL` and a 2xx came back
    /// (`target_host` + `duration_ms` fields). Observable history, never
    /// control flow: the run's `paused` outcome is identical with or
    /// without it, and the event lands BEFORE the seal so the chain
    /// covers the delivery claim.
    NotifyDelivered,
    /// The pause event's outbound delivery did not land — refused by the
    /// SSRF floor, timed out, or answered outside 2xx (`target_host` +
    /// `error` class fields · ADR-111). Journaled loudly, changes
    /// nothing: delivery failure is never the run's failure, and the run
    /// exits `paused` with the same code either way.
    NotifyFailed,
    /// The ONE summary of a rejection flood (`total` + `journaled` fields —
    /// additive cohort 2026-07-30 · H14 · NEP-0012 law 1): a store stuffed
    /// with rejections journals at most K individually-named
    /// `memory_entry_rejected` events, then this summary carrying the TRUE
    /// total once (the fold's `rejected: n` pins the same number in the
    /// seal's covers). Same diagnostic posture as its named sibling: the
    /// fold attests the count, the journal names the evidence, bounded.
    MemoryRejectionsSummary,
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
            Self::TaskRecovered => "task_recovered",
            Self::TaskCancelled => "task_cancelled",
            Self::WorkflowCancelled => "workflow_cancelled",
            Self::CostIncurred => "cost_incurred",
            Self::InferChunk => "infer_chunk",
            Self::PermitChecked => "permit_checked",
            Self::AgentToolsSelected => "agent_tools_selected",
            Self::AgentNudge => "agent_nudge",
            Self::AgentStalled => "agent_stalled",
            Self::AgentComposeChecked => "agent_compose_checked",
            Self::AgentBudgetCheckpoint => "agent_budget_checkpoint",
            Self::TaskCacheHit => "task_cache_hit",
            Self::WorkflowPaused => "workflow_paused",
            Self::RunSealed => "run_sealed",
            Self::Declassify => "declassify",
            Self::ApprovalDecided => "approval_decided",
            Self::MemoryEntryRejected => "memory_entry_rejected",
            Self::MemoryRejectionsSummary => "memory_rejections_summary",
            Self::NotifyDelivered => "notify_delivered",
            Self::NotifyFailed => "notify_failed",
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
            Self::WorkflowCompleted
                | Self::WorkflowFailed
                | Self::WorkflowCancelled
                | Self::WorkflowPaused
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
            | Self::WorkflowCancelled
            | Self::WorkflowPaused
            | Self::NotifyDelivered
            | Self::NotifyFailed => EventClass::Workflow,
            Self::TaskScheduled
            | Self::TaskStarted
            | Self::TaskCompleted
            | Self::TaskFailed
            | Self::TaskSkipped
            | Self::TaskRetrying
            | Self::TaskRecovered
            | Self::TaskCancelled
            | Self::TaskCacheHit => EventClass::Task,
            Self::VerbInvoked | Self::ToolInvoked => EventClass::Dispatch,
            Self::CheckpointWritten | Self::RunSealed => EventClass::Durability,
            Self::CostIncurred => EventClass::Cost,
            Self::InferChunk => EventClass::Stream,
            Self::PermitChecked
            | Self::Declassify
            | Self::ApprovalDecided
            | Self::MemoryEntryRejected
            | Self::MemoryRejectionsSummary => EventClass::Security,
            Self::AgentToolsSelected
            | Self::AgentNudge
            | Self::AgentStalled
            | Self::AgentComposeChecked
            | Self::AgentBudgetCheckpoint => EventClass::Agent,
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
    /// Agent-loop internal decisions (routing · reflection · stall ·
    /// compose · budget) — the `agent` verb's observable mind.
    Agent,
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
        EventKind::TaskRecovered,
        EventKind::TaskCancelled,
        EventKind::WorkflowCancelled,
        EventKind::CostIncurred,
        EventKind::InferChunk,
        EventKind::PermitChecked,
        EventKind::AgentToolsSelected,
        EventKind::AgentNudge,
        EventKind::AgentStalled,
        EventKind::AgentComposeChecked,
        EventKind::AgentBudgetCheckpoint,
        EventKind::TaskCacheHit,
        EventKind::WorkflowPaused,
        EventKind::RunSealed,
        EventKind::Declassify,
        EventKind::ApprovalDecided,
        EventKind::MemoryEntryRejected,
        EventKind::MemoryRejectionsSummary,
        EventKind::NotifyDelivered,
        EventKind::NotifyFailed,
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
                | EventKind::TaskRecovered
                | EventKind::TaskCancelled
                | EventKind::WorkflowCancelled
                | EventKind::CostIncurred
                | EventKind::InferChunk
                | EventKind::PermitChecked
                | EventKind::AgentToolsSelected
                | EventKind::AgentNudge
                | EventKind::AgentStalled
                | EventKind::AgentComposeChecked
                | EventKind::AgentBudgetCheckpoint
                | EventKind::TaskCacheHit
                | EventKind::WorkflowPaused
                | EventKind::RunSealed
                | EventKind::Declassify
                | EventKind::ApprovalDecided
                | EventKind::MemoryEntryRejected
                | EventKind::MemoryRejectionsSummary
                | EventKind::NotifyDelivered
                | EventKind::NotifyFailed => {}
            }
        }
        assert_eq!(ALL.len(), 32, "extend ALL when a variant is added");
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
        // A cache hit is a SUCCESS-shaped task event (ADR-099): never a
        // failure, never terminal for the run.
        assert!(!EventKind::TaskCacheHit.is_failure());
        assert!(!EventKind::TaskCacheHit.is_terminal());
        // Paused is terminal for THIS invocation but NEVER a failure —
        // the run exits cleanly with state `paused` (ADR-099 rider).
        assert!(EventKind::WorkflowPaused.is_terminal());
        assert!(!EventKind::WorkflowPaused.is_failure());
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
                "checkpoint_written" | "run_sealed" => Some(EventClass::Durability),
                "cost_incurred" => Some(EventClass::Cost),
                "infer_chunk" => Some(EventClass::Stream),
                "permit_checked"
                | "declassify"
                | "approval_decided"
                | "memory_entry_rejected"
                | "memory_rejections_summary" => Some(EventClass::Security),
                // ADR-111 · delivery evidence rides beside the pause it
                // narrates — Workflow class by decision, not by prefix.
                "notify_delivered" | "notify_failed" => Some(EventClass::Workflow),
                s if s.starts_with("agent_") => Some(EventClass::Agent),
                _ => None,
            };
            assert_eq!(Some(k.class()), expected, "class drift for {k:?}");
        }
    }

    #[test]
    fn agent_kinds_are_neither_terminal_nor_failures() {
        // The agent-loop telemetry is DIAGNOSTIC: a stall is evidence the
        // task-level failure event will carry the verdict for — the
        // lifecycle classifiers must not double-count it (ADR-096).
        for k in [
            EventKind::AgentToolsSelected,
            EventKind::AgentNudge,
            EventKind::AgentStalled,
            EventKind::AgentComposeChecked,
            EventKind::AgentBudgetCheckpoint,
        ] {
            assert!(!k.is_terminal(), "{k:?} must not be terminal");
            assert!(!k.is_failure(), "{k:?} must not be a lifecycle failure");
            assert_eq!(k.class(), EventClass::Agent);
        }
    }

    #[test]
    fn memory_entry_rejected_is_diagnostic_security_evidence() {
        // F-P8 (SMSR): a rejected memory entry is a trust-boundary VERDICT,
        // not a lifecycle state — the recall API carries the verdict, the
        // event is the journal's evidence. Diagnostic like the agent
        // cohort: never terminal, never a lifecycle failure.
        let k = EventKind::MemoryEntryRejected;
        assert!(!k.is_terminal(), "{k:?} must not be terminal");
        assert!(!k.is_failure(), "{k:?} must not be a lifecycle failure");
        assert_eq!(k.class(), EventClass::Security);
        assert_eq!(k.as_str(), "memory_entry_rejected");
    }

    #[test]
    fn memory_rejections_summary_is_diagnostic_security_evidence() {
        // H14: the flood summary rides the same posture as its named
        // sibling — the fold attests the count, the journal names the
        // evidence ONCE, and neither is a lifecycle state.
        let k = EventKind::MemoryRejectionsSummary;
        assert!(!k.is_terminal(), "{k:?} must not be terminal");
        assert!(!k.is_failure(), "{k:?} must not be a lifecycle failure");
        assert_eq!(k.class(), EventClass::Security);
        assert_eq!(k.as_str(), "memory_rejections_summary");
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

    #[test]
    fn notify_kinds_are_observable_history_never_control_flow() {
        // ADR-111: the pause event's outbound delivery is journaled
        // evidence — the run's `paused` outcome is identical with or
        // without it, so neither kind is terminal nor a failure, and
        // both class as Workflow beside the pause they narrate.
        for k in [EventKind::NotifyDelivered, EventKind::NotifyFailed] {
            assert!(!k.is_terminal(), "{k:?} must not be terminal");
            assert!(!k.is_failure(), "{k:?} must not be a lifecycle failure");
            assert_eq!(k.class(), EventClass::Workflow);
        }
        assert_eq!(EventKind::NotifyDelivered.as_str(), "notify_delivered");
        assert_eq!(EventKind::NotifyFailed.as_str(), "notify_failed");
    }
}
