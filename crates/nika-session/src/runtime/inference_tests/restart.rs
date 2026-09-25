// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Public Session doors across a real drop/reopen; synthetic loopback usage.
//! No private `MoneyState` mutation, external provider, or workflow execution.
use super::*;

#[test]
fn a_fresh_allowance_cannot_replace_prior_exposure_after_reopening() {
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
