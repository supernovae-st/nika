// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Label requests through the real verb/registry with the existing test HTTP seam.
//! No live provider or model-quality claim follows from this fixture.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::disallowed_types
)]
use super::*;
use crate::turn::{
    ReasonerClassifier, RoutingMethod, SessionPhase, TurnAct, TurnClassifier, TurnContext,
};
use nika_error::cost::Cost;
use nika_providers::InferenceAdmission;
use nika_providers::admission::{HardMonetaryCap, UnknownCostChoice, UnknownCostPolicy};
use serde_json::json;
use std::time::Duration;

// Both test suites share one mechanics peer module.
use crate::runtime::inference_tests::wire::{Peer, response};

const MODEL: &str = "deepseek/deepseek-v4-pro";
fn reasoner(model: &str) -> ProviderReasoner {
    ProviderReasoner {
        model: model.into(),
        label: "fixture choice".into(),
    }
}
fn account() -> InferenceAdmission {
    InferenceAdmission::new(Cost::new(20_000_000_000)).unwrap()
}
fn context() -> TurnContext {
    TurnContext {
        phase: SessionPhase::QuestionPending,
        automation: Some("the existing work".into()),
        last_prompt: Some("Which destination?".into()),
    }
}

#[test]
fn label_ceiling_reads_catalog_capability_without_guessing_names() {
    for model in [
        MODEL,
        "deep-seek/deepseek-v4-pro",
        "deepseek/deepseek-flash",
        "deepseek/deepseek-reasoner",
        "openai/o3",
    ] {
        assert_eq!(reasoner(model).label_ceiling(), 4096, "{model}");
    }
    for model in [
        "deepseek/deepseek-chat",
        "deepseek/deepseek-future",
        "deepseek/deepseek-v4-pro-extra",
        "openai/deepseek-v4-pro",
        "openrouter/deepseek-v4-pro",
        "h100/deepseek-v4-pro",
        "openai/gpt-4.1",
        "openai/unobserved",
        "deepseek-v4-pro",
        "mock/echo",
    ] {
        assert_eq!(reasoner(model).label_ceiling(), 1024, "{model}");
    }
}

#[test]
fn first_reasoning_label_uses_the_same_route_and_shared_account() {
    let peer = Peer::start(vec![(200, response("DISCUSS"))]);
    let _http = test_transport::install(&peer.url);
    let account = account();
    let mut seat = reasoner(MODEL);
    let text = "explain the existing choice";
    let reply = seat.reason_label_with_admission(text, &account).unwrap();
    assert_eq!(reply.text, "DISCUSS");
    assert!(reply.usage_observed);
    assert_eq!(seat.authoring_model().as_deref(), Some(MODEL));
    let bodies = peer.bodies();
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0]["model"], "deepseek-v4-pro");
    assert_eq!(bodies[0]["max_tokens"], 4096);
    assert!(!bodies[0].to_string().contains("reasoning_effort"));
    assert!(bodies[0].to_string().contains(text));
    let first = account.snapshot().unwrap();
    assert_eq!(first.attempts.len(), 1);
    let attempt = &first.attempts[0];
    assert_eq!(attempt.model, "deepseek-v4-pro");
    assert_eq!(attempt.tariff.provider, "deepseek");
    assert_eq!(
        attempt.endpoint,
        "https://api.deepseek.com/v1/chat/completions"
    );
    assert_eq!(attempt.reserved, attempt.tariff.reserve(4096).unwrap());
    assert!(attempt.sent && attempt.estimated.is_some());
    assert_eq!(first.billed, None);
    seat.reason_label_with_admission(text, &account).unwrap();
    assert_eq!(peer.bodies().len(), 2); // explicit second invocation, same allowance
    assert_eq!(account.snapshot().unwrap().attempts.len(), 2);
}

#[test]
fn explicit_infer_limit_is_never_raised_to_the_label_default() {
    let peer = Peer::start(vec![(200, response("DISCUSS"))]);
    let _http = test_transport::install(&peer.url);
    let account = account();
    reasoner(MODEL)
        .infer("bounded", Some(64), Some(&account))
        .unwrap();
    assert_eq!(peer.bodies()[0]["max_tokens"], 64);
    let receipt = account.snapshot().unwrap();
    assert_eq!(
        receipt.attempts[0].reserved,
        receipt.attempts[0].tariff.reserve(64).unwrap()
    );
}

#[test]
fn zero_and_a_limit_that_only_funds_the_old_label_refuse_before_http() {
    let old_quote = nika_catalog::admission::InferenceTariff::deepseek("deepseek-v4-pro")
        .unwrap()
        .reserve(1024)
        .unwrap();
    for limit in [Cost::zero(), old_quote] {
        let peer = Peer::start(vec![(200, response("DISCUSS"))]);
        let _http = test_transport::install(&peer.url);
        let account = InferenceAdmission::new(limit).unwrap();
        assert!(
            reasoner(MODEL)
                .reason_label_with_admission("bounded", &account)
                .is_err()
        );
        assert!(peer.bodies().is_empty());
        let receipt = account.snapshot().unwrap();
        assert!(receipt.attempts.is_empty());
        assert!(receipt.refusal.is_some());
    }
}

#[test]
fn a_spent_shared_account_does_not_reset_for_another_classifier() {
    let peer = Peer::start(vec![(200, response("DISCUSS"))]);
    let _http = test_transport::install(&peer.url);
    let account = account();
    reasoner(MODEL)
        .reason_label_with_admission("first", &account)
        .unwrap();
    let spent = account.snapshot().unwrap().estimated;
    account.amend(spent).unwrap();
    assert!(
        reasoner(MODEL)
            .reason_label_with_admission("second", &account)
            .is_err()
    );
    assert_eq!(peer.bodies().len(), 1);
    assert_eq!(account.snapshot().unwrap().estimated, spent);
}

fn unknown_account(model: &str, output: u32) -> InferenceAdmission {
    let choice = UnknownCostChoice::new(
        "candidate".into(),
        "label".into(),
        "deepseek".into(),
        model.into(),
        "https://api.deepseek.com/v1/chat/completions".into(),
        1,
        output,
        Duration::from_secs(2),
    )
    .unwrap();
    let policy = UnknownCostPolicy::new(
        true,
        HardMonetaryCap::Absent,
        HardMonetaryCap::Absent,
        HardMonetaryCap::Absent,
        None,
        None,
    );
    InferenceAdmission::new_unknown(choice, policy)
        .unwrap()
        .for_scope("candidate", "label")
        .unwrap()
}

#[test]
fn explicit_unknown_choice_keeps_its_output_and_request_bounds() {
    for output in [1024, 4096] {
        let peer = Peer::start(vec![(200, response("DISCUSS"))]);
        let _http = test_transport::install(&peer.url);
        let account = unknown_account("deepseek-v4-pro", output);
        let first = reasoner(MODEL).reason_label_with_admission("first", &account);
        assert_eq!(first.is_ok(), output == 4096);
        assert!(
            reasoner(MODEL)
                .reason_label_with_admission("second", &account)
                .is_err()
        );
        let sent = usize::from(output == 4096);
        assert_eq!(peer.bodies().len(), sent);
        assert_eq!(account.snapshot().unwrap().unknown_attempts.len(), sent);
    }
}

#[test]
fn ordinary_and_unobserved_models_keep_the_old_outbound_label_limit() {
    for model in ["deepseek-chat", "deepseek-unobserved"] {
        let mut body = response("DISCUSS");
        body["model"] = json!(model);
        let peer = Peer::start(vec![(200, body)]);
        let _http = test_transport::install(&peer.url);
        let account = unknown_account(model, 1024);
        let mut seat = reasoner(&format!("deepseek/{model}"));
        seat.reason_label_with_admission("bounded fixture", &account)
            .unwrap();
        assert_eq!(peer.bodies().len(), 1);
        assert_eq!(peer.bodies()[0]["model"], model);
        assert_eq!(peer.bodies()[0]["max_tokens"], 1024);
        assert_eq!(account.snapshot().unwrap().unknown_attempts.len(), 1);
    }
}

#[test]
fn cap_or_empty_answer_stays_a_failed_route_with_original_input_and_no_retry() {
    let mut cap = response("");
    cap["choices"][0]["finish_reason"] = json!("length");
    cap["usage"]["completion_tokens"] = json!(4096);
    cap["usage"]["total_tokens"] = json!(4196);
    for body in [cap, response("")] {
        let peer = Peer::start(vec![(200, body)]);
        let _http = test_transport::install(&peer.url);
        let account = account();
        let mut classifier = ReasonerClassifier::new(Box::new(reasoner(MODEL)));
        let raw = "Explain this destination, without changing it.\nKeep my exact words.";
        let result = classifier.classify_with_admission(&context(), raw, &account);
        assert_eq!(result.method, RoutingMethod::Failed);
        assert_eq!(result.act, TurnAct::Unknown);
        assert!(result.note.as_ref().is_some_and(|n| n.contains("empty")));
        let bodies = peer.bodies();
        assert_eq!(bodies.len(), 1);
        assert_eq!(bodies[0]["max_tokens"], 4096);
        assert!(
            bodies[0]["messages"]
                .as_array()
                .unwrap()
                .iter()
                .any(|m| m["content"].as_str().is_some_and(|text| text.contains(raw)))
        );
        assert_eq!(account.snapshot().unwrap().attempts.len(), 1);
    }
}

#[test]
fn malformed_wire_or_label_never_causes_an_automatic_reask() {
    for (body, expected) in [
        (json!({"choices":[]}), RoutingMethod::Failed),
        (response("unrecognized label"), RoutingMethod::Model),
    ] {
        let peer = Peer::start(vec![(200, body)]);
        let _http = test_transport::install(&peer.url);
        let account = account();
        let mut classifier = ReasonerClassifier::new(Box::new(reasoner(MODEL)));
        let result =
            classifier.classify_with_admission(&context(), "leave everything unchanged", &account);
        assert_eq!(result.method, expected);
        assert_eq!(result.act, TurnAct::Unknown);
        assert_eq!(peer.bodies().len(), 1);
    }
}
