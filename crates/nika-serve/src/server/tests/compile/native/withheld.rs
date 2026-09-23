// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The seat's own credential is never answered (S19 · S18 P1-3): the key the resolved provider
//! actually sends is withheld however the operator supplied it — a typed `ProvidersConfig`
//! included, no second naming, its own precedence respected — and every nonempty withheld value
//! counts, short or escaped. Every sentinel below is synthetic.

use nika_kernel::secret::Secret;

use super::refusals::operator;
use super::*;

/// A candidate for [`INTENT`] whose infer prompt carries `text`.
fn echoing(text: &str) -> String {
    native_answer(&candidate(RUN_MODEL, false).replace(
        "inventing nothing",
        &format!("inventing nothing, signed {text}"),
    ))
}

async fn answered_with(authoring: NativeAuthoring) -> WireResponse {
    let world = TestWorld::new();
    let (server, _backend) = start_native(&world, compile_limits(), authoring).await;
    let response = server.request(&compile_request(&fresh(&json!({})))).await;
    server.stop().await.expect("clean stop");
    response
}

fn assert_refused_whole(response: &WireResponse, sentinel: &str) {
    assert_eq!(response.status, 500, "{}", response.body);
    assert_eq!(
        response.json()["error"]["code"],
        "compile_disclosure_refused"
    );
    assert!(!response.body.contains(sentinel), "{}", response.body);
    assert!(
        response.header("nika-compile-replay").is_none(),
        "nothing kept"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_key_a_typed_config_seats_is_withheld_without_being_named_again() {
    const KEY: &str = "synthetic-S19-typed-key-123456";
    let seat = Seat::start(vec![Reply::Text(echoing(KEY))]);
    let providers = seat.providers().with_key("vllm", Secret::new(KEY));
    let authoring = NativeAuthoring::new(SEAT, providers).with_repairs(0);
    let response = answered_with(authoring).await;
    assert_refused_whole(&response, KEY);
    assert_eq!(seat.calls(), 1, "the call happened; its answer is refused");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_short_withheld_value_is_withheld() {
    const SHORT: &str = "k9#";
    let seat = Seat::start(vec![Reply::Text(echoing(SHORT))]);
    let authoring = operator(&seat)
        .with_repairs(0)
        .with_withheld(Secret::new(SHORT));
    let response = answered_with(authoring).await;
    assert_refused_whole(&response, SHORT);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_typed_key_the_document_only_carries_escaped_is_withheld() {
    // A quote and a backslash: the document carries them JSON-escaped, never raw.
    const KEY: &str = r#"synthetic"S19\key-escape"#;
    let seat = Seat::start(vec![Reply::Text(echoing(KEY))]);
    let providers = seat.providers().with_key("vllm", Secret::new(KEY));
    let authoring = NativeAuthoring::new(SEAT, providers).with_repairs(0);
    let response = answered_with(authoring).await;
    assert_refused_whole(&response, r"S19\\key-escape");
    assert!(!response.body.contains("S19"), "{}", response.body);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_withheld_key_is_the_one_the_seat_actually_sends() {
    const STALE: &str = "synthetic-S19-stale-key-000000";
    const LIVE: &str = "synthetic-S19-live-key-111111";
    let seat = Seat::start(vec![Reply::Text(echoing(LIVE))]);
    // The configuration's own precedence: the later key for the canonical id is the one sent.
    let providers = seat
        .providers()
        .with_key("vllm", Secret::new(STALE))
        .with_key("vllm", Secret::new(LIVE));
    let authoring = NativeAuthoring::new(SEAT, providers).with_repairs(0);
    let response = answered_with(authoring).await;
    assert_eq!(
        seat.authorizations(),
        vec![format!("Bearer {LIVE}")],
        "the key sent"
    );
    assert_refused_whole(&response, LIVE);
}
