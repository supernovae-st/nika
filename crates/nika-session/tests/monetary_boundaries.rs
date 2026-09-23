// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Public admission mechanics with counting protocol doubles and synthetic
//! pause observations. No provider qualification or workflow execution.
#![allow(clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use nika_session::money::{InferenceEnforcement, MonetarySource};
use nika_session::turn::{RoutingMethod, TurnAct, TurnClassifier, TurnContext, TurnDecision};
use nika_session::{
    IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence, SessionReasoner,
    SessionRuntime, TurnOutcome, UserIntelligencePreference,
};

const COPY: &str = "Read ./entree.txt and write it to ./sortie.txt";
const WORKFLOW: &str = "compiled-workflow.nika";

#[derive(Clone, Default)]
struct Calls {
    classifier: Arc<AtomicUsize>,
    reasoner: Arc<AtomicUsize>,
}

impl Calls {
    fn counts(&self) -> (usize, usize) {
        (
            self.classifier.load(Ordering::SeqCst),
            self.reasoner.load(Ordering::SeqCst),
        )
    }
}

impl TurnClassifier for Calls {
    fn classify(&mut self, _: &TurnContext, _: &str) -> TurnDecision {
        self.classifier.fetch_add(1, Ordering::SeqCst);
        TurnDecision::new(TurnAct::Unknown, RoutingMethod::Fallback)
    }
}

impl SessionReasoner for Calls {
    fn name(&self) -> String {
        "counting protocol double".to_owned()
    }

    fn reason(&mut self, prompt: &str) -> Result<nika_session::Reply, nika_session::ReasonError> {
        self.reasoner.fetch_add(1, Ordering::SeqCst);
        nika_session::ScriptedReasoner::new(vec!["controlled reply".to_owned()]).reason(prompt)
    }
}

fn open(root: &Path) -> (SessionRuntime, Calls) {
    let selected = ResolvedSessionIntelligence::resolve(
        &UserIntelligencePreference::new(IntelligenceKind::None, None),
        &IntelligenceCensus::empty(),
    );
    let calls = Calls::default();
    let mut session = SessionRuntime::open(root, selected, Box::new(calls.clone()));
    session.with_classifier(Box::new(calls.clone()));
    (session, calls)
}

fn saved(root: &Path, clause: &str) -> (SessionRuntime, Calls) {
    let (mut session, calls) = open(root);
    assert!(matches!(session.turn(COPY), TurnOutcome::Proposal { .. }));
    assert!(matches!(
        session.consent(clause),
        TurnOutcome::Proposal { .. }
    ));
    assert!(matches!(session.consent("yes"), TurnOutcome::Facts(_)));
    (session, calls)
}

fn paused(root: &Path, mode: &str) -> (SessionRuntime, Calls) {
    let (mut session, calls) = open(root);
    assert!(matches!(session.turn(COPY), TurnOutcome::Proposal { .. }));
    assert!(matches!(session.consent("yes"), TurnOutcome::Facts(_)));
    assert!(matches!(
        session.turn("run it"),
        TurnOutcome::RunRequested { .. }
    ));
    let trace = root.join("paused.ndjson");
    std::fs::write(
        &trace,
        format!("{{\"kind\":\"workflow_paused\",\"fields\":[{{\"key\":\"task\",\"value\":\"approve\"}},{{\"key\":\"mode\",\"value\":\"{mode}\"}},{{\"key\":\"message\",\"value\":\"Proceed?\"}}]}}\n"),
    ).expect("synthetic pause");
    assert!(matches!(
        session.observe_run(4, Some(&trace)),
        TurnOutcome::GateAsk { .. }
    ));
    (session, calls)
}

#[test]
fn compact_explicit_money_precedes_cognition_and_keeps_full_input() {
    for (clause, amount) in [
        ("budget=0", Some(0.0)),
        ("budget:0", Some(0.0)),
        ("0USD", Some(0.0)),
        ("budget=2", Some(2.0)),
        ("budget:0,50", Some(0.5)),
        ("2USD", Some(2.0)),
        ("0.50USD", Some(0.5)),
        ("budget=0USD", Some(0.0)),
        ("budget=0,50USD", Some(0.5)),
        ("budget=+2", Some(2.0)),
        ("budget=NaN", None),
        ("budget:inf", None),
        ("NaNUSD", None),
        ("-1USD", None),
        ("budget=1e999", None),
        ("budget=0.5.0USD", None),
        ("budget=oops", None),
        ("budget=0 budget:2", None),
    ] {
        let dir = tempfile::tempdir().expect("fixture");
        let (mut session, calls) = open(dir.path());
        let input = format!("  What can you tell me about stars, {clause}?  ");
        let outcome = session.turn(&input);
        assert_eq!(calls.counts(), (0, 0), "{input}: {outcome:?}");
        assert!(
            matches!(outcome, TurnOutcome::Refusal(_)),
            "{input}: {outcome:?}"
        );
        let money = session.monetary_decision().expect("admitted or rejected");
        assert_eq!(money.input, input);
        assert_eq!(money.original_intent, input);
        assert_eq!(money.effective_usd, amount, "{input}");
        assert_eq!(money.inference, InferenceEnforcement::CallsBlocked);
        assert_eq!(money.observed_cost_usd, None);
        assert_eq!(
            money.source,
            if amount.is_some() {
                MonetarySource::Explicit
            } else {
                MonetarySource::Rejected
            }
        );
        assert!(session.pending_proposal().is_none());
        assert!(session.pending_question().is_none());
        assert!(!dir.path().join(".nika").exists());
    }
}

#[test]
fn compact_money_inside_data_does_not_become_a_constraint() {
    for clause in [
        "at 9:00 for 9 files",
        "\"budget=NaN\"",
        "'budget:0'",
        "`0USD`",
        "« budget=0 »",
        "./budget=0/file.txt",
        "./budget:0/file.txt",
        "./0USD.txt",
        "budget=0.txt",
        "0USD.txt",
        "key=0",
        "0USD/file.txt",
    ] {
        let dir = tempfile::tempdir().expect("fixture");
        let (mut session, calls) = open(dir.path());
        let input = format!("What can you tell me about stars and {clause}?");
        session.turn(&input);
        let money = session.monetary_decision().expect("default");
        assert_eq!(money.effective_usd, Some(0.25), "{input}");
        assert_eq!(money.source, MonetarySource::SessionDefault, "{input}");
        assert_eq!(money.original_intent, input);
        assert_eq!(calls.reasoner.load(Ordering::SeqCst), 1, "{input}");
    }
}

#[test]
fn confirm_gate_money_precedes_both_public_answer_doors() {
    for addressed in [false, true] {
        for (clause, amount) in [
            ("budget 0 dollars", Some(0.0)),
            ("budget NaN dollars", None),
            ("budget 2 dollars", Some(2.0)),
            ("budget=0", Some(0.0)),
            ("budget=NaN", None),
            ("2USD", Some(2.0)),
        ] {
            let dir = tempfile::tempdir().expect("fixture");
            let (mut session, calls) = paused(dir.path(), "confirm");
            let gate = session.waiting_gate().expect("gate");
            let before = calls.counts();
            let input = format!("yes but {clause}");
            let outcome = if addressed {
                session.answer_gate_for(&gate, &input)
            } else {
                session.answer_gate(&input)
            };
            assert_eq!(calls.counts(), before, "{input}: {outcome:?}");
            assert!(
                matches!(outcome, TurnOutcome::Refusal(_) | TurnOutcome::Aside(_)),
                "{outcome:?}"
            );
            assert_eq!(session.waiting_gate(), Some(gate));
            let money = session.monetary_decision().expect("actual money");
            assert_eq!(money.input, input);
            assert_eq!(money.effective_usd, amount);
            assert_eq!(money.inference, InferenceEnforcement::CallsBlocked);
            assert!(session.pending_proposal().is_none());
        }
    }
}

#[test]
fn typed_text_and_choice_gate_values_remain_data() {
    for mode in ["text", "choice"] {
        for addressed in [false, true] {
            let dir = tempfile::tempdir().expect("fixture");
            let (mut session, calls) = paused(dir.path(), mode);
            let gate = session.waiting_gate().expect("gate");
            let before = calls.counts();
            let original = session.monetary_decision().expect("money").clone();
            let input = "budget=NaN";
            let out = if addressed {
                session.answer_gate_for(&gate, input)
            } else {
                session.answer_gate(input)
            };
            assert!(
                matches!(out, TurnOutcome::ResumeRequested { ref answer, .. } if answer == "approve=budget=NaN"),
                "{out:?}"
            );
            assert_eq!(calls.counts(), before);
            assert_eq!(session.monetary_decision(), Some(&original));
            assert!(session.waiting_gate().is_none());
        }
    }
}

#[test]
#[cfg(unix)]
fn same_file_symlink_keeps_zero_and_qualified_runs_call_nobody() {
    let dir = tempfile::tempdir().expect("fixture");
    let (mut session, calls) = saved(dir.path(), "budget 0 dollars");
    let proposal = session.monetary_decision().expect("saved").proposal.clone();
    std::os::unix::fs::symlink(WORKFLOW, dir.path().join("alias.nika")).expect("alias");
    let out = session.turn("run alias.nika");
    assert!(
        matches!(out, TurnOutcome::RunRequested { ref run, .. } if run.max_cost_usd.to_bits() == 0.0_f64.to_bits()),
        "{out:?}"
    );
    assert_eq!(
        session.monetary_decision().expect("saved").proposal,
        proposal
    );
    let before = calls.counts();
    assert!(!matches!(
        session.turn("run alias.nika but only on Fridays"),
        TurnOutcome::RunRequested { .. }
    ));
    assert_eq!(calls.counts(), before);
    let out = session.turn("run alias.nika budget 2 USD");
    assert!(
        matches!(out, TurnOutcome::RunRequested { ref run, .. } if run.max_cost_usd.to_bits() == 2.0_f64.to_bits()),
        "{out:?}"
    );
    let out = session.turn("run alias.nika");
    assert!(
        matches!(out, TurnOutcome::RunRequested { ref run, .. } if run.max_cost_usd.to_bits() == 0.0_f64.to_bits()),
        "override is per Run: {out:?}"
    );
}

#[test]
fn independently_created_equal_bytes_do_not_inherit_a_saved_identity() {
    let dir = tempfile::tempdir().expect("fixture");
    let (mut session, _) = saved(dir.path(), "budget 0 dollars");
    std::fs::copy(dir.path().join(WORKFLOW), dir.path().join("other.nika")).expect("separate file");
    let out = session.turn("run other.nika");
    assert!(
        matches!(out, TurnOutcome::RunRequested { ref run, .. } if run.max_cost_usd.to_bits() == 0.25_f64.to_bits()),
        "{out:?}"
    );
    assert_eq!(
        session.monetary_decision().expect("own default").proposal,
        None
    );
}

#[test]
#[cfg(unix)]
fn changed_saved_bytes_refuse_through_an_alias() {
    let dir = tempfile::tempdir().expect("fixture");
    let (mut session, calls) = saved(dir.path(), "budget 0 dollars");
    std::os::unix::fs::symlink(WORKFLOW, dir.path().join("alias.nika")).expect("alias");
    let path = dir.path().join(WORKFLOW);
    let mut bytes = std::fs::read_to_string(&path).expect("saved");
    bytes.push_str("\n# changed after review\n");
    std::fs::write(path, bytes).expect("drift");
    let before = calls.counts();
    assert!(matches!(
        session.turn("run alias.nika"),
        TurnOutcome::Refusal(_)
    ));
    assert_eq!(calls.counts(), before);
    assert_eq!(
        session.monetary_decision().expect("refused").effective_usd,
        None
    );
}

#[test]
fn reopened_saved_constraints_require_explicit_reconfirmation() {
    for restore in [false, true] {
        let dir = tempfile::tempdir().expect("fixture");
        drop(saved(dir.path(), "budget 0 dollars"));
        let (mut session, calls) = open(dir.path());
        if restore {
            assert!(session.restore_state().is_some());
        }
        let input = format!("run {WORKFLOW}");
        let out = session.turn(&input);
        assert!(
            matches!(out, TurnOutcome::Refusal(_)),
            "restore={restore}: {out:?}"
        );
        assert_eq!(calls.counts(), (0, 0));
        assert_eq!(
            session.monetary_decision().expect("unproved").effective_usd,
            None
        );
        assert!(session.pending_proposal().is_none());
        for amount in [0, 2] {
            let out = session.turn(&format!("run {WORKFLOW} budget {amount} USD"));
            assert!(
                matches!(out, TurnOutcome::RunRequested { ref run, .. } if run.max_cost_usd.to_bits() == f64::from(amount).to_bits()),
                "{out:?}"
            );
        }
        assert!(
            matches!(session.turn(&input), TurnOutcome::Refusal(_)),
            "a one-turn override must not prove a saved binding"
        );
    }
}

#[test]
#[cfg(unix)]
fn reopened_alias_cannot_escape_reconfirmation() {
    let dir = tempfile::tempdir().expect("fixture");
    drop(saved(dir.path(), "budget 0 dollars"));
    std::os::unix::fs::symlink(WORKFLOW, dir.path().join("alias.nika")).expect("alias");
    let (mut session, calls) = open(dir.path());
    let out = session.turn("run alias.nika");
    assert!(matches!(out, TurnOutcome::Refusal(_)), "{out:?}");
    assert_eq!(calls.counts(), (0, 0));
}

#[test]
fn unreadable_consent_evidence_does_not_prove_a_missing_constraint() {
    let dir = tempfile::tempdir().expect("fixture");
    drop(saved(dir.path(), "budget 0 dollars"));
    std::fs::write(dir.path().join(".nika/consents.ndjson"), "not json\n").expect("damaged record");
    let (mut session, calls) = open(dir.path());
    let out = session.turn(&format!("run {WORKFLOW}"));
    assert!(matches!(out, TurnOutcome::Refusal(_)), "{out:?}");
    assert_eq!(calls.counts(), (0, 0));
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".nika/consents.ndjson")).expect("kept"),
        "not json\n"
    );
}

#[test]
fn compact_amendments_get_fresh_identity_and_run_overrides_share_the_parser() {
    let dir = tempfile::tempdir().expect("fixture");
    let (mut session, calls) = open(dir.path());
    let TurnOutcome::Proposal { id: old, .. } = session.turn(COPY) else {
        panic!("proposal")
    };
    let TurnOutcome::Proposal { id, .. } = session.consent("budget=0") else {
        panic!("revision")
    };
    assert_ne!(id, old);
    assert_eq!(
        session.monetary_decision().expect("money").original_intent,
        COPY
    );
    assert!(matches!(
        session.consent_to(&old, "yes"),
        TurnOutcome::Refusal(_)
    ));
    assert!(matches!(
        session.consent_to(&id, "yes"),
        TurnOutcome::Facts(_)
    ));
    for (clause, amount) in [
        ("budget=2", 2.0),
        ("budget:0,50", 0.5),
        ("0USD", 0.0),
        ("2USD", 2.0),
    ] {
        let out = session.turn(&format!("run {WORKFLOW} {clause}"));
        assert!(
            matches!(out, TurnOutcome::RunRequested { ref run, .. } if run.max_cost_usd.to_bits() == f64::to_bits(amount)),
            "{out:?}"
        );
    }
    assert_eq!(calls.counts(), (0, 0));
}

#[test]
fn compact_invalid_prepare_and_consent_never_leave_stale_authority() {
    let dir = tempfile::tempdir().expect("fixture");
    let (mut session, calls) = open(dir.path());
    for clause in ["budget=NaN", "budget:inf", "-1USD", "budget=0 budget:2"] {
        let input = format!("Prépare la copie de entree.txt dans sortie.txt, {clause}.");
        assert!(matches!(session.turn(&input), TurnOutcome::Refusal(_)));
        assert_eq!(
            session
                .monetary_decision()
                .expect("refused")
                .original_intent,
            input
        );
        assert!(session.pending_proposal().is_none());
        let TurnOutcome::Proposal { id, .. } = session.turn(COPY) else {
            panic!("proposal")
        };
        assert!(matches!(session.consent(clause), TurnOutcome::Refusal(_)));
        assert!(matches!(
            session.consent_to(&id, "yes"),
            TurnOutcome::Refusal(_)
        ));
        assert!(!dir.path().join(WORKFLOW).exists());
    }
    assert_eq!(calls.counts(), (0, 0));
}

fn rejected_gate_sequence(addressed: bool, start_with_zero: bool) {
    let dir = tempfile::tempdir().expect("fixture");
    let (mut session, calls) = paused(dir.path(), "confirm");
    let gate = session.waiting_gate().expect("gate");
    let answer = |session: &mut SessionRuntime, input: &str| {
        if addressed {
            session.answer_gate_for(&gate, input)
        } else {
            session.answer_gate(input)
        }
    };
    if start_with_zero {
        assert!(matches!(
            answer(&mut session, "yes but budget 0 dollars"),
            TurnOutcome::Refusal(_)
        ));
        assert_eq!(
            session.monetary_decision().expect("zero").effective_usd,
            Some(0.0)
        );
    }
    let invalid = "yes but budget=NaN";
    assert!(matches!(
        answer(&mut session, invalid),
        TurnOutcome::Refusal(_)
    ));
    let rejected = session.monetary_decision().expect("rejected").clone();
    assert_eq!(rejected.input, invalid);
    assert_eq!(rejected.source, MonetarySource::Rejected);
    assert_eq!(rejected.effective_usd, None);
    assert_eq!(rejected.inference, InferenceEnforcement::CallsBlocked);
    assert_eq!(calls.counts(), (0, 0));
    for question in [
        "what does this permit?",
        "why?",
        "could you explain the next task?",
    ] {
        let out = answer(&mut session, question);
        assert_eq!(
            calls.counts(),
            (0, 0),
            "addressed={addressed}, zero={start_with_zero}, {question}: {out:?}"
        );
        assert!(matches!(
            out,
            TurnOutcome::Refusal(_) | TurnOutcome::Aside(_)
        ));
        assert_eq!(session.waiting_gate().as_ref(), Some(&gate));
        assert_eq!(session.monetary_decision(), Some(&rejected));
        assert!(session.pending_proposal().is_none());
    }
    assert!(session.status().contains("money: refused"));
    // A valid monetary replacement changes the observation, never grants
    // Resume or paid interpretation while the inference seam is unmetered.
    assert!(matches!(
        answer(&mut session, "budget 2 USD"),
        TurnOutcome::Refusal(_)
    ));
    assert_eq!(
        session
            .monetary_decision()
            .expect("replacement")
            .effective_usd,
        Some(2.0)
    );
    assert!(matches!(
        answer(&mut session, "what does this permit?"),
        TurnOutcome::Refusal(_)
    ));
    assert_eq!(calls.counts(), (0, 0));
    assert_eq!(session.waiting_gate().as_ref(), Some(&gate));
    // A deliberate protocol answer still decides the gate; a later new turn
    // admits its own default, rather than inheriting a rejected amendment.
    assert!(matches!(
        answer(&mut session, "no"),
        TurnOutcome::ResumeRequested { .. }
    ));
    assert!(session.waiting_gate().is_none());
    let fresh = "What can you tell me about stars?";
    assert!(matches!(session.turn(fresh), TurnOutcome::Reply(_)));
    assert_eq!(calls.counts(), (0, 1));
    let money = session.monetary_decision().expect("fresh work");
    assert_eq!(money.original_intent, fresh);
    assert_eq!(money.source, MonetarySource::SessionDefault);
    assert_eq!(money.inference, InferenceEnforcement::NotMetered);
}

#[test]
fn rejected_gate_continuations_stay_guarded() {
    rejected_gate_sequence(false, false);
    rejected_gate_sequence(false, true);
}

#[test]
fn addressed_rejected_gate_continuations_stay_guarded() {
    rejected_gate_sequence(true, false);
    rejected_gate_sequence(true, true);
}

#[test]
fn malformed_attached_currency_is_rejected_not_reclassified_as_data() {
    for clause in [
        "budget=0.5oopsUSD",
        "budget:0.5oopsUSD",
        "budget=0,5oopsUSD",
        "budget=0.5oopsusd",
        "budget=NaN.fooUSD",
        "budget=0.txtUSD",
        "0.5oopsUSD",
        "budget=0.5oopsdollars",
        "budget 0.5oopsUSD",
    ] {
        for request in [
            "What can you tell me about stars,",
            "Prépare la copie de entree.txt dans sortie.txt,",
        ] {
            let dir = tempfile::tempdir().expect("fixture");
            let (mut session, calls) = open(dir.path());
            let input = format!("{request} {clause}?");
            let out = session.turn(&input);
            assert_eq!(calls.counts(), (0, 0), "{input}: {out:?}");
            assert!(matches!(out, TurnOutcome::Refusal(_)), "{input}: {out:?}");
            let money = session.monetary_decision().expect("rejected");
            assert_eq!(money.original_intent, input);
            assert_eq!(money.input, input);
            assert_eq!(money.source, MonetarySource::Rejected);
            assert_eq!(money.effective_usd, None);
            assert_eq!(money.inference, InferenceEnforcement::CallsBlocked);
            assert!(session.pending_proposal().is_none());
            assert!(session.pending_question().is_none());
            assert!(!dir.path().join(".nika").exists());
        }
    }
}

#[test]
fn attached_currency_validation_keeps_decimal_and_data_controls() {
    for (clause, amount, expected_calls) in [
        ("budget=0.5USD", 0.5, 0),
        ("budget:0,50USD", 0.5, 0),
        ("budget=0USD", 0.0, 0),
        ("2USD", 2.0, 0),
        ("budget=0.txt", 0.25, 1),
        ("0.5oopsUSD.txt", 0.25, 1),
        ("\"budget=0.5oopsUSD\"", 0.25, 1),
        ("`budget=0.5oopsUSD`", 0.25, 1),
        ("./budget=0.5oopsUSD", 0.25, 1),
        ("./budget/0.5oopsUSD", 0.25, 1),
        ("at 9:00 for 9 files", 0.25, 1),
    ] {
        let dir = tempfile::tempdir().expect("fixture");
        let (mut session, calls) = open(dir.path());
        let input = format!("What can you tell me about stars and {clause}?");
        session.turn(&input);
        let money = session.monetary_decision().expect("money or data");
        assert_eq!(money.original_intent, input);
        assert_eq!(money.effective_usd, Some(amount), "{input}");
        assert_eq!(calls.counts(), (0, expected_calls), "{input}");
    }
}
