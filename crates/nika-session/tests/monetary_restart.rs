// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Restart through public doors, with counting cognition and the engine's own paused journals
//! (the C7b `S1` and `T1` public runs) staged under `.nika/traces`, where a reopened session
//! judges a gate before it offers it again.
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
/// The paused journal of the C7b `S1` public run (C6 binary, minimized, every frame kept): the
/// engine's own run identity, gate `ask`.
const S1_PAUSED: (&str, &str) = (
    "2026-09-28T12-59-54Z-53b6.ndjson",
    r#"{"id":{"uuid":"01a0e819-b68b-7649-85b2-39277be74e66"},"timestamp":1790600394379000000,"kind":"workflow_started","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"7466341540fd02fca9ec21937862176b7821a52495b86d81bb5f30d16c8462dc","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"project_root_fingerprint","value":"68bc0fa6f93982fd69bcd7dc3b4074d55f54a57579461599d47765293bdbf7cd"}]}
{"id":{"uuid":"01a0e819-b68c-726d-a8e3-3ef859c76d0f"},"timestamp":1790600394380000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"527926e042b24c4415b65b50cca37f0f1f609ec9f52478191a9faf23491600c3","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-b68d-735a-9777-3c6706958b21"},"timestamp":1790600394381000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"eee513cc41db18434eb38cbf51b55d48946fbea527dc0f80777a31deaff40551","fields":[{"key":"task","value":"ask"}]}
{"id":{"uuid":"01a0e819-b68d-735a-9777-3c683f5bba50"},"timestamp":1790600394381000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"0c76a73643ecc528974ba46ceb6025423d93139955cbd8807ee4549ed3be67f9","fields":[{"key":"task","value":"after_gate"}]}
{"id":{"uuid":"01a0e819-b690-75e2-8832-255d6a8aeee0"},"timestamp":1790600394384000000,"kind":"task_started","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"1ee9c3dc4a185833b486d65cc324a5022fc69a96f4f39707ff60d4f19e4493a2","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-b690-75e2-8832-255ea327f97a"},"timestamp":1790600394384000000,"kind":"permit_checked","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"a7f5e04b1f6ddcd5ee13fa89aed3240f228b59b4ea4620fbb6c51bd442e8c5d8","fields":[{"key":"task","value":"before"},{"key":"decision","value":"allow"},{"key":"why","value":"permits.tools covers the id"}]}
{"id":{"uuid":"01a0e819-b690-75e2-8832-255f93243257"},"timestamp":1790600394384000000,"kind":"task_completed","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"1e89b99a6737b7686166dd97a346879fb2d829c2ca73b550ba2a9b00967273b7","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-b691-7011-a1fc-369f8aa8657f"},"timestamp":1790600394385000000,"kind":"workflow_paused","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"33180c9c50ec797c947a4969df6319a91f404bad00f875bcfd7d44e02bdccba0","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"task","value":"ask"},{"key":"mode","value":"confirm"},{"key":"message","value":"Ship it?"},{"key":"status","value":"paused"},{"key":"cause","value":"human_gate"}]}
"#,
);
/// Another run's paused journal (the C7b `T1` public run): another gate identity.
const T1_PAUSED: (&str, &str) = (
    "2026-09-28T12-59-36Z-9a41.ndjson",
    r#"{"id":{"uuid":"01a0e819-71c9-7715-93d4-1573aa3681ab"},"timestamp":1790600376777000000,"kind":"workflow_started","execution":{"uuid":"01a0e819-71c4-7308-ae2d-839c28959a41"},"run":null,"correlation":null,"chain":"7466341540fd02fca9ec21937862176b7821a52495b86d81bb5f30d16c8462dc","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"project_root_fingerprint","value":"4e3fa6930c69d22160a6848d9f54e2a9682204ce9b0bc02888ad067c84b656d3"}]}
{"id":{"uuid":"01a0e819-71ca-7735-9a13-fdc763c04d79"},"timestamp":1790600376778000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-71c4-7308-ae2d-839c28959a41"},"run":null,"correlation":null,"chain":"e09aae0d994d24d936ed18cdca50c1decab36fc3fb09cfb4e1a318fd7e91f28c","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-71ca-7735-9a13-fdc884de3c0c"},"timestamp":1790600376778000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-71c4-7308-ae2d-839c28959a41"},"run":null,"correlation":null,"chain":"6dde84a55121d4570e233095bd7c9753e58d79ddb3d690318d8d869c98898969","fields":[{"key":"task","value":"ask"}]}
{"id":{"uuid":"01a0e819-71ca-7735-9a13-fdc9b18e9c3c"},"timestamp":1790600376778000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-71c4-7308-ae2d-839c28959a41"},"run":null,"correlation":null,"chain":"a03af3d59c3b0326bce200b443ce35d4a91cb43c0278a20b6dd5dd9ed18e28d9","fields":[{"key":"task","value":"after_gate"}]}
{"id":{"uuid":"01a0e819-71cd-7227-bcb3-ebc83f76de15"},"timestamp":1790600376781000000,"kind":"task_started","execution":{"uuid":"01a0e819-71c4-7308-ae2d-839c28959a41"},"run":null,"correlation":null,"chain":"f702107b5cc3687dd71afb0be317481dfbecf06bd9ef47ae77cbc29098a763a0","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-71cd-7227-bcb3-ebc984c9ef4a"},"timestamp":1790600376781000000,"kind":"permit_checked","execution":{"uuid":"01a0e819-71c4-7308-ae2d-839c28959a41"},"run":null,"correlation":null,"chain":"7d881cb52f88094c03ebedf8318dd4e061412fc5d6a59cd921a457a7c5baf8a2","fields":[{"key":"task","value":"before"},{"key":"decision","value":"allow"},{"key":"why","value":"permits.tools covers the id"}]}
{"id":{"uuid":"01a0e819-71cd-7227-bcb3-ebcaa3ea49c7"},"timestamp":1790600376781000000,"kind":"task_completed","execution":{"uuid":"01a0e819-71c4-7308-ae2d-839c28959a41"},"run":null,"correlation":null,"chain":"db1db29798bd9ebef63043ad9ec51680ca48deaac51618700a1a6850c9dcbf8e","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-71ce-7736-b844-f5081003d240"},"timestamp":1790600376782000000,"kind":"workflow_paused","execution":{"uuid":"01a0e819-71c4-7308-ae2d-839c28959a41"},"run":null,"correlation":null,"chain":"4d4f3e8f9c41210104fbce2cba848f83655fe6882e4060994e625391ff2772f2","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"task","value":"ask"},{"key":"mode","value":"confirm"},{"key":"message","value":"Ship it?"},{"key":"status","value":"paused"},{"key":"cause","value":"human_gate"}]}
"#,
);
/// The pause these tests used to craft: one frame, no run identity.
const FORGED: &str = r#"{"kind":"workflow_paused","fields":[{"key":"task","value":"approve"},{"key":"mode","value":"confirm"},{"key":"message","value":"Proceed?"}]}"#;

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
/// A journal staged in the project's trace store, where the engine writes its own.
fn stage(root: &Path, (name, body): (&str, &str)) -> PathBuf {
    let traces = root.join(".nika/traces");
    std::fs::create_dir_all(&traces).expect("trace store");
    let trace = traces.join(name);
    std::fs::write(&trace, body).expect("journal");
    trace
}
fn pause(s: &mut SessionRuntime, root: &Path) -> PathBuf {
    std::fs::write(root.join("entree.txt"), "A\n").expect("source");
    assert!(matches!(s.turn(COPY), TurnOutcome::Proposal { .. }));
    assert!(matches!(s.consent("yes"), TurnOutcome::Facts(_)));
    assert!(matches!(s.turn("run it"), TurnOutcome::RunRequested { .. }));
    let trace = stage(root, S1_PAUSED);
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
    pause(&mut first, root.path());
    first.answer_gate("budget 0 USD");
    // A later public host observation changes which gate waits. Keep the prior
    // monetary marker intact: the new trace is not its completion evidence.
    let other_trace = stage(root.path(), T1_PAUSED);
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

/// Forged control (C7b §3.4): the crafted one-frame pause names no run. It is asked while the
/// session that observed it lives, but after a close it cannot be judged from `.nika/traces` and
/// is never offered again, and the monetary hold its answer left is not released by it.
#[test]
fn a_forged_pause_is_never_offered_after_a_close_and_releases_nothing() {
    let root = tempfile::tempdir().expect("root");
    let (mut first, _) = boot(root.path(), None);
    std::fs::write(root.path().join("entree.txt"), "A\n").expect("source");
    assert!(matches!(first.turn(COPY), TurnOutcome::Proposal { .. }));
    assert!(matches!(first.consent("yes"), TurnOutcome::Facts(_)));
    assert!(matches!(
        first.turn("run it"),
        TurnOutcome::RunRequested { .. }
    ));
    let forged = root.path().join("paused.ndjson");
    std::fs::write(&forged, FORGED).expect("forged pause");
    assert!(matches!(
        first.observe_run(4, Some(&forged)),
        TurnOutcome::GateAsk { .. }
    ));
    assert!(matches!(
        first.answer_gate("budget 0 USD"),
        TurnOutcome::Refusal(_)
    ));
    drop(first);
    let (mut resumed, calls) = boot(root.path(), None);
    assert!(
        resumed.waiting_gate().is_none(),
        "a forged pause is never offered"
    );
    assert!(matches!(resumed.turn(QUESTION), TurnOutcome::Refusal(_)));
    assert_eq!(calls.count(), 0);
    assert!(!root.path().join("sortie.txt").exists());
}

/// Forged control: a byte copy of a real journal staged as another gate carries the same run
/// identity twice; after a close it cannot be judged and is never offered, and the hold stays.
#[test]
fn a_byte_copy_of_a_journal_is_never_offered_as_another_gate() {
    let root = tempfile::tempdir().expect("root");
    let (mut first, _) = boot(root.path(), None);
    pause(&mut first, root.path());
    first.answer_gate("budget 0 USD");
    let copy = stage(
        root.path(),
        ("2026-09-28T13-00-00Z-copy.ndjson", S1_PAUSED.1),
    );
    assert!(matches!(
        first.observe_run(4, Some(&copy)),
        TurnOutcome::GateAsk { .. }
    ));
    drop(first);
    let (mut resumed, calls) = boot(root.path(), None);
    assert!(
        resumed.waiting_gate().is_none(),
        "one run identity in two journals"
    );
    assert!(matches!(resumed.turn(QUESTION), TurnOutcome::Refusal(_)));
    assert_eq!(calls.count(), 0);
    assert!(!root.path().join("sortie.txt").exists());
}
