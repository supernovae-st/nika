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

#[test]
fn continuous_no_intelligence_greeting_and_fallback_create_no_project_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut session = SessionRuntime::open(
        dir.path(),
        super::super::tests::ready(IntelligenceKind::None, DataLocus::None),
        Box::new(crate::reasoner::NoReasoner),
    );
    session.enable_continuous_preparation();
    session.enable_history(home.path()).unwrap();
    assert_eq!(
        session.classify(SessionPhase::Idle, "hello there").method,
        crate::turn::RoutingMethod::Fallback
    );
    let out = session.turn("hello there, how are you today?");
    assert!(
        matches!(out, TurnOutcome::Refusal(ref why)
        if why.class == RefusalClass::NoIntelligence),
        "{out:?}"
    );
    assert!(!dir.path().join(".nika").exists());
    assert!(session.cost_observations().is_empty());
    assert_eq!(session.uncertain_charges(), 0);
}

#[test]
fn continuous_missing_classifier_is_fallback_without_an_inflight_record() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = open(dir.path());
    session.factory = None;
    session.enable_continuous_preparation();
    assert_eq!(
        session
            .classify(SessionPhase::Idle, "read this line")
            .method,
        crate::turn::RoutingMethod::Fallback
    );
    assert!(!dir.path().join(".nika").exists());
    assert!(session.cost_observations().is_empty());
}

struct BoundaryReasoner(std::path::PathBuf);
impl SessionReasoner for BoundaryReasoner {
    fn name(&self) -> String {
        "custom fixture".into()
    }
    fn reason(&mut self, _: &str) -> Result<crate::Reply, ReasonError> {
        assert_boundary(&self.0, None);
        Ok(crate::Reply::new("fixture reply".into(), false))
    }
}
fn assert_boundary(root: &Path, decision: Option<&str>) {
    let state = crate::SessionState::load(root).unwrap().unwrap();
    assert!(state.decisions.iter().any(|line| {
        line.starts_with(super::super::inference::OBSERVED_PREFIX)
            && decision.is_none_or(|seat| line.contains(seat))
    }));
}

#[test]
fn continuous_custom_reasoners_and_acp_keep_their_pre_dispatch_boundary() {
    use nika_types::access::HarnessTransport;
    for (kind, locus) in [
        (IntelligenceKind::None, DataLocus::None),
        (
            IntelligenceKind::Harness {
                seat: "claude-code".into(),
                transport: HarnessTransport::Acp,
            },
            DataLocus::Remote {
                product: "claude-code".into(),
            },
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut session = SessionRuntime::open(
            dir.path(),
            super::super::tests::ready(kind, locus),
            Box::new(BoundaryReasoner(dir.path().into())),
        );
        session.enable_continuous_preparation();
        assert!(session.reason_with_money("hello", false).is_ok());
        let settled = crate::SessionState::load(dir.path()).unwrap().unwrap();
        assert!(
            settled
                .decisions
                .iter()
                .all(|line| !line.starts_with(super::super::inference::OBSERVED_PREFIX))
        );
    }
}

struct BoundaryClassifier(std::path::PathBuf);
impl TurnClassifier for BoundaryClassifier {
    fn classify(&mut self, _: &TurnContext, _: &str) -> TurnDecision {
        assert_boundary(&self.0, None);
        TurnDecision::new(TurnAct::NewWork, crate::turn::RoutingMethod::Model)
    }
}
#[test]
fn continuous_injected_classifier_keeps_its_boundary_without_conversational_ai() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = SessionRuntime::open(
        dir.path(),
        super::super::tests::ready(IntelligenceKind::None, DataLocus::None),
        Box::new(crate::reasoner::NoReasoner),
    );
    session.with_classifier(Box::new(BoundaryClassifier(dir.path().into())));
    session.enable_continuous_preparation();
    assert_eq!(
        session.classify(SessionPhase::Idle, WORK).act,
        TurnAct::NewWork
    );
}

#[test]
fn continuous_deterministic_seat_keeps_its_explicit_decision_boundary() {
    use super::authoring_decision::{KEY, SEAT};
    use crate::authoring::{AuthoringContext, DecisionSetup};
    let dir = tempfile::tempdir().unwrap();
    let mut session = SessionRuntime::open(
        dir.path(),
        super::super::tests::ready(IntelligenceKind::None, DataLocus::None),
        Box::new(crate::reasoner::NoReasoner),
    );
    session.set_authoring_context(
        AuthoringContext::default().with_decision(Some(DecisionSetup::with_key(
            SEAT,
            Some(KEY.into()),
            None,
        ))),
    );
    session.enable_continuous_preparation();
    session
        .seated(&AuthoringSeat::Deterministic { why: None }, |_| {
            assert_boundary(dir.path(), Some(SEAT));
            Ok(())
        })
        .unwrap();
}

#[test]
fn returned_unpriced_preparation_reopens_without_an_incomplete_operation_notice() {
    for usage_present in [true, false] {
        let mut body = unpriced_response("Hello");
        if !usage_present {
            body.as_object_mut().unwrap().remove("usage");
        }
        let peer = Peer::start(vec![(200, body)]);
        let _transport = test_transport::install(&peer.url);
        let dir = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let mut first = open_unknown(dir.path());
        first.enable_continuous_preparation();
        first.enable_history(home.path()).unwrap();
        assert!(matches!(first.turn("hello"), TurnOutcome::Reply(_)));
        assert_eq!(peer.bodies().len(), 1);
        assert_eq!(first.uncertain_charges(), 0, "the response returned");
        let observations = first.cost_observations();
        let evidence = observations
            .iter()
            .find(|o| o["schema"] == "nika/preparation-cost-observation@1")
            .unwrap();
        assert_eq!(evidence["unknown_calls"], 1);
        assert_eq!(evidence["billing"], "unknown");
        assert_eq!(evidence["state"], "Closed");
        assert!(evidence["calls"][0]["estimated_usd"].is_null());
        let journal = std::fs::read_dir(home.path().join(".nika/sessions"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path()
            .join("events.ndjson");
        let text = std::fs::read_to_string(&journal).unwrap();
        let completed: Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
        assert_eq!(completed["event"]["effect"], "no_uncertainty_reported");
        drop(first);

        let mut resumed = open_unknown(dir.path());
        resumed.enable_continuous_preparation();
        let notice = resumed.enable_history(home.path()).unwrap().unwrap();
        assert!(!notice.contains("historical unresolved operation"));
        assert!(resumed.restore_state().is_some());
        assert!(resumed.cost_observations().contains(evidence));
        assert!(resumed.inference_line().contains("1 unpriced requests"));
        assert!(
            !resumed
                .kept_turns()
                .iter()
                .any(|(_, text)| text.contains("uncertain result"))
        );
        assert!(
            resumed.inference_receipt().unwrap().is_none(),
            "no account restored"
        );
        assert_eq!(peer.bodies().len(), 1, "reopen sends no request");
        assert!(!dir.path().join("sortie.txt").exists());
    }
}
