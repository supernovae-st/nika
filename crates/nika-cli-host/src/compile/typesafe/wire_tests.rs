// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The production [`TypesafeSeat::exchange`] against a scripted loopback System One peer: no
//! key, no DNS, no paid endpoint. Every script queues a valid answer AFTER the failure under
//! test, so a replayed request would be observable as a second request and a granted answer.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::disallowed_methods
)]
use super::*;
use nika_onboard::compile::decide::ChoiceOption;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const KEY: &str = "fixture-key-0123456789";

enum Reply {
    Status(u16, String),
    Redirect(u16, String),
    Close,
}

/// Records each request head that reached it; answers its script, then drops every connection.
struct Peer {
    base: String,
    heads: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Peer {
    fn start(script: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        let heads = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (log, halt) = (Arc::clone(&heads), Arc::clone(&stop));
        let thread = std::thread::spawn(move || {
            let mut script = script.into_iter();
            for stream in listener.incoming() {
                if halt.load(Ordering::SeqCst) {
                    break;
                }
                let mut stream = stream.expect("stream");
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .expect("timeout");
                let Some(head) = read_request(&mut stream) else {
                    continue;
                };
                log.lock().expect("heads").push(head);
                let reply = match script.next() {
                    Some(Reply::Status(status, body)) => format!(
                        "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    ),
                    Some(Reply::Redirect(status, location)) => format!(
                        "HTTP/1.1 {status} Redirect\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    ),
                    Some(Reply::Close) | None => continue,
                };
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        Self {
            base,
            heads,
            stop,
            thread: Some(thread),
        }
    }

    fn heads(&self) -> Vec<String> {
        self.heads.lock().expect("heads").clone()
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.base.trim_start_matches("http://"));
        if let Some(thread) = self.thread.take() {
            thread.join().expect("peer");
        }
    }
}

/// The request head (lower-cased) once its whole declared body arrived.
fn read_request(stream: &mut TcpStream) -> Option<String> {
    let mut data = Vec::new();
    let mut chunk = [0; 8192];
    let end = loop {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        data.extend_from_slice(&chunk[..n]);
        if let Some(i) = data.windows(4).position(|b| b == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&data[..end]).to_lowercase();
    let length: usize = head
        .lines()
        .find_map(|l| l.strip_prefix("content-length:"))
        .and_then(|s| s.trim().parse().ok())?;
    while data.len() < end + length {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        data.extend_from_slice(&chunk[..n]);
    }
    Some(head)
}

fn question(state: Value) -> ChoiceQuestion {
    ChoiceQuestion::new(
        "q",
        "Which operation does this clause ask for?",
        state,
        vec![
            ChoiceOption::new("search", "find matching items"),
            ChoiceOption::new("lookup", "fetch one record by its identifier"),
        ],
    )
}

fn answer(usage: Option<Value>) -> String {
    let mut body = json!({"model": "jev-1.13.0", "answers": {"q": {"type": "choice",
        "choice": "lookup", "probabilities": {"lookup": 0.9, "search": 0.1}, "confidence": 0.8}}});
    if let Some(usage) = usage {
        body["usage"] = usage;
    }
    body.to_string()
}

fn exchange(peer: &Peer, state: Value) -> Result<Exchange, ExchangeError> {
    let seat = TypesafeSeat::with_base(KEY.to_owned(), "jev-1.13.0", &peer.base).expect("seat");
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(seat.exchange(&question(state)))
}

#[test]
fn a_transient_refusal_is_one_request_and_grants_no_answer() {
    for status in [429, 503, 529] {
        let peer = Peer::start(vec![
            Reply::Status(
                status,
                r#"{"error":"retry after a short delay"}"#.to_owned(),
            ),
            Reply::Status(200, answer(None)),
        ]);
        let failure = exchange(&peer, json!("tickets")).expect_err("a refusal is no answer");
        assert_eq!(failure.delivery, Delivery::Responded(status));
        assert_eq!(peer.heads().len(), 1, "status {status} was retried");
    }
}

#[test]
fn a_connection_dropped_before_a_response_is_unknown_and_never_replayed() {
    let peer = Peer::start(vec![Reply::Close, Reply::Status(200, answer(None))]);
    let failure = exchange(&peer, json!("tickets")).expect_err("no response is no answer");
    assert_eq!(
        failure.delivery,
        Delivery::Unknown,
        "a sent request may be billed"
    );
    assert_eq!(peer.heads().len(), 1, "the dropped request was replayed");
    assert!(
        !failure.error.0.contains(KEY),
        "the key leaked into the failure"
    );
}

#[test]
fn a_redirect_is_reported_never_followed() {
    for status in [307, 308] {
        let target = TcpListener::bind("127.0.0.1:0").expect("target");
        target.set_nonblocking(true).expect("nonblocking");
        let location = format!("http://{}/v1/systemone", target.local_addr().expect("addr"));
        let peer = Peer::start(vec![Reply::Redirect(status, location)]);
        let failure = exchange(&peer, json!("tickets")).expect_err("a redirect is no answer");
        assert_eq!(failure.delivery, Delivery::Responded(status));
        assert_eq!(peer.heads().len(), 1);
        assert_eq!(
            target.accept().map(|_| ()).map_err(|e| e.kind()),
            Err(ErrorKind::WouldBlock),
            "the {status} redirect was followed"
        );
    }
}

#[test]
fn a_malformed_or_foreign_response_grants_no_answer() {
    let bodies = [
        "not json".to_owned(),
        json!({"model": "jev-1.13.0", "answers": {}}).to_string(),
        json!({"answers": {"q": {"type": "choice", "choice": "lookup"}}}).to_string(),
        json!({"model": "jev-1.13.0", "answers": {"q": {"type": "noul", "noul": 0.9}}}).to_string(),
    ];
    for body in bodies {
        let peer = Peer::start(vec![
            Reply::Status(200, body.clone()),
            Reply::Status(200, answer(None)),
        ]);
        let failure = exchange(&peer, json!("tickets")).expect_err("a malformed body answered");
        assert_eq!(failure.delivery, Delivery::Responded(200), "{body}");
        assert_eq!(peer.heads().len(), 1, "{body} was retried");
    }
}

#[test]
fn a_rejected_key_is_one_request_and_never_echoed() {
    let echoed = format!(r#"{{"error":"invalid key Bearer {KEY}"}}"#);
    let peer = Peer::start(vec![
        Reply::Status(401, echoed),
        Reply::Status(200, answer(None)),
    ]);
    let failure = exchange(&peer, json!("tickets")).expect_err("a refused key answered");
    assert_eq!(failure.delivery, Delivery::Responded(401));
    assert_eq!(peer.heads().len(), 1);
    assert!(
        !failure.error.0.contains(KEY),
        "the key leaked into the failure"
    );
}

#[test]
fn one_answer_is_one_bearer_request_and_missing_usage_stays_unknown() {
    let peer = Peer::start(vec![Reply::Status(200, answer(None))]);
    let exchanged = exchange(&peer, json!("tickets")).expect("answer");
    assert_eq!(exchanged.answer.choice, "lookup");
    assert_eq!(
        (
            exchanged.answer.input_tokens,
            exchanged.answer.output_tokens,
            exchanged.billing_units
        ),
        (None, None, None),
        "an unreported usage was invented"
    );
    let heads = peer.heads();
    assert_eq!(heads.len(), 1);
    assert!(
        heads[0].starts_with("post /v1/systemone http/1.1"),
        "{}",
        heads[0]
    );
    let bearer = format!("authorization: bearer {}", KEY.to_lowercase());
    assert!(
        heads[0].lines().any(|l| l == bearer),
        "the key rode outside its header"
    );
}

#[test]
fn json_bytes_are_not_treated_as_a_model_context_limit() {
    let peer = Peer::start(vec![Reply::Status(200, answer(None))]);
    let observed = exchange(&peer, json!("a word ".repeat(10_000))).expect("one request");
    assert_eq!(observed.status, 200);
    assert_eq!(peer.heads().len(), 1);
    let peer = Peer::start(vec![Reply::Status(422, "{}".into())]);
    let failure = exchange(&peer, json!("a word ".repeat(10_000))).expect_err("service refusal");
    assert_eq!(failure.delivery, Delivery::Responded(422));
    assert_eq!(peer.heads().len(), 1, "service refusal is never retried");
}
