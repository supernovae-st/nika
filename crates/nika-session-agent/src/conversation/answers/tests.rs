// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::conversation::Citations;
use nika_compile_fidelity::fidelity::resolution::Resolution;
use nika_session_change::outcome::{Incarnation, QuestionId};
use nika_session_change::work::{OfferValue, ValueRole};
use std::sync::Arc;

fn question(key: &str) -> AskedQuestion {
    let id = QuestionId::new(key.into(), &Arc::new(Incarnation));
    let options = [
        ("revenue", "Revenue", "revenue"),
        ("clients", "Customer ranking", "customer_ranking"),
        ("status", "Status breakdown", "status_breakdown"),
    ]
    .map(|(key, label, name)| {
        Offer::new(key, label, false).with_values(vec![OfferValue::new(
            ValueRole::Value,
            "true",
            Some(name.into()),
        )])
    });
    AskedQuestion::new(id, key, "Which indicators should the report include?").with_options(
        options.to_vec(),
        true,
        true,
    )
}

fn selected(keys: &str) -> (Conversation, Citations) {
    let mut conversation = Conversation::default();
    let mut citations = Citations::default();
    citations.record(
        1,
        Some((
            "u1".into(),
            "Prepare a sales report; do not save or run it".into(),
        )),
    );
    citations.record(2, Some(("u2".into(), keys.into())));
    conversation.asked.push(question("indicators"));
    conversation.answered_by("u2", keys, &BTreeMap::new());
    (conversation, citations)
}

fn expected() -> Value {
    json!({
        "bound_values": ["true", "true"],
        "bindings": [
            {"key":"indicators","role":"value","value":"true","provenance":
                {"kind":"offered","message":"u2","question":"indicators","option":"revenue"}},
            {"key":"indicators","role":"value","value":"true","provenance":
                {"kind":"offered","message":"u2","question":"indicators","option":"clients"}}
        ],
        "choice_context": {
            "question_key":"indicators",
            "question":"Which indicators should the report include?",
            "human_citation":"u2",
            "selected_options":[
                {"key":"revenue","label":"Revenue","values":[{"role":"value","value":"true","name":"revenue"}],"provenance":{"kind":"offered","message":"u2","question":"indicators","option":"revenue"}},
                {"key":"clients","label":"Customer ranking","values":[{"role":"value","value":"true","name":"customer_ranking"}],"provenance":{"kind":"offered","message":"u2","question":"indicators","option":"clients"}}
            ]
        }
    })
}

#[test]
fn equal_boolean_values_keep_distinct_indicators_through_candidate_and_reopen() {
    let (mut conversation, citations) = selected(r#"["revenue","clients"]"#);
    let read = |conversation: &Conversation| {
        serde_json::from_str::<Value>(&conversation.answers()["indicators"]).unwrap()
    };
    assert_eq!(read(&conversation), expected());
    let rows = ["revenue", "clients"].map(|option| {
        json!({"value":"true","kind":"offered","role":"value",
            "question":"indicators","option":option,"message":"u2"})
    });
    let wrote = conversation.write(
        &citations,
        (
            "nika: sales\nconst: { revenue: true, customer_ranking: true }\n".into(),
            "Sales".into(),
        ),
        (&rows, &[]),
        &mut |_| Ok(()),
    );
    assert!(!wrote.is_error, "{wrote:?}");
    assert_eq!(read(&conversation), expected());
    assert_eq!(
        conversation
            .selections()
            .1
            .iter()
            .map(Resolution::to_json)
            .collect::<Vec<_>>(),
        rows,
    );
    let restored = Conversation::restored(&conversation.kept()).unwrap();
    assert_eq!(read(&restored), expected());
    let mut legacy = conversation.kept();
    legacy["answer_meanings"]["indicators"]
        .as_object_mut()
        .unwrap()
        .remove("selection_kind");
    assert_eq!(
        read(&Conversation::restored(&legacy).unwrap()),
        expected(),
        "the exact older bindings still carry selected provenance"
    );
    assert!(restored.asked.is_empty());
    assert!(restored.open.is_empty());
    assert!(restored.answered.is_empty());
    assert!(restored.shown.is_none());
    assert!(
        restored.continuations.is_empty(),
        "meaning alone cannot continue a mandate"
    );
}

#[test]
fn each_grouped_question_keeps_its_own_selected_meaning_and_citation() {
    let mut conversation = Conversation::default();
    let first = question("sales");
    let second = question("refunds");
    conversation.asked = vec![first.clone(), second.clone()];
    conversation.answered_one_by(&first.id, "u2", "revenue", &BTreeMap::new());
    conversation.answered_one_by(&second.id, "u3", "clients", &BTreeMap::new());
    let answers = conversation.answers();
    assert_eq!(answers.len(), 2);
    for (key, citation, option) in [("sales", "u2", "revenue"), ("refunds", "u3", "clients")] {
        let answer: Value = serde_json::from_str(&answers[key]).unwrap();
        assert_eq!(answer["bound_values"], "true");
        assert_eq!(answer["choice_context"]["question_key"], key);
        assert_eq!(answer["choice_context"]["human_citation"], citation);
        assert_eq!(
            answer["choice_context"]["selected_options"][0]["key"],
            option
        );
        assert_eq!(
            answer["choice_context"]["selected_options"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}

#[test]
fn a_boolean_answer_does_not_settle_another_questions_boolean_options() {
    let (conversation, mut citations) = selected("revenue");
    citations.record(
        3,
        Some(("u3".into(), "Ask me which refund metric to include".into())),
    );
    let incarnation = Arc::new(Incarnation);
    let mut mint = |key: &str| QuestionId::new(key.into(), &incarnation);
    let asked = conversation.question(
        &json!({"key":"refunds","question":"Which refund metric?","options":[
            {"key":"revenue","label":"Refunded revenue","values":[{"role":"value","value":"true","name":"refunded_revenue"}]},
            {"key":"count","label":"Refunded order count","values":[{"role":"value","value":"true","name":"refund_count"}]}
        ],"multi_select":true}),
        &citations,
        (None, 0),
        &mut mint,
        &mut |_| None,
        super::super::QuestionUse::Ask,
    ).unwrap();
    assert_eq!(asked.key, "refunds");
    assert_eq!(
        asked
            .options
            .iter()
            .map(|offer| offer.key.as_str())
            .collect::<Vec<_>>(),
        ["revenue", "count"]
    );
}

#[test]
fn changing_the_pick_never_carries_the_deselected_alternative_as_a_requirement() {
    let (mut conversation, _) = selected(r#"["revenue","clients"]"#);
    conversation.asked.push(question("indicators"));
    conversation.answered_by("u3", "revenue", &BTreeMap::new());
    let answer: Value = serde_json::from_str(&conversation.answers()["indicators"]).unwrap();
    assert_eq!(answer["bound_values"], "true");
    assert_eq!(answer["choice_context"]["human_citation"], "u3");
    assert_eq!(
        answer["choice_context"]["selected_options"],
        json!([
            {"key":"revenue","label":"Revenue","values":[{"role":"value","value":"true","name":"revenue"}],"provenance":{"kind":"offered","message":"u3","question":"indicators","option":"revenue"}}
        ])
    );
    assert_eq!(answer["bindings"].as_array().unwrap().len(), 1);
}

#[test]
fn invalid_or_replaced_choices_supply_no_selected_context() {
    for line in [r#"["revenue","missing"]"#, r#"["revenue","revenue"]"#, "[]"] {
        let (conversation, _) = selected(line);
        assert!(conversation.answers().is_empty(), "{line}");
        assert!(conversation.answer_meanings.is_empty(), "{line}");
    }
    let (mut conversation, mut citations) = selected("revenue");
    citations.record(
        3,
        Some(("u3".into(), "Use total paid orders instead".into())),
    );
    conversation
        .bind_answer(
            &citations,
            &json!({"key":"indicators","value":"total paid orders",
        "message":"u3","excerpt":"Use total paid orders instead"}),
        )
        .unwrap();
    assert_eq!(
        conversation.answers(),
        BTreeMap::from([("indicators".into(), json!("total paid orders").to_string())])
    );
    assert!(conversation.answer_meanings.is_empty());
}

mod empty_values;
