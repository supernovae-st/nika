// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used)]
use super::*;
use nika_kernel::ai::provider::{
    ContentBlock, InferResponse, StopReason, TokenUsage, UsageCompleteness,
};
fn review() -> CostReview {
    CostReview::new(
        "candidate".into(),
        "invocation".into(),
        route(),
        CostHostEvidence::unmanaged_interactive_local(),
        None,
        None,
    )
    .expect("review")
}
fn route() -> CostRoute {
    CostRoute::observe("deepseek/unpriced-bounded-fixture", ProvidersConfig::new()).expect("route")
}
#[test]
fn bound_is_displayed_without_overflow_and_zero_is_refused() {
    assert!(review().for_run(0).is_err());
    assert!(
        review()
            .for_run(2)
            .expect("bound")
            .question()
            .contains("At most 2 requests")
    );
    assert!(
        review()
            .for_run(u32::MAX)
            .expect("finite")
            .question()
            .contains("515396075400 seconds")
    );
    assert!(
        review()
            .for_session()
            .question()
            .contains("At most 3 requests")
    );
}
#[test]
fn confirmed_run_account_enforces_count_independently_of_host_structure() {
    let route = route();
    let account = review()
        .for_run(2)
        .expect("bound")
        .confirm("candidate", &route)
        .expect("fresh yes");
    let mut response = InferResponse::new(
        vec![ContentBlock::Text { text: "ok".into() }],
        TokenUsage::new(8, 2),
        StopReason::EndTurn,
    );
    response.usage_completeness = UsageCompleteness::Complete;
    response.gen_ai.response_model = Some(route.model.clone());
    for _ in 0..2 {
        let mut call = account
            .reserve(&route.provider, &route.model, &route.endpoint, 32)
            .expect("within bound");
        call.sent().expect("send");
        call.settle(&response).expect("settle unknown cost");
    }
    assert!(
        account
            .reserve(&route.provider, &route.model, &route.endpoint, 32)
            .is_err()
    );
    let receipt = account.snapshot().expect("observation");
    assert_eq!(receipt.unknown_calls, 2);
    assert_eq!(receipt.estimated.nano_usd, 0); // known subtotal only, NOT total cost
    assert!(
        receipt
            .unknown_attempts
            .iter()
            .all(|a| a.estimated.is_none())
    );
}
#[test]
fn a_changed_candidate_or_route_cannot_use_a_compound_review() {
    assert!(
        review()
            .for_run(2)
            .expect("bound")
            .confirm("changed", &route())
            .is_err()
    );
    let mut changed = route();
    changed.endpoint.push_str("/different");
    assert!(
        review()
            .for_run(2)
            .expect("bound")
            .confirm("candidate", &changed)
            .is_err()
    );
}
