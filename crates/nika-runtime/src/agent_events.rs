// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The agent-loop telemetry adapter — `AgentObserver` → the canonical
//! event stream (ADR-093 · INV-024: this adapter is the ONE emission
//! site for the agent path's events).
//!
//! Topology: the dispatch pass is PEN-FREE (the settle pass owns the
//! stamper + sink), and a wave dispatches tasks CONCURRENTLY — so the
//! verb's decisions are BUFFERED per dispatch (`BufferingObserver`,
//! handed to [`nika_verb_agent::AgentVerb::run_observed`]), ride the
//! task's outcome to the settle pass, and are emitted there with the
//! task id stamped. Buffering trades sub-second liveness for exact
//! per-task attribution + the pen discipline; events keep their true
//! ORDER (the display contract folds on order, not wall time).

use std::sync::Mutex;

use nika_event::EventKind;
use nika_types::resource::Value as FieldValue;
use nika_verb_agent::{AgentEvent, AgentObserver, NudgeReason};

use crate::stamp::{EventSink, Stamper};
use crate::{emit, i, s};

/// Collects one dispatch's agent decisions (cheap: lock + push — the
/// observer contract). Drained by the dispatch that created it.
#[derive(Debug, Default)]
pub(crate) struct BufferingObserver {
    events: Mutex<Vec<AgentEvent>>,
}

impl BufferingObserver {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Take the buffered events (poisoning recovers the inner state —
    /// telemetry must never panic a run).
    pub(crate) fn into_events(self) -> Vec<AgentEvent> {
        self.events
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl AgentObserver for BufferingObserver {
    fn on_event(&self, event: &AgentEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event.clone());
        }
    }
}

/// Emit one task's buffered agent decisions onto the canonical stream
/// (settle pass · the pens are the caller's).
///
/// Mapping (additive cohort 2026-06-12 · `nika-event` agent kinds):
/// routing → `agent_tools_selected` · dispatched tool → `tool_invoked`
/// (the agent path's one emission site) · compose → `agent_compose_checked`
/// · nudge → `agent_nudge` · stall → `agent_stalled` · budget →
/// `agent_budget_checkpoint`. Loop lifecycle (`RunStarted` ·
/// `TurnStarted` · `Finished`) is intentionally NOT re-emitted — the
/// task lifecycle events already bracket the run (`TaskStarted` carries
/// the `agent · …` note · `TaskCompleted` carries turns + tokens).
pub(crate) fn emit_agent_events(
    task: &str,
    events: &[AgentEvent],
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    for event in events {
        emit_one(task, event, stamper, sink);
    }
}

/// Map ONE agent decision onto its canonical kind + fields.
fn emit_one(task: &str, event: &AgentEvent, stamper: &mut dyn Stamper, sink: &mut dyn EventSink) {
    match event {
        AgentEvent::ToolsSelected {
            turn,
            offered,
            universe,
            by_source,
        } => emit_selected(task, *turn, *offered, *universe, by_source, stamper, sink),
        AgentEvent::ToolCompleted {
            turn,
            name,
            is_error,
        } => put(
            stamper,
            sink,
            EventKind::ToolInvoked,
            &[
                ("task", s(task)),
                ("turn", i(i64::from(*turn))),
                ("tool", s(name)),
                ("error", FieldValue::Bool(*is_error)),
            ],
        ),
        AgentEvent::ComposeChecked {
            turn,
            valid,
            violations,
        } => put(
            stamper,
            sink,
            EventKind::AgentComposeChecked,
            &[
                ("task", s(task)),
                ("turn", i(i64::from(*turn))),
                ("valid", FieldValue::Bool(*valid)),
                ("violations", i(i64::from(*violations))),
            ],
        ),
        AgentEvent::Nudged {
            turn,
            reason,
            period,
        } => put(
            stamper,
            sink,
            EventKind::AgentNudge,
            &[
                ("task", s(task)),
                ("turn", i(i64::from(*turn))),
                ("reason", s(reason_slug(*reason))),
                ("period", i(i64::from(*period))),
            ],
        ),
        AgentEvent::Stalled {
            turn,
            period,
            repeats,
        } => put(
            stamper,
            sink,
            EventKind::AgentStalled,
            &[
                ("task", s(task)),
                ("turn", i(i64::from(*turn))),
                ("period", i(i64::from(*period))),
                ("repeats", i(i64::from(*repeats))),
            ],
        ),
        AgentEvent::BudgetCheckpoint {
            turn,
            total_tokens,
            budget,
        } => {
            let mut fields = vec![
                ("task", s(task)),
                ("turn", i(i64::from(*turn))),
                (
                    "total_tokens",
                    i(i64::try_from(*total_tokens).unwrap_or(i64::MAX)),
                ),
            ];
            if let Some(b) = budget {
                fields.push(("budget", i(i64::try_from(*b).unwrap_or(i64::MAX))));
            }
            put(stamper, sink, EventKind::AgentBudgetCheckpoint, &fields);
        }
        // Loop lifecycle — bracketed by the task events (see above).
        // `#[non_exhaustive]`: a future decision kind lands here
        // silently-skipped until this adapter learns its mapping.
        _ => {}
    }
}

/// The routing decision's 9 fields (the per-source breakdown).
fn emit_selected(
    task: &str,
    turn: u32,
    offered: u32,
    universe: u32,
    by_source: &nika_verb_agent::SourceCounts,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    put(
        stamper,
        sink,
        EventKind::AgentToolsSelected,
        &[
            ("task", s(task)),
            ("turn", i(i64::from(turn))),
            ("offered", i(i64::from(offered))),
            ("universe", i(i64::from(universe))),
            ("builtin", i(i64::from(by_source.builtin))),
            ("mcp", i(i64::from(by_source.mcp))),
            ("skill", i(i64::from(by_source.skill))),
            ("intrinsic", i(i64::from(by_source.intrinsic))),
            ("other", i(i64::from(by_source.other))),
        ],
    );
}

/// Emit-and-discard (the settle pass tracks timestamps only for the
/// lifecycle frames; decision events don't feed the record).
fn put(
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
    kind: EventKind,
    fields: &[(&str, FieldValue)],
) {
    let _ = emit(stamper, sink, kind, fields);
}

/// The wire slug for a nudge reason (closed vocabulary · stable).
fn reason_slug(reason: NudgeReason) -> &'static str {
    match reason {
        NudgeReason::RepeatedActions => "repeated_actions",
        NudgeReason::ErrorStreak => "error_streak",
        // `#[non_exhaustive]` upstream — a future reason keeps a stable
        // fallback rather than breaking the runtime build.
        _ => "other",
    }
}
