// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Synthetic journal witnesses for the task outcome machine projection.

use super::tasks_json;
use crate::{RunView, demo};
use nika_event::{Event, EventKind};
use nika_types::resource::{KeyValue, Value as FieldValue};
use serde_json::{Value, json};

fn event(kind: EventKind, task: &str) -> Event {
    demo::bare_event(kind, 1).with_field(KeyValue::new("task", FieldValue::String(task.to_owned())))
}

fn field(event: Event, key: &str, value: FieldValue) -> Event {
    event.with_field(KeyValue::new(key, value))
}

fn outcome(kind: EventKind, task: &str, class: &str, cause: &str, payload: Value) -> Event {
    let mut envelope = json!({"class": class, "cause": cause});
    envelope["payload"] = payload;
    field(
        event(kind, task),
        "outcome",
        FieldValue::String(envelope.to_string()),
    )
}

fn failure(code: &str, message: &str, cause: &str) -> Event {
    outcome(
        EventKind::TaskFailed,
        "task",
        "failure",
        cause,
        json!({"error": {"code": code, "message": message, "transient": false}, "attempts": 1}),
    )
}

fn marker(code: FieldValue) -> Event {
    field(event(EventKind::TaskRecovered, "task"), "code", code)
}

fn recovery() -> Vec<Event> {
    vec![
        event(EventKind::TaskStarted, "task"),
        marker(FieldValue::String("NIKA-EXEC-001".into())),
        outcome(
            EventKind::TaskCompleted,
            "task",
            "success",
            "recovered",
            json!({"value": "fallback", "attempts": 1,
                   "recovered_from": {"code": "NIKA-EXEC-001", "message": "old", "transient": false}}),
        ),
    ]
}

fn row(events: &[Event]) -> Value {
    let mut view = RunView::new();
    for event in events {
        view.apply(event);
    }
    tasks_json(&view, events)["tasks"]
        .as_array()
        .expect("task rows")
        .iter()
        .find(|row| row["id"] == "task")
        .expect("task row")
        .clone()
}

fn no_terminal_error(row: &Value) {
    assert!(row["error_code"].is_null(), "{row}");
    assert!(row["error_message"].is_null(), "{row}");
}

fn no_terminal_facts(row: &Value) {
    no_terminal_error(row);
    assert!(row["cause"].is_null(), "{row}");
    assert!(row["recovered_from"].is_null(), "{row}");
}

#[test]
fn failed_outcomes_preserve_recorded_codes_causes_and_messages_without_a_start() {
    for (cause, code) in [
        ("verb_error", "NIKA-EXEC-001"),
        ("timeout", "NIKA-TIMEOUT-001"),
        ("retry_exhausted", "NIKA-EXEC-001"),
    ] {
        let message = "échec 🦋\n[redacted]";
        let got = row(&[failure(code, message, cause)]);
        assert_eq!(got["status"], "failed");
        assert_eq!(got["cause"], cause);
        assert_eq!(got["error_code"], code);
        assert_eq!(got["error_message"], message);
        assert!(got["recovered_from"].is_null());
    }
}

#[test]
fn error_skip_keeps_its_error_but_gate_skip_never_reads_prose_as_an_error() {
    let skipped = outcome(
        EventKind::TaskSkipped,
        "task",
        "skipped",
        "error_skip",
        json!({"error": {"code": "NIKA-EXEC-001", "message": "preserved", "transient": false}}),
    );
    let got = row(&[skipped]);
    assert_eq!(got["status"], "skipped");
    assert_eq!(got["cause"], "error_skip");
    assert_eq!(got["error_code"], "NIKA-EXEC-001");
    assert_eq!(got["error_message"], "preserved");
    let gate = field(
        outcome(EventKind::TaskSkipped, "task", "skipped", "gate", json!({})),
        "detail",
        FieldValue::String("NIKA-EXEC-999 in prose".into()),
    );
    let got = row(&[gate]);
    assert_eq!(got["cause"], "gate");
    no_terminal_error(&got);
    assert!(got["recovered_from"].is_null());
}

#[test]
fn recovery_is_provenance_and_never_the_terminal_error() {
    let got = row(&recovery());
    assert_eq!(got["status"], "recovered");
    assert_eq!(got["cause"], "recovered");
    assert_eq!(got["recovered_from"], "NIKA-EXEC-001");
    no_terminal_error(&got);
}

#[test]
fn recovery_cannot_override_a_new_running_failed_successful_or_cached_occurrence() {
    for (terminal, status, cause, code) in [
        (None, "running", None, None),
        (
            Some(failure("NIKA-TIMEOUT-001", "new", "timeout")),
            "failed",
            Some("timeout"),
            Some("NIKA-TIMEOUT-001"),
        ),
        (
            Some(outcome(
                EventKind::TaskCompleted,
                "task",
                "success",
                "normal",
                json!({"value": 1, "attempts": 1}),
            )),
            "ok",
            Some("normal"),
            None,
        ),
        (
            Some(outcome(
                EventKind::TaskCacheHit,
                "task",
                "success",
                "normal",
                json!({"value": 1, "attempts": 1}),
            )),
            "ok",
            Some("normal"),
            None,
        ),
    ] {
        let mut events = recovery();
        events.push(event(EventKind::TaskStarted, "task"));
        if let Some(terminal) = terminal {
            events.push(terminal);
        }
        let got = row(&events);
        assert_eq!(got["status"], status, "{got}");
        assert_eq!(got["cause"], json!(cause), "{got}");
        assert_eq!(got["error_code"], json!(code), "{got}");
        assert!(got["recovered_from"].is_null(), "{got}");
    }
    let mut events = recovery();
    events.push(event(EventKind::TaskCacheHit, "task"));
    let got = row(&events);
    assert_eq!(got["status"], "ok");
    no_terminal_facts(&got);
}

#[test]
fn malformed_current_failure_never_borrows_a_previous_code_or_display_detail() {
    let mut events = vec![failure("NIKA-EXEC-001", "old", "verb_error")];
    events.push(event(EventKind::TaskStarted, "task"));
    let running = row(&events);
    assert_eq!(running["status"], "running");
    no_terminal_facts(&running);
    events.push(field(
        field(
            event(EventKind::TaskFailed, "task"),
            "outcome",
            FieldValue::String("{".into()),
        ),
        "detail",
        FieldValue::String("NIKA-EXEC-999 in prose".into()),
    ));
    let got = row(&events);
    assert_eq!(got["status"], "failed");
    no_terminal_facts(&got);
}

#[test]
fn absent_terminal_outcomes_and_later_nonterminal_boundaries_hide_old_errors() {
    for kind in [
        EventKind::TaskFailed,
        EventKind::TaskCompleted,
        EventKind::TaskSkipped,
        EventKind::TaskCancelled,
        EventKind::TaskCacheHit,
        EventKind::TaskScheduled,
        EventKind::TaskStarted,
        EventKind::TaskRetrying,
        EventKind::TaskRecovered,
        EventKind::WorkflowPaused,
        EventKind::WorkflowStarted,
    ] {
        let got = row(&[
            failure("NIKA-EXEC-001", "old", "verb_error"),
            event(kind, "task"),
        ]);
        no_terminal_facts(&got);
    }
}

#[test]
fn journal_order_wins_over_timestamps_and_other_task_events() {
    let mut old = failure("NIKA-EXEC-001", "old", "verb_error");
    old.timestamp = nika_types::timestamp::Timestamp::from_unix_ms(900);
    let newer = failure("NIKA-TIMEOUT-001", "new", "timeout");
    let other = outcome(
        EventKind::TaskFailed,
        "other",
        "failure",
        "verb_error",
        json!({
            "error": {"code": "NIKA-EXEC-999", "message": "unrelated"}, "attempts": 1
        }),
    );
    let got = row(&[old, newer, other]);
    assert_eq!(got["cause"], "timeout");
    assert_eq!(got["error_code"], "NIKA-TIMEOUT-001");
    assert_eq!(got["error_message"], "new");
}

#[test]
fn invalid_outcome_envelopes_never_enable_recovery_fallback() {
    let mut bad = vec![FieldValue::Int(2), FieldValue::String("{".into())];
    for value in [
        Value::Null,
        json!([]),
        json!({}),
        json!({"class": "failure", "cause": "verb_error", "payload": {}}),
        json!({"class": "success", "cause": "timeout", "payload": {}}),
        json!({"class": "success", "cause": "future", "payload": {}}),
        json!({"class": 1, "cause": "normal", "payload": {}}),
        json!({"class": "success", "cause": false, "payload": {}}),
        json!({"class": "success", "cause": "normal", "payload": null}),
        json!({"class": "success", "cause": "normal", "payload": []}),
    ] {
        bad.push(FieldValue::String(value.to_string()));
    }
    for value in bad {
        let mut events = recovery();
        events.push(event(EventKind::TaskStarted, "task"));
        events.push(marker(FieldValue::String("NIKA-TIMEOUT-001".into())));
        events.push(field(
            event(EventKind::TaskCompleted, "task"),
            "outcome",
            value,
        ));
        let got = row(&events);
        assert_eq!(got["status"], "ok", "{got}");
        no_terminal_facts(&got);
    }
}

#[test]
fn error_leaves_are_independent_strings_or_null_without_coercion() {
    for (error, code, message) in [
        (Value::Null, None, None),
        (json!("NIKA-EXEC-001"), None, None),
        (json!({}), None, None),
        (
            json!({"code": 42, "message": "recorded"}),
            None,
            Some("recorded"),
        ),
        (
            json!({"code": "NIKA-EXEC-001", "message": false}),
            Some("NIKA-EXEC-001"),
            None,
        ),
        (json!({"code": null, "message": null}), None, None),
    ] {
        let got = row(&[outcome(
            EventKind::TaskFailed,
            "task",
            "failure",
            "verb_error",
            json!({"error": error}),
        )]);
        assert_eq!(got["cause"], "verb_error");
        assert_eq!(got["error_code"], json!(code));
        assert_eq!(got["error_message"], json!(message));
    }
}

#[test]
fn legacy_recovery_uses_the_latest_marker_without_searching_past_a_bad_code() {
    for (code, expected) in [
        (
            Some(FieldValue::String("NIKA-TIMEOUT-001".into())),
            Some("NIKA-TIMEOUT-001"),
        ),
        (Some(FieldValue::Int(9)), None),
        (None, None),
    ] {
        let mut events = recovery();
        events.push(event(EventKind::TaskStarted, "task"));
        events.push(marker(FieldValue::String("NIKA-EXEC-001".into())));
        events.push(code.map_or_else(|| event(EventKind::TaskRecovered, "task"), marker));
        events.push(event(EventKind::TaskCompleted, "task"));
        let got = row(&events);
        assert_eq!(got["status"], "recovered");
        assert_eq!(got["recovered_from"], json!(expected));
        assert!(got["cause"].is_null());
        no_terminal_error(&got);
    }
}

#[test]
fn legacy_recovery_cannot_cross_an_occurrence_boundary() {
    for boundary in [
        EventKind::TaskStarted,
        EventKind::TaskScheduled,
        EventKind::TaskRetrying,
        EventKind::WorkflowPaused,
        EventKind::WorkflowStarted,
        EventKind::TaskFailed,
        EventKind::TaskSkipped,
        EventKind::TaskCancelled,
        EventKind::TaskCacheHit,
        EventKind::TaskCompleted,
    ] {
        let events = [
            marker(FieldValue::String("NIKA-EXEC-001".into())),
            event(boundary, "task"),
            event(EventKind::TaskCompleted, "task"),
        ];
        let got = row(&events);
        assert_eq!(got["status"], "ok", "{boundary:?}: {got}");
        no_terminal_facts(&got);
    }
}

#[test]
fn explicit_normal_outcome_outranks_a_recovery_marker() {
    let got = row(&[
        marker(FieldValue::String("NIKA-EXEC-001".into())),
        outcome(
            EventKind::TaskCompleted,
            "task",
            "success",
            "normal",
            json!({"value": 1, "attempts": 1}),
        ),
    ]);
    assert_eq!(got["status"], "ok");
    assert_eq!(got["cause"], "normal");
    assert!(got["recovered_from"].is_null());
    no_terminal_error(&got);
}

#[test]
fn recovered_outcome_has_string_or_null_provenance_without_a_marker() {
    for original in [Value::Null, json!("old"), json!({"code": 8}), json!({})] {
        let got = row(&[outcome(
            EventKind::TaskCompleted,
            "task",
            "success",
            "recovered",
            json!({
                "value": "fallback", "attempts": 1, "recovered_from": original
            }),
        )]);
        assert_eq!(got["status"], "recovered");
        assert_eq!(got["cause"], "recovered");
        assert!(got["recovered_from"].is_null());
        no_terminal_error(&got);
    }
    let got = row(&[outcome(
        EventKind::TaskCompleted,
        "task",
        "success",
        "recovered",
        json!({
            "value": "fallback", "attempts": 1,
            "recovered_from": {"code": "NIKA-TIMEOUT-001", "message": "old", "transient": false}
        }),
    )]);
    assert_eq!(got["recovered_from"], "NIKA-TIMEOUT-001");
    no_terminal_error(&got);
}

#[test]
fn cancelled_causes_are_recorded_but_a_cache_hit_cannot_claim_recovery() {
    for cause in ["upstream", "operator", "budget"] {
        let got = row(&[outcome(
            EventKind::TaskCancelled,
            "task",
            "cancelled",
            cause,
            json!({"reason": cause}),
        )]);
        assert_eq!(got["status"], "cancelled");
        assert_eq!(got["cause"], cause);
        no_terminal_error(&got);
    }
    let got = row(&[outcome(
        EventKind::TaskCacheHit,
        "task",
        "success",
        "recovered",
        json!({
            "value": "cached", "attempts": 1, "recovered_from": {"code": "NIKA-EXEC-001"}
        }),
    )]);
    assert_eq!(got["status"], "ok");
    no_terminal_facts(&got);
}

#[test]
fn in_flight_states_never_inherit_recovered_status() {
    for (kind, status) in [
        (EventKind::TaskScheduled, "pending"),
        (EventKind::TaskStarted, "running"),
        (EventKind::TaskRetrying, "retrying"),
        (EventKind::TaskRecovered, "running"),
        (EventKind::WorkflowPaused, "paused"),
    ] {
        let mut events = recovery();
        events.push(event(kind, "task"));
        let got = row(&events);
        assert_eq!(got["status"], status, "{got}");
        no_terminal_facts(&got);
    }
}
