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
    let session = review().for_session().question();
    assert!(
        session.contains(
            "At most 7 requests; each at most 32768 output tokens and 180 seconds (at most 1260 seconds of model wait)"
        ),
        "{session}"
    );
    // A Run review keeps its own bounds whatever a Session review admits.
    let run = review().for_run(2).expect("bound").question();
    assert!(
        run.contains("each at most 8192 output tokens and 120 seconds"),
        "{run}"
    );
}

#[test]
fn a_session_review_admits_the_sessions_widened_output_and_a_run_review_does_not() {
    let route = route();
    let session = review().for_session();
    assert_eq!(
        (
            session.max_requests(),
            session.max_output_tokens(),
            session.request_timeout()
        ),
        (
            SESSION_REVIEW_MAX_REQUESTS,
            SESSION_REVIEW_MAX_OUTPUT_TOKENS,
            SESSION_REVIEW_TIMEOUT
        )
    );
    let account = session.confirm("candidate", &route).expect("fresh yes");
    // The first native call (16384) and a truncation-widened repair (32768) both fit.
    for output in [16_384, SESSION_REVIEW_MAX_OUTPUT_TOKENS] {
        let call = account
            .reserve(&route.provider, &route.model, &route.endpoint, output)
            .expect("within the session review");
        drop(call); // never dispatched: released, still counted against the request bound
    }
    assert!(
        account
            .reserve(
                &route.provider,
                &route.model,
                &route.endpoint,
                SESSION_REVIEW_MAX_OUTPUT_TOKENS + 1
            )
            .is_err(),
        "the hard ceiling holds"
    );
    let run = review()
        .for_run(2)
        .expect("bound")
        .confirm("candidate", &route)
        .expect("fresh yes");
    assert!(
        run.reserve(&route.provider, &route.model, &route.endpoint, 16_384)
            .is_err(),
        "a Run review never admits the Session's widened output"
    );
}

#[test]
fn the_session_review_counts_its_eighth_request_as_over_the_bound() {
    let route = route();
    let account = review()
        .for_session()
        .confirm("candidate", &route)
        .expect("fresh yes");
    let mut response = InferResponse::new(
        vec![ContentBlock::Text { text: "ok".into() }],
        TokenUsage::new(8, 2),
        StopReason::EndTurn,
    );
    response.usage_completeness = UsageCompleteness::Complete;
    response.gen_ai.response_model = Some(route.model.clone());
    for _ in 0..SESSION_REVIEW_MAX_REQUESTS {
        let mut call = account
            .reserve(&route.provider, &route.model, &route.endpoint, 8192)
            .expect("within bound");
        call.sent().expect("send");
        call.settle(&response).expect("settle unknown cost");
    }
    assert!(
        account
            .reserve(&route.provider, &route.model, &route.endpoint, 8192)
            .is_err(),
        "an eighth request is refused"
    );
    assert_eq!(
        account.snapshot().expect("observation").unknown_calls,
        SESSION_REVIEW_MAX_REQUESTS as usize
    );
}

#[test]
fn a_numeric_allowance_is_never_widened_by_the_session_review() {
    // A priced route under a numeric allowance never reaches the unknown-cost review: the
    // Session's widened output is reserved at its full catalog worst case and the allowance
    // admits exactly what its limit covers, with no request count and no unknown scope.
    const ENDPOINT: &str = "https://api.deepseek.com/v1/chat/completions";
    let tariff =
        nika_catalog::admission::InferenceTariff::deepseek("deepseek-v4-pro").expect("tariff");
    let first = tariff.reserve(16_384).expect("quote");
    let widened = tariff
        .reserve(SESSION_REVIEW_MAX_OUTPUT_TOKENS)
        .expect("quote");
    assert!(
        widened.nano_usd > first.nano_usd,
        "the widened output costs more"
    );
    let account = InferenceAdmission::new(first).expect("allowance");
    assert!(
        account
            .reserve(
                "deepseek",
                "deepseek-v4-pro",
                ENDPOINT,
                SESSION_REVIEW_MAX_OUTPUT_TOKENS
            )
            .is_err(),
        "the widened output does not fit an allowance sized for the first call"
    );
    let call = account
        .reserve("deepseek", "deepseek-v4-pro", ENDPOINT, 16_384)
        .expect("exactly covered");
    assert!(
        account
            .reserve("deepseek", "deepseek-v4-pro", ENDPOINT, 1)
            .is_err(),
        "the allowance is spent, whatever a Session review would admit"
    );
    drop(call);
    let receipt = account.snapshot().expect("receipt");
    assert_eq!(receipt.limit, first, "the allowance itself is unchanged");
    assert!(receipt.unknown_cost.is_none());
    assert_eq!(receipt.unknown_calls, 0);
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
