// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The System One wire form of closed choices (the `TypeSafe` decision service,
//! `POST /v1/systemone`), without any transport: the body of ONE request for one or several
//! questions and the reading of its reply. A host that seats the service sends these bytes
//! through its own single-attempt client and keeps its key; nothing here opens a connection,
//! holds a credential or decides a retry.
//!
//! The questions map carries every question under its id, which the service never shows to the
//! model. The state the questions read alike rides once as the request state; what a question
//! reads beyond it rides in its own structured instructions (`own_state`), so each question reads
//! exactly the state it reads alone. Replies bind by id ([`bind`]); the usage is counted once per
//! request and rides only its first answered question.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use super::{
    Bound, Carried, ChoiceAnswer, ChoiceQuestion, DecisionError, NONE_OPTION, WrittenKeys, bind,
};

/// How a question whose state extends the request state reads it.
const BESIDE: &str = "This question reads the request state together with `own_state`: the entries of its own state that no other question of this request shares.";

/// How a question whose state is not an extension of the request state reads it.
const WHOLE: &str =
    "This question reads `own_state` as its whole state, in place of the request state.";

/// What one request's response reported it used, counted once per physical request, never per
/// question: `None` when it did not say.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Usage {
    /// Reported input tokens.
    pub input_tokens: Option<u64>,
    /// Reported output tokens.
    pub output_tokens: Option<u64>,
    /// The service's reported billing units — never relabelled as input tokens.
    pub billing_units: Option<u64>,
}

impl Usage {
    /// The usage a parsed System One response reports.
    #[must_use]
    pub fn of(parsed: &Value) -> Self {
        let count = |name: &str| parsed["usage"].get(name).and_then(Value::as_u64);
        Self {
            input_tokens: count("input_tokens"),
            output_tokens: count("output_tokens"),
            billing_units: count("billing_units"),
        }
    }

    /// Whether the response reported any of it.
    #[must_use]
    pub fn reported(&self) -> bool {
        self.input_tokens.is_some() || self.output_tokens.is_some() || self.billing_units.is_some()
    }

    /// Its journal projection.
    #[must_use]
    pub fn record(&self) -> Value {
        json!({
            "input_tokens": self.input_tokens,
            "output_tokens": self.output_tokens,
            "billing_units": self.billing_units,
        })
    }
}

/// The body of ONE request for `questions` (their ids distinct): the state they read alike,
/// once; each question with its whole instructions and options, and what it reads beyond the
/// request state in its own structured instructions. A single question's body carries its own
/// state and words unchanged.
#[must_use]
pub fn request(model: &str, questions: &[&ChoiceQuestion]) -> Value {
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

/// The Choice criteria of `question`: each offered key and what choosing it means.
#[must_use]
pub fn criteria(question: &ChoiceQuestion) -> Map<String, Value> {
    (question.options.iter())
        .map(|o| (o.key.clone(), Value::String(o.description.clone())))
        .collect()
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
    }
}

/// The state one question of a request body reads: the request state with what its structured
/// instructions carry beside it (what [`request`] writes, for a witness to compare with the
/// state the question reads alone).
#[must_use]
pub fn read_state(body: &Value, id: &str) -> Option<Value> {
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

/// The answer to `id` in one parsed System One response, as a single question reads it, with
/// the billing units it reported; the compiler revalidates the choice against its options.
///
/// # Errors
/// The response names no model or no answer to `id`, or the answer is not a choice.
pub fn answer(parsed: &Value, id: &str) -> Result<(ChoiceAnswer, Option<u64>), String> {
    let model = (parsed.get("model").and_then(Value::as_str)).ok_or("response lacks model")?;
    let answer =
        (parsed.get("answers").and_then(|a| a.get(id))).ok_or("response lacks the answer")?;
    if answer.get("type").and_then(Value::as_str) != Some("choice") {
        return Err("answer is not a choice".to_owned());
    }
    let choice = (answer.get("choice").and_then(Value::as_str)).ok_or("answer lacks choice")?;
    let mut result = ChoiceAnswer::new(choice, model);
    if let Some(map) = answer.get("probabilities").and_then(Value::as_object) {
        for (key, value) in map {
            if let Some(p) = unit(Some(value)) {
                result.probabilities.insert(key.clone(), p);
            }
        }
    }
    result.confidence = unit(answer.get("confidence"));
    // Billing units are the service's own accounting, not tokens: never relabelled as input.
    let usage = Usage::of(parsed);
    result.input_tokens = usage.input_tokens;
    result.output_tokens = usage.output_tokens;
    Ok((result, usage.billing_units))
}

/// What one System One response says about the questions it was asked.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Reply {
    /// The model the response named.
    pub model: String,
    /// Each asked question's outcome (`chosen` · `none` · `outside_options` · `unanswered` ·
    /// `repeated` · `malformed`) and answer or why it has none, in asked order.
    pub items: Vec<(&'static str, Result<ChoiceAnswer, DecisionError>)>,
    /// Ids the response answered that no asked question carries: recorded, never assigned.
    pub unasked: Vec<String>,
}

/// Read one response (`status`, `body`) to `questions`: the usage it reported, once and whatever
/// it answered, and its reply bound by id, or why it answers no question. The usage also rides
/// the first answered question, so summing answers counts it once.
pub fn read(
    questions: &[&ChoiceQuestion],
    status: u16,
    body: &[u8],
) -> (Usage, Result<Reply, String>) {
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
        Err(message) => return (usage, Err(message)),
    };
    let ids: Vec<&str> = questions.iter().map(|q| q.id.as_str()).collect();
    let (bound, unasked) = bind(&ids, answers, written);
    let mut carries_usage = true;
    let items = (questions.iter().zip(bound))
        .map(|(question, bound)| {
            let (outcome, answer) = match bound {
                Bound::Answer(entry) => answer_to(question, entry, model),
                Bound::Repeated => ("repeated", Err("answered more than once".to_owned())),
                Bound::Unanswered => (
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
            (outcome, answer.map_err(DecisionError))
        })
        .collect();
    let reply = Reply {
        model: model.to_owned(),
        items,
        unasked,
    };
    (usage, Ok(reply))
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

/// A reported probability or concentration, kept only within `0..=1`.
fn unit(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|p| (0.0..=1.0).contains(p))
}

/// A response's top-level shape as written: its `answers` keys, repeats kept.
#[derive(serde::Deserialize)]
struct Listed {
    answers: WrittenKeys,
}
