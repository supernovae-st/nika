// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The operator-selected decision seat through a whole Session turn: the API seat's no-budget
//! observation, the `Jev` seat's WARM decision through the real shared adapter on loopback, the
//! line on disk before the `Jev` request could leave, the persisted journal and the status words.
//! Loopback mechanics only; no provider qualification.
use super::*;
use crate::authoring::decision::tests::{KEY, Peer as SystemOne, Reply, SEAT, TICKETS, answer};
use crate::authoring::{AuthoringContext, DECISION_SCHEMA, DecisionSetup};
use crate::runtime::inference::OBSERVED_PREFIX;
use std::sync::{Arc, Mutex};

fn seated(root: &Path, base: &str) -> SessionRuntime {
    std::fs::write(
        root.join("tickets.json"),
        r#"[{"id": "41", "title": "synthetic one"}, {"id": "42", "title": "synthetic two"}]"#,
    )
    .expect("fixture");
    let mut s = open(root);
    // The default strategy (escalate): the compiler's own ladder, WARM before any generation.
    s.set_authoring_context(AuthoringContext::default().with_decision(Some(
        DecisionSetup::with_key(SEAT, Some(KEY.to_owned()), Some(base)),
    )));
    s
}

#[test]
fn a_turn_consults_the_selected_seat_once_and_persists_its_observation() {
    let deepseek = Peer::start(vec![(200, response("unused"))]);
    let _transport = test_transport::install(&deepseek.url);
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let on_disk = Arc::new(Mutex::new(None));
    let seen = Arc::clone(&on_disk);
    let jev = SystemOne::start_with(vec![Reply::Json(200, answer("lookup"))], move || {
        // While the Jev request is in flight, the record already says it may have been sent.
        let state = crate::SessionState::load(&root).ok().flatten();
        *seen.lock().unwrap() = state.map(|s| {
            s.decisions
                .iter()
                .any(|d| d.starts_with(OBSERVED_PREFIX) && d.contains(SEAT))
        });
    });
    let mut s = seated(dir.path(), &jev.base);
    let out = s.turn(TICKETS);
    assert!(
        matches!(&out, TurnOutcome::Question { key, .. } if key == "const.ticket_id_field"),
        "the decision settled the clause; only the id field is asked: {out:?}"
    );
    assert_eq!(jev.requests().len(), 1, "one decision call");
    assert!(
        deepseek.bodies().is_empty(),
        "WARM settled it: no generative call"
    );
    assert_eq!(
        *on_disk.lock().unwrap(),
        Some(true),
        "the line naming the seat was on disk before its request"
    );
    let kept = crate::SessionState::load(dir.path()).unwrap().unwrap();
    assert!(
        kept.decisions
            .iter()
            .all(|d| !d.starts_with(OBSERVED_PREFIX)),
        "settled"
    );
    let o = kept
        .inference_observations
        .iter()
        .find(|o| o["schema"] == DECISION_SCHEMA)
        .expect("the decision seat's observation is persisted");
    assert_eq!(o["calls_sent"], 1);
    assert_eq!(o["unbudgeted"], true);
    assert_eq!(o["attempts"][0]["outcome"], "chosen");
    assert!(o["cost"].as_str().unwrap().starts_with("unknown"));
    let file = std::fs::read_to_string(dir.path().join(".nika/session-state.json")).unwrap();
    assert!(!file.contains(KEY), "the key reached the record");
    let status = s.status();
    assert!(
        status.contains(
            "decision seat typesafe/jev-test (operator-selected, outside any allowance or cap): 1 call(s) sent"
        ),
        "{status}"
    );
    assert!(
        !status.contains("priced call(s) sent"),
        "the no-budget priced subtotal claims the decision call: {status}"
    );
}

#[test]
fn a_numeric_session_allowance_is_never_charged_with_the_decision_seat() {
    let deepseek = Peer::start(vec![(200, response("unused"))]);
    let _transport = test_transport::install(&deepseek.url);
    let dir = tempfile::tempdir().unwrap();
    let jev = SystemOne::start(vec![Reply::Json(200, answer("lookup"))]);
    let mut s = seated(dir.path(), &jev.base);
    let _ = s.turn(&format!("{TICKETS} budget 2 USD."));
    assert!(
        jev.requests().is_empty(),
        "the numeric allowance was charged"
    );
    let kept = crate::SessionState::load(dir.path()).unwrap().unwrap();
    if let Some(o) = kept
        .inference_observations
        .iter()
        .find(|o| o["schema"] == DECISION_SCHEMA)
    {
        assert_eq!(o["calls_sent"], 0, "claimed as used: {o}");
        assert!(o["refused"].is_string(), "{o}");
    }
}
