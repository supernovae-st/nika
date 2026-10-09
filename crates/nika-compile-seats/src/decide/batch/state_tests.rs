// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Grouping never loses context: whatever the JSON kind of its state (nested objects, arrays,
//! strings, numbers, booleans, null; a key missing or null; values that differ per item), each
//! item reads in a batch exactly the state it reads alone ([`Carried`]). The witnesses over the
//! older batch API live in `regression_tests`.

use serde_json::{Value, json};

use super::super::{ChoiceOption, ChoiceQuestion};
use super::{Carried, ChoiceBatch};

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

/// Every kind of state set a batch may group.
fn corpus() -> Vec<Vec<Value>> {
    vec![
        // nested objects sharing an entry, differing below it
        vec![
            json!({"request": "r", "clause": {"text": "a", "tags": ["x", null]}}),
            json!({"request": "r", "clause": {"text": "b", "tags": []}}),
        ],
        // a key missing in one state, null in another
        vec![json!({"request": "r", "k": null}), json!({"request": "r"})],
        // null held alike
        vec![
            json!({"request": "r", "k": null}),
            json!({"request": "r", "k": null}),
        ],
        // nested values that differ only by a null below
        vec![json!({"a": {"x": 1}}), json!({"a": {"x": 1, "y": null}})],
        // the same number written as an integer and as a float
        vec![json!({"a": 1}), json!({"a": 1.0})],
        // strings, arrays, numbers, booleans and null: alike, then differing
        vec![json!("tickets"), json!("tickets")],
        vec![json!("tickets"), json!("orders")],
        vec![json!(["x", {"y": 2}]), json!(["x", {"y": 2}])],
        vec![json!(["x"]), json!(["y", "z"])],
        vec![json!(42), json!(42)],
        vec![json!(42), json!(true)],
        vec![json!(null), json!(null)],
        vec![json!(null), json!({"a": 1})],
        vec![json!({}), json!(null)],
        // every kind at once
        vec![
            json!({"a": 1}),
            json!("a"),
            json!([1]),
            json!(null),
            json!(0),
            json!(false),
        ],
        // one question alone
        vec![json!({})],
        vec![json!("alone")],
        // more items share than differ
        vec![
            json!({"request": "r"}),
            json!({"request": "r"}),
            json!({"request": "r", "extra": [null]}),
        ],
    ]
}

/// The state an item's `adds` restates beside the shared state: nothing (`null`) is the shared
/// state, entries beside an object extend it, any other value is the whole state.
fn restated(shared: &Value, adds: &Value) -> Value {
    match (shared, adds) {
        (_, Value::Null) => shared.clone(),
        (Value::Object(shared), Value::Object(own)) => {
            let mut whole = shared.clone();
            whole.extend(own.clone());
            Value::Object(whole)
        }
        (_, own) => own.clone(),
    }
}

/// For every set of states: each item is its question asked alone, and the batch's shared
/// state with what the item carries (both [`Carried`] and `adds`) is exactly its whole state.
#[test]
fn every_item_reads_in_a_batch_exactly_the_state_it_reads_alone() {
    for states in corpus() {
        let asked = questions(&states);
        let batch = ChoiceBatch::of("b", &asked);
        assert_eq!(batch.items.len(), asked.len());
        for (item, question) in batch.items.iter().zip(&asked) {
            assert_eq!(&item.question, question, "{states:?}");
            let carried = Carried::of(&batch.state, &question.state);
            assert_eq!(carried.state(&batch.state), question.state, "{states:?}");
            assert_eq!(
                restated(&batch.state, &item.adds),
                question.state,
                "{states:?}"
            );
        }
    }
}

/// What one state carries beside another: nothing, the entries it adds, or its whole state.
#[test]
fn a_state_carries_nothing_its_extension_or_its_whole_self() {
    let cases = [
        (json!({"a": 1}), json!({"a": 1}), Carried::Shared),
        (json!(null), json!(null), Carried::Shared),
        (
            json!({"a": 1}),
            json!({"a": 1, "b": null}),
            Carried::Beside(json!({"b": null}).as_object().cloned().unwrap()),
        ),
        (
            json!({}),
            json!({"b": [1]}),
            Carried::Beside(json!({"b": [1]}).as_object().cloned().unwrap()),
        ),
        // an object that lacks, or changes, a shared entry is carried whole
        (json!({"a": null}), json!({}), Carried::Whole(json!({}))),
        (
            json!({"a": 1}),
            json!({"a": 2}),
            Carried::Whole(json!({"a": 2})),
        ),
        (
            json!(null),
            json!({"a": 1}),
            Carried::Whole(json!({"a": 1})),
        ),
        (json!({}), json!(null), Carried::Whole(json!(null))),
        (json!("x"), json!("y"), Carried::Whole(json!("y"))),
        (json!([1]), json!([1, 2]), Carried::Whole(json!([1, 2]))),
    ];
    for (shared, state, expected) in cases {
        let carried = Carried::of(&shared, &state);
        assert_eq!(carried, expected, "{shared} · {state}");
        assert_eq!(carried.state(&shared), state, "{shared} · {state}");
    }
    assert_eq!(Carried::common(&[]), json!({}));
}
