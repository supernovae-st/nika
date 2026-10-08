// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The production [`TypesafeSeat::exchange_each`] against the scripted loopback System One peer:
//! the exact number of HTTP requests, the ids, shared state and per-question context each body
//! carries, and how each reply binds to its item. No key, no DNS, no paid endpoint; every script
//! queues a valid answer after the case under test, so a replay would be seen.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::disallowed_methods
)]
use super::super::wire_tests::{KEY, Peer, Reply};
use super::*;
use nika_onboard::compile::decide::{ChoiceOption, DecisionSeat};

const MODEL: &str = "jev-1.13.0";
const REQUEST: &str = "Read ./a.csv and save one row per customer to ./b.json";

fn seat(peer: &Peer) -> TypesafeSeat {
    TypesafeSeat::with_base(KEY.to_owned(), MODEL, &peer.base).expect("seat")
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(future)
}

/// One reference question as the Foundry qualification asks it alone.
fn reference(k: usize, text: &str) -> ChoiceQuestion {
    ChoiceQuestion::new(
        format!("reference-{k}"),
        format!(
            "Decide whether the reference serves the request.\n\nThis question judges reference {k}."
        ),
        json!({"request": REQUEST, "reference": {"id": format!("block:{k}"), "text": text}}),
        vec![
            ChoiceOption::new("applies", "it serves a requirement this request states"),
            ChoiceOption::new("unrelated", "it serves no requirement of this request"),
        ],
    )
}

fn references() -> Vec<ChoiceQuestion> {
    vec![
        reference(0, "read a CSV file"),
        reference(1, "diff two snapshots"),
        reference(2, "write one JSON row per record"),
    ]
}

/// A Choice answer as System One writes it.
fn choice(key: &str) -> String {
    format!(
        r#"{{"type": "choice", "choice": "{key}", "probabilities": {{"{key}": 0.9}}, "confidence": 0.8}}"#
    )
}

/// A response whose `answers` text is written as given (order, repeats and all).
fn reply(answers: &str, usage: &str) -> Reply {
    Reply::Status(
        200,
        format!(r#"{{"model": "{MODEL}", "answers": {{{answers}}}, "usage": {usage}}}"#),
    )
}

const USAGE: &str = r#"{"input_tokens": 300, "output_tokens": 9, "billing_units": 4}"#;

/// The body `exchange` sends for one question alone (the single-question request).
fn alone(question: &ChoiceQuestion) -> Value {
    json!({
        "model": MODEL,
        "state": question.state,
        "questions": {question.id.clone(): {"type": "choice",
            "instructions": question.instructions, "criteria": criteria(question)}},
    })
}

/// Three independent questions ride ONE request: the state they share once, each question its
/// whole words, its own reference and its options; the answers, written in another order, bind
/// by id; the request's usage is counted once, on the exchange and on the first answered item.
#[test]
fn three_independent_questions_ride_one_request_bound_by_id() {
    let answers = format!(
        r#""reference-2": {}, "reference-0": {}, "reference-1": {}"#,
        choice("none"),
        choice("applies"),
        choice("unrelated")
    );
    let peer = Peer::start(vec![reply(&answers, USAGE), reply(&answers, USAGE)]);
    let questions = references();
    let batch = ChoiceBatch::of("foundry-qualification", &questions);
    let exchange = block_on(seat(&peer).exchange_each(&batch));
    assert_eq!(
        peer.heads().len(),
        1,
        "one physical request for three questions"
    );
    let body = &peer.bodies()[0];
    assert_eq!(body["model"], MODEL);
    assert_eq!(
        body["state"],
        json!({"request": REQUEST}),
        "the shared state, once"
    );
    let ids: Vec<&String> = body["questions"].as_object().unwrap().keys().collect();
    assert_eq!(ids, ["reference-0", "reference-1", "reference-2"]);
    for question in &questions {
        let asked = &body["questions"][question.id.as_str()];
        assert_eq!(
            asked["instructions"]["question"],
            question.instructions.as_str()
        );
        assert_eq!(
            asked["instructions"]["own_state"],
            json!({"reference": question.state["reference"]})
        );
        assert_eq!(asked["criteria"], json!(criteria(question)));
        assert_eq!(
            read_state(body, &question.id).as_ref(),
            Some(&question.state)
        );
    }
    let chosen: Vec<Option<&str>> = (exchange.answers.iter())
        .map(|a| a.as_ref().ok().map(|a| a.choice.as_str()))
        .collect();
    assert_eq!(chosen, [Some("applies"), Some("unrelated"), Some("none")]);
    assert_eq!(exchange.outcomes, ["chosen", "chosen", "none"]);
    assert_eq!(exchange.delivery, Delivery::Responded(200));
    assert_eq!(exchange.model.as_deref(), Some(MODEL));
    assert_eq!(
        (
            exchange.usage.input_tokens,
            exchange.usage.output_tokens,
            exchange.usage.billing_units
        ),
        (Some(300), Some(9), Some(4))
    );
    let tokens: Vec<Option<u64>> = (exchange.answers.iter())
        .map(|a| a.as_ref().ok().and_then(|a| a.input_tokens))
        .collect();
    assert_eq!(
        tokens,
        [Some(300), None, None],
        "the usage rides one answer"
    );
    assert!(exchange.unasked.is_empty() && exchange.error.is_none());
    let first = exchange.answers[0].as_ref().unwrap();
    assert_eq!(
        (first.confidence, first.probabilities.get("applies")),
        (Some(0.8), Some(&0.9))
    );
}

/// Every kind of state set a batch may group.
fn corpus() -> Vec<Vec<Value>> {
    vec![
        vec![
            json!({"request": "r", "clause": {"text": "a", "tags": ["x", null]}}),
            json!({"request": "r", "clause": {"text": "b"}}),
        ],
        vec![json!({"request": "r", "k": null}), json!({"request": "r"})],
        vec![json!({"a": {"x": 1}}), json!({"a": {"x": 1, "y": null}})],
        vec![json!("tickets"), json!("tickets")],
        vec![json!("tickets"), json!("orders")],
        vec![json!(["x", {"y": 2}]), json!(["x"])],
        vec![json!(42), json!(42)],
        vec![json!(null), json!({"a": 1})],
        vec![json!({}), json!(null)],
        vec![
            json!({"a": 1}),
            json!("a"),
            json!([1]),
            json!(null),
            json!(0),
        ],
        vec![json!(null), json!(null)],
    ]
}

fn over(states: &[Value]) -> Vec<ChoiceQuestion> {
    (states.iter().enumerate())
        .map(|(k, state)| {
            ChoiceQuestion::new(
                format!("q{k}"),
                format!("Which reading fits item {k}?"),
                state.clone(),
                vec![ChoiceOption::new("first", "the first reading")],
            )
        })
        .collect()
}

/// Whatever the JSON kind of the states (nested objects, arrays, strings, numbers, null, a key
/// missing or null, values that differ per item), each question of the ONE request reads
/// exactly the state, the words and the options it reads alone, and the request state holds
/// nothing one of them does not read.
#[test]
fn each_question_reads_exactly_the_state_it_reads_alone() {
    for states in corpus() {
        let questions = over(&states);
        let asked: Vec<&ChoiceQuestion> = questions.iter().collect();
        let body = request(MODEL, &asked);
        for question in &questions {
            let single = alone(question);
            let alone_asked = &single["questions"][question.id.as_str()];
            let batched = &body["questions"][question.id.as_str()];
            assert_eq!(
                read_state(&body, &question.id).as_ref(),
                Some(&question.state),
                "{states:?}"
            );
            let words = match &batched["instructions"] {
                Value::String(words) => words.as_str(),
                structured => structured["question"].as_str().unwrap(),
            };
            assert_eq!(words, alone_asked["instructions"], "{states:?}");
            assert_eq!(batched["criteria"], alone_asked["criteria"], "{states:?}");
            assert_eq!(batched["type"], "choice");
        }
        let shared = &body["state"];
        if let Some(entries) = shared.as_object() {
            for question in &questions {
                let whole = read_state(&body, &question.id).unwrap();
                let reads_it = |(key, value): (&String, &Value)| whole.get(key) == Some(value);
                let beside =
                    body["questions"][question.id.as_str()]["instructions"]["reads"] == WHOLE;
                assert!(beside || entries.iter().all(reads_it), "{states:?}");
            }
        }
    }
}

/// On the wire, the ONE request is exactly the body written for the batch; a batch of one is
/// byte for byte the request of the question asked alone.
#[test]
fn the_wire_carries_the_written_body_and_a_batch_of_one_is_the_single_request() {
    let peer = Peer::start(vec![
        reply(
            &format!(r#""q0": {}, "q1": {}"#, choice("first"), choice("first")),
            USAGE,
        ),
        reply(&format!(r#""q0": {}"#, choice("first")), USAGE),
        reply(&format!(r#""q0": {}"#, choice("first")), USAGE),
    ]);
    let questions = over(&[json!("tickets"), json!(["orders"])]);
    let asked: Vec<&ChoiceQuestion> = questions.iter().collect();
    let exchange = block_on(seat(&peer).exchange_each(&ChoiceBatch::of("b", &questions)));
    assert!(exchange.answers.iter().all(Result::is_ok), "{exchange:?}");
    assert_eq!(peer.bodies()[0], request(MODEL, &asked));
    assert_eq!(
        peer.bodies()[0]["state"],
        json!({}),
        "nothing shared: every state rides whole"
    );
    let one = ChoiceBatch::of("b", &questions[..1]);
    block_on(seat(&peer).exchange_each(&one));
    block_on(seat(&peer).exchange(&questions[0])).expect("alone");
    let bodies = peer.bodies();
    assert_eq!(bodies.len(), 3);
    assert_eq!(bodies[1], bodies[2], "a batch of one is the single request");
    assert_eq!(bodies[2], alone(&questions[0]));
}

/// An id the response leaves out, answers twice or never asked binds to nothing: the missing
/// and repeated items fail alone, the unasked id is recorded, every valid answer keeps its own
/// item, and the usage rides the first ANSWERED item.
#[test]
fn a_reply_missing_repeating_or_inventing_ids_never_reassigns_an_answer() {
    let answers = format!(
        r#""reference-0": {}, "reference-9": {}, "reference-2": {}, "reference-0": {}"#,
        choice("applies"),
        choice("applies"),
        choice("unrelated"),
        choice("unrelated")
    );
    let peer = Peer::start(vec![reply(&answers, USAGE), reply(&answers, USAGE)]);
    let exchange = block_on(seat(&peer).exchange_each(&ChoiceBatch::of("b", &references())));
    assert_eq!(peer.heads().len(), 1);
    assert_eq!(exchange.outcomes, ["repeated", "unanswered", "chosen"]);
    let errors: Vec<String> = (exchange.answers.iter())
        .filter_map(|a| a.as_ref().err().map(|e| e.0.clone()))
        .collect();
    assert!(errors[0].contains("more than once"), "{errors:?}");
    assert!(errors[1].contains("no answer"), "{errors:?}");
    let kept = exchange.answers[2].as_ref().unwrap();
    assert_eq!(kept.choice, "unrelated");
    assert_eq!(
        kept.input_tokens,
        Some(300),
        "the first answered item carries the usage"
    );
    assert_eq!(exchange.unasked, ["reference-9"]);
    assert_eq!(exchange.delivery, Delivery::Responded(200));
    assert!(exchange.error.is_none(), "the request itself answered");
}

/// An answer outside its question's options, of another type, without a choice, or with a
/// distribution over keys the question never offered fails its item alone.
#[test]
fn an_answer_outside_its_options_or_malformed_fails_only_its_item() {
    let answers = format!(
        r#""reference-0": {}, "reference-1": {{"type": "noul", "noul": 0.9}}, "reference-2": {{"type": "choice", "choice": "applies", "probabilities": {{"applies": 0.5, "lookup": 0.5}}}}"#,
        choice("lookup")
    );
    let peer = Peer::start(vec![reply(&answers, USAGE)]);
    let exchange = block_on(seat(&peer).exchange_each(&ChoiceBatch::of("b", &references())));
    assert_eq!(
        exchange.outcomes,
        ["outside_options", "malformed", "malformed"]
    );
    assert!(exchange.answers.iter().all(Result::is_err));
    let lookup = exchange.answers[0].as_ref().unwrap_err();
    assert!(lookup.0.contains("`lookup`"), "{lookup:?}");
    assert_eq!(
        (exchange.usage.input_tokens, exchange.usage.output_tokens),
        (Some(300), Some(9)),
        "no item answered, the request's usage stays on the exchange"
    );
    let missing_choice = format!(
        r#""reference-0": {{"type": "choice"}}, "reference-1": {}, "reference-2": {}"#,
        choice("applies"),
        choice("applies")
    );
    let peer = Peer::start(vec![reply(&missing_choice, USAGE)]);
    let exchange = block_on(seat(&peer).exchange_each(&ChoiceBatch::of("b", &references())));
    assert_eq!(exchange.outcomes, ["malformed", "chosen", "chosen"]);
    let tokens: Vec<Option<u64>> = (exchange.answers.iter())
        .map(|a| a.as_ref().ok().and_then(|a| a.input_tokens))
        .collect();
    assert_eq!(tokens, [None, Some(300), None]);
}

/// An id the batch asks twice is never sent: the other items ride ONE request that carries only
/// them; when every id is asked twice, nothing leaves.
#[test]
fn an_id_asked_twice_is_never_sent() {
    let peer = Peer::start(vec![reply(
        &format!(r#""reference-2": {}"#, choice("applies")),
        USAGE,
    )]);
    let mut questions = references();
    questions[1].id = "reference-0".to_owned();
    let exchange = block_on(seat(&peer).exchange_each(&ChoiceBatch::of("b", &questions)));
    assert_eq!(peer.heads().len(), 1);
    let bodies = peer.bodies();
    let ids: Vec<&String> = bodies[0]["questions"].as_object().unwrap().keys().collect();
    assert_eq!(ids, ["reference-2"]);
    assert_eq!(exchange.outcomes, ["not_sent", "not_sent", "chosen"]);
    assert!(
        exchange.answers[0]
            .as_ref()
            .unwrap_err()
            .0
            .contains("more than once")
    );
    let twice = ChoiceBatch::of("b", &questions[..2]);
    let exchange = block_on(seat(&peer).exchange_each(&twice));
    assert_eq!(
        peer.heads().len(),
        1,
        "nothing left for a batch of repeated ids"
    );
    assert_eq!(exchange.delivery, Delivery::NotSent);
    assert!(exchange.error.is_some());
}

/// An empty batch asks nothing.
#[test]
fn an_empty_batch_sends_nothing() {
    let peer = Peer::start(vec![reply("", USAGE)]);
    let empty = ChoiceBatch::new("empty", "", json!({}), Vec::new());
    let exchange = block_on(seat(&peer).exchange_each(&empty));
    assert!(exchange.answers.is_empty() && exchange.outcomes.is_empty());
    assert_eq!(
        (exchange.delivery, exchange.error),
        (Delivery::NotSent, None)
    );
    assert!(block_on(seat(&peer).choose_each(&empty)).is_empty());
    assert!(peer.heads().is_empty(), "a request left for an empty batch");
}

/// A refused, dropped or unreadable request answers no item: each fails once with how far the
/// ONE request went, and it is never retried (a valid answer waits behind every case).
#[test]
fn a_failed_request_fails_every_item_once_and_is_never_retried() {
    let valid = format!(
        r#""reference-0": {}, "reference-1": {}, "reference-2": {}"#,
        choice("applies"),
        choice("applies"),
        choice("applies")
    );
    for status in [429_u16, 500, 503, 529] {
        let peer = Peer::start(vec![
            Reply::Status(status, r#"{"error": "retry after a short delay"}"#.into()),
            reply(&valid, USAGE),
        ]);
        let exchange = block_on(seat(&peer).exchange_each(&ChoiceBatch::of("b", &references())));
        assert_eq!(peer.heads().len(), 1, "status {status} was retried");
        assert_eq!(exchange.delivery, Delivery::Responded(status));
        assert_eq!(exchange.outcomes, ["failed", "failed", "failed"]);
        assert!(
            exchange
                .error
                .as_ref()
                .unwrap()
                .0
                .contains(&status.to_string())
        );
    }
    let peer = Peer::start(vec![Reply::Close, reply(&valid, USAGE)]);
    let exchange = block_on(seat(&peer).exchange_each(&ChoiceBatch::of("b", &references())));
    assert_eq!(peer.heads().len(), 1, "the dropped request was replayed");
    assert_eq!(
        exchange.delivery,
        Delivery::Unknown,
        "a sent request may be billed"
    );
    assert!(exchange.answers.iter().all(Result::is_err));
    assert!(
        !exchange.error.as_ref().unwrap().0.contains(KEY),
        "the key leaked"
    );
    for (body, usage) in [
        ("not json".to_owned(), (None, None)),
        (
            format!(r#"{{"answers": {{}}, "usage": {USAGE}}}"#),
            (Some(300), Some(9)),
        ),
        (
            format!(r#"{{"model": "{MODEL}", "answers": [], "usage": {USAGE}}}"#),
            (Some(300), Some(9)),
        ),
        (
            format!(r#"{{"model": "{MODEL}", "answers": {{}}, "answers": {{}}}}"#),
            (None, None),
        ),
    ] {
        let peer = Peer::start(vec![Reply::Status(200, body.clone()), reply(&valid, USAGE)]);
        let exchange = block_on(seat(&peer).exchange_each(&ChoiceBatch::of("b", &references())));
        assert_eq!(peer.heads().len(), 1, "{body} was retried");
        assert_eq!(exchange.delivery, Delivery::Responded(200), "{body}");
        assert_eq!(exchange.outcomes, ["failed", "failed", "failed"], "{body}");
        assert_eq!(
            (exchange.usage.input_tokens, exchange.usage.output_tokens),
            usage,
            "{body}: a reported usage is kept even when nothing is answered"
        );
    }
}

/// The seat's batch door is this exchange: one request, the answers in item order.
#[test]
fn the_seats_batch_door_is_one_request_in_item_order() {
    let answers = format!(
        r#""reference-1": {}, "reference-0": {}, "reference-2": {}"#,
        choice("applies"),
        choice("unrelated"),
        choice("applies")
    );
    let peer = Peer::start(vec![reply(&answers, USAGE), reply(&answers, USAGE)]);
    let seat = seat(&peer);
    let answers = block_on(seat.choose_each(&ChoiceBatch::of("b", &references())));
    let chosen: Vec<&str> = answers
        .iter()
        .map(|a| a.as_ref().unwrap().choice.as_str())
        .collect();
    assert_eq!(chosen, ["unrelated", "applies", "applies"]);
    assert_eq!(peer.heads().len(), 1);
}
