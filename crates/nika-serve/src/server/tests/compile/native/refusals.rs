// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Native authoring's boundary (S09 B03–B23): typed refusals that reach no seat, replays that
//! repeat their round or refuse, provider failures as fixed reasons, the withheld value, the
//! call budget, the slot a stopped caller keeps paying for, the pinned snapshot, the bounded
//! store, and a seat refused before the listener binds.

use clap::Parser as _;
use nika_kernel::secret::Secret;

use super::*;
use crate::{NativeAuthoringArgs, NativeAuthoringError, seat_native_authoring};

/// Present in every refused body below; no answer may carry it back.
const SENTINEL: &str = "s06-private-sentinel";
/// The seat's own credential, as the operator withholds it.
const WITHHELD: &str = "sk-withheld-S06-0123456789abcdef";

/// The operator's seat every refusal below is judged under.
pub(super) fn operator(seat: &Seat) -> NativeAuthoring {
    NativeAuthoring::new(SEAT, seat.providers())
        .with_max_tokens(4096)
        .with_repairs(1)
}

fn edit(fields: &Value) -> String {
    let mut body = json!({
        "compile_version": 2,
        "mode": "edit",
        "cognition": "explicitProvider",
        "source": candidate(RUN_MODEL, false),
        "change": {"text": format!("also say {SENTINEL}")},
        "original_intent": INTENT,
    });
    merge(&mut body, fields);
    body.to_string()
}

fn nested(depth: usize) -> String {
    format!("{}1{}", "[".repeat(depth), "]".repeat(depth))
}

type Case = (String, u16, &'static str);

fn malformed(body: String) -> Case {
    (body, 422, "malformed_compile_request")
}

/// A fresh body spelled by hand, for the keys `json!` cannot repeat or null out.
fn raw(tail: &str) -> String {
    format!(
        r#"{{"compile_version":2,"mode":"create","cognition":"explicitProvider","intent":"{SENTINEL}"{tail}}}"#
    )
}

/// Structure: a missing or null member, a repeated key, a positional array, a wrong type.
fn shape_refusals() -> Vec<Case> {
    let s = SENTINEL;
    vec![
        malformed(fresh(&json!({"cognition": null}))),
        malformed(format!(
            r#"{{"compile_version":2,"mode":"create","cognition":null,"intent":"{s}"}}"#
        )),
        malformed(raw(r#","cognition":"explicitProvider""#)),
        malformed(format!(r#"[2,"create","explicitProvider","{s}"]"#)),
        malformed(raw(r#","answers":null"#)),
        malformed(raw(r#","limits":null"#)),
        malformed(raw(r#","answers":{"model":"a/b","model":"a/b"}"#)),
        malformed(raw(r#","limits":{"repairs":0,"repairs":0}"#)),
        malformed(fresh(&json!({"limits": {"model": s}}))),
        malformed(fresh(&json!({"limits": {"repairs": -1}}))),
        malformed(fresh(&json!({"limits": {"max_tokens": 1.5}}))),
        malformed(fresh(&json!({"limits": {"deadline_ms": "10"}}))),
    ]
}

/// Forms: a field of another form, a token where none belongs or misspelled, a literal
/// that repeats a key or nests past the parser's ceiling.
fn form_refusals() -> Vec<Case> {
    let s = SENTINEL;
    let token = "a".repeat(64);
    let repeated = |fields: &Value, from: &str, to: &str| fresh(fields).replace(from, to);
    vec![
        malformed(fresh(&json!({"original_intent": s}))),
        malformed(fresh(&json!({"replay_token": token}))),
        malformed(fresh(&json!({"cognition": "deterministicOnly"}))),
        malformed(replay(&token, &json!({"limits": {"repairs": 0}}))),
        malformed(replay(&token.to_uppercase(), &json!({}))),
        malformed(replay(&"a".repeat(63), &json!({}))),
        malformed(replay(&"g".repeat(64), &json!({}))),
        malformed(repeated(
            &json!({"answers": {"const.label": {"a": 1, "b": 2}}}),
            "\"b\"",
            "\"a\"",
        )),
        malformed(repeated(
            &json!({"answers": {"const.label": [{"x": {"y": 1, "z": s}}]}}),
            "\"z\"",
            "\"y\"",
        )),
        malformed(raw(&format!(
            r#","answers":{{"const.deep":{}}}"#,
            nested(200)
        ))),
        malformed(edit(&json!({"original_intent": null}))),
        malformed(edit(&json!({"workflow_id": "renamed"}))),
        malformed(edit(
            &json!({"change": {"set_constant": {"name": "x", "value": 1}}}),
        )),
    ]
}

/// Vocabulary, bounds and rounds, each with its own typed code.
fn typed_refusals() -> Vec<Case> {
    let long = format!("{SENTINEL}{}", "x".repeat(4096));
    let limit = |fields: &Value| (fresh(fields), 422, "compile_limit");
    let cognition = "compile_cognition_unsupported";
    vec![
        (
            fresh(&json!({"mode": "revise"})),
            422,
            "compile_mode_unsupported",
        ),
        (
            fresh(&json!({"cognition": "explicitDecision"})),
            422,
            cognition,
        ),
        (fresh(&json!({"cognition": "native"})), 422, cognition),
        (
            fresh(&json!({"compile_version": 3})),
            422,
            "compile_version_unsupported",
        ),
        limit(&json!({"limits": {"repairs": 2}})),
        limit(&json!({"limits": {"max_tokens": 0}})),
        limit(&json!({"limits": {"max_tokens": 4097}})),
        limit(&json!({"limits": {"call_timeout_ms": 0}})),
        limit(&json!({"limits": {"call_timeout_ms": 120_001}})),
        limit(&json!({"limits": {"deadline_ms": 300_001}})),
        limit(&json!({"intent": long})),
        (
            edit(&json!({"original_intent": long})),
            422,
            "compile_limit",
        ),
        (
            fresh(&json!({"answers": {"intent.clarification": SENTINEL}})),
            422,
            "compile_new_intent_required",
        ),
        (
            replay(&"a".repeat(64), &json!({})),
            409,
            "compile_replay_unavailable",
        ),
    ]
}

/// Caller-named authority is an unknown field, whatever the name: no host is opened.
fn authority_refusals() -> Vec<Case> {
    [
        "model",
        "provider",
        "base_url",
        "endpoint",
        "api_key",
        "access",
        "harness",
        "snapshot",
        "knowledge",
        "knowledge_pack",
        "path",
        "strategy",
        "plan",
        "receipt",
        "seat",
    ]
    .into_iter()
    .map(|field| malformed(fresh(&json!({ field: format!("/tmp/{SENTINEL}") }))))
    .collect()
}

/// Every generation-2 refusal this server answers, with its status and code.
fn refusals() -> Vec<Case> {
    let mut cases = shape_refusals();
    cases.extend(form_refusals());
    cases.extend(typed_refusals());
    cases.extend(authority_refusals());
    cases
}

#[tokio::test(flavor = "multi_thread")]
async fn generation_two_refusals_are_typed_echo_nothing_and_reach_no_seat() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![Reply::Text(native_answer(&candidate(
        RUN_MODEL, false,
    )))]);
    let (server, backend) = start_native(&world, compile_limits(), operator(&seat)).await;
    // Authentication first: the same body without the Bearer is a 401, never parsed.
    let body = fresh(&json!({"intent": SENTINEL}));
    let anonymous = format!(
        "POST /v1/compile HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let refused = server.request(&anonymous).await;
    assert_eq!(refused.status, 401, "{}", refused.body);
    assert!(!refused.body.contains(SENTINEL));
    for (body, status, code) in refusals() {
        let response = server.request(&compile_request(&body)).await;
        assert_eq!(response.status, status, "{body}: {}", response.body);
        assert_eq!(response.json()["error"]["code"], code, "{body}");
        assert!(!response.body.contains(SENTINEL), "{}", response.body);
        assert!(response.body.len() < 400, "bounded: {}", response.body);
        assert!(response.header("nika-compile-replay").is_none());
    }
    // Native permission where the core needs no seat: the zero-call answer, no receipt, no
    // token (S09 B12) — a skeleton, a structured constant, a literal null handed to the core.
    for body in [
        fresh(&json!({"intent": "hello", "limits": {"repairs": 0, "max_tokens": 1}})),
        fresh(&json!({"intent": "hello", "answers": {"const.x": null}})),
        edit(&json!({
            "change": {"set_constant": {"name": "missing", "value": 1}},
            "original_intent": null,
        })),
    ] {
        let response = server.request(&compile_request(&body)).await;
        assert_eq!(response.status, 200, "{body}: {}", response.body);
        assert_eq!(response.json()["compile_version"], 1, "{}", response.body);
        assert!(response.header("nika-compile-replay").is_none());
    }
    assert_eq!(
        seat.calls(),
        0,
        "no refused or zero-call request reached the seat"
    );
    assert_eq!(backend.calls(), 0);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_replay_repeats_its_round_or_refuses_and_never_buys_a_call() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![Reply::Text(native_answer(&candidate(
        "mock/echo",
        false,
    )))]);
    let (server, _backend) = start_native(&world, compile_limits(), operator(&seat)).await;
    let first = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(first.status, 200, "{}", first.body);
    let token = token_of(&first);
    for (fields, code) in [
        (
            json!({"intent": format!("{INTENT} today")}),
            "compile_replay_input_changed",
        ),
        (
            json!({"workflow_id": "renamed"}),
            "compile_replay_input_changed",
        ),
        (
            json!({"answers": {"intent.clarification": "Write ./b.md only"}}),
            "compile_replay_input_changed",
        ),
    ] {
        let response = server
            .request(&compile_request(&replay(&token, &fields)))
            .await;
        assert_eq!(response.status, 409, "{}", response.body);
        assert_eq!(response.json()["error"]["code"], code);
        assert!(!response.body.contains(&token));
    }
    let other_mode = json!({
        "compile_version": 2, "mode": "edit", "cognition": "deterministicOnly",
        "replay_token": token, "source": candidate(RUN_MODEL, false),
        "change": {"text": "also keep a copy"}, "original_intent": INTENT,
    });
    let response = server
        .request(&compile_request(&other_mode.to_string()))
        .await;
    assert_eq!(
        response.json()["error"]["code"],
        "compile_replay_input_changed"
    );

    // Another server run keeps none of this run's rounds: the token is unknown there.
    let elsewhere = TestWorld::new();
    let other_seat = Seat::start(vec![Reply::Text(String::new())]);
    let (other, _other_backend) =
        start_native(&elsewhere, compile_limits(), operator(&other_seat)).await;
    let foreign = other
        .request(&compile_request(&replay(&token, &json!({}))))
        .await;
    assert_eq!(foreign.status, 409, "{}", foreign.body);
    assert_eq!(
        foreign.json()["error"]["code"],
        "compile_replay_unavailable"
    );
    assert_eq!(other_seat.calls(), 0);
    other.stop().await.expect("clean stop");

    // A kept round expires on its own clock; use never renews it.
    let brief = TestWorld::new();
    let brief_seat = Seat::start(vec![Reply::Text(native_answer(&candidate(
        "mock/echo",
        false,
    )))]);
    let (short, _short_backend) = start_native(
        &brief,
        compile_limits(),
        operator(&brief_seat).with_replay(4, Duration::from_millis(400)),
    )
    .await;
    let kept = short.request(&compile_request(&fresh(&json!({})))).await;
    let brief_token = token_of(&kept);
    let answers = json!({"answers": {"model": RUN_MODEL}});
    let used = short
        .request(&compile_request(&replay(&brief_token, &answers)))
        .await;
    assert_eq!(used.status, 200, "{}", used.body);
    tokio::time::sleep(Duration::from_millis(600)).await;
    let expired = short
        .request(&compile_request(&replay(&brief_token, &answers)))
        .await;
    assert_eq!(expired.status, 409, "{}", expired.body);
    assert_eq!(
        expired.json()["error"]["code"],
        "compile_replay_unavailable"
    );
    assert_eq!(
        brief_seat.calls(),
        1,
        "an expired round is refused, never regenerated"
    );
    assert_eq!(seat.calls(), 1);
    short.stop().await.expect("clean stop");
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn provider_failures_reach_the_document_as_fixed_reasons_and_a_withheld_value_is_refused_whole()
 {
    let world = TestWorld::new();
    let leaky = format!(
        r#"{{"error":{{"message":"bad key sk-live-{SENTINEL} for http://internal.example/v1 at /etc/{SENTINEL}"}}}}"#
    );
    let seat = Seat::start(vec![Reply::Status(401, leaky)]);
    let (server, _backend) =
        start_native(&world, compile_limits(), operator(&seat).with_repairs(0)).await;
    let response = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(response.status, 200, "{}", response.body);
    for needle in [SENTINEL, "internal.example", "127.0.0.1", "/etc/"] {
        assert!(
            !response.body.contains(needle),
            "{needle}: {}",
            response.body
        );
    }
    let document = response.json();
    assert_eq!(document["status"], "incomplete");
    assert_eq!(document["provenance"]["authoring"]["calls"], 1);
    let failure = document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .find(|d| d["target"] == "authoring_provider")
        .expect("the provider failure is stated")["message"]
        .as_str()
        .expect("message")
        .to_owned();
    assert!(
        failure.starts_with("provider error: the authoring provider")
            || failure.starts_with("provider error: the connection to the authoring provider"),
        "a fixed reason: {failure}"
    );
    assert_eq!(seat.calls(), 1, "no retry");
    server.stop().await.expect("clean stop");

    // A seat that echoes the operator's credential: the whole answer is refused.
    let world = TestWorld::new();
    let echoed = candidate(RUN_MODEL, false).replace(
        "inventing nothing",
        &format!("inventing nothing, signed {WITHHELD}"),
    );
    let seat = Seat::start(vec![Reply::Text(native_answer(&echoed))]);
    let authoring = operator(&seat)
        .with_repairs(0)
        .with_withheld(Secret::new(WITHHELD));
    let (server, _backend) = start_native(&world, compile_limits(), authoring).await;
    let response = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(response.status, 500, "{}", response.body);
    assert_eq!(
        response.json()["error"]["code"],
        "compile_disclosure_refused"
    );
    assert!(!response.body.contains(WITHHELD));
    assert!(
        response.header("nika-compile-replay").is_none(),
        "nothing kept"
    );
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_repair_rounds_are_the_call_budget_and_a_caller_can_only_narrow_them() {
    let broken = native_answer("nika: broken\ntasks: {}\n");
    let fixed = native_answer(&candidate(RUN_MODEL, false));
    // The operator's one repair: the refused candidate, then the repaired one — two calls.
    let world = TestWorld::new();
    let seat = Seat::start(vec![
        Reply::Text(broken.clone()),
        Reply::Text(fixed.clone()),
    ]);
    let (server, _backend) = start_native(&world, compile_limits(), operator(&seat)).await;
    let response = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(response.status, 200, "{}", response.body);
    let document = response.json();
    assert_eq!(
        document["provenance"]["authoring"]["calls"], 2,
        "{document:#}"
    );
    assert_eq!(document["status"], "ready", "{document:#}");
    assert_eq!(seat.calls(), 2);
    server.stop().await.expect("clean stop");
    // The caller narrows to zero repairs: one call, the refused candidate stays refused.
    let world = TestWorld::new();
    let seat = Seat::start(vec![Reply::Text(broken), Reply::Text(fixed)]);
    let (server, _backend) = start_native(&world, compile_limits(), operator(&seat)).await;
    let response = server
        .request(&compile_request(&fresh(
            &json!({"limits": {"repairs": 0, "max_tokens": 1024}}),
        )))
        .await;
    assert_eq!(response.status, 200, "{}", response.body);
    let document = response.json();
    assert_eq!(document["provenance"]["authoring"]["calls"], 1);
    assert_ne!(document["status"], "ready");
    assert_eq!(seat.calls(), 1);
    assert_eq!(seat.bodies()[0]["max_tokens"].as_u64(), Some(1024));
    server.stop().await.expect("clean stop");
}

pub(super) fn one_slot() -> ServerLimits {
    ServerLimits::new(
        2 * 1024 * 1024,
        Duration::from_secs(20),
        Duration::from_secs(5),
        Duration::from_millis(500),
        4,
        16,
        64,
        32,
    )
    .with_max_compile_requests(1)
}

pub(super) async fn admitted_again(server: &TestServer) -> bool {
    let hello = r#"{"compile_version":1,"mode":"create","intent":"hello"}"#;
    for _ in 0..400 {
        let response = server.request(&compile_request(hello)).await;
        if response.status == 200 {
            return true;
        }
        assert_eq!(response.status, 503, "{}", response.body);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    false
}

pub(super) fn wait_entered(seat: &Seat) {
    seat.entered
        .recv_timeout(Duration::from_secs(20))
        .expect("the seat received the call");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_disconnected_or_expired_native_round_holds_its_slot_until_its_work_stops() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![Reply::Parked(native_answer(&candidate(
        "mock/echo",
        false,
    )))]);
    let (server, _backend) = start_native(&world, one_slot(), operator(&seat)).await;
    let mut caller = tokio::net::TcpStream::connect(server.address)
        .await
        .expect("connect");
    caller
        .write_all(compile_request(&fresh(&json!({}))).as_bytes())
        .await
        .expect("request");
    tokio::task::block_in_place(|| wait_entered(&seat));
    drop(caller);
    // The caller is gone, the paid work is not: the one slot is still taken.
    for body in [
        fresh(&json!({})),
        r#"{"compile_version":1,"mode":"create","intent":"hello"}"#.to_owned(),
    ] {
        let busy = server.request(&compile_request(&body)).await;
        assert_eq!(busy.status, 503, "{}", busy.body);
        assert_eq!(busy.json()["error"]["code"], "compile_busy");
    }
    assert_eq!(
        server.request(&get_request("/v1/workflows")).await.status,
        200
    );
    seat.release.send(()).expect("the parked call is alive");
    assert!(
        admitted_again(&server).await,
        "the slot returns when the work ends"
    );
    assert_eq!(seat.calls(), 1, "no later repair and no retry");
    server.stop().await.expect("clean stop");

    // The operator's deadline stops the work: 408, and the slot returns with it.
    let world = TestWorld::new();
    let seat = Seat::start(vec![Reply::Parked(native_answer(&candidate(
        "mock/echo",
        false,
    )))]);
    let authoring = operator(&seat).with_deadline(Duration::from_millis(400));
    let (server, _backend) = start_native(&world, one_slot(), authoring).await;
    let started = std::time::Instant::now();
    let expired = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(expired.status, 408, "{}", expired.body);
    assert_eq!(expired.json()["error"]["code"], "compile_deadline_exceeded");
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(expired.header("nika-compile-replay").is_none());
    assert!(
        admitted_again(&server).await,
        "the stopped work freed its slot"
    );
    seat.release.send(()).expect("release the parked seat");
    assert_eq!(seat.calls(), 1);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_native_round_outlives_the_request_deadline_that_still_bounds_generation_one() {
    let limits = ServerLimits::new(
        2 * 1024 * 1024,
        Duration::from_millis(300),
        Duration::from_secs(5),
        Duration::from_millis(500),
        4,
        16,
        64,
        32,
    )
    .with_max_compile_requests(1);
    let (entered, entered_signal) = oneshot::channel::<()>();
    let (release, parked_until) = std::sync::mpsc::channel::<()>();
    let park: super::super::super::super::test_support::CaptureAction = Box::new(move || {
        let _sent = entered.send(());
        let _resumed = parked_until.recv();
    });
    let world = TestWorld::new();
    let seat = Seat::start(vec![Reply::Parked(native_answer(&candidate(
        "mock/echo",
        false,
    )))]);
    let (server, _backend) = start_native_parked(&world, limits, operator(&seat), Some(park)).await;
    // Generation 1 on a native server: the request deadline as before, the slot kept.
    let hello = r#"{"compile_version":1,"mode":"create","intent":"hello"}"#;
    let timed_out = server.request(&compile_request(hello)).await;
    assert_eq!(timed_out.status, 408, "{}", timed_out.body);
    assert_eq!(timed_out.json()["error"]["code"], "request_timeout");
    entered_signal
        .await
        .expect("the parked section was entered");
    let busy = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(busy.status, 503, "{}", busy.body);
    release.send(()).expect("the parked section is alive");
    assert!(admitted_again(&server).await);
    // A native round answers after a wait far beyond that deadline.
    let address = server.address;
    let pending =
        tokio::spawn(
            async move { wire_request(address, &compile_request(&fresh(&json!({})))).await },
        );
    tokio::task::block_in_place(|| wait_entered(&seat));
    tokio::time::sleep(Duration::from_millis(900)).await;
    seat.release.send(()).expect("the parked call is alive");
    let answered = pending.await.expect("the round answered");
    assert_eq!(answered.status, 200, "{}", answered.body);
    assert_eq!(answered.json()["compile_version"], 2);
    assert_eq!(seat.calls(), 1);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_snapshot_changed_after_start_is_refused_before_the_seat() {
    let world = TestWorld::new();
    let foundry = Foundry::create(&world.root.path().join("knowledge"));
    let seat = Seat::start(vec![Reply::Text(native_answer(&candidate(
        "mock/echo",
        false,
    )))]);
    let authoring = operator(&seat).with_knowledge(&foundry.snapshot, None);
    let (server, _backend) = start_native(&world, compile_limits(), authoring).await;
    let first = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(first.status, 200, "{}", first.body);
    let token = token_of(&first);
    // A presented file edited after the export: no pack is composed under the pinned identity.
    std::fs::write(
        foundry.root.join("blocks/s06-transform.nika"),
        "# edited after the export\n",
    )
    .expect("edit");
    let stale = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(stale.status, 409, "{}", stale.body);
    assert_eq!(stale.json()["error"]["code"], "compile_context_changed");
    assert!(
        !stale
            .body
            .contains(&world.root.path().display().to_string())
    );
    // A replay presents nothing, so the kept round still answers ...
    let answers = json!({"answers": {"model": RUN_MODEL}});
    let replayed = server
        .request(&compile_request(&replay(&token, &answers)))
        .await;
    assert_eq!(replayed.status, 200, "{}", replayed.body);
    // ... until the snapshot itself is exported again: the pin no longer holds.
    let manifest = foundry.snapshot.join("manifest.json");
    let text = std::fs::read_to_string(&manifest).expect("manifest");
    std::fs::write(&manifest, text.replace("digest-s06-a", "digest-s06-b")).expect("re-export");
    let moved = server
        .request(&compile_request(&replay(&token, &answers)))
        .await;
    assert_eq!(moved.status, 409, "{}", moved.body);
    assert_eq!(moved.json()["error"]["code"], "compile_context_changed");
    assert_eq!(seat.calls(), 1, "a changed context never reaches the seat");
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_full_store_refuses_a_fresh_round_before_it_spends() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![Reply::Text(native_answer(&candidate(
        "mock/echo",
        false,
    )))]);
    let authoring = operator(&seat).with_replay(1, Duration::from_secs(60));
    let (server, _backend) = start_native(&world, compile_limits(), authoring).await;
    let first = server.request(&compile_request(&fresh(&json!({})))).await;
    let _token = token_of(&first);
    let full = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(full.status, 503, "{}", full.body);
    assert_eq!(full.json()["error"]["code"], "compile_replay_capacity");
    assert_eq!(seat.calls(), 1, "refused before any call");
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_invalid_seat_refuses_the_listener_before_it_binds() {
    let world = TestWorld::new();
    let providers = ProvidersConfig::new;
    let not_a_snapshot = world.root.path().join("empty");
    std::fs::create_dir_all(&not_a_snapshot).expect("empty dir");
    let cases: Vec<(NativeAuthoring, &str)> = vec![
        (
            NativeAuthoring::new("claude-code/default", providers()),
            "model",
        ),
        (NativeAuthoring::new("no-slash", providers()), "model"),
        (NativeAuthoring::new("unknownprov/x", providers()), "model"),
        (NativeAuthoring::new("openai/gpt-x", providers()), "model"),
        (
            NativeAuthoring::new(SEAT, providers()).with_max_tokens(0),
            "bound",
        ),
        (
            NativeAuthoring::new(SEAT, providers()).with_max_tokens(32_769),
            "bound",
        ),
        (
            NativeAuthoring::new(SEAT, providers()).with_call_timeout(Duration::ZERO),
            "bound",
        ),
        (
            NativeAuthoring::new(SEAT, providers()).with_deadline(Duration::from_secs(3601)),
            "bound",
        ),
        (
            NativeAuthoring::new(SEAT, providers()).with_repairs(6),
            "bound",
        ),
        (
            NativeAuthoring::new(SEAT, providers()).with_replay(0, Duration::from_secs(1)),
            "bound",
        ),
        (
            NativeAuthoring::new(SEAT, providers()).with_knowledge(&not_a_snapshot, None),
            "knowledge",
        ),
    ];
    for (authoring, kind) in cases {
        let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
        let resident = ResidentConfig::new(&world.state).with_limits(compile_limits());
        let authority = ResidentAuthority::open(resident, backend)
            .await
            .expect("authority");
        let described = format!("{authoring:?}");
        let config = ServerConfig::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            &world.workflows,
            &world.token,
        )
        .with_native_authoring(authoring);
        let matched = match BoundServer::attach(config, &authority).await {
            Err(ServerError::NativeAuthoring(NativeAuthoringError::Model { .. })) => "model",
            Err(ServerError::NativeAuthoring(NativeAuthoringError::Bound(_))) => "bound",
            Err(ServerError::NativeAuthoring(NativeAuthoringError::Knowledge(_))) => "knowledge",
            Err(_) => "another refusal",
            Ok(_) => "a bound listener",
        };
        assert_eq!(matched, kind, "{described}");
        drop(authority);
    }
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

#[test]
fn the_flags_seat_only_what_the_operator_names() {
    assert!(
        door(&["--authoring-model", SEAT]).is_err(),
        "a seat needs the listener"
    );
    assert!(door(&["--bind", "127.0.0.1:0", "--authoring-timeout", "5"]).is_err());
    assert!(
        door(&[
            "--bind",
            "x",
            "--authoring-model",
            SEAT,
            "--knowledge-exclude",
            "c"
        ])
        .is_err(),
        "an exclusion needs its snapshot"
    );
    let named = door(&[
        "--bind",
        "127.0.0.1:0",
        "--authoring-model",
        SEAT,
        "--authoring-max-tokens",
        "1024",
        "--authoring-timeout",
        "30",
        "--authoring-deadline",
        "90",
        "--authoring-repairs",
        "0",
    ])
    .expect("parses");
    assert_eq!(named.model.as_deref(), Some(SEAT));
    let config = || {
        ServerConfig::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            "/workflows",
            "/token",
        )
    };
    let unnamed = door(&[]).expect("parses");
    assert!(
        seat_native_authoring(None, &unnamed)
            .expect("untouched")
            .is_none()
    );
    let untouched = seat_native_authoring(Some(config()), &unnamed)
        .expect("untouched")
        .expect("config");
    assert!(format!("{untouched:?}").contains("native_authoring: None"));
    assert_eq!(
        seat_native_authoring(None, &named).err(),
        Some(NativeAuthoringError::NeedsListener)
    );
    let seated = seat_native_authoring(Some(config()), &named)
        .expect("seated")
        .expect("config");
    let described = format!("{seated:?}");
    for part in [
        "model: \"vllm/s06-seat\"",
        "max_tokens: 1024",
        "call_timeout: 30s",
        "deadline: 90s",
        "repairs: 0",
    ] {
        assert!(described.contains(part), "{described}");
    }
}
