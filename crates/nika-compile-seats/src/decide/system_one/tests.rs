// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The System One refusal classification and the partition law, without any transport: the
//! bodies are the ones `jev-1.13.0` answered a bounded synthetic diagnosis with (2026-10-08:
//! many questions past the capacity, one question alone past it, and one malformed question).

use serde_json::{Value, json};

use super::super::{ChoiceBatch, ChoiceOption, ChoiceQuestion};
use super::{Partition, Refusal, read, refusal, request};

/// The refusal of a request past the context capacity, for many questions or one alone.
const MAX_TOKENS: &str = r#"{"detail": {"error_type": "max_tokens_exceeded"}}"#;

/// The refusal of a question type the service does not define: also HTTP 400.
const INVALID: &str =
    r#"{"detail": {"error_type": "api_usage_error", "message": "Invalid request."}}"#;

fn question(k: usize) -> ChoiceQuestion {
    ChoiceQuestion::new(
        format!("item-{k}"),
        "Decide whether the reference serves the request.",
        json!({"request": "r", "reference": {"id": format!("ref:{k}"), "text": format!("text {k}")}}),
        vec![ChoiceOption::new("applies", "it serves it")],
    )
}

fn batch(n: usize) -> ChoiceBatch {
    ChoiceBatch::of("b", &(0..n).map(question).collect::<Vec<_>>())
}

/// A 200 answering every item of `at` with `applies`, reporting `tokens` input tokens.
fn answering(batch: &ChoiceBatch, at: &[usize], tokens: u64) -> Vec<u8> {
    let answers: serde_json::Map<String, Value> = (at.iter())
        .map(|k| {
            let id = batch.items[*k].question.id.clone();
            (id, json!({"type": "choice", "choice": "applies"}))
        })
        .collect();
    let body = json!({"model": "jev-1.13.0", "answers": answers,
        "usage": {"input_tokens": tokens, "output_tokens": 1}});
    serde_json::to_vec(&body).unwrap_or_default()
}

/// Only the closed `detail.error_type` of a 4xx says capacity or invalid; every other refusal is
/// its status alone, whatever words it holds.
#[test]
fn a_refusal_is_read_by_its_closed_reason_only() {
    let capacity = refusal(400, MAX_TOKENS.as_bytes());
    assert_eq!(capacity, Some(Refusal::Capacity { status: 400 }));
    assert_eq!(capacity.map(|r| r.word()), Some("capacity"));
    assert_eq!(
        refusal(400, INVALID.as_bytes()),
        Some(Refusal::Invalid { status: 400 })
    );
    for (status, body) in [
        (400_u16, "{}"),
        (400, "not json"),
        (400, "max_tokens_exceeded"),
        (400, r#"{"detail": "max_tokens_exceeded"}"#),
        (400, r#"{"error_type": "max_tokens_exceeded"}"#),
        (400, r#"{"detail": {"error_type": "MAX_TOKENS_EXCEEDED"}}"#),
        (400, r#"{"detail": {"message": "max_tokens_exceeded"}}"#),
        (500, MAX_TOKENS),
        (529, MAX_TOKENS),
        (401, r#"{"detail": "invalid key"}"#),
        (429, "{}"),
    ] {
        assert_eq!(
            refusal(status, body.as_bytes()),
            Some(Refusal::Status(status)),
            "{body}"
        );
    }
    let named =
        r#"{"detail": {"error_type": "api_usage_error", "message": "max_tokens_exceeded"}}"#;
    assert_eq!(
        refusal(400, named.as_bytes()),
        Some(Refusal::Invalid { status: 400 })
    );
    assert_eq!(
        refusal(200, MAX_TOKENS.as_bytes()),
        None,
        "a success is no refusal"
    );
}

/// A refusal's words are the engine's own: its status and closed code, never the body's text.
#[test]
fn a_refusal_never_repeats_the_bodys_words() {
    let echoed = r#"{"detail": {"error_type": "max_tokens_exceeded", "message": "Bearer secret-sentinel in the reflected state"}}"#;
    let said = refusal(400, echoed.as_bytes())
        .map(|r| r.to_string())
        .unwrap_or_default();
    assert_eq!(
        said,
        "typesafe http status 400: max_tokens_exceeded, the request reads more tokens than the model's context holds"
    );
    let invalid = refusal(400, INVALID.as_bytes())
        .map(|r| r.to_string())
        .unwrap_or_default();
    assert!(!invalid.contains("Invalid request."), "{invalid}");
    assert_eq!(
        refusal(418, b"teapot secret-sentinel").map(|r| r.to_string()),
        Some("typesafe http status 418".to_owned())
    );
    let questions = [question(0)];
    let (usage, reading) = read(&[&questions[0]], 400, echoed.as_bytes());
    assert_eq!(reading, Err(said), "read names the same refusal");
    assert!(!usage.reported());
}

/// A refusal for capacity of several items puts its two halves next, first half first, each the
/// body written for its own questions; a refused single item is over capacity and nothing waits.
#[test]
fn only_a_capacity_refusal_halves_and_a_refused_single_item_ends_over_capacity() {
    let batch = batch(5);
    let mut partition = Partition::of(&batch);
    let (first, body) = partition.begin(&batch, "jev").unwrap_or_default();
    let all: Vec<&ChoiceQuestion> = batch.items.iter().map(|i| &i.question).collect();
    assert_eq!((first, body), (0, request("jev", &all)));
    partition.responded(first, &batch, 400, MAX_TOKENS.as_bytes());
    assert!(partition.attempts[0].halved);
    let (second, body) = partition.begin(&batch, "jev").unwrap_or_default();
    assert_eq!(body, request("jev", &all[..2]), "the first half first");
    assert_eq!(partition.attempts[second].halves, Some(0));
    partition.responded(second, &batch, 200, &answering(&batch, &[0, 1], 7));
    let (third, body) = partition.begin(&batch, "jev").unwrap_or_default();
    assert_eq!(body, request("jev", &all[2..]));
    partition.responded(third, &batch, 400, MAX_TOKENS.as_bytes());
    let (fourth, _) = partition.begin(&batch, "jev").unwrap_or_default();
    assert_eq!(partition.attempts[fourth].items, [2]);
    partition.responded(fourth, &batch, 400, MAX_TOKENS.as_bytes());
    let (fifth, _) = partition.begin(&batch, "jev").unwrap_or_default();
    assert_eq!(partition.attempts[fifth].items, [3, 4]);
    partition.responded(fifth, &batch, 400, INVALID.as_bytes());
    assert!(!partition.waiting(), "an invalid request is never halved");
    assert_eq!(
        partition.outcomes,
        ["chosen", "chosen", "over_capacity", "failed", "failed"]
    );
    let alone = partition.answers[2]
        .as_ref()
        .err()
        .map(|e| e.0.clone())
        .unwrap_or_default();
    assert!(
        alone.contains("max_tokens_exceeded") && alone.contains("asked alone"),
        "{alone}"
    );
    assert_eq!(
        partition.error().map(|e| e.0),
        Some(alone),
        "the first final failure"
    );
}

/// A batch whose every request is refused for capacity ends after exactly 2n - 1 attempts, every
/// item over capacity: halves are strictly smaller, so no recursion or quota is needed.
#[test]
fn a_batch_refused_at_every_size_ends_after_2n_minus_1_attempts() {
    for n in [1_usize, 2, 3, 7, 16, 382] {
        let batch = batch(n);
        let mut partition = Partition::of(&batch);
        while let Some((at, _)) = partition.begin(&batch, "jev") {
            partition.responded(at, &batch, 400, MAX_TOKENS.as_bytes());
        }
        assert_eq!(partition.attempts.len(), 2 * n - 1, "{n}");
        assert!(
            partition.outcomes.iter().all(|o| *o == "over_capacity"),
            "{n}"
        );
        let singles = partition
            .attempts
            .iter()
            .filter(|a| a.items.len() == 1)
            .count();
        assert_eq!(singles, n, "each item asked alone exactly once");
    }
}

/// An id asked twice never rides any request, whatever half it would fall in; a batch whose
/// every id is repeated waits for nothing and says why.
#[test]
fn an_id_asked_twice_never_waits_for_any_request() {
    let mut questions: Vec<ChoiceQuestion> = (0..4).map(question).collect();
    questions[3].id = questions[0].id.clone();
    let batch = ChoiceBatch::of("b", &questions);
    let mut partition = Partition::of(&batch);
    let (at, _) = partition.begin(&batch, "jev").unwrap_or_default();
    assert_eq!(partition.attempts[at].items, [1, 2]);
    assert_eq!(partition.outcomes[0], "not_sent");
    let record = partition.unsent(&batch, "not_sent").unwrap_or_default();
    assert_eq!(record["items"].as_array().map(Vec::len), Some(2));
    assert_eq!(record["sent"], false);
    let twice = ChoiceBatch::of("t", &[question(0), question(0)]);
    let nothing = Partition::of(&twice);
    assert!(!nothing.waiting());
    assert!(
        nothing
            .error()
            .is_some_and(|e| e.0.contains("nothing sent"))
    );
}

/// The usage of a batch is each physical request's once: a count is known only when every
/// request that may have left reported it, never a sum that hides an unknown.
#[test]
fn usage_is_known_only_when_every_sent_request_reported_it() {
    let batch = batch(4);
    let mut partition = Partition::of(&batch);
    let (at, _) = partition.begin(&batch, "jev").unwrap_or_default();
    partition.responded(at, &batch, 400, MAX_TOKENS.as_bytes());
    for half in [[0, 1], [2, 3]] {
        let (at, _) = partition.begin(&batch, "jev").unwrap_or_default();
        partition.responded(at, &batch, 200, &answering(&batch, &half, 100));
    }
    assert_eq!(partition.attempts.len(), 3);
    assert_eq!(
        partition.usage().input_tokens,
        None,
        "the refusal reported none"
    );
    let tokens: Vec<Option<u64>> = (partition.attempts.iter())
        .map(|a| a.usage.input_tokens)
        .collect();
    assert_eq!(
        tokens,
        [None, Some(100), Some(100)],
        "each request's own, once"
    );
    let mut answered = Partition::of(&batch);
    let (at, _) = answered.begin(&batch, "jev").unwrap_or_default();
    answered.responded(at, &batch, 200, &answering(&batch, &[0, 1, 2, 3], 300));
    assert_eq!(answered.usage().input_tokens, Some(300));
    let mut malformed = Partition::of(&batch);
    let (at, _) = malformed.begin(&batch, "jev").unwrap_or_default();
    let reported = br#"{"answers": {}, "usage": {"input_tokens": 55}}"#;
    malformed.responded(at, &batch, 200, reported);
    assert_eq!(
        malformed.outcomes, ["failed"; 4],
        "a malformed 200 answers none"
    );
    assert_eq!(
        malformed.attempts[0].usage.input_tokens,
        Some(55),
        "its usage is kept"
    );
    assert!(!malformed.waiting(), "and it is never resent");
    let mut kept = Partition::of(&batch);
    let (at, _) = kept.begin(&batch, "jev").unwrap_or_default();
    kept.lost(
        at,
        false,
        super::DecisionError("journal unavailable".to_owned()),
    );
    assert_eq!(
        kept.usage().input_tokens,
        None,
        "nothing left, nothing reported"
    );
    assert_eq!(kept.outcomes, ["not_sent"; 4]);
}

/// Withheld items never leave: each fails under the word it was withheld with, and the reason is
/// the partition's once nothing else failed.
#[test]
fn withheld_items_never_leave_and_say_why() {
    let batch = batch(3);
    let mut partition = Partition::of(&batch);
    let (at, _) = partition.begin(&batch, "jev").unwrap_or_default();
    partition.responded(at, &batch, 400, MAX_TOKENS.as_bytes());
    let stopped = super::DecisionError("not sent: stopped".to_owned());
    partition.withhold("stopped", &stopped);
    assert!(!partition.waiting());
    assert!(partition.begin(&batch, "jev").is_none());
    assert_eq!(partition.outcomes, ["stopped"; 3]);
    assert_eq!(partition.error(), Some(stopped));
    let record = partition.unsent(&batch, "stopped").unwrap_or_default();
    assert_eq!(record["outcome"], "stopped");
    assert_eq!(record["items"].as_array().map(Vec::len), Some(3));
}

/// A journal record names its items by id with their options and outcomes, never their words or
/// state; in flight before its response, then settled; a half names the slot of the attempt it
/// halves; its body's exact size and digest are what left.
#[test]
fn a_record_names_each_physical_request_without_its_text() {
    let batch = batch(2);
    let mut partition = Partition::of(&batch);
    let (at, body) = partition.begin(&batch, "jev").unwrap_or_default();
    let flying = partition.record(at, &batch, &[]);
    assert_eq!(flying["outcome"], "in_flight");
    assert_eq!(flying["sent"], true);
    assert_eq!(
        flying["items"][0],
        json!({"question": "item-0",
        "options": ["applies", "none"], "outcome": "in_flight"})
    );
    let text = serde_json::to_string(&body).unwrap_or_default();
    assert_eq!(flying["body"]["bytes"], text.len());
    assert_eq!(
        flying["body"]["sha256"],
        nika_compile::surface::sha256(&text)
    );
    assert!(
        !flying.to_string().contains("text 0"),
        "a state reached the record"
    );
    partition.responded(at, &batch, 400, MAX_TOKENS.as_bytes());
    let refused = partition.record(at, &batch, &[]);
    assert_eq!(
        (
            &refused["outcome"],
            &refused["status"],
            &refused["refusal"],
            &refused["halved"]
        ),
        (
            &json!("http_error"),
            &json!(400),
            &json!("capacity"),
            &json!(true)
        )
    );
    assert_eq!(refused["items"][1]["outcome"], "halved");
    let (half, _) = partition.begin(&batch, "jev").unwrap_or_default();
    partition.responded(half, &batch, 200, &answering(&batch, &[0], 9));
    let settled = partition.record(half, &batch, &[41, 42]);
    assert_eq!(settled["halves"], 41, "the slot of the attempt it halves");
    assert_eq!(settled["outcome"], "answered");
    assert_eq!(settled["usage"]["input_tokens"], 9);
    assert_eq!(settled["items"][0]["choice"], "applies");
    assert!(settled.get("refusal").is_none() && settled.get("halved").is_none());
}
