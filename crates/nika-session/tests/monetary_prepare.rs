// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Public Session mechanics, not live qualification. The input is submitted
//! unchanged; protocol doubles count attempted cognition, never invent money.
#![allow(clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use nika_session::reasoner::NoReasoner;
use nika_session::turn::{RoutingMethod, TurnAct, TurnClassifier, TurnContext, TurnDecision};
use nika_session::{
    IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence, SessionRuntime, TurnOutcome,
    UserIntelligencePreference,
};

struct Calls(Arc<AtomicUsize>);

impl TurnClassifier for Calls {
    fn classify(&mut self, _: &TurnContext, _: &str) -> TurnDecision {
        self.0.fetch_add(1, Ordering::SeqCst);
        TurnDecision::new(TurnAct::NewWork, RoutingMethod::Fallback)
    }
}

fn session(dir: &std::path::Path) -> (SessionRuntime, Arc<AtomicUsize>) {
    let selected = ResolvedSessionIntelligence::resolve(
        &UserIntelligencePreference::new(IntelligenceKind::None, None),
        &IntelligenceCensus::empty(),
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let mut runtime = SessionRuntime::open(dir, selected, Box::new(NoReasoner));
    runtime.with_classifier(Box::new(Calls(calls.clone())));
    (runtime, calls)
}

fn invalid_prepare(clause: &str) {
    let dir = tempfile::tempdir().expect("fixture");
    std::fs::write(dir.path().join("entree.txt"), "A\n").expect("input");
    let (mut runtime, calls) = session(dir.path());
    let input = format!("Prépare la copie de entree.txt dans sortie.txt, {clause}");
    let out = runtime.turn(&input);
    assert!(
        matches!(&out, TurnOutcome::Refusal(r) if r.text.contains("finite")),
        "{input}: {out:?}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0, "before classification");
    assert!(runtime.pending_proposal().is_none());
    assert!(runtime.pending_question().is_none());
    assert!(!dir.path().join("sortie.txt").exists());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("entree.txt")).expect("input"),
        "A\n"
    );
}

#[test]
fn budget_03_prepare_nan_refuses_before_cognition() {
    invalid_prepare("budget NaN dollars.");
}

#[test]
fn budget_04_prepare_infinity_refuses_before_cognition() {
    invalid_prepare("budget inf dollars.");
}

#[test]
fn budget_05_prepare_negative_refuses_before_cognition() {
    invalid_prepare("budget -1 dollar.");
}

use nika_session::money::{CapKnowledge, InferenceEnforcement, MonetarySource};

/// Original Prepare population, unchanged, with a deterministic None seat.
/// BUDGET-10 below additionally uses a subscription protocol double; neither
/// run is a live subscription qualification.
#[test]
fn all_ten_original_prepare_clauses_report_actual_money_without_execution() {
    for (clause, value, source) in [
        ("à 9 heures.", Some(1.0), MonetarySource::ProjectDefault),
        (
            "pour 9 fichiers.",
            Some(1.0),
            MonetarySource::ProjectDefault,
        ),
        ("budget NaN dollars.", None, MonetarySource::Rejected),
        ("budget inf dollars.", None, MonetarySource::Rejected),
        ("budget -1 dollar.", None, MonetarySource::Rejected),
        ("budget 0 dollar.", Some(0.0), MonetarySource::Override),
        ("budget 0,50 dollar.", Some(0.5), MonetarySource::Override),
        (
            "budget 2 dollars, remplace explicitement mon défaut de 1 dollar.",
            Some(2.0),
            MonetarySource::Override,
        ),
        (
            "sans fournisseur configuré.",
            Some(1.0),
            MonetarySource::ProjectDefault,
        ),
        (
            "avec mon abonnement configuré dont le coût est inconnu.",
            Some(1.0),
            MonetarySource::ProjectDefault,
        ),
    ] {
        let dir = tempfile::tempdir().expect("fixture");
        std::fs::write(dir.path().join("entree.txt"), "A\n").expect("fixture");
        std::fs::write(dir.path().join("nika.yaml"), "nika: fixture\nceiling: 1\n")
            .expect("project");
        let (mut runtime, calls) = session(dir.path());
        let input = format!("Prépare la copie de entree.txt dans sortie.txt, {clause}");
        let out = runtime.turn(&input);
        let money = runtime.monetary_decision().expect("actual admission");
        assert_eq!(money.input, input);
        assert_eq!(money.effective_usd, value, "{input}: {out:?}");
        assert_eq!(money.source, source);
        assert_eq!(money.project_default_usd, Some(1.0));
        assert_eq!(
            money.project_file.as_deref(),
            Some(dir.path().join("nika.yaml").as_path())
        );
        assert!(
            matches!(&money.policy_cap, CapKnowledge::Unknown { reason } if !reason.is_empty())
        );
        assert!(
            matches!(&money.machine_cap, CapKnowledge::Unknown { reason } if !reason.is_empty())
        );
        assert_eq!(money.observed_cost_usd, None);
        assert_eq!(calls.load(Ordering::SeqCst), 0, "{input}");
        assert!(!matches!(out, TurnOutcome::RunRequested { .. }));
        assert!(!dir.path().join("sortie.txt").exists());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("entree.txt")).expect("input"),
            "A\n"
        );
        assert_eq!(runtime.intelligence.kind, IntelligenceKind::None);
    }
}

#[test]
fn comma_zero_and_unset_are_distinct_and_intent_is_verbatim() {
    let dir = tempfile::tempdir().expect("fixture");
    let (mut runtime, _) = session(dir.path());
    for (input, expected, source, literal) in [
        (
            "  Prépare la copie de entree.txt dans sortie.txt, budget 0,50 dollar.  ",
            0.5,
            MonetarySource::Explicit,
            Some("0,50"),
        ),
        (
            "Prépare la copie de entree.txt dans sortie.txt, budget 0 dollar.",
            0.0,
            MonetarySource::Explicit,
            Some("0"),
        ),
        (
            "Prépare la copie de entree.txt dans sortie.txt, pour 9 fichiers.",
            0.25,
            MonetarySource::SessionDefault,
            None,
        ),
    ] {
        runtime.turn(input);
        let money = runtime.monetary_decision().expect("admitted");
        assert_eq!(money.input, input);
        assert_eq!(money.effective_usd, Some(expected));
        assert_eq!(money.source, source);
        assert_eq!(money.explicit_amount.as_deref(), literal);
        assert!(runtime.status().contains("money:"));
        assert!(
            matches!(runtime.turn("/meaning"), TurnOutcome::Aside(ref view) if view.contains("money:"))
        );
    }
}

#[test]
fn monetary_decoys_keep_defaults_but_conflicts_and_malformed_money_refuse() {
    let dir = tempfile::tempdir().expect("fixture");
    let (mut runtime, _) = session(dir.path());
    for clause in [
        "à 9:00 pour 9 fichiers",
        "avec la donnée \"budget NaN dollars\"",
        "avec la donnée 'budget -1 dollar'",
        "avec la donnée `budget inf dollars`",
        "avec la donnée « budget 999 dollars »",
        "avec le chemin ./budget/NaN.txt et ./9/$200/data.txt",
        "avec le chemin 9.txt",
    ] {
        let line = format!("Prépare la copie de entree.txt dans sortie.txt, {clause}");
        runtime.turn(&line);
        assert_eq!(
            runtime.monetary_decision().expect("observed").effective_usd,
            Some(0.25),
            "{line}"
        );
    }
    for clause in [
        "budget 1 dollar et plafond 2 dollars",
        "budget 0,50 dollar et 2 USD",
        "budget -0,50 dollar",
        "budget 1e999 dollars",
        "budget infinity dollars",
        "budget 1,2,3 dollars",
        "budget",
        "budget nope dollars",
        "budget 2 dollars, remplace explicitement mon défaut de 1 dollar",
    ] {
        let line = format!("Prépare la copie de entree.txt dans sortie.txt, {clause}");
        assert!(
            matches!(runtime.turn(&line), TurnOutcome::Refusal(_)),
            "{line}"
        );
        assert_eq!(
            runtime.monetary_decision().expect("observed").effective_usd,
            None
        );
        assert!(runtime.pending_proposal().is_none());
    }
}

#[test]
fn invalid_project_defaults_are_not_silently_absent() {
    for value in ["-1", "0", ".nan", ".inf", "\"0.50\""] {
        let dir = tempfile::tempdir().expect("fixture");
        std::fs::write(
            dir.path().join("nika.yaml"),
            format!("nika: fixture\nceiling: {value}\n"),
        )
        .expect("project");
        let (mut runtime, calls) = session(dir.path());
        assert!(runtime.snapshot.project_error.is_some());
        assert!(matches!(
            runtime.turn("Prépare la copie de entree.txt dans sortie.txt, à 9 heures."),
            TurnOutcome::Refusal(_)
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            runtime.monetary_decision().expect("refused").source,
            MonetarySource::Rejected
        );
    }
}

struct ReasonCalls(Arc<AtomicUsize>);
impl nika_session::SessionReasoner for ReasonCalls {
    fn name(&self) -> String {
        "subscription protocol double".to_owned()
    }
    fn reason(&mut self, prompt: &str) -> Result<nika_session::Reply, nika_session::ReasonError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        nika_session::ScriptedReasoner::new(vec!["unpriced reply".to_owned()]).reason(prompt)
    }
}

#[test]
fn zero_and_invalid_money_call_neither_classifier_nor_selected_reasoner() {
    let dir = tempfile::tempdir().expect("fixture");
    for input in [
        "Prépare la copie de entree.txt dans sortie.txt, budget NaN dollars.",
        "Prépare la copie de entree.txt dans sortie.txt, budget inf dollars.",
        "Prépare la copie de entree.txt dans sortie.txt, budget -1 dollar.",
        "Prépare la copie de entree.txt dans sortie.txt, budget 0 dollar.",
        "Prépare la copie de entree.txt dans sortie.txt, budget 0,50 dollar.",
        "Prépare la copie de entree.txt dans sortie.txt, budget 2 dollars.",
        "Invent an entirely novel automation, budget 0 dollars",
    ] {
        let (mut runtime, classifier) = session(dir.path());
        // Public resolved intelligence is mutable for host observations. This
        // is explicitly a protocol double, never a real subscription fixture.
        runtime.intelligence.kind = IntelligenceKind::Harness {
            seat: "test-subscription".to_owned(),
        };
        let calls = Arc::new(AtomicUsize::new(0));
        let mut runtime = SessionRuntime::open(
            dir.path(),
            runtime.intelligence,
            Box::new(ReasonCalls(calls.clone())),
        );
        runtime.with_classifier(Box::new(Calls(classifier.clone())));
        runtime.turn(input);
        assert_eq!(calls.load(Ordering::SeqCst), 0, "{input}");
        assert_eq!(classifier.load(Ordering::SeqCst), 0, "{input}");
        assert_eq!(
            runtime.monetary_decision().expect("decision").inference,
            InferenceEnforcement::CallsBlocked
        );
        assert!(matches!(
            runtime.intelligence.kind,
            IntelligenceKind::Harness { .. }
        ));
    }
}

#[test]
fn budget_10_subscription_protocol_keeps_unknown_cost_unknown() {
    let dir = tempfile::tempdir().expect("fixture");
    let (runtime, _) = session(dir.path());
    let mut selected = runtime.intelligence;
    selected.kind = IntelligenceKind::Harness {
        seat: "test-subscription".to_owned(),
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let mut runtime = SessionRuntime::open(dir.path(), selected, Box::new(ReasonCalls(calls)));
    runtime.turn("Prépare la copie de entree.txt dans sortie.txt, avec mon abonnement configuré dont le coût est inconnu.");
    assert_eq!(
        runtime
            .monetary_decision()
            .expect("decision")
            .observed_cost_usd,
        None
    );
    assert!(matches!(
        runtime.intelligence.kind,
        IntelligenceKind::Harness { .. }
    ));
    assert!(runtime.status().contains("billed cost unknown"));
}

const COPY: &str = "Read ./entree.txt and write it to ./sortie.txt";
const WORKFLOW: &str = "compiled-workflow.nika";

#[test]
fn prepared_default_is_bound_to_proposal_and_saved_bytes_not_the_next_project_default() {
    let dir = tempfile::tempdir().expect("fixture");
    std::fs::write(dir.path().join("entree.txt"), "A\n").expect("fixture");
    std::fs::write(dir.path().join("nika.yaml"), "nika: fixture\nceiling: 1\n").expect("project");
    let (mut runtime, _) = session(dir.path());
    let TurnOutcome::Proposal { id, .. } = runtime.turn(COPY) else {
        panic!("real deterministic candidate")
    };
    assert_eq!(
        runtime
            .monetary_decision()
            .expect("money")
            .proposal
            .as_ref(),
        Some(&id)
    );
    assert!(matches!(runtime.consent("yes"), TurnOutcome::Facts(_)));
    runtime.snapshot.ceiling = Some(0.1);
    let TurnOutcome::RunRequested { run, .. } = runtime.turn("run it") else {
        panic!("run requested separately")
    };
    assert_eq!(run.max_cost_usd.to_bits(), 1.0_f64.to_bits());
    assert_eq!(
        runtime
            .monetary_decision()
            .expect("money")
            .proposal
            .as_ref(),
        Some(&id)
    );
    assert!(!dir.path().join("sortie.txt").exists());
    let path = dir.path().join(WORKFLOW);
    let mut bytes = std::fs::read_to_string(&path).expect("saved");
    bytes.push_str("\n# changed outside the reviewed proposal\n");
    std::fs::write(path, bytes).expect("drift");
    assert!(
        matches!(runtime.turn("run it"), TurnOutcome::Refusal(_)),
        "old budget cannot attach to different bytes"
    );
}

#[test]
fn monetary_revision_has_new_consent_identity_and_carries_explicit_money_through_save_run() {
    let dir = tempfile::tempdir().expect("fixture");
    let (mut runtime, calls) = session(dir.path());
    let TurnOutcome::Proposal { id: old, .. } = runtime.turn(COPY) else {
        panic!("candidate")
    };
    let TurnOutcome::Proposal { id, preview } = runtime.consent("budget 0,50 dollars") else {
        panic!("monetary revision")
    };
    assert_ne!(id, old);
    assert!(preview.contains("$0.5 USD"));
    let money = runtime.monetary_decision().expect("money");
    assert_eq!(money.original_intent, COPY);
    assert_eq!(money.input, "budget 0,50 dollars");
    assert_eq!(money.effective_usd, Some(0.5));
    assert_eq!(money.proposal.as_ref(), Some(&id));
    assert_eq!(money.inference, InferenceEnforcement::CallsBlocked);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "money-only amendment calls no classifier"
    );
    assert!(matches!(
        runtime.consent_to(&old, "yes"),
        TurnOutcome::Refusal(_)
    ));
    assert!(matches!(
        runtime.consent_to(&id, "yes"),
        TurnOutcome::Facts(_)
    ));
    let out = runtime.turn("run it");
    assert!(
        matches!(out, TurnOutcome::RunRequested { ref run, .. } if run.max_cost_usd.to_bits() == 0.5_f64.to_bits()),
        "{out:?}"
    );
    let out = runtime.turn("run it budget 2 USD");
    assert!(
        matches!(out, TurnOutcome::RunRequested { ref run, .. } if run.max_cost_usd.to_bits() == 2.0_f64.to_bits()),
        "{out:?}"
    );
}

#[test]
fn changing_default_changes_proposal_identity_even_with_identical_workflow_bytes() {
    let dir = tempfile::tempdir().expect("fixture");
    let (mut runtime, _) = session(dir.path());
    runtime.snapshot.ceiling = Some(1.0);
    let TurnOutcome::Proposal { id: old, .. } = runtime.turn(COPY) else {
        panic!("candidate")
    };
    runtime.snapshot.ceiling = Some(2.0);
    let TurnOutcome::Proposal { id, .. } = runtime.turn(COPY) else {
        panic!("fresh candidate")
    };
    assert_ne!(id, old);
    assert!(matches!(
        runtime.consent_to(&old, "yes"),
        TurnOutcome::Refusal(_)
    ));
    assert!(!dir.path().join(WORKFLOW).exists());
}

#[test]
fn invalid_amendment_expires_old_proposal_and_new_work_does_not_inherit_it() {
    let dir = tempfile::tempdir().expect("fixture");
    let (mut runtime, calls) = session(dir.path());
    let TurnOutcome::Proposal { id, .. } = runtime.turn(COPY) else {
        panic!("proposal")
    };
    assert!(matches!(
        runtime.consent("yes but budget NaN dollars"),
        TurnOutcome::Refusal(_)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(runtime.pending_proposal().is_none());
    assert!(matches!(
        runtime.consent_to(&id, "yes"),
        TurnOutcome::Refusal(_)
    ));
    assert!(!dir.path().join(WORKFLOW).exists());
    runtime.turn("Prépare la copie de entree.txt dans sortie.txt, budget 0 dollar.");
    assert_eq!(
        runtime.monetary_decision().expect("zero").effective_usd,
        Some(0.0)
    );
    runtime.turn(COPY);
    assert_eq!(
        runtime.monetary_decision().expect("fresh").effective_usd,
        Some(0.25)
    );
}

#[test]
fn run_uses_the_same_currency_reader_and_still_refuses_unbound_numeric_conditions() {
    let dir = tempfile::tempdir().expect("fixture");
    let (mut runtime, _) = session(dir.path());
    runtime.turn(COPY);
    runtime.consent("yes");
    for input in [
        "run it at 9",
        "run it 9 times",
        "run it budget NaN dollars",
        "run it budget 1 dollar and 2 USD",
    ] {
        assert!(
            matches!(runtime.turn(input), TurnOutcome::Refusal(_)),
            "{input}"
        );
    }
    for input in [
        "run it budget 0,50 dollar",
        "run it with a ceiling of $0.50 usd",
        "run it --max-cost-usd=0.5",
    ] {
        let out = runtime.turn(input);
        assert!(
            matches!(out, TurnOutcome::RunRequested { ref run, .. } if run.max_cost_usd.to_bits() == 0.5_f64.to_bits()),
            "{input}: {out:?}"
        );
    }
}

#[test]
fn a_saved_explicit_ceiling_still_blocks_cognition_on_a_qualified_run() {
    let dir = tempfile::tempdir().expect("fixture");
    let (mut runtime, calls) = session(dir.path());
    runtime.turn(COPY);
    runtime.consent("budget 0 dollars");
    runtime.consent("yes");
    let outcome = runtime.turn("run it but only on Fridays");
    assert!(!matches!(outcome, TurnOutcome::RunRequested { .. }));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        runtime
            .monetary_decision()
            .expect("bound money")
            .effective_usd,
        Some(0.0)
    );
}

#[test]
fn an_invalid_amendment_writes_no_project_state_or_old_consent() {
    let dir = tempfile::tempdir().expect("fixture");
    let (mut runtime, _) = session(dir.path());
    runtime.turn(COPY);
    let outcome = runtime.consent("budget -1 dollar");
    assert!(matches!(outcome, TurnOutcome::Refusal(_)));
    assert!(runtime.pending_proposal().is_none());
    assert_eq!(
        std::fs::read_dir(dir.path())
            .expect("unchanged tree")
            .count(),
        0
    );
}
