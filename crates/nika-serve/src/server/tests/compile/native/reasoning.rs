// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The operator's explicit reasoning effort on the compile door (R4 B16): the seat's word rides
//! every call of a round and reaches its receipt, an unknown word refuses before the listener
//! binds, and a route the catalog does not qualify (here the operator's loopback override of the
//! `DeepSeek` endpoint) sends nothing. The bodies are the ones the loopback seat received: a seam
//! capture of the dispatched bytes, never a capture of the provider's network.

use clap::Parser as _;
use nika_kernel::secret::Secret;

use super::*;
use crate::{NativeAuthoringArgs, NativeAuthoringError};

/// The one model the catalog lists effort levels for, qualified on its direct route only.
const PRO: &str = "deepseek/deepseek-v4-pro";

/// The `DeepSeek` seat at the loopback: an operator's base-URL override, never the direct route.
fn deepseek(seat: &Seat) -> ProvidersConfig {
    ProvidersConfig::new()
        .with_key("deepseek", Secret::new("fixture"))
        .with_base_url(
            "deepseek",
            format!("http://127.0.0.1:{}/v1/chat/completions", seat.port),
        )
}

/// The first call's entry in a compile document's authoring receipt.
fn first_call(document: &Value) -> Value {
    document["provenance"]["authoring"]["context"][0].clone()
}

/// The control reaches the seat with the route's own bytes and the request; each named level is
/// recorded as configured and refused before a byte leaves, the seat still at one body.
#[tokio::test(flavor = "multi_thread")]
async fn a_named_level_rides_the_round_and_an_unqualified_route_sends_nothing() {
    let world = TestWorld::new();
    let seat = Seat::start(question_round());
    let control = NativeAuthoring::new(PRO, deepseek(&seat)).with_repairs(0);
    let (server, _) = start_native(&world, compile_limits(), control).await;
    let response = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(response.status, 200, "{}", response.body);
    let document = response.json();
    let bodies = seat.bodies();
    assert_eq!(bodies.len(), 1, "{document:#}");
    assert_eq!(bodies[0]["model"], "deepseek-v4-pro");
    assert_eq!(bodies[0]["max_tokens"].as_u64(), Some(16_384));
    assert!(
        bodies[0].get("reasoning_effort").is_none(),
        "the default capacity does not select the short-answer effort: {}",
        bodies[0]
    );
    assert!(bodies[0].get("thinking").is_none(), "{}", bodies[0]);
    assert!(
        message(&bodies[0], "user").contains(INTENT),
        "{}",
        bodies[0]
    );
    let call = first_call(&document);
    assert_eq!(call["reasoning"]["configured"], Value::Null, "{call:#}");
    assert_eq!(
        call["reasoning"]["transmitted"],
        json!({"thinking": null, "effort": null}),
        "{call:#}"
    );
    assert_eq!(call["reasoning"]["served"], "unknown", "{call:#}");
    server.stop().await.expect("clean stop");

    for word in ["low", "high", "max"] {
        let named = NativeAuthoring::new(PRO, deepseek(&seat))
            .with_repairs(0)
            .with_reasoning(word);
        let (server, _) = start_native(&world, compile_limits(), named).await;
        let response = server.request(&compile_request(&fresh(&json!({})))).await;
        assert_eq!(response.status, 200, "{word}: {}", response.body);
        let document = response.json();
        assert_eq!(seat.calls(), 1, "{word}: nothing more reached the seat");
        assert_ne!(document["status"], "ready", "{document:#}");
        let call = first_call(&document);
        assert_eq!(call["reasoning"]["configured"], word, "{call:#}");
        assert_eq!(call["reasoning"]["transmitted"], "unobserved", "{call:#}");
        assert_eq!(
            call["result"]["failure_kind"], "admission_refused",
            "{call:#}"
        );
        // The refusal names its own cause, never the request grant's remedy (R4 B17).
        let said = document["diagnostics"].to_string();
        let cause = format!("the explicit reasoning effort `{word}` is not qualified");
        assert!(said.contains(&cause), "{word}: {said}");
        assert!(!said.contains("authorize sufficient max_calls"), "{said}");
        server.stop().await.expect("clean stop");
    }
}

/// An explicit short-answer cap still selects the route's existing low-effort behavior;
/// the receipt reports those dispatched bytes without claiming the served effort is known.
#[tokio::test(flavor = "multi_thread")]
async fn an_explicit_short_answer_keeps_the_route_effort_and_its_wire_receipt() {
    let world = TestWorld::new();
    let seat = Seat::start(question_round());
    let control = NativeAuthoring::new(PRO, deepseek(&seat))
        .with_repairs(0)
        .with_max_tokens(8192);
    let (server, _) = start_native(&world, compile_limits(), control).await;
    let response = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(response.status, 200, "{}", response.body);
    let document = response.json();
    let bodies = seat.bodies();
    assert_eq!(bodies.len(), 1, "{document:#}");
    assert_eq!(bodies[0]["model"], "deepseek-v4-pro");
    assert_eq!(bodies[0]["max_tokens"], 8192);
    assert_eq!(bodies[0]["reasoning_effort"], "low");
    assert!(bodies[0].get("thinking").is_none(), "{}", bodies[0]);
    assert!(message(&bodies[0], "user").contains(INTENT));
    let call = first_call(&document);
    assert_eq!(call["reasoning"]["configured"], Value::Null, "{call:#}");
    assert_eq!(
        call["reasoning"]["transmitted"],
        json!({"thinking": null, "effort": "low"}),
        "{call:#}"
    );
    assert_eq!(call["reasoning"]["served"], "unknown", "{call:#}");
    server.stop().await.expect("clean stop");
}

/// A word outside low · high · max (case and spelling exact) refuses the seat before the listener
/// binds, naming the word; no request reaches the seat.
#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_effort_word_refuses_the_listener_before_it_binds() {
    let world = TestWorld::new();
    let seat = Seat::start(Vec::new());
    for word in ["medium", "MAX", "maximum", "x-high", ""] {
        let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
        let resident = ResidentConfig::new(&world.state).with_limits(compile_limits());
        let authority = ResidentAuthority::open(resident, backend)
            .await
            .expect("authority");
        let config = ServerConfig::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            &world.workflows,
            &world.token,
        )
        .with_native_authoring(NativeAuthoring::new(PRO, deepseek(&seat)).with_reasoning(word));
        let refused = match BoundServer::attach(config, &authority).await {
            Err(ServerError::NativeAuthoring(NativeAuthoringError::Model { reason, .. })) => reason,
            Err(other) => format!("another refusal: {other}"),
            Ok(_) => "a bound listener".to_owned(),
        };
        let named = format!("`{word}` is not a reasoning effort");
        assert!(refused.contains(&named), "{word}: {refused}");
        drop(authority);
    }
    assert_eq!(seat.calls(), 0);
}

#[derive(clap::Parser)]
struct Door {
    #[arg(long)]
    bind: Option<String>,
    #[command(flatten)]
    authoring: NativeAuthoringArgs,
}

fn door(argv: &[&str]) -> Result<NativeAuthoringArgs, clap::Error> {
    Door::try_parse_from(std::iter::once("serve").chain(argv.iter().copied()))
        .map(|door| door.authoring)
}

/// The effort flag names a word only beside the seat it is asked of; its word is carried as
/// typed, judged when the listener attaches.
#[test]
fn the_effort_flag_names_a_word_only_beside_a_seat() {
    let bind = ["--bind", "127.0.0.1:0"];
    assert!(
        door(&[bind[0], bind[1], "--authoring-reasoning", "max"]).is_err(),
        "a word needs a seat"
    );
    let named = door(&[
        bind[0],
        bind[1],
        "--authoring-model",
        PRO,
        "--authoring-reasoning",
        "max",
    ])
    .expect("parses");
    assert_eq!(named.reasoning.as_deref(), Some("max"));
    let unnamed = door(&[bind[0], bind[1], "--authoring-model", PRO]).expect("parses");
    assert_eq!(unnamed.reasoning, None);
}
