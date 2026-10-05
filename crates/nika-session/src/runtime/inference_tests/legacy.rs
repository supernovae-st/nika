// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Legacy exposure + fresh public-door consent, loopback only; no bill reconciliation.
use super::*;
use nika_runtime::cost_choice::CostHostEvidence;
fn legacy() -> Value {
    serde_json::from_str(include_str!(
        "../../../../nika-providers/src/admission/legacy/fixture.json"
    ))
    .unwrap()
}
fn seed(root: &Path) {
    let mut state = crate::SessionState::new("2026-10-05T08:20:45Z".into());
    state
        .decisions
        .push(super::super::inference::RECONFIRM.into());
    state.inference_observations.push(legacy());
    state.save(root).unwrap();
}
fn restored(root: &Path, home: &Path) -> SessionRuntime {
    let mut session = open(root);
    session.enable_history(home).unwrap();
    session.restore_state().expect("legacy record");
    session.set_cost_host_evidence(CostHostEvidence::unmanaged_interactive_local());
    session
}
fn review(out: &TurnOutcome) {
    super::unknown_cost::asked(out);
    assert!(
        matches!(out, TurnOutcome::Question { question, .. }
        if question.contains("legacy exposure retained")
        && question.contains("no guaranteed TOTAL ceiling")
        && question.contains("only the NEW invocation")),
        "{out:?}"
    );
}
#[test]
fn fresh_review_each_time_and_after_reopen_keeps_the_exact_old_report_once() {
    let peer = Peer::start(vec![(200, response("Hello"))]);
    let _transport = test_transport::install(&peer.url);
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    seed(root.path());
    let mut session = restored(root.path(), home.path());
    review(&session.turn("hello"));
    assert!(peer.bodies().is_empty());
    assert!(session.inference_receipt().unwrap().is_none());
    assert!(matches!(session.turn("yes"), TurnOutcome::Reply(_)));
    assert_eq!(peer.bodies().len(), 1);
    assert!(session.inference_receipt().unwrap().is_none());
    assert!(session.money.reconfirm);
    let first = session.cost_observations();
    assert_eq!(first.len(), 2);
    assert_eq!(first[0], legacy());
    assert_eq!(first[1]["state"], "Closed");
    assert!(session.status().contains("legacy exposure retained"));
    review(&session.turn("hello"));
    assert_eq!(peer.bodies().len(), 1);
    assert!(matches!(session.turn("no"), TurnOutcome::Facts(_)));
    let before = crate::SessionState::load(root.path()).unwrap().unwrap();
    assert!(before.inference_checkpoint.is_none());
    assert_eq!(before.inference_observations, first);
    drop(session);
    let mut session = restored(root.path(), home.path());
    review(&session.turn("hello"));
    assert_eq!(peer.bodies().len(), 1);
    assert!(matches!(session.turn("oui"), TurnOutcome::Reply(_)));
    assert_eq!(peer.bodies().len(), 2);
    let after = session.cost_observations();
    assert_eq!(after.len(), 3);
    assert_eq!(&after[..2], &first);
    assert_eq!(after.iter().filter(|o| **o == legacy()).count(), 1);
    assert!(session.money.reconfirm);
}
#[test]
fn an_amount_never_approves_unknown_cost_and_zero_never_offers_a_review() {
    let peer = Peer::start(vec![(200, response("must not be called"))]);
    let _transport = test_transport::install(&peer.url);
    for amount in [0.01, 10.0, 50.0] {
        let root = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        seed(root.path());
        let mut session = restored(root.path(), home.path());
        review(&session.turn(&format!("hello budget {amount} USD")));
        review(&session.turn("budget 100 USD"));
        assert_eq!(session.cost_observations(), vec![legacy()]);
        assert!(session.inference_receipt().unwrap().is_none());
    }
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    seed(root.path());
    let mut session = restored(root.path(), home.path());
    let _ = session.turn("hello budget 0 USD");
    assert!(!session.waiting_cost_choice());
    assert!(peer.bodies().is_empty());
}
#[test]
fn changed_record_source_or_model_cannot_confirm_the_old_review() {
    let peer = Peer::start(vec![(200, response("must not be called"))]);
    let _transport = test_transport::install(&peer.url);
    for change in 0..4 {
        let root = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        seed(root.path());
        let mut session = restored(root.path(), home.path());
        review(&session.turn("hello"));
        match change {
            0 => {
                let mut state = crate::SessionState::load(root.path()).unwrap().unwrap();
                state.updated_at = "2026-10-05T09:20:45Z".into();
                state.save(root.path()).unwrap();
            }
            1 => std::fs::write(root.path().join(".nika/session-state.json"), "broken").unwrap(),
            2 => std::fs::write(root.path().join("other.nika"), "nika: changed\ntasks: {}\n")
                .unwrap(),
            _ => {
                session.reasoner = Box::new(ProviderReasoner {
                    model: "deepseek/deepseek-v4-flash".into(),
                    label: "changed".into(),
                });
            }
        }
        assert!(matches!(session.turn("yes"), TurnOutcome::Refusal(_)));
        assert!(peer.bodies().is_empty());
    }
}
#[test]
fn invalid_or_active_exposure_and_unknown_host_refuse_before_any_call() {
    let peer = Peer::start(vec![(200, response("must not be called"))]);
    let _transport = test_transport::install(&peer.url);
    for change in 0..5 {
        let root = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        seed(root.path());
        let mut state = crate::SessionState::load(root.path()).unwrap().unwrap();
        match change {
            0 => state
                .decisions
                .push(format!("{}old", super::super::inference::DISPATCH_PREFIX)),
            1 => state.inference_observations[0]["known_subtotal_nano_usd"] = json!("0"),
            2 => state.inference_checkpoint = Some(json!("unreadable old checkpoint")),
            3 => {
                state.pending = Some(crate::state::Pending::Gate {
                    workflow: "flow.nika".into(),
                    trace: "trace".into(),
                    task: "gate".into(),
                    mode: "confirm".into(),
                });
            }
            _ => {}
        }
        state.save(root.path()).unwrap();
        let mut session = restored(root.path(), home.path());
        if change == 4 {
            session.set_cost_host_evidence(CostHostEvidence::default());
        }
        assert!(matches!(session.turn("hello"), TurnOutcome::Refusal(_)));
        assert!(!session.waiting_cost_choice());
        assert!(peer.bodies().is_empty());
    }
}
#[test]
fn a_new_uncertain_invocation_is_not_absorbed_into_the_legacy_exception() {
    let peer = Peer::start(vec![(503, json!({"error":"possibly billed"}))]);
    let _transport = test_transport::install(&peer.url);
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    seed(root.path());
    let mut session = restored(root.path(), home.path());
    review(&session.turn("hello"));
    let _ = session.turn("yes");
    assert_eq!(peer.bodies().len(), 1);
    assert_eq!(session.cost_observations()[0], legacy());
    assert_eq!(session.cost_observations()[1]["state"], "Uncertain");
    assert!(session.status().contains("legacy exposure retained"));
    assert!(matches!(session.turn("hello"), TurnOutcome::Refusal(_)));
    assert_eq!(peer.bodies().len(), 1);
    drop(session);
    let mut session = restored(root.path(), home.path());
    assert!(matches!(
        session.turn("hello budget 50 USD"),
        TurnOutcome::Refusal(_)
    ));
    assert_eq!(peer.bodies().len(), 1);
}

#[test]
fn explicit_small_amount_still_needs_the_described_override_and_keeps_the_old_limit() {
    let peer = Peer::start(vec![(200, response("Hello"))]);
    let _transport = test_transport::install(&peer.url);
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    seed(root.path());
    let mut session = restored(root.path(), home.path());
    review(&session.turn("What can you tell me about stars, budget 0.01 USD?"));
    assert!(peer.bodies().is_empty());
    assert!(matches!(session.turn("yes"), TurnOutcome::Reply(_)));
    assert!(!peer.bodies().is_empty());
    assert_eq!(session.cost_observations()[0], legacy());
    assert_eq!(
        session.cost_observations()[0]["limit_nano_usd"],
        "15000000000"
    );
    assert!(session.inference_receipt().unwrap().is_none());
    assert!(session.money.reconfirm);
}
#[test]
fn revising_a_proposal_on_the_priced_route_asks_for_a_new_review_before_calls() {
    let script = vec![(200, response("NEW_WORK"))]
        .into_iter()
        .chain(authored(response))
        .collect();
    let peer = Peer::start(script);
    let _transport = test_transport::install(&peer.url);
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    seed(root.path());
    let mut session = restored(root.path(), home.path());
    review(&session.turn(WORK));
    let out = session.turn("yes");
    let TurnOutcome::Proposal { id, .. } = out else {
        panic!("{out:?}");
    };
    let sent = peer.bodies().len();
    assert_eq!(sent, 1 + CREATE_CALLS);
    review(&session.consent("Modifie le workflow : copie les octets dans autre.txt."));
    assert_eq!(peer.bodies().len(), sent);
    let _ = session.turn("no");
    assert_eq!(session.pending_proposal(), Some(id));
    assert_eq!(session.cost_observations()[0], legacy());
}
