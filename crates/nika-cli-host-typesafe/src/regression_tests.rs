// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Regression witnesses over the seat API as it stood before the System One batch transport
//! (only `DecisionSeat::choose_each` and the Session's journaling seat): independent questions
//! put to a `TypeSafe` seat together left as one request per question, and the Session journal
//! counted one call per question. The loopback peer answers every request with all the answers,
//! so each request it receives is visible and none fails.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::disallowed_methods
)]
use super::session::DecisionSetup;
use super::wire_tests::{KEY, Peer, Reply};
use super::*;
use nika_onboard::compile::decide::{ChoiceBatch, ChoiceOption, ChoiceQuestion, DecisionSeat};
use serde_json::json;

fn block_on<F: Future>(future: F) -> F::Output {
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

/// Every part's answer, written in another order than asked.
fn every_answer() -> Reply {
    let answer = |key: &str| json!({"type": "choice", "choice": key, "confidence": 0.7});
    let body = format!(
        r#"{{"model": "jev-test", "answers": {{"verify-part-2": {}, "verify-part-0": {}, "verify-part-1": {}}}, "usage": {{"input_tokens": 90, "output_tokens": 3}}}}"#,
        answer("missing"),
        answer("carried"),
        answer("carried")
    );
    Reply::Status(200, body)
}

/// Three independent questions put to a System One seat together ride ONE request, each answer
/// bound to its question by id.
#[test]
fn three_independent_questions_through_a_system_one_seat_ride_one_request() {
    let peer = Peer::start(vec![every_answer(), every_answer(), every_answer()]);
    let seat = TypesafeSeat::with_base(KEY.to_owned(), "jev-test", &peer.base).expect("seat");
    let answers = block_on(seat.choose_each(&parts()));
    let chosen: Vec<Option<&str>> = (answers.iter())
        .map(|a| a.as_ref().ok().map(|a| a.choice.as_str()))
        .collect();
    assert_eq!(chosen, [Some("carried"), Some("carried"), Some("missing")]);
    assert_eq!(
        peer.heads().len(),
        1,
        "one physical request for three questions"
    );
}

/// The Session journals a batch as ONE call sent, one attempt: never one per question.
#[test]
fn a_session_batch_is_one_journaled_call() {
    let peer = Peer::start(vec![every_answer(), every_answer(), every_answer()]);
    let setup =
        DecisionSetup::with_key("typesafe/jev-test", Some(KEY.to_owned()), Some(&peer.base));
    let seat = setup.consult(Ok(()));
    let answers = block_on(seat.choose_each(&parts()));
    assert!(answers.iter().all(Result::is_ok), "{answers:?}");
    let receipt = seat.finish().expect("the seat was needed");
    assert_eq!(peer.heads().len(), 1, "one physical request");
    assert_eq!(receipt["calls_sent"], 1, "{receipt}");
    assert_eq!(receipt["unknown_calls"], 1, "{receipt}");
    assert_eq!(
        receipt["attempts"].as_array().unwrap().len(),
        1,
        "{receipt}"
    );
}
