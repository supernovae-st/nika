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

    /// Events buffered so far (the attempt-boundary marks read this
    /// between awaits — never concurrently with a push).
    pub(crate) fn len(&self) -> usize {
        self.events.lock().map_or(0, |events| events.len())
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

/// One buffered decision with its provenance: which attempt produced it
/// (1-based · the `TaskRetrying` frames carry the same counter) and, for
/// `for_each` tasks, which iteration — without these two stamps a
/// retried agent and a 2-iteration fan-out produce indistinguishable
/// flat streams (the wiring review's F3).
#[derive(Debug, Clone)]
pub(crate) struct StampedAgentEvent {
    pub attempt: u32,
    pub iteration: Option<u32>,
    pub event: AgentEvent,
}

/// Stamp a drained buffer with attempt provenance. `marks[i]` is the
/// buffer length after attempt `i+1` returned; events past the last
/// mark belong to the in-flight attempt a timeout/cancel cut short —
/// exactly the evidence worth keeping (the wiring review's F1).
pub(crate) fn stamp_attempts(events: Vec<AgentEvent>, marks: &[usize]) -> Vec<StampedAgentEvent> {
    events
        .into_iter()
        .enumerate()
        .map(|(idx, event)| {
            let settled = marks.iter().filter(|&&m| idx >= m).count();
            #[allow(clippy::cast_possible_truncation)] // attempts ≤ u32 by spec
            StampedAgentEvent {
                attempt: settled as u32 + 1,
                iteration: None,
                event,
            }
        })
        .collect()
}

/// Emit one task's buffered agent decisions onto the canonical stream
/// (settle pass · the pens are the caller's).
///
/// Mapping (additive cohort 2026-06-12 · `nika-event` agent kinds):
/// routing → `agent_tools_selected` · dispatched tool → `tool_invoked`
/// (the agent path's one emission site) · compose → `agent_compose_checked`
/// · nudge → `agent_nudge` · stall → `agent_stalled` · budget →
/// `agent_budget_checkpoint` — each stamped with `task` + `attempt` (+
/// `iteration` on fan-out lanes). Loop lifecycle (`RunStarted` ·
/// `TurnStarted` · `Finished`) is intentionally NOT re-emitted — the
/// task lifecycle events already bracket the run (`TaskStarted` carries
/// the `agent · …` note · `TaskCompleted` carries turns + tokens).
pub(crate) fn emit_agent_events(
    task: &str,
    events: &[StampedAgentEvent],
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    for stamped in events {
        let Some((kind, mut fields)) = fields_of(&stamped.event) else {
            continue; // lifecycle — bracketed by the task frames
        };
        fields.insert(0, ("task", s(task)));
        fields.push(("attempt", i(i64::from(stamped.attempt))));
        if let Some(iteration) = stamped.iteration {
            fields.push(("iteration", i(i64::from(iteration))));
        }
        put(stamper, sink, kind, &fields);
    }
}

/// Map ONE agent decision onto its canonical kind + specific fields
/// (`None` for loop lifecycle — `RunStarted` · `TurnStarted` ·
/// `Finished` are bracketed by the task frames; `#[non_exhaustive]`: a
/// future decision kind lands here silently-skipped until this adapter
/// learns its mapping).
#[allow(clippy::type_complexity)] // one private call site · the tuple IS the contract
fn fields_of(event: &AgentEvent) -> Option<(EventKind, Vec<(&'static str, FieldValue)>)> {
    match event {
        AgentEvent::ToolsSelected {
            turn,
            offered,
            universe,
            by_source,
        } => Some((
            EventKind::AgentToolsSelected,
            vec![
                ("turn", i(i64::from(*turn))),
                ("offered", i(i64::from(*offered))),
                ("universe", i(i64::from(*universe))),
                ("builtin", i(i64::from(by_source.builtin))),
                ("mcp", i(i64::from(by_source.mcp))),
                ("other", i(i64::from(by_source.other))),
            ],
        )),
        AgentEvent::ToolCompleted {
            turn,
            name,
            is_error,
        } => Some((
            EventKind::ToolInvoked,
            vec![
                ("turn", i(i64::from(*turn))),
                ("tool", s(name)),
                ("error", FieldValue::Bool(*is_error)),
            ],
        )),
        AgentEvent::ComposeChecked {
            turn,
            valid,
            violations,
        } => Some((
            EventKind::AgentComposeChecked,
            vec![
                ("turn", i(i64::from(*turn))),
                ("valid", FieldValue::Bool(*valid)),
                ("violations", i(i64::from(*violations))),
            ],
        )),
        AgentEvent::Nudged {
            turn,
            reason,
            period,
        } => Some((
            EventKind::AgentNudge,
            vec![
                ("turn", i(i64::from(*turn))),
                ("reason", s(reason_slug(*reason))),
                ("period", i(i64::from(*period))),
            ],
        )),
        AgentEvent::Stalled {
            turn,
            period,
            repeats,
        } => Some((
            EventKind::AgentStalled,
            vec![
                ("turn", i(i64::from(*turn))),
                ("period", i(i64::from(*period))),
                ("repeats", i(i64::from(*repeats))),
            ],
        )),
        AgentEvent::BudgetCheckpoint {
            turn,
            total_tokens,
            budget,
        } => {
            let mut fields = vec![
                ("turn", i(i64::from(*turn))),
                (
                    "total_tokens",
                    i(i64::try_from(*total_tokens).unwrap_or(i64::MAX)),
                ),
            ];
            if let Some(b) = budget {
                fields.push(("budget", i(i64::try_from(*b).unwrap_or(i64::MAX))));
            }
            Some((EventKind::AgentBudgetCheckpoint, fields))
        }
        _ => None,
    }
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
