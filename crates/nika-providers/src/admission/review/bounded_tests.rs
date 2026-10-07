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
fn declared_free_openrouter_route_is_not_an_unknown_charge() {
    let free = CostRoute::observe(
        "openrouter/qwen/qwen3.8-27b:free",
        ProvidersConfig::new(),
    )
    .expect("free route");
    assert!(!free.needs_unknown_choice());
    let priced = CostRoute::observe(
        "openrouter/qwen/qwen3.8-max-0902",
        ProvidersConfig::new(),
    )
    .expect("priced route");
    assert!(priced.needs_unknown_choice());
    let missing = CostRoute::observe(
        "openrouter/some-vendor/not-in-the-snapshot",
        ProvidersConfig::new(),
    )
    .expect("missing route");
    assert!(missing.needs_unknown_choice());
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

const SEQUENTIAL_RUN_QUESTION: &str = "USD cost is unknown; a charge is possible on deepseek/unpriced-bounded-fixture.\nAt most 2 requests; each at most 8192 output tokens and 120 seconds (at most 240 seconds of model wait). Any schema re-asks consume this same request bound. No automatic transport retry.\nOverrides only the shown defaults (invocation: none; project: none); no hard cap is overridden.\nContinue once? yes / no";
#[test]
fn a_sequential_run_review_keeps_its_historical_question_bytes() {
    let run = review().for_run(2).expect("bound");
    assert_eq!(run.question(), SEQUENTIAL_RUN_QUESTION);
}
#[test]
fn host_evidence_names_its_own_refusal_before_any_review() {
    let local = CostHostEvidence::unmanaged_interactive_local();
    assert_eq!(local.unknown_cost_refusal(), None);
    assert!(CostHostEvidence::default().unknown_cost_refusal().is_some());
    let absent = |origin: &str| CapEvidence::NotApplicable {
        origin: origin.into(),
    };
    let machine = CapEvidence::Observed {
        cap: HardMonetaryCap::Capped(Cost::new(1)),
        origin: "machine ceiling".into(),
    };
    let capped = CostHostEvidence::new(true, absent("policy"), machine, absent("occurrence"));
    let why = capped.unknown_cost_refusal().expect("a hard cap refuses");
    let framed = CostReview::new(
        "candidate".into(),
        "invocation".into(),
        route(),
        capped,
        None,
        None,
    );
    assert_eq!(framed.expect_err("the same refusal"), why);
}
#[test]
fn session_and_single_attempt_reviews_keep_the_conservative_uncertain_law() {
    // Neither authored a task retry, so a received 503 never answers its
    // attempt: the account is Uncertain and nothing else may be sent.
    let route = route();
    for review in [review().for_session(), review().for_run(2).expect("bound")] {
        let account = review.confirm("candidate", &route).expect("fresh yes");
        let reserve = || account.reserve(&route.provider, &route.model, &route.endpoint, 32);
        let mut call = reserve().expect("first");
        call.sent().expect("send");
        call.answered(503, &route.endpoint);
        drop(call);
        assert!(reserve().is_err());
        let receipt = account.snapshot().expect("receipt");
        assert_eq!(receipt.state, crate::AdmissionState::Uncertain);
        let choice = serde_json::to_value(receipt.unknown_cost).expect("choice");
        assert!(choice.get("authored_retry").is_none(), "{choice}");
    }
}
#[test]
fn a_fan_review_shows_its_breakdown_and_confirms_its_concurrency() {
    let sequential = review()
        .for_run(2)
        .expect("bound")
        .with_concurrency(1)
        .expect("one at a time")
        .with_breakdown(Vec::new())
        .with_authored_retry(false);
    assert_eq!(sequential.question(), SEQUENTIAL_RUN_QUESTION);
    assert_eq!(review().for_session().max_in_flight(), 1);
    for width in [0, 7] {
        let widened = review().for_run(6).expect("bound").with_concurrency(width);
        assert!(widened.is_err(), "{width}");
    }
    let line = "`review`: 3 items × 2 attempts = 6 requests, at most 3 at once";
    let fan = review()
        .for_run(6)
        .expect("bound")
        .with_concurrency(3)
        .expect("within the total")
        .with_breakdown(vec![line.into()])
        .with_authored_retry(true);
    assert_eq!(fan.max_in_flight(), 3);
    let question = fan.question();
    let shown = format!(
        "No automatic transport retry.\n{line}\nAt most 3 in flight at once; authored retries and schema re-asks consume this same request bound.\nTask retries authored in the workflow (retry.max_attempts) may send a new request only after a completed response or a received 429 or 503; the transport never resends on its own, and any other failure stops every further request.\nOverrides only"
    );
    assert!(question.contains("At most 6 requests;"), "{question}");
    assert!(question.contains(&shown), "{question}");
    let route = route();
    let account = fan.confirm("candidate", &route).expect("fresh yes");
    let reserve = || account.reserve(&route.provider, &route.model, &route.endpoint, 32);
    let held: Vec<_> = (0..3).map(|_| reserve().expect("in flight")).collect();
    assert!(reserve().is_err(), "a fourth waits for a free slot");
    drop(held); // never sent: released, still counted
    let mut answered = reserve().expect("fourth");
    answered.sent().expect("send");
    answered.answered(429, &route.endpoint); // the confirmed choice carries the retry law
    drop(answered);
    let rest: Vec<_> = (0..2).map(|_| reserve().expect("in the total")).collect();
    drop(rest);
    assert!(reserve().is_err(), "the confirmed total is spent");
    let receipt = account.snapshot().expect("receipt");
    assert_eq!(receipt.state, crate::AdmissionState::Open);
    assert_eq!(receipt.unknown_calls, 1);
}
/// B12 r5 · the review names the triple it admits, and the confirmed choice
/// enforces exactly that triple: a host shows the owner's answer.
#[test]
fn a_review_names_the_bounds_its_confirmed_choice_enforces() {
    let session = review().for_session();
    assert_eq!(
        session.bounds(),
        (
            SESSION_REVIEW_MAX_REQUESTS,
            SESSION_REVIEW_MAX_OUTPUT_TOKENS,
            SESSION_REVIEW_TIMEOUT
        )
    );
    let run = review().for_run(6).expect("bound");
    let (requests, tokens, timeout) = run.bounds();
    assert_eq!(
        (requests, tokens, timeout),
        (6, RUN_REVIEW_MAX_OUTPUT_TOKENS, RUN_REVIEW_TIMEOUT)
    );
    let account = run.confirm("candidate", &route()).expect("fresh yes");
    let receipt = account.snapshot().expect("receipt");
    let choice = serde_json::to_value(receipt.unknown_cost).expect("choice");
    let millis = u64::try_from(timeout.as_millis()).expect("millis");
    assert_eq!(
        (
            &choice["max_requests"],
            &choice["max_output_tokens"],
            &choice["timeout_ms"]
        ),
        (
            &serde_json::json!(requests),
            &serde_json::json!(tokens),
            &serde_json::json!(millis)
        )
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
