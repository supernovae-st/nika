// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Public Session → native compiler/reasoner → registry → injected HTTP.
//! Fixture mechanics only, never model or billing qualification.
use super::*;
use nika_providers::admission::HardMonetaryCap;
use nika_runtime::cost_choice::{CapEvidence, CostHostEvidence};
use nika_types::cost::Cost;
const UNPRICED: &str = "deepseek/s81-unpriced-fixture";
fn open_unknown(root: &Path) -> SessionRuntime {
    let mut s = open(root);
    s.intelligence.model = Some(UNPRICED.into());
    s.reasoner = Box::new(ProviderReasoner {
        model: UNPRICED.into(),
        label: "selected unpriced route".into(),
    });
    s.factory = Some(Box::new(|_| {
        Box::new(ProviderReasoner {
            model: UNPRICED.into(),
            label: "selected unpriced route".into(),
        })
    }));
    s.refresh_seat();
    s.set_cost_host_evidence(CostHostEvidence::unmanaged_interactive_local());
    s
}
fn unpriced_response(text: &str) -> Value {
    let mut body = response(text);
    body["model"] = json!("s81-unpriced-fixture");
    body
}
fn asked(out: &TurnOutcome) {
    assert!(
        matches!(out, TurnOutcome::Question { key, question }
        if key == "unknown_cost" && question.contains("3 requests") && question.contains("8192")
        && question.contains("120 seconds") && question.contains("USD cost is unknown")),
        "{out:?}"
    );
}
#[test]
fn public_turn_zero_http_before_confirmation_one_after_and_observation_survives_restart() {
    let peer = Peer::start(vec![(200, unpriced_response("Hello"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open_unknown(dir.path());
    asked(&s.turn("hello"));
    assert!(peer.bodies().is_empty());
    assert!(s.inference_receipt().unwrap().is_none());
    assert!(s.waiting_cost_choice());
    let out = s.turn("yes");
    assert!(matches!(out, TurnOutcome::Reply(_)), "{out:?}");
    assert_eq!(peer.bodies().len(), 1);
    let receipt = s.inference_receipt().unwrap().unwrap();
    assert_eq!(receipt.unknown_calls, 1);
    assert_eq!(receipt.billed, None);
    assert_eq!(receipt.observation()["limit_nano_usd"], Value::Null);
    assert_eq!(receipt.unknown_attempts[0].estimated, None);
    let state = crate::SessionState::load(dir.path()).unwrap().unwrap();
    assert_eq!(state.inference_observations[0]["unknown_calls"], 1);
    assert_eq!(
        state.inference_observations[0]["known_subtotal_nano_usd"],
        "0"
    );
    let mut restored = open_unknown(dir.path());
    assert!(restored.restore_state().is_some());
    assert!(restored.inference_receipt().unwrap().is_none());
    assert_eq!(restored.cost_observations()[0]["unknown_calls"], 1);
    assert!(matches!(restored.turn("yes"), TurnOutcome::Refusal(_)));
    let _ = restored.turn("hello");
    assert_eq!(peer.bodies().len(), 1);
}
#[test]
fn native_compiler_candidate_requires_separate_save_review() {
    let peer = Peer::start(vec![
        (200, unpriced_response("NEW_WORK")),
        (200, unpriced_response(&native())),
    ]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open_unknown(dir.path());
    asked(&s.turn(WORK));
    assert!(peer.bodies().is_empty());
    let out = s.turn("yes");
    let TurnOutcome::Proposal { id, .. } = out else {
        panic!("native Compiler: {out:?}");
    };
    assert_eq!(peer.bodies().len(), 2);
    assert!(!dir.path().join("sortie.txt").exists());
    assert_eq!(s.pending_proposal(), Some(id.clone()));
    assert!(matches!(s.consent_to(&id, "yes"), TurnOutcome::Facts(_)));
    assert_eq!(peer.bodies().len(), 2);
}
#[test]
fn defaults_need_explicit_override_and_hard_or_unknown_caps_never_send() {
    let peer = Peer::start(vec![
        (200, unpriced_response("DISCUSS")),
        (200, unpriced_response("hello")),
    ]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("nika.yaml"),
        "nika: budget-project\nceiling: 0.01\n",
    )
    .unwrap();
    let mut s = open_unknown(dir.path());
    asked(&s.turn("hello budget 0.02 USD"));
    assert!(peer.bodies().is_empty());
    let _ = s.turn("yes");
    let r = s.inference_receipt().unwrap().unwrap();
    assert_eq!(
        r.overridden_defaults,
        [Some(Cost::new(20_000_000)), Some(Cost::new(10_000_000))]
    );
    assert_eq!(peer.bodies().len(), 2);
    for cap in [
        HardMonetaryCap::Capped(Cost::zero()),
        HardMonetaryCap::Capped(Cost::new(1_000_000)),
        HardMonetaryCap::Unknown,
    ] {
        for layer in 0..3 {
            let dir = tempfile::tempdir().unwrap();
            let mut s = open_unknown(dir.path());
            let mut evidence = std::array::from_fn(|_| CapEvidence::NotApplicable {
                origin: "test host composition".into(),
            });
            evidence[layer] = CapEvidence::Observed {
                cap,
                origin: "test configured limit".into(),
            };
            let [policy, machine, occurrence] = evidence;
            s.set_cost_host_evidence(CostHostEvidence::new(true, policy, machine, occurrence));
            assert!(matches!(s.turn("hello"), TurnOutcome::Refusal(_)));
            assert_eq!(peer.bodies().len(), 2);
        }
    }
    let mut s = open_unknown(dir.path());
    s.set_cost_host_evidence(CostHostEvidence::default());
    assert!(matches!(s.turn("hello"), TurnOutcome::Refusal(_)));
    assert_eq!(peer.bodies().len(), 2);
}
#[test]
fn wrong_model_source_revision_cancel_and_old_yes_cannot_spend() {
    let peer = Peer::start(vec![(200, unpriced_response("hello"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open_unknown(dir.path());
    asked(&s.turn("hello"));
    s.reasoner = Box::new(ProviderReasoner {
        model: MODEL.into(),
        label: "changed".into(),
    });
    assert!(matches!(s.turn("yes"), TurnOutcome::Refusal(_)));
    let mut s = open_unknown(dir.path());
    asked(&s.turn("hello"));
    std::fs::write(
        dir.path().join("new.nika"),
        "nika: changed\ntasks:\n  one:\n    exec: echo ok\n",
    )
    .unwrap();
    assert!(matches!(s.turn("yes"), TurnOutcome::Refusal(_)));
    asked(&s.turn("hello"));
    let _ = s.turn("hello revised request");
    assert!(!s.waiting_cost_choice());
    assert!(matches!(s.turn("yes"), TurnOutcome::Refusal(_)));
    asked(&s.turn("hello"));
    let _ = s.turn("cancel");
    assert!(matches!(s.turn("yes"), TurnOutcome::Refusal(_)));
    assert!(peer.bodies().is_empty());
}
#[test]
fn uncertain_dispatch_does_not_retry_or_mint_a_second_allowance() {
    let peer = Peer::start(vec![(503, json!({"error":"possibly billed"}))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open_unknown(dir.path());
    asked(&s.turn("hello"));
    let _ = s.turn("yes");
    assert_eq!(peer.bodies().len(), 1);
    assert_eq!(
        s.inference_receipt().unwrap().unwrap().state,
        AdmissionState::Uncertain
    );
    assert!(matches!(s.turn("hello"), TurnOutcome::Refusal(_)));
    let _ = s.turn("yes");
    assert_eq!(peer.bodies().len(), 1);
}
#[test]
fn deterministic_work_and_zero_constraints_still_need_no_http() {
    let peer = Peer::start(vec![(200, unpriced_response("unused"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open_unknown(dir.path());
    assert!(matches!(
        s.turn("hello budget 0 USD"),
        TurnOutcome::Refusal(_)
    ));
    assert!(peer.bodies().is_empty());
    let mut s = open_unknown(dir.path());
    let out = s.turn("Read ./notes/brief.md and write it to ./out/copy.md");
    assert!(!s.waiting_cost_choice(), "{out:?}");
    assert!(peer.bodies().is_empty());
}

#[test]
fn exact_scaleway_route_retains_native_eur_and_unknown_usd() {
    let mut body = response("hello");
    body["model"] = json!("gpt-oss-120b");
    body["usage"] = json!({"prompt_tokens":100,"completion_tokens":20,"total_tokens":120,
        "prompt_tokens_details":{"cached_tokens":0}});
    let peer = Peer::start(vec![(200, body)]);
    let _transport = test_transport::install(&peer.url);
    test_transport::set_config(
        nika_providers::ProvidersConfig::new()
            .with_key(
                "openai",
                nika_kernel::secret::Secret::new("fixture-not-a-key"),
            )
            .with_base_url("openai", "https://api.scaleway.ai/v1/chat/completions"),
    );
    let dir = tempfile::tempdir().unwrap();
    let mut s = open_unknown(dir.path());
    s.intelligence.kind = IntelligenceKind::Api {
        provider: "openai".into(),
    };
    s.intelligence.model = Some("openai/gpt-oss-120b".into());
    s.reasoner = Box::new(ProviderReasoner {
        model: "openai/gpt-oss-120b".into(),
        label: "Scaleway".into(),
    });
    s.refresh_seat();
    asked(&s.turn("hello"));
    assert!(peer.bodies().is_empty());
    assert!(matches!(s.turn("yes"), TurnOutcome::Reply(_)));
    assert_eq!(peer.bodies().len(), 1);
    assert_eq!(peer.bodies()[0]["max_completion_tokens"], 8192);
    let r = s.inference_receipt().unwrap().unwrap();
    assert_eq!(r.unknown_calls, 1);
    assert_eq!(r.unknown_attempts[0].currency.as_deref(), Some("EUR"));
    assert!(r.unknown_attempts[0].native_estimated_nano.is_some());
    assert_eq!(r.unknown_attempts[0].estimated, None);
    assert_eq!(r.billed, None);
}

#[test]
fn changed_endpoint_after_review_refuses_before_transport() {
    let peer = Peer::start(vec![(200, unpriced_response("unused"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open_unknown(dir.path());
    asked(&s.turn("hello"));
    test_transport::set_config(
        nika_providers::ProvidersConfig::new()
            .with_key(
                "deepseek",
                nika_kernel::secret::Secret::new("fixture-not-a-key"),
            )
            .with_base_url("deepseek", "https://different.example/v1/chat/completions"),
    );
    assert!(matches!(s.turn("yes"), TurnOutcome::Refusal(_)));
    assert!(peer.bodies().is_empty());
}

#[test]
fn revision_has_a_new_cost_question_and_cannot_apply_old_candidate_identity() {
    let peer = Peer::start(vec![
        (200, unpriced_response("NEW_WORK")),
        (200, unpriced_response(&native())),
    ]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open_unknown(dir.path());
    asked(&s.turn(WORK));
    let TurnOutcome::Proposal { id: old, .. } = s.turn("yes") else {
        panic!("initial candidate");
    };
    let first_count = peer.bodies().len();
    asked(&s.consent("Change the destination to ./revised.txt"));
    assert_eq!(peer.bodies().len(), first_count);
    let details = s.cost_choice_details().unwrap();
    assert!(details.contains("invocation"));
    let _ = s.turn("cancel");
    // Cancellation of a cost revision expires the pending work review too.
    assert!(matches!(s.consent_to(&old, "yes"), TurnOutcome::Refusal(_)));
    assert_eq!(peer.bodies().len(), first_count);
}

#[test]
fn confirmed_revision_produces_new_candidate_and_new_save_review() {
    let mut revised: Value =
        serde_json::from_str(&native().replace("./sortie.txt", "./revised.txt")).unwrap();
    revised["gaps"] = json!(["./sortie.txt is superseded by the requested ./revised.txt"]);
    let revised = revised.to_string();
    let peer = Peer::start(vec![
        (200, unpriced_response("NEW_WORK")),
        (200, unpriced_response(&native())),
        (200, unpriced_response("MODIFY")),
        (200, unpriced_response(&revised)),
    ]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open_unknown(dir.path());
    asked(&s.turn(WORK));
    let TurnOutcome::Proposal { id: old, .. } = s.turn("yes") else {
        panic!("initial candidate");
    };
    asked(&s.consent("Change the destination to ./revised.txt"));
    assert_eq!(peer.bodies().len(), 2);
    assert!(matches!(s.consent_to(&old, "yes"), TurnOutcome::Refusal(_)));
    let out = s.turn("yes");
    let TurnOutcome::Proposal { id: revised_id, .. } = out else {
        panic!("revised candidate: {out:?}");
    };
    assert_ne!(old, revised_id);
    assert_eq!(peer.bodies().len(), 4);
    assert!(matches!(s.consent_to(&old, "yes"), TurnOutcome::Refusal(_)));
    assert!(matches!(
        s.consent_to(&revised_id, "yes"),
        TurnOutcome::Facts(_)
    ));
    assert!(!dir.path().join("revised.txt").exists());
}

#[test]
fn a_custom_api_without_admission_capability_is_refused_before_any_call() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    let calls = Arc::new(AtomicUsize::new(0));
    s.reasoner = Box::new(super::scopes::LegacyCalls(calls.clone()));
    let out = s.turn("What can you tell me about stars?");
    assert!(
        matches!(out, TurnOutcome::Refusal(ref r)
        if r.text.contains("cannot enforce")),
        "{out:?}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
