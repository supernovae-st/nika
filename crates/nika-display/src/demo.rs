// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Deterministic demo streams — the spec §3.3 storyboard as REAL
//! [`nika_event::Event`] values (the 11 shipped kinds, nothing invented).
//!
//! Deterministic by construction: sequential `Uuid::from_u128` ids +
//! fixed millisecond timeline → golden tests pin exact frames, and
//! `nika-cli trace replay --demo` renders identically forever.

use nika_event::{Event, EventKind};
use nika_types::id::EventId;
use nika_types::resource::{KeyValue, Value};
use nika_types::timestamp::Timestamp;
use uuid::Uuid;

/// Build one event at `ms` on the demo timeline.
fn at(seq: u128, ms: u64, kind: EventKind) -> Event {
    Event::new(
        EventId::new(Uuid::from_u128(seq)),
        Timestamp::from_unix_ms(ms),
        kind,
    )
}

fn s(key: &str, value: &str) -> KeyValue {
    KeyValue::new(key, Value::String(value.to_owned()))
}

fn f(key: &str, value: f64) -> KeyValue {
    KeyValue::new(key, Value::Float(value))
}

fn i(key: &str, value: i64) -> KeyValue {
    KeyValue::new(key, Value::Int(value))
}

/// A bare event with no fields (test helper: events must degrade safely).
#[must_use]
pub fn bare_event(kind: EventKind, ms: u64) -> Event {
    at(999, ms, kind)
}

/// The shared opening: workflow header + the five scheduled tasks.
fn opening() -> Vec<Event> {
    let mut events = vec![
        at(1, 0, EventKind::WorkflowStarted)
            .with_field(s("workflow", "veille-news"))
            .with_field(f("ceiling_usd", 0.04))
            .with_field(s(
                "permits",
                "network:read(hn.algolia.com) · fs:write(./out)",
            )),
    ];
    for (n, task) in [
        "fetch_top",
        "extract_ai",
        "summarize",
        "write_md",
        "notify_slack",
    ]
    .iter()
    .enumerate()
    {
        events.push(at(2 + n as u128, 10, EventKind::TaskScheduled).with_field(s("task", task)));
    }
    events.extend([
        at(10, 20, EventKind::TaskStarted)
            .with_field(s("task", "fetch_top"))
            .with_field(s("note", "invoke · nika:fetch")),
        at(11, 1200, EventKind::TaskCompleted)
            .with_field(s("task", "fetch_top"))
            .with_field(s("note", "http 200 · 1.2s · 34 KB")),
        at(12, 1210, EventKind::TaskStarted)
            .with_field(s("task", "extract_ai"))
            .with_field(s("note", "exec · jq")),
        at(13, 1340, EventKind::TaskCompleted)
            .with_field(s("task", "extract_ai"))
            .with_field(s("note", "jq · 0.1s · 12 items")),
        at(14, 1350, EventKind::TaskStarted)
            .with_field(s("task", "summarize"))
            .with_field(s("note", "infer · claude-sonnet")),
    ]);
    events
}

/// The success storyboard (spec §3.3 · ends `workflow_completed`).
#[must_use]
pub fn success() -> Vec<Event> {
    let mut events = opening();
    events.extend([
        at(20, 4400, EventKind::TaskCompleted)
            .with_field(s("task", "summarize"))
            .with_field(s("note", "claude-sonnet · 3.1s · $0.011"))
            .with_field(f("cost_usd", 0.011))
            .with_field(i("tokens", 710)),
        at(21, 4410, EventKind::TaskStarted)
            .with_field(s("task", "write_md"))
            .with_field(s("note", "invoke · nika:write → ./out")),
        at(22, 4700, EventKind::TaskCompleted)
            .with_field(s("task", "write_md"))
            .with_field(s("note", "2.1 KB written")),
        at(23, 4710, EventKind::TaskSkipped)
            .with_field(s("task", "notify_slack"))
            .with_field(s("note", "when: env.CI != 'true'")),
        at(24, 4720, EventKind::WorkflowCompleted).with_field(s("workflow", "veille-news")),
    ]);
    events
}

/// The mid-retry storyboard (§3.1 `↻` — a transient refusal · the
/// retry scheduled · the run still in flight). The one state the
/// success/failure tapes never show.
#[must_use]
pub fn retrying() -> Vec<Event> {
    let mut events = opening();
    events.extend([
        at(10, 600, EventKind::TaskStarted)
            .with_field(s("task", "fetch_top"))
            .with_field(s("note", "invoke · nika:fetch")),
        at(11, 1800, EventKind::TaskCompleted)
            .with_field(s("task", "fetch_top"))
            .with_field(s("note", "http 200 · 1.2s · 34 KB")),
        at(12, 1900, EventKind::TaskStarted)
            .with_field(s("task", "summarize"))
            .with_field(s("note", "infer · claude-sonnet")),
        // The attempt failed · the TASK has not — the row holds yellow
        // until a terminal frame replaces it (the runtime stamps the
        // chosen backoff on the frame · attempt/max_attempts/delay_ms).
        at(13, 3100, EventKind::TaskRetrying)
            .with_field(s("task", "summarize"))
            .with_field(s("note", "rate limited · retrying"))
            .with_field(i("attempt", 1))
            .with_field(i("max_attempts", 3))
            .with_field(i("delay_ms", 740)),
    ]);
    events
}

/// The failure storyboard (provider refusal → downstream skips → card).
#[must_use]
pub fn failure() -> Vec<Event> {
    let mut events = opening();
    events.extend([
        at(30, 5600, EventKind::TaskFailed)
            .with_field(s("task", "summarize"))
            .with_field(s("note", "NIKA-431 · provider refused"))
            .with_field(s(
                "detail",
                "summarize failed · NIKA-431 provider refused (429) · retried 2×",
            )),
        // Spec 03 · an upstream failure CANCELS default-gate downstream
        // (blocked `⊘` · a decision, not a defect — the runtime's cascade).
        at(31, 5610, EventKind::TaskCancelled)
            .with_field(s("task", "write_md"))
            .with_field(s("note", "upstream failed")),
        at(32, 5620, EventKind::TaskCancelled)
            .with_field(s("task", "notify_slack"))
            .with_field(s("note", "upstream failed")),
        at(33, 5630, EventKind::WorkflowFailed).with_field(s("workflow", "veille-news")),
    ]);
    events
}

/// A completed run that repaired a task (`task_recovered` then Ok).
/// Persona 14 · gauntlet g2: the first glance (`--quiet` headline · the
/// shareable card title) must not look like an unblemished success.
#[must_use]
pub fn recovered() -> Vec<Event> {
    vec![
        at(100, 0, EventKind::WorkflowStarted).with_field(s("workflow", "recovered")),
        at(101, 10, EventKind::TaskScheduled).with_field(s("task", "risky")),
        at(102, 11, EventKind::TaskScheduled).with_field(s("task", "greet")),
        at(103, 20, EventKind::TaskStarted)
            .with_field(s("task", "risky"))
            .with_field(s("note", "invoke · nika:assert")),
        at(104, 25, EventKind::TaskRecovered)
            .with_field(s("task", "risky"))
            .with_field(s("note", "on_error recover")),
        at(105, 30, EventKind::TaskCompleted)
            .with_field(s("task", "risky"))
            .with_field(s("note", "recovered")),
        at(106, 40, EventKind::TaskStarted)
            .with_field(s("task", "greet"))
            .with_field(s("note", "infer · mock/echo")),
        at(107, 80, EventKind::TaskCompleted)
            .with_field(s("task", "greet"))
            .with_field(s("note", "mocked")),
        at(108, 90, EventKind::WorkflowCompleted).with_field(s("workflow", "recovered")),
    ]
}

/// The paused storyboard (ADR-099 rider · ends `workflow_paused`): the
/// run parked at a human gate — no verdict, the gate row left awaiting.
#[must_use]
pub fn paused() -> Vec<Event> {
    let mut events = opening();
    events.extend([at(30, 4400, EventKind::WorkflowPaused)
        .with_field(s("task", "summarize"))
        .with_field(s("mode", "confirm"))
        .with_field(s("message", "send the digest?"))]);
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_streams_are_deterministic() {
        assert_eq!(success(), success());
        assert_eq!(failure(), failure());
        assert_eq!(paused(), paused());
        assert_eq!(recovered(), recovered());
        assert_ne!(success(), failure());
        assert_ne!(success(), recovered());
    }

    #[test]
    fn demo_uses_only_shipped_event_kinds() {
        // The Rust demo never invents §3bis proposed kinds — honesty gate.
        for ev in success().iter().chain(failure().iter()) {
            // Compiles = the kind exists in the shipped enum; the assert
            // documents the intent for the reader.
            let _ = ev.kind.as_str();
        }
    }

    #[test]
    fn scheduled_event_ids_pin_the_documented_sequence() {
        // The five TaskScheduled events occupy seq 2..=6 — golden replay
        // identity (drifted id arithmetic would break byte-stable traces).
        let events = success();
        for (n, ev) in events[1..6].iter().enumerate() {
            assert_eq!(
                ev.id,
                EventId::new(Uuid::from_u128(2 + n as u128)),
                "seq of scheduled[{n}]"
            );
        }
    }
}
