// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The real Session → compiler/verb → registry → reqwest path on loopback.
//! No ambient keys/env mutation; the HTTP test effect alone substitutes its
//! destination. Production endpoint qualification remains exact and closed.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::disallowed_types
)]
use super::*;
use crate::DataLocus;
use crate::money::InferenceEnforcement;
use crate::reasoner::{ProviderReasoner, test_transport};
use crate::turn::{SessionPhase, TurnAct, TurnClassifier, TurnContext, TurnDecision};
use nika_providers::AdmissionState;
use serde_json::{Value, json};
mod carried;
#[cfg(feature = "access-harness")]
mod connection;
mod continuous_preparation;
mod cost_choice_host;
mod decision_seat;
/// A workflow the person already has, named in a change, revised over its complete document.
mod document_revision;
mod interrupted;
mod judge_trials;
mod legacy;
mod no_budget;
mod observed_project;
mod question_budget;
mod question_identity;
mod read_only;
/// A recorded workflow takes a change its graph refuses over its whole document.
mod recorded_fallback;
mod recovery;
mod restart;
mod revision_question;
mod run_scope;
mod scopes;
mod unjudged;
mod unknown_cost;
mod unknown_reopen;
pub(crate) mod wire;
use wire::{Peer, response};
const MODEL: &str = "deepseek/deepseek-v4-pro";
const WORK: &str =
    "Je veux que sortie.txt contienne exactement les octets présents dans entree.txt.";
fn native() -> String {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/compile/copy-fr.json"))
            .expect("fixture");
    let candidate = fixture["candidate"]
        .as_str()
        .expect("candidate")
        .replace("./notes/brief.md", "./entree.txt")
        .replace("./out/copie.md", "./sortie.txt");
    json!({"candidate":candidate,"questions":[],"gaps":[],"notes":"copy the exact bytes"})
        .to_string()
}
/// A fresh CREATE of `WORK` as the semantic doors answer it (`semantic_copy` to `sortie.txt`).
pub(crate) fn semantic_create() -> [String; 3] {
    semantic_copy("sortie.txt")
}
/// A fresh CREATE of the exact-byte copy of `entree.txt` into `destination`, as the semantic
/// doors answer it, in call order: the private plan names the copy as the part it cannot carry
/// (so the request escalates to the sketch door), the sketch reads the stated source and writes
/// the stated destination under the name the copy fixture carries, and the fills add nothing
/// (the edge carries the bytes). The compiler writes the source; no reply is whole source.
pub(super) fn semantic_copy(destination: &str) -> [String; 3] {
    let plan = json!({"steps":[],"effects":[],"obligations":[],"constraints":[],
        "unknowns":[format!("{destination} contienne exactement les octets présents dans entree.txt")],
        "regions":[],"approval_bypass":{"present":false}});
    let task = |id: &str, tool: &str, extra: Value| {
        let mut task = json!({"id":id,"verb":"invoke","tool":tool,"purpose":id});
        task.as_object_mut()
            .expect("task")
            .extend(extra.as_object().expect("extra").clone());
        task
    };
    let sketch = json!({"name":"compiled-workflow","tasks":[
        task("read_source", "nika:read", json!({"reads":["entree.txt"]})),
        task("write_output", "nika:write",
            json!({"writes":[destination],"with":[{"name":"content","from":"read_source"}]})),
    ],"outputs":[],"questions":[],"gaps":[],"notes":"copy the exact bytes"});
    let fills = json!({"fills":[],"notes":"the edge carries the bytes"});
    [plan.to_string(), sketch.to_string(), fills.to_string()]
}
/// The calls one approved semantic CREATE of `WORK` sends: plan, sketch, fills, judge.
pub(crate) const CREATE_CALLS: usize = 4;
/// `semantic_create` then the approving judge, each reply wrapped by `reply`.
pub(crate) fn authored(reply: fn(&str) -> Value) -> Vec<(u16, Value)> {
    let mut script: Vec<_> = semantic_create().iter().map(|t| (200, reply(t))).collect();
    script.push((200, reply(JUDGE_APPROVES)));
    script
}
/// The verifier's closed choice, approved (native step 1, R4 A11): the explicit answer a test
/// scripts at the judge's position, after a native candidate READY in its authoring round. The
/// judge's call is a real request, counted and journaled like any other.
pub(crate) const JUDGE_APPROVES: &str = r#"{"choice":"faithful"}"#;
fn open(root: &Path) -> SessionRuntime {
    std::fs::write(root.join("entree.txt"), "A\n").expect("input");
    let selected = ResolvedSessionIntelligence {
        kind: IntelligenceKind::Api {
            provider: "deepseek".into(),
        },
        model: Some(MODEL.into()),
        locus: DataLocus::Metered {
            provider: "deepseek".into(),
        },
        ready: true,
        why: None,
    };
    let mut s = SessionRuntime::open(
        root,
        selected,
        Box::new(ProviderReasoner {
            model: MODEL.into(),
            label: "DeepSeek".into(),
        }),
    );
    s.factory = Some(Box::new(|_| {
        Box::new(ProviderReasoner {
            model: MODEL.into(),
            label: "DeepSeek".into(),
        })
    }));
    // No settings file is read: the shared default strategy (escalate, semantic CREATE).
    s.set_authoring_context(crate::authoring::AuthoringContext::from_settings(
        &nika_cli_host::compile::config::AuthoringSettings::none(),
        &nika_cli_host::compile::config::AuthoringSettings::none(),
    ));
    s
}
#[test]
fn positive_authoring_amendment_and_save_share_one_account() {
    // This wording enters authoring directly. Classifier/conversation/compiler
    // aggregation is exercised separately below through those real consumers.
    let peer = Peer::start(authored(response));
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let mut s = open(dir.path());
    let input = format!("{WORK} budget 2 USD.");
    let out = s.turn(&input);
    let TurnOutcome::Proposal { id: old, .. } = out else {
        panic!("positive authoring: {out:?}");
    };
    let before = s.inference_receipt().expect("receipt").expect("account");
    assert_eq!(
        before.attempts.len(),
        CREATE_CALLS,
        "the plan, sketch and fill calls and the judge's"
    );
    assert!(before.estimated.nano_usd > 0);
    assert_eq!(before.billed, None);
    assert_eq!(s.monetary_decision().expect("money").original_intent, input);
    assert_eq!(
        s.monetary_decision().expect("money").inference,
        InferenceEnforcement::CatalogAdmission
    );
    assert!(
        peer.bodies()
            .iter()
            .all(|b| b["model"] == "deepseek-v4-pro")
    );
    // The seat reads the work; the admitted ceiling is the Session's, never work (R4 B15).
    let asked = peer.bodies()[0].to_string();
    assert!(
        asked.contains(WORK) && !asked.contains("budget 2 USD"),
        "{asked}"
    );
    assert!(!dir.path().join("sortie.txt").exists());
    let out = s.consent("budget 3 USD");
    let TurnOutcome::Proposal { id, .. } = out else {
        panic!("amend: {out:?}");
    };
    assert_ne!(old, id);
    assert_eq!(peer.bodies().len(), CREATE_CALLS);
    assert_eq!(
        s.inference_receipt().unwrap().unwrap().estimated,
        before.estimated
    );
    assert!(matches!(s.consent_to(&old, "yes"), TurnOutcome::Refusal(_)));
    assert!(matches!(s.consent_to(&id, "yes"), TurnOutcome::Facts(_)));
    assert!(
        matches!(s.turn("run it"),TurnOutcome::RunRequested{ref run,..} if run.max_cost_usd.to_bits() == 3.0_f64.to_bits())
    );
    assert_eq!(peer.bodies().len(), CREATE_CALLS);
}
#[test]
fn fresh_classifier_conversation_and_compiler_do_not_reset_exposure() {
    let peer = Peer::start(
        vec![(200, response("NEW_WORK")), (200, response("Hello"))]
            .into_iter()
            .chain(authored(response))
            .collect(),
    );
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    s.admit_money("budget 2 USD", false, false).expect("admit");
    assert_eq!(s.classify(SessionPhase::Idle, WORK).act, TurnAct::NewWork);
    s.reason_with_money("explain this work", false)
        .expect("reason");
    let round = AuthoringRound::new(WORK);
    s.compile_round(&round, &s.seat.clone()).expect("compile");
    let r = s.inference_receipt().unwrap().unwrap();
    assert_eq!(
        r.attempts.len(),
        2 + CREATE_CALLS,
        "classifier, conversation, plan, sketch, fill, judge"
    );
    assert_eq!(peer.bodies().len(), 2 + CREATE_CALLS);
    assert_eq!(peer.bodies()[0]["max_tokens"], 4096);
    assert_eq!(peer.bodies()[1]["max_tokens"], 8192);
    s.admit_money("budget 2 USD", true, false).unwrap();
    assert_eq!(
        s.inference_receipt().unwrap().unwrap().estimated,
        r.estimated
    );
    s.money
        .account
        .as_ref()
        .unwrap()
        .amend(r.estimated)
        .unwrap();
    assert!(s.reason_with_money("another", false).is_err());
    assert_eq!(peer.bodies().len(), 2 + CREATE_CALLS);
}
#[test]
fn zero_invalid_and_unknown_charge_remain_guarded_across_questions() {
    for status in [200, 429, 503] {
        let mut b = response("hello");
        if status == 200 {
            b.as_object_mut().unwrap().remove("usage");
        }
        let peer = Peer::start(vec![(status, b)]);
        let _transport = test_transport::install(&peer.url);
        let dir = tempfile::tempdir().unwrap();
        let mut s = open(dir.path());
        s.admit_money("budget 2 USD", false, false).unwrap();
        assert!(s.reason_with_money("hello", false).is_err());
        let r = s.inference_receipt().unwrap().unwrap();
        assert_eq!(r.state, AdmissionState::Uncertain);
        assert!(r.held_unknown.nano_usd > 0);
        s.admit_money("budget 3 USD", true, false).unwrap();
        assert!(s.money_blocks_cognition());
        let _ = s.turn("what happened?");
        assert_eq!(peer.bodies().len(), 1);
    }
    let peer = Peer::start(vec![(200, response("hello"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    s.admit_money("budget 2 USD", false, false).unwrap();
    s.reason_with_money("hello", false).unwrap();
    for value in ["budget 0 USD", "budget NaN USD"] {
        let _ = s.turn(value);
        let _ = s.turn("what happened?");
        assert_eq!(peer.bodies().len(), 1);
        assert!(s.inference_receipt().unwrap().unwrap().estimated.nano_usd > 0);
    }
}
struct Unmetered;
impl TurnClassifier for Unmetered {
    fn classify(&mut self, _: &TurnContext, _: &str) -> TurnDecision {
        panic!("unmetered classifier called")
    }
}
#[test]
fn custom_classifier_default_refuses_and_reopen_requires_reconfirmation() {
    let peer = Peer::start(vec![(200, response("hello"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    {
        let mut s = open(dir.path());
        s.enable_history(home.path()).unwrap();
        let _ = s.turn("hello budget 2 USD");
        s.with_classifier(Box::new(Unmetered));
        assert_eq!(s.classify(SessionPhase::Idle, WORK).act, TurnAct::Unknown);
    }
    let count = peer.bodies().len();
    let mut restored = open(dir.path());
    restored.enable_history(home.path()).unwrap();
    assert!(restored.money_blocks_cognition());
    let _ = restored.turn("what happened?");
    assert_eq!(peer.bodies().len(), count);
    assert_eq!(restored.inference_receipt().unwrap(), None);
}

#[test]
fn a_first_zero_or_invalid_amount_cannot_fall_back_to_an_unbounded_question() {
    let peer = Peer::start(vec![(200, response("unexpected"))]);
    let _transport = test_transport::install(&peer.url);
    for first in ["budget 0 USD", "budget NaN USD", "budget -1 USD"] {
        let dir = tempfile::tempdir().unwrap();
        let mut s = open(dir.path());
        let _ = s.turn(first);
        let _ = s.turn("what happened?");
        assert!(s.money_blocks_cognition());
        assert!(peer.bodies().is_empty());
    }
}

struct UnmeteredReasoner;
impl SessionReasoner for UnmeteredReasoner {
    fn name(&self) -> String {
        "unmetered fixture".into()
    }
    fn reason(&mut self, _: &str) -> Result<crate::Reply, crate::ReasonError> {
        panic!("old unbounded reasoner called")
    }
}
#[test]
fn a_fresh_unmetered_factory_cannot_escape_the_active_account() {
    let peer = Peer::start(vec![(200, response("unexpected"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    s.admit_money("budget 2 USD", false, false).unwrap();
    s.factory = Some(Box::new(|_| Box::new(UnmeteredReasoner)));
    assert_eq!(s.classify(SessionPhase::Idle, WORK).act, TurnAct::Unknown);
    assert!(peer.bodies().is_empty());
}
#[test]
fn both_confirm_gate_doors_hold_paid_accounts_without_resume_or_calls() {
    let peer = Peer::start(vec![(200, response("hello"))]);
    let _transport = test_transport::install(&peer.url);
    for addressed in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let mut s = open(dir.path());
        s.admit_money("budget 2 USD", false, false).unwrap();
        s.reason_with_money("hello", false).unwrap();
        let before = peer.bodies().len();
        let receipt = s.inference_receipt().unwrap();
        let draft = s.money.draft.clone();
        s.pending_gate = Some(PendingGate {
            workflow: "w.nika".into(),
            trace: dir.path().join("paused.ndjson"),
            task: "approve".into(),
            mode: "confirm".into(),
            message: "Proceed?".into(),
        });
        let gate = s.waiting_gate().unwrap();
        for line in [
            "yes but budget 3 USD",
            "what happened?",
            "budget NaN USD",
            "what happened?",
        ] {
            let out = if addressed {
                s.answer_gate_for(&gate, line)
            } else {
                s.answer_gate(line)
            };
            assert!(
                !matches!(out, TurnOutcome::ResumeRequested { .. }),
                "{out:?}"
            );
            assert_eq!(s.waiting_gate(), Some(gate.clone()));
            assert_eq!(peer.bodies().len(), before);
            assert_eq!(s.inference_receipt().unwrap(), receipt);
            assert_eq!(s.money.draft, draft);
        }
        let out = if addressed {
            s.answer_gate_for(&gate, "no")
        } else {
            s.answer_gate("no")
        };
        assert!(
            matches!(out, TurnOutcome::ResumeRequested { ref answer, .. } if answer == "approve=false")
        );
        assert!(s.waiting_gate().is_none());
        assert_eq!(s.inference_receipt().unwrap(), receipt);
        assert_eq!(s.money.draft, draft);
        assert_eq!(peer.bodies().len(), before);
        s.reason_with_money("explain after the gate", false)
            .unwrap();
        let after = s.inference_receipt().unwrap().unwrap();
        let prior = receipt.unwrap();
        assert_eq!(after.limit, prior.limit);
        assert_eq!(after.attempts.len(), prior.attempts.len() + 1);
        assert!(after.estimated.nano_usd > prior.estimated.nano_usd);
        assert_eq!(peer.bodies().len(), before + 1);
    }
}

#[test]
fn zero_and_invalid_constraints_survive_structured_state_restoration() {
    let peer = Peer::start(vec![(200, response("unexpected"))]);
    let _transport = test_transport::install(&peer.url);
    for line in ["budget 0 USD", "budget NaN USD"] {
        let dir = tempfile::tempdir().unwrap();
        let mut s = open(dir.path());
        let _ = s.turn(line);
        let mut state = crate::state::SessionState::new("2026-09-24T00:00:00Z".into());
        state.decisions = s.intent.decisions.clone();
        state.save(dir.path()).expect("save state");
        let mut restored = open(dir.path());
        restored.restore_state().expect("restore state");
        assert!(restored.money_blocks_cognition());
        let _ = restored.turn("explain this please");
        assert!(peer.bodies().is_empty());
    }
}
