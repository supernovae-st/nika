// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Restart through public doors, with counting cognition and a synthetic pause.
//! No provider or engine process is started by these tests.
#![allow(clippy::expect_used, clippy::panic)]
use nika_session::state::SessionState;
use nika_session::turn::{RoutingMethod, TurnAct, TurnClassifier, TurnContext, TurnDecision};
use nika_session::{
    IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence, SessionReasoner,
    SessionRuntime, TurnOutcome, UserIntelligencePreference,
};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const COPY: &str = "Read ./entree.txt and write it to ./sortie.txt";
const QUESTION: &str = "What can you tell me about stars?";

#[derive(Clone, Default)]
struct Calls(Arc<AtomicUsize>);
impl Calls {
    fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}
impl TurnClassifier for Calls {
    fn classify(&mut self, _: &TurnContext, _: &str) -> TurnDecision {
        self.0.fetch_add(1, Ordering::SeqCst);
        TurnDecision::new(TurnAct::Unknown, RoutingMethod::Fallback)
    }
}
impl SessionReasoner for Calls {
    fn name(&self) -> String {
        "restart counting cognition".into()
    }
    fn reason(&mut self, prompt: &str) -> Result<nika_session::Reply, nika_session::ReasonError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        nika_session::ScriptedReasoner::new(vec!["controlled reply".into()]).reason(prompt)
    }
}
fn boot(root: &Path, home: Option<&Path>) -> (SessionRuntime, Calls) {
    let selected = ResolvedSessionIntelligence::resolve(
        &UserIntelligencePreference::new(IntelligenceKind::None, None),
        &IntelligenceCensus::empty(),
    );
    let calls = Calls::default();
    let mut s = SessionRuntime::open(root, selected, Box::new(calls.clone()));
    s.with_classifier(Box::new(calls.clone()));
    if let Some(home) = home {
        s.enable_history(home).expect("history");
    }
    s.restore_state();
    (s, calls)
}
fn pause(s: &mut SessionRuntime, root: &Path) -> PathBuf {
    std::fs::write(root.join("entree.txt"), "A\n").expect("source");
    assert!(matches!(s.turn(COPY), TurnOutcome::Proposal { .. }));
    assert!(matches!(s.consent("yes"), TurnOutcome::Facts(_)));
    assert!(matches!(s.turn("run it"), TurnOutcome::RunRequested { .. }));
    let trace = root.join("paused.ndjson");
    std::fs::write(&trace, r#"{"kind":"workflow_paused","fields":[{"key":"task","value":"approve"},{"key":"mode","value":"confirm"},{"key":"message","value":"Proceed?"}]}"#).expect("synthetic pause");
    assert!(matches!(
        s.observe_run(4, Some(&trace)),
        TurnOutcome::GateAsk { .. }
    ));
    trace
}

#[test]
fn gate_money_survives_reopen_with_present_missing_or_absent_history() {
    for history in ["present", "missing", "absent"] {
        for clause in ["budget 0 USD", "budget NaN USD"] {
            for addressed in [false, true] {
                let root = tempfile::tempdir().expect("root");
                let home = tempfile::tempdir().expect("home");
                let empty_home = tempfile::tempdir().expect("missing history home");
                let old_home = (history != "absent").then_some(home.path());
                let new_home = match history {
                    "present" => Some(home.path()),
                    "missing" => Some(empty_home.path()),
                    _ => None,
                };
                let (mut first, first_calls) = boot(root.path(), old_home);
                pause(&mut first, root.path());
                let gate = first.waiting_gate().expect("gate");
                let out = if addressed {
                    first.answer_gate_for(&gate, clause)
                } else {
                    first.answer_gate(clause)
                };
                assert!(matches!(out, TurnOutcome::Refusal(_)));
                assert_eq!(first_calls.count(), 0);
                assert!(matches!(first.answer_gate("/quit"), TurnOutcome::Quit));
                drop(first);
                let (mut resumed, calls) = boot(root.path(), new_home);
                let gate = resumed
                    .waiting_gate()
                    .expect("trace still carries the paused gate");
                assert_eq!(
                    resumed
                        .monetary_decision()
                        .expect("restored hold")
                        .effective_usd,
                    None,
                    "persisted hold is not a recovered numeric allowance"
                );
                let out = if addressed {
                    resumed.answer_gate_for(&gate, "what does this permit?")
                } else {
                    resumed.answer_gate("what does this permit?")
                };
                assert!(
                    matches!(out, TurnOutcome::Refusal(_)),
                    "{history}/{clause}: {out:?}"
                );
                assert_eq!(calls.count(), 0);
                assert_eq!(resumed.waiting_gate(), Some(gate.clone()));
                assert!(resumed.inference_receipt().expect("receipt").is_none());
                assert!(matches!(
                    resumed.answer_gate_for(&gate, "no"),
                    TurnOutcome::ResumeRequested { .. }
                ));
                // Existing intact history has only a coarse monetary_seen bit;
                // it must remain conservative rather than invent an account.
                let next = resumed.turn(QUESTION);
                if history == "present" {
                    assert!(matches!(next, TurnOutcome::Refusal(_)));
                    assert_eq!(calls.count(), 0);
                } else {
                    assert!(matches!(next, TurnOutcome::Reply(_)), "{next:?}");
                    assert_eq!(calls.count(), 1);
                }
                assert!(!root.path().join("sortie.txt").exists());
            }
        }
    }
}

#[test]
fn losing_the_gate_trace_does_not_grant_inference_permission() {
    let root = tempfile::tempdir().expect("root");
    let (mut first, _) = boot(root.path(), None);
    let trace = pause(&mut first, root.path());
    assert!(matches!(
        first.answer_gate("budget 0 USD"),
        TurnOutcome::Refusal(_)
    ));
    drop(first);
    std::fs::rename(&trace, root.path().join("preserved-paused.ndjson"))
        .expect("unavailable original handle");
    let (mut resumed, calls) = boot(root.path(), None);
    assert!(
        resumed.waiting_gate().is_none(),
        "no gate authority can be restored"
    );
    assert!(matches!(resumed.turn(QUESTION), TurnOutcome::Refusal(_)));
    assert_eq!(calls.count(), 0);
    assert!(matches!(
        resumed.turn("What can you tell me about stars, budget 2 USD?"),
        TurnOutcome::Refusal(_)
    ));
    assert_eq!(calls.count(), 0);
}

#[test]
fn completing_a_gate_removes_only_its_own_persistent_marker() {
    for prior in [None, Some("budget 0 USD"), Some("budget NaN USD")] {
        let root = tempfile::tempdir().expect("root");
        let (mut first, calls) = boot(root.path(), None);
        if let Some(prior) = prior {
            first.turn(prior);
        }
        pause(&mut first, root.path());
        let before = SessionState::load(root.path())
            .expect("record")
            .expect("present")
            .decisions;
        first.answer_gate("budget NaN USD");
        first.answer_gate("budget 2 USD");
        assert_eq!(calls.count(), 0);
        assert!(matches!(
            first.answer_gate("no"),
            TurnOutcome::ResumeRequested { .. }
        ));
        let after = SessionState::load(root.path())
            .expect("record")
            .expect("present");
        assert!(
            before.iter().all(|d| after.decisions.contains(d)),
            "preexisting Session markers survive"
        );
        assert!(after.pending.is_none());
        drop(first);
        let (mut resumed, seen) = boot(root.path(), None);
        let out = resumed.turn(QUESTION);
        if prior.is_some() {
            assert!(matches!(out, TurnOutcome::Refusal(_)));
            assert_eq!(seen.count(), 0);
        } else {
            assert!(matches!(out, TurnOutcome::Reply(_)), "{out:?}");
            assert_eq!(seen.count(), 1);
        }
    }
}

#[test]
fn stale_structured_state_cannot_erase_a_history_inference_marker() {
    let root = tempfile::tempdir().expect("root");
    let home = tempfile::tempdir().expect("home");
    let (mut first, _) = boot(root.path(), Some(home.path()));
    pause(&mut first, root.path());
    assert!(matches!(
        first.answer_gate("no"),
        TurnOutcome::ResumeRequested { .. }
    ));
    // A later Session constraint is in history; the earlier project record
    // deliberately remains the one written by gate completion.
    first.turn("budget 0 USD");
    drop(first);
    let (mut resumed, _) = boot(root.path(), Some(home.path()));
    // An observation keeps the merged structured record, as actual hosts do.
    resumed.observe_run(0, None);
    drop(resumed);
    let (mut without_history, calls) = boot(root.path(), None);
    assert!(matches!(
        without_history.turn(QUESTION),
        TurnOutcome::Refusal(_)
    ));
    assert_eq!(calls.count(), 0);
}

#[test]
fn unreadable_state_without_history_is_not_evidence_of_a_free_allowance() {
    let root = tempfile::tempdir().expect("root");
    std::fs::create_dir(root.path().join(".nika")).expect("state dir");
    let path = root.path().join(".nika/session-state.json");
    std::fs::write(&path, "{ broken").expect("unreadable state");
    let (mut resumed, calls) = boot(root.path(), None);
    assert!(matches!(resumed.turn(QUESTION), TurnOutcome::Refusal(_)));
    assert_eq!(calls.count(), 0);
    assert_eq!(
        std::fs::read_to_string(path).expect("preserved"),
        "{ broken"
    );
}

#[test]
fn a_different_gate_cannot_release_a_restored_monetary_hold() {
    let root = tempfile::tempdir().expect("root");
    let (mut first, _) = boot(root.path(), None);
    let original_trace = pause(&mut first, root.path());
    first.answer_gate("budget 0 USD");
    // A later public host observation changes which gate waits. Keep the prior
    // monetary marker intact: the new trace is not its completion evidence.
    let other_trace = root.path().join("other-paused.ndjson");
    std::fs::copy(&original_trace, &other_trace).expect("other trace identity");
    assert!(matches!(
        first.observe_run(4, Some(&other_trace)),
        TurnOutcome::GateAsk { .. }
    ));
    drop(first);
    let (mut resumed, calls) = boot(root.path(), None);
    let gate = resumed.waiting_gate().expect("different gate");
    assert!(matches!(
        resumed.answer_gate_for(&gate, "no"),
        TurnOutcome::ResumeRequested { .. }
    ));
    assert!(matches!(resumed.turn(QUESTION), TurnOutcome::Refusal(_)));
    assert_eq!(calls.count(), 0);
    drop(resumed);
    let (mut again, calls) = boot(root.path(), None);
    assert!(matches!(again.turn(QUESTION), TurnOutcome::Refusal(_)));
    assert_eq!(calls.count(), 0);
}
