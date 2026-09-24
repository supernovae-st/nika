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
