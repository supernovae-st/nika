// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
use super::{failed_usage_split, price_failed_spend, spend_for_calls};
use crate::usage::{UsageSplit, push_usage_fields};
use nika_kernel::provider::TokenUsage;
use nika_types::cost::{Cost, InferenceCall, InferenceRoute, SpendOnFailure};

fn known() -> InferenceCall {
    let mut c = InferenceCall::new();
    c.route = Some(InferenceRoute::new(
        "fixture".into(),
        "exact".into(),
        "https://one.example/complete".into(),
    ));
    c.response_model = Some("exact".into());
    c.usage = Some(TokenUsage::new(10, 2));
    c.usage_complete = true;
    c.pricing = Some(r#"{"kind":"test_fixture","currency":"USD"}"#.into());
    c.estimated_usd = Some(Cost::new(50_000_000));
    c
}

#[test]
fn s80_partial_failure_prices_known_calls_and_tools_keeps_unknown_count() {
    let spend = SpendOnFailure::new(
        TokenUsage::new(10, 2),
        Some(0.25),
        Some("requested/model".into()),
    )
    .with_inference_calls(vec![known(), InferenceCall::new(), InferenceCall::new()]);
    let (cost, _, why) = price_failed_spend(Some(&spend));
    let recovered: SpendOnFailure =
        serde_json::from_str(&serde_json::to_string(&spend).expect("receipt")).expect("recovery");
    assert_eq!(price_failed_spend(Some(&recovered)).0, cost);
    assert_eq!(
        failed_usage_split(Some(&recovered))
            .expect("receipt")
            .unknown_calls(),
        Some(2)
    );
    assert_eq!(cost, Some(0.30));
    assert!(why.is_some());
    let split = failed_usage_split(Some(&spend)).expect("even empty usage carries evidence");
    assert_eq!(split.unknown_calls(), Some(2));
    let ledger = crate::ledger::RunLedger::new(None);
    ledger.debit_observed(Some("requested/model"), cost, true, Some(&split));
    let snapshot = ledger.snapshot();
    assert_eq!(snapshot.unpriced_calls, 2);
    assert!((snapshot.spent_usd - 0.30).abs() < f64::EPSILON);
    assert!(
        (snapshot.by_source["fixture/exact @ https://one.example:443"] - 0.05).abs() < f64::EPSILON
    );
    assert!(
        snapshot
            .by_source
            .keys()
            .all(|key| !key.contains("/complete"))
    );
    assert!((snapshot.by_source["requested/model (tools)"] - 0.25).abs() < f64::EPSILON);
    assert!(!snapshot.by_source.contains_key("requested/model"));
}

#[test]
fn s80_agent_success_consumer_keeps_each_route_and_unknown() {
    let mut other = known();
    other.route.as_mut().expect("route").endpoint = "https://two.example/complete".into();
    other.estimated_usd = None;
    let out = nika_verb_agent::AgentOutput::new(
        nika_verb_agent::AgentValue::Text("done".into()),
        nika_kernel::runtime::agent::AgentStopReason::Completed,
        2,
        24,
    )
    .with_spend_identity(TokenUsage::new(20, 4), "requested/model".into())
    .with_tools_cost_usd(Some(0.25))
    .with_inference_calls(vec![known(), other]);
    let dispatched = super::super::verb_outcome::agent_success(out, None);
    let Ok(ok) = dispatched.result else {
        panic!("success");
    };
    assert_eq!(ok.cost_usd, Some(0.30));
    assert!(ok.cost_unpriced.is_some());
    let split = ok.usage.as_deref().expect("usage");
    assert_eq!(split.unknown_calls(), Some(1));
    assert_ne!(
        split.inference_calls[0].route,
        split.inference_calls[1].route
    );
}

#[test]
fn s80_unknown_only_is_absent_money_and_survives_trace_and_recovery() {
    let spend = SpendOnFailure::default().with_inference_calls(vec![InferenceCall::new()]);
    assert_eq!(price_failed_spend(Some(&spend)).0, None);
    let encoded = serde_json::to_string(&spend).expect("persist");
    let recovered: SpendOnFailure = serde_json::from_str(&encoded).expect("recover");
    let split = failed_usage_split(Some(&recovered)).expect("unknown dispatch survives");
    let mut fields = Vec::new();
    push_usage_fields(&mut fields, Some(&split));
    assert!(
        fields
            .iter()
            .any(|(key, value)| *key == "cost_unknown_calls" && value == &crate::i(1))
    );
    assert!(fields.iter().any(|(key, _)| *key == "inference_calls"));
    let ledger = crate::ledger::RunLedger::new(None);
    ledger.debit_observed(None, None, true, Some(&split));
    assert!(!ledger.snapshot().any_priced);
    assert_eq!(ledger.snapshot().unpriced_calls, 1);
}

#[test]
fn s80_returned_model_and_absent_usage_cannot_reuse_a_known_estimate() {
    for mutate in [
        (|c: &mut InferenceCall| c.response_model = Some("other".into())) as fn(&mut InferenceCall),
        |c| c.usage = None,
        |c| c.usage_complete = false,
        |c| c.route = None,
        |c| c.estimated_usd = Some(Cost::new(-1)),
    ] {
        let mut call = known();
        mutate(&mut call);
        assert_eq!(spend_for_calls(&[call]).0, None);
    }
}

/// O-L12: two priced requests through `DeepSeek`'s two curated paths share one
/// origin label and sum under it; another origin keeps its own; an unknown call
/// is counted, never keyed. Totals and counters are the fold of the same calls:
/// the label is presentation, each call is still debited by its own estimate.
#[test]
fn same_origin_routes_share_one_label_and_totals_are_unchanged() {
    let priced = |endpoint: &str, nano: i128| {
        let mut call = known();
        call.route = Some(InferenceRoute::new(
            "deepseek".into(),
            "deepseek-v4-pro".into(),
            endpoint.into(),
        ));
        call.response_model = Some("deepseek-v4-pro".into());
        call.estimated_usd = Some(Cost::new(nano));
        call
    };
    let calls = [
        priced("https://api.deepseek.com/v1/chat/completions", 50_000_000),
        priced("https://api.deepseek.com/chat/completions", 20_000_000),
        priced("https://gateway.example/v1/chat/completions", 5_000_000),
        InferenceCall::new(),
    ];
    let ledger = crate::ledger::RunLedger::new(None);
    let known = ledger.debit_calls(&calls);
    let snapshot = ledger.snapshot();
    assert!((known - 0.075).abs() < 1e-12);
    assert!((snapshot.spent_usd - 0.075).abs() < 1e-12);
    assert_eq!((snapshot.priced_calls, snapshot.unpriced_calls), (3, 1));
    let label = "deepseek/deepseek-v4-pro @ https://api.deepseek.com:443";
    let other = "deepseek/deepseek-v4-pro @ https://gateway.example:443";
    assert_eq!(
        snapshot.by_source.keys().collect::<Vec<_>>(),
        [label, other]
    );
    assert!((snapshot.by_source[label] - 0.07).abs() < 1e-12);
    assert!((snapshot.by_source[other] - 0.005).abs() < 1e-12);
    let mut fields = Vec::new();
    push_usage_fields(&mut fields, Some(&UsageSplit::default().with_calls(&calls)));
    let Some((_, crate::FieldValue::String(text))) =
        fields.iter().find(|(key, _)| *key == "inference_calls")
    else {
        panic!("inference_calls rides: {fields:?}");
    };
    assert!(!text.contains("/chat/completions"), "{text}");
    let durable: serde_json::Value = serde_json::from_str(text).expect("durable calls");
    assert_eq!(
        durable[1]["route"]["origin"],
        "https://api.deepseek.com:443"
    );
    assert_eq!(durable[0]["estimate_known"], true);
    assert_eq!(durable[3]["estimate_known"], false);
}

#[test]
fn s80_retry_receipt_fold_retains_all_calls_without_second_debit() {
    let mut split = Some(Box::new(UsageSplit::default().with_calls(&[known()])));
    UsageSplit::join_calls(&mut split, &[InferenceCall::new()], true);
    let split = split.expect("receipt");
    assert_eq!(split.inference_calls.len(), 2);
    assert_eq!(split.unknown_calls(), Some(1));
    assert_eq!(spend_for_calls(&split.inference_calls).0, Some(0.05));
}
