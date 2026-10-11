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
use nika_onboard::compile::decide::system_one::{criteria, read_state, request};
use nika_onboard::compile::decide::{Carried, ChoiceOption, ChoiceQuestion, DecisionSeat};
use serde_json::{Value, json};

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
                let whole = matches!(Carried::of(shared, &question.state), Carried::Whole(_));
                assert!(whole || entries.iter().all(reads_it), "{states:?}");
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

/// What System One answered a request past its context capacity, as observed on `jev-1.13.0`
/// (a bounded synthetic diagnosis, 2026-10-08, HTTP 400): its one capacity signal.
const MAX_TOKENS: &str = r#"{"detail": {"error_type": "max_tokens_exceeded"}}"#;

/// What it answered a question of a type it does not define in the same diagnosis: also 400.
const INVALID: &str =
    r#"{"detail": {"error_type": "api_usage_error", "message": "Invalid request."}}"#;

const FIRST_HALF: &str = r#"{"input_tokens": 30500, "output_tokens": 8100}"#;
const SECOND_HALF: &str = r#"{"input_tokens": 29900, "output_tokens": 7900}"#;

fn over_capacity() -> Reply {
    Reply::Status(400, MAX_TOKENS.to_owned())
}

/// The key item `k` is answered with: one of its options, by position.
fn key(k: usize) -> &'static str {
    ["applies", "unrelated", "none"][k % 3]
}

/// A response answering exactly the items at `at`, written last first, with `usage`.
fn answering(
    questions: &[ChoiceQuestion],
    at: impl IntoIterator<Item = usize>,
    usage: &str,
) -> Reply {
    let mut at: Vec<usize> = at.into_iter().collect();
    at.reverse();
    let answers: Vec<String> = (at.iter())
        .map(|k| format!(r#""{}": {}"#, questions[*k].id, choice(key(*k))))
        .collect();
    reply(&answers.join(", "), usage)
}

/// The ids one request body asks, sorted.
fn asked(body: &Value) -> Vec<String> {
    let mut ids: Vec<String> = body["questions"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    ids.sort();
    ids
}

fn ids(questions: &[ChoiceQuestion]) -> Vec<String> {
    let mut ids: Vec<String> = questions.iter().map(|q| q.id.clone()).collect();
    ids.sort();
    ids
}

/// Every question `body` asks reads exactly the state, the words and the options it reads alone.
fn carried_alone(body: &Value, questions: &[ChoiceQuestion]) {
    for id in asked(body) {
        let question = questions.iter().find(|q| q.id == id).unwrap();
        assert_eq!(
            read_state(body, &id).as_ref(),
            Some(&question.state),
            "{id}"
        );
        let batched = &body["questions"][id.as_str()];
        let words = match &batched["instructions"] {
            Value::String(words) => words.as_str(),
            structured => structured["question"].as_str().unwrap(),
        };
        assert_eq!(words, question.instructions, "{id}");
        assert_eq!(batched["criteria"], json!(criteria(question)), "{id}");
    }
}

fn qualification(n: usize) -> Vec<ChoiceQuestion> {
    (0..n)
        .map(|k| reference(k, &format!("reference text {k}")))
        .collect()
}

/// A 382-item qualification the service refuses as past its context capacity is asked again in
/// its two halves, in item order: every original id exactly once across the halves, each
/// question as it is asked alone, each answer bound to its own item, each half's usage counted
/// once, and no request beyond the three (a resend would take the spare answer). The seat's model
/// documents no capacity, so the batch leaves whole; a documented one is planned before it leaves
/// (`a_batch_past_the_documented_capacity_leaves_in_requests_that_fit_never_whole`).
#[test]
fn a_capacity_refusal_asks_a_382_item_batch_again_in_ordered_halves() {
    let questions = qualification(382);
    let peer = Peer::start(vec![
        over_capacity(),
        answering(&questions, 0..191, FIRST_HALF),
        answering(&questions, 191..382, SECOND_HALF),
        answering(&questions, 0..382, USAGE),
    ]);
    let batch = ChoiceBatch::of("foundry-qualification", &questions);
    let seat = TypesafeSeat::with_base(KEY.to_owned(), "jev-undocumented", &peer.base);
    let exchange = block_on(seat.expect("seat").exchange_each(&batch));
    let bodies = peer.bodies();
    assert_eq!(bodies.len(), 3, "the refused request and its two halves");
    assert_eq!(asked(&bodies[0]), ids(&questions));
    assert_eq!(asked(&bodies[1]), ids(&questions[..191]));
    assert_eq!(asked(&bodies[2]), ids(&questions[191..]));
    carried_alone(&bodies[1], &questions);
    carried_alone(&bodies[2], &questions);
    let chosen: Vec<&str> = (exchange.answers.iter())
        .map(|a| a.as_ref().unwrap().choice.as_str())
        .collect();
    assert_eq!(
        chosen,
        (0..382).map(key).collect::<Vec<_>>(),
        "an answer moved"
    );
    assert!(
        (exchange.outcomes.iter()).all(|o| *o == "chosen" || *o == "none"),
        "{:?}",
        exchange.outcomes
    );
    let carried: Vec<(usize, u64)> = (exchange.answers.iter().enumerate())
        .filter_map(|(k, a)| a.as_ref().ok().and_then(|a| a.input_tokens).map(|t| (k, t)))
        .collect();
    assert_eq!(
        carried,
        [(0, 30500), (191, 29900)],
        "each request's usage rides its first answered item, once"
    );
    assert_eq!(
        (exchange.usage.input_tokens, exchange.usage.output_tokens),
        (None, None),
        "the refused request reported no usage: the total stays unknown, never a partial sum"
    );
    assert_eq!(exchange.delivery, Delivery::Responded(200));
    assert!(exchange.error.is_none() && exchange.unasked.is_empty());
}

/// Only the service's capacity code halves a request: a bare 400, an invalid request (also 400),
/// the capacity word in another shape or field, the capacity code under another status, an auth
/// or rate refusal are each ONE request whose items all fail with its status, never resent.
#[test]
fn only_the_capacity_code_ever_halves_a_request() {
    let questions = references();
    for (status, body) in [
        (400_u16, "{}"),
        (400, INVALID),
        (400, r#"{"detail": "max_tokens_exceeded"}"#),
        (400, r#"{"error_type": "max_tokens_exceeded"}"#),
        (
            400,
            r#"{"detail": {"error_type": "api_usage_error", "message": "max_tokens_exceeded"}}"#,
        ),
        (400, "max_tokens_exceeded"),
        (500, MAX_TOKENS),
        (401, r#"{"detail": "invalid key"}"#),
        (429, r#"{"detail": "rate limited"}"#),
    ] {
        let peer = Peer::start(vec![
            Reply::Status(status, body.to_owned()),
            answering(&questions, 0..3, USAGE),
            answering(&questions, 0..3, USAGE),
        ]);
        let exchange = block_on(seat(&peer).exchange_each(&ChoiceBatch::of("b", &questions)));
        assert_eq!(peer.heads().len(), 1, "{status} {body}: halved or resent");
        assert_eq!(exchange.outcomes, ["failed", "failed", "failed"], "{body}");
        assert_eq!(exchange.delivery, Delivery::Responded(status));
        let error = exchange.error.as_ref().unwrap();
        assert!(error.0.contains(&status.to_string()), "{error:?}");
    }
}

/// A single question the service refuses for capacity is unresolved: explicitly over capacity,
/// sent whole as it is asked alone, never truncated, halved or resent.
#[test]
fn a_single_question_refused_for_capacity_is_explicitly_over_capacity() {
    let questions = references();
    let peer = Peer::start(vec![over_capacity(), answering(&questions, 0..1, USAGE)]);
    let exchange = block_on(seat(&peer).exchange_each(&ChoiceBatch::of("b", &questions[..1])));
    assert_eq!(peer.heads().len(), 1, "the refused question was resent");
    assert_eq!(peer.bodies()[0], alone(&questions[0]), "sent whole");
    assert_eq!(exchange.outcomes, ["over_capacity"]);
    let error = exchange.answers[0].as_ref().unwrap_err();
    assert!(error.0.contains("max_tokens_exceeded"), "{error:?}");
    assert_eq!(exchange.error.as_ref(), Some(error));
}

/// One question too large alone amid three that fit: halving isolates it, the three are answered
/// and it alone ends over capacity. Fewer siblings never claim to cure it.
#[test]
fn one_oversized_question_amid_small_ones_ends_alone_over_capacity() {
    let questions = qualification(4);
    let peer = Peer::start(vec![
        over_capacity(),
        answering(&questions, 0..2, FIRST_HALF),
        over_capacity(),
        over_capacity(),
        answering(&questions, 3..4, SECOND_HALF),
        answering(&questions, 0..4, USAGE),
    ]);
    let exchange = block_on(seat(&peer).exchange_each(&ChoiceBatch::of("b", &questions)));
    let sent: Vec<Vec<String>> = peer.bodies().iter().map(asked).collect();
    let expected = [
        ids(&questions),
        ids(&questions[..2]),
        ids(&questions[2..]),
        ids(&questions[2..3]),
        ids(&questions[3..]),
    ];
    assert_eq!(sent, expected, "the halving order");
    assert_eq!(
        exchange.outcomes,
        ["chosen", "chosen", "over_capacity", "chosen"]
    );
    let chosen: Vec<Option<&str>> = (exchange.answers.iter())
        .map(|a| a.as_ref().ok().map(|a| a.choice.as_str()))
        .collect();
    assert_eq!(
        chosen,
        [Some("applies"), Some("unrelated"), None, Some("applies")]
    );
    let tokens: Vec<Option<u64>> = (exchange.answers.iter())
        .map(|a| a.as_ref().ok().and_then(|a| a.input_tokens))
        .collect();
    assert_eq!(
        tokens,
        [Some(30500), None, None, Some(29900)],
        "each answered request's usage once, on its first answered item"
    );
    assert_eq!(
        exchange.usage.input_tokens, None,
        "three refusals reported none"
    );
}

/// An id asked twice is refused before any subdivision: two items that would fall in different
/// halves never become sendable, and the unique items are halved around them.
#[test]
fn an_id_asked_twice_stays_unsent_across_halves() {
    let mut questions = qualification(6);
    questions[4].id = questions[1].id.clone();
    let peer = Peer::start(vec![
        over_capacity(),
        answering(&questions, [0, 2], FIRST_HALF),
        answering(&questions, [3, 5], SECOND_HALF),
        answering(&questions, [0, 2, 3, 5], USAGE),
    ]);
    let exchange = block_on(seat(&peer).exchange_each(&ChoiceBatch::of("b", &questions)));
    let sent: Vec<Vec<String>> = peer.bodies().iter().map(asked).collect();
    let unique = |at: &[usize]| ids(&at.iter().map(|k| questions[*k].clone()).collect::<Vec<_>>());
    assert_eq!(
        sent,
        [unique(&[0, 2, 3, 5]), unique(&[0, 2]), unique(&[3, 5])]
    );
    assert_eq!(
        exchange.outcomes,
        ["chosen", "not_sent", "none", "chosen", "not_sent", "none"]
    );
    assert!(
        exchange.answers[4]
            .as_ref()
            .unwrap_err()
            .0
            .contains("more than once")
    );
}

/// A half's partial, repeated, unasked or lost answers fail only its own items and are never
/// resent: a missing id is unanswered, a repeated one repeated, an id no request asked is kept as
/// unasked, a malformed 200 fails its own half alone, and a half left without a response is
/// unknown.
#[test]
fn a_partial_or_failed_reply_to_a_half_fails_only_its_items_and_is_never_resent() {
    let questions = qualification(6);
    let partial = reply(
        &format!(
            r#""reference-2": {}, "reference-99": {}, "reference-0": {}, "reference-2": {}"#,
            choice("applies"),
            choice("applies"),
            choice("applies"),
            choice("unrelated")
        ),
        FIRST_HALF,
    );
    let peer = Peer::start(vec![
        over_capacity(),
        partial,
        Reply::Close,
        answering(&questions, 3..6, USAGE),
    ]);
    let exchange = block_on(seat(&peer).exchange_each(&ChoiceBatch::of("b", &questions)));
    assert_eq!(peer.heads().len(), 3, "a failed half was resent");
    assert_eq!(
        exchange.outcomes,
        [
            "chosen",
            "unanswered",
            "repeated",
            "failed",
            "failed",
            "failed"
        ]
    );
    assert_eq!(exchange.unasked, ["reference-99"]);
    assert_eq!(
        exchange.delivery,
        Delivery::Unknown,
        "a sent half may be billed"
    );
    let malformed = format!(r#"{{"answers": {{}}, "usage": {SECOND_HALF}}}"#);
    let peer = Peer::start(vec![
        over_capacity(),
        answering(&questions, 0..3, FIRST_HALF),
        Reply::Status(200, malformed),
        answering(&questions, 3..6, USAGE),
    ]);
    let exchange = block_on(seat(&peer).exchange_each(&ChoiceBatch::of("b", &questions)));
    assert_eq!(peer.heads().len(), 3, "a malformed half was resent");
    assert_eq!(&exchange.outcomes[..3], ["chosen", "chosen", "none"]);
    assert_eq!(&exchange.outcomes[3..], ["failed", "failed", "failed"]);
}

/// Stop withholds every later request of a batch: requested while the refused request is out,
/// no half leaves and every item is withheld as stopped; requested before the batch, nothing
/// leaves at all.
#[test]
fn stop_withholds_every_later_request_of_a_batch() {
    use nika_providers::authoring::preparation::PreparationCosts;
    let questions = references();
    let mut costs = PreparationCosts::default();
    let raised = costs.begin_turn();
    let _scope = costs.enter();
    let peer = Peer::start_with(
        vec![
            over_capacity(),
            answering(&questions, 0..1, USAGE),
            answering(&questions, 1..3, USAGE),
        ],
        move || raised.cancel(),
    );
    let exchange = block_on(seat(&peer).exchange_each(&ChoiceBatch::of("b", &questions)));
    assert_eq!(peer.heads().len(), 1, "a half left after Stop");
    assert_eq!(exchange.outcomes, ["stopped", "stopped", "stopped"]);
    assert!((exchange.answers.iter()).all(|a| a.as_ref().is_err_and(|e| e.0.contains("stopped"))));
    let peer = Peer::start(vec![answering(&questions, 0..3, USAGE)]);
    let exchange = block_on(seat(&peer).exchange_each(&ChoiceBatch::of("b", &questions)));
    assert!(peer.heads().is_empty(), "a request left after Stop");
    assert_eq!(exchange.outcomes, ["stopped", "stopped", "stopped"]);
    assert_eq!(exchange.delivery, Delivery::NotSent);
}

/// An ordinary 12-item batch within capacity is still ONE request, exactly the body written for
/// the batch, its usage on its first answered item only.
#[test]
fn an_ordinary_12_item_batch_is_still_one_request() {
    let questions = qualification(12);
    let peer = Peer::start(vec![
        answering(&questions, 0..12, USAGE),
        answering(&questions, 0..12, USAGE),
    ]);
    let exchange =
        block_on(seat(&peer).exchange_each(&ChoiceBatch::of("verify-parts", &questions)));
    assert_eq!(peer.heads().len(), 1);
    let all: Vec<&ChoiceQuestion> = questions.iter().collect();
    assert_eq!(peer.bodies()[0], request(MODEL, &all));
    let tokens: Vec<Option<u64>> = (exchange.answers.iter())
        .map(|a| a.as_ref().ok().and_then(|a| a.input_tokens))
        .collect();
    assert_eq!(tokens[0], Some(300));
    assert_eq!(tokens.iter().flatten().count(), 1, "usage counted once");
    assert_eq!(exchange.delivery, Delivery::Responded(200));
}

/// Whatever the JSON kind of the states (nested, arrays, scalars, null, a key missing or null),
/// each half is exactly the request written for its questions, every one read as it is asked
/// alone, and the halves ask every id once.
#[test]
fn every_half_carries_each_question_exactly_as_it_is_asked_alone() {
    for states in corpus() {
        let questions = over(&states);
        let half = questions.len() / 2;
        let answer = |at: std::ops::Range<usize>| {
            let answers: Vec<String> = at
                .map(|k| format!(r#""q{k}": {}"#, choice("first")))
                .collect();
            reply(&answers.join(", "), USAGE)
        };
        let peer = Peer::start(vec![
            over_capacity(),
            answer(0..half),
            answer(half..questions.len()),
            answer(0..questions.len()),
        ]);
        let exchange = block_on(seat(&peer).exchange_each(&ChoiceBatch::of("b", &questions)));
        let bodies = peer.bodies();
        assert_eq!(bodies.len(), 3, "{states:?}");
        for (body, part) in bodies[1..]
            .iter()
            .zip([&questions[..half], &questions[half..]])
        {
            assert_eq!(asked(body), ids(part), "{states:?}");
            assert_eq!(
                *body,
                request(MODEL, &part.iter().collect::<Vec<_>>()),
                "{states:?}"
            );
            carried_alone(body, &questions);
        }
        assert!(exchange.answers.iter().all(Result::is_ok), "{states:?}");
    }
}

/// A seat of `jev-1.13.0` starts a batch past the capacity its model documents in the requests
/// that capacity admits, never whole: the requests the partition law plans for the seat's
/// capacity, each carrying exactly its items, every answer bound to its own item, the usage of
/// each request counted once.
#[test]
fn a_batch_past_the_documented_capacity_leaves_in_requests_that_fit_never_whole() {
    let text = "x".repeat(15_000);
    let questions: Vec<ChoiceQuestion> = (0..36).map(|k| reference(k, &text)).collect();
    let batch = ChoiceBatch::of("foundry-qualification", &questions);
    let mut law = system_one::Capacity::of_model(MODEL).partition(&batch);
    let (mut planned, mut script) = (Vec::new(), Vec::new());
    while let Some((at, _)) = law.begin(&batch, MODEL) {
        let mut asked_ids: Vec<String> = (law.attempts[at].items.iter())
            .map(|k| batch.items[*k].question.id.clone())
            .collect();
        asked_ids.sort();
        let answers: Vec<String> = (asked_ids.iter())
            .map(|id| format!(r#""{id}": {}"#, choice("applies")))
            .collect();
        let body = format!(
            r#"{{"model": "{MODEL}", "answers": {{{}}}, "usage": {USAGE}}}"#,
            answers.join(", ")
        );
        law.responded(at, &batch, 200, body.as_bytes());
        script.push(Reply::Status(200, body));
        planned.push(asked_ids);
    }
    assert!(planned.len() > 1, "the fixture exceeds one request");
    let peer = Peer::start(script);
    let exchange = block_on(seat(&peer).exchange_each(&batch));
    let sent: Vec<Vec<String>> = peer.bodies().iter().map(asked).collect();
    assert_eq!(
        sent, planned,
        "the planned requests, in order, never the whole batch"
    );
    assert!(
        exchange.outcomes.iter().all(|o| *o == "chosen"),
        "{:?}",
        exchange.outcomes
    );
    let per_request = 300 * u64::try_from(planned.len()).expect("a small count");
    assert_eq!(exchange.usage.input_tokens, Some(per_request));
}
