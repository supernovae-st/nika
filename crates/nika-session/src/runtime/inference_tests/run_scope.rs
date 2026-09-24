// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Run-local money through public turns; loopback cognition, no workflow execution.
use super::*;

#[test]
fn restored_draft_run_ceiling_does_not_poison_following_cognition_or_reopen() {
    let peer = Peer::start(vec![
        (200, response("An earlier observed answer.")),
        (200, response("Stars emit light.")),
        (200, response("Stars emit light again.")),
    ]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut first = open(dir.path());
    first.enable_history(home.path()).unwrap();
    assert!(matches!(
        first.turn("What can you tell me about stars?"),
        TurnOutcome::Reply(_)
    ));
    let out = first.turn("Read ./entree.txt and write it to ./sortie.txt");
    assert!(matches!(out, TurnOutcome::Proposal { .. }), "{out:?}");
    assert!(matches!(first.consent("/quit"), TurnOutcome::Quit));
    assert_eq!(peer.bodies().len(), 1);
    drop(first);

    let mut resumed = open(dir.path());
    resumed.enable_history(home.path()).unwrap();
    let _ = resumed.restore_state();
    assert!(matches!(
        resumed.repropose_restored_draft(),
        TurnOutcome::Proposal { .. }
    ));
    assert!(matches!(resumed.consent("yes"), TurnOutcome::Facts(_)));
    assert!(matches!(resumed.turn("run it"), TurnOutcome::Refusal(_)));
    assert!(matches!(resumed.turn("run it with a ceiling of 0"),
        TurnOutcome::RunRequested { ref run, .. } if run.max_cost_usd.to_bits() == 0.0_f64.to_bits()));
    assert_eq!(
        peer.bodies().len(),
        1,
        "restore, Save and Run need no model"
    );
    assert!(
        !dir.path().join("sortie.txt").exists(),
        "Run only requested"
    );
    resumed.observe_run(0, None); // synthetic host completion, not a real Run proof
    assert!(
        matches!(resumed.turn("run it"), TurnOutcome::Refusal(_)),
        "the prior explicit Run ceiling is not inherited"
    );
    assert!(matches!(
        resumed.turn("What can you tell me about stars?"),
        TurnOutcome::Reply(_)
    ));
    assert_eq!(peer.bodies().len(), 2);
    assert!(resumed.inference_receipt().unwrap().is_none());
    assert!(matches!(resumed.turn("/quit"), TurnOutcome::Quit));
    drop(resumed);

    let mut again = open(dir.path());
    again.enable_history(home.path()).unwrap();
    let _ = again.restore_state();
    let out = again.turn("What can you tell me about stars?");
    assert!(
        matches!(out, TurnOutcome::Reply(_)),
        "{out:?}; {}",
        again.status()
    );
    assert_eq!(peer.bodies().len(), 3);
    assert!(again.inference_receipt().unwrap().is_none());
    let observed_calls: usize = again
        .cost_observations()
        .iter()
        .map(|v| v["attempts"].as_array().map_or(0, Vec::len))
        .sum();
    assert_eq!(observed_calls, 3, "all no-budget observations survive");
}

#[test]
fn independent_run_cannot_amend_existing_shared_zero_or_uncertain_account() {
    for zero in [false, true] {
        let mut unknown = response("unknown charge");
        unknown.as_object_mut().unwrap().remove("usage");
        let peer = Peer::start(vec![(200, unknown)]);
        let _transport = test_transport::install(&peer.url);
        let dir = tempfile::tempdir().unwrap();
        let mut s = open(dir.path());
        let doc: Value = serde_json::from_str(&native()).unwrap();
        std::fs::write(
            dir.path().join("ready.nika"),
            doc["candidate"].as_str().unwrap(),
        )
        .unwrap();
        s.admit_money("budget 2 USD", false).unwrap();
        let shared = s.money.account.clone().unwrap();
        if zero {
            s.admit_money("budget 0 USD", false).unwrap();
        } else {
            assert!(
                s.reason_with_money("a request with unknown cost", false)
                    .is_err()
            );
        }
        let before = shared.snapshot().unwrap();
        let guard = s.money.inference_guard.clone();
        let decisions = s.intent.decisions.clone();
        let count = peer.bodies().len();
        for ceiling in ["0", "5"] {
            assert!(matches!(
                s.turn(&format!("run ready.nika budget {ceiling} USD")),
                TurnOutcome::RunRequested { .. }
            ));
            s.observe_run(0, None);
            assert_eq!(shared.snapshot().unwrap(), before);
            assert_eq!(s.inference_receipt().unwrap().unwrap(), before);
            assert_eq!(s.money.inference_guard, guard);
            assert!(decisions.iter().all(|d| s.intent.decisions.contains(d)));
            assert!(s.money_blocks_cognition());
        }
        assert!(matches!(
            s.turn("What can you tell me about stars?"),
            TurnOutcome::Refusal(_)
        ));
        assert_eq!(peer.bodies().len(), count);
        assert!(!dir.path().join("sortie.txt").exists());
    }
}

#[test]
fn invalid_run_ceiling_refuses_execution_without_new_session_restriction() {
    let peer = Peer::start(vec![(200, response("Stars emit light."))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    for input in ["run it budget NaN USD", "run it at 9", "run it cap $-1"] {
        assert!(matches!(s.turn(input), TurnOutcome::Refusal(_)));
        assert!(s.pending_input().is_none());
        assert!(peer.bodies().is_empty());
    }
    assert!(matches!(
        s.turn("What can you tell me about stars?"),
        TurnOutcome::Reply(_)
    ));
    assert_eq!(peer.bodies().len(), 1);
}

#[test]
fn plain_run_cannot_bypass_or_amend_a_pending_gate() {
    let peer = Peer::start(vec![]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    s.admit_gate_money("budget 0 USD").unwrap();
    let gate = s.money.gate.clone();
    for input in ["run missing.nika", "run missing.nika budget 5 USD"] {
        assert!(matches!(s.turn(input), TurnOutcome::Refusal(_)));
        assert_eq!(s.money.gate, gate);
        assert!(s.money_blocks_cognition());
    }
    assert!(peer.bodies().is_empty());
}

#[test]
fn history_keeps_legacy_money_and_interrupted_run_uncertainty() {
    use super::super::history::{AuthorityState, EffectState, History, Operation, RunState, Saved};
    for legacy in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let mut h = History::open(home.path(), dir.path()).unwrap();
        h.begin(
            if legacy {
                Operation::Turn
            } else {
                Operation::Run
            },
            "run it budget 0 USD",
        )
        .unwrap();
        if legacy {
            h.complete(
                Saved::default(),
                RunState::Idle,
                AuthorityState::None,
                "refused".into(),
                EffectState::NoUncertaintyReported,
            )
            .unwrap();
        }
        drop(h);
        let h = History::open(home.path(), dir.path()).unwrap();
        assert_eq!(h.monetary_seen, legacy);
        assert_eq!(h.uncertain, !legacy);
        if !legacy {
            assert!(
                h.state
                    .recent
                    .iter()
                    .any(|(input, _)| input == "run it budget 0 USD")
            );
        }
    }
}
