// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The HTTP door over a real listener, and its parity with the native door: the same command
//! script on two copies of one project gives the same frames (Session identities, snapshot
//! handles and the project root aside) and the same bytes on disk.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use http_body_util::BodyExt as _;
use hyper::body::Incoming;
use serde_json::Value;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use super::*;
use crate::host::tests::{COPY, Gate, runtime, world};
use crate::machine::tests::Lines;
use crate::run::NoRunDoor;

const WAIT: Duration = Duration::from_secs(60);

fn sessions(root: &Path) -> Arc<Sessions> {
    let root = root.to_path_buf();
    let opener: Opener = Arc::new(move || Ok((runtime(&root), Vec::new())));
    let doors: Doors = Box::new(|| Box::new(NoRunDoor::new("none")));
    Arc::new(Sessions::new(opener, doors))
}

/// A listener that answers every request with the Session routes (the server's own checks
/// sit in front of them in `nika serve`).
async fn listen(sessions: Arc<Sessions>) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let sessions = Arc::clone(&sessions);
            tokio::spawn(async move {
                let service =
                    hyper::service::service_fn(move |request: hyper::Request<Incoming>| {
                        let sessions = Arc::clone(&sessions);
                        async move {
                            let (parts, body) = request.into_parts();
                            let body = (body.collect().await)
                                .map(http_body_util::Collected::to_bytes)
                                .unwrap_or_default();
                            let routed = sessions
                                .route(&parts.method, parts.uri.path(), &parts.headers, body)
                                .await;
                            Ok::<_, Infallible>(routed.unwrap_or_else(|| {
                                let mut missing =
                                    Response::new(Full::new(Bytes::new()).boxed_unsync());
                                *missing.status_mut() = StatusCode::NOT_FOUND;
                                missing
                            }))
                        }
                    });
                let io = hyper_util::rt::TokioIo::new(stream);
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, service)
                    .await;
            });
        }
    });
    address
}

fn head(method: &str, path: &str, body: &str, extra: &str) -> String {
    format!(
        "{method} {path} HTTP/1.1\r\nHost: test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",
        body.len()
    )
}

/// One request, its status and its JSON body.
async fn call(address: SocketAddr, method: &str, path: &str, body: &str) -> (u16, Value) {
    let mut stream = tokio::net::TcpStream::connect(address)
        .await
        .expect("connect");
    stream
        .write_all(head(method, path, body, "").as_bytes())
        .await
        .expect("request");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("response");
    let text = String::from_utf8(raw).expect("utf-8");
    let (top, body) = text.split_once("\r\n\r\n").expect("head");
    let status = top
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("status");
    let value = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(body).expect("json body")
    };
    (status, value)
}

/// An event stream, opened: its connection, once the server answered its head (the Session
/// was found), and what was read past that head.
async fn subscribe(
    address: SocketAddr,
    path: &str,
    last: Option<&str>,
) -> (tokio::net::TcpStream, Vec<u8>) {
    let mut stream = tokio::net::TcpStream::connect(address)
        .await
        .expect("connect");
    let extra = last
        .map(|id| format!("Last-Event-ID: {id}\r\n"))
        .unwrap_or_default();
    let request = head("GET", path, "", &extra);
    stream.write_all(request.as_bytes()).await.expect("request");
    let mut read = Vec::new();
    let mut buffer = [0_u8; 4096];
    while !read.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = tokio::time::timeout(WAIT, stream.read(&mut buffer))
            .await
            .expect("a head")
            .expect("read");
        assert!(n > 0, "the stream closed before its head");
        read.extend_from_slice(&buffer[..n]);
    }
    (stream, read)
}

/// Every event of a stream that ended: (id, frame).
async fn drain((mut stream, mut raw): (tokio::net::TcpStream, Vec<u8>)) -> Vec<(String, Value)> {
    tokio::time::timeout(WAIT, stream.read_to_end(&mut raw))
        .await
        .expect("the stream ends when the Session closes")
        .expect("read");
    let text = String::from_utf8(raw).expect("utf-8");
    let (top, chunked) = text.split_once("\r\n\r\n").expect("head");
    assert!(top.starts_with("HTTP/1.1 200"), "{top}");
    assert!(top.to_ascii_lowercase().contains("text/event-stream"));
    let mut body = String::new();
    let mut rest = chunked;
    while let Some((size, after)) = rest.split_once("\r\n") {
        let size = usize::from_str_radix(size.trim(), 16).expect("chunk size");
        if size == 0 {
            break;
        }
        body.push_str(&after[..size]);
        rest = &after[size + 2..];
    }
    (body.split("\n\n").filter(|e| !e.trim().is_empty()))
        .map(|event| {
            let id = event
                .lines()
                .find_map(|l| l.strip_prefix("id: "))
                .unwrap_or_default();
            let data = event
                .lines()
                .find_map(|l| l.strip_prefix("data: "))
                .expect("data");
            (id.to_owned(), serde_json::from_str(data).expect("frame"))
        })
        .collect()
}

fn command(op: &str, command: &str, snapshot: &Value, line: &str) -> Value {
    if op == "stop" {
        return serde_json::json!({"contract": CONTRACT, "op": "stop", "command": command});
    }
    serde_json::json!({
        "contract": CONTRACT, "op": "submit", "command": command,
        "snapshot": snapshot, "line": line,
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_live_session_per_project_and_its_routes_answer_by_identity() {
    let root = world();
    let address = listen(sessions(root.path())).await;
    let (status, opened) = call(address, "POST", "/v1/sessions", "").await;
    assert_eq!(status, 201);
    let session = opened["session"].as_str().expect("session").to_owned();
    let (status, live) = call(address, "POST", "/v1/sessions", "").await;
    assert_eq!(status, 409);
    assert_eq!(live["error"], "session_live");
    assert_eq!(
        live["session"],
        session.as_str(),
        "the live identity to attach to"
    );
    let (status, missing) = call(address, "GET", "/v1/sessions/ses_other", "").await;
    assert_eq!(
        (status, missing["error"].as_str()),
        (404, Some("session_not_found"))
    );
    let read = serde_json::json!({"contract": CONTRACT, "op": "snapshot"}).to_string();
    let (status, refused) = call(
        address,
        "POST",
        &format!("/v1/sessions/{session}/commands"),
        &read,
    )
    .await;
    assert_eq!(
        (status, refused["error"].as_str()),
        (400, Some("malformed"))
    );
    let wrong = r#"{"contract":"nika/session-host@9"}"#;
    let (status, _) = call(address, "POST", "/v1/sessions", wrong).await;
    assert_eq!(status, 400);
    let (status, closed) = call(address, "DELETE", &format!("/v1/sessions/{session}"), "").await;
    assert_eq!((status, closed["frame"].as_str()), (200, Some("closed")));
    let (status, _) = call(address, "GET", &format!("/v1/sessions/{session}"), "").await;
    assert_eq!(status, 404, "a closed Session is gone");
    let (status, reopened) = call(address, "POST", "/v1/sessions", "").await;
    assert_eq!(status, 201);
    assert_ne!(reopened["session"], session.as_str(), "another incarnation");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_command_outlives_the_request_that_carried_it() {
    let root = world();
    let sessions = sessions(root.path());
    let address = listen(Arc::clone(&sessions)).await;
    let (_, opened) = call(address, "POST", "/v1/sessions", "").await;
    let session = opened["session"].as_str().expect("session").to_owned();
    let host = sessions.any_live().await.expect("live");
    let gate = Gate::default();
    host.pause_at(gate.pause());
    gate.hold("returned");
    let body = command("submit", "c-1", &opened["snapshot"]["snapshot"], COPY).to_string();
    let path = format!("/v1/sessions/{session}/commands");
    {
        // The client leaves before the result: the turn is the worker's, not the request's.
        let mut stream = tokio::net::TcpStream::connect(address)
            .await
            .expect("connect");
        stream
            .write_all(head("POST", &path, &body, "").as_bytes())
            .await
            .expect("request");
        let blocking = Arc::clone(&host);
        let reached = gate.clone();
        tokio::task::spawn_blocking(move || {
            reached.reached();
            drop(blocking);
        })
        .await
        .expect("held");
    }
    gate.release();
    let (status, result) = call(address, "POST", &path, &body).await;
    assert_eq!(status, 200);
    assert_eq!(result["replayed"], true);
    assert_eq!(result["outcomes"][0]["kind"], "proposal");
    let (events, _) = host.events_after(0);
    assert_eq!(events.iter().filter(|f| f.kind() == "accepted").count(), 1);
    let conflict = command("submit", "c-1", &opened["snapshot"]["snapshot"], "other").to_string();
    let (status, refused) = call(address, "POST", &path, &conflict).await;
    assert_eq!(
        (status, refused["error"].as_str()),
        (409, Some("command_conflict"))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cursor_names_its_session_and_any_other_resyncs() {
    let root = world();
    let address = listen(sessions(root.path())).await;
    let (_, opened) = call(address, "POST", "/v1/sessions", "").await;
    let session = opened["session"].as_str().expect("session").to_owned();
    let path = format!("/v1/sessions/{session}/commands");
    let body = command("submit", "c-1", &opened["snapshot"]["snapshot"], COPY).to_string();
    let (_, result) = call(address, "POST", &path, &body).await;
    let at = result["event"].as_u64().expect("event");
    let events = format!("/v1/sessions/{session}/events");
    let all = subscribe(address, &events, None).await;
    let resumed = subscribe(address, &events, Some(&format!("{session}:2"))).await;
    let foreign = subscribe(address, &events, Some("ses_other:2")).await;
    let future = subscribe(address, &events, Some(&format!("{session}:{}", at + 50))).await;
    let (status, _) = call(address, "DELETE", &format!("/v1/sessions/{session}"), "").await;
    assert_eq!(status, 200);
    let all = drain(all).await;
    assert_eq!(all[0].0, format!("{session}:1"));
    assert_eq!(all[0].1["frame"], "opened");
    assert_eq!(
        all.last().map(|(_, f)| f["frame"].clone()),
        Some("closed".into())
    );
    let numbers: Vec<u64> = all
        .iter()
        .filter_map(|(_, f)| f["event"].as_u64())
        .collect();
    assert_eq!(
        numbers,
        (1..=numbers.len() as u64).collect::<Vec<_>>(),
        "no hole"
    );
    let resumed = drain(resumed).await;
    assert_eq!(resumed[0].1["event"], 3, "resumes after the named event");
    for stream in [drain(foreign).await, drain(future).await] {
        assert_eq!(stream[0].1["frame"], "resync", "{stream:?}");
        assert!(stream[0].1["snapshot"]["work"].is_object());
        assert_eq!(
            stream.last().map(|(_, f)| f["frame"].clone()),
            Some("closed".into())
        );
    }
}

/// Session identities, snapshot handles and the project root, named by first appearance.
fn normalize(value: &mut Value, root: &str, names: &mut BTreeMap<String, String>) {
    match value {
        Value::String(text) => {
            let found = names.len();
            if text.starts_with("snp_") || text.starts_with("ses_") {
                let name = names
                    .entry(text.clone())
                    .or_insert_with(|| format!("{}#{found}", &text[..3]));
                *text = name.clone();
            } else if text.contains(root) {
                *text = text.replace(root, "<root>");
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|v| normalize(v, root, names)),
        Value::Object(map) => map.values_mut().for_each(|v| normalize(v, root, names)),
        _ => {}
    }
}

/// The parity script on the native door: each command's own answer, and the whole log.
fn native_script(root: &Path) -> (Vec<Value>, Vec<Value>) {
    let host = Arc::new(
        SessionHost::start(runtime(root), Box::new(NoRunDoor::new("none")), Vec::new())
            .expect("host"),
    );
    let (reader, mut input) = std::io::pipe().expect("pipe");
    let (sender, lines) = mpsc::channel::<String>();
    let output = Arc::new(Mutex::new(Lines(sender, Vec::new())));
    let driven = Arc::clone(&host);
    std::thread::Builder::new()
        .name("native".to_owned())
        .spawn(move || crate::machine::drive(&driven, std::io::BufReader::new(reader), &output))
        .expect("native door");
    let mut log = Vec::new();
    let mut answers = Vec::new();
    {
        let mut next = |want: &dyn Fn(&Value) -> bool| -> Value {
            loop {
                let line = lines.recv_timeout(WAIT).expect("frame");
                let frame: Value = serde_json::from_str(&line).expect("json");
                if frame.get("event").is_some() && frame["replayed"] != true {
                    log.push(frame.clone());
                }
                if want(&frame) {
                    return frame;
                }
            }
        };
        let opened = next(&|f| f["frame"] == "opened");
        let first = opened["snapshot"]["snapshot"].clone();
        answers.push(opened);
        let mut send = |value: Value| writeln!(input, "{value}").expect("stdin");
        send(command("submit", "c-1", &first, COPY));
        let proposed = next(&|f| f["frame"] == "result" && f["command"] == "c-1");
        let current = proposed["snapshot"]["snapshot"].clone();
        answers.push(proposed);
        send(command("submit", "c-2", &first, "yes"));
        answers.push(next(&|f| f["frame"] == "refused"));
        send(command("submit", "c-1", &first, COPY));
        answers.push(next(&|f| f["replayed"] == true));
        send(serde_json::json!({"contract": CONTRACT, "op": "details"}));
        answers.push(next(&|f| f["frame"] == "details"));
        send(command("submit", "c-3", &current, "yes"));
        answers.push(next(&|f| f["frame"] == "result" && f["command"] == "c-3"));
        send(command("stop", "s-1", &Value::Null, ""));
        answers.push(next(&|f| f["frame"] == "result" && f["command"] == "s-1"));
        send(serde_json::json!({"contract": CONTRACT, "op": "close"}));
        answers.push(next(&|f| f["frame"] == "closed"));
    }
    (answers, log)
}

/// The same script on the HTTP door.
async fn http_script(root: &Path) -> (Vec<Value>, Vec<Value>) {
    let address = listen(sessions(root)).await;
    let (status, opened) = call(address, "POST", "/v1/sessions", "").await;
    assert_eq!(status, 201);
    let session = opened["session"].as_str().expect("session").to_owned();
    let stream = subscribe(address, &format!("/v1/sessions/{session}/events"), None).await;
    let first = opened["snapshot"]["snapshot"].clone();
    let path = format!("/v1/sessions/{session}/commands");
    let post = |value: Value| {
        let path = path.clone();
        async move { call(address, "POST", &path, &value.to_string()).await.1 }
    };
    let mut answers = vec![opened];
    let proposed = post(command("submit", "c-1", &first, COPY)).await;
    let current = proposed["snapshot"]["snapshot"].clone();
    answers.push(proposed);
    answers.push(post(command("submit", "c-2", &first, "yes")).await);
    answers.push(post(command("submit", "c-1", &first, COPY)).await);
    answers.push(
        call(
            address,
            "GET",
            &format!("/v1/sessions/{session}/details"),
            "",
        )
        .await
        .1,
    );
    answers.push(post(command("submit", "c-3", &current, "yes")).await);
    answers.push(post(command("stop", "s-1", &Value::Null, "")).await);
    answers.push(
        call(address, "DELETE", &format!("/v1/sessions/{session}"), "")
            .await
            .1,
    );
    let log = drain(stream)
        .await
        .into_iter()
        .map(|(_, frame)| frame)
        .collect();
    (answers, log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_native_and_http_doors_give_the_same_session() {
    let (native_root, http_root) = (world(), world());
    let native_path = native_root.path().to_path_buf();
    let (mut native, mut native_log) =
        tokio::task::spawn_blocking(move || native_script(&native_path))
            .await
            .expect("native");
    let (mut http, mut http_log) = http_script(http_root.path()).await;
    let normalized = |values: &mut Vec<Value>, root: &Path| {
        let canonical = root.canonicalize().expect("root");
        let mut names = BTreeMap::new();
        for value in values.iter_mut() {
            normalize(value, &canonical.display().to_string(), &mut names);
            normalize(value, &root.display().to_string(), &mut names);
        }
    };
    normalized(&mut native, native_root.path());
    normalized(&mut http, http_root.path());
    assert_eq!(native.len(), http.len());
    for (one, other) in native.iter().zip(&http) {
        assert_eq!(one, other, "each command answers the same on both doors");
    }
    normalized(&mut native_log, native_root.path());
    normalized(&mut http_log, http_root.path());
    assert_eq!(native_log, http_log, "the same log, event by event");
    let kinds: Vec<&str> = native
        .iter()
        .map(|f| f["frame"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(
        kinds,
        [
            "opened", "result", "refused", "result", "details", "result", "result", "closed"
        ]
    );
    assert_eq!(native[2]["error"], "stale_snapshot");
    assert_eq!(native[3]["replayed"], true);
    assert_eq!(native[6]["receipt"], "nothing_to_stop");
    let saved = native[5]["snapshot"]["work"]["saved"]["workflow"]
        .as_str()
        .expect("saved")
        .to_owned();
    let bytes = |root: &Path| std::fs::read(root.join(&saved)).expect("saved bytes");
    assert_eq!(bytes(native_root.path()), bytes(http_root.path()));
}
