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
    Carried, ChoiceAnswer, ChoiceBatch, ChoiceQuestion, DecisionError, NONE_OPTION,
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
        let mut asked: BTreeMap<&str, usize> = BTreeMap::new();
        for item in &batch.items {
            *asked.entry(item.question.id.as_str()).or_default() += 1;
        }
        let once = |id: &str| asked.get(id) == Some(&1);
        let sent: Vec<usize> = (batch.items.iter().enumerate())
            .filter(|(_, item)| once(&item.question.id))
            .map(|(at, _)| at)
            .collect();
        let answers = (batch.items.iter())
            .map(|item| {
                Err(DecisionError(format!(
                    "the batch asks `{}` more than once: an answer keyed by id cannot tell them apart; not sent",
                    item.question.id
                )))
            })
            .collect();
        let outcomes = (batch.items.iter())
            .map(|item| {
                if once(&item.question.id) {
                    "failed"
                } else {
                    "not_sent"
                }
            })
            .collect();
        let error = (sent.is_empty() && !batch.items.is_empty()).then(|| {
            DecisionError("every id of this batch is asked more than once; nothing sent".into())
        });
        let exchange = Self {
            answers,
            outcomes,
            delivery: Delivery::NotSent,
            model: None,
            usage: Usage::default(),
            unasked: Vec::new(),
            error,
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
        let failure = |message: String, usage: Usage| ExchangeError {
            error: DecisionError(message),
            delivery: Delivery::Responded(status),
            usage,
        };
        let parsed = match serde_json::from_slice::<Value>(body) {
            Ok(parsed) => parsed,
            Err(error) => {
                let message = if (200..300).contains(&status) {
                    format!("response is not JSON: {error}")
                } else {
                    format!("typesafe http status {status}")
                };
                return self.failed(sent, failure(message, Usage::default()));
            }
        };
        let usage = Usage::of(&parsed);
        if !(200..300).contains(&status) {
            return self.failed(
                sent,
                failure(format!("typesafe http status {status}"), usage),
            );
        }
        let (
            Some(model),
            Some(answers),
            Ok(Listed {
                answers: Named(named),
            }),
        ) = (
            parsed["model"].as_str(),
            parsed["answers"].as_object(),
            serde_json::from_slice::<Listed>(body),
        )
        else {
            let message = "response lacks its model, or one map of answers by question id";
            return self.failed(sent, failure(message.to_owned(), usage));
        };
        self.model = Some(model.to_owned());
        self.usage = usage;
        let mut carries_usage = true;
        for (at, question) in sent.iter().zip(questions) {
            let times = named.iter().filter(|id| **id == question.id).count();
            let (outcome, answer) = match (times, answers.get(&question.id)) {
                (1, Some(entry)) => answer_to(question, entry, model),
                (0 | 1, _) => (
                    "unanswered",
                    Err("the response carries no answer to this question".to_owned()),
                ),
                _ => (
                    "repeated",
                    Err("the response answers this question more than once".to_owned()),
                ),
            };
            let answer = answer.map(|mut answer| {
                if carries_usage {
                    answer.input_tokens = usage.input_tokens;
                    answer.output_tokens = usage.output_tokens;
                    carries_usage = false;
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
        let asked: Vec<&str> = questions.iter().map(|q| q.id.as_str()).collect();
        for id in named {
            if !asked.contains(&id.as_str()) && !self.unasked.contains(&id) {
                self.unasked.push(id);
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
    answers: Named,
}

/// The keys of one JSON object in the order written, repeats kept (a parsed map keeps one).
struct Named(Vec<String>);

impl<'de> serde::Deserialize<'de> for Named {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(NamedVisitor)
    }
}

struct NamedVisitor;

impl<'de> serde::de::Visitor<'de> for NamedVisitor {
    type Value = Named;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("one JSON object")
    }

    fn visit_map<M: serde::de::MapAccess<'de>>(self, mut map: M) -> Result<Named, M::Error> {
        let mut keys = Vec::new();
        while let Some(key) = map.next_key::<String>()? {
            map.next_value::<serde::de::IgnoredAny>()?;
            keys.push(key);
        }
        Ok(Named(keys))
    }
}

#[cfg(test)]
mod live_tests;
#[cfg(test)]
mod tests;
