// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Regression witnesses over the batch API as it stood before grouping became lossless (only
//! `ChoiceBatch::of`, `closed_choices` and `ProviderChoice::choose_each`): a string, array or
//! number state reached no request, an empty batch still sent one, an id asked twice shared one
//! answer, a failed first item dropped the request's usage, and a repeated answer key kept its
//! last value silently.

use std::sync::Mutex;
use std::time::Duration;

use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, Message, ProviderError, ProviderInferDyn,
    ResponseFormat, StopReason, TokenUsage,
};
use serde_json::{Value, json};

use super::super::{ChoiceOption, ChoiceQuestion, DecisionSeat, ProviderChoice};
use super::{BatchItem, ChoiceBatch, closed_choices};

/// A question as it is asked alone, over `state`.
fn asked(k: usize, state: Value) -> ChoiceQuestion {
    ChoiceQuestion::new(
        format!("q{k}"),
        format!("REFERENCE: the shared words.\n\nJudge item {k} alone."),
        state,
        vec![
            ChoiceOption::new("fits", "it fits"),
            ChoiceOption::new("misfits", "it does not"),
        ],
    )
}

fn questions(states: &[Value]) -> Vec<ChoiceQuestion> {
    (states.iter().enumerate())
        .map(|(k, state)| asked(k, state.clone()))
        .collect()
}

/// A part question as the verifier asks it alone.
fn part(k: usize, clause: &str) -> ChoiceQuestion {
    ChoiceQuestion::new(
        format!("verify-part-{k}"),
        format!(
            "REFERENCE: how the compiler writes.\n\nJudge ONE clause. This clause is part {k}."
        ),
        json!({"request": "the whole request", "clause": {"text": clause}}),
        vec![
            ChoiceOption::new("carried", "done"),
            ChoiceOption::new("missing", "not done"),
        ],
    )
}

/// The words of a request message.
fn words(message: &Message) -> String {
    (message.content.iter())
        .map(|block| match block {
            ContentBlock::Text { text } => text.clone(),
            _ => String::new(),
        })
        .collect()
}

/// A provider answering every request with `text`, keeping each request it received.
struct Provider {
    text: String,
    requests: Mutex<Vec<InferRequest>>,
}

impl ProviderInferDyn for Provider {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        self.requests.lock().unwrap().push(request);
        Ok(InferResponse::new(
            vec![ContentBlock::Text {
                text: self.text.clone(),
            }],
            TokenUsage::new(100, 20),
            StopReason::EndTurn,
        ))
    }
}

fn answering(text: &str) -> Provider {
    Provider {
        text: text.to_owned(),
        requests: Mutex::new(Vec::new()),
    }
}

/// The item ids a request's answer schema asks for.
fn asked_ids(request: &InferRequest) -> Vec<String> {
    let ResponseFormat::JsonSchema(schema) = &request.response_format else {
        panic!("a closed schema");
    };
    (schema["properties"].as_object().into_iter().flatten())
        .map(|(id, _)| id.clone())
        .collect()
}

/// A provider request shows every item's own state, whatever its kind: a string, an array, a
/// number and a missing-or-null key each reach the request, under their item.
#[test]
fn a_provider_request_carries_every_items_own_state() {
    let states = [
        json!("TICKETS-FILE"),
        json!(["ORDER-A", "ORDER-B"]),
        json!(4217),
    ];
    let (messages, _) = closed_choices(&ChoiceBatch::of("b", &questions(&states)));
    let user = words(&messages[1]);
    for needle in ["TICKETS-FILE", "ORDER-A", "ORDER-B", "4217"] {
        assert!(user.contains(needle), "{needle} was lost: {user}");
    }
    let keyed = [json!({"request": "r", "k": null}), json!({"request": "r"})];
    let (messages, _) = closed_choices(&ChoiceBatch::of("b", &questions(&keyed)));
    let user = words(&messages[1]);
    assert_eq!(user.matches("\"k\": null").count(), 1, "{user}");
    assert_eq!(
        user.matches("\"request\": \"r\"").count(),
        1,
        "shared once: {user}"
    );
}

/// What is shared is what every state holds alike, never more: an entry one state lacks, or
/// holds with another value, stays with the items that hold it; a non-object state is never
/// reduced to nothing.
#[test]
fn the_shared_state_is_what_every_state_holds_alike() {
    let shared = |states: &[Value]| ChoiceBatch::of("b", &questions(states)).state;
    let adds = |states: &[Value]| -> Vec<Value> {
        let batch = ChoiceBatch::of("b", &questions(states));
        batch.items.into_iter().map(|item| item.adds).collect()
    };
    let missing_or_null = [json!({"request": "r", "k": null}), json!({"request": "r"})];
    assert_eq!(shared(&missing_or_null), json!({"request": "r"}));
    assert_eq!(adds(&missing_or_null), [json!({"k": null}), Value::Null]);
    let nested = [json!({"a": {"x": 1}}), json!({"a": {"x": 1, "y": null}})];
    assert_eq!(shared(&nested), json!({}));
    assert_eq!(adds(&nested), nested);
    let strings = [json!("tickets"), json!("tickets")];
    assert_eq!(shared(&strings), json!("tickets"));
    assert_eq!(adds(&strings), [Value::Null, Value::Null]);
    let differing = [json!("tickets"), json!(["orders"]), json!(null)];
    assert_eq!(shared(&differing), Value::Null, "nothing is shared");
    assert_eq!(
        adds(&differing),
        [json!("tickets"), json!(["orders"]), Value::Null]
    );
}

/// A batch built by hand cannot lose what its questions read: the request renders each item from
/// the question it asks alone, never only from the item's own projection.
#[test]
fn a_hand_built_batch_still_renders_each_questions_own_state_and_words() {
    let question = asked(0, json!({"request": "r", "clause": "WRITE-B"}));
    let item = BatchItem::new(question, "", Value::Null);
    let batch = ChoiceBatch::new(
        "b",
        "unrelated shared words",
        json!({"request": "r"}),
        vec![item],
    );
    let (messages, _) = closed_choices(&batch);
    let user = words(&messages[1]);
    assert!(user.contains("WRITE-B"), "{user}");
    assert!(user.contains("Judge item 0 alone."), "{user}");
}

/// An empty batch asks nothing: no request leaves, no answer is invented.
#[tokio::test]
async fn an_empty_batch_sends_no_request() {
    let provider = answering("{}");
    let seat = ProviderChoice::new(&provider, "test/model", Duration::from_secs(5), 512);
    let answers = seat
        .choose_each(&ChoiceBatch::new("empty", "", json!({}), Vec::new()))
        .await;
    assert!(answers.is_empty());
    assert_eq!(provider.requests.lock().unwrap().len(), 0, "a request left");
}

/// An id the batch asks twice is never sent (an answer keyed by id cannot tell its questions
/// apart): both fail, the other items still ride ONE request that carries only them.
#[tokio::test]
async fn an_id_asked_twice_is_never_sent_and_never_shares_an_answer() {
    let questions = [part(0, "a"), part(0, "b"), part(1, "c")];
    let batch = ChoiceBatch::of("verify-parts", &questions);
    let provider =
        answering(&json!({"verify-part-0": "carried", "verify-part-1": "missing"}).to_string());
    let seat = ProviderChoice::new(&provider, "test/model", Duration::from_secs(5), 512);
    let answers = seat.choose_each(&batch).await;
    {
        let requests = provider.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(asked_ids(&requests[0]), ["verify-part-1"]);
    }
    for repeated in &answers[..2] {
        let error = repeated.as_ref().expect_err("never answered");
        assert!(error.0.contains("more than once"), "{error:?}");
    }
    assert_eq!(
        answers[2].as_ref().map(|a| a.choice.as_str()),
        Ok("missing")
    );
    // Every id asked twice: nothing leaves at all.
    let twice = ChoiceBatch::of("verify-parts", &questions[..2]);
    let answers = seat.choose_each(&twice).await;
    assert!(answers.iter().all(Result::is_err));
    assert_eq!(
        provider.requests.lock().unwrap().len(),
        1,
        "a second request left"
    );
}

/// The request's usage rides its first ANSWERED item: an undecided first item cannot drop it,
/// and it is counted once.
#[tokio::test]
async fn the_request_usage_rides_the_first_answered_item_once() {
    let questions = [part(0, "a"), part(1, "b"), part(2, "c")];
    let batch = ChoiceBatch::of("verify-parts", &questions);
    let provider = answering(
        &json!({"verify-part-0": "invented", "verify-part-1": "carried", "verify-part-2": "missing"})
            .to_string(),
    );
    let seat = ProviderChoice::new(&provider, "test/model", Duration::from_secs(5), 512);
    let answers = seat.choose_each(&batch).await;
    assert!(
        answers[0].is_err(),
        "an answer outside its keys decides nothing"
    );
    let usage: Vec<Option<u64>> = (answers.iter())
        .map(|a| a.as_ref().ok().and_then(|a| a.input_tokens))
        .collect();
    assert_eq!(usage, [None, Some(100), None]);
}

/// A repeated key in the answer decides neither of its values (a parsed map would keep the last
/// silently); the other items stay bound by their own ids.
#[tokio::test]
async fn a_repeated_answer_key_decides_nothing_for_that_item() {
    let questions = [part(0, "a"), part(1, "b")];
    let batch = ChoiceBatch::of("verify-parts", &questions);
    let provider = answering(
        r#"{"verify-part-0": "carried", "verify-part-1": "carried", "verify-part-0": "missing"}"#,
    );
    let seat = ProviderChoice::new(&provider, "test/model", Duration::from_secs(5), 512);
    let answers = seat.choose_each(&batch).await;
    assert!(
        answers[0].is_err(),
        "a repeated key decided its item: {:?}",
        answers[0]
    );
    assert_eq!(
        answers[1].as_ref().map(|a| a.choice.as_str()),
        Ok("carried")
    );
    assert_eq!(provider.requests.lock().unwrap().len(), 1);
}
