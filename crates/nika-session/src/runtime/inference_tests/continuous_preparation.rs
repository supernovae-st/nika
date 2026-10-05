// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Real Session/registry/loopback checks for the interactive preparation path. No providers.
use super::unknown_cost::{open_unknown, unpriced_response};
use super::*;
#[test]
fn fresh_unpriced_preparation_answers_without_a_cost_gate_and_preserves_unknown() {
    let peer = Peer::start(vec![(200, unpriced_response("Hello"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut session = open_unknown(dir.path());
    session.enable_continuous_preparation();
    let out = session.turn("hello");
    assert!(
        matches!(out, TurnOutcome::Reply(_)),
        "unpriced preparation must answer without a gate"
    );
    assert_eq!(peer.bodies().len(), 1);
    assert!(!session.waiting_cost_choice());
    let observations = session.cost_observations();
    let actual = observations
        .iter()
        .find(|o| o["schema"] == "nika/preparation-cost-observation@1")
        .expect("observation");
    assert_eq!(actual["unknown_calls"], 1);
    assert!(actual["calls"][0]["estimated_usd"].is_null());
    assert!(session.inference_line().contains("unpriced requests"));
}
#[test]
fn reopen_keeps_unknown_exposure_and_does_not_replay_or_require_budget_confirmation() {
    let peer = Peer::start(vec![
        (200, unpriced_response("Hello")),
        (200, unpriced_response("Hello again")),
    ]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    {
        let mut first = open_unknown(dir.path());
        first.enable_continuous_preparation();
        assert!(matches!(first.turn("hello"), TurnOutcome::Reply(_)));
    }
    let before = crate::SessionState::load(dir.path())
        .unwrap()
        .unwrap()
        .inference_observations;
    let mut second = open_unknown(dir.path());
    second.enable_continuous_preparation();
    assert!(second.restore_state().is_some());
    assert_eq!(peer.bodies().len(), 1);
    assert!(matches!(second.turn("hello"), TurnOutcome::Reply(_)));
    assert_eq!(peer.bodies().len(), 2);
    for observation in before {
        assert!(second.cost_observations().contains(&observation));
    }
    assert!(second.inference_line().contains("2 unpriced requests"));
}
#[test]
fn historical_numeric_account_does_not_become_fresh_credit_or_a_design_gate() {
    let peer = Peer::start(vec![(200, response("Hello"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut session = open(dir.path());
    let account = nika_providers::InferenceAdmission::new(nika_types::cost::Cost::zero()).unwrap();
    account.close("old choice").unwrap();
    session.money.account = Some(account.clone());
    session.money.reconfirm = true;
    let before = account.snapshot().unwrap().observation();
    session.enable_continuous_preparation();
    assert!(matches!(session.turn("hello"), TurnOutcome::Reply(_)));
    assert!(
        account.snapshot().unwrap().observation() == before,
        "preparation must preserve the complete historical account"
    );
    assert_eq!(peer.bodies().len(), 1);
    assert!(session.inference_receipt().unwrap().is_some());
}
#[test]
fn continuous_policy_uses_route_capacity_while_bounded_callers_keep_their_contract() {
    use crate::authoring::session_policy;
    use nika_onboard::compile::NativeMode;
    let old = session_policy(MODEL, false, NativeMode::Escalate);
    assert_eq!(old.max_tokens, 32_768);
    let costs = nika_providers::authoring::preparation::PreparationCosts::default();
    let _scope = costs.enter();
    let continuous = session_policy(MODEL, false, NativeMode::Escalate);
    assert_eq!(continuous.initial_max_tokens, Some(131_072));
    assert_eq!(continuous.max_tokens, 393_216);
    assert_eq!(continuous.repair_limit(), None);
    assert_eq!(continuous.timeout, std::time::Duration::from_secs(600));
}

#[test]
fn jev_closed_and_uncertain_observations_survive_reopen_without_a_model_replay() {
    let peer = Peer::start(vec![(200, unpriced_response("Hello"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut state = crate::SessionState::new("2026-10-05T00:00:00Z".into());
    let kept = vec![
        json!({"schema":"nika/session-decision-seat@2","state":"Closed","attempts":[],"cost":"unknown"}),
        json!({"schema":"nika/session-decision-seat@2","state":"Uncertain","attempts":[{"sent":true,"outcome":"in_flight"}],"cost":"unknown"}),
    ];
    state.inference_observations = kept.clone();
    state.save(dir.path()).unwrap();
    let mut session = open_unknown(dir.path());
    session.enable_continuous_preparation();
    assert!(session.restore_state().is_some());
    assert!(peer.bodies().is_empty());
    assert!(matches!(session.turn("hello"), TurnOutcome::Reply(_)));
    for observation in kept {
        assert!(session.cost_observations().contains(&observation));
    }
    assert_eq!(peer.bodies().len(), 1);
}

#[test]
fn preparation_does_not_save_or_run_and_the_run_keeps_its_explicit_zero() {
    let mut script = vec![(200, unpriced_response("NEW_WORK"))];
    script.extend(authored(unpriced_response));
    let peer = Peer::start(script);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut session = open_unknown(dir.path());
    session.enable_continuous_preparation();
    let TurnOutcome::Proposal { id, .. } = session.turn(WORK) else {
        panic!("expected Save review");
    };
    assert!(!dir.path().join("sortie.txt").exists());
    assert!(session.last_workflow.is_none());
    assert!(matches!(
        session.consent_to(&id, "yes"),
        TurnOutcome::Facts(_)
    ));
    let workflow = session.last_workflow.as_ref().expect("saved workflow");
    assert!(dir.path().join(workflow).exists());
    let sent = peer.bodies().len();
    assert!(matches!(session.turn("run it with a ceiling of 0"),
        TurnOutcome::RunRequested { ref run, .. } if run.max_cost_usd.to_bits() == 0.0_f64.to_bits()));
    assert_eq!(
        peer.bodies().len(),
        sent,
        "Save and Run need no preparation model"
    );
    assert!(
        !dir.path().join("sortie.txt").exists(),
        "Run is requested, not executed by Session"
    );
}

#[test]
fn each_unsettled_preparation_failure_is_unknown_in_the_durable_history() {
    let failed = json!({"error":{"message":"fixture refused"}});
    let peer = Peer::start(vec![(400, failed.clone()), (400, failed)]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut session = open(dir.path());
    session.enable_continuous_preparation();
    session.enable_history(home.path()).unwrap();
    for expected in 1..=2 {
        assert!(!matches!(session.turn("hello"), TurnOutcome::Reply(_)));
        assert_eq!(peer.bodies().len(), expected);
        assert_eq!(session.uncertain_charges(), expected);
        let project = std::fs::read_dir(home.path().join(".nika/sessions"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let log = std::fs::read_to_string(project.join("events.ndjson")).unwrap();
        let event: Value = serde_json::from_str(log.lines().last().unwrap()).unwrap();
        assert_eq!(event["event"]["effect"], "unknown");
        assert!(!session.waiting_cost_choice());
    }
}
