// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The session's decision seat through the REAL shared adapter (`TypesafeSeat`), its transport
//! pointed at a loopback System One peer: no ambient key, no env mutation, nothing leaves.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::disallowed_methods
)]
use super::*;
use crate::authoring::{
    AuthoringContext, AuthoringContextError, AuthoringError, AuthoringSeat,
    compile_in_with_admission,
};
use nika_onboard::compile::decide::{ChoiceOption, ChoiceQuestion, DecisionSeat, NONE_OPTION};
use nika_onboard::compile::{
    Cognition, CompileRequest, CompileStatus, NoProvider, Strategy, compile_with_cognition,
};
use nika_types::cost::Cost;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub(crate) const KEY: &str = "fixture-key-0123456789";
pub(crate) const SEAT: &str = "typesafe/jev-test";
/// A lookup by identifier over a file the request reads: a finite ambiguity (search · lookup).
pub(crate) const TICKETS: &str =
    "Read ./tickets.json, find ticket 42 and write it to ./ticket-42.json";
/// A fully readable request: HOT, never a seat call.
const READABLE: &str = "Look up the customer, classify the ticket and draft a reply.";

pub(crate) enum Reply {
    Json(u16, Value),
    Close,
}

/// A loopback System One peer: records each request's path, bearer and body, answers a script.
pub(crate) struct Peer {
    pub(crate) base: String,
    seen: Arc<Mutex<Vec<(String, String, Value)>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Peer {
    pub(crate) fn start(script: Vec<Reply>) -> Self {
        Self::start_with(script, || {})
    }

    /// The same peer, running `on_request` when a request arrived and before it answers.
    pub(crate) fn start_with(script: Vec<Reply>, on_request: impl Fn() + Send + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (log, halt) = (Arc::clone(&seen), Arc::clone(&stop));
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
                let Some((path, bearer, body)) = read(&mut stream) else {
                    continue;
                };
                log.lock().expect("seen").push((path, bearer, body));
                on_request();
                match script.next() {
                    Some(Reply::Json(status, value)) => {
                        let data = value.to_string();
                        let _ = write!(
                            stream,
                            "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{data}",
                            data.len()
                        );
                    }
                    Some(Reply::Close) | None => drop(stream),
                }
            }
        });
        Self {
            base,
            seen,
            stop,
            thread: Some(thread),
        }
    }
    pub(crate) fn requests(&self) -> Vec<(String, String, Value)> {
        self.seen.lock().expect("seen").clone()
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

fn read(stream: &mut TcpStream) -> Option<(String, String, Value)> {
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
    let head = String::from_utf8_lossy(&data[..end]).to_string();
    let path = head.split_whitespace().nth(1).unwrap_or("").to_owned();
    let lower = head.to_lowercase();
    let bearer = lower
        .lines()
        .find_map(|l| l.strip_prefix("authorization:"))
        .map(|v| v.trim().to_owned())
        .unwrap_or_default();
    let n: usize = lower
        .lines()
        .find_map(|l| l.strip_prefix("content-length:"))
        .and_then(|s| s.trim().parse().ok())?;
    while data.len() < end + n {
        let count = stream.read(&mut chunk).ok()?;
        if count == 0 {
            return None;
        }
        data.extend_from_slice(&chunk[..count]);
    }
    Some((
        path,
        bearer,
        serde_json::from_slice(&data[end..end + n]).ok()?,
    ))
}

/// A System One answer to the first ambiguous clause.
pub(crate) fn answer(choice: &str) -> Value {
    let mut probabilities = serde_json::Map::new();
    probabilities.insert(choice.to_owned(), json!(0.9));
    json!({"model": "jev-test", "answers": {"clause-0": {"type": "choice", "choice": choice,
        "probabilities": probabilities, "confidence": 0.7}},
        "usage": {"input_tokens": 40, "output_tokens": 2}})
}

pub(crate) fn setup(peer: &Peer) -> DecisionSetup {
    DecisionSetup::with_key(SEAT, Some(KEY.to_owned()), Some(&peer.base))
}

fn question() -> ChoiceQuestion {
    ChoiceQuestion::new(
        "clause-0",
        "Which operation does this clause ask for?",
        json!({"request": TICKETS}),
        vec![
            ChoiceOption::new("search", "find matching items"),
            ChoiceOption::new("lookup", "fetch one record by its identifier"),
        ],
    )
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(future)
}

/// The compiler alone (no generative seat) under this session seat.
fn warm(seat: &SessionSeat, intent: &str) -> nika_onboard::compile::CompileOutcome {
    block_on(compile_with_cognition::<NoProvider>(
        &CompileRequest::create(intent),
        Cognition {
            provider: None,
            seat: Some(seat),
        },
    ))
    .expect("compile")
}

fn keys(out: &nika_onboard::compile::CompileOutcome) -> Vec<&str> {
    out.questions.iter().map(|q| q.key.as_str()).collect()
}

#[test]
fn a_chosen_option_through_the_session_door_shapes_the_candidate_and_is_journaled() {
    let peer = Peer::start(vec![Reply::Json(200, answer("lookup"))]);
    let context = AuthoringContext::default().with_decision(Some(setup(&peer)));
    assert!(context.refusal().is_none(), "{:?}", context.refusal());
    let seat = AuthoringSeat::Provider {
        model: "mock/echo".to_owned(),
    };
    let account = InferenceAdmission::unbudgeted();
    let out = compile_in_with_admission(
        &seat,
        &context,
        &CompileRequest::create(TICKETS),
        TICKETS,
        &account,
    )
    .expect("seated compile");
    // The decision settled the clause: WARM, one bound value left to ask (the id field).
    assert_eq!(out.provenance.strategy, Some(Strategy::Warm), "{out:#?}");
    assert_eq!(keys(&out), vec!["const.ticket_id_field"], "{out:#?}");
    // Exactly one physical request, to the one endpoint, the key only in its header.
    let seen = peer.requests();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, "/v1/systemone");
    assert_eq!(seen[0].1, format!("bearer {}", KEY.to_lowercase()));
    assert_eq!(seen[0].2["model"], "jev-test");
    assert!(
        seen[0].2["state"]["request"]
            .as_str()
            .unwrap()
            .contains("tickets.json")
    );
    let criteria = seen[0].2["questions"]["clause-0"]["criteria"]
        .as_object()
        .unwrap();
    assert!(criteria.contains_key("lookup") && criteria.contains_key(NONE_OPTION));
    // The receipt beside the compiler's own record, and the persisted journal.
    let decision = out.provenance.decision.as_ref().expect("decision record");
    assert_eq!(decision["questions"][0]["choice"], "lookup");
    let receipt = &decision["session"]["decision_seat"];
    assert_eq!(
        decision["session"]["authoring"]["strategy"], "escalate",
        "{decision}"
    );
    assert_eq!(receipt["schema"], DECISION_SCHEMA);
    assert_eq!(receipt["calls_sent"], 1);
    assert_eq!(receipt["retries"], 0);
    let role = receipt["role"].as_str().unwrap();
    assert!(
        role.starts_with("compiler routing") && role.contains("never Foundry"),
        "{role}"
    );
    assert_eq!(receipt["attempts"][0]["outcome"], "chosen");
    assert_eq!(receipt["attempts"][0]["status"], 200);
    assert_eq!(receipt["attempts"][0]["usage"]["input_tokens"], 40);
    assert_eq!(
        receipt["attempts"][0]["usage"]["billing_units"],
        Value::Null
    );
    assert_eq!(receipt["endpoint_host"], "127.0.0.1");
    assert!(receipt["cost"].as_str().unwrap().starts_with("unknown"));
    assert_eq!(receipt["state"], "Open");
    let journal = context.decision().unwrap().observations();
    assert_eq!(journal.len(), 1);
    assert_eq!(journal[0]["unbudgeted"], true);
    let text = serde_json::to_string(&journal).unwrap() + &format!("{context:?}");
    assert!(!text.contains(KEY), "the key leaked into a record or Debug");
}

#[test]
fn none_leaves_the_clause_to_a_clarification_never_the_least_wrong_option() {
    let peer = Peer::start(vec![Reply::Json(200, answer(NONE_OPTION))]);
    let seat = setup(&peer).consult(Ok(()));
    let out = warm(&seat, TICKETS);
    assert!(out.candidate.is_none());
    assert_ne!(out.status, CompileStatus::Ready);
    let receipt = seat.receipt().expect("needed");
    assert_eq!(receipt["attempts"][0]["outcome"], "none");
    assert_eq!(peer.requests().len(), 1);
}

#[test]
fn an_answer_outside_the_options_is_refused_by_the_compiler_and_recorded() {
    let peer = Peer::start(vec![Reply::Json(200, answer("send"))]);
    let seat = setup(&peer).consult(Ok(()));
    let out = warm(&seat, TICKETS);
    assert!(out.candidate.is_none());
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
    let record = out.provenance.decision.as_ref().expect("decision record");
    assert!(record["questions"][0]["error"].is_string(), "{record}");
    assert_eq!(
        seat.receipt().unwrap()["attempts"][0]["outcome"],
        "outside_options"
    );
}

#[test]
fn a_settled_readable_request_never_calls_the_seat() {
    let peer = Peer::start(vec![Reply::Json(200, answer("lookup"))]);
    let context = AuthoringContext::default().with_decision(Some(setup(&peer)));
    let seat = AuthoringSeat::Provider {
        model: "mock/echo".to_owned(),
    };
    let account = InferenceAdmission::unbudgeted();
    let out = compile_in_with_admission(
        &seat,
        &context,
        &CompileRequest::create(READABLE),
        READABLE,
        &account,
    )
    .expect("seated compile");
    assert_eq!(out.provenance.strategy, Some(Strategy::Hot), "{out:#?}");
    assert!(peer.requests().is_empty());
    assert!(context.decision().unwrap().observations().is_empty());
}

#[test]
fn a_numeric_zero_or_closed_account_is_never_charged_and_the_need_stays_visible() {
    let numeric = InferenceAdmission::new(Cost::new(500_000_000)).unwrap();
    let zero = InferenceAdmission::new(Cost::zero()).unwrap();
    let closed = InferenceAdmission::unbudgeted();
    closed.close("test").unwrap();
    for account in [numeric, zero, closed] {
        let peer = Peer::start(vec![Reply::Json(200, answer("lookup"))]);
        let verdict = admit(Some(&account));
        assert!(verdict.is_err());
        let seat = setup(&peer).consult(verdict);
        let out = warm(&seat, TICKETS);
        assert!(
            peer.requests().is_empty(),
            "a numeric or closed account was charged"
        );
        assert_ne!(out.provenance.strategy, Some(Strategy::Warm));
        let record = out
            .provenance
            .decision
            .as_ref()
            .expect("the need is recorded");
        assert!(
            record["questions"][0]["error"]
                .as_str()
                .unwrap()
                .contains("not consulted")
        );
        let receipt = seat.receipt().unwrap();
        assert_eq!(receipt["calls_sent"], 0, "claimed as used");
        assert_eq!(receipt["attempts"][0]["outcome"], "refused");
        assert!(receipt["refused"].is_string());
    }
    assert!(
        admit(None).is_err(),
        "a local or unpriced route never consults the service"
    );
    assert!(admit(Some(&InferenceAdmission::unbudgeted())).is_ok());
}

#[test]
fn missing_credentials_or_another_vendor_are_a_visible_configuration_refusal() {
    let peer = Peer::start(vec![]);
    for setup in [
        DecisionSetup::with_key(SEAT, None, None),
        DecisionSetup::with_key("deepseek/deepseek-flash", Some(KEY.to_owned()), None),
        DecisionSetup::with_key("typesafe/", Some(KEY.to_owned()), None),
    ] {
        let context = AuthoringContext::default().with_decision(Some(setup));
        assert!(matches!(
            context.refusal(),
            Some(AuthoringContextError::Decision { .. })
        ));
        assert!(context.line().contains("refused"), "{}", context.line());
        let error = compile_in_with_admission(
            &AuthoringSeat::Provider {
                model: "mock/echo".to_owned(),
            },
            &context,
            &CompileRequest::create(TICKETS),
            TICKETS,
            &InferenceAdmission::unbudgeted(),
        )
        .unwrap_err();
        assert!(matches!(error, AuthoringError::Context(_)), "{error:?}");
    }
    assert!(peer.requests().is_empty());
}

#[test]
fn the_fourth_need_of_one_request_is_refused_unsent_by_the_cap() {
    let peer = Peer::start((0..4).map(|_| Reply::Json(200, answer("lookup"))).collect());
    let seat = setup(&peer).consult(Ok(()));
    let question = question();
    let results: Vec<_> = (0..4).map(|_| block_on(seat.choose(&question))).collect();
    assert!(results[..3].iter().all(Result::is_ok));
    assert!(results[3].as_ref().unwrap_err().0.contains("cap"));
    assert_eq!(peer.requests().len(), MAX_DECISION_CALLS);
    let receipt = seat.receipt().unwrap();
    assert_eq!(receipt["calls_sent"], 3);
    assert_eq!(receipt["attempts"][3]["outcome"], "capped");
    assert_eq!(receipt["attempts"][3]["sent"], false);
}

#[test]
fn an_http_error_is_recorded_once_and_never_retried() {
    let peer = Peer::start(vec![
        Reply::Json(500, json!({"error": "boom"})),
        Reply::Json(200, answer("lookup")),
    ]);
    let seat = setup(&peer).consult(Ok(()));
    let error = block_on(seat.choose(&question())).unwrap_err();
    assert!(error.0.contains("500"), "{error:?}");
    assert_eq!(peer.requests().len(), 1, "retried");
    let receipt = seat.receipt().unwrap();
    assert_eq!(receipt["attempts"][0]["outcome"], "http_error");
    assert_eq!(receipt["attempts"][0]["status"], 500);
    assert_eq!(receipt["state"], "Open");
}

#[test]
fn a_request_left_without_a_response_is_an_uncertain_charge() {
    let peer = Peer::start(vec![Reply::Close]);
    let seat = setup(&peer).consult(Ok(()));
    let error = block_on(seat.choose(&question())).unwrap_err();
    assert!(error.0.contains("transport"), "{error:?}");
    let receipt = seat.receipt().unwrap();
    assert_eq!(receipt["attempts"][0]["outcome"], "transport_error");
    assert_eq!(receipt["attempts"][0]["sent"], true);
    assert_eq!(receipt["state"], "Uncertain");
    assert_eq!(receipt["unknown_calls"], 1);
}

#[test]
fn the_status_line_names_the_seat_its_bounds_and_its_unknown_cost() {
    let peer = Peer::start(vec![]);
    let line = AuthoringContext::default()
        .with_decision(Some(setup(&peer)))
        .line();
    assert!(line.contains(SEAT), "{line}");
    assert!(line.contains("at most 3 call(s)"), "{line}");
    assert!(line.contains("cost unknown"), "{line}");
    assert!(!line.contains(KEY));
}
