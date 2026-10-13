// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Presentation facts never manufacture a continuation or authorize its effects.

use super::*;
use crate::change::Witness;
use crate::runtime::tests::ready_with;
use crate::work::Waiting;
use serde_json::json;

fn round() -> AuthoringRound {
    let mut round = AuthoringRound::new("Copy the paid rows from sales.csv to paid.csv.");
    round.answers.insert("currency".into(), "\"EUR\"".into());
    round.continuation =
        Some(json!({"candidate": "kept exact bytes", "obligations": ["paid only"]}));
    round
}

#[test]
fn reading_the_waiting_judgment_keeps_the_exact_round_without_proposing_or_running() {
    let dir = tempfile::tempdir().expect("project");
    let mut session = ready_with(dir.path(), vec![]);
    assert_eq!(session.waiting_judgment(), None);
    let held = round();
    session.authoring = Some(held.clone());
    let identity = session.waiting_judgment().expect("kept continuation");
    for line in ["", "yes", "save", "run"] {
        assert_eq!(session.waiting_judgment().as_ref(), Some(&identity));
        assert!(matches!(
            session.submit(line, &Waiting::Free),
            TurnOutcome::Facts(_)
        ));
        assert_eq!(session.authoring, Some(held.clone()));
        assert_eq!(session.waiting(), Waiting::Free);
        assert_eq!(session.pending_proposal(), None);
        assert_eq!(session.work().requested, None);
        assert_eq!(session.work().run, None);
    }
    assert_eq!(std::fs::read_dir(dir.path()).expect("project").count(), 0);
}

#[test]
fn a_waiting_judgment_identity_changes_with_the_exact_round_and_selected_context() {
    let dir = tempfile::tempdir().expect("project");
    let mut session = ready_with(dir.path(), vec![]);
    let held = round();
    session.authoring = Some(held.clone());
    let original = session.waiting_judgment().expect("identity");
    for field in [
        "intent",
        "answer",
        "record",
        "edit",
        "target",
        "money",
        "knowledge",
    ] {
        let mut changed = held.clone();
        match field {
            "intent" => changed.intent.push_str(" Preserve order."),
            "answer" => {
                changed.answers.insert("currency".into(), "\"USD\"".into());
            }
            "record" => {
                changed.continuation =
                    Some(json!({"candidate": "replacement bytes", "obligations": ["paid only"]}));
            }
            "edit" => changed.edit = Some(("base bytes".into(), "keep paid rows".into(), None)),
            "target" => changed.target = Some(("paid.nika".into(), Witness::of(b"base bytes"))),
            "money" => changed.money.push(0..4),
            "knowledge" => changed.knowledge = Some(json!({"manifest": "replacement knowledge"})),
            _ => panic!("unknown fixture"),
        }
        session.authoring = Some(changed);
        assert_ne!(
            session.waiting_judgment().expect(field),
            original,
            "{field}"
        );
    }
    session.authoring = Some(held);
    assert_eq!(session.waiting_judgment().as_ref(), Some(&original));
    session.intelligence.model = Some("mock/another".into());
    assert_ne!(
        session.waiting_judgment().expect("new selected model"),
        original
    );
}

#[test]
fn another_prompt_or_a_round_without_continuation_has_no_retry_presentation() {
    let dir = tempfile::tempdir().expect("project");
    let mut session = ready_with(dir.path(), vec![]);
    session.authoring = Some(round());
    let original = session.waiting_judgment().expect("identity");
    session.pending_choice = true;
    assert_eq!(session.waiting_judgment(), None);
    session.pending_choice = false;
    let _ = session.run_review_asked("Run once?", "exact cost evidence");
    assert_eq!(session.waiting_judgment(), None);
    let _ = session.decline_run_review();
    assert_eq!(session.waiting_judgment().as_ref(), Some(&original));
    let request = nika_onboard::compile::CompileRequest::create(
        "Read ./notes/brief.md, draft a 3-bullet summary of it and write the summary to ./out/summary.md",
    );
    let questions = nika_onboard::compile::compile(&request)
        .expect("compile fixture")
        .questions;
    assert_eq!(questions[0].key, "model");
    session.authoring.as_mut().expect("round").questions = questions;
    assert_eq!(session.waiting_judgment(), None);
    let held = session.authoring.as_mut().expect("round");
    held.questions.clear();
    held.continuation = None;
    assert_eq!(session.waiting_judgment(), None);
    session.authoring = Some(round());
    let discarded = session.submit("cancel", &Waiting::Free);
    assert!(
        matches!(discarded, TurnOutcome::Facts(ref text) if text == "The unjudged candidate was discarded; your request is kept.")
    );
    assert_eq!(session.waiting_judgment(), None);
    assert_eq!(session.pending_proposal(), None);
    assert_eq!(session.work().requested, None);
}
