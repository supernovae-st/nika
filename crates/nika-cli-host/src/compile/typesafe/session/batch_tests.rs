// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A batch through the session's journaling seat, its transport the real shared adapter on a
//! loopback System One peer: ONE journaled attempt per physical request, written before it can
//! leave, with its items' outcomes and the request's usage once. No key leaves, nothing paid.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::disallowed_methods
)]
use super::super::wire_tests::{KEY, Peer, Reply};
use super::*;
use nika_onboard::compile::decide::ChoiceOption;
use std::sync::atomic::{AtomicBool, Ordering};

const SEAT: &str = "typesafe/jev-test";

fn setup(peer: &Peer) -> DecisionSetup {
    DecisionSetup::with_key(SEAT, Some(KEY.to_owned()), Some(&peer.base))
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(future)
}

fn part(k: usize) -> ChoiceQuestion {
    ChoiceQuestion::new(
        format!("verify-part-{k}"),
        "REFERENCE.\n\nJudge this part alone.",
        json!({"request": "the whole request", "clause": {"text": format!("clause {k}")}}),
        vec![
            ChoiceOption::new("carried", "done"),
            ChoiceOption::new("missing", "not done"),
        ],
    )
}

fn parts() -> ChoiceBatch {
    ChoiceBatch::of("verify-parts", &[part(0), part(1), part(2)])
}

fn reply(answers: &str) -> Reply {
    Reply::Status(
        200,
        format!(
            r#"{{"model": "jev-test", "answers": {{{answers}}}, "usage": {{"input_tokens": 120, "output_tokens": 6}}}}"#
        ),
    )
}

fn choice(id: &str, key: &str) -> String {
    format!(r#""{id}": {{"type": "choice", "choice": "{key}", "confidence": 0.7}}"#)
}

/// A batch is ONE journaled physical request: one call sent (its cost unknown), one attempt
/// naming the batch, each item's outcome bound by id, the usage once; the scope closes clean.
#[test]
fn a_batch_is_one_journaled_request_with_its_items_and_usage_once() {
    let answers = [
        choice("verify-part-2", "missing"),
        choice("verify-part-0", "carried"),
        choice("verify-part-9", "carried"),
    ]
    .join(", ");
    let peer = Peer::start(vec![reply(&answers), reply(&answers)]);
    let setup = setup(&peer);
    let seat = setup.consult(Ok(()));
    let answers = block_on(seat.choose_each(&parts()));
    assert_eq!(peer.heads().len(), 1, "one physical request");
    let chosen: Vec<Option<&str>> = (answers.iter())
        .map(|a| a.as_ref().ok().map(|a| a.choice.as_str()))
        .collect();
    assert_eq!(chosen, [Some("carried"), None, Some("missing")]);
    let receipt = seat.finish().expect("the seat was needed");
    assert_eq!(receipt["calls_sent"], 1);
    assert_eq!(receipt["unknown_calls"], 1);
    assert_eq!(receipt["state"], "Closed");
    let attempts = receipt["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 1, "one attempt for one request");
    let attempt = &attempts[0];
    assert_eq!(attempt["batch"], "verify-parts");
    assert_eq!(
        (&attempt["sent"], &attempt["outcome"], &attempt["status"]),
        (&json!(true), &json!("answered"), &json!(200))
    );
    assert_eq!(attempt["model"], "jev-test");
    assert_eq!(
        attempt["usage"],
        json!({"input_tokens": 120, "output_tokens": 6, "billing_units": null}),
        "the request's usage, once"
    );
    let outcomes: Vec<&str> = (attempt["items"].as_array().unwrap().iter())
        .map(|item| item["outcome"].as_str().unwrap())
        .collect();
    assert_eq!(outcomes, ["chosen", "unanswered", "chosen"]);
    assert_eq!(attempt["items"][0]["choice"], "carried");
    assert_eq!(
        attempt["items"][0]["options"],
        json!(["carried", "missing", "none"])
    );
    assert!(attempt["items"][1]["error"].is_string());
    assert!(
        attempt["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item.get("usage").is_none())
    );
    assert_eq!(attempt["unasked"], json!(["verify-part-9"]));
    assert_eq!(setup.observations(), vec![receipt]);
}

/// A batch the money law refuses sends nothing: ONE refused attempt naming its items, no call.
#[test]
fn a_refused_batch_sends_nothing_and_is_one_refused_attempt() {
    let peer = Peer::start(vec![reply(&choice("verify-part-0", "carried"))]);
    let seat = setup(&peer).consult(Err("the allowance is spent".into()));
    let answers = block_on(seat.choose_each(&parts()));
    assert!(
        answers
            .iter()
            .all(|a| a.as_ref().is_err_and(|e| e.0.contains("not consulted")))
    );
    assert!(peer.heads().is_empty());
    let receipt = seat.receipt().unwrap();
    assert_eq!(receipt["calls_sent"], 0);
    assert_eq!(receipt["attempts"].as_array().unwrap().len(), 1);
    assert_eq!(receipt["attempts"][0]["outcome"], "refused");
    assert_eq!(receipt["attempts"][0]["sent"], false);
    assert_eq!(receipt["attempts"][0]["items"].as_array().unwrap().len(), 3);
    assert_eq!(receipt["refused"], "not consulted: the allowance is spent");
}

/// A batch request left without a response is ONE uncertain charge: sent, never retried.
#[test]
fn a_batch_left_without_a_response_is_one_uncertain_charge() {
    let peer = Peer::start(vec![
        Reply::Close,
        reply(&choice("verify-part-0", "carried")),
    ]);
    let seat = setup(&peer).consult(Ok(()));
    let answers = block_on(seat.choose_each(&parts()));
    assert!(answers.iter().all(Result::is_err));
    assert_eq!(peer.heads().len(), 1, "retried");
    let receipt = seat.receipt().unwrap();
    assert_eq!(receipt["attempts"][0]["outcome"], "transport_error");
    assert_eq!(receipt["attempts"][0]["sent"], true);
    assert_eq!(receipt["state"], "Uncertain");
    assert_eq!(
        (&receipt["calls_sent"], &receipt["unknown_calls"]),
        (&json!(1), &json!(1))
    );
}

/// A batch cancelled while its request is out is never closed as settled: its ONE attempt stays
/// in flight and the scope ends Uncertain.
#[test]
fn a_cancelled_batch_stays_in_flight_and_uncertain() {
    let reached = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&reached);
    let peer = Peer::start_with(
        vec![reply(&choice("verify-part-0", "carried"))],
        move || {
            flag.store(true, Ordering::SeqCst);
        },
    );
    let setup = setup(&peer);
    let seat = setup.consult(Ok(()));
    let batch = parts();
    // Read on every poll, before the batch: the peer raises it before it answers, so the batch
    // can never settle first.
    let arrived = std::future::poll_fn(|cx| {
        if reached.load(Ordering::SeqCst) {
            std::task::Poll::Ready(())
        } else {
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    });
    block_on(async {
        tokio::select! {
            biased;
            () = arrived => {}
            _ = seat.choose_each(&batch) => panic!("the batch settled before cancellation"),
        }
    });
    drop(seat);
    assert_eq!(peer.heads().len(), 1);
    let kept = setup.observations();
    assert_eq!(kept[0]["state"], "Uncertain");
    assert_eq!(kept[0]["scope_ended"], true);
    assert_eq!(kept[0]["attempts"][0]["outcome"], "in_flight");
    assert_eq!(kept[0]["attempts"][0]["items"].as_array().unwrap().len(), 3);
    assert_eq!(kept[0]["unknown_calls"], 1);
}

/// An empty batch asks nothing and invents no observation; a batch whose every id is asked
/// twice leaves nothing and counts no call.
#[test]
fn an_empty_or_unsendable_batch_sends_and_counts_nothing() {
    let peer = Peer::start(vec![reply(&choice("verify-part-0", "carried"))]);
    let setup = setup(&peer);
    let seat = setup.consult(Ok(()));
    let empty = ChoiceBatch::new("empty", "", json!({}), Vec::new());
    assert!(block_on(seat.choose_each(&empty)).is_empty());
    assert!(
        seat.receipt().is_none(),
        "an empty batch made an observation"
    );
    let twice = ChoiceBatch::of("twice", &[part(0), part(0)]);
    let answers = block_on(seat.choose_each(&twice));
    assert!(
        answers
            .iter()
            .all(|a| a.as_ref().is_err_and(|e| e.0.contains("more than once")))
    );
    assert!(peer.heads().is_empty());
    let receipt = seat.finish().unwrap();
    assert_eq!(receipt["attempts"][0]["outcome"], "not_sent");
    assert_eq!(receipt["attempts"][0]["sent"], false);
    assert_eq!(
        (&receipt["calls_sent"], &receipt["unknown_calls"]),
        (&json!(0), &json!(0))
    );
    assert_eq!(receipt["state"], "Closed");
}

/// A malformed reply answers no item but its usage is still journaled, on the attempt, once.
#[test]
fn a_reply_that_answers_no_item_still_journals_its_usage() {
    let malformed = r#"{"answers": {}, "usage": {"input_tokens": 55, "output_tokens": 2}}"#;
    let peer = Peer::start(vec![Reply::Status(200, malformed.into())]);
    let seat = setup(&peer).consult(Ok(()));
    assert!(
        block_on(seat.choose_each(&parts()))
            .iter()
            .all(Result::is_err)
    );
    let attempt = seat.receipt().unwrap()["attempts"][0].clone();
    assert_eq!(attempt["outcome"], "http_error");
    assert_eq!(attempt["status"], 200);
    assert_eq!(attempt["usage"]["input_tokens"], 55);
    assert!(
        attempt["error"]
            .as_str()
            .unwrap()
            .contains("lacks its model")
    );
    // The single question's malformed reply keeps its reported usage too.
    let peer = Peer::start(vec![Reply::Status(200, malformed.into())]);
    let seat = setup(&peer).consult(Ok(()));
    assert!(block_on(seat.choose(&part(0))).is_err());
    let attempt = seat.receipt().unwrap()["attempts"][0].clone();
    assert_eq!(attempt["outcome"], "http_error");
    assert_eq!(attempt["usage"]["input_tokens"], 55);
}
