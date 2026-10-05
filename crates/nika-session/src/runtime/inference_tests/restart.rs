// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Public Session doors across a real drop/reopen; synthetic loopback usage.
//! No private `MoneyState` mutation, external provider, or workflow execution.
use super::*;

fn assert_total_amendment(
    resumed: &mut SessionRuntime,
    old: &nika_providers::InferenceReceipt,
    peer: &Peer,
    count: usize,
    root: &std::path::Path,
) {
    let kept = resumed.inference_receipt().unwrap().unwrap();
    assert_eq!(kept.state, AdmissionState::Closed);
    assert_eq!(kept.limit, old.limit);
    assert_eq!(kept.estimated, old.estimated);
    assert_eq!(kept.attempts, old.attempts);
    assert!(matches!(
        resumed.turn("What can you tell me about stars?"),
        TurnOutcome::Refusal(_)
    ));
    assert_eq!(peer.bodies().len(), count);
    let out = resumed.turn("What can you tell me about stars, budget 3 USD?");
    assert!(matches!(out, TurnOutcome::Reply(_)), "{out:?}");
    let next = resumed.inference_receipt().unwrap().unwrap();
    assert_eq!(next.limit.nano_usd, 3_000_000_000);
    assert_eq!(next.estimated.nano_usd, old.estimated.nano_usd * 2);
    assert_eq!(next.attempts.len(), 2);
    assert_eq!(next.attempts[1].id, 1);
    assert_eq!(peer.bodies().len(), count + 1);
    assert!(matches!(
        resumed.turn("What can you tell me about stars, budget 0.000001 USD?"),
        TurnOutcome::Refusal(_)
    ));
    assert_eq!(
        resumed.inference_receipt().unwrap().unwrap().estimated,
        next.estimated
    );
    assert_eq!(peer.bodies().len(), count + 1);
    assert!(!root.join("sortie.txt").exists());
}

#[test]
fn reopening_only_amends_a_complete_concordant_account_after_fresh_total_consent() {
    for complete in [false, true] {
        for keep_history in [false, true] {
            let mut unknown = response("unknown charge");
            if !complete {
                unknown.as_object_mut().unwrap().remove("usage");
            }
            let peer = Peer::start(vec![
                (200, unknown),
                (200, response("must not be requested")),
            ]);
            let _transport = test_transport::install(&peer.url);
            let dir = tempfile::tempdir().unwrap();
            let home = tempfile::tempdir().unwrap();
            let mut first = open(dir.path());
            first.enable_history(home.path()).unwrap();
            let out = first.turn("What can you tell me about stars, budget 2 USD?");
            assert_eq!(matches!(out, TurnOutcome::Reply(_)), complete, "{out:?}");
            if !complete {
                assert!(matches!(out, TurnOutcome::Refusal(_)), "{out:?}");
            }
            let old = first.inference_receipt().unwrap().unwrap();
            if complete {
                assert_eq!(old.state, AdmissionState::Open);
                assert!(old.estimated.nano_usd > 0);
            } else {
                assert_eq!(old.state, AdmissionState::Uncertain);
                assert!(old.held_unknown.nano_usd > 0);
            }
            assert_eq!(old.billed, None);
            // Save a deterministic proposal through the public door so the
            // structured record also carries the preexisting Session restriction.
            assert!(matches!(
                first.turn("Read ./entree.txt and write it to ./sortie.txt"),
                TurnOutcome::Proposal { .. }
            ));
            assert!(matches!(first.consent("yes"), TurnOutcome::Facts(_)));
            let count = peer.bodies().len();
            assert_eq!(count, 1);
            drop(first);

            let mut resumed = open(dir.path());
            if keep_history {
                resumed.enable_history(home.path()).unwrap();
            }
            assert!(resumed.restore_state().is_some());
            if complete && keep_history {
                assert_total_amendment(&mut resumed, &old, &peer, count, dir.path());
                continue;
            }
            assert!(
                resumed.inference_receipt().unwrap().is_none(),
                "no restored ledger was proved"
            );
            assert!(matches!(
                resumed.turn("What can you tell me about stars?"),
                TurnOutcome::Refusal(_)
            ));
            for input in [
                "What can you tell me about stars, budget 3 USD?",
                "What can you tell me about stars, budget 0 USD?",
                "What can you tell me about stars, budget 4 USD?",
            ] {
                assert!(
                    matches!(resumed.turn(input), TurnOutcome::Refusal(_)),
                    "{input}"
                );
                assert!(
                    resumed.inference_receipt().unwrap().is_none(),
                    "unknown is not a new zero-spend account"
                );
                assert_eq!(peer.bodies().len(), count);
            }
            // Execution authority is separate: a fresh explicit Run ceiling can
            // be requested, but neither it nor its completion repairs the ledger.
            assert!(
                matches!(resumed.turn("run compiled-workflow.nika budget 5 USD"),
                TurnOutcome::RunRequested { ref run, .. } if run.max_cost_usd.to_bits() == 5.0_f64.to_bits())
            );
            resumed.observe_run(0, None); // synthetic host observation; no engine ran
            assert!(resumed.inference_receipt().unwrap().is_none());
            assert!(matches!(
                resumed.turn("What can you tell me about stars?"),
                TurnOutcome::Refusal(_)
            ));
            assert_eq!(peer.bodies().len(), count);
            assert!(
                resumed
                    .status()
                    .contains("restored inference exposure is unknown")
            );
            assert!(!dir.path().join("sortie.txt").exists());
        }
    }
}

#[test]
fn old_missing_corrupt_or_divergent_checkpoints_never_admit_a_fresh_allowance() {
    for damage in [
        "legacy",
        "missing-history",
        "digest",
        "dispatch",
        "inconsistent",
        "observation",
    ] {
        let peer = Peer::start(vec![(200, response("one paid observation"))]);
        let _transport = test_transport::install(&peer.url);
        let dir = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let mut first = open(dir.path());
        first.enable_history(home.path()).unwrap();
        assert!(matches!(
            first.turn("What can you tell me about stars, budget 2 USD?"),
            TurnOutcome::Reply(_)
        ));
        drop(first);
        let path = dir.path().join(".nika/session-state.json");
        let mut raw: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        match damage {
            "legacy" => {
                raw.as_object_mut().unwrap().remove("inference_checkpoint");
            }
            "observation" => raw["inference_observations"] = json!([]),
            "digest" => raw["inference_checkpoint"]["digest"] = json!("damaged"),
            "inconsistent" => raw["inference_checkpoint"]["account"]["estimated"] = json!("0"),
            "dispatch" => raw["decisions"].as_array_mut().unwrap().push(json!(format!(
                "{}interrupted fixture",
                super::super::inference::DISPATCH_PREFIX
            ))),
            _ => {}
        }
        std::fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
        let mut resumed = open(dir.path());
        if damage != "missing-history" {
            resumed.enable_history(home.path()).unwrap();
        }
        assert!(resumed.restore_state().is_some());
        assert!(resumed.inference_receipt().unwrap().is_none(), "{damage}");
        assert!(
            matches!(
                resumed.turn("What can you tell me about stars, budget 3 USD?"),
                TurnOutcome::Refusal(_)
            ),
            "{damage}"
        );
        assert_eq!(peer.bodies().len(), 1, "{damage}");
    }
}

#[test]
fn a_project_zero_default_cannot_reopen_a_restored_empty_account() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    s.admit_money("Budget 2 USD", false, false).unwrap();
    let a = s.money.account.as_ref().unwrap();
    a.amend(nika_types::cost::Cost::zero()).unwrap();
    a.close("restored").unwrap();
    s.money.reconfirm = true;
    let mut default = s.money.draft.clone().unwrap();
    default.effective_usd = Some(0.0);
    default.explicit_amount = None;
    default.source = crate::money::MonetarySource::ProjectDefault;
    s.configure_admission(&mut default);
    assert_eq!(
        s.inference_receipt().unwrap().unwrap().state,
        AdmissionState::Closed
    );
    assert!(s.money_blocks_cognition());
}
