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
            ],
            "human_reply":"[\"revenue\",\"clients\"]"
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

/// The person's own words for a question Nika asked, on the line the Session read as their
/// answer, keep that question's scope: its words, that line, the role it asked and no option
/// picked, through a reopen.
#[test]
fn a_typed_reply_keeps_its_questions_scope() {
    let mut conversation = Conversation::default();
    let mut citations = Citations::default();
    citations.record(1, Some(("u1".into(), "Prepare the stock payload".into())));
    citations.record(2, Some(("u2".into(), "out/stock.json".into())));
    let words = "Which file is « payload out/stock.json »?";
    let id = QuestionId::new("payload_path".into(), &Arc::new(Incarnation));
    let asked = AskedQuestion::new(id, "payload_path", words).with_role(ValueRole::OutputPath);
    conversation.asked.push(asked);
    conversation.answered_by("u2", "out/stock.json", &BTreeMap::new());
    let typed = json!({"key": "payload_path", "value": "out/stock.json", "message": "u2",
        "excerpt": "out/stock.json"});
    conversation.bind_answer(&citations, &typed).unwrap();
    let expected = json!({
        "bound_values": "out/stock.json",
        "bindings": [{"key": "payload_path", "role": "output_path", "value": "out/stock.json",
            "provenance": {"kind": "answered", "message": "u2", "excerpt": "out/stock.json"}}],
        "choice_context": {"question_key": "payload_path", "question": words,
            "human_citation": "u2", "role": "output_path", "selected_options": [],
            "human_reply": "out/stock.json"},
    });
    let read = |conversation: &Conversation| {
        serde_json::from_str::<Value>(&conversation.answers()["payload_path"]).unwrap()
    };
    assert_eq!(read(&conversation), expected);
    let restored = Conversation::restored(&conversation.kept()).unwrap();
    assert_eq!(read(&restored), expected);
}

/// A line the Session recorded as one question's answer keeps that attribution when the author
/// binds it under another key: it is described as that question's answer, never a plain fact the
/// other key could stand for.
#[test]
fn a_line_recorded_for_one_question_keeps_it_under_another_key() {
    let mut conversation = Conversation::default();
    let mut citations = Citations::default();
    citations.record(1, Some(("u1".into(), "Prepare the stock payload".into())));
    citations.record(2, Some(("u2".into(), "out/stock.json".into())));
    let words = "Which file is « payload out/stock.json »?";
    let id = QuestionId::new("payload_path".into(), &Arc::new(Incarnation));
    conversation
        .asked
        .push(AskedQuestion::new(id, "payload_path", words));
    conversation.answered_by("u2", "out/stock.json", &BTreeMap::new());
    let elsewhere = json!({"key": "reread_path", "value": "out/stock.json", "message": "u2",
        "excerpt": "out/stock.json"});
    conversation.bind_answer(&citations, &elsewhere).unwrap();
    let answers = conversation.answers_in(&citations);
    let answer: Value = serde_json::from_str(&answers["reread_path"]).unwrap();
    let context = &answer["choice_context"];
    assert_eq!(context["question_key"], "payload_path", "{answer}");
    assert_eq!(context["question"], words, "{answer}");
    assert_eq!(context["human_citation"], "u2", "{answer}");
    assert_eq!(context["human_reply"], "out/stock.json", "{answer}");
    assert_eq!(context["human_lines"], json!([1, 2]), "{answer}");
    assert_eq!(answer["bindings"][0]["key"], "reread_path", "{answer}");
}

/// The Session locates each answer's line in the person's words by its citation, never by its
/// text: a request whose own second line has the reply's text keeps it, the reply u2 is found
/// where it stands, and a later ordinary line u3 with the same text is not taken for it.
#[test]
fn an_answers_line_is_located_by_its_citation_never_its_text() {
    let mut conversation = Conversation::default();
    let mut citations = Citations::default();
    let request = "Prepare the stock payload\nout/stock.json";
    citations.record(1, Some(("u1".into(), request.into())));
    citations.record(2, Some(("u2".into(), "out/stock.json".into())));
    let words = "Which file is « payload out/stock.json »?";
    let id = QuestionId::new("payload_path".into(), &Arc::new(Incarnation));
    conversation
        .asked
        .push(AskedQuestion::new(id, "payload_path", words));
    conversation.answered_by("u2", "out/stock.json", &BTreeMap::new());
    let typed = json!({"key": "payload_path", "value": "out/stock.json", "message": "u2",
        "excerpt": "out/stock.json"});
    conversation.bind_answer(&citations, &typed).unwrap();
    citations.record(3, Some(("u3".into(), "out/stock.json".into())));
    let stated = "Prepare the stock payload\nout/stock.json\nout/stock.json\nout/stock.json";
    assert_eq!(citations.stated(0), stated);
    assert_eq!(citations.located(0, "u2"), Some((2, 3)));
    assert_eq!(citations.located(0, "u3"), Some((3, 4)));
    let answer: Value =
        serde_json::from_str(&conversation.answers_in(&citations)["payload_path"]).unwrap();
    assert_eq!(
        answer["choice_context"]["human_lines"],
        json!([2, 3]),
        "{answer}"
    );
    let plain: Value = serde_json::from_str(&conversation.answers()["payload_path"]).unwrap();
    assert!(
        plain["choice_context"].get("human_lines").is_none(),
        "{plain}"
    );
}

/// A value bound for no question Nika asked, or from a line the Session did not read as the
/// question's answer, states a typed value: its literal, no question's scope.
#[test]
fn a_typed_value_outside_its_question_keeps_its_literal() {
    let mut conversation = Conversation::default();
    let mut citations = Citations::default();
    let request = "Write the payload to out/stock.json";
    citations.record(1, Some(("u1".into(), request.into())));
    citations.record(2, Some(("u2".into(), "out/stock.json".into())));
    let fact = json!({"key": "payload_path", "value": "out/stock.json", "message": "u1",
        "excerpt": "out/stock.json"});
    let literal = BTreeMap::from([(
        "payload_path".to_owned(),
        json!("out/stock.json").to_string(),
    )]);
    conversation.bind_answer(&citations, &fact).unwrap();
    assert_eq!(conversation.answers(), literal, "no question asked");
    assert!(conversation.answer_meanings.is_empty());
    let id = QuestionId::new("payload_path".into(), &Arc::new(Incarnation));
    conversation
        .asked
        .push(AskedQuestion::new(id, "payload_path", "Which file?"));
    conversation.answered_by("u2", "out/stock.json", &BTreeMap::new());
    conversation.bind_answer(&citations, &fact).unwrap();
    assert_eq!(
        conversation.answers(),
        literal,
        "the request's words, no reply"
    );
    assert!(conversation.answer_meanings.is_empty());
}

/// Two independent questions sharing their keys, the first offer of each labelled with the
/// words the native control records, the second recommended.
fn delegable(key: &str) -> AskedQuestion {
    let id = QuestionId::new(key.into(), &Arc::new(Incarnation));
    let offer = |option: &str, label: &str, recommended: bool, value: String| {
        Offer::new(option, label, recommended).with_values(vec![OfferValue::new(
            ValueRole::Value,
            value,
            None,
        )])
    };
    let options = vec![
        offer(
            "other",
            "I leave the choice to Nika",
            false,
            format!("{key}-other"),
        ),
        offer(
            "recommended",
            "Recommended source",
            true,
            format!("{key}-default"),
        ),
    ];
    AskedQuestion::new(id, key, "Which source?").with_options(options, true, false)
}

/// The words a label names pick that offer, as any whole line naming a label does. The
/// native delegation act binds the named question's recommendation as delegated instead,
/// whatever another label reads, and the other grouped question keeps waiting, its shared
/// keys untouched.
#[test]
fn the_delegation_act_binds_the_recommendation_whatever_a_label_reads() {
    let words = "I leave the choice to Nika.";
    let (q2, q1) = (delegable("q2"), delegable("q1"));
    let picked = |conversation: &Conversation, key: &str| -> Value {
        serde_json::from_str(&conversation.answers()[key]).unwrap()
    };
    let mut by_words = Conversation {
        asked: vec![q2.clone(), q1.clone()],
        ..Default::default()
    };
    by_words.answered_one_by(&q2.id, "u2", words, &BTreeMap::new());
    let answer = picked(&by_words, "q2");
    assert_eq!(answer["bound_values"], "q2-other", "{answer}");
    let offered = &answer["choice_context"]["selected_options"][0]["provenance"];
    assert_eq!(offered["kind"], "offered", "{answer}");
    assert_eq!(offered["option"], "other", "{answer}");
    let mut by_act = Conversation {
        asked: vec![q2.clone(), q1.clone()],
        ..Default::default()
    };
    let said = by_act.delegated_one_by(&q2.id, "u2", words);
    assert_eq!(said.len(), 1, "{said:?}");
    let answer = picked(&by_act, "q2");
    assert_eq!(answer["bound_values"], "q2-default", "{answer}");
    let delegated = &answer["choice_context"]["selected_options"][0]["provenance"];
    assert_eq!(delegated["kind"], "delegated", "{answer}");
    assert_eq!(delegated["message"], "u2", "{answer}");
    assert_eq!(delegated["question"], "q2", "{answer}");
    assert_eq!(delegated["option"], "recommended", "{answer}");
    assert_eq!(by_act.asked_ids(), vec![q1.id.clone()], "q1 keeps waiting");
    assert!(!by_act.answers().contains_key("q1"));
    by_act.answered_one_by(&q1.id, "u3", "other", &BTreeMap::new());
    let answer = picked(&by_act, "q1");
    assert_eq!(answer["bound_values"], "q1-other", "{answer}");
    let q2_again = picked(&by_act, "q2");
    assert_eq!(q2_again["bound_values"], "q2-default", "q2 unchanged");
}

mod empty_values;

/// A free reply that bound nothing still answered its question: the judges read it with the
/// question, its line and no option or value (live gauntlet q02: a free « Oui » to the author's
/// approval question was never seen, so the gate it approved read as invented). A reply that
/// bound a value keeps its described answer, never a second one.
#[test]
fn a_free_reply_that_bound_nothing_reaches_the_judges_with_its_question() {
    let mut conversation = Conversation::default();
    let mut citations = Citations::default();
    citations.record(1, Some(("u1".into(), "Surveille python.org".into())));
    citations.record(2, Some(("u2".into(), "Oui".into())));
    let id = QuestionId::new("approval".into(), &Arc::new(Incarnation));
    conversation.asked.push(AskedQuestion::new(
        id,
        "approval",
        "Veux-tu valider chaque envoi avant qu'il parte ?",
    ));
    conversation.answered_by("u2", "Oui", &BTreeMap::new());
    assert!(
        !conversation.answers().contains_key("approval"),
        "nothing was bound"
    );
    let answers = conversation.answers_in(&citations);
    let read: Value = serde_json::from_str(&answers["approval"]).unwrap();
    assert_eq!(
        read["choice_context"],
        json!({
            "question_key": "approval",
            "question": "Veux-tu valider chaque envoi avant qu'il parte ?",
            "human_citation": "u2",
            "selected_options": [],
            "human_reply": "Oui",
            "human_lines": [1, 2]
        })
    );
    assert!(read.get("bindings").is_none() && read.get("bound_values").is_none());
    let (bound, citations) = selected(r#"["revenue","clients"]"#);
    let answers = bound.answers_in(&citations);
    assert_eq!(answers.len(), 1, "{answers:?}");
    // « Non », or a free value, approves nothing: no description for the judges.
    for reply in ["Non", "./out/digest.md"] {
        let mut conversation = Conversation::default();
        let mut citations = Citations::default();
        citations.record(1, Some(("u1".into(), "Surveille python.org".into())));
        citations.record(2, Some(("u2".into(), reply.into())));
        let id = QuestionId::new("approval".into(), &Arc::new(Incarnation));
        conversation.asked.push(AskedQuestion::new(
            id,
            "approval",
            "Veux-tu valider chaque envoi avant qu'il parte ?",
        ));
        conversation.answered_by("u2", reply, &BTreeMap::new());
        assert!(conversation.answers_in(&citations).is_empty(), "{reply}");
    }
}
