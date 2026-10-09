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
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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

/// What System One answered a request past its context capacity, as observed on `jev-1.13.0`
/// (a bounded synthetic diagnosis, 2026-10-08, HTTP 400).
const MAX_TOKENS: &str = r#"{"detail": {"error_type": "max_tokens_exceeded"}}"#;

fn over_capacity() -> Reply {
    Reply::Status(400, MAX_TOKENS.into())
}

/// A response answering exactly the parts in `at`, written last first, with `usage`.
fn answering(at: std::ops::Range<usize>, usage: &str) -> Reply {
    let key = |k: usize| {
        if k.is_multiple_of(2) {
            "carried"
        } else {
            "missing"
        }
    };
    let answers: Vec<String> = (at.rev())
        .map(|k| choice(&format!("verify-part-{k}"), key(k)))
        .collect();
    Reply::Status(
        200,
        format!(
            r#"{{"model": "jev-test", "answers": {{{}}}, "usage": {usage}}}"#,
            answers.join(", ")
        ),
    )
}

const FIRST_HALF: &str = r#"{"input_tokens": 30500, "output_tokens": 8100}"#;
const SECOND_HALF: &str = r#"{"input_tokens": 29900, "output_tokens": 7900}"#;

/// Each physical request of a halved batch is ONE journaled attempt, on the journal before it can
/// leave: the refused request (its items halved), then each half naming the attempt it halves,
/// carrying its own items and its own usage once; three calls sent, three charges unknown, and
/// the scope closes clean.
#[test]
fn each_physical_request_of_a_halved_batch_is_journaled_before_it_leaves() {
    let questions: Vec<ChoiceQuestion> = (0..382).map(part).collect();
    let watched: Arc<Mutex<Option<DecisionSetup>>> = Arc::default();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (journal, log) = (Arc::clone(&watched), Arc::clone(&seen));
    let peer = Peer::start_with(
        vec![
            over_capacity(),
            answering(0..191, FIRST_HALF),
            answering(191..382, SECOND_HALF),
            answering(0..382, FIRST_HALF),
        ],
        move || {
            if let Some(setup) = journal.lock().unwrap().as_ref() {
                let observed = &setup.observations()[0];
                let attempts = observed["attempts"].as_array().unwrap();
                let last = attempts.last().unwrap()["outcome"].clone();
                let entry = (attempts.len(), last, observed["calls_sent"].clone());
                log.lock().unwrap().push(entry);
            }
        },
    );
    let setup = setup(&peer);
    *watched.lock().unwrap() = Some(setup.clone());
    let seat = setup.consult(Ok(()));
    let answers = block_on(seat.choose_each(&ChoiceBatch::of("foundry-qualification", &questions)));
    assert!(
        answers.iter().all(Result::is_ok),
        "{:?}",
        answers.iter().find(|a| a.is_err())
    );
    let flight = |n: u64| (usize::try_from(n).unwrap(), json!("in_flight"), json!(n));
    assert_eq!(
        *seen.lock().unwrap(),
        [flight(1), flight(2), flight(3)],
        "each request was journaled before it left"
    );
    let receipt = seat.finish().unwrap();
    assert_eq!(receipt["calls_sent"], 3, "{receipt}");
    assert_eq!(receipt["unknown_calls"], 3);
    assert_eq!(receipt["state"], "Closed");
    let attempts = receipt["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 3, "one attempt per physical request");
    let refused = &attempts[0];
    assert_eq!(refused["batch"], "foundry-qualification");
    assert_eq!(refused["sent"], true);
    assert_eq!(refused["outcome"], "http_error");
    assert_eq!(refused["status"], 400);
    assert_eq!(refused["refusal"], "capacity");
    assert_eq!(refused["halved"], true);
    let items = refused["items"].as_array().unwrap();
    assert_eq!(items.len(), 382);
    assert!(items.iter().all(|item| item["outcome"] == "halved"));
    for (attempt, (at, tokens)) in attempts[1..]
        .iter()
        .zip([(0..191, 30500), (191..382, 29900)])
    {
        assert_eq!(attempt["halves"], 0, "{attempt}");
        assert_eq!(attempt["outcome"], "answered");
        assert_eq!(attempt["usage"]["input_tokens"], tokens);
        let carried: Vec<&str> = (attempt["items"].as_array().unwrap().iter())
            .map(|item| item["question"].as_str().unwrap())
            .collect();
        let expected: Vec<String> = at.map(|k| format!("verify-part-{k}")).collect();
        assert_eq!(carried, expected, "a half's items moved");
    }
}

/// A halved batch dropped while its second half is out keeps what was sent: the refused request
/// and the first half settled, the second half in flight and nothing after it; the scope ends
/// Uncertain with every sent call counted.
#[test]
fn a_halved_batch_dropped_during_a_half_keeps_its_sent_requests() {
    let arrivals = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&arrivals);
    let peer = Peer::start_with(
        vec![
            over_capacity(),
            answering(0..1, FIRST_HALF),
            answering(1..3, SECOND_HALF),
            answering(0..3, FIRST_HALF),
        ],
        move || {
            counted.fetch_add(1, Ordering::SeqCst);
        },
    );
    let setup = setup(&peer);
    let seat = setup.consult(Ok(()));
    let batch = parts();
    let third = std::future::poll_fn(|cx| {
        if arrivals.load(Ordering::SeqCst) >= 3 {
            std::task::Poll::Ready(())
        } else {
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    });
    block_on(async {
        tokio::select! {
            biased;
            () = third => {}
            _ = seat.choose_each(&batch) => panic!("the batch settled before cancellation"),
        }
    });
    drop(seat);
    assert_eq!(peer.heads().len(), 3);
    let kept = &setup.observations()[0];
    let attempts = kept["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 3, "{kept}");
    assert_eq!(attempts[0]["halved"], true);
    assert_eq!(attempts[1]["outcome"], "answered");
    assert_eq!(attempts[2]["outcome"], "in_flight");
    assert_eq!(kept["state"], "Uncertain");
    assert_eq!(kept["scope_ended"], true);
    assert_eq!(
        (&kept["calls_sent"], &kept["unknown_calls"]),
        (&json!(3), &json!(3))
    );
}

/// Stop requested while the refused request is out withholds its halves: ONE call sent, the
/// refused attempt settled, then one attempt never sent naming every withheld item as stopped.
#[test]
fn stop_after_a_capacity_refusal_journals_the_withheld_items_unsent() {
    use nika_providers::authoring::preparation::PreparationCosts;
    let mut costs = PreparationCosts::default();
    let raised = costs.begin_turn();
    let _scope = costs.enter();
    let peer = Peer::start_with(
        vec![over_capacity(), answering(0..3, FIRST_HALF)],
        move || raised.cancel(),
    );
    let seat = setup(&peer).consult(Ok(()));
    let answers = block_on(seat.choose_each(&parts()));
    assert!(
        answers
            .iter()
            .all(|a| a.as_ref().is_err_and(|e| e.0.contains("stopped")))
    );
    assert_eq!(peer.heads().len(), 1, "a half left after Stop");
    let receipt = seat.finish().unwrap();
    assert_eq!(receipt["calls_sent"], 1);
    let attempts = receipt["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 2, "{receipt}");
    assert_eq!(attempts[0]["refusal"], "capacity");
    assert_eq!(attempts[1]["sent"], false);
    assert_eq!(attempts[1]["outcome"], "stopped");
    let withheld: Vec<&str> = (attempts[1]["items"].as_array().unwrap().iter())
        .map(|item| item["outcome"].as_str().unwrap())
        .collect();
    assert_eq!(withheld, ["stopped", "stopped", "stopped"]);
    assert_eq!(receipt["state"], "Closed");
}

/// Every request that left carries the time its transport took, in whole milliseconds (measured,
/// so only its presence and type are judged): each physical request of a halved batch, and a
/// single question's. A request never sent carries none.
#[test]
fn each_request_that_left_carries_its_transport_time() {
    let peer = Peer::start(vec![
        over_capacity(),
        answering(0..1, FIRST_HALF),
        answering(1..3, SECOND_HALF),
    ]);
    let journal = setup(&peer);
    let seat = journal.consult(Ok(()));
    let answers = block_on(seat.choose_each(&parts()));
    assert!(answers.iter().all(Result::is_ok), "{answers:?}");
    let alone = Peer::start(vec![reply(&choice("verify-part-0", "carried"))]);
    let single = setup(&alone).consult(Ok(()));
    assert!(block_on(single.choose(&part(0))).is_ok());
    let refused = setup(&alone).consult(Err("the allowance is spent".into()));
    assert!(block_on(refused.choose(&part(1))).is_err());
    let receipt = seat.finish().unwrap();
    let mut left = receipt["attempts"].as_array().unwrap().clone();
    assert_eq!(left.len(), 3, "the refused request and its two halves");
    left.push(single.finish().unwrap()["attempts"][0].clone());
    for attempt in &left {
        assert_eq!(attempt["sent"], true, "{attempt}");
        assert!(attempt["elapsed_ms"].is_u64(), "{attempt}");
    }
    let unsent = refused.finish().unwrap()["attempts"][0].clone();
    assert_eq!(unsent["sent"], false);
    assert!(unsent.get("elapsed_ms").is_none(), "{unsent}");
}

/// The seat a Session keeps learns its capacity across compiles. A qualification of eight is
/// refused whole, then in its first half of four, and answered in parts of two, its second four
/// split before it left. The next compile's batch of eight to the same seat starts under what
/// was learned: four requests of two, none refused, each naming the bound it was split under.
#[test]
fn a_later_compile_starts_under_the_capacity_the_seat_learned() {
    let eight: Vec<ChoiceQuestion> = (0..8).map(part).collect();
    let pairs = |k: usize| answering(k..k + 2, FIRST_HALF);
    let mut script = vec![over_capacity(), over_capacity()];
    script.extend([0, 2, 4, 6].map(pairs));
    script.extend([0, 2, 4, 6].map(pairs));
    let peer = Peer::start(script);
    let journal = setup(&peer);
    for _ in 0..2 {
        let seat = journal.consult(Ok(()));
        let batch = ChoiceBatch::of("foundry-qualification", &eight);
        let answers = block_on(seat.choose_each(&batch));
        assert!(answers.iter().all(Result::is_ok), "{answers:?}");
        assert!(seat.finish().is_some());
    }
    let asked: Vec<usize> = (peer.bodies().iter())
        .map(|body| body["questions"].as_object().unwrap().len())
        .collect();
    assert_eq!(
        asked,
        [8, 4, 2, 2, 2, 2, 2, 2, 2, 2],
        "two refusals, then parts of two"
    );
    let observations = journal.observations();
    let first = observations[0]["attempts"].as_array().unwrap();
    let split: Vec<&Value> = first
        .iter()
        .map(|attempt| &attempt["split_below"])
        .collect();
    let (none, four) = (&Value::Null, &json!(4));
    assert_eq!(split, [none, none, none, none, four, four]);
    let later = observations[1]["attempts"].as_array().unwrap();
    assert_eq!(later.len(), 4, "{later:?}");
    for attempt in later {
        let seen = (&attempt["outcome"], &attempt["split_below"]);
        assert_eq!(seen, (&json!("answered"), &json!(3)), "{attempt}");
        assert!(attempt.get("refusal").is_none(), "{attempt}");
    }
}
