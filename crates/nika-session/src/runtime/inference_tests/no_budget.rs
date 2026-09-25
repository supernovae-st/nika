// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Priced cognition without any Session budget (S98 F11): the selected model
//! still answers, every call is observed through the bounded seam and the
//! record, and nothing becomes an allowance, a cap, a review or authority.
//! Explicit money keeps its own law. Loopback mechanics only; no provider
//! qualification.
use super::*;
use crate::runtime::inference::OBSERVED_PREFIX;

/// DIALOG-11's request: the native seat asks the destination (the fixture shape
/// of `question_identity`, kept local to these mechanics).
const DESTINATION: &str = "Copie entree.txt vers une destination à préciser.";
const DESTINATION_DRAFT: &str = r#"nika: copy-to-chosen-file
const:
  destination_path: ""
permits:
  fs:
    read: ["./entree.txt"]
    write: [""]
  tools: ["nika:read", "nika:write"]
tasks:
  read_source:
    invoke:
      tool: "nika:read"
      args: { path: "./entree.txt" }
  write_destination:
    with: { text: "${{ tasks.read_source.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "${{ const.destination_path }}", content: "${{ with.text }}" }
"#;

fn asks_destination() -> String {
    json!({"candidate": DESTINATION_DRAFT, "questions": [{"key": "const.destination_path",
        "label": "Destination file path", "answer_type": "text",
        "why": "The request leaves the destination to be specified."}],
        "gaps": [], "notes": "the destination is asked"})
    .to_string()
}

/// A door's classifier that takes every line at a question as its answer.
struct Answers;

impl TurnClassifier for Answers {
    fn classify(&mut self, context: &TurnContext, _raw: &str) -> TurnDecision {
        let act = if matches!(context.phase, SessionPhase::QuestionPending) {
            TurnAct::Answer
        } else {
            TurnAct::NewWork
        };
        TurnDecision::new(act, crate::turn::RoutingMethod::Model)
    }
}

#[test]
fn a_priced_call_without_a_budget_is_observed_never_admitted() {
    let peer = Peer::start(vec![(200, response("Hello"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    s.admit_money("hello", false)
        .expect("no money is not a refusal");
    assert_eq!(
        s.monetary_decision().unwrap().inference,
        InferenceEnforcement::NotMetered
    );
    let reply = s
        .reason_with_money("hello", false)
        .expect("the chosen model");
    assert_eq!(reply.text, "Hello");
    let bodies = peer.bodies();
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0]["model"], "deepseek-v4-pro");
    assert_eq!(
        bodies[0]["max_tokens"], 8192,
        "bounded like every observed call"
    );
    assert!(
        s.inference_receipt().unwrap().is_none(),
        "no Session allowance"
    );
    let r = s.money.observed.snapshot().unwrap();
    assert!(r.unbudgeted && r.state == AdmissionState::Open, "{r:?}");
    assert_eq!(r.attempts.len(), 1);
    let attempt = &r.attempts[0];
    assert!(attempt.sent);
    assert_eq!(
        attempt.estimated,
        attempt.tariff.price(100, 20, 0),
        "the catalog price of the reported usage, never an invoice"
    );
    assert_eq!(r.billed, None);
    let kept = crate::SessionState::load(dir.path()).unwrap().unwrap();
    assert!(
        kept.decisions
            .iter()
            .all(|d| !d.starts_with(OBSERVED_PREFIX)),
        "settled"
    );
    let o = kept.inference_observations.last().unwrap();
    assert_eq!(o["unbudgeted"], true);
    assert_eq!(o["limit_nano_usd"], Value::Null);
    assert_eq!(o["billed_nano_usd"], Value::Null);
    let estimated = attempt.estimated.unwrap().nano_usd.to_string();
    assert_eq!(o["attempts"][0]["estimated_nano_usd"], json!(estimated));
    let status = s.status();
    assert!(
        status.contains(
            "no-budget observation (outside any allowance or cap): 1 priced call(s) sent"
        ) && status.contains("0 without usable settlement"),
        "{status}"
    );
}

#[test]
fn explicit_zero_and_an_insufficient_ceiling_never_fall_back_to_observation() {
    for (money, refusal) in [
        ("budget 0 USD", None),
        (
            "budget 0.0001 USD",
            Some("cannot cover the full-context reservation"),
        ),
    ] {
        let peer = Peer::start(vec![(200, response("Hello"))]);
        let _transport = test_transport::install(&peer.url);
        let dir = tempfile::tempdir().unwrap();
        let mut s = open(dir.path());
        s.admit_money(money, false).expect("admitted money");
        assert!(s.reason_with_money("hello", false).is_err(), "{money}");
        assert!(peer.bodies().is_empty(), "{money}: nothing was sent");
        let observed = s.money.observed.snapshot().unwrap();
        assert!(
            observed.attempts.is_empty(),
            "{money}: never observed instead"
        );
        let refused = s.inference_receipt().unwrap().and_then(|r| r.refusal);
        match refusal {
            None => assert!(refused.is_none(), "{money}: zero opens no account"),
            Some(why) => assert!(refused.is_some_and(|r| r.contains(why)), "{money}"),
        }
        let observations = s.cost_observations();
        assert!(
            observations.iter().all(|o| o["unbudgeted"] != true),
            "{money}"
        );
    }
}

#[test]
fn every_dispatch_site_rides_the_one_no_budget_observation() {
    let peer = Peer::start(vec![
        (200, response("NEW_WORK")),
        (200, response("Hello")),
        (200, response("ANSWER")),
        (200, response(&native())),
    ]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    assert_eq!(s.classify(SessionPhase::Idle, WORK).act, TurnAct::NewWork);
    s.reason_with_money("explain this work", false)
        .expect("conversation");
    s.reason_with_money("read one label", true).expect("label");
    let round = AuthoringRound::new(WORK);
    s.compile_round(&round, &s.seat).expect("compile");
    // A revision rides the same bracket (S102 left this site unrecorded).
    s.compile_request(&round.request(), WORK).expect("revision");
    let r = s.money.observed.snapshot().unwrap();
    assert_eq!(r.attempts.len(), 5);
    assert!(
        r.attempts.iter().all(|a| a.sent && a.estimated.is_some()),
        "{r:?}"
    );
    assert_eq!(peer.bodies().len(), 5);
    assert!(s.inference_receipt().unwrap().is_none());
    let kept = crate::SessionState::load(dir.path()).unwrap().unwrap();
    assert_eq!(kept.inference_observations.len(), 1);
    let attempts = kept.inference_observations[0]["attempts"].as_array();
    assert_eq!(attempts.map(Vec::len), Some(5));
}

#[test]
fn a_continuation_stays_on_its_frozen_observation_until_new_work() {
    let mut contradicted = response("archive/copie.txt");
    contradicted["model"] = json!("s108-another-served-model");
    let peer = Peer::start(vec![
        (200, response(&asks_destination())),
        (200, contradicted),
        (200, response("Hello")),
    ]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    s.with_classifier(Box::new(Answers));
    let out = s.turn(DESTINATION);
    assert!(
        matches!(&out, TurnOutcome::Question { key, .. } if key == "const.destination_path"),
        "{out:?}"
    );
    // The answer said in words is read on the same observation; contradicted, it freezes.
    let answer = "la destination sera archive/copie.txt merci";
    let out = s.turn(answer);
    assert!(
        matches!(&out, TurnOutcome::Question { .. }),
        "the question waits: {out:?}"
    );
    assert_eq!(peer.bodies().len(), 2);
    let frozen = s.money.observed.snapshot().unwrap();
    assert_eq!(frozen.state, AdmissionState::Uncertain);
    // The same work continued sends nothing: no automatic replay after a possible charge.
    let out = s.turn(answer);
    assert!(
        matches!(&out, TurnOutcome::Question { question, .. } if question.contains("nothing was bound")),
        "{out:?}"
    );
    assert_eq!(peer.bodies().len(), 2, "a continuation never reopens it");
    let kept = crate::SessionState::load(dir.path()).unwrap().unwrap();
    assert!(
        kept.decisions
            .iter()
            .all(|d| !d.starts_with(OBSERVED_PREFIX)),
        "a frozen account sends nothing, so it writes no line"
    );
    assert!(matches!(s.turn("cancel"), TurnOutcome::Facts(_)));
    // New work, and only new work, observes afresh; the frozen account is history.
    let out = s.turn("hello");
    assert!(
        matches!(&out, TurnOutcome::Reply(text) if text == "Hello"),
        "{out:?}"
    );
    assert_eq!(peer.bodies().len(), 3);
    let states: Vec<_> = s
        .cost_observations()
        .iter()
        .map(|o| (o["unbudgeted"].clone(), o["state"].clone()))
        .collect();
    assert_eq!(
        states,
        vec![
            (json!(true), json!("Uncertain")),
            (json!(true), json!("Open"))
        ]
    );
}

#[test]
fn a_recovered_observation_is_history_never_authority() {
    let peer = Peer::start(vec![(200, response("Hello"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut first = open(dir.path());
    first.reason_with_money("hello", false).expect("observed");
    drop(first);
    let kept = crate::SessionState::load(dir.path()).unwrap().unwrap();
    assert_eq!(kept.inference_observations.len(), 1);
    let mut resumed = open(dir.path());
    assert!(resumed.restore_state().is_some());
    assert!(!resumed.money.reconfirm, "no allowance was ever set");
    assert!(resumed.inference_receipt().unwrap().is_none(), "no account");
    let live = resumed.money.observed.snapshot().unwrap();
    assert!(live.attempts.is_empty(), "never the live observation");
    assert_eq!(resumed.cost_observations(), kept.inference_observations);
    resumed
        .reason_with_money("hello", false)
        .expect("the chosen model answers");
    assert_eq!(resumed.cost_observations().len(), 2);
    drop(resumed);
    // An observation written without the field reads as it always did: false,
    // so restrictive. Nothing requires the field to decode.
    let mut old = kept;
    if let Some(o) = old.inference_observations[0].as_object_mut() {
        o.remove("unbudgeted");
    }
    old.save(dir.path()).unwrap();
    let mut legacy = open(dir.path());
    assert!(legacy.restore_state().is_some(), "still readable");
    assert!(
        legacy.money.reconfirm,
        "absent is false: the old restriction holds"
    );
    assert!(legacy.reason_with_money("hello", false).is_err());
    assert_eq!(peer.bodies().len(), 2, "nothing sent under it");
}

#[test]
fn a_later_ceiling_keeps_the_observation_and_never_claims_to_cover_it() {
    let peer = Peer::start(vec![(200, response("Hello"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    s.reason_with_money("hello", false).expect("observed");
    s.admit_money("budget 2 USD", false)
        .expect("a ceiling from now on");
    s.reason_with_money("hello again", false).expect("admitted");
    let admitted = s.inference_receipt().unwrap().unwrap();
    assert_eq!(
        admitted.attempts.len(),
        1,
        "the ceiling holds its own calls"
    );
    assert_eq!(Some(admitted.estimated), admitted.attempts[0].estimated);
    let observed = s.money.observed.snapshot().unwrap();
    assert_eq!(
        observed.attempts.len(),
        1,
        "the earlier call stays observed"
    );
    let observations = s.cost_observations();
    assert_eq!(observations.len(), 2);
    assert_eq!(
        observations
            .iter()
            .filter(|o| o["unbudgeted"] == true)
            .count(),
        1
    );
    let status = s.status();
    assert!(
        status.contains("catalog admission: allowance")
            && status.contains(
                "no-budget observation (outside any allowance or cap): 1 priced call(s) sent"
            ),
        "{status}"
    );
    assert_eq!(peer.bodies().len(), 2);
}
