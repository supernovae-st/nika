// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::sync::Mutex;
use std::time::Duration;

use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    StopReason, TokenUsage,
};
use serde_json::{Value, json};

use super::super::{
    ChoiceAnswer, ChoiceFuture, ChoiceOption, ChoiceQuestion, DecisionError, DecisionSeat,
    ProviderChoice,
};
use super::{Bound, ChoiceBatch, WrittenKeys, bind};

/// A reply keyed by id binds each asked id to its one answer, nothing to an id answered twice or
/// not at all, and lists the ids nobody asked once each, in the order first written.
#[test]
fn a_keyed_reply_binds_by_id_and_records_what_nobody_asked() {
    let text = r#"{"z": 1, "a": 2, "b": 3, "a": 4, "y": 5, "z": 6}"#;
    let written: WrittenKeys = serde_json::from_str(text).unwrap();
    assert_eq!(written.keys, ["z", "a", "b", "a", "y", "z"]);
    let reply: Value = serde_json::from_str(text).unwrap();
    let (bound, unasked) = bind(&["a", "b", "c"], reply.as_object().unwrap(), &written);
    assert_eq!(
        bound,
        [Bound::Repeated, Bound::Answer(&json!(3)), Bound::Unanswered]
    );
    assert_eq!(unasked, ["z", "y"]);
    assert!(serde_json::from_str::<WrittenKeys>("[1, 2]").is_err());
}

/// The shared reference, then each question's own words.
const REFERENCE: &str = "REFERENCE: how the compiler writes.\n\nJudge ONE clause.";

/// A part question as the verifier asks it alone: the shared reference and state, its clause.
fn part(k: usize, clause: &str) -> ChoiceQuestion {
    ChoiceQuestion::new(
        format!("verify-part-{k}"),
        format!("{REFERENCE} This clause is part {k}."),
        json!({"request": "the whole request", "clause": {"text": clause}}),
        vec![
            ChoiceOption::new("carried", "done"),
            ChoiceOption::new("missing", "not done"),
        ],
    )
}

/// What the questions share becomes the batch's; what each adds stays its own, by id.
#[test]
fn a_batch_shares_what_its_questions_share_and_keeps_each_ones_own() {
    let questions = [part(0, "read ./a.csv"), part(1, "write ./b.csv")];
    let batch = ChoiceBatch::of("verify-parts", &questions);
    assert_eq!(batch.id, "verify-parts");
    assert_eq!(batch.instructions, "REFERENCE: how the compiler writes.");
    assert_eq!(batch.state, json!({"request": "the whole request"}));
    let asks: Vec<&str> = batch.items.iter().map(|i| i.asks.as_str()).collect();
    assert_eq!(
        asks,
        [
            "Judge ONE clause. This clause is part 0.",
            "Judge ONE clause. This clause is part 1."
        ]
    );
    let adds: Vec<&Value> = batch.items.iter().map(|i| &i.adds).collect();
    assert_eq!(
        adds,
        [
            &json!({"clause": {"text": "read ./a.csv"}}),
            &json!({"clause": {"text": "write ./b.csv"}})
        ]
    );
    // Each item is still the question as asked alone.
    assert_eq!(batch.items[1].question, questions[1]);
}

/// A seat that answers each question by its id, keeping the ids it was asked.
struct Answering(Mutex<Vec<String>>);

impl DecisionSeat for Answering {
    fn name(&self) -> &'static str {
        "test/seat"
    }
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        Box::pin(async move {
            self.0.lock().unwrap().push(question.id.clone());
            if question.id.ends_with('1') {
                return Err(DecisionError("unreachable".into()));
            }
            Ok(ChoiceAnswer::new("carried", "test/seat"))
        })
    }
}

/// By default a seat asks each item alone, every one in flight together: one answer per item in
/// item order, a failure failing only its own item.
#[tokio::test]
async fn a_seat_asks_each_item_alone_and_answers_in_item_order() {
    let questions = [part(0, "a"), part(1, "b"), part(2, "c")];
    let batch = ChoiceBatch::of("verify-parts", &questions);
    let seat = Answering(Mutex::new(Vec::new()));
    let answers = seat.choose_each(&batch).await;
    assert_eq!(answers.len(), 3);
    assert_eq!(
        answers[0].as_ref().map(|a| a.choice.as_str()),
        Ok("carried")
    );
    assert!(answers[1].is_err());
    assert_eq!(
        answers[2].as_ref().map(|a| a.choice.as_str()),
        Ok("carried")
    );
    assert_eq!(seat.0.lock().unwrap().len(), 3, "one request per item");
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

/// A provider seated for decisions settles a batch in ONE request: the shared reference once,
/// an answer schema keyed by item id; each item's key is bound to it by id, an item left
/// without one of its keys fails alone, and the request's usage rides the first answer only.
#[tokio::test]
async fn a_provider_seat_settles_a_batch_in_one_request_bound_by_id() {
    let questions = [part(0, "a"), part(1, "b"), part(2, "c")];
    let batch = ChoiceBatch::of("verify-parts", &questions);
    let provider = Provider {
        text: json!({"verify-part-2": "missing", "verify-part-0": "carried", "verify-part-1": "maybe"})
            .to_string(),
        requests: Mutex::new(Vec::new()),
    };
    let seat = ProviderChoice::new(&provider, "test/model", Duration::from_secs(5), 512);
    let answers = seat.choose_each(&batch).await;
    let requests = provider.requests.lock().unwrap();
    assert_eq!(requests.len(), 1, "one request for the batch");
    let ResponseFormat::JsonSchema(schema) = &requests[0].response_format else {
        panic!("a closed schema");
    };
    let keys: Vec<&String> = schema["properties"].as_object().unwrap().keys().collect();
    assert_eq!(keys, ["verify-part-0", "verify-part-1", "verify-part-2"]);
    let messages = format!("{:?}", requests[0].messages);
    assert_eq!(
        messages
            .matches("REFERENCE: how the compiler writes.")
            .count(),
        1
    );
    drop(requests);
    assert_eq!(
        answers[0].as_ref().map(|a| a.choice.as_str()),
        Ok("carried")
    );
    assert!(
        answers[1].is_err(),
        "an answer outside its keys decides nothing"
    );
    assert_eq!(
        answers[2].as_ref().map(|a| a.choice.as_str()),
        Ok("missing")
    );
    let usage: Vec<Option<u64>> = (answers.iter())
        .map(|a| a.as_ref().ok().and_then(|a| a.input_tokens))
        .collect();
    assert_eq!(usage, [Some(100), None, None]);
}

/// A provider answering every request with `text`.
fn answering(text: &str) -> Provider {
    Provider {
        text: text.to_owned(),
        requests: Mutex::new(Vec::new()),
    }
}

/// A reply that decides no item still accounts for its request: the seat's receipt names the
/// questions the ONE request carried and keeps the usage its response reported, once.
#[tokio::test]
async fn a_reply_that_decides_nothing_still_accounts_for_its_request() {
    let questions = [part(0, "a"), part(1, "b")];
    let batch = ChoiceBatch::of("verify-parts", &questions);
    let provider = answering("not one JSON object");
    let seat = ProviderChoice::new(&provider, "test/model", Duration::from_secs(5), 512);
    let answers = seat.choose_each(&batch).await;
    assert!(answers.iter().all(Result::is_err), "{answers:?}");
    let receipts = seat.requests();
    assert_eq!(receipts.len(), 1, "one physical request");
    assert_eq!(
        receipts[0],
        json!({"questions": ["verify-part-0", "verify-part-1"], "outcome": "answered",
            "usage": {"input_tokens": 100, "output_tokens": 20}})
    );
    // A single question's request is receipted the same way, decided or not.
    assert!(seat.choose(&questions[0]).await.is_err());
    assert_eq!(seat.requests().len(), 2);
    assert_eq!(seat.requests()[1]["questions"], json!(["verify-part-0"]));
}
