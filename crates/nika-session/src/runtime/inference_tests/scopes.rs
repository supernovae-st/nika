// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Gate-local decisions and Run observations cannot replace a Session allowance.
//! Loopback protocol evidence only: these tests neither run workflows nor bill providers.
use super::*;
use crate::money::MonetarySource;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct LegacyCalls(Arc<AtomicUsize>);
impl SessionReasoner for LegacyCalls {
    fn name(&self) -> String {
        "counting unmetered fixture".into()
    }
    fn reason(&mut self, prompt: &str) -> Result<crate::Reply, crate::ReasonError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        crate::ScriptedReasoner::new(vec!["controlled unmetered reply".into()]).reason(prompt)
    }
}

fn pause(s: &mut SessionRuntime, root: &Path) -> GateId {
    s.pending_gate = Some(PendingGate {
        workflow: "w.nika".into(),
        trace: root.join("paused.ndjson"),
        task: "approve".into(),
        mode: "confirm".into(),
        message: "Proceed?".into(),
    });
    s.waiting_gate().expect("pending gate")
}

fn answer(s: &mut SessionRuntime, gate: &GateId, addressed: bool, line: &str) -> TurnOutcome {
    if addressed {
        s.answer_gate_for(gate, line)
    } else {
        s.answer_gate(line)
    }
}

fn amendments(s: &mut SessionRuntime, gate: &GateId, addressed: bool) {
    for line in [
        "yes but budget NaN USD",
        "what does this permit?",
        "budget 2 USD",
        "what next?",
    ] {
        let out = answer(s, gate, addressed, line);
        assert!(matches!(out, TurnOutcome::Refusal(_)), "{line}: {out:?}");
        assert_eq!(s.waiting_gate().as_ref(), Some(gate));
        assert!(s.money_blocks_cognition());
    }
    assert_eq!(s.monetary_decision().unwrap().effective_usd, Some(2.0));
}

#[test]
fn gate_completion_preserves_prior_zero_rejected_and_reconfirmation_guards() {
    let peer = Peer::start(vec![(200, response("unexpected"))]);
    let _transport = test_transport::install(&peer.url);
    for addressed in [false, true] {
        for account_first in [false, true] {
            for prior in ["budget 0 USD", "budget NaN USD", "reconfirm"] {
                let dir = tempfile::tempdir().unwrap();
                let mut s = open(dir.path());
                if account_first {
                    s.admit_money("budget 3 USD", false).unwrap();
                    s.reason_with_money("charge before revocation", false)
                        .unwrap();
                }
                let count = peer.bodies().len();
                if prior == "reconfirm" {
                    s.money.reconfirm = true;
                } else {
                    let _ = s.admit_money(prior, false);
                }
                let receipt = s.inference_receipt().unwrap();
                let draft = s.money.draft.clone();
                let decisions = s.intent.decisions.clone();
                let reconfirm = s.money.reconfirm;
                let gate = pause(&mut s, dir.path());
                amendments(&mut s, &gate, addressed);
                assert_eq!(s.inference_receipt().unwrap(), receipt);
                assert_eq!(s.money.draft, draft);
                // Even another ordinary turn cannot use a paused-gate amendment
                // to open/reprice the preexisting Session account.
                let _ = s.turn("what next, budget 4 USD?");
                assert_eq!(s.inference_receipt().unwrap(), receipt);
                assert_eq!(s.money.draft, draft);
                assert!(matches!(
                    answer(&mut s, &gate, addressed, "no"),
                    TurnOutcome::ResumeRequested { .. }
                ));
                assert_eq!(s.inference_receipt().unwrap(), receipt);
                assert_eq!(s.money.draft, draft);
                assert_eq!(s.money.reconfirm, reconfirm);
                assert!(decisions.iter().all(|d| s.intent.decisions.contains(d)));
                assert!(
                    s.money_blocks_cognition(),
                    "{prior}, account={account_first}"
                );
                assert!(
                    s.reason_with_money("explain after the gate", false)
                        .is_err()
                );
                // Changing the intelligence must not reach its legacy seam either.
                s.reasoner = Box::new(UnmeteredReasoner);
                s.refresh_seat();
                assert!(
                    s.reason_with_money("explain with another model", false)
                        .is_err()
                );
                assert_eq!(peer.bodies().len(), count);
            }
        }
    }
}

#[test]
fn uncertain_account_and_shared_handle_survive_gate_completion_and_run_default() {
    let mut unknown = response("unknown charge");
    unknown.as_object_mut().unwrap().remove("usage");
    let peer = Peer::start(vec![(200, unknown)]);
    let _transport = test_transport::install(&peer.url);
    for addressed in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let mut s = open(dir.path());
        s.admit_money("budget 3 USD", false).unwrap();
        let account = s.money.account.clone().unwrap();
        assert!(s.reason_with_money("hello", false).is_err());
        let receipt = s.inference_receipt().unwrap();
        assert_eq!(receipt.as_ref().unwrap().state, AdmissionState::Uncertain);
        assert!(receipt.as_ref().unwrap().held_unknown.nano_usd > 0);
        let count = peer.bodies().len();
        let gate = pause(&mut s, dir.path());
        amendments(&mut s, &gate, addressed);
        assert!(matches!(
            answer(&mut s, &gate, addressed, "no"),
            TurnOutcome::ResumeRequested { .. }
        ));
        std::fs::write(dir.path().join("independent.nika"), "identity fixture").unwrap();
        assert_eq!(
            s.run_money("run independent.nika", Path::new("independent.nika"), None)
                .unwrap()
                .to_bits(),
            0.25_f64.to_bits()
        );
        assert_eq!(s.inference_receipt().unwrap(), receipt);
        assert!(s.reason_with_money("after Run", false).is_err());
        assert_eq!(peer.bodies().len(), count);
        account.close("shared original account").unwrap();
        assert_eq!(
            s.inference_receipt().unwrap().unwrap(),
            account.snapshot().unwrap()
        );
    }
}

#[test]
fn independent_run_default_keeps_zero_guard_and_saved_identity_separate() {
    let peer = Peer::start(vec![(200, response("unexpected"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    assert!(matches!(
        s.turn("Read ./entree.txt and write it to ./sortie.txt"),
        TurnOutcome::Proposal { .. }
    ));
    assert!(matches!(
        s.consent("budget 0 USD"),
        TurnOutcome::Proposal { .. }
    ));
    assert!(matches!(s.consent("yes"), TurnOutcome::Facts(_)));
    let saved = s.monetary_decision().unwrap().proposal.clone();
    let guard = s.money.inference_guard.clone();
    std::fs::copy(
        dir.path().join("compiled-workflow.nika"),
        dir.path().join("independent.nika"),
    )
    .unwrap();
    for ceiling in [None, Some(0.75)] {
        s.snapshot.ceiling = ceiling;
        assert!(
            matches!(s.turn("run independent.nika"), TurnOutcome::RunRequested { ref run, .. } if run.max_cost_usd.to_bits() == ceiling.unwrap_or(0.25).to_bits())
        );
        assert_eq!(s.monetary_decision().unwrap().proposal, None);
        assert_eq!(
            s.monetary_decision().unwrap().inference,
            InferenceEnforcement::CallsBlocked
        );
        assert_eq!(s.money.inference_guard, guard);
        assert!(s.money_blocks_cognition());
        assert!(s.reason_with_money("explain after Run", false).is_err());
    }
    assert!(
        matches!(s.turn("run compiled-workflow.nika"), TurnOutcome::RunRequested { ref run, .. } if run.max_cost_usd.to_bits() == 0.0_f64.to_bits())
    );
    assert_eq!(s.monetary_decision().unwrap().proposal, saved);
    assert_eq!(s.money.inference_guard, guard);
    assert!(peer.bodies().is_empty());
    assert!(!dir.path().join("sortie.txt").exists());
}

#[test]
fn restoring_an_unmetered_saved_run_never_replaces_a_later_session_refusal() {
    let peer = Peer::start(vec![(200, response("unexpected"))]);
    let _transport = test_transport::install(&peer.url);
    for prior in ["budget 0 USD", "budget NaN USD"] {
        let dir = tempfile::tempdir().unwrap();
        let mut s = open(dir.path());
        s.turn("Read ./entree.txt and write it to ./sortie.txt");
        assert!(matches!(s.consent("yes"), TurnOutcome::Facts(_)));
        let _ = s.admit_money(prior, false);
        let guard = s.money.inference_guard.clone();
        assert!(
            matches!(s.turn("run compiled-workflow.nika"), TurnOutcome::RunRequested { ref run, .. } if run.max_cost_usd.to_bits() == 0.25_f64.to_bits())
        );
        assert_eq!(s.money.inference_guard, guard);
        assert!(s.money_blocks_cognition());
        assert!(s.reason_with_money("explain after Run", false).is_err());
        assert!(peer.bodies().is_empty());
    }
}

#[test]
fn a_gate_only_amendment_ends_without_creating_a_session_allowance() {
    let peer = Peer::start(vec![(200, response("hello"))]);
    let _transport = test_transport::install(&peer.url);
    for addressed in [false, true] {
        for token in ["yes", "no"] {
            let dir = tempfile::tempdir().unwrap();
            let mut s = open(dir.path());
            let gate = pause(&mut s, dir.path());
            let count = peer.bodies().len();
            amendments(&mut s, &gate, addressed);
            assert!(s.inference_receipt().unwrap().is_none());
            let stale = GateId::new(&dir.path().join("other.ndjson"), "approve");
            assert!(matches!(
                s.answer_gate_for(&stale, token),
                TurnOutcome::Refusal(_)
            ));
            assert!(s.money_blocks_cognition());
            for aside in ["why?", "", "/quit"] {
                assert!(!matches!(
                    answer(&mut s, &gate, addressed, aside),
                    TurnOutcome::ResumeRequested { .. }
                ));
                assert!(s.money_blocks_cognition());
                assert_eq!(s.waiting_gate().as_ref(), Some(&gate));
            }
            assert!(matches!(
                answer(&mut s, &gate, addressed, token),
                TurnOutcome::ResumeRequested { .. }
            ));
            assert!(!s.money_blocks_cognition());
            assert!(s.inference_receipt().unwrap().is_none());
            assert!(s.money.inference_guard.is_none());
            assert!(
                !s.intent
                    .decisions
                    .iter()
                    .any(|d| d == super::super::inference::RECONFIRM)
            );
            assert_eq!(peer.bodies().len(), count);
            // The HTTP substitution deliberately accepts bounded calls only.
            // Prove the restored legacy seam with a counting custom reasoner;
            // do not weaken its endpoint/redirect assertions for an unmetered call.
            let calls = Arc::new(AtomicUsize::new(0));
            s.reasoner = Box::new(LegacyCalls(calls.clone()));
            s.refresh_seat();
            assert!(matches!(
                s.turn("What can you tell me about stars?"),
                TurnOutcome::Reply(_)
            ));
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert_eq!(peer.bodies().len(), count);
            let d = s.monetary_decision().unwrap();
            assert_eq!(d.source, MonetarySource::SessionDefault);
            assert_eq!(d.inference, InferenceEnforcement::NotMetered);
            assert_eq!(d.effective_usd, Some(0.25));
        }
    }
}

#[test]
fn gate_completion_does_not_restore_authority_expired_by_a_monetary_refusal() {
    let peer = Peer::start(vec![(200, response("unexpected"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    let out = s.turn("Read ./entree.txt and write it to ./sortie.txt");
    let TurnOutcome::Proposal { id, .. } = out else {
        panic!("deterministic proposal: {out:?}");
    };
    let gate = pause(&mut s, dir.path());
    amendments(&mut s, &gate, true);
    assert!(s.pending_proposal().is_none());
    assert!(matches!(s.consent_to(&id, "yes"), TurnOutcome::Refusal(_)));
    assert!(matches!(
        s.answer_gate_for(&gate, "no"),
        TurnOutcome::ResumeRequested { .. }
    ));
    assert!(s.pending_proposal().is_none());
    assert!(matches!(s.consent_to(&id, "yes"), TurnOutcome::Refusal(_)));
    assert!(!dir.path().join("compiled-workflow.nika").exists());
    assert!(!dir.path().join("sortie.txt").exists());
    assert!(peer.bodies().is_empty());
}

#[test]
fn fresh_prepare_defaults_never_replace_a_session_inference_constraint() {
    for prior in ["zero", "rejected", "spent", "unknown"] {
        let mut body = response("hello");
        if prior == "unknown" {
            body.as_object_mut().unwrap().remove("usage");
        }
        let peer = Peer::start(vec![(200, body)]);
        let _transport = test_transport::install(&peer.url);
        let dir = tempfile::tempdir().unwrap();
        let mut s = open(dir.path());
        match prior {
            "zero" => s.admit_money("budget 0 USD", false).unwrap(),
            "rejected" => assert!(s.admit_money("budget NaN USD", false).is_err()),
            _ => {
                s.admit_money("budget 3 USD", false).unwrap();
                let result = s.reason_with_money("charge before new work", false);
                assert_eq!(result.is_ok(), prior == "spent");
            }
        }
        let guard = s.money.inference_guard.clone();
        let receipt = s.inference_receipt().unwrap();
        let before = peer.bodies().len();
        let copy = "Read ./entree.txt and write it to ./sortie.txt";
        let TurnOutcome::Proposal { id, preview } = s.turn(copy) else {
            panic!("fresh deterministic proposal for {prior}");
        };
        let money = s.monetary_decision().unwrap();
        assert_eq!(money.effective_usd, Some(0.25));
        assert_eq!(money.original_intent, copy);
        assert_eq!(money.source, MonetarySource::SessionDefault);
        assert!(preview.contains("proposal/Run ceiling"));
        assert!(preview.contains("Session inference:"));
        match prior {
            "zero" => assert!(s.status().contains("catalog allowance is zero")),
            "rejected" => assert!(
                s.status()
                    .contains("earlier Session monetary admission refused")
            ),
            "spent" => assert!(preview.contains(&format!(
                "catalog allowance {}",
                receipt.as_ref().unwrap().limit
            ))),
            "unknown" => assert!(s.status().contains("charge-unknown")),
            _ => panic!("unrecognized prior allowance fixture: {prior}"),
        }
        assert_eq!(s.money.inference_guard, guard);
        assert_eq!(s.inference_receipt().unwrap(), receipt);
        assert_eq!(peer.bodies().len(), before);
        // Inspecting the proposal keeps its reviewed identity and account.
        let _ = s.consent("show");
        assert_eq!(s.pending_proposal().as_ref(), Some(&id));
        assert_eq!(s.money.inference_guard, guard);
        assert_eq!(s.inference_receipt().unwrap(), receipt);
        assert_eq!(peer.bodies().len(), before);
        let outcome = s.turn("What can you tell me about stars?");
        assert_eq!(s.money.inference_guard, guard);
        if prior == "spent" {
            assert!(matches!(outcome, TurnOutcome::Reply(_)));
            let after = s.inference_receipt().unwrap().unwrap();
            let receipt = receipt.unwrap();
            assert_eq!(after.limit, receipt.limit);
            assert!(after.estimated.nano_usd > receipt.estimated.nano_usd);
            assert_eq!(after.attempts.len(), receipt.attempts.len() + 1);
            assert_eq!(peer.bodies().len(), before + 1);
        } else {
            assert!(matches!(outcome, TurnOutcome::Refusal(_)));
            assert!(s.money_blocks_cognition());
            assert_eq!(s.inference_receipt().unwrap(), receipt);
            assert_eq!(peer.bodies().len(), before);
        }
        assert!(!dir.path().join("compiled-workflow.nika").exists());
        assert!(!dir.path().join("sortie.txt").exists());
    }
}
