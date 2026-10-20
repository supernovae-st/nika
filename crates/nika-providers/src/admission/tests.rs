// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Admission mechanics; no paid provider or socket is used.
use super::*;
use nika_kernel::ai::provider::{ContentBlock, StopReason, UsageCompleteness};
use std::sync::{Arc, Barrier};

const ENDPOINT: &str = "https://api.deepseek.com/v1/chat/completions";
fn account() -> InferenceAdmission {
    InferenceAdmission::new(Cost::new(2_000_000_000)).expect("account")
}
fn lease(a: &InferenceAdmission) -> Attempt {
    a.reserve("deepseek", "deepseek-v4-pro", ENDPOINT, 8192)
        .expect("reservation")
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

const FREE_MODEL: &str = "nvidia/nemotron-3-super-120b-a12b:free";
const FREE_ENDPOINT: &str = "https://openrouter.ai/api/v1/chat/completions";

#[test]
fn an_exact_zero_tariff_fits_a_zero_monetary_allowance_and_settles() {
    let account = InferenceAdmission::new(Cost::zero()).expect("zero allowance");
    let mut call = account
        .reserve("openrouter", FREE_MODEL, FREE_ENDPOINT, 512)
        .expect("known zero is within zero USD");
    call.sent().expect("dispatch");
    let mut response = complete();
    response.gen_ai.response_model = Some(FREE_MODEL.into());
    // OpenRouter reports its own charge; its wire sets this zero only under the zero law.
    response.cost = Some(Cost::zero());
    call.settle(&response).expect("qualified complete response");
    let receipt = account.snapshot().expect("receipt");
    assert_eq!(receipt.estimated, Cost::zero());
    assert_eq!(receipt.attempts[0].estimated, Some(Cost::zero()));
    assert_eq!(receipt.unknown_calls, 0);
    assert_eq!(receipt.billed, None, "a tariff is never an invoice");
    assert_eq!(receipt.state, AdmissionState::Open);
    account.close("operator revoked calls").expect("revoke");
    assert!(
        account
            .reserve("openrouter", FREE_MODEL, FREE_ENDPOINT, 512)
            .is_err()
    );
}

/// A biller that reports its own charge settles only at that charge.
/// A complete reply without it stays unknown, even on a zero tariff.
#[test]
fn a_zero_tariff_never_settles_a_reply_without_its_reported_charge() {
    let account = InferenceAdmission::new(Cost::zero()).expect("zero allowance");
    let mut call = account
        .reserve("openrouter", FREE_MODEL, FREE_ENDPOINT, 512)
        .expect("zero reservation");
    call.sent().expect("dispatch");
    let mut response = complete();
    response.gen_ai.response_model = Some(FREE_MODEL.into());
    assert!(
        call.settle(&response).is_err(),
        "no reported charge, no settlement"
    );
    let receipt = account.snapshot().expect("receipt");
    assert_eq!(receipt.state, AdmissionState::Uncertain);
    assert_eq!(receipt.attempts[0].estimated, None);
}

#[test]
fn zero_price_never_discharges_missing_or_contradictory_execution_evidence() {
    for failure in ["dropped", "usage", "identity", "revoked"] {
        let account = InferenceAdmission::new(Cost::zero()).expect("zero allowance");
        let mut call = account
            .reserve("openrouter", FREE_MODEL, FREE_ENDPOINT, 512)
            .expect("zero reservation");
        if failure == "revoked" {
            account.close("operator revoked calls").expect("revoke");
            assert!(call.sent().is_err());
            drop(call);
            assert_eq!(account.snapshot().expect("receipt").unknown_calls, 0);
            continue;
        }
        call.sent().expect("dispatch");
        if failure != "dropped" {
            let mut response = complete();
            response.gen_ai.response_model = Some(FREE_MODEL.into());
            if failure == "usage" {
                response.usage_completeness = UsageCompleteness::Unknown;
            } else {
                response.gen_ai.response_model = Some("different-model".into());
            }
            assert!(call.settle(&response).is_err());
        }
        drop(call);
        let receipt = account.snapshot().expect("receipt");
        assert_eq!(receipt.state, AdmissionState::Uncertain, "{failure}");
        assert_eq!(receipt.unknown_calls, 1, "{failure}");
        assert_eq!(receipt.attempts[0].estimated, None, "{failure}");
        assert!(
            account
                .reserve("openrouter", FREE_MODEL, FREE_ENDPOINT, 512)
                .is_err()
        );
    }
}

#[test]
fn zero_price_route_observation_and_reservation_agree() {
    use crate::ProvidersConfig;
    let model = format!("openrouter/{FREE_MODEL}");
    let known = super::review::CostRoute::observe(&model, ProvidersConfig::new()).expect("route");
    assert!(!known.needs_unknown_choice());
    for (model, config) in [
        (
            "openrouter/vendor/unseen:free".into(),
            ProvidersConfig::new(),
        ),
        (
            "openrouter/qwen/qwen3.8-max-0902".into(),
            ProvidersConfig::new(),
        ),
        (format!("{model}-extra"), ProvidersConfig::new()),
        ("openrouter/openrouter/free".into(), ProvidersConfig::new()),
        (
            "openrouter/google/lyria-3-clip-preview".into(),
            ProvidersConfig::new(),
        ),
        (
            model,
            ProvidersConfig::new()
                .with_base_url("openrouter", "https://gateway.example/v1/chat/completions"),
        ),
    ] {
        let route = super::review::CostRoute::observe(&model, config).expect("route");
        assert!(route.needs_unknown_choice(), "{model}");
        assert!(
            InferenceAdmission::new(Cost::zero())
                .expect("account")
                .reserve(&route.provider, &route.model, &route.endpoint, 512)
                .is_err()
        );
    }
}
#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "OS threads exercise concurrent locking around a blocking barrier"
)]
fn concurrent_reservations_cannot_double_spend() {
    let a = account();
    let barrier = Arc::new(Barrier::new(3));
    let joins: Vec<_> = (0..2)
        .map(|_| {
            let (a, b) = (a.clone(), barrier.clone());
            std::thread::spawn(move || {
                b.wait();
                let reservation = a.reserve("deepseek", "deepseek-v4-pro", ENDPOINT, 8192);
                b.wait();
                reservation.is_ok()
            })
        })
        .collect();
    barrier.wait();
    barrier.wait();
    assert_eq!(
        joins
            .into_iter()
            .map(|j| usize::from(j.join().expect("thread")))
            .sum::<usize>(),
        1
    );
}
#[test]
fn complete_usage_releases_only_unused_allowance_and_amendment_keeps_spend() {
    let a = account();
    let mut call = lease(&a);
    call.sent().expect("dispatch");
    call.settle(&complete()).expect("settled");
    let before = a.snapshot().expect("snapshot");
    assert!(before.estimated.nano_usd > 0);
    assert_eq!(before.held_unknown, Cost::zero());
    assert_eq!(before.billed, None);
    a.amend(Cost::new(3_000_000_000)).expect("amend");
    assert_eq!(a.snapshot().expect("snapshot").estimated, before.estimated);
    assert!(a.amend(Cost::zero()).is_err());
    assert!(
        a.reserve("deepseek", "deepseek-v4-pro", ENDPOINT, 8192)
            .is_err()
    );
}
#[test]
fn dropped_or_partial_attempt_holds_and_freezes() {
    for partial in [false, true] {
        let a = account();
        let mut call = lease(&a);
        call.sent().expect("dispatch");
        if partial {
            let mut r = complete();
            r.usage_completeness = UsageCompleteness::Unknown;
            assert!(call.settle(&r).is_err());
        }
        drop(call);
        let s = a.snapshot().expect("snapshot");
        assert_eq!(s.state, AdmissionState::Uncertain);
        assert!(s.held_unknown.nano_usd > 0);
        assert!(a.amend(Cost::new(100_000_000_000)).is_err());
        assert!(
            a.reserve("deepseek", "deepseek-v4-pro", ENDPOINT, 8192)
                .is_err()
        );
    }
}
#[test]
fn unsent_release_zero_negative_and_endpoint_spoofing() {
    let a = account();
    drop(lease(&a));
    assert_eq!(a.snapshot().expect("snapshot").active, Cost::zero());
    for endpoint in [
        "https://api.deepseek.com.evil.test/v1/chat/completions",
        "http://api.deepseek.com/v1/chat/completions",
        "https://api.deepseek.com/v1/chat/completions?x=1",
        "http://127.0.0.1:9/v1/chat/completions",
    ] {
        assert!(
            a.reserve("deepseek", "deepseek-v4-pro", endpoint, 8192)
                .is_err()
        );
    }
    assert!(
        a.reserve("openai", "deepseek-v4-pro", ENDPOINT, 8192)
            .is_err()
    );
    assert!(
        a.reserve("deepseek", "deepseek-v4-pro-new", ENDPOINT, 8192)
            .is_err()
    );
    assert!(InferenceAdmission::new(Cost::new(-1)).is_err());
    assert!(
        InferenceAdmission::new(Cost::zero())
            .expect("zero account")
            .reserve("deepseek", "deepseek-v4-pro", ENDPOINT, 8192)
            .is_err()
    );
}

#[test]
fn equality_lowering_and_double_completion_preserve_every_nano() {
    let tariff = InferenceTariff::deepseek("deepseek-v4-pro").expect("tariff");
    let quote = tariff.reserve(8192).expect("quote");
    let a = InferenceAdmission::new(quote).expect("account");
    let mut call = lease(&a);
    assert!(
        a.reserve("deepseek", "deepseek-v4-pro", ENDPOINT, 8192)
            .is_err()
    );
    call.sent().expect("sent");
    assert!(call.sent().is_err());
    call.settle(&complete()).expect("settle");
    let s = a.snapshot().expect("snapshot");
    assert!(call.settle(&complete()).is_err());
    drop(call);
    assert_eq!(a.snapshot().expect("snapshot").estimated, s.estimated);
    a.amend(Cost::new(quote.nano_usd + s.estimated.nano_usd))
        .expect("raise total");
    let mut call = lease(&a);
    a.close("invalid amended ceiling").expect("close");
    assert!(call.sent().is_err());
    drop(call);
    assert_eq!(a.snapshot().expect("snapshot").active, Cost::zero());
}

#[test]
fn an_unbudgeted_account_observes_each_quote_without_any_allowance() {
    let quote = InferenceTariff::deepseek("deepseek-v4-pro")
        .expect("tariff")
        .reserve(8192)
        .expect("quote");
    let a = InferenceAdmission::unbudgeted();
    // Three full-context reservations at once: none is compared with a limit.
    let mut calls: Vec<Attempt> = (0..3).map(|_| lease(&a)).collect();
    for call in &mut calls {
        call.sent().expect("dispatch");
    }
    calls[0].settle(&complete()).expect("settled");
    let s = a.snapshot().expect("snapshot");
    assert!(s.unbudgeted);
    assert_eq!(s.state, AdmissionState::Open);
    assert_eq!(
        s.attempts[0].estimated,
        s.attempts[0].tariff.price(100, 20, 0)
    );
    assert_eq!(Some(s.estimated), s.attempts[0].estimated);
    assert_eq!(s.active.nano_usd, 2 * quote.nano_usd);
    assert_eq!(s.billed, None);
    let observation = s.observation();
    assert_eq!(observation["unbudgeted"], true);
    assert!(observation["limit_nano_usd"].is_null());
    assert!(
        a.amend(Cost::new(1_000_000_000_000)).is_err(),
        "an observation never becomes an allowance"
    );
    drop(calls);
    let s = a.snapshot().expect("snapshot");
    assert_eq!(s.state, AdmissionState::Uncertain);
    assert_eq!(s.held_unknown.nano_usd, 2 * quote.nano_usd);
    assert_eq!(s.unknown_calls, 2);
    assert!(
        a.reserve("deepseek", "deepseek-v4-pro", ENDPOINT, 8192)
            .is_err()
    );
}

#[test]
fn an_unbudgeted_account_keeps_qualification_and_the_contradiction_law() {
    let a = InferenceAdmission::unbudgeted();
    let spoofed = "http://127.0.0.1:9/v1/chat/completions";
    assert!(
        a.reserve("deepseek", "deepseek-v4-pro", spoofed, 8192)
            .is_err()
    );
    let mut call = lease(&a);
    call.sent().expect("dispatch");
    let mut served = complete();
    served.gen_ai.response_model = Some("deepseek-v4-flash".into());
    assert!(call.settle(&served).is_err());
    drop(call);
    let s = a.snapshot().expect("snapshot");
    assert_eq!(s.state, AdmissionState::Uncertain);
    assert_eq!(s.attempts[0].usage, Some(served.usage.clone()));
    assert_eq!(
        s.attempts[0].estimated, None,
        "never an invented settlement"
    );
    assert_eq!(s.unknown_calls, 1);
    let strict = account().snapshot().expect("snapshot").observation();
    assert!(
        strict.get("unbudgeted").is_none(),
        "existing bytes unchanged"
    );
    assert!(strict["limit_nano_usd"].is_string());
}

/// A GPT-6 call that wrote the prompt cache settles at the page's write rate, beside the read and
/// the plain input; GPT-5.5 lists no write rate, so the same reply stays unknown there.
#[test]
fn an_openai_cache_write_settles_only_where_the_page_prices_it() {
    const OPENAI: &str = "https://api.openai.com/v1/chat/completions";
    for (model, answer, priced) in [
        // 60 plain input at 2,000, 10 read at 200, 30 written at 2,500, 20 output at 10,000.
        (
            "gpt-6-sol",
            "gpt-6-sol",
            Some(60 * 2_000 + 10 * 200 + 30 * 2_500 + 20 * 10_000),
        ),
        ("gpt-5.5", "gpt-5.5-2026-04-23", None),
    ] {
        let a = InferenceAdmission::new(Cost::new(10_000_000_000)).expect("account");
        let mut call = a
            .reserve("openai", model, OPENAI, 8192)
            .expect("reservation");
        call.sent().expect("dispatch");
        let mut r = complete();
        r.usage.cache_read_tokens = Some(10);
        r.usage.cache_write_tokens = Some(30);
        r.gen_ai.response_model = Some(answer.into());
        assert_eq!(call.settle(&r).is_ok(), priced.is_some(), "{model}");
        let s = a.snapshot().expect("snapshot");
        assert_eq!(s.attempts[0].estimated, priced.map(Cost::new), "{model}");
    }
}

/// `OpenAI`'s direct row names the dated snapshot its replies report: a reply naming it, or the
/// model itself, settles the exact route at the row's price; an undocumented date or another
/// model leaves the charge unknown.
#[test]
fn an_openai_reply_naming_its_documented_snapshot_settles_the_route() {
    const OPENAI: &str = "https://api.openai.com/v1/chat/completions";
    for (answer, settles) in [
        ("gpt-4o-mini", true),
        ("gpt-4o-mini-2024-07-18", true),
        ("gpt-4o-mini-2099-01-01", false),
        ("gpt-4o", false),
    ] {
        let a = account();
        let mut call = a
            .reserve("openai", "gpt-4o-mini", OPENAI, 8192)
            .expect("reservation");
        call.sent().expect("dispatch");
        let mut r = complete();
        r.gen_ai.response_model = Some(answer.into());
        assert_eq!(call.settle(&r).is_ok(), settles, "{answer}");
        // 100 input tokens, none cached, at 150 nano-USD; 20 output tokens at 600.
        let priced = if settles { 100 * 150 + 20 * 600 } else { 0 };
        assert_eq!(
            a.snapshot().expect("snapshot").estimated,
            Cost::new(priced),
            "{answer}"
        );
    }
}
