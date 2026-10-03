// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The child run a task called is known only from that task's settle frame
//! in this run (`task_completed`, its `child` row): its journal opens from
//! the task's detail on `Enter`, the host reads it, and `Backspace` returns
//! to the parent, which never stopped folding its own frames. A row seen on
//! another frame, of another run, after the settlement, malformed or with an
//! outcome nobody recognises opens nothing.

use crossterm::event::KeyCode;

use super::task_tests::{
    EXEC, WIDE, asked, desk, field, key, object, quoted, settled, start, task,
};
use crate::session::acquire::{ChildRead, Fetched, Proven};
use crate::workspace::desk::{Desk, Route, acquire_all};
use nika_display::run_story::{ChildRun, ExecutionId};

const OTHER: &str = "01a0ef11-0212-70de-a8b3-99de94270000";
const SOURCE: &str =
    "nika: parent\npermits: {}\ntasks:\n  call:\n    invoke: { workflow: \"./child.nika\" }\n";

/// The `child` row the producer writes (compact JSON) for `trace`.
fn row(trace: &str, outcome: &str) -> String {
    serde_json::json!({
        "target": "./child.nika",
        "trace_id": trace,
        "chain_head": "ab".repeat(32),
        "def_hash": "cd".repeat(32),
        "outcome": outcome,
    })
    .to_string()
}

fn child(json: &str) -> String {
    field("child", &quoted(json))
}

/// `call` ran in `exec` and settled with `extra` on its settle frame.
fn called(exec: &str, extra: &[String]) -> Vec<crate::session::feed::Observed> {
    vec![
        task(exec, 2, "task_scheduled", "call", &[]),
        task(exec, 3, "task_started", "call", &[]),
        task(exec, 4, "task_completed", "call", extra),
    ]
}

fn parent(extra: &[String]) -> Vec<crate::session::feed::Observed> {
    let mut seen = vec![asked(None, false), start(EXEC, SOURCE)];
    seen.extend(called(EXEC, extra));
    seen.push(settled(EXEC, "succeeded"));
    seen
}

#[test]
fn the_settle_frame_names_a_child_whose_journal_opens_from_the_detail() {
    let mut desk = desk(parent(&[child(&row("child-0001.ndjson", "success"))]));
    assert_eq!(desk.route(key(KeyCode::Enter), WIDE), Route::Repaint);
    let (title, detail) = object(&mut desk, WIDE, false);
    assert!(title.contains("task call"), "{title}");
    let text = detail.join("\n");
    assert!(text.contains("child run · ./child.nika"), "{text}");
    assert!(text.contains("child-0001.ndjson"), "{text}");
    assert!(text.contains("abababababab"), "the head it named: {text}");
    assert!(text.contains("cdcdcdcdcdcd"), "the source it named: {text}");
    assert!(text.contains("Enter: open its journal"), "{text}");
    assert!(desk.wanted().is_none(), "nothing is read before Enter");
    assert_eq!(desk.route(key(KeyCode::Enter), WIDE), Route::Inspect);
    assert!(desk.wanted().is_some(), "the child's journal is to be read");
    let (title, _) = object(&mut desk, WIDE, false);
    assert!(
        title.contains("child ./child.nika") && title.contains("Backspace"),
        "{title}"
    );
    assert_eq!(desk.route(key(KeyCode::Backspace), WIDE), Route::Repaint);
    let (title, _) = object(&mut desk, WIDE, false);
    assert!(
        title.contains("task call") && !title.contains("child"),
        "{title}"
    );
    assert!(desk.wanted().is_none(), "the parent's detail reads nothing");
    assert_eq!(desk.route(key(KeyCode::Backspace), WIDE), Route::Repaint);
    let (title, _) = object(&mut desk, WIDE, false);
    assert!(!title.contains("task call"), "{title}");
}

#[test]
fn a_child_row_anywhere_but_an_admissible_settle_opens_nothing() {
    let mut started = vec![asked(None, false), start(EXEC, SOURCE)];
    started.push(task(EXEC, 2, "task_scheduled", "call", &[]));
    started.push(task(
        EXEC,
        3,
        "task_started",
        "call",
        &[child(&row("child-0001.ndjson", "success"))],
    ));
    started.push(task(EXEC, 4, "task_completed", "call", &[]));
    let mut foreign = vec![asked(None, false), start(EXEC, SOURCE)];
    foreign.extend(called(EXEC, &[]));
    foreign.push(task(
        OTHER,
        5,
        "task_completed",
        "call",
        &[child(&row("child-0001.ndjson", "success"))],
    ));
    let mut late = vec![asked(None, false), start(EXEC, SOURCE)];
    late.extend(called(EXEC, &[]));
    late.push(settled(EXEC, "succeeded"));
    late.push(task(
        EXEC,
        6,
        "task_completed",
        "call",
        &[child(&row("child-0001.ndjson", "success"))],
    ));
    let malformed = parent(&[child("{\"target\": ")]);
    for (case, seen) in [
        ("on a start", started),
        ("of another run", foreign),
        ("after the settlement", late),
        ("malformed", malformed),
    ] {
        let mut desk = desk(seen);
        assert_eq!(
            desk.route(key(KeyCode::Enter), WIDE),
            Route::Repaint,
            "{case}"
        );
        let (_, detail) = object(&mut desk, WIDE, false);
        let text = detail.join("\n");
        assert!(!text.contains("open its journal"), "{case}: {text}");
        assert!(!text.contains("child-0001"), "{case}: {text}");
        assert_eq!(
            desk.route(key(KeyCode::Enter), WIDE),
            Route::Nothing,
            "{case}"
        );
        assert!(desk.wanted().is_none(), "{case}");
    }
}

#[test]
fn an_unrecognised_outcome_or_no_journal_is_said_and_opens_nothing() {
    for (json, said) in [
        (row("child-0001.ndjson", "maybe"), "outcome not recognized"),
        (
            serde_json::json!({"target": "./child.nika", "outcome": "success"}).to_string(),
            "names no journal",
        ),
    ] {
        let mut desk = desk(parent(&[child(&json)]));
        desk.route(key(KeyCode::Enter), WIDE);
        let (_, detail) = object(&mut desk, WIDE, false);
        let text = detail.join("\n");
        assert!(text.contains("child run · ./child.nika"), "{text}");
        assert!(text.contains(said), "{said}: {text}");
        assert!(!text.contains("Enter: open its journal"), "{text}");
        assert_eq!(desk.route(key(KeyCode::Enter), WIDE), Route::Nothing);
        assert!(desk.wanted().is_none());
    }
}

#[test]
fn a_new_attempt_replaces_the_relation_its_settle_named() {
    let mut seen = vec![asked(None, false), start(EXEC, SOURCE)];
    seen.extend(called(EXEC, &[child(&row("child-0001.ndjson", "success"))]));
    seen.push(task(EXEC, 5, "task_started", "call", &[]));
    seen.push(task(
        EXEC,
        6,
        "task_completed",
        "call",
        &[child(&row("child-0002.ndjson", "success"))],
    ));
    let mut settled_twice = desk(seen);
    settled_twice.route(key(KeyCode::Enter), WIDE);
    let (_, detail) = object(&mut settled_twice, WIDE, false);
    let text = detail.join("\n");
    assert!(text.contains("child-0002.ndjson"), "{text}");
    assert!(
        !text.contains("child-0001"),
        "the earlier attempt's relation is gone: {text}"
    );
    let mut retried = vec![asked(None, false), start(EXEC, SOURCE)];
    retried.extend(called(EXEC, &[child(&row("child-0001.ndjson", "success"))]));
    retried.push(task(EXEC, 5, "task_started", "call", &[]));
    let mut running = desk(retried);
    running.route(key(KeyCode::Enter), WIDE);
    let (_, detail) = object(&mut running, WIDE, false);
    let text = detail.join("\n");
    assert!(
        !text.contains("child-0001"),
        "a new attempt drops it: {text}"
    );
}

/// A conversation that counts every call the navigation could cause and
/// answers a child read with `said` (a refusal carrying those words).
#[derive(Default)]
struct Counter {
    submitted: usize,
    performed: usize,
    fetched: usize,
    proved: usize,
    children: usize,
}

impl crate::model::Conversation for Counter {
    fn open(&mut self) -> Vec<crate::model::Beat> {
        Vec::new()
    }
    fn submit(&mut self, _line: &str) -> crate::model::Turn {
        self.submitted += 1;
        crate::model::Turn {
            beats: Vec::new(),
            handoff: None,
        }
    }
    fn perform(&mut self, _handoff: &crate::model::Handoff) -> Vec<crate::model::Beat> {
        self.performed += 1;
        Vec::new()
    }
    fn fetch(&mut self, _execution: &ExecutionId, path: &str) -> Option<Fetched> {
        self.fetched += 1;
        Some(Fetched::refused(path, "counted"))
    }
    fn prove(&mut self, _execution: &ExecutionId) -> Option<Proven> {
        self.proved += 1;
        Some(Proven::refused("", "counted"))
    }
    fn child(
        &mut self,
        _execution: &ExecutionId,
        _task: &str,
        relation: &ChildRun,
    ) -> Option<ChildRead> {
        self.children += 1;
        let trace = relation.trace_id.clone().unwrap_or_default();
        Some(ChildRead::refused(
            &trace,
            format!("read number {}", self.children),
        ))
    }
}

/// Open the child of `call` from the list: Enter on the task, Enter on its
/// detail; what the desk then wants read.
fn open_child(
    desk: &mut Desk,
) -> (
    ExecutionId,
    Vec<crate::workspace::live::Want>,
    crate::workspace::desk::Reading,
) {
    assert_eq!(desk.route(key(KeyCode::Enter), WIDE), Route::Repaint);
    assert_eq!(desk.route(key(KeyCode::Enter), WIDE), Route::Inspect);
    desk.wanted().expect("the child's journal is wanted")
}

/// The body of the object in view, joined.
fn shown(desk: &mut Desk) -> String {
    object(desk, WIDE, false).1.join("\n")
}

/// A read asked for a relation is applied only while that relation is the
/// task's own since the view opened: one that left and came back equal
/// (A, then a new attempt, then A again) sets the answer aside, and so does
/// a relation that is gone. Nothing is asked again for a stale view.
#[test]
fn a_read_for_a_relation_that_changed_meanwhile_is_set_aside() {
    let a = row("child-0001.ndjson", "success");
    for (case, later) in [
        (
            "A, a new attempt, A again",
            vec![
                task(EXEC, 5, "task_started", "call", &[]),
                task(EXEC, 6, "task_completed", "call", &[child(&a)]),
            ],
        ),
        (
            "A, then gone",
            vec![task(EXEC, 5, "task_started", "call", &[])],
        ),
    ] {
        let mut seen = vec![asked(None, false), start(EXEC, SOURCE)];
        seen.extend(called(EXEC, &[child(&a)]));
        let mut desk = desk(seen);
        let (execution, wants, generation) = open_child(&mut desk);
        let mut counter = Counter::default();
        let late = acquire_all(&mut counter, &execution, wants);
        desk.observe(later.into_iter());
        assert!(
            desk.acquired(execution, generation, late),
            "{case}: the leg is the same"
        );
        let text = shown(&mut desk);
        assert!(!text.contains("read number 1"), "{case}: set aside: {text}");
        assert!(text.contains("relation changed"), "{case}: {text}");
        assert!(desk.wanted().is_none(), "{case}: nothing asked again");
        assert_eq!(desk.route(key(KeyCode::Backspace), WIDE), Route::Repaint);
        let (title, _) = object(&mut desk, WIDE, false);
        assert!(
            title.contains("task call") && !title.contains("child"),
            "{case}: {title}"
        );
    }
}

/// While the child is in view the parent stays the run followed: its
/// frames keep folding, its spend is its own (never the child's added),
/// and leaving the child calls nothing; navigation submits, performs,
/// fetches and proves nothing, and reads the child once per opening.
#[test]
fn the_parent_keeps_folding_and_navigation_calls_nothing() {
    let mut seen = vec![asked(None, false), start(EXEC, SOURCE)];
    seen.extend(called(
        EXEC,
        &[
            child(&row("child-0001.ndjson", "success")),
            field("cost_usd", "0.25"),
        ],
    ));
    let mut desk = desk(seen);
    let mut counter = Counter::default();
    let (execution, wants, generation) = open_child(&mut desk);
    let got = acquire_all(&mut counter, &execution, wants);
    assert!(desk.acquired(execution, generation, got));
    assert!(shown(&mut desk).contains("read number 1"));
    assert!(desk.wanted().is_none(), "read once");
    desk.observe(
        [
            task(EXEC, 7, "task_started", "after", &[]),
            task(
                EXEC,
                8,
                "task_completed",
                "after",
                &[field("cost_usd", "0.5")],
            ),
        ]
        .into_iter(),
    );
    let leg = desk.live.as_ref().expect("the parent");
    assert_eq!(leg.task("after"), Some(nika_display::state::TaskState::Ok));
    assert!(
        (leg.view.cost_usd - 0.75).abs() < 1e-9,
        "{}",
        leg.view.cost_usd
    );
    assert!(
        shown(&mut desk).contains("read number 1"),
        "the child stays in view"
    );
    assert_eq!(desk.route(key(KeyCode::Backspace), WIDE), Route::Repaint);
    assert_eq!(desk.route(key(KeyCode::Backspace), WIDE), Route::Repaint);
    let (_, body) = object(&mut desk, WIDE, false);
    assert!(body.iter().any(|r| r.contains("✔ after")), "{body:#?}");
    assert_eq!(
        (
            counter.submitted,
            counter.performed,
            counter.fetched,
            counter.proved,
            counter.children
        ),
        (0, 0, 0, 0, 1)
    );
    // Opened again: read again, once.
    let (execution, wants, generation) = open_child(&mut desk);
    let got = acquire_all(&mut counter, &execution, wants);
    assert!(desk.acquired(execution, generation, got));
    assert_eq!(counter.children, 2);
    assert!(shown(&mut desk).contains("read number 2"));
}

/// `r` in the child view reads it again: a reading asked before it arrives
/// late and is dropped; the new one is applied.
#[test]
fn reading_the_child_again_drops_the_earlier_answer() {
    let mut desk = desk(parent(&[child(&row("child-0001.ndjson", "success"))]));
    let mut counter = Counter::default();
    let (execution, wants, stale) = open_child(&mut desk);
    let late = acquire_all(&mut counter, &execution, wants);
    assert_eq!(desk.route(key(KeyCode::Char('r')), WIDE), Route::Inspect);
    assert!(
        !desk.acquired(execution, stale, late),
        "a late reading is dropped"
    );
    let (execution, wants, generation) = desk.wanted().expect("asked again");
    let got = acquire_all(&mut counter, &execution, wants);
    assert!(desk.acquired(execution, generation, got));
    let text = shown(&mut desk);
    assert!(
        text.contains("read number 2") && !text.contains("read number 1"),
        "{text}"
    );
}
