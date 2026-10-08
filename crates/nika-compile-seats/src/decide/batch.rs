// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Several independent closed choices over one state (A1: independent questions are grouped).
//! Each item keeps its own id, options and NONE, and stays answerable alone. A provider seat
//! settles a batch in one request whose answer maps each item id to one of that item's keys, so
//! every answer is bound to its question by id, never by position. A seat that cannot group
//! questions asks each item as its own question, all at once: one physical request per item, no
//! retry, the answers in item order.
//!
//! Grouping never loses context: whatever its JSON kind (object, array, string, number, null),
//! each item's whole state is what the batch shares plus what the item carries beside it
//! ([`Carried`]), and a request renders every item from the question it asks alone.

use std::{
    borrow::Cow,
    collections::BTreeSet,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use nika_kernel::ai::provider::{InferResponse, Message, Role};
use serde_json::{Map, Value, json};

use super::{ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionError, NONE_OPTION, answer_text};

/// One item of a batch: the question as it is asked alone, and what it adds to the batch's
/// shared instructions and state when the batch is asked in one request.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct BatchItem {
    /// The item asked alone: its id, its whole instructions and state, its options and NONE.
    pub question: ChoiceQuestion,
    /// What the item asks beyond the batch's shared instructions (empty when nothing).
    pub asks: String,
    /// What the item adds to the batch's shared state (its clause, its part of a run): `null`
    /// when nothing, the entries it adds when both states are objects, else its whole state, read
    /// in place of the shared one ([`Carried`]).
    pub adds: Value,
}

/// How one question's whole state rides beside the state a batch shares: what a request must
/// carry with the item so that it reads exactly the state it reads alone, never less.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Carried {
    /// The shared state is the question's whole state: nothing rides beside it.
    Shared,
    /// The shared state (an object) and these entries of the question's own, read together.
    Beside(Map<String, Value>),
    /// The question's whole state, read in place of the shared state.
    Whole(Value),
}

impl Carried {
    /// The state `states` hold alike: when all are objects, the entries every one holds with the
    /// same value (`{}` when none); else the one state they all are; else nothing (`null`). A
    /// key one state lacks is never shared with a state that holds it, even as `null`.
    #[must_use]
    pub fn common(states: &[&Value]) -> Value {
        let Some(first) = states.first() else {
            return Value::Object(Map::new());
        };
        if states.iter().all(|state| state.is_object()) {
            let alike =
                |key: &str, value: &Value| (states.iter()).all(|s| s.get(key) == Some(value));
            let entries = (first.as_object().into_iter().flatten())
                .filter(|(key, value)| alike(key, value))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            Value::Object(entries)
        } else if states.iter().all(|state| state == first) {
            (*first).clone()
        } else {
            Value::Null
        }
    }

    /// What `state` carries beside `shared`: nothing when they are equal; the entries an object
    /// adds when it holds every shared entry alike; else its whole state.
    #[must_use]
    pub fn of(shared: &Value, state: &Value) -> Self {
        if state == shared {
            return Self::Shared;
        }
        match (shared.as_object(), state.as_object()) {
            (Some(shared), Some(own)) if shared.iter().all(|(k, v)| own.get(k) == Some(v)) => {
                let beside = (own.iter())
                    .filter(|(key, _)| !shared.contains_key(*key))
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect();
                Self::Beside(beside)
            }
            _ => Self::Whole(state.clone()),
        }
    }

    /// The whole state a question reads from `shared` and what it carries; for every pair,
    /// `Carried::of(shared, state).state(shared)` is `state`.
    #[must_use]
    pub fn state(&self, shared: &Value) -> Value {
        match (self, shared) {
            (Self::Shared, _) => shared.clone(),
            (Self::Beside(own), Value::Object(entries)) => {
                let mut whole = entries.clone();
                whole.extend(own.iter().map(|(key, value)| (key.clone(), value.clone())));
                Value::Object(whole)
            }
            (Self::Beside(own), _) => Value::Object(own.clone()),
            (Self::Whole(state), _) => state.clone(),
        }
    }

    /// The item's [`BatchItem::adds`]: `null`, the entries it adds, or its whole state. A batch
    /// made by [`ChoiceBatch::of`] never carries a whole `null` (it would be shared).
    fn adds(self) -> Value {
        match self {
            Self::Shared => Value::Null,
            Self::Beside(own) => Value::Object(own),
            Self::Whole(state) => state,
        }
    }
}

impl BatchItem {
    /// One item: the question as asked alone, what it asks beyond the shared instructions, the
    /// state it adds to the shared state.
    #[must_use]
    pub fn new(question: ChoiceQuestion, asks: impl Into<String>, adds: Value) -> Self {
        Self {
            question,
            asks: asks.into(),
            adds,
        }
    }
}

/// Several independent closed choices over one state.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChoiceBatch {
    /// Stable batch id.
    pub id: String,
    /// What the seat decides for every item.
    pub instructions: String,
    /// The bounded state every item reads.
    pub state: Value,
    /// The items, in order.
    pub items: Vec<BatchItem>,
}

impl ChoiceBatch {
    /// A batch of items over one shared state and shared instructions.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        instructions: impl Into<String>,
        state: Value,
        items: Vec<BatchItem>,
    ) -> Self {
        Self {
            id: id.into(),
            instructions: instructions.into(),
            state,
            items,
        }
    }
}

impl ChoiceBatch {
    /// The batch of `questions`, each as it is asked alone: what their instructions share (their
    /// longest common prefix, ended at the last paragraph break in it) and the state they hold
    /// alike ([`Carried::common`]) become the batch's; what each adds stays its item's own, so
    /// every item's whole state is the shared state with what it carries, whatever its JSON kind.
    #[must_use]
    pub fn of(id: impl Into<String>, questions: &[ChoiceQuestion]) -> Self {
        let shared = (questions.iter().map(|q| q.instructions.as_str()))
            .reduce(common_prefix)
            .unwrap_or_default();
        let shared = shared.rfind("\n\n").map_or("", |end| &shared[..end]);
        let states: Vec<&Value> = questions.iter().map(|q| &q.state).collect();
        let state = Carried::common(&states);
        let items = (questions.iter())
            .map(|q| {
                let asks = q.instructions[shared.len()..].trim_start().to_owned();
                BatchItem::new(q.clone(), asks, Carried::of(&state, &q.state).adds())
            })
            .collect();
        Self::new(id, shared, state, items)
    }

    /// The item ids this batch asks more than once: an answer keyed by id cannot tell their
    /// questions apart, so no request may carry them.
    pub(super) fn repeated(&self) -> BTreeSet<&str> {
        let mut seen = BTreeSet::new();
        (self.items.iter())
            .map(|item| item.question.id.as_str())
            .filter(|id| !seen.insert(*id))
            .collect()
    }
}

/// Why an item is not sent: its id is asked more than once in its batch.
pub(super) fn unsent(id: &str) -> DecisionError {
    DecisionError(format!(
        "the batch asks `{id}` more than once: an answer keyed by id cannot tell them apart; not sent"
    ))
}

/// The batch of its items at `at` (in order), its id, instructions and shared state unchanged.
pub(super) fn only<'b>(batch: &'b ChoiceBatch, at: &[usize]) -> Cow<'b, ChoiceBatch> {
    if at.len() == batch.items.len() {
        return Cow::Borrowed(batch);
    }
    let items = (at.iter()).filter_map(|k| batch.items.get(*k).cloned());
    Cow::Owned(ChoiceBatch::new(
        batch.id.clone(),
        batch.instructions.clone(),
        batch.state.clone(),
        items.collect(),
    ))
}

/// What a question asks beyond the shared instructions, read from the question it asks alone:
/// the rest of its words when they open with the shared ones up to a break, else all of them.
fn own_words<'q>(shared: &str, whole: &'q str) -> &'q str {
    match whole.strip_prefix(shared) {
        Some(rest)
            if !shared.is_empty() && (rest.is_empty() || rest.starts_with(char::is_whitespace)) =>
        {
            rest.trim_start()
        }
        _ => whole,
    }
}

/// The longest common prefix of two texts, on a character boundary.
fn common_prefix<'t>(a: &'t str, b: &str) -> &'t str {
    let end = (a.char_indices())
        .zip(b.chars())
        .find(|((_, x), y)| x != y)
        .map_or(a.len().min(b.len()), |((at, _), _)| at);
    &a[..end]
}

/// The object-safe future a seat returns for a batch: one answer per item, in item order.
pub type BatchFuture<'a> =
    Pin<Box<dyn Future<Output = Vec<Result<ChoiceAnswer, DecisionError>>> + Send + 'a>>;

/// Each item of `batch` asked alone of `ask`, all at once: one physical request per item, the
/// answers in item order.
pub fn each_alone<'a>(
    batch: &'a ChoiceBatch,
    ask: impl Fn(&'a ChoiceQuestion) -> ChoiceFuture<'a>,
) -> BatchFuture<'a> {
    let asked: Vec<ChoiceFuture<'a>> = (batch.items.iter())
        .map(|item| ask(&item.question))
        .collect();
    Box::pin(joined(asked))
}

/// Every future to its end, polled together; the outputs in the order given.
async fn joined<T>(mut futures: Vec<Pin<Box<dyn Future<Output = T> + Send + '_>>>) -> Vec<T> {
    let mut done: Vec<Option<T>> = futures.iter().map(|_| None).collect();
    std::future::poll_fn(|cx: &mut Context<'_>| {
        let mut pending = false;
        for (slot, future) in done.iter_mut().zip(futures.iter_mut()) {
            if slot.is_none() {
                match future.as_mut().poll(cx) {
                    Poll::Ready(value) => *slot = Some(value),
                    Poll::Pending => pending = true,
                }
            }
        }
        if pending {
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await;
    done.into_iter().flatten().collect()
}

/// The two messages and the answer schema of a batch asked in one request: the shared
/// instructions and state once, then each item with what it asks, what it carries beside the
/// shared state and its options, all read from the question it asks alone ([`Carried`]); the
/// answer names, for each item id, one of that item's keys.
#[must_use]
pub fn closed_choices(batch: &ChoiceBatch) -> (Vec<Message>, Value) {
    let system = format!(
        "You settle SEVERAL independent closed choices for a workflow compiler, one per item. Read the shared STATE once, then judge each item alone, on its own words and its own state, and pick exactly one of THAT item's option keys. {} Choose \"{NONE_OPTION}\" for an item when none of its options fits. Return only a JSON object mapping each item id to the key you chose for it.",
        batch.instructions
    );
    let pretty = |value: &Value| serde_json::to_string_pretty(value).unwrap_or_default();
    let items: Vec<String> = (batch.items.iter())
        .map(|item| {
            let options: Vec<String> = (item.question.options.iter())
                .map(|o| format!("- {}: {}", o.key, o.description))
                .collect();
            let mut text = format!("ITEM {}:", item.question.id);
            let asks = own_words(&batch.instructions, &item.question.instructions);
            if !asks.is_empty() {
                text.push('\n');
                text.push_str(asks);
            }
            match Carried::of(&batch.state, &item.question.state) {
                Carried::Shared => {}
                Carried::Beside(own) => {
                    text.push_str("\nITEM STATE, read with the shared STATE:\n");
                    text.push_str(&pretty(&Value::Object(own)));
                }
                Carried::Whole(own) => {
                    text.push_str("\nITEM STATE, read in place of the shared STATE:\n");
                    text.push_str(&pretty(&own));
                }
            }
            format!("{text}\nOPTIONS:\n{}", options.join("\n"))
        })
        .collect();
    let user = format!(
        "STATE:\n{}\n\nITEMS:\n\n{}",
        serde_json::to_string_pretty(&batch.state).unwrap_or_default(),
        items.join("\n\n")
    );
    let properties: Map<String, Value> = (batch.items.iter())
        .map(|item| {
            let keys = item.question.keys();
            (
                item.question.id.clone(),
                json!({"type": "string", "enum": keys}),
            )
        })
        .collect();
    let required: Vec<&str> = (batch.items.iter())
        .map(|item| item.question.id.as_str())
        .collect();
    let schema = json!({
        "type": "object", "additionalProperties": false, "required": required,
        "properties": properties,
    });
    (
        vec![
            Message::text(Role::System, system),
            Message::text(Role::User, user),
        ],
        schema,
    )
}

/// Each item's key in a batch answer: one complete JSON object naming, for each item id, one of
/// that item's offered keys. An item the answer leaves out, answers more than once, or answers
/// outside its options, is undecided (`None`).
///
/// # Errors
/// A [`DecisionError`] when the answer is not one complete JSON object: it decides no item.
pub fn decoded_each(
    batch: &ChoiceBatch,
    response: &InferResponse,
) -> Result<Vec<Option<String>>, DecisionError> {
    decoded_items(batch, response).map(|items| items.into_iter().map(Result::ok).collect())
}

/// Each item's key in a batch answer, or why the answer gives it none; bound by id, never by
/// position, and a repeated id decides neither of its answers.
///
/// # Errors
/// A [`DecisionError`] when the answer is not one complete JSON object: it decides no item.
pub(super) fn decoded_items(
    batch: &ChoiceBatch,
    response: &InferResponse,
) -> Result<Vec<Result<String, DecisionError>>, DecisionError> {
    let text = answer_text(response).ok_or_else(|| {
        DecisionError("the seat did not return one complete JSON text".to_owned())
    })?;
    let value: Value = serde_json::from_str(text)
        .map_err(|e| DecisionError(format!("the seat answer is not JSON: {e}")))?;
    let (Some(answer), Ok(Named(named))) = (value.as_object(), serde_json::from_str(text)) else {
        return Err(DecisionError(
            "the seat answer is not one object of item keys".to_owned(),
        ));
    };
    let item = |question: &ChoiceQuestion| {
        let id = question.id.as_str();
        match (
            named.iter().filter(|key| *key == id).count(),
            answer.get(id),
        ) {
            (0, _) => Err("the seat left this item without an answer".to_owned()),
            (1, Some(Value::String(key))) if question.keys().contains(key) => Ok(key.clone()),
            (1, Some(Value::String(key))) => Err(format!(
                "the seat chose `{key}` for this item, which it did not offer"
            )),
            (1, _) => Err("the seat answered this item with no key".to_owned()),
            _ => Err("the seat answered this item more than once".to_owned()),
        }
    };
    Ok((batch.items.iter())
        .map(|entry| item(&entry.question).map_err(DecisionError))
        .collect())
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
mod regression_tests;
#[cfg(test)]
mod state_tests;
#[cfg(test)]
mod tests;
