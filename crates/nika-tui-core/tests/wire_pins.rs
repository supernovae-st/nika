#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! The spellings this crate COPIES are pinned to their owner (ADR-130).
//!
//! `nika-tui-core` reads the flight recorder's NDJSON without depending
//! on `nika-event` (the wasm build stays dependency-light), so it carries
//! the frame kinds it folds as string literals. A copy that nothing checks
//! drifts the day the owner renames; this test is the check — a
//! dev-dependency only, never a runtime edge.

use nika_event::EventKind;
use nika_tui_core::ingress::run_from_journal;

/// The kinds the journal fold reads, as this crate spells them, beside the
/// owner's spelling. A rename on either side fails here first.
const COPIED_KINDS: &[(&str, EventKind)] = &[
    ("workflow_started", EventKind::WorkflowStarted),
    ("task_completed", EventKind::TaskCompleted),
    ("task_failed", EventKind::TaskFailed),
    ("task_cancelled", EventKind::TaskCancelled),
    ("task_skipped", EventKind::TaskSkipped),
];

#[test]
fn every_copied_kind_is_the_owners_spelling() {
    for (copied, owner) in COPIED_KINDS {
        assert_eq!(*copied, owner.as_str(), "{copied} drifted from nika-event");
    }
}

/// The fold reads exactly the kinds it copies: a journal whose terminal
/// task frames carry the owner's spellings folds every step, and one
/// spelled differently folds none — the pin is load-bearing, not
/// decorative.
#[test]
fn the_fold_reads_the_owners_spellings_and_nothing_else() {
    let owner = format!(
        "{{\"kind\":\"{}\",\"id\":{{\"uuid\":\"t\"}},\"timestamp\":1000000,\"fields\":[]}}\n\
         {{\"kind\":\"{}\",\"timestamp\":2000000,\"fields\":[{{\"key\":\"task\",\"value\":\"a\"}},{{\"key\":\"duration_ms\",\"value\":1}}]}}\n",
        EventKind::WorkflowStarted.as_str(),
        EventKind::TaskCompleted.as_str(),
    );
    let run = run_from_journal(&owner).expect("the owner's journal folds");
    assert_eq!(run.steps.len(), 1, "one terminal task frame → one step");

    let stranger = owner.replace("task_completed", "task_done");
    let run = run_from_journal(&stranger).expect("still a journal");
    assert!(
        run.steps.is_empty(),
        "an unknown spelling folds no step: the fold reads the copied kinds only"
    );
}

/// The graph envelope this crate reads is the engine's own number: a bump
/// on either side fails here first (the 2026-09-04 audit found the crate
/// reading format 2 while the engine emitted 3 — a copy nothing pinned).
#[test]
fn the_graph_format_is_the_engines() {
    assert_eq!(
        nika_tui_core::ingress::GRAPH_FORMAT,
        nika_graph::GRAPH_FORMAT,
        "nika-tui-core reads another graph format than nika-graph emits"
    );
}

/// A cleanup unit (`kind: "finally"`) is a node on the map, never a slot on
/// the board — the reason format 3 exists (a reader that did not know
/// `kind` seated it as a task).
#[test]
fn a_cleanup_unit_is_never_a_slot() {
    let doc: nika_tui_core::ingress::GraphDoc = serde_json::from_value(serde_json::json!({
        "graph_format": nika_graph::GRAPH_FORMAT,
        "workflow": "t",
        "nodes": [
            {"id": "a", "verb": "exec"},
            {"id": "cleanup", "verb": "exec", "kind": "finally"}
        ],
        "edges": []
    }))
    .expect("a format-3 document");
    let board = nika_tui_core::plan::seat_first(&doc);
    assert_eq!(board.slots, vec![Some("a".to_owned())]);
    assert_eq!(doc.tasks().count(), 1);
}

/// The published law: a reader refuses a graph format it does not speak,
/// naming both numbers — never a silent misread.
#[test]
fn a_reader_refuses_a_format_it_does_not_speak() {
    let stale = r#"{"graph_format":2,"workflow":"t","nodes":[],"edges":[]}"#;
    let out = nika_tui_core::wasm::board_first(stale);
    let v: serde_json::Value = serde_json::from_str(&out).expect("json");
    let error = v["error"].as_str().expect("a refusal");
    assert!(
        error.contains("graph_format 2 is not 3"),
        "the numbers are named: {error}"
    );
    let spoken = format!(
        r#"{{"graph_format":{},"workflow":"t","nodes":[],"edges":[]}}"#,
        nika_graph::GRAPH_FORMAT
    );
    let ok: serde_json::Value =
        serde_json::from_str(&nika_tui_core::wasm::board_first(&spoken)).expect("json");
    assert!(ok.get("error").is_none(), "{ok}");
}
