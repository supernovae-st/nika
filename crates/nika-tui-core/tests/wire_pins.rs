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
