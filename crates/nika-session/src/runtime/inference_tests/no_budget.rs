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
    s.admit_money("hello", false, false)
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
        s.admit_money(money, false, false).expect("admitted money");
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
    s.admit_money("budget 2 USD", false, false)
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

/// The ticket request of the money directive tests (R4 A6): a key the file's header declares.
const TICKETS: &str =
    "Read ./tickets.csv, keep only the rows whose status is open and write them to ./open.csv";

fn tickets_csv(root: &Path, header: &str) {
    std::fs::write(
        root.join("tickets.csv"),
        format!("{header}\n1,open,10\n2,closed,20\n3,open,30\n"),
    )
    .expect("fixture");
}

/// The workflow bytes the pending proposal would write.
fn proposed(s: &SessionRuntime) -> String {
    s.pending.as_ref().expect("a proposal").changes[0]
        .content()
        .to_owned()
}

/// A request's monetary directive is its ceiling, never a business clause (R4 A6 · D6-S2/S3):
/// every equivalent spelling admits the same $0, the work is read without it (READY with the
/// plain rule, zero calls on the chosen seat), and the original request stays bound.
#[test]
fn every_spelling_of_a_zero_ceiling_is_the_ceiling_never_a_clause() {
    for suffix in [
        ". Budget: $0.",
        ", budget=0",
        ", budget: 0",
        " --max-cost-usd 0",
        " with a budget of 0 USD",
        ". Plafond de 0 dollars.",
    ] {
        let peer = Peer::start(vec![]);
        let _transport = test_transport::install(&peer.url);
        let dir = tempfile::tempdir().unwrap();
        tickets_csv(dir.path(), "id,status,amount");
        let mut s = open(dir.path());
        let request = format!("{TICKETS}{suffix}");
        let out = s.turn(&request);
        assert!(
            matches!(out, TurnOutcome::Proposal { .. }),
            "{request}: {out:?}"
        );
        let bytes = proposed(&s);
        assert!(bytes.contains(".status == \"open\""), "{bytes}");
        assert!(
            !bytes.contains("budget") && !bytes.contains("dollar"),
            "{bytes}"
        );
        let money = s.monetary_decision().expect("admitted");
        assert_eq!(money.effective_usd, Some(0.0), "{request}");
        assert_eq!(money.original_intent, request);
        assert!(s.money_blocks_cognition(), "{request}");
        assert!(peer.bodies().is_empty(), "{request}: no provider request");
        assert!(!dir.path().join("open.csv").exists());
    }
}

/// Words that only look like money stay the request's data (R4 A6): a field named `budget`, a
/// quoted value, a price the rule compares. No ceiling is admitted from them and none refused.
#[test]
fn a_budget_field_a_quoted_value_and_a_price_are_data_never_money() {
    let peer = Peer::start(vec![]);
    let _transport = test_transport::install(&peer.url);
    for (header, request, word) in [
        (
            "id,status,budget",
            "Read ./tickets.csv, keep only the rows whose budget is above 15 and write them to ./big.csv",
            ".budget",
        ),
        (
            "id,status,amount",
            "Read ./tickets.csv, keep only the rows whose status is \"budget=0\" and write them to ./open.csv",
            "budget=0",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        tickets_csv(dir.path(), header);
        let mut s = open(dir.path());
        let out = s.turn(request);
        assert!(
            matches!(out, TurnOutcome::Proposal { .. }),
            "{request}: {out:?}"
        );
        assert!(proposed(&s).contains(word), "{}", proposed(&s));
        let money = s.monetary_decision().expect("observed");
        assert_eq!(money.explicit_amount, None, "{request}");
        assert!(money.refusal.is_none(), "{request}");
    }
    assert!(peer.bodies().is_empty());
}

/// A ceiling without a currency whose anchor is a column of the named file reads both ways
/// (R4 A6): the $0 holds on the chosen seat, the work is read neither way, and the refusal says
/// the compiler's reason — never the bare ceiling, never a guessed rule.
#[test]
fn a_ceiling_that_also_names_a_field_is_said_never_read_either_way() {
    let peer = Peer::start(vec![]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    tickets_csv(dir.path(), "id,status,budget");
    let mut s = open(dir.path());
    let TurnOutcome::Refusal(refusal) = s.turn(&format!("{TICKETS}, budget=0")) else {
        panic!("a refusal that says the reason");
    };
    assert_eq!(refusal.class, RefusalClass::NotAllowed);
    for said in [
        "`budget=0` reads as the monetary ceiling",
        "observed field `budget`",
        "no further cognition admitted",
    ] {
        assert!(refusal.text.contains(said), "{said}: {}", refusal.text);
    }
    assert!(s.pending_proposal().is_none() && s.pending_question().is_none());
    let money = s.monetary_decision().expect("admitted");
    assert_eq!(money.effective_usd, Some(0.0));
    assert!(peer.bodies().is_empty(), "no provider request");
    assert!(!dir.path().join("open.csv").exists());
}

/// A Session on a route whose USD cost the catalog cannot qualify (the unknown-cost fixture's
/// unpriced model): every priced call there needs its own review first.
fn open_unpriced(root: &Path) -> SessionRuntime {
    const UNPRICED: &str = "deepseek/s81-unpriced-fixture";
    let mut s = open(root);
    s.intelligence.model = Some(UNPRICED.into());
    s.reasoner = Box::new(ProviderReasoner {
        model: UNPRICED.into(),
        label: "selected unpriced route".into(),
    });
    s.refresh_seat();
    s.set_cost_host_evidence(
        nika_runtime::cost_choice::CostHostEvidence::unmanaged_interactive_local(),
    );
    s
}

/// The gate that keeps the deterministic ladder free of cost consent reads a work request with
/// its own monetary directives admitted (R4 A6): on an unpriced route the stated ceiling is the
/// same READY proposal, never a cost review or a zero-constraint refusal, and nothing is sent.
#[test]
fn a_ceiling_on_an_unpriced_route_keeps_the_deterministic_round() {
    let peer = Peer::start(vec![]);
    let _transport = test_transport::install(&peer.url);
    for (suffix, usd) in [
        (". Budget: $5.", 5.0),
        (" --max-cost-usd 0", 0.0),
        (". Plafond de 0 dollars.", 0.0),
    ] {
        let dir = tempfile::tempdir().unwrap();
        tickets_csv(dir.path(), "id,status,amount");
        let mut s = open_unpriced(dir.path());
        let request = format!("{TICKETS}{suffix}");
        let out = s.turn(&request);
        assert!(
            matches!(out, TurnOutcome::Proposal { .. }),
            "{request}: {out:?}"
        );
        assert!(!s.waiting_cost_choice(), "{request}");
        let money = s.monetary_decision().expect("admitted");
        assert_eq!(money.effective_usd, Some(usd), "{request}");
        assert!(proposed(&s).contains(".status == \"open\""));
    }
    assert!(peer.bodies().is_empty(), "no provider request");
}

/// An explicit zero forbids every call on any route, so no cost review is staged for it (R4 A6):
/// on an unpriced route, a directive that also names a field and work that needs a model are
/// refused with the reader's own reasons, never « the explicit zero constraint forbids this call »
/// alone, and nothing is sent.
#[test]
fn a_zero_ceiling_on_an_unpriced_route_stages_no_review() {
    let peer = Peer::start(vec![]);
    let _transport = test_transport::install(&peer.url);
    for (header, request, said) in [
        (
            "id,status,budget",
            format!("{TICKETS}, budget=0"),
            "observed field `budget`",
        ),
        (
            "id,status,amount",
            "Prépare un résumé en trois points de entree.txt dans sortie.txt, budget 0 dollar."
                .to_owned(),
            "no further cognition admitted",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        tickets_csv(dir.path(), header);
        let mut s = open_unpriced(dir.path());
        let TurnOutcome::Refusal(refusal) = s.turn(&request) else {
            panic!("{request}: a refusal that says the reason");
        };
        assert!(refusal.text.contains(said), "{request}: {}", refusal.text);
        assert!(!s.waiting_cost_choice(), "{request}");
        assert!(s.pending_proposal().is_none() && s.pending_question().is_none());
        assert_eq!(
            s.monetary_decision().expect("admitted").effective_usd,
            Some(0.0)
        );
    }
    assert!(peer.bodies().is_empty(), "no provider request");
}

/// A positive ceiling is admitted the same way and the deterministic reading needs no call; a
/// malformed, negative, non-finite or conflicting ceiling refuses before any effect (R4 A6).
#[test]
fn a_positive_ceiling_reads_deterministically_and_a_bad_one_refuses_first() {
    let peer = Peer::start(vec![]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    tickets_csv(dir.path(), "id,status,amount");
    let mut s = open(dir.path());
    let request = format!("{TICKETS}. Budget: $5.");
    assert!(matches!(s.turn(&request), TurnOutcome::Proposal { .. }));
    assert_eq!(
        s.monetary_decision().expect("admitted").effective_usd,
        Some(5.0)
    );
    for suffix in [
        ". Budget: $abc.",
        ", budget=-1",
        ". Budget: NaN.",
        ". Budget: inf.",
        ". Budget: $1. Cap: $2.",
    ] {
        let dir = tempfile::tempdir().unwrap();
        tickets_csv(dir.path(), "id,status,amount");
        let mut s = open(dir.path());
        let request = format!("{TICKETS}{suffix}");
        let out = s.turn(&request);
        assert!(
            matches!(out, TurnOutcome::Refusal(ref r) if r.class == RefusalClass::NotAllowed),
            "{request}: {out:?}"
        );
        assert!(s.pending_proposal().is_none() && s.pending_question().is_none());
        let money = s.monetary_decision().expect("the refusal is observed");
        assert!(
            money.effective_usd.is_none() && money.refusal.is_some(),
            "{request}"
        );
        assert!(!dir.path().join("open.csv").exists());
    }
    assert!(peer.bodies().is_empty());
}

/// The copy the s49 recovery fixture used to state (R4 A6): with its « budget 0,50 dollar » read
/// as the ceiling, the copy is deterministic work, proposed with zero provider calls on the chosen
/// seat; the fixture that needed a provider now states a summary.
#[test]
fn the_former_recovery_copy_is_deterministic_work_under_its_ceiling() {
    let peer = Peer::start(vec![]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    let request = "Prépare la copie de entree.txt dans sortie.txt, budget 0,50 dollar.";
    let out = s.turn(request);
    assert!(matches!(out, TurnOutcome::Proposal { .. }), "{out:?}");
    let money = s.monetary_decision().expect("admitted");
    assert_eq!(money.effective_usd, Some(0.5));
    assert_eq!(money.original_intent, request);
    assert!(peer.bodies().is_empty(), "no provider request");
    assert!(!dir.path().join("sortie.txt").exists());
}
