// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use crate::change::PendingGate;
use crate::runtime::tests::ready_with;
use serde_json::json;

fn paused(options: &serde_json::Value) -> String {
    json!({"kind":"workflow_paused","fields":[
        {"key":"task","value":"pick"}, {"key":"mode","value":"choice"},
        {"key":"message","value":"Choose exactly one"},
        {"key":"choices","value":options.to_string()}
    ]})
    .to_string()
}

#[test]
fn native_gate_options_and_witness_come_from_the_same_exact_bytes() {
    let dir = tempfile::tempdir().expect("root");
    let trace = dir.path().join("paused.ndjson");
    let raw = paused(&json!(["yes", "true", "1", "/quit", "  spaced  "]));
    std::fs::write(&trace, &raw).expect("trace");
    let mut session = ready_with(dir.path(), vec![]);
    session.pending_gate = PendingGate::from_trace(std::path::Path::new("gate.nika"), &trace);
    let before = session.pending_gate.clone();
    let (id, choices, digest) = session.waiting_gate_choices().expect("typed choices");
    assert_eq!(id, session.waiting_gate().expect("gate"));
    assert_eq!(choices, ["yes", "true", "1", "/quit", "  spaced  "]);
    assert_eq!(digest, nika_event::source_id::sha256_hex(raw.as_bytes()));
    assert_eq!(session.pending_gate, before, "projection never answers");
    let changed = format!("{raw}\n");
    std::fs::write(&trace, &changed).expect("trace changed");
    let (_, same, new_digest) = session.waiting_gate_choices().expect("same options");
    assert_eq!(same, choices);
    assert_ne!(
        new_digest, digest,
        "even a change outside options invalidates the presentation"
    );
    std::fs::write(
        &trace,
        raw.replace("Choose exactly one", "Different question"),
    )
    .expect("changed question");
    assert_eq!(session.waiting_gate_choices(), None);
}

#[test]
fn native_gate_options_never_guess_missing_malformed_or_secret_values() {
    let dir = tempfile::tempdir().expect("root");
    let trace = dir.path().join("paused.ndjson");
    let mut session = ready_with(dir.path(), vec![]);
    for options in [
        json!([]),
        json!(["a", 1]),
        json!(["\u{f8ff}nika:secret:key:env:KEY\u{f8ff}", "decline"]),
    ] {
        std::fs::write(&trace, paused(&options)).expect("trace");
        session.pending_gate = PendingGate::from_trace(std::path::Path::new("gate.nika"), &trace);
        assert_eq!(session.waiting_gate_choices(), None);
        assert!(
            session.waiting_gate().is_some(),
            "the original gate remains pending"
        );
    }
    std::fs::remove_file(&trace).expect("remove fixture trace");
    assert_eq!(session.waiting_gate_choices(), None);
}

#[test]
fn the_gate_prompt_preserves_exact_words_and_names_the_pending_gate_without_answering() {
    let dir = tempfile::tempdir().expect("root");
    let trace = dir.path().join("paused.ndjson");
    std::fs::write(&trace, paused(&json!(["a", "b"]))).expect("trace");
    let mut session = ready_with(dir.path(), vec![]);
    assert_eq!(session.waiting_gate_prompt(), None);
    for mode in ["choice", "confirm"] {
        session.pending_gate = PendingGate::from_trace(std::path::Path::new("gate.nika"), &trace);
        let gate = session.pending_gate.as_mut().expect("gate");
        gate.mode = mode.into();
        gate.message = "  Exact prompt?\nSecond line  ".into();
        let before = session.pending_gate.clone();
        assert_eq!(
            session.waiting_gate_prompt(),
            Some((
                session.waiting_gate().expect("id"),
                "  Exact prompt?\nSecond line  ".into()
            ))
        );
        assert_eq!(session.pending_gate, before);
        assert!(session.answered.is_none());
    }
}

mod expiry;
