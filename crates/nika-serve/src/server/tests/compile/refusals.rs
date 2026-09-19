// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The compile door's protocol edge: typed refusals that echo nothing, exact bounds,
//! hostile input as bounded data, and the slot a cancelled request keeps paying for.
use std::fmt::Write as _;
use std::sync::mpsc;

use super::super::super::compile as door;
use super::super::super::test_support::CaptureAction;
use super::*;

const LITERAL_BASE: &str = "nika: literal-edit\nconst: {payload: 0}\npermits: {tools: ['nika:jq']}\ntasks:\n  echo:\n    invoke:\n      tool: nika:jq\n      args: {input: '${{ const.payload }}', expression: '.'}\n";
/// Present in every refused body below; no refusal may carry it back.
const SENTINEL: &str = "private-sentinel-7f3a";

async fn refusal(server: &TestServer, body: &str) -> (u16, String) {
    let response = server.request(&compile_request(body)).await;
    let error = response.json()["error"].clone();
    let object = error.as_object().expect("error envelope");
    assert_eq!(
        object.len(),
        2,
        "exactly code and message: {}",
        response.body
    );
    assert!(
        response.body.len() < 320,
        "bounded refusal: {}",
        response.body
    );
    assert!(!response.body.contains(SENTINEL), "{}", response.body);
    (
        response.status,
        error["code"].as_str().expect("code").to_owned(),
    )
}

fn edit_body(source: &str, literal: &str) -> String {
    format!(
        r#"{{"compile_version":1,"mode":"edit","source":{},"change":{{"text":"Set const.payload"}},"answers":{{"const.payload":{literal}}}}}"#,
        serde_json::to_string(source).expect("encoded source")
    )
}

fn envelope_refusals() -> Vec<(String, &'static str)> {
    let s = SENTINEL;
    vec![
        // Not the generation-1 envelope at all.
        (format!("{{ {s} not json"), "malformed_compile_request"),
        (format!(r#"["{s}"]"#), "malformed_compile_request"),
        (format!(r#""{s}""#), "malformed_compile_request"),
        (
            format!(r#"{{"mode":"create","intent":"{s}"}}"#),
            "malformed_compile_request",
        ),
        (
            format!(r#"{{"compile_version":"1","mode":"create","intent":"{s}"}}"#),
            "malformed_compile_request",
        ),
        (
            format!(r#"{{"compile_version":1.5,"mode":"create","intent":"{s}"}}"#),
            "malformed_compile_request",
        ),
        // A positional array is not a spelling of any object in this contract.
        (
            format!(r#"[1,"create","{s}"]"#),
            "malformed_compile_request",
        ),
        (
            format!(r#"[2,"create","{s}"]"#),
            "malformed_compile_request",
        ),
        (
            format!(
                r#"{{"compile_version":1,"mode":"edit","source":"{s}","change":["Set const.payload to 1"]}}"#
            ),
            "malformed_compile_request",
        ),
        (
            format!(
                r#"{{"compile_version":1,"mode":"edit","source":"{s}","change":{{"set_constant":["payload",1]}}}}"#
            ),
            "malformed_compile_request",
        ),
        // Unknown fields, including the path a host-reading door would have needed.
        (
            format!(
                r#"{{"compile_version":1,"mode":"create","intent":"hello","base":"/etc/{s}"}}"#
            ),
            "malformed_compile_request",
        ),
        (
            format!(
                r#"{{"compile_version":1,"mode":"create","intent":"hello","dest":"{s}.nika"}}"#
            ),
            "malformed_compile_request",
        ),
        (
            format!(
                r#"{{"compile_version":1,"mode":"create","intent":"hello","force":true,"x":"{s}"}}"#
            ),
            "malformed_compile_request",
        ),
        // A present null never means absent.
        (
            format!(r#"{{"compile_version":1,"mode":"create","intent":"{s}","answers":null}}"#),
            "malformed_compile_request",
        ),
        (
            format!(r#"{{"compile_version":1,"mode":"create","intent":"{s}","workflow_id":null}}"#),
            "malformed_compile_request",
        ),
        (
            format!(r#"{{"compile_version":1,"mode":"create","intent":null,"x":"{s}"}}"#),
            "malformed_compile_request",
        ),
    ]
}

fn typed_field_refusals() -> Vec<(String, &'static str)> {
    let s = SENTINEL;
    vec![
        // Duplicate keys select nothing, at the envelope and inside answers.
        (
            format!(r#"{{"compile_version":1,"mode":"create","intent":"hello","intent":"{s}"}}"#),
            "malformed_compile_request",
        ),
        (
            format!(
                r#"{{"compile_version":1,"compile_version":1,"mode":"create","intent":"{s}"}}"#
            ),
            "malformed_compile_request",
        ),
        (
            format!(
                r#"{{"compile_version":1,"mode":"create","intent":"classify-and-route","answers":{{"const.request":"{s}","const.request":"other"}}}}"#
            ),
            "malformed_compile_request",
        ),
        (
            format!(
                r#"{{"compile_version":1,"mode":"edit","source":"{s}","change":{{"text":"x","text":"x"}}}}"#
            ),
            "malformed_compile_request",
        ),
        (
            format!(
                r#"{{"compile_version":1,"mode":"edit","source":"{s}","change":{{"set_constant":{{"name":"x","value":1,"value":2}}}}}}"#
            ),
            "malformed_compile_request",
        ),
        (
            format!(
                r#"{{"compile_version":1,"mode":"edit","source":"{s}","change":{{"set_constant":{{"name":"x","value":1,"extra":true}}}}}}"#
            ),
            "malformed_compile_request",
        ),
        // Wrong types.
        (
            format!(r#"{{"compile_version":1,"mode":"create","intent":["{s}"]}}"#),
            "malformed_compile_request",
        ),
        (
            format!(
                r#"{{"compile_version":1,"mode":"create","intent":"hello","answers":["{s}"]}}"#
            ),
            "malformed_compile_request",
        ),
        (
            format!(r#"{{"compile_version":1,"mode":7,"intent":"{s}"}}"#),
            "malformed_compile_request",
        ),
    ]
}

fn operation_refusals() -> Vec<(String, &'static str)> {
    let s = SENTINEL;
    vec![
        // A field of the other mode, or a missing required one.
        (
            format!(r#"{{"compile_version":1,"mode":"create","intent":"hello","source":"{s}"}}"#),
            "malformed_compile_request",
        ),
        (
            format!(
                r#"{{"compile_version":1,"mode":"create","intent":"hello","change":{{"text":"{s}"}}}}"#
            ),
            "malformed_compile_request",
        ),
        (
            format!(r#"{{"compile_version":1,"mode":"create","workflow_id":"{s}"}}"#),
            "malformed_compile_request",
        ),
        (
            format!(r#"{{"compile_version":1,"mode":"edit","source":"{s}"}}"#),
            "malformed_compile_request",
        ),
        (
            format!(r#"{{"compile_version":1,"mode":"edit","change":{{"text":"{s}"}}}}"#),
            "malformed_compile_request",
        ),
        (
            format!(
                r#"{{"compile_version":1,"mode":"edit","intent":"{s}","source":"x","change":{{"text":"y"}}}}"#
            ),
            "malformed_compile_request",
        ),
        (
            format!(r#"{{"compile_version":1,"mode":"edit","source":"{s}","change":{{}}}}"#),
            "malformed_compile_request",
        ),
        (
            format!(
                r#"{{"compile_version":1,"mode":"edit","source":"{s}","change":{{"text":"a","set_constant":{{"name":"b","value":1}}}}}}"#
            ),
            "malformed_compile_request",
        ),
        (
            format!(
                r#"{{"compile_version":1,"mode":"edit","source":"{s}","change":{{"set_constant":{{"name":"b"}}}}}}"#
            ),
            "malformed_compile_request",
        ),
        (
            format!(
                r#"{{"compile_version":1,"mode":"edit","source":"{s}","change":{{"replace":"everything"}}}}"#
            ),
            "malformed_compile_request",
        ),
        // A lone surrogate is not a string this door can carry as an intent.
        (
            format!(r#"{{"compile_version":1,"mode":"create","intent":"\ud800{s}"}}"#),
            "malformed_compile_request",
        ),
    ]
}

fn vocabulary_refusals() -> Vec<(String, &'static str)> {
    let s = SENTINEL;
    vec![
        // Vocabulary this build does not speak is named, never approximated.
        (
            format!(r#"{{"compile_version":2,"mode":"create","intent":"{s}"}}"#),
            "compile_version_unsupported",
        ),
        (
            format!(
                r#"{{"compile_version":2,"mode":"create","intent":"{s}","capability_context":{{"a":1}}}}"#
            ),
            "compile_version_unsupported",
        ),
        (
            format!(r#"{{"compile_version":0,"mode":"create","intent":"{s}"}}"#),
            "compile_version_unsupported",
        ),
        (
            format!(r#"{{"compile_version":1,"mode":"explore","intent":"{s}"}}"#),
            "compile_mode_unsupported",
        ),
        (
            format!(r#"{{"compile_version":1,"mode":"CREATE","intent":"{s}"}}"#),
            "compile_mode_unsupported",
        ),
        (
            format!(r#"{{"compile_version":1,"mode":"create","intent":"{s}","cognition":"warm"}}"#),
            "compile_cognition_unsupported",
        ),
        (
            format!(
                r#"{{"compile_version":1,"mode":"create","intent":"{s}","cognition":"deterministiconly"}}"#
            ),
            "compile_cognition_unsupported",
        ),
    ]
}

#[tokio::test(flavor = "multi_thread")]
async fn protocol_vocabulary_and_shape_refusals_are_typed_and_echo_nothing() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), compile_limits()).await;
    let cases = [
        envelope_refusals(),
        typed_field_refusals(),
        operation_refusals(),
        vocabulary_refusals(),
    ]
    .concat();
    for (body, code) in &cases {
        let (status, refused) = refusal(&server, body).await;
        assert_eq!(status, 422, "{body}");
        assert_eq!(&refused, code, "{body}");
    }
    // The one cognition this build has is accepted when named explicitly.
    let named = server
        .request(&compile_request(
            r#"{"compile_version":1,"mode":"create","intent":"hello","cognition":"deterministicOnly"}"#,
        ))
        .await;
    assert_eq!(named.status, 200, "{}", named.body);
    assert_eq!(named.json()["provenance"]["cognition"], "deterministicOnly");
    assert_eq!(backend.calls(), 0);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn media_type_encoding_and_method_follow_the_other_body_doors() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend, compile_limits()).await;
    let body = r#"{"compile_version":1,"mode":"create","intent":"hello"}"#;
    let with = |extra: &str| {
        format!(
            "POST /v1/compile HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n{extra}Content-Length: {}\r\n{}\r\n{body}",
            body.len(),
            auth_header()
        )
    };
    let text = server.request(&with("Content-Type: text/plain\r\n")).await;
    assert_eq!(text.status, 415);
    assert_eq!(text.json()["error"]["code"], "unsupported_media_type");
    let absent = server.request(&with("")).await;
    assert_eq!(absent.status, 415);
    let gzip = server
        .request(&with(
            "Content-Type: application/json\r\nContent-Encoding: gzip\r\n",
        ))
        .await;
    assert_eq!(gzip.status, 415);
    assert_eq!(gzip.json()["error"]["code"], "unsupported_content_encoding");
    let get = server.request(&get_request("/v1/compile")).await;
    assert_eq!(get.status, 404, "only POST is a compile door");
    let nested = server
        .request(&compile_request(body).replace("/v1/compile ", "/v1/compile/jobs "))
        .await;
    assert_eq!(nested.status, 404);
    server.stop().await.expect("clean stop");
}

fn bound_cases() -> Vec<(&'static str, String, String)> {
    let create = |intent: &str| {
        json!({"compile_version": 1, "mode": "create", "intent": intent}).to_string()
    };
    let padded_source = |total: usize| {
        let padding = total - LITERAL_BASE.len() - 3;
        format!("{LITERAL_BASE}# {}\n", "a".repeat(padding))
    };
    let answers = |count: usize| {
        let map: serde_json::Map<String, Value> =
            (0..count).map(|n| (format!("k{n:03}"), json!(1))).collect();
        json!({"compile_version": 1, "mode": "create", "intent": "hello", "answers": map})
            .to_string()
    };
    let keyed = |length: usize| {
        let map: serde_json::Map<String, Value> =
            std::iter::once(("k".repeat(length), json!(1))).collect();
        json!({"compile_version": 1, "mode": "create", "intent": "hello", "answers": map})
            .to_string()
    };
    let named = |length: usize| {
        json!({"compile_version": 1, "mode": "create", "intent": "hello",
               "workflow_id": "w".repeat(length)})
        .to_string()
    };
    let constant = |length: usize| {
        json!({"compile_version": 1, "mode": "edit", "source": LITERAL_BASE,
               "change": {"set_constant": {"name": "n".repeat(length), "value": 1}}})
        .to_string()
    };
    let string_literal = |bytes: usize| format!("\"{}\"", "a".repeat(bytes - 2));
    assert_eq!(
        padded_source(door::MAX_COMPILE_SOURCE_BYTES).len(),
        door::MAX_COMPILE_SOURCE_BYTES
    );
    vec![
        (
            "intent",
            create(&"a".repeat(door::MAX_COMPILE_TEXT_BYTES)),
            create(&"a".repeat(door::MAX_COMPILE_TEXT_BYTES + 1)),
        ),
        (
            // Bytes, not characters: 2049 two-byte letters are 4098 bytes.
            "intent bytes",
            create(&"é".repeat(door::MAX_COMPILE_TEXT_BYTES / 2)),
            create(&"é".repeat(door::MAX_COMPILE_TEXT_BYTES / 2 + 1)),
        ),
        (
            "change.text",
            json!({"compile_version": 1, "mode": "edit", "source": LITERAL_BASE,
                   "change": {"text": "x".repeat(door::MAX_COMPILE_TEXT_BYTES)}})
            .to_string(),
            json!({"compile_version": 1, "mode": "edit", "source": LITERAL_BASE,
                   "change": {"text": "x".repeat(door::MAX_COMPILE_TEXT_BYTES + 1)}})
            .to_string(),
        ),
        (
            "source",
            edit_body(&padded_source(door::MAX_COMPILE_SOURCE_BYTES), "1"),
            edit_body(&padded_source(door::MAX_COMPILE_SOURCE_BYTES + 1), "1"),
        ),
        (
            "answers",
            answers(door::MAX_COMPILE_ANSWERS),
            answers(door::MAX_COMPILE_ANSWERS + 1),
        ),
        (
            "answer key",
            keyed(door::MAX_COMPILE_ANSWER_KEY_BYTES),
            keyed(door::MAX_COMPILE_ANSWER_KEY_BYTES + 1),
        ),
        (
            "answer literal",
            edit_body(LITERAL_BASE, &string_literal(door::MAX_COMPILE_LITERAL_BYTES)),
            edit_body(LITERAL_BASE, &string_literal(door::MAX_COMPILE_LITERAL_BYTES + 1)),
        ),
        (
            "workflow_id",
            named(door::MAX_COMPILE_NAME_BYTES),
            named(door::MAX_COMPILE_NAME_BYTES + 1),
        ),
        (
            "set_constant.value",
            json!({"compile_version": 1, "mode": "edit", "source": LITERAL_BASE,
                   "change": {"set_constant": {"name": "payload", "value": "a".repeat(door::MAX_COMPILE_LITERAL_BYTES - 2)}}})
                .to_string(),
            json!({"compile_version": 1, "mode": "edit", "source": LITERAL_BASE,
                   "change": {"set_constant": {"name": "payload", "value": "a".repeat(door::MAX_COMPILE_LITERAL_BYTES - 1)}}})
                .to_string(),
        ),
        (
            "set_constant.name",
            constant(door::MAX_COMPILE_NAME_BYTES),
            constant(door::MAX_COMPILE_NAME_BYTES + 1),
        ),
    ]
}

#[tokio::test(flavor = "multi_thread")]
async fn every_bound_accepts_its_exact_limit_and_refuses_the_next_byte() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), compile_limits()).await;
    let exact = bound_cases();
    for (bound, at_limit, beyond) in &exact {
        let accepted = server.request(&compile_request(at_limit)).await;
        assert_eq!(
            accepted.status, 200,
            "{bound} at its limit is authoring data"
        );
        assert!(accepted.json()["status"].is_string(), "{bound}");
        let refused = server.request(&compile_request(beyond)).await;
        assert_eq!(refused.status, 422, "{bound} beyond its limit");
        assert_eq!(refused.json()["error"]["code"], "compile_limit", "{bound}");
        assert!(
            refused.body.len() < 320,
            "{bound}: a refusal never returns the input"
        );
    }
    assert_eq!(backend.calls(), 0);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_oversized_body_is_a_typed_413_declared_or_chunked() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    // The listener would accept 2 MiB; the compile ceiling is the one that binds.
    let server = world.start(backend.clone(), compile_limits()).await;
    let create = r#"{"compile_version":1,"mode":"create","intent":"hello"}"#;
    let exact = format!(
        "{create}{}",
        " ".repeat(door::MAX_COMPILE_BODY_BYTES - create.len())
    );
    let accepted = server.request(&compile_request(&exact)).await;
    assert_eq!(accepted.status, 200, "{}", accepted.body);
    assert_eq!(accepted.json()["status"], "ready");
    let next_byte = server.request(&compile_request(&format!("{exact} "))).await;
    assert_eq!(next_byte.status, 413, "{}", next_byte.body);
    assert_eq!(next_byte.json()["error"]["code"], "body_too_large");
    let padding = "a".repeat(door::MAX_COMPILE_BODY_BYTES);
    let body =
        format!(r#"{{"compile_version":1,"mode":"create","intent":"hello","x":"{padding}"}}"#);
    assert!(body.len() > door::MAX_COMPILE_BODY_BYTES && body.len() < 2 * 1024 * 1024);
    let declared = server.request(&compile_request(&body)).await;
    assert_eq!(declared.status, 413, "{}", declared.body);
    assert_eq!(declared.json()["error"]["code"], "body_too_large");

    // Exactly the ceiling, then one byte: the overflow is decided on the last frame,
    // so no unread upload is left behind when the listener answers and closes.
    let chunk = "a".repeat(64 * 1024);
    assert_eq!(chunk.len() * 16, door::MAX_COMPILE_BODY_BYTES);
    let mut chunked = format!(
        "POST /v1/compile HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n{}\r\n",
        auth_header()
    );
    for _ in 0..16 {
        let _ = write!(chunked, "{:x}\r\n{chunk}\r\n", chunk.len());
    }
    chunked.push_str("1\r\na\r\n0\r\n\r\n");
    let streamed = server.request(&chunked).await;
    assert_eq!(streamed.status, 413, "{}", streamed.body);
    assert_eq!(streamed.json()["error"]["code"], "body_too_large");

    // A listener configured BELOW the compile ceiling keeps its own lower bound.
    let small = TestWorld::new();
    let small_server = small.start(backend.clone(), limits()).await;
    let over_listener = format!(
        r#"{{"compile_version":1,"mode":"create","intent":"{}"}}"#,
        "a".repeat(1100)
    );
    let refused = small_server.request(&compile_request(&over_listener)).await;
    assert_eq!(refused.status, 413, "{}", refused.body);
    assert_eq!(backend.calls(), 0);
    small_server.stop().await.expect("clean stop");
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn hostile_source_and_literals_are_bounded_data_never_a_server_fault() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), compile_limits()).await;
    let mut bomb = String::from("nika: bomb\nconst:\n  a0: &a0 [x, x, x, x, x, x, x, x, x]\n");
    for level in 1..10 {
        let previous = level - 1;
        let alias = format!("*a{previous}");
        let aliases = [alias.as_str(); 9].join(", ");
        let _ = writeln!(bomb, "  a{level}: &a{level} [{aliases}]");
    }
    bomb.push_str("tasks: {}\n");
    let deep_yaml = format!(
        "nika: deep\nconst:\n  nest: {}1{}\ntasks: {{}}\n",
        "[".repeat(1000),
        "]".repeat(1000)
    );
    let sources = [
        ("alias bomb", bomb),
        ("deep flow sequence", deep_yaml),
        (
            "control bytes",
            "nika: ctl\u{0}\u{1b}[31m\u{202e}\nconst: {x: 1}\n".to_owned(),
        ),
        (
            "not yaml",
            "\u{feff}%YAML 9.9\n--- !!binary |\n  AAAA\n...\n".to_owned(),
        ),
        ("empty", String::new()),
    ];
    for (name, source) in &sources {
        let started = std::time::Instant::now();
        let response = server
            .request(&compile_request(&edit_body(source, "1")))
            .await;
        assert_eq!(response.status, 200, "{name}: {}", response.body);
        let document = response.json();
        assert_ne!(document["status"], "ready", "{name}: {document}");
        assert_eq!(
            document["candidate"],
            source.as_str(),
            "{name}: never repaired or rewritten"
        );
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{name} stayed bounded"
        );
    }
    // Literals the transport carries untouched; the core judges each as data.
    for literal in [
        format!("{}{}", "[".repeat(20_000), "]".repeat(20_000)),
        "1e999999".to_owned(),
        "-0.0".to_owned(),
        r#""\ud800""#.to_owned(),
        r#""\u0000\u001b[2J\u202e""#.to_owned(),
    ] {
        let response = server
            .request(&compile_request(&edit_body(LITERAL_BASE, &literal)))
            .await;
        assert_eq!(response.status, 200, "{}", response.body);
        assert!(response.json()["status"].is_string());
    }
    assert_eq!(backend.calls(), 0);
    server.stop().await.expect("clean stop");
}

/// A listener whose first compile parks inside its blocking section until released.
async fn start_parked(
    world: &TestWorld,
    limits: ServerLimits,
) -> (TestServer, oneshot::Receiver<()>, mpsc::Sender<()>) {
    let (entered, entered_signal) = oneshot::channel::<()>();
    let (release, parked_until) = mpsc::channel::<()>();
    let park: CaptureAction = Box::new(move || {
        let _sent = entered.send(());
        let _resumed = parked_until.recv();
    });
    let backend: Arc<dyn ExecutionBackend> =
        Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let resident = ResidentConfig::new(&world.state).with_limits(limits);
    let authority = ResidentAuthority::open(resident, backend)
        .await
        .expect("authority");
    let config = ServerConfig::new(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        &world.workflows,
        &world.token,
    );
    let bound = BoundServer::attach(config, &authority).await.expect("bind");
    *bound.state.before_compile.lock().expect("compile probe") = Some(park);
    let address = bound.local_addr().expect("local address");
    let shutdown_probe = authority.state.store.shutdown_test_probe();
    let shutdown_observer = Arc::clone(&shutdown_probe);
    let (shutdown, receiver) = oneshot::channel();
    let join = tokio::spawn(authority.serve_with_http(bound, async move {
        let _result = receiver.await;
        shutdown_observer.mark_shutdown_loop_observed();
    }));
    let server = TestServer {
        address,
        shutdown: Some(shutdown),
        join,
        shutdown_probe,
    };
    (server, entered_signal, release)
}

fn one_slot_limits(request_timeout: Duration) -> ServerLimits {
    ServerLimits::new(
        2 * 1024 * 1024,
        request_timeout,
        Duration::from_secs(5),
        Duration::from_millis(500),
        4,
        16,
        64,
        32,
    )
    .with_max_compile_requests(1)
}

async fn compiles_again(server: &TestServer, body: &str) -> bool {
    for _ in 0..400 {
        let response = server.request(&compile_request(body)).await;
        if response.status == 200 {
            return response.json()["status"] == "ready";
        }
        assert_eq!(response.status, 503, "{}", response.body);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    false
}

#[tokio::test(flavor = "multi_thread")]
async fn a_timed_out_request_keeps_its_compile_slot_until_the_cpu_work_ends() {
    let world = TestWorld::new();
    let (server, entered, release) =
        start_parked(&world, one_slot_limits(Duration::from_millis(250))).await;
    let body = r#"{"compile_version":1,"mode":"create","intent":"hello"}"#;

    // The handler times out while its blocking section is parked.
    let first = server.request(&compile_request(body)).await;
    assert_eq!(first.status, 408, "{}", first.body);
    assert_eq!(first.json()["error"]["code"], "request_timeout");
    tokio::time::timeout(Duration::from_secs(5), entered)
        .await
        .expect("the blocking section was entered")
        .expect("the parked section signalled before the deadline fired");

    // The request is gone, the CPU work is not: its slot is still taken.
    for _ in 0..3 {
        let saturated = server.request(&compile_request(body)).await;
        assert_eq!(saturated.status, 503, "{}", saturated.body);
        assert_eq!(saturated.json()["error"]["code"], "compile_busy");
    }
    // Saturated authoring takes down no other door.
    assert_eq!(
        server.request(&get_request("/v1/workflows")).await.status,
        200
    );

    release.send(()).expect("the parked section is still alive");
    assert!(
        compiles_again(&server, body).await,
        "the slot returns once the parked work finishes"
    );
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_disconnected_caller_keeps_its_compile_slot_until_the_cpu_work_ends() {
    let world = TestWorld::new();
    let (server, entered, release) =
        start_parked(&world, one_slot_limits(Duration::from_secs(20))).await;
    let body = r#"{"compile_version":1,"mode":"create","intent":"hello"}"#;

    let mut caller = tokio::net::TcpStream::connect(server.address)
        .await
        .expect("connect");
    caller
        .write_all(compile_request(body).as_bytes())
        .await
        .expect("request");
    tokio::time::timeout(Duration::from_secs(5), entered)
        .await
        .expect("the blocking section was entered")
        .expect("the parked section signalled");
    drop(caller);

    let saturated = server.request(&compile_request(body)).await;
    assert_eq!(saturated.status, 503, "{}", saturated.body);
    assert_eq!(saturated.json()["error"]["code"], "compile_busy");

    release.send(()).expect("the parked section is still alive");
    assert!(compiles_again(&server, body).await);
    server.stop().await.expect("clean stop");
}
