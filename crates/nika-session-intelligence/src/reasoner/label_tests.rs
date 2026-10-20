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
use super::wire::{Peer, response};

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
        assert_eq!(label_ceiling(model), 4096, "{model}");
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
        assert_eq!(label_ceiling(model), 1024, "{model}");
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
        .infer(
            "bounded",
            Some(64),
            Some(&account),
            None,
            ReplyPurpose::Chat,
        )
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

#[test]
fn continuous_chat_uses_the_route_capacity_and_names_a_truncated_reply() {
    let mut truncated = response("partial answer");
    truncated["choices"][0]["finish_reason"] = json!("length");
    let peer = Peer::start(vec![(200, response("complete answer")), (200, truncated)]);
    let _http = test_transport::install(&peer.url);
    let costs = PreparationCosts::default();
    let _scope = costs.enter();
    let mut seat = reasoner(MODEL);
    assert_eq!(
        seat.reason("explain the choice").unwrap().text,
        "complete answer"
    );
    let reply = seat.reason("explain the rest").unwrap();
    assert!(reply.text.starts_with("partial answer"));
    assert!(reply.text.contains("Incomplete response"));
    let capacity = completion_bounds(MODEL, false, provider_config());
    let bodies = peer.bodies();
    assert_eq!(bodies.len(), 2);
    assert!(
        bodies
            .iter()
            .all(|b| b["max_tokens"] == capacity.max_tokens)
    );
}

#[test]
fn nonempty_truncated_classifiers_never_invent_or_accept_an_act() {
    for partial in ["DISCUSS", "DISCU", "Let me think"] {
        for continuous in [false, true] {
            for admitted in [false, true] {
                let mut body = response(partial);
                body["choices"][0]["finish_reason"] = json!("length");
                let peer = Peer::start(vec![(200, body)]);
                let _http = test_transport::install(&peer.url);
                let costs = PreparationCosts::default();
                let _scope = continuous.then(|| costs.enter());
                let account = account();
                let mut classifier = ReasonerClassifier::new(Box::new(reasoner(MODEL)));
                let raw = "Explain this destination without changing it.";
                let result = if admitted {
                    classifier.classify_with_admission(&context(), raw, &account)
                } else {
                    classifier.classify(&context(), raw)
                };
                assert_eq!(result.method, RoutingMethod::Failed, "{partial}");
                assert_eq!(result.act, TurnAct::Unknown, "{partial}");
                assert!(
                    result
                        .note
                        .as_ref()
                        .is_some_and(|n| n.contains("truncated"))
                );
                let bodies = peer.bodies();
                assert_eq!(bodies.len(), 1, "no automatic retry");
                // Continuous preparation asks the route's capacity; only an explicit
                // allowance keeps the historical label ceiling.
                let asked = if continuous {
                    completion_bounds(MODEL, false, provider_config()).max_tokens
                } else {
                    label_ceiling(MODEL)
                };
                assert_eq!(bodies[0]["max_tokens"], asked);
                assert!(
                    bodies[0]["messages"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|m| m["content"].as_str().is_some_and(|text| text.contains(raw)))
                );
                if admitted {
                    let receipt = account.snapshot().unwrap();
                    assert_eq!(receipt.attempts.len(), 1);
                    assert!(receipt.attempts[0].sent);
                    assert_eq!(
                        receipt.attempts[0].reserved,
                        receipt.attempts[0].tariff.reserve(asked).unwrap()
                    );
                }
            }
        }
    }
}

#[test]
fn continuous_scope_preserves_an_explicit_completion_ceiling_and_account() {
    let peer = Peer::start(vec![(200, response("complete answer"))]);
    let _http = test_transport::install(&peer.url);
    let costs = PreparationCosts::default();
    let _scope = costs.enter();
    let account = account();
    let reply = reasoner(MODEL)
        .infer(
            "bounded",
            Some(64),
            Some(&account),
            None,
            ReplyPurpose::Chat,
        )
        .unwrap();
    assert_eq!(reply.text, "complete answer");
    let bodies = peer.bodies();
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0]["max_tokens"], 64);
    let receipt = account.snapshot().unwrap();
    assert_eq!(receipt.attempts.len(), 1);
    assert_eq!(
        receipt.attempts[0].reserved,
        receipt.attempts[0].tariff.reserve(64).unwrap()
    );
}

/// A label call on a Claude 5 model asks no temperature, which its provider would refuse with
/// HTTP 400: the same ceilinged call, its body without the key; an earlier Claude model keeps the
/// zero temperature. The ceilinged-call temperature is judged per model, never per provider.
#[test]
fn a_claude_5_label_call_asks_no_temperature() {
    for (model, wire, temperature) in [
        ("anthropic/claude-sonnet-5-5", "claude-sonnet-5-5", None),
        ("anthropic/sonnet", "claude-sonnet-5-5", None),
        (
            "anthropic/claude-sonnet-4-6",
            "claude-sonnet-4-6",
            Some(0.0),
        ),
    ] {
        let reply = json!({
            "id": "msg_fixture", "model": wire,
            "content": [{"type": "text", "text": "DISCUSS"}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 2}
        });
        let peer = Peer::start(vec![(200, reply)]);
        let _http = test_transport::install(&peer.url);
        test_transport::set_config(nika_providers::ProvidersConfig::new().with_key(
            "anthropic",
            nika_kernel::secret::Secret::new("fixture-not-a-credential"),
        ));
        let text = reasoner(model)
            .reason_label("explain the choice")
            .unwrap()
            .text;
        assert_eq!(text, "DISCUSS", "{model}");
        let bodies = peer.bodies();
        assert_eq!(bodies.len(), 1, "{model}");
        assert_eq!(bodies[0]["model"], wire, "{model}");
        assert_eq!(bodies[0]["max_tokens"], label_ceiling(model), "{model}");
        assert_eq!(
            bodies[0]
                .get("temperature")
                .and_then(serde_json::Value::as_f64),
            temperature,
            "{model}: {}",
            bodies[0]
        );
        assert_eq!(label_temperature(model).map(f64::from), temperature);
    }
    for model in [
        "deepseek/deepseek-v4-pro",
        "openai/gpt-4o-mini",
        "mock/echo",
        "bare",
    ] {
        assert_eq!(label_temperature(model), Some(0.0), "{model}");
    }
}

/// NIK-152: a Session allowance with a Claude author qualifies, and each call settles at the
/// model's admission tariff: the label is reserved, sent once without a temperature and priced
/// from its complete usage (100 input tokens at 2,000 and 20 output at 10,000 nano-USD).
#[test]
fn a_claude_label_under_an_allowance_settles_at_its_tariff() {
    const SONNET: &str = "anthropic/claude-sonnet-5-5";
    let reply = json!({
        "id": "msg_fixture", "model": "claude-sonnet-5-5",
        "content": [{"type": "text", "text": "DISCUSS"}], "stop_reason": "end_turn",
        "usage": {"input_tokens": 100, "output_tokens": 20}
    });
    let peer = Peer::start(vec![(200, reply)]);
    let _http = test_transport::install(&peer.url);
    test_transport::set_config(nika_providers::ProvidersConfig::new().with_key(
        "anthropic",
        nika_kernel::secret::Secret::new("fixture-not-a-credential"),
    ));
    assert_eq!(
        InferenceAdmission::qualify_selected(SONNET, provider_config()),
        Ok(())
    );
    let account = account();
    let label = reasoner(SONNET)
        .reason_label_with_admission("explain the choice", &account)
        .unwrap();
    assert_eq!(label.text, "DISCUSS");
    let bodies = peer.bodies();
    assert_eq!(bodies.len(), 1);
    assert!(bodies[0].get("temperature").is_none(), "{}", bodies[0]);
    let receipt = account.snapshot().unwrap();
    assert_eq!(receipt.attempts.len(), 1);
    let attempt = &receipt.attempts[0];
    assert_eq!(
        (attempt.tariff.provider, attempt.model.as_str()),
        ("anthropic", "claude-sonnet-5-5")
    );
    assert!(attempt.sent);
    assert_eq!(
        attempt.estimated,
        Some(Cost::new(100 * 2_000 + 20 * 10_000))
    );
    assert_eq!(receipt.estimated, Cost::new(100 * 2_000 + 20 * 10_000));
}

/// NIK-153: a missing key reaches the conversation as the variable to set, the Nika-scoped one
/// first, without the command-line teaching a CLI error appends; nothing is sent.
#[test]
fn a_missing_key_is_said_without_the_command_line_teaching() {
    let peer = Peer::start(vec![(200, response("unused"))]);
    let _http = test_transport::install(&peer.url);
    test_transport::set_config(nika_providers::ProvidersConfig::new());
    let Err(ReasonError::Provider(said)) = reasoner("anthropic/claude-sonnet-5-5").reason("hello")
    else {
        panic!("a missing key refuses the turn");
    };
    assert_eq!(
        said,
        "authentication failed: no API key for 'anthropic' · set NIKA_ANTHROPIC_API_KEY, read \
         by Nika alone; ANTHROPIC_API_KEY also works (e.g. `export NIKA_ANTHROPIC_API_KEY=…`)"
    );
    assert!(
        !said.contains("--model") && !said.contains("timeout"),
        "{said}"
    );
    assert!(peer.bodies().is_empty(), "nothing was sent");
}

/// The TUI's « Test the key » door reads the configuration a run reads: with no key configured
/// it judges nothing and sends nothing (no automatic probe, no request without a key).
#[test]
fn a_key_check_without_a_key_sends_nothing() {
    let peer = Peer::start(vec![(200, response("unused"))]);
    let _http = test_transport::install(&peer.url);
    test_transport::set_config(nika_providers::ProvidersConfig::new());
    let checked = check_api_key("anthropic").unwrap();
    assert!(
        matches!(checked, nika_providers::KeyCheck::Unknown { ref why } if why.contains("no key")),
        "{checked:?}"
    );
    assert!(peer.bodies().is_empty());
}
