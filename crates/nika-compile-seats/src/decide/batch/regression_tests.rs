// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Regression witnesses over the batch API as it stood before grouping became lossless (only
//! `ChoiceBatch::of` and `closed_choices`): a string, array or number state reached no request,
//! and a request rendered an item from its projection, never from the question it asks alone.

use nika_kernel::ai::provider::{ContentBlock, Message};
use serde_json::{Value, json};

use super::super::{ChoiceOption, ChoiceQuestion};
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

/// The words of a request message.
fn words(message: &Message) -> String {
    (message.content.iter())
        .map(|block| match block {
            ContentBlock::Text { text } => text.clone(),
            _ => String::new(),
        })
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
