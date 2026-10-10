// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use nika_session_agent::Store;
use serde_json::json;

use super::store::TREE_FILE;
use super::*;

/// A conversation's tree kept beside the history is read back whole, its instructions and the
/// person's citations with it; a damaged one is reported, never reset or replaced.
#[test]
fn a_kept_tree_is_read_back_and_a_damaged_one_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let held = || OwnedDir::open(dir.path());
    let mut store = TreeFile::home(held().unwrap());
    let mut tree = start_tree(&mut store, dir.path(), "vllm/oux-author").unwrap();
    let said = tree.append_user("un digest des news", None, None, 2, |line| {
        Store::append(&mut store, line)
    });
    assert_eq!(said.unwrap().1, "u1");

    let driver = Driver::resumed(held(), None);
    assert!(driver.damaged.is_none());
    let resumed = driver.tree.as_ref().unwrap();
    assert_eq!(resumed.next_cite(), "u2");
    let system = resumed.context().system.unwrap();
    assert!(
        system.starts_with(SYSTEM) && system.contains("vllm/oux-author"),
        "{system}"
    );
    let citations = driver.store.citations();
    let cited = citations.lock().unwrap().get("u1").map(|c| c.text.clone());
    assert_eq!(cited.as_deref(), Some("un digest des news"));

    let kept = std::fs::read_to_string(dir.path().join(TREE_FILE)).unwrap();
    std::fs::write(dir.path().join(TREE_FILE), format!("{kept}not a line\n")).unwrap();
    let damaged = Driver::resumed(held(), None);
    assert!(damaged.tree.is_none() && damaged.damaged.is_some());
    assert!(
        std::fs::read_to_string(dir.path().join(TREE_FILE))
            .unwrap()
            .ends_with("not a line\n")
    );
}

/// What a history kept of a conversation resumes as evidence: its values and their provenance,
/// never a question; a value this engine does not read resumes nothing.
#[test]
fn kept_evidence_resumes_without_a_question() {
    let dir = tempfile::tempdir().unwrap();
    let binding = json!({"role": "read_source", "value": "https://news.ycombinator.com",
        "provenance": {"kind": "offered", "message": "u2", "question": "plan", "option": "recommended"}});
    let kept = json!({"version": 1, "request": 0, "since": 0, "candidate": null,
        "bindings": [binding], "accepted": [binding], "delegations": []});
    let driver = Driver::resumed(OwnedDir::open(dir.path()), Some(&kept));
    assert_eq!(driver.conversation.bindings().len(), 1);
    assert!(driver.open().is_empty() && driver.tree.is_none());
    let unread = Driver::resumed(OwnedDir::open(dir.path()), Some(&json!({"version": 7})));
    assert!(unread.conversation.bindings().is_empty());
}
