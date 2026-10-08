// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Several independent closed choices in ONE System One request (A1). The questions map carries
//! every item under its own id; the service scores each question on its own and never shows an
//! id to the model, so nothing about an item rides in its id. The state the items read alike
//! rides once as the request state; what an item reads beyond it rides in its own structured
//! instructions (`own_state`, named by its words), so each question reads exactly the state it
//! reads alone, whatever the JSON kind of that state. A question keeps its whole instructions
//! and options.
//!
//! The answers come back under the same ids and are bound by id, never by position: an id the
//! response leaves out, answers twice or answers outside its options fails that item alone; an
//! id no item asked is recorded and never assigned; an id the batch asks twice is never sent.
//! One request is one accounting event: its usage is counted once on the exchange, apart from
//! the questions, and rides only the first answered item so that summing answers counts it once.

use std::collections::BTreeMap;
use std::{future::Future, pin::Pin};

use nika_onboard::compile::decide::{
    Bound, Carried, ChoiceAnswer, ChoiceBatch, ChoiceQuestion, DecisionError, NONE_OPTION,
    WrittenKeys, bind,
};
use serde_json::{Map, Value, json};

use super::{Delivery, ExchangeError, TypesafeSeat, Usage, criteria, unit};

/// How a question whose state extends the request state reads it.
const BESIDE: &str = "This question reads the request state together with `own_state`: the entries of its own state that no other question of this request shares.";

/// How a question whose state is not an extension of the request state reads it.
const WHOLE: &str =
    "This question reads `own_state` as its whole state, in place of the request state.";

/// One System One request for a batch: each item's answer or why it has none, and what the wire
/// reported, once, for the request.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct BatchExchange {
    /// Each item's answer, or why it has none, in item order.
    pub answers: Vec<Result<ChoiceAnswer, DecisionError>>,
    /// Each item's outcome, in item order: `chosen` · `none` · `outside_options` ·
    /// `unanswered` · `repeated` · `malformed` · `not_sent` (its id asked twice) · `failed`
    /// (the request answered no item).
    pub outcomes: Vec<&'static str>,
    /// How far the ONE request went; `NotSent` when no item could be sent.
    pub delivery: Delivery,
    /// The model the response named.
    pub model: Option<String>,
    /// What the response reported it used, once for the request, whatever its items came to.
    pub usage: Usage,
    /// Ids the response answered that no sent item asked: recorded, never assigned.
    pub unasked: Vec<String>,
    /// Why the request as a whole answered no item, when it did not.
    pub error: Option<DecisionError>,
}

/// The object-safe future of one batch exchange.
pub type BatchExchangeFuture<'a> = Pin<Box<dyn Future<Output = BatchExchange> + Send + 'a>>;

impl TypesafeSeat {
    /// Exactly one physical request for every item of `batch` whose id it asks once, with what
    /// the wire reported: no retry, no fallback, no second request. Nothing is sent for an empty
    /// batch, nor when every id is asked twice.
    #[must_use]
    pub fn exchange_each<'a>(&'a self, batch: &'a ChoiceBatch) -> BatchExchangeFuture<'a> {
        Box::pin(async move {
            let (mut exchange, sent) = BatchExchange::before(batch);
            if sent.is_empty() {
                return exchange;
            }
            let questions: Vec<&ChoiceQuestion> = (sent.iter())
                .filter_map(|at| batch.items.get(*at).map(|item| &item.question))
                .collect();
            match self.post(&request(&self.model, &questions)).await {
                Err(failure) => exchange.failed(&sent, failure),
                Ok(response) => {
                    exchange.delivery = Delivery::Responded(response.status);
                    exchange.read(&sent, &questions, response.status, &response.body);
                }
            }
            exchange
        })
    }
}

impl BatchExchange {
    /// The exchange before its request, and the items it may send (their ids asked once): an
    /// item whose id the batch asks more than once is never sent.
    fn before(batch: &ChoiceBatch) -> (Self, Vec<usize>) {
        let repeated = batch.repeated();
        let sent: Vec<usize> = (batch.items.iter().enumerate())
            .filter(|(_, item)| !repeated.contains(item.question.id.as_str()))
            .map(|(at, _)| at)
            .collect();
        let unsent = |id: &str| {
            let why = "more than once: an answer keyed by id cannot tell them apart; not sent";
            Err(DecisionError(format!("the batch asks `{id}` {why}")))
        };
        let exchange = Self {
            answers: (batch.items.iter())
                .map(|item| unsent(&item.question.id))
                .collect(),
            outcomes: vec!["not_sent"; batch.items.len()],
            delivery: Delivery::NotSent,
            model: None,
            usage: Usage::default(),
            unasked: Vec::new(),
            error: (sent.is_empty() && !batch.items.is_empty()).then(|| {
                DecisionError("every id of this batch is asked more than once; nothing sent".into())
            }),
        };
        (exchange, sent)
    }

    /// The request answered no item: every sent item fails with its error.
    fn failed(&mut self, sent: &[usize], failure: ExchangeError) {
        for at in sent {
            if let (Some(answer), Some(outcome)) =
                (self.answers.get_mut(*at), self.outcomes.get_mut(*at))
            {
                *answer = Err(failure.error.clone());
                *outcome = "failed";
            }
        }
        self.delivery = failure.delivery;
        self.usage = failure.usage;
        self.error = Some(failure.error);
    }

    /// Read the response to the sent items: each answer bound by id and validated against what
    /// its question offered; the usage once, whatever the items came to.
    fn read(&mut self, sent: &[usize], questions: &[&ChoiceQuestion], status: u16, body: &[u8]) {
        let parsed = serde_json::from_slice::<Value>(body);
        let usage = parsed.as_ref().map_or_else(|_| Usage::default(), Usage::of);
        let written = serde_json::from_slice::<Listed>(body).map(|listed| listed.answers);
        let read = match (&parsed, (200..300).contains(&status)) {
            (_, false) => Err(format!("typesafe http status {status}")),
            (Err(error), true) => Err(format!("response is not JSON: {error}")),
            (Ok(parsed), true) => {
                match (
                    parsed["model"].as_str(),
                    parsed["answers"].as_object(),
                    &written,
                ) {
                    (Some(model), Some(answers), Ok(written)) => Ok((model, answers, written)),
                    _ => Err("response lacks its model, or one map of answers by id".to_owned()),
                }
            }
        };
        let (model, answers, written) = match read {
            Ok(read) => read,
            Err(message) => {
                let error = DecisionError(message);
                let delivery = Delivery::Responded(status);
                return self.failed(
                    sent,
                    ExchangeError {
                        error,
                        delivery,
                        usage,
                    },
                );
            }
        };
        self.model = Some(model.to_owned());
        self.usage = usage;
        let ids: Vec<&str> = questions.iter().map(|q| q.id.as_str()).collect();
        let (bound, unasked) = bind(&ids, answers, written);
        self.unasked = unasked;
        let mut carries_usage = true;
        for ((at, question), bound) in sent.iter().zip(questions).zip(bound) {
            let (outcome, answer) = match bound {
                Bound::Answer(entry) => answer_to(question, entry, model),
                Bound::Repeated => ("repeated", Err("answered more than once".to_owned())),
                _ => (
                    "unanswered",
                    Err("the response carries no answer to it".to_owned()),
                ),
            };
            let answer = answer.map(|mut answer| {
                if std::mem::take(&mut carries_usage) {
                    answer.input_tokens = usage.input_tokens;
                    answer.output_tokens = usage.output_tokens;
                }
                answer
            });
            if let (Some(slot), Some(word)) =
                (self.answers.get_mut(*at), self.outcomes.get_mut(*at))
            {
                *slot = answer.map_err(DecisionError);
                *word = outcome;
            }
        }
    }
}

/// The answer `entry` gives `question`, validated against what the question offered, with its
/// outcome word; or the outcome and why it answers nothing.
fn answer_to(
    question: &ChoiceQuestion,
    entry: &Value,
    model: &str,
) -> (&'static str, Result<ChoiceAnswer, String>) {
    let malformed = |why: String| ("malformed", Err(why));
    if entry.get("type").and_then(Value::as_str) != Some("choice") {
        return malformed("the answer is not a choice".to_owned());
    }
    let Some(choice) = entry.get("choice").and_then(Value::as_str) else {
        return malformed("the answer lacks its choice".to_owned());
    };
    let offered = question.keys();
    if !offered.iter().any(|key| key == choice) {
        let why = format!("the seat chose `{choice}`, which this question did not offer");
        return ("outside_options", Err(why));
    }
    let mut probabilities = BTreeMap::new();
    if let Some(reported) = entry.get("probabilities") {
        let Some(reported) = reported.as_object() else {
            return malformed("the answer's probabilities are not one map".to_owned());
        };
        for (key, value) in reported {
            if !offered.contains(key) {
                return malformed(format!(
                    "the answer reports a probability for `{key}`, which this question did not offer"
                ));
            }
            if let Some(p) = unit(Some(value)) {
                probabilities.insert(key.clone(), p);
            }
        }
    }
    let mut answer = ChoiceAnswer::new(choice, model);
    answer.probabilities = probabilities;
    answer.confidence = unit(entry.get("confidence"));
    let outcome = if choice == NONE_OPTION {
        "none"
    } else {
        "chosen"
    };
    (outcome, Ok(answer))
}

/// The ONE request body for `questions` (their ids distinct): the state they read alike, once;
/// each question with its whole instructions and options, and what it reads beyond the request
/// state in its own structured instructions.
fn request(model: &str, questions: &[&ChoiceQuestion]) -> Value {
    let states: Vec<&Value> = questions.iter().map(|q| &q.state).collect();
    let shared = request_state(&states);
    let asked: Map<String, Value> = (questions.iter())
        .map(|q| {
            let question = json!({"type": "choice", "instructions": instructions(&shared, q),
                "criteria": criteria(q)});
            (q.id.clone(), question)
        })
        .collect();
    json!({"model": model, "state": shared, "questions": asked})
}

/// The state the request carries once: what every question reads alike ([`Carried::common`]),
/// or `{}` when they read nothing alike and are not all `null` (every question reads the request
/// state, so it never holds what one of them does not read).
fn request_state(states: &[&Value]) -> Value {
    let common = Carried::common(states);
    if common.is_null() && states.iter().any(|state| !state.is_null()) {
        Value::Object(Map::new())
    } else {
        common
    }
}

/// A question's instructions in the request: its own words alone when the request state is its
/// whole state; else its words, the state it reads beyond the request state and how to read it.
fn instructions(shared: &Value, question: &ChoiceQuestion) -> Value {
    let structured = |own: Value, reads: &str| json!({"question": question.instructions, "own_state": own, "reads": reads});
    match Carried::of(shared, &question.state) {
        Carried::Shared => Value::String(question.instructions.clone()),
        Carried::Beside(own) => structured(Value::Object(own), BESIDE),
        Carried::Whole(own) => structured(own, WHOLE),
        _ => structured(question.state.clone(), WHOLE),
    }
}

/// The state one question of a request body reads: the request state with what its structured
/// instructions carry beside it (the reading [`request`] writes, for a witness to compare with
/// the state the question reads alone).
#[cfg(test)]
pub(super) fn read_state(body: &Value, id: &str) -> Option<Value> {
    let shared = &body["state"];
    let instructions = &body["questions"][id]["instructions"];
    match (instructions, instructions["reads"].as_str()) {
        (Value::String(_), _) => Some(shared.clone()),
        (_, Some(BESIDE)) => {
            let mut whole = shared.as_object()?.clone();
            whole.extend(instructions["own_state"].as_object()?.clone());
            Some(Value::Object(whole))
        }
        (_, Some(WHOLE)) => Some(instructions["own_state"].clone()),
        _ => None,
    }
}

/// The response's top-level shape as written: its `answers` keys, repeats kept.
#[derive(serde::Deserialize)]
struct Listed {
    answers: WrittenKeys,
}

#[cfg(test)]
mod live_tests;
#[cfg(test)]
mod tests;
