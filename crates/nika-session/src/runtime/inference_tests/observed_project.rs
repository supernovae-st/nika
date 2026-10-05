// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The project the Session observes for its seat: the first generation is told every
//! column of the file the request names (not only the ones it words), an answer round and a
//! revision are told the same project for the intent the compiler reads, and a link that leads
//! outside the project is never read. Loopback mechanics only; nothing leaves the machine.
use super::*;
use crate::authoring::{
    AUTHORING_CALLS_PER_COMPILE, AUTHORING_INITIAL_TOKENS, AUTHORING_MAX_TOKENS, AUTHORING_REPAIRS,
    AUTHORING_TIMEOUT, AuthoringContext, AuthoringRound, AuthoringSeat, compile_in_with_admission,
    session_policy,
};
use crate::runtime::tests::ready;
use nika_onboard::compile::{CompileRequest, CompileStatus, NativeMode, revise_intent};
use nika_providers::InferenceAdmission;

const SALES: &str = "Lis ventes.csv, garde les lignes dont statut vaut paye, écris-les dans paiements.csv avec le même en-tête et écris leur montant total sous forme de nombre dans paiements-total.txt.";
const CHANGE: &str =
    "Appelle finalement le fichier règlements.csv ; conserve le filtre et le total.";
/// Columns the request never words: the observation must still carry them.
const UNWORDED: [&str; 3] = ["client", "devise", "reference_interne"];

fn sales(root: &Path) {
    std::fs::write(
        root.join("ventes.csv"),
        "id,date,client,statut,montant,devise,reference_interne\n1,2026-09-01,Acme,paye,120.50,EUR,A-1\n2,2026-09-02,Bolt,impaye,80,EUR,A-2\n3,2026-09-03,Cora,paye,42,EUR,A-3\n",
    )
    .expect("fixture");
}

/// The default context (escalate: semantic CREATE, the private plan first), observing `root`.
fn context(root: &Path) -> AuthoringContext {
    AuthoringContext::from_settings(
        &nika_cli_host::compile::config::AuthoringSettings::none(),
        &nika_cli_host::compile::config::AuthoringSettings::none(),
    )
    .with_project_root(root.to_path_buf())
}

fn seat() -> AuthoringSeat {
    AuthoringSeat::Provider {
        model: MODEL.to_owned(),
    }
}

/// The text of the first request the seat received.
fn first_request(peer: &Peer) -> String {
    peer.bodies()
        .first()
        .map(Value::to_string)
        .expect("a generation was sent")
}

#[test]
fn the_first_generation_is_told_every_observed_column_of_the_named_source() {
    let peer = Peer::start(vec![(200, response("{}"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    sales(dir.path());
    let out = compile_in_with_admission(
        &seat(),
        &context(dir.path()),
        &CompileRequest::create(SALES),
        SALES,
        &InferenceAdmission::unbudgeted(),
    );
    let first = first_request(&peer);
    assert!(first.contains("observed_world"), "{first}");
    for column in ["statut", "montant"].iter().chain(UNWORDED.iter()) {
        assert!(first.contains(column), "{column} missing: {first}");
    }
    assert!(
        first.contains("impaye"),
        "the categorical values ride along: {first}"
    );
    // Data only: no row value that is not categorical (a client name, an amount) is presented.
    assert!(
        !first.contains("Acme") && !first.contains("120.50"),
        "{first}"
    );
    // The receipt says what was observed and presented, by path and count.
    if let Ok(out) = out {
        let observed = &out.provenance.decision.as_ref().expect("record")["session"]["observed"];
        assert_eq!(observed["attached"], true);
        assert_eq!(observed["presented"], true);
        assert_eq!(observed["rows"][0]["path"], "ventes.csv", "{observed}");
        assert_eq!(observed["rows"][0]["state"], "observed", "{observed}");
        assert_eq!(observed["rows"][0]["columns"], 7, "{observed}");
    }
}

#[test]
fn an_answer_round_is_told_the_project_for_the_intent_it_reads() {
    let peer = Peer::start(vec![(200, response("{}"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    sales(dir.path());
    // The first words named no file; the answered clarification replaces the request.
    let mut round = AuthoringRound::new("Fais le tri des paiements.");
    round.answers.insert(
        "intent.clarification".to_owned(),
        serde_json::to_string(SALES).unwrap(),
    );
    let _ = round.compile_with_admission(
        &seat(),
        &context(dir.path()),
        &InferenceAdmission::unbudgeted(),
    );
    let first = first_request(&peer);
    for column in UNWORDED {
        assert!(first.contains(column), "{column} missing: {first}");
    }
}

#[test]
fn a_revision_is_told_the_base_requests_project_and_its_change() {
    let peer = Peer::start(vec![(200, response("{}"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    sales(dir.path());
    let base: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/compile/copy-fr.json"))
            .expect("fixture");
    let base = base["candidate"].as_str().expect("candidate").to_owned();
    let request = CompileRequest::edit(base, CHANGE).with_original_intent(SALES);
    let revised = revise_intent(&request).expect("the revision reads the base request");
    let _ = compile_in_with_admission(
        &seat(),
        &context(dir.path()),
        &request,
        &revised,
        &InferenceAdmission::unbudgeted(),
    );
    let first = first_request(&peer);
    for column in UNWORDED {
        assert!(
            first.contains(column),
            "{column} missing from the revision: {first}"
        );
    }
    assert!(
        first.contains("r\\u00e8glements.csv") || first.contains("règlements.csv"),
        "the change's own destination is observed too: {first}"
    );
}

#[cfg(unix)]
#[test]
fn a_link_leading_outside_the_project_is_never_presented() {
    let peer = Peer::start(vec![(200, response("{}"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(
        outside.path().join("secret.csv"),
        "salaire_confidentiel,iban_prive\n1,FR76\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("secret.csv"),
        dir.path().join("ventes.csv"),
    )
    .unwrap();
    let _ = compile_in_with_admission(
        &seat(),
        &context(dir.path()),
        &CompileRequest::create(SALES),
        SALES,
        &InferenceAdmission::unbudgeted(),
    );
    let first = first_request(&peer);
    for leaked in ["salaire_confidentiel", "iban_prive", "FR76"] {
        assert!(!first.contains(leaked), "{leaked} leaked: {first}");
    }
    assert!(first.contains("outside_project"), "{first}");
}

#[test]
fn without_a_project_root_nothing_is_observed() {
    let peer = Peer::start(vec![(200, response("{}"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    sales(dir.path());
    let context = AuthoringContext::from_settings(
        &nika_cli_host::compile::config::AuthoringSettings::none(),
        &nika_cli_host::compile::config::AuthoringSettings::none(),
    );
    let _ = compile_in_with_admission(
        &seat(),
        &context,
        &CompileRequest::create(SALES),
        SALES,
        &InferenceAdmission::unbudgeted(),
    );
    assert!(!first_request(&peer).contains("reference_interne"));
}

#[test]
fn the_session_budget_and_its_review_are_one_bound() {
    let policy = session_policy(MODEL, false, NativeMode::Escalate);
    assert_eq!(policy.max_tokens, AUTHORING_MAX_TOKENS);
    assert_eq!(policy.initial_max_tokens, Some(AUTHORING_INITIAL_TOKENS));
    const { assert!(AUTHORING_INITIAL_TOKENS < AUTHORING_MAX_TOKENS) };
    assert_eq!(policy.timeout, AUTHORING_TIMEOUT);
    assert_eq!(policy.repair_limit(), Some(AUTHORING_REPAIRS));
    assert_eq!(
        session_policy(MODEL, true, NativeMode::Escalate).timeout,
        std::time::Duration::from_secs(300),
        "a subscription harness keeps its own deadline"
    );
    // One classification + one seated compile (COLD plan and evidence repair, native opening and
    // its repairs) is exactly what a fresh unknown-cost review admits, at the same ceilings.
    assert_eq!(
        nika_providers::admission::SESSION_REVIEW_MAX_REQUESTS,
        1 + AUTHORING_CALLS_PER_COMPILE
    );
    assert_eq!(
        nika_providers::admission::SESSION_REVIEW_MAX_OUTPUT_TOKENS,
        AUTHORING_MAX_TOKENS
    );
    assert_eq!(
        nika_providers::admission::SESSION_REVIEW_TIMEOUT,
        AUTHORING_TIMEOUT
    );
}

#[test]
fn a_hot_copy_observes_the_source_without_claiming_model_presentation() {
    let peer = Peer::start(vec![]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    sales(dir.path());
    let intent = "Copy ./ventes.csv to ./copie.csv";
    let out = compile_in_with_admission(
        &seat(),
        &AuthoringContext::default().with_project_root(dir.path()),
        &CompileRequest::create(intent),
        intent,
        &InferenceAdmission::unbudgeted(),
    )
    .expect("hot copy");
    assert!(peer.bodies().is_empty());
    let observed = &out.provenance.decision.expect("record")["session"]["observed"];
    assert_eq!(observed["attached"], true);
    assert_eq!(observed["presented"], false);
    assert_eq!(observed["rows"][0]["state"], "observed");
}

/// A request over a CSV the test writes: its key named by the request, or not.
const TICKETS: &str =
    "Read ./tickets.csv, keep only the rows whose status is open and write them to ./open.csv";
const TICKETS_WORD: &str =
    "Read ./tickets.csv, keep only the rows whose state is open and write them to ./open.csv";

fn tickets(root: &Path, extra: &str) {
    std::fs::write(
        root.join("tickets.csv"),
        format!("id,status,amount\n1,open,10\n2,closed,20\n3,open,30\n{extra}"),
    )
    .expect("fixture");
}

/// A deterministic seat observes the project too (R4 S1): the key the request words and the
/// file's header declares is grounded — READY, zero calls, the observation recorded — while the
/// pure deterministic compile, with no root, has no world and asks the exact key.
#[test]
fn the_deterministic_seat_observes_the_named_source_and_grounds_its_key() {
    let dir = tempfile::tempdir().unwrap();
    tickets(dir.path(), "");
    let seat = AuthoringSeat::Deterministic { why: None };
    let request = CompileRequest::create(TICKETS);
    let out = crate::authoring::compile_in(&seat, &context(dir.path()), &request, TICKETS)
        .expect("compiles");
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let decision = out.provenance.decision.as_ref().expect("decision");
    let entry = &decision["grounding"][0];
    assert_eq!(entry["field"], "status", "{decision:#}");
    assert_eq!(entry["grade"], "declared", "{decision:#}");
    assert!(
        entry["revision"].as_str().is_some_and(|r| r.len() == 64),
        "the revision is the peek's hash: {decision:#}"
    );
    assert!(decision["session"]["observed"].is_object(), "{decision:#}");
    let pure = crate::authoring::compile_deterministic(&request).expect("compiles");
    assert_ne!(pure.status, CompileStatus::Ready, "{pure:#?}");
    assert!(
        pure.questions.iter().any(|q| q.key == "const.rule_field_1"),
        "{pure:#?}"
    );
}

fn deterministic_session(root: &Path) -> SessionRuntime {
    SessionRuntime::open(
        root,
        ready(crate::intelligence::IntelligenceKind::None, DataLocus::None),
        Box::new(crate::reasoner::NoReasoner),
    )
}

/// The Session's deterministic ladder observes the project (R4 S1): a word the file does not
/// spell is asked over its observed keys; answered on the same file, the work is proposed; the
/// same answer after the file changed maps another revision and is asked again.
#[test]
fn a_deterministic_session_asks_over_the_observed_keys_and_refuses_a_stale_answer() {
    for changed in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        tickets(dir.path(), "");
        let mut s = deterministic_session(dir.path());
        let TurnOutcome::Question { key, question } = s.turn(TICKETS_WORD) else {
            panic!("the word the file does not spell is asked");
        };
        assert_eq!(key, "const.rule_field_1", "{question}");
        assert!(
            ["id", "status", "amount"]
                .iter()
                .all(|k| question.contains(k)),
            "the observed keys are offered: {question}"
        );
        if changed {
            tickets(dir.path(), "4,open,40\n");
        }
        let outcome = s.turn("status");
        if changed {
            assert!(
                matches!(outcome, TurnOutcome::Question { ref key, .. } if key == "const.rule_field_1"),
                "a stale answer is asked again: {outcome:?}"
            );
        } else {
            assert!(
                matches!(outcome, TurnOutcome::Proposal { .. }),
                "{outcome:?}"
            );
        }
        assert!(
            !dir.path().join("open.csv").exists(),
            "nothing is written before consent"
        );
    }
}

/// When money blocks cognition, the deterministic compiles the Session falls back to observe the
/// project too (R4 S1): a request read again and a round compiled again ground the key the file
/// declares, with no call.
#[test]
fn a_money_blocked_deterministic_round_still_observes_the_project() {
    let dir = tempfile::tempdir().unwrap();
    tickets(dir.path(), "");
    let mut s = deterministic_session(dir.path());
    s.money.reconfirm = true;
    assert!(s.money_blocks_cognition());
    let out = s
        .compile_request(&CompileRequest::create(TICKETS), TICKETS)
        .expect("compiles");
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let grounding = &out.provenance.decision.as_ref().expect("decision")["grounding"][0];
    assert_eq!(grounding["grade"], "declared", "{grounding}");
    let outcome = s.compile_again(AuthoringRound::new(TICKETS));
    assert!(
        matches!(outcome, TurnOutcome::Proposal { .. }),
        "{outcome:?}"
    );
}

/// A seated Session round roots the session's own context at its project (R4 S1): the host's
/// context names no root, yet the first generation is told the project for the files the request
/// names, and the session's context keeps that root.
#[test]
fn a_seated_session_round_is_told_the_project_under_its_own_root() {
    let peer = Peer::start(authored(response));
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    assert_eq!(s.authoring_context().project_root(), None);
    let out = s.turn(&format!("{WORK} budget 2 USD."));
    assert!(matches!(out, TurnOutcome::Proposal { .. }), "{out:?}");
    let first = first_request(&peer);
    assert!(first.contains("observed_world"), "{first}");
    assert_eq!(
        s.authoring_context().project_root(),
        Some(s.snapshot.root.as_path())
    );
}

#[test]
fn a_renamed_header_reasks_the_field_without_losing_the_round() {
    let dir = tempfile::tempdir().unwrap();
    tickets(dir.path(), "");
    let mut s = deterministic_session(dir.path());
    assert!(matches!(s.turn(TICKETS_WORD), TurnOutcome::Question { .. }));
    std::fs::write(
        dir.path().join("tickets.csv"),
        "id,state,amount\n1,open,10\n2,closed,20\n3,open,30\n",
    )
    .unwrap();
    let outcome = s.turn("status");
    let TurnOutcome::Question { question, .. } = outcome else {
        panic!("the stale answer must leave a fresh question: {outcome:?}");
    };
    assert!(question.contains("state"), "{question}");
    assert!(s.pending_question().is_some());
    assert_eq!(s.intent.goal.as_deref(), Some(TICKETS_WORD));
    assert!(s.pending_proposal().is_none());
    assert!(!dir.path().join("open.csv").exists());
    assert!(matches!(s.turn("state"), TurnOutcome::Proposal { .. }));
}

/// The refreshed question is bound to the observation it showed (R4 A6): the refused answer
/// leaves the round, an aside beside the question changes nothing, and a second change before
/// the fresh answer asks again; only an answer for the current file proposes.
#[test]
fn a_second_change_before_the_fresh_answer_asks_again() {
    let dir = tempfile::tempdir().unwrap();
    tickets(dir.path(), "");
    let header = |line: &str| {
        let rows = "1,open,10\n2,closed,20\n3,open,30\n";
        std::fs::write(dir.path().join("tickets.csv"), format!("{line}\n{rows}")).unwrap();
    };
    let mut s = deterministic_session(dir.path());
    assert!(matches!(s.turn(TICKETS_WORD), TurnOutcome::Question { .. }));
    header("id,state,amount");
    let TurnOutcome::Question { question, .. } = s.turn("status") else {
        panic!("the stale answer is asked again");
    };
    assert!(question.contains("state"), "{question}");
    let round = s.authoring.as_ref().expect("the round is kept");
    assert!(
        !round.answers.contains_key("const.rule_field_1"),
        "{round:?}"
    );
    assert_eq!(round.intent, TICKETS_WORD);
    assert!(matches!(s.turn("why?"), TurnOutcome::Aside(_)));
    assert!(s.pending_question().is_some());
    header("id,etat,amount");
    let outcome = s.turn("state");
    let TurnOutcome::Question { question, .. } = outcome else {
        panic!("an answer for the first rename is stale again: {outcome:?}");
    };
    assert!(question.contains("etat"), "{question}");
    assert!(s.pending_proposal().is_none());
    assert!(matches!(s.turn("etat"), TurnOutcome::Proposal { .. }));
    assert!(!dir.path().join("open.csv").exists());
}

#[test]
fn a_zero_budget_work_request_keeps_the_deterministic_business_round() {
    for (intent, ask) in [(TICKETS, false), (TICKETS_WORD, true)] {
        let peer = Peer::start(vec![]);
        let _transport = test_transport::install(&peer.url);
        let dir = tempfile::tempdir().unwrap();
        tickets(dir.path(), "");
        let mut s = open(dir.path());
        let request = format!("{intent}. Budget: $0.");
        let outcome = s.turn(&request);
        if ask {
            assert!(
                matches!(outcome, TurnOutcome::Question { ref key, .. } if key == "const.rule_field_1"),
                "the business field is asked without cognition: {outcome:?}"
            );
            assert!(matches!(s.turn("status"), TurnOutcome::Proposal { .. }));
        } else {
            assert!(
                matches!(outcome, TurnOutcome::Proposal { .. }),
                "{outcome:?}"
            );
        }
        assert!(s.money_blocks_cognition());
        assert_eq!(s.monetary_decision().unwrap().original_intent, request);
        assert!(peer.bodies().is_empty(), "no provider request");
        assert!(!dir.path().join("open.csv").exists());
        assert!(!dir.path().join("compiled-workflow.nika").exists());
    }
}
