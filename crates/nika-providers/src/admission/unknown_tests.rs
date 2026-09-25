// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Hermetic request accounting and authority counterexamples, no provider calls.
use super::*;
use nika_kernel::ai::provider::{ContentBlock, StopReason};
use std::time::Duration;

const URL: &str = "https://api.deepseek.com/v1/chat/completions";
fn choice() -> UnknownCostChoice {
    UnknownCostChoice::new(
        "candidate-a".into(),
        "invocation-a".into(),
        "deepseek".into(),
        "deepseek-v4-pro".into(),
        URL.into(),
        2,
        8192,
        Duration::from_secs(10),
    )
    .expect("choice")
}
fn policy() -> UnknownCostPolicy {
    UnknownCostPolicy::new(
        true,
        HardMonetaryCap::Absent,
        HardMonetaryCap::Absent,
        HardMonetaryCap::Absent,
        Some(Cost::zero()),
        Some(Cost::new(10)),
    )
}
fn account() -> InferenceAdmission {
    InferenceAdmission::new_unknown(choice(), policy())
        .expect("account")
        .for_scope("candidate-a", "invocation-a")
        .expect("bind")
}
fn complete() -> InferResponse {
    let mut usage = TokenUsage::new(100, 20);
    usage.cache_read_tokens = Some(0);
    let mut r = InferResponse::new(
        vec![ContentBlock::Text { text: "ok".into() }],
        usage,
        StopReason::EndTurn,
    );
    r.usage_completeness = UsageCompleteness::Complete;
    r.gen_ai.response_model = Some("deepseek-v4-pro".into());
    r
}
#[test]
fn explicit_choice_supersedes_only_soft_defaults_and_requires_actual_scope() {
    let a = InferenceAdmission::new_unknown(choice(), policy()).expect("soft defaults overridden");
    assert!(a.reserve("deepseek", "deepseek-v4-pro", URL, 8192).is_err());
    assert!(a.for_scope("candidate-b", "invocation-a").is_err());
    assert!(a.for_scope("candidate-a", "invocation-b").is_err());
    let bound = a
        .for_scope("candidate-a", "invocation-a")
        .expect("same scope");
    assert!(
        bound
            .reserve("deepseek", "deepseek-v4-pro", URL, 8192)
            .is_ok()
    );
    assert!(bound.amend(Cost::new(100)).is_err());
    assert!(
        InferenceAdmission::new(Cost::zero())
            .expect("strict")
            .reserve("deepseek", "deepseek-v4-pro", URL, 8192)
            .is_err()
    );
}
#[test]
fn hard_cap_unknown_or_policy_denial_cannot_be_overridden() {
    for cap in [
        HardMonetaryCap::Unknown,
        HardMonetaryCap::Capped(Cost::zero()),
        HardMonetaryCap::Capped(Cost::new(100)),
        HardMonetaryCap::Capped(Cost::new(-1)),
    ] {
        for slot in 0..3 {
            let mut caps = [HardMonetaryCap::Absent; 3];
            caps[slot] = cap;
            let p = UnknownCostPolicy::new(true, caps[0], caps[1], caps[2], None, None);
            assert!(InferenceAdmission::new_unknown(choice(), p).is_err());
        }
    }
    let p = UnknownCostPolicy::new(
        false,
        HardMonetaryCap::Absent,
        HardMonetaryCap::Absent,
        HardMonetaryCap::Absent,
        None,
        None,
    );
    assert!(InferenceAdmission::new_unknown(choice(), p).is_err());
}
#[test]
fn route_model_output_concurrency_and_request_count_are_enforced() {
    let a = account();
    for (provider, model, endpoint, tokens) in [
        ("openai", "deepseek-v4-pro", URL, 8192),
        ("deepseek", "deepseek-flash", URL, 8192),
        (
            "deepseek",
            "deepseek-v4-pro",
            "https://gateway.test/v1/chat/completions",
            8192,
        ),
        ("deepseek", "deepseek-v4-pro", URL, 0),
        ("deepseek", "deepseek-v4-pro", URL, 8193),
    ] {
        assert!(a.reserve(provider, model, endpoint, tokens).is_err());
    }
    for _ in 0..2 {
        let mut call = a
            .reserve("deepseek", "deepseek-v4-pro", URL, 8192)
            .expect("request");
        assert!(a.reserve("deepseek", "deepseek-v4-pro", URL, 8192).is_err());
        call.sent().expect("send");
        call.settle(&complete()).expect("settle");
    }
    assert!(a.reserve("deepseek", "deepseek-v4-pro", URL, 8192).is_err());
}
#[test]
fn known_subtotal_unknown_count_and_null_survive_observation_roundtrip() {
    let a = account();
    let mut first = a
        .reserve("deepseek", "deepseek-v4-pro", URL, 8192)
        .expect("first");
    first.sent().expect("send");
    first.settle(&complete()).expect("known");
    let known = a.snapshot().expect("snapshot").estimated;
    assert!(known.nano_usd > 0);
    let mut second = a
        .reserve("deepseek", "deepseek-v4-pro", URL, 8192)
        .expect("second");
    second.sent().expect("send");
    drop(second);
    let snap = a.snapshot().expect("snapshot");
    assert_eq!(snap.estimated, known);
    assert_eq!(snap.unknown_calls, 1);
    assert_eq!(snap.state, AdmissionState::Uncertain);
    assert!(a.reserve("deepseek", "deepseek-v4-pro", URL, 8192).is_err());
    let bytes = serde_json::to_vec(&snap.observation()).expect("serialize");
    let recovered: serde_json::Value = serde_json::from_slice(&bytes).expect("recover evidence");
    assert_eq!(
        recovered["known_subtotal_nano_usd"],
        serde_json::json!(known.nano_usd.to_string())
    );
    assert_eq!(recovered["unknown_calls"], 1);
    assert!(recovered["limit_nano_usd"].is_null());
    assert!(recovered["unknown_attempts"][1]["estimated_nano_usd"].is_null());
    assert_eq!(recovered["unknown_cost"]["endpoint"], URL);
    // No deserializer or restore method exists for an admission capability.
}
#[test]
fn declaration_rejects_nonfinite_negative_and_wrong_scope() {
    let c = choice();
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1, f64::MAX] {
        for axis in 0..3 {
            let mut rates = [1.0, 2.0, 0.1];
            rates[axis] = invalid;
            assert!(
                DeclaredTariff::new(
                    &c,
                    "my-biller".into(),
                    "USD".into(),
                    TariffUnit::PerMillionTokens,
                    rates,
                    "operator rate card".into(),
                    "2026-09-24/v1".into()
                )
                .is_err()
            );
        }
    }
    let tariff = DeclaredTariff::new(
        &c,
        "my-biller".into(),
        "EUR".into(),
        TariffUnit::PerMillionTokens,
        [0.15, 0.60, 0.15],
        "operator rate card".into(),
        "2026-09-24/v1".into(),
    )
    .expect("finite declared EUR");
    assert_eq!(
        tariff.price_native(1_000_000, 1_000_000, 0),
        Some(750_000_000)
    );
    assert!(tariff.price_native(1, 0, 2).is_none());
    let other = UnknownCostChoice::new(
        "c".into(),
        "i".into(),
        "deepseek".into(),
        "deepseek-flash".into(),
        URL.into(),
        1,
        1,
        Duration::from_secs(1),
    )
    .expect("other");
    assert!(other.with_declared_tariff(tariff).is_err());
}
