// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use crate::compile::{CompileRequest, compile};

/// An outcome the deterministic core really produced, with a decision record to stamp into.
fn outcome() -> CompileOutcome {
    let mut out = compile(&CompileRequest::create("chain")).expect("compiles");
    out.provenance.decision.get_or_insert_with(|| json!({}));
    out
}

#[test]
fn only_the_native_doors_calls_read_knowledge() {
    for call in [
        "native",
        "native-repair",
        "sketch",
        "sketch-repair",
        "fill",
        "fill-repair",
    ] {
        assert!(reads_knowledge(call), "{call}");
    }
    for call in ["plan", "repair", "transform", "natives"] {
        assert!(!reads_knowledge(call), "{call}");
    }
}

#[test]
fn a_carried_record_says_it_was_carried_and_keeps_the_rest() {
    let kept = json!({"pack_sha256": "abc", "presented": true});
    let carried = carried_record(&kept);
    assert_eq!(carried["carried"], true);
    assert_eq!(carried["pack_sha256"], "abc");
    assert_eq!(carried["presented"], true);
    // A value that is not a record is carried unchanged, never invented into one.
    assert_eq!(carried_record(&json!("x")), json!("x"));
}

#[test]
fn only_a_presented_pack_is_what_an_answer_round_carries() {
    let mut out = outcome();
    out.provenance.decision = None;
    assert_eq!(
        presented_knowledge(&out),
        None,
        "no record: nothing to carry"
    );
    for (presented, expect) in [(true, true), (false, false)] {
        out.provenance.decision = Some(json!({"session": {"authoring": {"knowledge": {
            "presented": presented,
            "pack_sha256": "abc",
        }}}}));
        assert_eq!(presented_knowledge(&out).is_some(), expect, "{presented}");
    }
}

#[test]
fn the_stamp_names_the_strategy_its_source_and_keeps_the_decision_seat() {
    let mut out = outcome();
    out.provenance.decision =
        Some(json!({"session": {"decision_seat": {"model": "m"}}, "kept": 1}));
    let knowledge = json!({"pack_sha256": "abc"});
    stamp(&mut out, "escalate", "environment", Some(&knowledge));
    let decision = out.provenance.decision.expect("stamped");
    assert_eq!(decision["session"]["authoring"]["strategy"], "escalate");
    assert_eq!(decision["session"]["authoring"]["source"], "environment");
    assert_eq!(
        decision["session"]["authoring"]["knowledge"]["pack_sha256"],
        "abc"
    );
    assert_eq!(decision["session"]["decision_seat"]["model"], "m");
    assert_eq!(
        decision["kept"], 1,
        "the compiler's own record stays beside the stamp"
    );
    let mut bare = outcome();
    bare.provenance.decision = None;
    stamp(&mut bare, "off", "default", None);
    let decision = bare.provenance.decision.expect("created");
    assert!(decision["session"]["authoring"]["knowledge"].is_null());
    assert!(decision["session"].get("decision_seat").is_none());
}

#[test]
fn the_observed_record_counts_columns_and_never_carries_their_names() {
    let world = json!({"observed": [
        {"path": "data/orders.csv", "state": "present", "kind": "csv",
         "columns": ["customer", "amount", "status"]},
        {"path": "notes.md", "state": "absent", "kind": null},
    ]});
    let out = observed_in(outcome(), Some(&world));
    let record = &out.provenance.decision.expect("decision")["session"]["observed"];
    assert_eq!(record["attached"], true);
    assert_eq!(record["presented"], false, "no native call read a pack");
    assert_eq!(record["rows"][0]["columns"], 3);
    assert_eq!(record["rows"][1]["columns"], 0);
    assert!(!record.to_string().contains("customer"), "{record}");
    // C10: the identity of the whole world attached, beside the names-free summary.
    assert_eq!(record["world_sha256"], json!(world_sha256(&world)));
    // Nothing observed: the outcome's record is untouched.
    let untouched = outcome();
    let before = untouched.provenance.decision.clone();
    assert_eq!(observed_in(untouched, None).provenance.decision, before);
}

#[test]
fn a_pin_names_its_identity_in_words_and_refuses_what_is_no_snapshot() {
    let words = KnowledgePin::words(Some("v1"), None, &"a".repeat(64), &"b".repeat(64));
    assert_eq!(
        words,
        "v1 (declared digest none · manifest aaaaaaaaaaaa · rows bbbbbbbbbbbb)"
    );
    assert_eq!(short("0123456789abcdef"), "0123456789ab");
    let dir = tempfile::tempdir().expect("an empty directory");
    let refused = KnowledgePin::open(dir.path().to_path_buf(), None);
    assert!(
        refused.is_err(),
        "a directory that is no snapshot is never pinned"
    );
}

/// C10 · D-K · a pin names both identities when the snapshot moved under it: rows it did not
/// read, a re-declared digest; the same snapshot has not moved.
#[test]
fn a_pin_says_when_its_snapshot_moved_under_it() {
    let dir = tempfile::tempdir().expect("a snapshot");
    let manifest = |digest: &str| {
        json!({"knowledge_version": "k1", "digest": digest, "files": {}}).to_string()
    };
    std::fs::write(dir.path().join("manifest.json"), manifest("d1")).expect("manifest");
    let pin = KnowledgePin::open(dir.path().to_path_buf(), None).expect("pinned");
    let now = || Snapshot::open(dir.path()).expect("still a snapshot");
    assert_eq!(pin.moved(&now()), None, "the same snapshot has not moved");
    std::fs::write(dir.path().join("families.jsonl"), "{}\n").expect("a row file");
    let (pinned, found) = pin.moved(&now()).expect("rows it did not read moved it");
    assert!(pinned.starts_with("k1 (declared digest d1"), "{pinned}");
    assert!(found.starts_with("k1 (declared digest d1"), "{found}");
    assert_ne!(pinned, found, "the rows differ in words");
    std::fs::remove_file(dir.path().join("families.jsonl")).expect("rows as pinned");
    std::fs::write(dir.path().join("manifest.json"), manifest("d2")).expect("re-declared");
    let (_, found) = pin.moved(&now()).expect("a re-declared digest moved it");
    assert!(found.contains("declared digest d2"), "{found}");
}

/// C10 · the decision seat's receipt sits beside the compiler's own record of the same
/// questions, created when there is none, and the session's later stamp keeps it.
#[test]
fn the_seat_receipt_is_stamped_beside_the_record_and_kept_by_the_stamp() {
    let mut out = outcome();
    out.provenance.decision = Some(json!({"kept": 1}));
    stamp_seat(&mut out, json!({"model": "typesafe/jev"}));
    stamp(&mut out, "escalate", "environment", None);
    let decision = out.provenance.decision.expect("stamped");
    assert_eq!(
        decision["session"]["decision_seat"]["model"],
        "typesafe/jev"
    );
    assert_eq!(decision["session"]["authoring"]["strategy"], "escalate");
    assert_eq!(decision["kept"], 1, "the compiler's own record stays");
    let mut bare = outcome();
    bare.provenance.decision = None;
    stamp_seat(&mut bare, json!({"model": "m"}));
    let created = bare.provenance.decision.expect("created");
    assert_eq!(created["session"]["decision_seat"]["model"], "m");
}
