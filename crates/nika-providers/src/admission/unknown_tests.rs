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

fn fan(total: u32, width: u32) -> InferenceAdmission {
    let wide = UnknownCostChoice::new(
        "candidate-a".into(),
        "invocation-a".into(),
        "deepseek".into(),
        "deepseek-v4-pro".into(),
        URL.into(),
        total,
        8192,
        Duration::from_secs(10),
    )
    .and_then(|c| c.with_max_in_flight(width))
    .expect("finite fan choice")
    .with_authored_retry();
    InferenceAdmission::new_unknown(wide, policy())
        .expect("account")
        .for_scope("candidate-a", "invocation-a")
        .expect("bind")
}
fn take(a: &InferenceAdmission) -> Result<Attempt, ProviderError> {
    a.reserve("deepseek", "deepseek-v4-pro", URL, 8192)
}
#[test]
fn concurrency_widens_only_explicitly_and_every_slot_is_released_once() {
    for width in [0, 3] {
        assert!(choice().with_max_in_flight(width).is_err(), "{width}");
    }
    // One in flight and no authored retry stay the historical choice, byte for byte.
    let historical = serde_json::to_value(choice()).expect("choice");
    assert!(historical.get("max_in_flight").is_none());
    assert!(historical.get("authored_retry").is_none());
    let one = choice().with_max_in_flight(1).expect("sequential");
    assert_eq!(serde_json::to_value(one).expect("choice"), historical);
    let two = choice().with_max_in_flight(2).expect("within the total");
    let two = serde_json::to_value(two.with_authored_retry()).expect("choice");
    assert_eq!(
        (&two["max_in_flight"], &two["authored_retry"]),
        (&2.into(), &true.into())
    );
    let a = fan(4, 2);
    let mut first = take(&a).expect("first");
    let mut second = take(&a).expect("second in flight");
    assert!(take(&a).is_err(), "at most two in flight");
    first.sent().expect("send");
    first.settle(&complete()).expect("settle");
    first.answered(429, URL); // a settled attempt is never answered again
    drop(first);
    let third = take(&a).expect("the settled slot is free");
    assert!(
        take(&a).is_err(),
        "settle, answer and drop released one slot"
    );
    drop(third); // never sent: released, still counted against the total
    let mut fourth = take(&a).expect("fourth");
    for call in [&mut second, &mut fourth] {
        call.sent().expect("send");
        call.settle(&complete()).expect("settle");
    }
    assert!(take(&a).is_err(), "every reservation counts, sent or not");
    let snap = a.snapshot().expect("snapshot");
    assert_eq!(snap.unknown_attempts.len(), 4);
    assert_eq!(snap.unknown_attempts.iter().filter(|x| x.sent).count(), 3);
    assert_eq!(snap.unknown_attempts[2].note, "not dispatched");
    assert_eq!(snap.state, AdmissionState::Open);
}
proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(48))]
    #[test]
    fn simultaneous_racers_never_exceed_the_total_or_the_in_flight_bound(
        total in 1u32..=6,
        width in 1u32..=6,
        racers in 1usize..=8,
    ) {
        let width = width.min(total);
        let a = fan(total, width);
        let mut dispatched = 0usize;
        let gate = std::sync::Barrier::new(racers);
        for _wave in 0..=total {
            let held: Vec<Attempt> = std::thread::scope(|s| {
                let racing: Vec<_> = (0..racers)
                    .map(|_| s.spawn(|| { gate.wait(); take(&a).ok() }))
                    .collect();
                racing.into_iter().filter_map(|r| r.join().expect("racer")).collect()
            });
            let left = total as usize - dispatched;
            proptest::prop_assert_eq!(held.len(), racers.min(width as usize).min(left));
            dispatched += held.len();
            for mut call in held {
                call.sent().expect("send");
                call.settle(&complete()).expect("settle");
            }
        }
        proptest::prop_assert_eq!(dispatched, total as usize);
        let snap = a.snapshot().expect("snapshot");
        proptest::prop_assert_eq!(snap.unknown_attempts.len(), total as usize);
        proptest::prop_assert_eq!(snap.state, AdmissionState::Open);
    }
}
#[test]
fn a_sibling_left_uncertain_revokes_pending_and_new_dispatch() {
    let a = fan(4, 3);
    let mut left = take(&a).expect("first");
    let mut answered = take(&a).expect("second");
    let mut pending = take(&a).expect("third, reserved but not sent");
    left.sent().expect("send");
    answered.sent().expect("send");
    drop(left); // sent without settlement: possibly billed
    assert!(
        pending.sent().is_err(),
        "no dispatch after an Uncertain sibling"
    );
    answered
        .settle(&complete())
        .expect("a response already in flight is still recorded");
    drop(pending);
    assert!(take(&a).is_err(), "free slots and total grant nothing now");
    let snap = a.snapshot().expect("snapshot");
    assert_eq!(snap.state, AdmissionState::Uncertain);
    assert_eq!(snap.unknown_attempts.iter().filter(|x| x.sent).count(), 2);
    assert_eq!(snap.unknown_calls, 1);
    assert_eq!(snap.unknown_attempts[2].note, "not dispatched");
}
#[test]
fn an_answered_status_keeps_an_uncertain_sibling_and_every_count() {
    let a = fan(4, 3);
    let mut left = take(&a).expect("first");
    let mut answered = take(&a).expect("second");
    left.sent().expect("send");
    answered.sent().expect("send");
    drop(left); // possibly billed: the account is Uncertain
    answered.answered(429, URL);
    drop(answered);
    let snap = a.snapshot().expect("snapshot");
    assert_eq!(
        snap.state,
        AdmissionState::Uncertain,
        "an answer never reopens"
    );
    assert_eq!((snap.unknown_attempts.len(), snap.unknown_calls), (2, 2));
    assert!(
        snap.unknown_attempts
            .iter()
            .all(|x| x.sent && x.estimated.is_none())
    );
    assert_eq!(
        snap.unknown_attempts[1].note,
        "answered HTTP 429; usage and USD cost unknown"
    );
    assert!(take(&a).is_err(), "no retry after an Uncertain sibling");
}
#[test]
fn a_received_429_or_503_permits_only_an_authored_retry_inside_the_total() {
    for status in [429, 503] {
        // Without an authored retry the historical law holds: Uncertain.
        let historical = account();
        let mut call = take(&historical).expect("first");
        call.sent().expect("send");
        call.answered(status, URL);
        drop(call);
        let snap = historical.snapshot().expect("snapshot");
        assert_eq!(snap.state, AdmissionState::Uncertain, "{status}");
        assert!(take(&historical).is_err(), "{status}");
        let a = fan(2, 1);
        let mut first = take(&a).expect("first");
        first.sent().expect("send");
        first.answered(status, URL);
        drop(first);
        let snap = a.snapshot().expect("snapshot");
        assert_eq!(snap.state, AdmissionState::Open, "{status}");
        assert_eq!(snap.unknown_calls, 1, "answered: usage unknown, never zero");
        assert_eq!(snap.unknown_attempts[0].estimated, None);
        assert_eq!(
            snap.unknown_attempts[0].note,
            format!("answered HTTP {status}; usage and USD cost unknown")
        );
        let mut retry = take(&a).expect("an authored retry inside the total");
        retry.sent().expect("send");
        retry.answered(status, URL);
        drop(retry);
        assert!(take(&a).is_err(), "the original total bounds every retry");
        assert_eq!(a.snapshot().expect("snapshot").unknown_calls, 2);
    }
}
#[test]
fn other_statuses_changed_endpoints_and_unsent_attempts_are_never_answered() {
    for (status, endpoint) in [
        (500, URL),
        (408, URL),
        (200, URL),
        (429, "https://api.deepseek.com/v1/chat/completions/"),
        (503, "https://gateway.test/v1/chat/completions"),
    ] {
        let a = fan(2, 1);
        let mut call = take(&a).expect("first");
        call.sent().expect("send");
        call.answered(status, endpoint);
        drop(call);
        let snap = a.snapshot().expect("snapshot");
        assert_eq!(snap.state, AdmissionState::Uncertain, "{status} {endpoint}");
        assert_eq!(
            snap.unknown_attempts[0].note,
            "possibly billed; no automatic retry"
        );
        assert!(take(&a).is_err(), "{status} {endpoint}");
    }
    let a = fan(2, 1);
    let mut unsent = take(&a).expect("first");
    unsent.answered(429, URL);
    drop(unsent);
    let snap = a.snapshot().expect("snapshot");
    assert_eq!(snap.state, AdmissionState::Open);
    assert_eq!(snap.unknown_calls, 0);
    assert_eq!(snap.unknown_attempts[0].note, "not dispatched");
}

/// Endpoints an unknown-cost choice never binds: forms the URL parser would
/// rewrite (backslash, case, an explicit default port, a raw Unicode host, a
/// space, no path) and parts a route must not carry (userinfo, query,
/// fragment); plain HTTP and an `@` in the path stay refused as before.
const NONCANONICAL: [&str; 11] = [
    "https://api.deepseek.com\\v1\\chat\\completions",
    "https://API.deepseek.com/v1/chat/completions",
    "https://api.deepseek.com:443/v1/chat/completions",
    "https://dëepseek.example/v1/chat/completions",
    "https://api.deepseek.com/v1/chat completions",
    "https://api.deepseek.com",
    "https://user:pw@api.deepseek.com/v1/chat/completions",
    "https://api.deepseek.com/v1/chat/completions?k=v",
    "https://api.deepseek.com/v1/chat/completions#f",
    "http://api.deepseek.com/v1/chat/completions",
    "https://api.deepseek.com/v1/@chat/completions",
];

fn choice_at(endpoint: &str) -> Result<UnknownCostChoice, ProviderError> {
    UnknownCostChoice::new(
        "candidate-a".into(),
        "invocation-a".into(),
        "deepseek".into(),
        "deepseek-v4-pro".into(),
        endpoint.into(),
        2,
        8192,
        Duration::from_secs(10),
    )
}

#[test]
fn a_choice_binds_only_a_canonical_endpoint() {
    for endpoint in NONCANONICAL {
        assert!(choice_at(endpoint).is_err(), "{endpoint}");
    }
    let exact = choice_at(URL).expect("canonical");
    assert_eq!(exact.endpoint(), URL, "the exact endpoint is kept whole");
    assert_eq!(
        exact.origin().as_deref(),
        Some("https://api.deepseek.com:443")
    );
    let gateway = "https://gateway.example/Tenant-A/v1/chat/completions";
    assert_eq!(
        choice_at(gateway).expect("a canonical path").endpoint(),
        gateway
    );
}

/// Counts every request that reaches the transport and answers none.
struct Counted(std::sync::atomic::AtomicUsize);
impl nika_kernel::http::HttpPostDyn for Counted {
    fn supports_single_attempt(&self) -> bool {
        true
    }
    async fn post(
        &self,
        _: nika_kernel::http::HttpRequest,
    ) -> Result<nika_kernel::http::HttpResponse, nika_kernel::http::HttpError> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(nika_kernel::http::HttpError::Connection {
            reason: "counted".into(),
        })
    }
    async fn send_streaming(
        &self,
        _: nika_kernel::http::HttpRequest,
    ) -> Result<nika_kernel::http::HttpStreamResponse, nika_kernel::http::HttpError> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(nika_kernel::http::HttpError::Connection {
            reason: "counted".into(),
        })
    }
}

/// A control, before and after the canonical law: a Run observer's route to a
/// noncanonical endpoint crosses no transport.
#[tokio::test]
async fn a_noncanonical_route_reaches_no_transport() {
    use nika_kernel::ai::provider::{InferRequest, Message, ProviderInferDyn, Role};
    for endpoint in &NONCANONICAL[..6] {
        let http = std::sync::Arc::new(Counted(std::sync::atomic::AtomicUsize::new(0)));
        let config = crate::ProvidersConfig::new()
            .with_key("deepseek", nika_kernel::secret::Secret::new("fixture"))
            .with_base_url("deepseek", *endpoint);
        let registry = crate::ProviderRegistry::new(http.clone(), config)
            .with_inference_admission(InferenceAdmission::observe_run());
        let mut request = InferRequest::new(
            "deepseek/deepseek-v4-pro",
            vec![Message::text(Role::User, "hello")],
        );
        request.max_tokens = Some(64);
        let answered = match registry.resolve("deepseek/deepseek-v4-pro") {
            Ok(provider) => provider.infer(request).await.is_ok(),
            Err(_) => false,
        };
        assert!(!answered, "{endpoint}");
        let sent = http.0.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(sent, 0, "{endpoint}: a request crossed the transport");
    }
}
