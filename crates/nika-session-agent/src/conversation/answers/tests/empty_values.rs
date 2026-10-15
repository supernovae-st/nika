// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use crate::conversation::Pick;

fn without_values(key: &str) -> AskedQuestion {
    let mut asked = question(key);
    for offer in &mut asked.options {
        offer.values.clear();
    }
    asked
}

#[test]
fn grouped_choices_without_business_values_keep_exact_meaning_through_write_and_reopen() {
    let detail = AskedQuestion::new(
        QuestionId::new("detail".into(), &Arc::new(Incarnation)),
        "detail",
        "Quel niveau de détail ?",
    )
    .with_options(
        vec![
            Offer::new("court", "Court", false),
            Offer::new("detaille", "Détaillé", false),
        ],
        true,
        false,
    );
    let indicators = without_values("indicators");
    let mut conversation = Conversation {
        asked: vec![detail.clone(), indicators.clone()],
        ..Conversation::default()
    };
    conversation.answered_one_by(&detail.id, "u2", "detaille", &BTreeMap::new());
    conversation.answered_one_by(
        &indicators.id,
        "u3",
        r#"["revenue","clients"]"#,
        &BTreeMap::new(),
    );
    let expected = BTreeMap::from([
        ("detail".to_owned(), json!({
            "bound_values":[], "bindings":[],
            "choice_context": {
                "question_key":"detail", "question":"Quel niveau de détail ?", "human_citation":"u2",
                "selected_options":[{"key":"detaille","label":"Détaillé","values":[],
                    "provenance":{"kind":"offered","message":"u2","question":"detail","option":"detaille"}}]
            }
        }).to_string()),
        ("indicators".to_owned(), json!({
            "bound_values":[], "bindings":[],
            "choice_context": {
                "question_key":"indicators", "question":"Which indicators should the report include?", "human_citation":"u3",
                "selected_options":[
                    {"key":"revenue","label":"Revenue","values":[],
                     "provenance":{"kind":"offered","message":"u3","question":"indicators","option":"revenue"}},
                    {"key":"clients","label":"Customer ranking","values":[],
                     "provenance":{"kind":"offered","message":"u3","question":"indicators","option":"clients"}}
                ]
            }
        }).to_string()),
    ]);
    assert_eq!(conversation.answers(), expected);
    assert!(
        conversation.bindings.is_empty(),
        "choosing a description fabricates no value"
    );
    let wrote = conversation.write(
        &Citations::default(),
        ("nika: sales\n".into(), "Sales report".into()),
        (&[], &[]),
        &mut |_| Ok(()),
    );
    assert!(!wrote.is_error, "{wrote:?}");
    assert_eq!(conversation.answers(), expected);
    assert_eq!(conversation.selections(), (vec![], vec![]));
    let restored = Conversation::restored(&conversation.kept()).unwrap();
    assert_eq!(restored.answers(), expected);
    assert!(restored.bindings.is_empty());
    assert!(restored.asked.is_empty());
    assert!(restored.open.is_empty());
    assert!(restored.answered.is_empty());
    assert!(restored.shown.is_none());
}

#[test]
fn no_value_choices_require_a_valid_answer_and_keep_only_the_latest_selected_options() {
    for invalid in [
        "missing",
        r#"["revenue","missing"]"#,
        r#"["revenue","revenue"]"#,
        "[]",
    ] {
        let mut conversation = Conversation::default();
        conversation.asked.push(without_values("indicators"));
        conversation.answered_by("u2", invalid, &BTreeMap::new());
        assert_eq!(conversation.answers(), BTreeMap::new(), "{invalid}");
    }
    let mut conversation = Conversation::default();
    conversation.asked.push(without_values("indicators"));
    conversation.answered_by("u2", r#"["revenue","clients"]"#, &BTreeMap::new());
    conversation.asked.push(without_values("indicators"));
    conversation.answered_by("u3", "clients", &BTreeMap::new());
    let answer: Value = serde_json::from_str(&conversation.answers()["indicators"]).unwrap();
    assert_eq!(answer["bound_values"], json!([]));
    assert_eq!(answer["bindings"], json!([]));
    assert_eq!(answer["choice_context"]["human_citation"], "u3");
    assert_eq!(
        answer["choice_context"]["selected_options"],
        json!([
            {"key":"clients","label":"Customer ranking","values":[],
             "provenance":{"kind":"offered","message":"u3","question":"indicators","option":"clients"}}
        ])
    );
}

#[test]
fn a_delegated_description_is_recorded_as_delegation_without_manufacturing_a_value() {
    let mut asked = without_values("indicators");
    asked.options[0].recommended = true;
    let mut conversation = Conversation::default();
    conversation.asked.push(asked);
    conversation.answered_by(
        "u2",
        "Choose for me, but do not save or run",
        &BTreeMap::from([("indicators".into(), Pick::Delegated)]),
    );
    let answer: Value = serde_json::from_str(&conversation.answers()["indicators"]).unwrap();
    assert_eq!(
        answer["choice_context"]["selected_options"],
        json!([
            {"key":"revenue","label":"Revenue","values":[],
             "provenance":{"kind":"delegated","message":"u2","question":"indicators","option":"revenue"}}
        ])
    );
    assert!(conversation.bindings.is_empty());
    assert!(
        conversation.continuations.is_empty(),
        "meaning does not grant a mandate continuation"
    );
}
