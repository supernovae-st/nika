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
//!
//! A refusal is read by the closed reason its body names, never by its words ([`refusal`]):
//! `max_tokens_exceeded` is the service's one capacity signal, and the same HTTP 400 also
//! carries an invalid request (`api_usage_error`), so a status alone never says capacity.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};

use nika_compile::surface::sha256;
use serde_json::{Map, Value, json};

use super::batch::unsent;
use super::{
    Bound, Carried, ChoiceAnswer, ChoiceBatch, ChoiceQuestion, DecisionError, NONE_OPTION,
    WrittenKeys, bind,
};

/// How a question whose state extends the request state reads it.
const BESIDE: &str = "This question reads the request state together with `own_state`: the entries of its own state that no other question of this request shares.";

/// How a question whose state is not an extension of the request state reads it.
const WHOLE: &str =
    "This question reads `own_state` as its whole state, in place of the request state.";

/// The `detail.error_type` of a request the service refuses as more tokens than the model's
/// context holds (HTTP 400 on `jev-1.13.0`, no numbers: the documented capacities are 64k tokens
/// per request and 32k for the state with the longest question, and the code names neither).
const MAX_TOKENS: &str = "max_tokens_exceeded";

/// The `detail.error_type` of a request the service refuses as invalid (HTTP 400 on
/// `jev-1.13.0`, where the API reference documents 422 for a body that fails validation).
const INVALID: &str = "api_usage_error";

/// Why one non-success System One response answered no question, as far as its body names a
/// closed reason: never its words, which may reflect the request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Refusal {
    /// More tokens than the model's context holds (`max_tokens_exceeded`): the only refusal that
    /// the same questions asked in smaller requests may avoid. It does not say which documented
    /// capacity was exceeded, so it never says a smaller request will fit.
    Capacity {
        /// The response status.
        status: u16,
    },
    /// An invalid request (`api_usage_error`): asking it again, whole or in parts, cannot cure it.
    Invalid {
        /// The response status.
        status: u16,
    },
    /// Any other non-success response, whatever its body: its status alone.
    Status(u16),
}

impl Refusal {
    /// The status of the response.
    #[must_use]
    pub const fn status(&self) -> u16 {
        match self {
            Self::Capacity { status } | Self::Invalid { status } | Self::Status(status) => *status,
        }
    }

    /// The journal's word: `capacity` · `invalid` · `status`.
    #[must_use]
    pub const fn word(&self) -> &'static str {
        match self {
            Self::Capacity { .. } => "capacity",
            Self::Invalid { .. } => "invalid",
            Self::Status(_) => "status",
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = self.status();
        match self {
            Self::Capacity { .. } => write!(
                f,
                "typesafe http status {status}: {MAX_TOKENS}, the request reads more tokens than the model's context holds"
            ),
            Self::Invalid { .. } => write!(
                f,
                "typesafe http status {status}: {INVALID}, the service refused the request as invalid"
            ),
            Self::Status(_) => write!(f, "typesafe http status {status}"),
        }
    }
}

/// The refusal one response (`status`, `body`) makes, by the closed reason a 4xx body names in
/// `detail.error_type`; `None` for a success.
#[must_use]
pub fn refusal(status: u16, body: &[u8]) -> Option<Refusal> {
    let parsed = serde_json::from_slice::<Value>(body).ok();
    refused(status, parsed.as_ref())
}

fn refused(status: u16, parsed: Option<&Value>) -> Option<Refusal> {
    if (200..300).contains(&status) {
        return None;
    }
    let named = parsed.and_then(|body| body["detail"]["error_type"].as_str());
    Some(match named {
        Some(MAX_TOKENS) if (400..500).contains(&status) => Refusal::Capacity { status },
        Some(INVALID) if (400..500).contains(&status) => Refusal::Invalid { status },
        _ => Refusal::Status(status),
    })
}

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
/// it answered, and its reply bound by id, or why it answers no question (a refusal by its
/// closed reason, [`Refusal`]). The usage also rides the first answered question, so summing
/// answers counts it once.
pub fn read(
    questions: &[&ChoiceQuestion],
    status: u16,
    body: &[u8],
) -> (Usage, Result<Reply, String>) {
    let parsed = serde_json::from_slice::<Value>(body);
    let usage = parsed.as_ref().map_or_else(|_| Usage::default(), Usage::of);
    let written = serde_json::from_slice::<Listed>(body).map(|listed| listed.answers);
    let read = match (&parsed, refused(status, parsed.as_ref().ok())) {
        (_, Some(refusal)) => Err(refusal.to_string()),
        (Err(error), None) => Err(format!("response is not JSON: {error}")),
        (Ok(parsed), None) => {
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

/// Why a single item refused for capacity is not asked again.
const ALONE: &str =
    "this question was asked alone, so no smaller request can carry it; not asked again";

/// Why nothing of a batch whose every id is repeated is sent.
const ALL_REPEATED: &str = "every id of this batch is asked more than once; nothing sent";

/// Why an item has no answer while its request has not settled.
const WAITING: &str = "not answered: its request has not settled";

/// The capacity the service documents for a model, read in body bytes: a request is planned
/// inside a share of it at a rate measured on the service, so a batch never starts with a
/// request the documentation already says the service refuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Declared {
    /// The tokens one request may hold.
    tokens: usize,
}

impl Declared {
    /// What the service documents for `model`: `jev-1.13.0` takes 64k tokens per request (and
    /// 32k for the state of its longest question, which no single reference of a release
    /// nears); its refusal names no number. Any other model has none.
    fn of(model: &str) -> Option<Self> {
        (model == "jev-1.13.0").then_some(Self { tokens: 64_000 })
    }

    /// The tokens planned for a body of `bytes` asking `items` questions: five per sixteen bytes
    /// (3.2 bytes per token; the service counted real Foundry-qualification bodies on
    /// `jev-1.13.0` at 3.27, 3.65 and 3.73 bytes per token: 1,453 tokens for 4,755 bytes, 5,839
    /// for 21,334 and 14,104 for 52,568, 2026-10-10), plus 48 per answer (the same requests
    /// spent 37 per answer at the most).
    fn planned(bytes: usize, items: usize) -> usize {
        (bytes.saturating_mul(5).div_ceil(16)).saturating_add(items.saturating_mul(48))
    }

    /// Whether such a body fits 85% of the documented tokens: the rest covers the difference
    /// between the planned rate and the service's own count.
    fn fits(self, bytes: usize, items: usize) -> bool {
        Self::planned(bytes, items) <= self.tokens / 100 * 85
    }
}

/// One physical request of a [`Partition`]: the items it carried and what came of it.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Attempt {
    /// The positions in the batch of the items it carries, in item order.
    pub items: Vec<usize>,
    /// The attempt whose capacity refusal it asks one half of again.
    pub halves: Option<usize>,
    /// The bound it was split under before it left, when it was: the fewest items of a request
    /// refused for capacity, in this batch or before it ([`Partition::seeded`]).
    pub split_below: Option<usize>,
    /// The size of a body found too large to leave whole before it was split, when it was:
    /// past the capacity its model documents ([`Capacity::of_model`]), or as large as a body
    /// refused for capacity, in this batch or before it.
    pub split_bytes: Option<usize>,
    /// How long its transport took, as the host measured it ([`Partition::timed`]).
    pub elapsed_ms: Option<u64>,
    /// The size of its body in bytes: what left, never a token count.
    pub bytes: usize,
    /// The SHA-256 of its body.
    pub sha256: String,
    /// Whether it may have left: `false` only when it was kept from leaving.
    pub sent: bool,
    /// Its response status, once a response was read.
    pub status: Option<u16>,
    /// What its response reported it used, once (all unknown while none was read).
    pub usage: Usage,
    /// The model its response named.
    pub model: Option<String>,
    /// Ids its response answered that it did not ask: recorded, never assigned.
    pub unasked: Vec<String>,
    /// Why it answered none of its items, when it did not.
    pub error: Option<DecisionError>,
    /// The closed reason of its refusal.
    pub refusal: Option<Refusal>,
    /// Whether its refusal for capacity put its items next in its two halves (each asked unless
    /// withheld).
    pub halved: bool,
}

/// The physical requests one batch makes and what each of its items came to (A1). The first
/// request carries every item whose id the batch asks once: an id asked twice is never sent,
/// whatever request it would fall in. Only a refusal for capacity ([`Refusal::Capacity`]) of a
/// request carrying several items asks them again, in its two halves, first half first, each
/// half the body [`request`] writes for its own questions, so each still reads exactly the state
/// it reads alone. Halves are strictly smaller, so the partition ends; a single item so refused
/// is `over_capacity`, never truncated or resent, and nothing else is ever asked again. Such a
/// refusal also bounds the batch: once a request of n items was refused for capacity, a waiting
/// request of n items or more is split in halves before it leaves, in order, never sent whole
/// to be refused again; a single item is never split, nor refused in advance. A partition may
/// start under the bound its seat learned from earlier batches ([`Partition::seeded`]), and, for
/// a seat whose model documents its capacity, under that capacity and below the smallest body
/// it was refused for capacity with ([`Capacity::partition`]): a waiting request whose exact body
/// is too large is split in halves before it leaves, the same way. Pure: the host sends each
/// body through its own single-attempt client, says what came back, and withholds what waits
/// when it must stop ([`Partition::withhold`]); it reads no clock, the host says how long each
/// request took ([`Partition::timed`]).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Partition {
    /// Each item's answer, or why it has none, in item order.
    pub answers: Vec<Result<ChoiceAnswer, DecisionError>>,
    /// Each item's outcome, in item order: `chosen` · `none` · `outside_options` · `unanswered` ·
    /// `repeated` · `malformed` · `failed` (its request answered none of its items) ·
    /// `over_capacity` · `not_sent` (its id asked twice, or its request kept from leaving) · the
    /// word it was withheld with · `waiting` until its request settles.
    pub outcomes: Vec<&'static str>,
    /// Each physical request, in the order begun.
    pub attempts: Vec<Attempt>,
    /// Why the items still waiting were withheld, when they were.
    withheld: Option<DecisionError>,
    /// Each request still to begin, in order.
    waiting: VecDeque<Part>,
    /// A request of this many items or more is split before it leaves: the fewest items of a
    /// request refused for capacity, or the bound the partition was seeded with.
    below: Option<usize>,
    /// The capacity its seat's model documents: a body past it is split before it leaves.
    declared: Option<Declared>,
    /// The size of the smallest body of several items refused for capacity, in this batch or
    /// by its seat before it: a waiting body that large is split before it leaves.
    refused_bytes: Option<usize>,
}

/// One request still to begin: its items, the attempt whose capacity refusal it asks again, and
/// the bound or body size it was split under.
#[derive(Clone, Debug, PartialEq)]
struct Part {
    items: Vec<usize>,
    halves: Option<usize>,
    split_below: Option<usize>,
    split_bytes: Option<usize>,
}

impl Partition {
    /// The partition of `batch` before any request: every item whose id it asks once waits for
    /// the first request, in item order.
    #[must_use]
    pub fn of(batch: &ChoiceBatch) -> Self {
        let repeated = batch.repeated();
        let once = |at: &usize| !repeated.contains(batch.items[*at].question.id.as_str());
        let sendable: Vec<usize> = (0..batch.items.len()).filter(once).collect();
        let (answers, outcomes): (Vec<_>, Vec<_>) = (batch.items.iter())
            .map(|item| match item.question.id.as_str() {
                id if repeated.contains(id) => (Err(unsent(id)), "not_sent"),
                _ => (Err(DecisionError(WAITING.to_owned())), "waiting"),
            })
            .unzip();
        let first = Part {
            items: sendable,
            halves: None,
            split_below: None,
            split_bytes: None,
        };
        Self {
            answers,
            outcomes,
            attempts: Vec::new(),
            withheld: None,
            waiting: (!first.items.is_empty())
                .then_some(first)
                .into_iter()
                .collect(),
            below: None,
            declared: None,
            refused_bytes: None,
        }
    }

    /// The same partition before any request, starting under `below` (its seat's [`Capacity`]):
    /// a request of `below` items or more is split in halves before it leaves, as after a
    /// refusal for capacity of a request that size; `None` changes nothing.
    #[must_use]
    pub fn seeded(mut self, below: Option<usize>) -> Self {
        self.below = below;
        self
    }

    /// Whether a request is still to begin.
    #[must_use]
    pub fn waiting(&self) -> bool {
        !self.waiting.is_empty()
    }

    /// Begin the next request: its attempt, in flight, and the body to send for `model`; `None`
    /// when nothing waits. A request as large as one refused for capacity, in items or in body
    /// size, or whose body is past the capacity its model documents, is first split in halves,
    /// the first half begun and the second waiting next, until it is smaller or a single item.
    pub fn begin(&mut self, batch: &ChoiceBatch, model: &str) -> Option<(usize, Value)> {
        let mut part = self.waiting.pop_front()?;
        while part.items.len() > 1 {
            let (below, bytes) = match self.below.filter(|b| part.items.len() >= *b) {
                Some(bound) => (Some(bound), None),
                None => match self.oversized(batch, model, &part.items) {
                    Some(bytes) => (None, Some(bytes)),
                    None => break,
                },
            };
            let second = part.items.split_off(part.items.len() / 2);
            (part.split_below, part.split_bytes) =
                (below.or(part.split_below), bytes.or(part.split_bytes));
            self.waiting.push_front(Part {
                items: second,
                halves: part.halves,
                split_below: part.split_below,
                split_bytes: part.split_bytes,
            });
        }
        let body = request(model, &carried(batch, &part.items));
        let text = serde_json::to_string(&body).unwrap_or_default();
        self.attempts.push(Attempt {
            items: part.items,
            halves: part.halves,
            split_below: part.split_below,
            split_bytes: part.split_bytes,
            elapsed_ms: None,
            bytes: text.len(),
            sha256: sha256(&text),
            sent: true,
            status: None,
            usage: Usage::default(),
            model: None,
            unasked: Vec::new(),
            error: None,
            refusal: None,
            halved: false,
        });
        Some((self.attempts.len() - 1, body))
    }

    /// The size of the body `items` would send, when it may not leave whole: past the capacity
    /// the seat's model documents, or as large as a body refused for capacity. Nothing is
    /// measured when neither bounds the partition.
    fn oversized(&self, batch: &ChoiceBatch, model: &str, items: &[usize]) -> Option<usize> {
        if self.declared.is_none() && self.refused_bytes.is_none() {
            return None;
        }
        let body = serde_json::to_string(&request(model, &carried(batch, items)));
        let bytes = body.map_or(usize::MAX, |text| text.len());
        let past = self.declared.is_some_and(|d| !d.fits(bytes, items.len()));
        let refused = self.refused_bytes.is_some_and(|refused| bytes >= refused);
        (past || refused).then_some(bytes)
    }

    /// The response read for attempt `at` (`status`, `body`): each item it carried answered by
    /// id; for a refusal each failed, or, refused for capacity, its two halves waiting next.
    pub fn responded(&mut self, at: usize, batch: &ChoiceBatch, status: u16, body: &[u8]) {
        let Some(attempt) = self.attempts.get_mut(at) else {
            return;
        };
        let (usage, reading) = read(&carried(batch, &attempt.items), status, body);
        (attempt.status, attempt.usage) = (Some(status), usage);
        let message = match reading {
            Ok(reply) => {
                for (k, (outcome, answer)) in attempt.items.iter().zip(reply.items) {
                    if let (Some(slot), Some(word)) =
                        (self.answers.get_mut(*k), self.outcomes.get_mut(*k))
                    {
                        (*slot, *word) = (answer, outcome);
                    }
                }
                (attempt.model, attempt.unasked) = (Some(reply.model), reply.unasked);
                return;
            }
            Err(message) => message,
        };
        attempt.refusal = refusal(status, body);
        let (word, error) = match attempt.refusal {
            Some(Refusal::Capacity { .. }) if attempt.items.len() > 1 => {
                let refused = attempt.items.len();
                self.below = Some(self.below.map_or(refused, |below| below.min(refused)));
                let bytes = attempt.bytes;
                self.refused_bytes = Some(self.refused_bytes.map_or(bytes, |b| b.min(bytes)));
                let (first, second) = attempt.items.split_at(refused / 2);
                let half = |items: &[usize]| Part {
                    items: items.to_vec(),
                    halves: Some(at),
                    split_below: None,
                    split_bytes: None,
                };
                self.waiting.push_front(half(second));
                self.waiting.push_front(half(first));
                (attempt.halved, attempt.error) = (true, Some(DecisionError(message)));
                return;
            }
            Some(Refusal::Capacity { .. }) => ("over_capacity", format!("{message}; {ALONE}")),
            _ => ("failed", message),
        };
        let error = DecisionError(error);
        fail(
            (&mut self.answers, &mut self.outcomes),
            &attempt.items,
            word,
            &error,
        );
        attempt.error = Some(error);
    }

    /// Attempt `at` got no response, `sent` whether it may have left: each item it carried fails
    /// with `error` (`failed`, or `not_sent` when it was kept from leaving), never asked again.
    pub fn lost(&mut self, at: usize, sent: bool, error: DecisionError) {
        let Some(attempt) = self.attempts.get_mut(at) else {
            return;
        };
        let word = if sent { "failed" } else { "not_sent" };
        fail(
            (&mut self.answers, &mut self.outcomes),
            &attempt.items,
            word,
            &error,
        );
        (attempt.sent, attempt.error) = (sent, Some(error));
    }

    /// Withhold every item still waiting (the host stops, or cannot record a request before it
    /// leaves): never sent, each failing with `error` under the outcome `word`.
    pub fn withhold(&mut self, word: &'static str, error: &DecisionError) {
        for Part { items, .. } in self.waiting.drain(..) {
            fail((&mut self.answers, &mut self.outcomes), &items, word, error);
        }
        self.withheld.get_or_insert_with(|| error.clone());
    }

    /// The host measured attempt `at`'s transport, from just before its request left to its
    /// response or failure (`elapsed_ms` on its record): the partition reads no clock.
    pub fn timed(&mut self, at: usize, elapsed: std::time::Duration) {
        if let Some(attempt) = self.attempts.get_mut(at) {
            attempt.elapsed_ms = Some(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX));
        }
    }

    /// The bound a later batch to the same seat may start under ([`Capacity`]), once a request
    /// of this batch carrying several items was refused for capacity: one more than the most
    /// items a request of it carried and got answered, never above the fewest a request was
    /// refused for capacity with, and that fewest when none was answered. `None` when no such
    /// request was refused: the batch never met the seat's capacity, whatever it answered.
    #[must_use]
    pub fn learned(&self) -> Option<usize> {
        let refused = (self.attempts.iter().filter(|a| a.halved))
            .map(|a| a.items.len())
            .min()?;
        let answered = (self.attempts.iter())
            .filter(|a| a.status.is_some() && a.error.is_none())
            .map(|a| a.items.len())
            .max();
        Some(answered.map_or(refused, |most| refused.min(most.saturating_add(1))))
    }

    /// What the attempts reported they used: each count summed over the attempts that may have
    /// left, known only when every one of them reported it (an unknown is never zero).
    #[must_use]
    pub fn usage(&self) -> Usage {
        let total = |count: fn(&Usage) -> Option<u64>| -> Option<u64> {
            let sent = self.attempts.iter().filter(|a| a.sent);
            let mut counts = sent.map(|a| count(&a.usage)).peekable();
            counts.peek()?;
            counts.sum()
        };
        Usage {
            input_tokens: total(|u| u.input_tokens),
            output_tokens: total(|u| u.output_tokens),
            billing_units: total(|u| u.billing_units),
        }
    }

    /// The model the first response that named one named.
    #[must_use]
    pub fn model(&self) -> Option<String> {
        self.attempts.iter().find_map(|a| a.model.clone())
    }

    /// The ids responses answered that their request did not ask, each once, as first answered.
    #[must_use]
    pub fn unasked(&self) -> Vec<String> {
        let mut unasked: Vec<String> = Vec::new();
        for id in self.attempts.iter().flat_map(|a| &a.unasked) {
            if !unasked.contains(id) {
                unasked.push(id.clone());
            }
        }
        unasked
    }

    /// The first final reason items have no answer: a request that answered none of its items
    /// (a capacity refusal whose halves were asked is not final), why the waiting items were
    /// withheld, or every id asked twice (nothing sent).
    #[must_use]
    pub fn error(&self) -> Option<DecisionError> {
        let none_sent = self.outcomes.iter().all(|outcome| *outcome == "not_sent");
        let repeated = self.attempts.is_empty() && !self.outcomes.is_empty() && none_sent;
        (self.attempts.iter().filter(|a| !a.halved))
            .find_map(|a| a.error.clone())
            .or_else(|| self.withheld.clone())
            .or_else(|| repeated.then(|| DecisionError(ALL_REPEATED.to_owned())))
    }

    /// The journal record of attempt `at`: its items by id with their options and outcomes,
    /// never their words or state; how far it went, its status, model, usage once, the ids it
    /// was answered without asking, its body's size and digest, and its refusal's closed reason.
    /// A half names the journal slot of the attempt it halves (`slots`, one per attempt begun); a
    /// request split before it left names the bound it was split under (`split_below`) or the
    /// body size found too large (`split_bytes`); and once the host timed it, its transport time
    /// (`elapsed_ms`).
    #[must_use]
    pub fn record(&self, at: usize, batch: &ChoiceBatch, slots: &[usize]) -> Value {
        let Some(attempt) = self.attempts.get(at) else {
            return Value::Null;
        };
        let flying = attempt.sent && attempt.status.is_none() && attempt.error.is_none();
        let outcome = match (flying, attempt.status, &attempt.error) {
            (true, _, _) => "in_flight",
            (_, Some(_), None) => "answered",
            (_, Some(_), Some(_)) => "http_error",
            _ if attempt.sent => "transport_error",
            _ => "not_sent",
        };
        let word = (flying.then_some("in_flight")).or(attempt.halved.then_some("halved"));
        let mut record = json!({
            "batch": batch.id, "sent": attempt.sent, "outcome": outcome,
            "status": attempt.status, "model": attempt.model, "usage": attempt.usage.record(),
            "items": self.items(batch, &attempt.items, word), "unasked": attempt.unasked,
            "body": {"bytes": attempt.bytes, "sha256": attempt.sha256},
        });
        if let Some(error) = &attempt.error {
            record["error"] = json!(error.0);
        }
        if let Some(refusal) = attempt.refusal {
            record["refusal"] = json!(refusal.word());
        }
        if attempt.halved {
            record["halved"] = json!(true);
        }
        if let Some(slot) = attempt.halves.and_then(|parent| slots.get(parent)) {
            record["halves"] = json!(slot);
        }
        if let Some(below) = attempt.split_below {
            record["split_below"] = json!(below);
        }
        if let Some(bytes) = attempt.split_bytes {
            record["split_bytes"] = json!(bytes);
        }
        if let Some(elapsed) = attempt.elapsed_ms {
            record["elapsed_ms"] = json!(elapsed);
        }
        record
    }

    /// The journal record of the items no request carried whose outcome is `word` (an id asked
    /// twice: `not_sent`; withheld: the word they were withheld with): ONE attempt never sent.
    #[must_use]
    pub fn unsent(&self, batch: &ChoiceBatch, word: &str) -> Option<Value> {
        let at: Vec<usize> = (self.outcomes.iter().enumerate())
            .filter(|(_, outcome)| **outcome == word)
            .map(|(k, _)| k)
            .collect();
        let items = self.items(batch, &at, None);
        (!items.is_empty())
            .then(|| json!({"batch": batch.id, "items": items, "sent": false, "outcome": word}))
    }

    /// The journal items at `at`: each by id, its options and its outcome (`word` for all when
    /// given), its choice and concentration or why it has none.
    fn items(&self, batch: &ChoiceBatch, at: &[usize], word: Option<&str>) -> Vec<Value> {
        let each = |k: &usize| {
            let outcome = self.outcomes.get(*k).copied()?;
            Some((batch.items.get(*k)?, self.answers.get(*k)?, outcome))
        };
        (at.iter().filter_map(each))
            .map(|(item, answer, outcome)| {
                let mut record = json!({"question": item.question.id,
                    "options": item.question.keys(), "outcome": word.unwrap_or(outcome)});
                match (word, answer) {
                    (Some(_), _) => {}
                    (None, Ok(answer)) => {
                        record["choice"] = json!(answer.choice);
                        record["confidence"] = json!(answer.confidence);
                    }
                    (None, Err(error)) => record["error"] = json!(error.0),
                }
                record
            })
            .collect()
    }
}

/// What one seat learned of its capacity across its batches, for as long as the seat lives
/// (nothing is persisted), and what its model's documentation declares: the bounds its next
/// batch starts under. Each settled batch refused for capacity lowers the item bound to what that
/// batch learned ([`Partition::learned`]) and the body bound to the smallest body of several
/// items it was refused with; neither ever rises. A seat of a model whose capacity is documented
/// ([`Capacity::of_model`]) starts every batch in requests that capacity admits; a seat with no
/// documentation and no refusal has none, so its batches start whole.
#[derive(Debug)]
#[non_exhaustive]
pub struct Capacity {
    /// The item bound, `usize::MAX` while none is known.
    below: AtomicUsize,
    /// The size of the smallest body of several items refused for capacity, `usize::MAX` while
    /// none was.
    refused_bytes: AtomicUsize,
    /// The capacity its model documents.
    declared: Option<Declared>,
}

impl Default for Capacity {
    fn default() -> Self {
        Self {
            below: AtomicUsize::new(usize::MAX),
            refused_bytes: AtomicUsize::new(usize::MAX),
            declared: None,
        }
    }
}

impl Capacity {
    /// The capacity of a seat of `model`: nothing learned yet, and what the service documents
    /// for that model, if anything (`jev-1.13.0`: 64k tokens per request), so its first batch
    /// already starts in requests the documentation admits.
    #[must_use]
    pub fn of_model(model: &str) -> Self {
        Self {
            declared: Declared::of(model),
            ..Self::default()
        }
    }

    /// The bound learned so far: a request of this many items or more is split before it leaves.
    #[must_use]
    pub fn below(&self) -> Option<usize> {
        Some(self.below.load(Ordering::Acquire)).filter(|below| *below < usize::MAX)
    }

    /// The partition of `batch`, starting under the bounds learned so far
    /// ([`Partition::seeded`]) and the capacity its model documents.
    #[must_use]
    pub fn partition(&self, batch: &ChoiceBatch) -> Partition {
        let mut partition = Partition::of(batch).seeded(self.below());
        partition.declared = self.declared;
        partition.refused_bytes =
            Some(self.refused_bytes.load(Ordering::Acquire)).filter(|b| *b < usize::MAX);
        partition
    }

    /// Lower the bounds to what the settled `partition` learned, when it learned any.
    pub fn learn(&self, partition: &Partition) {
        if let Some(learned) = partition.learned() {
            self.below.fetch_min(learned, Ordering::AcqRel);
        }
        if let Some(bytes) = partition.refused_bytes {
            self.refused_bytes.fetch_min(bytes, Ordering::AcqRel);
        }
    }
}

/// The questions of the items at `at`, in order.
fn carried<'b>(batch: &'b ChoiceBatch, at: &[usize]) -> Vec<&'b ChoiceQuestion> {
    (at.iter().filter_map(|k| batch.items.get(*k)))
        .map(|item| &item.question)
        .collect()
}

/// Each item at `at` fails with `error` under the outcome `word`.
fn fail(
    (answers, outcomes): (
        &mut [Result<ChoiceAnswer, DecisionError>],
        &mut [&'static str],
    ),
    at: &[usize],
    word: &'static str,
    error: &DecisionError,
) {
    for k in at {
        if let (Some(answer), Some(outcome)) = (answers.get_mut(*k), outcomes.get_mut(*k)) {
            (*answer, *outcome) = (Err(error.clone()), word);
        }
    }
}

#[cfg(test)]
mod tests;
