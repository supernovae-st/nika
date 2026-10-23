// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! NIK-131 · the journal keeps a frame over its line bound whole: the
//! largest values move beside it, the chain commits to their digests, and
//! the readers get the frame back as it was emitted.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use nika_event::{Event, EventKind};
use nika_runtime::EventSink as _;
use nika_types::id::EventId;
use nika_types::resource::{KeyValue, Value as FieldValue};
use nika_types::timestamp::Timestamp;
use uuid::Uuid;

use super::{SPILLED_FIELDS, rehydrate, unresolved_values, values_dir};
use crate::journal::TraceFileSink;

fn frame(kind: EventKind, ms: u64) -> Event {
    Event::new(
        EventId::new(Uuid::from_u128(u128::from(ms))),
        Timestamp::from_unix_ms(ms),
        kind,
    )
}

/// A fetched JSON page of `bytes`, quotes and all (each one escapes twice
/// on its line: once in the field's own JSON text, once on the line).
fn page(bytes: usize) -> String {
    let row = r#"{"id":1234,"title":"a story « quoted »","url":"https://example.org/x"},"#;
    let mut text = String::from("[");
    while text.len() < bytes {
        text.push_str(row);
    }
    text.push(']');
    text
}

/// The `task_completed` the runtime writes for a task that returned `value`:
/// its `output` and its `outcome` each carry the whole value.
fn completed(value: &str) -> Event {
    let output = serde_json::to_string(value).expect("output text");
    let outcome = serde_json::json!({
        "class": "success", "cause": "normal", "payload": {"attempts": 1, "value": value},
    })
    .to_string();
    frame(EventKind::TaskCompleted, 2)
        .with_field(KeyValue::new("task", FieldValue::string("fetch")))
        .with_field(KeyValue::new(
            "def_hash",
            FieldValue::string("d".repeat(64)),
        ))
        .with_field(KeyValue::new(
            "input_hash",
            FieldValue::string("i".repeat(64)),
        ))
        .with_field(KeyValue::new("output", FieldValue::string(output)))
        .with_field(KeyValue::new("outcome", FieldValue::string(outcome)))
}

/// Journal `events` under `dir`; the journal path and its whole bytes.
fn journal(dir: &Path, events: &[Event]) -> (PathBuf, String) {
    let mut sink = TraceFileSink::new(dir);
    for event in events {
        sink.emit(event.clone());
    }
    sink.finalize();
    let path = sink.path().expect("the journal opened").to_path_buf();
    assert!(sink.into_error().is_none(), "every frame was written");
    let raw = std::fs::read_to_string(&path).expect("journal bytes");
    (path, raw)
}

fn run_of(value: &str) -> Vec<Event> {
    vec![
        frame(EventKind::WorkflowStarted, 1),
        completed(value),
        frame(EventKind::WorkflowCompleted, 3),
    ]
}

/// The live case: a 454,763-byte page made a frame over the 1 MiB bound.
#[test]
fn a_frame_over_the_line_bound_is_journaled_whole() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let value = page(454_763);
    let events = run_of(&value);
    let emitted = serde_json::to_string(&events[1]).expect("frame");
    assert!(
        emitted.len() > crate::chain::MAX_LINE_BYTES,
        "the fixture is the live case: {} bytes",
        emitted.len()
    );
    let (path, raw) = journal(tmp.path(), &events);
    assert!(
        raw.lines().all(|l| l.len() <= crate::chain::MAX_LINE_BYTES),
        "the line bound holds"
    );
    assert!(
        matches!(
            crate::chain::walk(&raw),
            crate::chain::Verdict::Intact { events: 3, .. }
        ),
        "the trace is complete and its chain intact"
    );
    let kept = std::fs::read_dir(values_dir(&path))
        .expect("values beside")
        .count();
    assert_eq!(kept, 1, "one value moved: the line fit after it");
    let mut read = crate::recover::recover_events(&raw, "spill")
        .expect("recovers")
        .events;
    assert!(
        read[1].str_field(SPILLED_FIELDS).is_some(),
        "the line names what it moved"
    );
    let report = rehydrate(&mut read, &path);
    assert_eq!((report.restored, report.unresolved.len()), (1, 0));
    assert_eq!(read, events, "every frame reads back as emitted");
    assert!(unresolved_values(&raw, &path).is_empty());
}

/// Larger than two lines: both copies of the value move, the line stays.
#[test]
fn a_value_larger_than_a_line_moves_every_copy() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let value = page(1_500_000);
    let events = run_of(&value);
    let (path, raw) = journal(tmp.path(), &events);
    assert!(raw.lines().all(|l| l.len() <= crate::chain::MAX_LINE_BYTES));
    let kept = std::fs::read_dir(values_dir(&path))
        .expect("values beside")
        .count();
    assert_eq!(kept, 2, "`output` and `outcome` are two different texts");
    let mut read = crate::recover::recover_events(&raw, "spill")
        .expect("recovers")
        .events;
    assert_eq!(rehydrate(&mut read, &path).restored, 2);
    assert_eq!(read, events);
}

/// A value whose bytes changed, or that is gone, is never read back: its
/// field keeps the digest (a resume runs the task again) and it is named.
#[test]
fn an_altered_or_missing_value_is_never_read_back() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let events = run_of(&page(454_763));
    let (path, raw) = journal(tmp.path(), &events);
    let kept = (std::fs::read_dir(values_dir(&path)).expect("values"))
        .map(|entry| entry.expect("entry").path())
        .next()
        .expect("one value kept");
    let mut bytes = std::fs::read(&kept).expect("kept value");
    bytes[10] ^= 1;
    std::fs::write(&kept, &bytes).expect("alter");
    for gone in [false, true] {
        if gone {
            std::fs::remove_file(&kept).expect("remove");
        }
        let mut read = crate::recover::recover_events(&raw, "spill")
            .expect("recovers")
            .events;
        let report = rehydrate(&mut read, &path);
        assert_eq!(report.restored, 0);
        assert_eq!(report.unresolved.len(), 1, "{report:?}");
        assert!(report.unresolved[0].starts_with("task_completed.outcome sha256:"));
        let left = read[1].str_field("outcome").expect("the field stays");
        assert!(left.starts_with("sha256:") && left.len() == 71, "{left}");
        assert!(read[1].str_field(SPILLED_FIELDS).is_some(), "still named");
        assert_eq!(unresolved_values(&raw, &path), report.unresolved);
    }
}

/// The bound still holds where no value can move: a line that stays over
/// it is refused, the journal keeps its prefix and stops there.
#[test]
fn a_line_no_value_can_shorten_is_still_refused() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut sink = TraceFileSink::new(tmp.path());
    sink.emit(frame(EventKind::WorkflowStarted, 1));
    let keyed = frame(EventKind::TaskCompleted, 2).with_field(KeyValue::new(
        "k".repeat(crate::chain::MAX_LINE_BYTES),
        FieldValue::Int(1),
    ));
    sink.emit(keyed);
    let path = sink.path().expect("journal").to_path_buf();
    assert!(sink.is_disabled(), "the lane retired");
    let error = sink.into_error().expect("refused").to_string();
    assert!(error.contains("journal line is"), "{error}");
    assert!(!values_dir(&path).exists(), "nothing was moved");
    let raw = std::fs::read_to_string(&path).expect("prefix");
    assert_eq!(raw.lines().count(), 1);
}
