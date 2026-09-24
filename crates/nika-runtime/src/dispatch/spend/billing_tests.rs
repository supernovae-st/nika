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
        (snapshot.by_source["fixture/exact @ https://one.example/complete"] - 0.05).abs()
            < f64::EPSILON
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

#[test]
fn s80_retry_receipt_fold_retains_all_calls_without_second_debit() {
    let mut split = Some(Box::new(UsageSplit::default().with_calls(&[known()])));
    UsageSplit::join_calls(&mut split, &[InferenceCall::new()], true);
    let split = split.expect("receipt");
    assert_eq!(split.inference_calls.len(), 2);
    assert_eq!(split.unknown_calls(), Some(1));
    assert_eq!(spend_for_calls(&split.inference_calls).0, Some(0.05));
}
