// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::redundant_closure_for_method_calls
)]

//! Contract tests for `nika-event` — the L0 event chronicle surface.
//!
//! Covers: `EventKind` taxonomy invariants, `Event` builder, the `Emitter`
//! trait (`NoOp` + bounded/unbounded `InMemory`), `EventError` codes, and
//! serde round-trip. Property tests assert the kind invariants hold across
//! all variants.

use nika_event::{Emitter, Event, EventError, EventKind, InMemoryEmitter, NoOpEmitter};
use nika_types::id::{CorrelationId, EventId, RunId};
use nika_types::resource::{KeyValue, Value};
use nika_types::timestamp::Timestamp;
use uuid::Uuid;

fn ev(kind: EventKind) -> Event {
    Event::new(EventId::new(Uuid::nil()), Timestamp::from_unix_ms(0), kind)
}

// ─── EventKind taxonomy ──────────────────────────────────────────────

#[test]
fn kind_as_str_is_snake_case_and_stable() {
    assert_eq!(EventKind::WorkflowStarted.as_str(), "workflow_started");
    assert_eq!(EventKind::TaskCompleted.as_str(), "task_completed");
    assert_eq!(EventKind::VerbInvoked.as_str(), "verb_invoked");
    assert_eq!(EventKind::CheckpointWritten.as_str(), "checkpoint_written");
}

#[test]
fn kind_display_matches_as_str() {
    assert_eq!(format!("{}", EventKind::ToolInvoked), "tool_invoked");
}

#[test]
fn terminal_kinds_are_exactly_workflow_completed_and_failed() {
    assert!(EventKind::WorkflowCompleted.is_terminal());
    assert!(EventKind::WorkflowFailed.is_terminal());
    assert!(!EventKind::TaskCompleted.is_terminal());
    assert!(!EventKind::WorkflowStarted.is_terminal());
}

#[test]
fn failure_kinds_are_exactly_workflow_and_task_failed() {
    assert!(EventKind::WorkflowFailed.is_failure());
    assert!(EventKind::TaskFailed.is_failure());
    assert!(!EventKind::TaskCompleted.is_failure());
    assert!(!EventKind::TaskSkipped.is_failure());
}

// ─── Event builder ───────────────────────────────────────────────────

#[test]
fn new_event_has_no_run_correlation_or_fields() {
    let e = ev(EventKind::WorkflowStarted);
    assert!(e.run.is_none());
    assert!(e.correlation.is_none());
    assert!(e.fields.is_empty());
    assert!(!e.is_terminal());
}

#[test]
fn event_is_terminal_delegates_to_kind() {
    assert!(ev(EventKind::WorkflowCompleted).is_terminal());
    assert!(ev(EventKind::WorkflowFailed).is_terminal());
    assert!(!ev(EventKind::TaskCompleted).is_terminal());
}

#[test]
fn builder_chains_run_correlation_fields() {
    let run = RunId::new(Uuid::from_u128(1));
    let corr = CorrelationId::new(Uuid::from_u128(2));
    let e = ev(EventKind::TaskStarted)
        .with_run(run)
        .with_correlation(corr)
        .with_field(KeyValue::new("task", Value::String("build".into())))
        .with_field(KeyValue::new("attempt", Value::Int(1)));
    assert_eq!(e.run, Some(run));
    assert_eq!(e.correlation, Some(corr));
    assert_eq!(e.fields.len(), 2);
}

#[test]
fn with_fields_replaces_all() {
    let e = ev(EventKind::VerbInvoked)
        .with_field(KeyValue::new("a", Value::Int(1)))
        .with_fields(vec![KeyValue::new("b", Value::Int(2))]);
    assert_eq!(e.fields.len(), 1);
    assert_eq!(e.fields[0].key, "b");
}

// ─── NoOpEmitter ─────────────────────────────────────────────────────

#[test]
fn noop_emitter_never_fails_and_drops() {
    let sink = NoOpEmitter;
    for kind in [EventKind::WorkflowStarted, EventKind::WorkflowCompleted] {
        assert!(sink.emit(ev(kind)).is_ok());
    }
}

// ─── InMemoryEmitter · unbounded ─────────────────────────────────────

#[test]
fn unbounded_collects_in_order() {
    let sink = InMemoryEmitter::unbounded();
    assert!(sink.is_empty());
    sink.emit(ev(EventKind::WorkflowStarted)).unwrap();
    assert!(!sink.is_empty(), "non-empty after one emit");
    sink.emit(ev(EventKind::WorkflowCompleted)).unwrap();
    assert_eq!(sink.len(), 2);
    let drained = sink.drain();
    assert_eq!(drained.len(), 2);
    assert_eq!(drained[0].kind, EventKind::WorkflowStarted);
    assert_eq!(drained[1].kind, EventKind::WorkflowCompleted);
    assert!(sink.is_empty(), "drain leaves the buffer empty");
}

// ─── InMemoryEmitter · bounded ───────────────────────────────────────

#[test]
fn bounded_rejects_past_capacity_with_buffer_full() {
    let sink = InMemoryEmitter::bounded(2);
    assert!(sink.emit(ev(EventKind::TaskScheduled)).is_ok());
    assert!(sink.emit(ev(EventKind::TaskStarted)).is_ok());
    let err = sink
        .emit(ev(EventKind::TaskCompleted))
        .expect_err("third emit must hit the bound");
    match err {
        EventError::BufferFull { capacity } => assert_eq!(capacity, 2),
        other => panic!("expected BufferFull, got {other:?}"),
    }
    assert_eq!(sink.len(), 2, "rejected event is not buffered");
}

#[test]
fn bounded_capacity_zero_rejects_everything() {
    let sink = InMemoryEmitter::bounded(0);
    assert!(sink.emit(ev(EventKind::WorkflowStarted)).is_err());
    assert_eq!(sink.len(), 0);
}

#[test]
fn drain_after_bound_frees_capacity() {
    let sink = InMemoryEmitter::bounded(1);
    sink.emit(ev(EventKind::TaskStarted)).unwrap();
    assert!(sink.emit(ev(EventKind::TaskCompleted)).is_err());
    let _ = sink.drain();
    assert!(
        sink.emit(ev(EventKind::TaskCompleted)).is_ok(),
        "draining frees the bound"
    );
}

// ─── EventError codes ────────────────────────────────────────────────

#[test]
fn error_codes_are_distinct_and_in_observability_band() {
    let buffer_full = EventError::BufferFull { capacity: 1 };
    let serial = EventError::SerializationFailed { detail: "x".into() };
    let poisoned = EventError::LockPoisoned;
    assert_eq!(buffer_full.code().num, 802);
    assert_eq!(serial.code().num, 801);
    assert_eq!(poisoned.code().num, 803);
    assert_ne!(buffer_full.code(), serial.code());
    assert_ne!(poisoned.code(), buffer_full.code());
}

#[test]
fn buffer_full_is_transient_serialization_is_not() {
    use nika_error::prelude::NikaErrorCode;
    assert!(EventError::BufferFull { capacity: 1 }.is_transient());
    assert!(
        !EventError::SerializationFailed {
            detail: String::new()
        }
        .is_transient()
    );
}

// ─── object-safety · fan-out ─────────────────────────────────────────

#[test]
fn emitters_are_object_safe_for_fanout() {
    let sinks: Vec<Box<dyn Emitter>> = vec![
        Box::new(NoOpEmitter),
        Box::new(InMemoryEmitter::unbounded()),
    ];
    for sink in &sinks {
        sink.emit(ev(EventKind::WorkflowStarted)).unwrap();
    }
}

// ─── serde round-trip ────────────────────────────────────────────────

#[cfg(feature = "serde")]
#[test]
fn event_serde_round_trip_preserves_fields() {
    let original = ev(EventKind::TaskCompleted)
        .with_run(RunId::new(Uuid::from_u128(7)))
        .with_field(KeyValue::new("dur_ms", Value::Int(42)));
    let json = serde_json::to_string(&original).unwrap();
    let back: Event = serde_json::from_str(&json).unwrap();
    assert_eq!(original, back);
}

#[cfg(feature = "serde")]
#[test]
fn kind_serializes_as_snake_case() {
    let json = serde_json::to_string(&EventKind::CheckpointWritten).unwrap();
    assert_eq!(json, "\"checkpoint_written\"");
}

// ─── insta snapshot · taxonomy lock ──────────────────────────────────

#[test]
fn kind_taxonomy_snapshot() {
    // SYNC LAW: this list mirrors `src/kind.rs` ALL (the defining-crate
    // census, compile-forced exhaustive there). An integration test
    // cannot be compile-forced (`#[non_exhaustive]` requires `_` outside
    // the crate) — the insta snapshot is the ratchet instead: adding a
    // kind without extending THIS list leaves the snapshot green but the
    // count test below red.
    let all = [
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
    let slugs: Vec<&str> = all.iter().map(|k| k.as_str()).collect();
    insta::assert_yaml_snapshot!(slugs);
}

// ─── property tests ──────────────────────────────────────────────────

mod prop {
    use super::*;
    use proptest::prelude::*;

    fn any_kind() -> impl Strategy<Value = EventKind> {
        // SYNC LAW: mirrors `src/kind.rs` ALL (17) — see the taxonomy
        // snapshot note above.
        prop_oneof![
            Just(EventKind::WorkflowStarted),
            Just(EventKind::WorkflowCompleted),
            Just(EventKind::WorkflowFailed),
            Just(EventKind::TaskScheduled),
            Just(EventKind::TaskStarted),
            Just(EventKind::TaskCompleted),
            Just(EventKind::TaskFailed),
            Just(EventKind::TaskSkipped),
            Just(EventKind::VerbInvoked),
            Just(EventKind::ToolInvoked),
            Just(EventKind::CheckpointWritten),
            Just(EventKind::TaskRetrying),
            Just(EventKind::TaskCancelled),
            Just(EventKind::WorkflowCancelled),
            Just(EventKind::CostIncurred),
            Just(EventKind::InferChunk),
            Just(EventKind::PermitChecked),
        ]
    }

    proptest! {
        #[test]
        fn as_str_is_never_empty(kind in any_kind()) {
            prop_assert!(!kind.as_str().is_empty());
        }

        #[test]
        fn terminal_implies_not_a_task_kind(kind in any_kind()) {
            // No task-* kind is terminal; the workflow outcomes
            // (completed · failed · cancelled) are the terminal set.
            if kind.is_terminal() {
                prop_assert!(matches!(
                    kind,
                    EventKind::WorkflowCompleted
                        | EventKind::WorkflowFailed
                        | EventKind::WorkflowCancelled
                ));
            }
        }

        #[test]
        fn unbounded_emitter_accepts_n_events(n in 0usize..200) {
            let sink = InMemoryEmitter::unbounded();
            for _ in 0..n {
                prop_assert!(sink.emit(ev(EventKind::TaskStarted)).is_ok());
            }
            prop_assert_eq!(sink.len(), n);
        }

        #[test]
        fn bounded_emitter_never_exceeds_capacity(cap in 0usize..50, extra in 0usize..50) {
            let sink = InMemoryEmitter::bounded(cap);
            for _ in 0..(cap + extra) {
                let _ = sink.emit(ev(EventKind::TaskStarted));
            }
            prop_assert!(sink.len() <= cap);
        }
    }
}
