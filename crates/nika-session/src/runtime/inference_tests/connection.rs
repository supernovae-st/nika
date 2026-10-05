// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Real API loopback accounting and an injected subscription reasoner; no installed CLI.
use super::*;
use crate::intelligence::SeatSeen;
use crate::reasoner::{Reply, SessionReasoner};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Subscription(Arc<AtomicUsize>, nika_types::access::HarnessTransport);
impl SessionReasoner for Subscription {
    fn name(&self) -> String {
        "claude-code fixture".into()
    }
    fn authoring_harness(&self) -> Option<String> {
        Some("claude-code".into())
    }
    fn harness_transport(&self) -> nika_types::access::HarnessTransport {
        self.1
    }
    fn reason(&mut self, _: &str) -> Result<Reply, ReasonError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(Reply {
            text: "subscription fixture reply".into(),
            usage_observed: false,
        })
    }
}
fn connected(root: &Path, home: &Path, calls: &Arc<AtomicUsize>) -> SessionRuntime {
    let pref = UserIntelligencePreference::load(home).unwrap_or_else(|| {
        UserIntelligencePreference::new(
            IntelligenceKind::Api {
                provider: "deepseek".into(),
            },
            Some(MODEL.into()),
        )
    });
    let census = IntelligenceCensus {
        seats: vec![SeatSeen {
            id: "claude-code".into(),
            product_present: true,
            configured: true,
            answers_here: true,
        }],
        api_keys: vec!["deepseek".into()],
        locals: vec![],
    };
    let calls = Arc::clone(calls);
    let mut session = SessionRuntime::open_with(
        root,
        census,
        &pref,
        Some(home),
        Box::new(move |resolved| match resolved.kind {
            IntelligenceKind::Harness { transport, .. } => {
                Box::new(Subscription(Arc::clone(&calls), transport))
            }
            _ => Box::new(ProviderReasoner {
                model: MODEL.into(),
                label: "DeepSeek".into(),
            }),
        }),
    );
    session.set_authoring_context(crate::authoring::AuthoringContext::from_settings(
        &nika_cli_host::compile::config::AuthoringSettings::none(),
        &nika_cli_host::compile::config::AuthoringSettings::none(),
    ));
    session.enable_history(home).unwrap();
    session.restore_state();
    session
}
fn choose(session: &mut SessionRuntime, choice: &str) -> String {
    assert!(matches!(session.turn("/intelligence"), TurnOutcome::Ask(_)));
    let TurnOutcome::Facts(text) = session.choose(choice) else {
        panic!("choice")
    };
    text
}
fn account(root: &Path) -> Value {
    let state: Value =
        serde_json::from_slice(&std::fs::read(root.join(".nika/session-state.json")).unwrap())
            .unwrap();
    state["inference_checkpoint"]["account"].clone()
}
fn same_costs(before: &Value, after: &Value) {
    for key in [
        "identity",
        "limit",
        "estimated",
        "active",
        "held",
        "uncertain",
        "attempts",
    ] {
        assert_eq!(before[key], after[key], "{key}");
    }
}

#[test]
fn explicit_subscription_choice_keeps_api_account_across_reload_and_return() {
    subscription_account_cycle("1 claude-code/claude-fable-5-1");
}
#[test]
#[cfg(unix)]
fn explicit_acp_choice_keeps_api_account_across_reload_and_return() {
    subscription_account_cycle("1 acp:claude-code/claude-fable-5-1");
}
fn subscription_account_cycle(choice: &str) {
    let peer = Peer::start(vec![(200, response("first")), (200, response("next"))]);
    let _transport = test_transport::install(&peer.url);
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut session = connected(root.path(), home.path(), &calls);
    assert!(matches!(
        session.turn("What can you tell me about stars, budget 2 USD?"),
        TurnOutcome::Reply(_)
    ));
    let before = account(root.path());
    assert!(before["identity"].is_string());
    let notice = choose(&mut session, choice);
    assert!(notice.contains("invoice unknown") && notice.contains("allowance is suspended"));
    assert_eq!(
        session.inference_receipt().unwrap().unwrap().state,
        AdmissionState::Closed
    );
    assert!(matches!(
        session.turn("What can you tell me about stars?"),
        TurnOutcome::Reply(_)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(peer.bodies().len(), 1, "no API fallback");
    same_costs(&before, &account(root.path()));
    assert_eq!(
        session.monetary_decision().unwrap().inference,
        InferenceEnforcement::NotMetered
    );
    let seat = session.authoring_seat().clone();
    assert!(matches!(
        seat,
        crate::authoring::AuthoringSeat::Harness { .. }
    ));
    session
        .seated(&seat, |admission| {
            assert!(
                admission.is_none(),
                "the compiler's subscription seam receives no API account"
            );
            Ok(())
        })
        .unwrap();
    drop(session);

    let mut resumed = connected(root.path(), home.path(), &calls);
    assert!(matches!(
        resumed.intelligence.kind,
        IntelligenceKind::Harness { .. }
    ));
    assert!(matches!(
        resumed.turn("What can you tell me about stars?"),
        TurnOutcome::Reply(_)
    ));
    same_costs(&before, &account(root.path()));
    choose(&mut resumed, "2 deepseek/deepseek-v4-pro");
    assert!(matches!(
        resumed.turn("What can you tell me about stars?"),
        TurnOutcome::Refusal(_)
    ));
    assert_eq!(peer.bodies().len(), 1);
    same_costs(&before, &account(root.path()));
    assert!(matches!(
        resumed.turn("What can you tell me about stars, budget 3 USD?"),
        TurnOutcome::Reply(_)
    ));
    let after = account(root.path());
    assert_eq!(after["identity"], before["identity"]);
    assert_eq!(after["limit"], "3000000000");
    assert_eq!(
        after["estimated"]
            .as_str()
            .unwrap()
            .parse::<i128>()
            .unwrap(),
        before["estimated"]
            .as_str()
            .unwrap()
            .parse::<i128>()
            .unwrap()
            * 2
    );
    assert_eq!(after["attempts"].as_array().unwrap().len(), 2);
    assert_eq!(peer.bodies().len(), 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn subscription_ceiling_refusal_survives_reload_until_explicit_reselection() {
    subscription_reselection_cycle("1 claude-code/claude-fable-5-1");
}
#[test]
#[cfg(unix)]
fn acp_ceiling_refusal_survives_reload_until_explicit_reselection() {
    subscription_reselection_cycle("1 acp:claude-code/claude-fable-5-1");
}
fn subscription_reselection_cycle(choice: &str) {
    let peer = Peer::start(vec![(200, response("first"))]);
    let _transport = test_transport::install(&peer.url);
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut session = connected(root.path(), home.path(), &calls);
    assert!(matches!(
        session.turn("What can you tell me about stars, budget 2 USD?"),
        TurnOutcome::Reply(_)
    ));
    let before = account(root.path());
    choose(&mut session, choice);
    assert!(matches!(
        session.turn("What can you tell me about stars, budget 0 USD?"),
        TurnOutcome::Refusal(_)
    ));
    same_costs(&before, &account(root.path()));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    drop(session);
    let mut resumed = connected(root.path(), home.path(), &calls);
    let outcome = resumed.turn("What can you tell me about stars?");
    assert!(
        matches!(outcome, TurnOutcome::Refusal(ref why) if why.text.contains("use /intelligence"))
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(peer.bodies().len(), 1);
    same_costs(&before, &account(root.path()));
    choose(&mut resumed, choice);
    assert!(matches!(
        resumed.turn("What can you tell me about stars?"),
        TurnOutcome::Reply(_)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    same_costs(&before, &account(root.path()));
    assert_eq!(peer.bodies().len(), 1);
}

#[test]
fn subscription_selection_does_not_settle_an_uncertain_api_attempt() {
    let mut unknown = response("unknown");
    unknown.as_object_mut().unwrap().remove("usage");
    let peer = Peer::start(vec![(200, unknown)]);
    let _transport = test_transport::install(&peer.url);
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut session = connected(root.path(), home.path(), &calls);
    assert!(matches!(
        session.turn("What can you tell me about stars, budget 2 USD?"),
        TurnOutcome::Refusal(_)
    ));
    let before = account(root.path());
    assert_eq!(before["uncertain"], true);
    choose(&mut session, "1 claude-code/claude-fable-5-1");
    same_costs(&before, &account(root.path()));
    let receipt = session.inference_receipt().unwrap().unwrap();
    assert_eq!(receipt.state, AdmissionState::Uncertain);
    assert!(receipt.held_unknown.nano_usd > 0);
    choose(&mut session, "2 deepseek/deepseek-v4-pro");
    assert!(matches!(
        session.turn("What can you tell me about stars, budget 3 USD?"),
        TurnOutcome::Refusal(_)
    ));
    same_costs(&before, &account(root.path()));
    assert_eq!(peer.bodies().len(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn subscription_reselection_does_not_release_project_zero_or_a_lost_gate_hold() {
    for project_zero in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let mut session = connected(root.path(), home.path(), &calls);
        if project_zero {
            session.snapshot.ceiling = Some(0.0);
        } else {
            session.intent.decisions.push(format!(
                "{}unresolved gate fixture",
                super::super::inference::GATE_MONEY_PREFIX
            ));
        }
        choose(&mut session, "1 claude-code/claude-fable-5-1");
        assert!(session.money_blocks_cognition());
        assert!(session.enter_dispatch(None).is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
