// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::collections::BTreeMap;
use std::sync::Arc;

use nika_session_change::outcome::{Incarnation, QuestionId};

use super::*;

const REQUEST: &str =
    "Prepare a summary and show output alternatives before saving; do not save or run.";
const MARKDOWN: &str = "./out/summary.md";
const JSON: &str = "./out/summary.json";

fn citations() -> Citations {
    let mut lines = Citations::default();
    lines.record(1, Some(("u1".into(), REQUEST.into())));
    lines
}

fn choice(with_role: bool) -> Value {
    let mut choice = json!({"key": "format", "question": "Output format?", "options": [
        {"key": "md", "label": "Markdown summary", "recommended": true,
         "values": [{"role": "output_path", "value": MARKDOWN}]},
        {"key": "json", "label": "JSON figures", "values": [{"role": "output_path", "value": JSON}]}
    ]});
    if with_role {
        choice["role"] = json!("output_path");
    }
    choice
}

fn write(
    conversation: &mut Conversation,
    lines: &Citations,
    kind: &str,
    value: &str,
    cite: &str,
    excerpt: &str,
) {
    let rows = [json!({"kind": kind, "role": "output_path", "value": value,
        "message": cite, "excerpt": excerpt})];
    let source = format!("nika: summary\n# writes {value}\n");
    let reply = conversation.write(lines, (source, "summary".into()), (&rows, &[]), &mut |_| {
        Ok(())
    });
    assert!(!reply.is_error, "{reply:?}");
}

fn open(
    conversation: &Conversation,
    lines: &Citations,
    choice: &Value,
) -> Result<Vec<AskedQuestion>, String> {
    let identity = Arc::new(Incarnation);
    conversation.open_choices(
        lines,
        &json!({"open": [choice]}),
        Some("propose-1"),
        (
            &mut |context| QuestionId::new(context.to_owned(), &identity),
            &mut |_| None,
        ),
    )
}

fn derived() -> (Conversation, Citations) {
    let mut conversation = Conversation::default();
    let lines = citations();
    write(
        &mut conversation,
        &lines,
        "derived",
        MARKDOWN,
        "u1",
        REQUEST,
    );
    (conversation, lines)
}

#[test]
fn a_current_derived_output_can_be_shown_with_alternatives_but_grants_no_act() {
    for with_role in [true, false] {
        let (mut conversation, lines) = derived();
        let choices = open(&conversation, &lines, &choice(with_role)).unwrap();
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].options[0].values[0].value, MARKDOWN);
        assert_eq!(choices[0].options[1].values[0].value, JSON);
        assert_eq!(
            conversation.bindings()[0].provenance.kind,
            ProvenanceKind::Derived
        );
        assert!(conversation.answers().is_empty());
        assert_eq!(
            conversation.propose(&lines, &json!({}), ("scope".into(), choices)),
            Ok(None)
        );
        assert_eq!(conversation.open().len(), 1);
        assert!(conversation.saved().is_none());
    }
}

#[test]
fn a_derived_output_is_still_not_a_preparation_question() {
    for with_role in [true, false] {
        let (mut conversation, lines) = derived();
        let identity = Arc::new(Incarnation);
        let reply = conversation.ask(
            &lines,
            &json!({"questions": [choice(with_role)]}),
            Some("ask-1"),
            (
                &mut |context| QuestionId::new(context.to_owned(), &identity),
                &mut |_| None,
            ),
        );
        let result: Value = serde_json::from_str(&reply.text).unwrap();
        assert_eq!(result["refused"][0]["reason"], "settled");
        assert!(!reply.ends_turn);
        assert!(conversation.asked_ids().is_empty());
    }
}

#[test]
fn a_named_or_typed_output_cannot_reopen_by_omitting_its_question_role() {
    for kind in ["named", "answered", "delegated"] {
        for with_role in [true, false] {
            let mut conversation = Conversation::default();
            let mut lines = citations();
            lines.record(2, Some(("u2".into(), format!("Use {MARKDOWN}"))));
            write(&mut conversation, &lines, kind, MARKDOWN, "u2", MARKDOWN);
            let refused = open(&conversation, &lines, &choice(with_role)).unwrap_err();
            assert_eq!(
                refused,
                "`format`: already settled by the person; leave it out of `open`"
            );
        }
    }
}

#[test]
fn a_later_explicit_reopening_still_allows_a_human_settled_output_choice() {
    let mut conversation = Conversation::default();
    let mut lines = citations();
    lines.record(2, Some(("u2".into(), format!("Use {MARKDOWN}"))));
    write(
        &mut conversation,
        &lines,
        "answered",
        MARKDOWN,
        "u2",
        MARKDOWN,
    );
    lines.record(
        3,
        Some(("u3".into(), "Let me change the output format".into())),
    );
    for with_role in [true, false] {
        let mut reopened = choice(with_role);
        reopened["reopens"] = json!({"message": "u3", "excerpt": "change the output format"});
        assert_eq!(open(&conversation, &lines, &reopened).unwrap().len(), 1);
    }
}

#[test]
fn a_delegated_output_stays_settled_when_written_as_derived_under_a_new_key() {
    let mut conversation = Conversation::default();
    let mut lines = citations();
    let identity = Arc::new(Incarnation);
    conversation.asked.push(
        AskedQuestion::new(
            QuestionId::new("old-output".into(), &identity),
            "old-output",
            "Which output?",
        )
        .with_role(ValueRole::OutputPath),
    );
    lines.record(2, Some(("u2".into(), "Choose it for me".into())));
    let pick = BTreeMap::from([("old-output".into(), Pick::Delegated)]);
    conversation.answered_by("u2", "Choose it for me", &pick);
    assert_eq!(conversation.delegations().len(), 1);
    write(
        &mut conversation,
        &lines,
        "derived",
        MARKDOWN,
        "u2",
        "Choose it for me",
    );
    for with_role in [true, false] {
        assert_eq!(
            open(&conversation, &lines, &choice(with_role)).unwrap_err(),
            "`format`: already settled by the person; leave it out of `open`"
        );
    }
}

#[test]
fn a_derived_output_already_shown_and_retained_is_not_a_new_default() {
    let (mut conversation, lines) = derived();
    assert_eq!(
        conversation.propose(&lines, &json!({}), ("scope".into(), Vec::new())),
        Ok(None)
    );
    write(
        &mut conversation,
        &lines,
        "retained",
        MARKDOWN,
        "u1",
        REQUEST,
    );
    for with_role in [true, false] {
        assert_eq!(
            open(&conversation, &lines, &choice(with_role)).unwrap_err(),
            "`format`: already settled by the person; leave it out of `open`"
        );
    }
}

#[test]
fn another_choice_can_revise_the_candidate_while_its_unanswered_output_default_remains() {
    for kind in ["derived", "retained"] {
        for with_role in [true, false] {
            let (mut conversation, mut lines) = derived();
            let output = open(&conversation, &lines, &choice(with_role))
                .unwrap()
                .remove(0);
            let identity = Arc::new(Incarnation);
            let model = AskedQuestion::new(
                QuestionId::new("run-model".into(), &identity),
                "model",
                "Which model?",
            )
            .with_role(ValueRole::RunModel)
            .with_options(
                vec![
                    Offer::new("a", "Model A", true).with_values(vec![
                        super::super::OfferValue::new(ValueRole::RunModel, "fixture/model-a", None),
                    ]),
                    Offer::new("b", "Model B", false).with_values(vec![
                        super::super::OfferValue::new(ValueRole::RunModel, "fixture/model-b", None),
                    ]),
                ],
                true,
                false,
            );
            let id = model.id.clone();
            assert_eq!(
                conversation.propose(&lines, &json!({}), ("scope".into(), vec![output, model])),
                Ok(None)
            );
            lines.record(2, Some(("u2".into(), "b".into())));
            conversation.answered_one_by(&id, "u2", "b", &BTreeMap::new());
            assert_eq!(conversation.open().len(), 1);
            assert_eq!(conversation.open()[0].key, "format");
            write(&mut conversation, &lines, kind, MARKDOWN, "u1", REQUEST);
            let remaining = open(&conversation, &lines, &choice(with_role)).unwrap();
            assert_eq!(remaining.len(), 1);
            assert_eq!(remaining[0].options[0].values[0].value, MARKDOWN);
            let mut renamed = choice(with_role);
            renamed["key"] = json!("new-output-choice");
            assert_eq!(
                open(&conversation, &lines, &renamed).unwrap_err(),
                "`new-output-choice`: already settled by the person; leave it out of `open`"
            );
            assert_eq!(
                conversation.propose(&lines, &json!({}), ("scope".into(), remaining)),
                Ok(None)
            );
            assert!(conversation.saved().is_none());
            let picked = conversation
                .bindings()
                .iter()
                .find(|b| b.role == ValueRole::RunModel)
                .unwrap();
            assert_eq!(picked.value, "fixture/model-b");
            assert_eq!(picked.provenance.kind, ProvenanceKind::Offered);
            assert_eq!(picked.provenance.message, "u2");
        }
    }
}

#[test]
fn a_different_or_missing_default_does_not_free_a_derived_output_binding() {
    let (conversation, lines) = derived();
    let mut alternative = choice(true);
    alternative["options"][0]["recommended"] = json!(false);
    alternative["options"][1]["recommended"] = json!(true);
    let mut missing = choice(true);
    missing["options"][0]["values"] = json!([]);
    for invalid in [alternative, missing] {
        assert_eq!(
            open(&conversation, &lines, &invalid).unwrap_err(),
            "`format`: already settled by the person; leave it out of `open`"
        );
    }
}

#[test]
fn picking_an_alternative_replaces_the_derived_default_without_authorizing_save_or_run() {
    let (mut conversation, mut lines) = derived();
    let choices = open(&conversation, &lines, &choice(true)).unwrap();
    assert_eq!(
        conversation.propose(&lines, &json!({}), ("scope".into(), choices)),
        Ok(None)
    );
    lines.record(2, Some(("u2".into(), "json".into())));
    conversation.answered_by("u2", "json", &BTreeMap::new());
    assert_eq!(conversation.bindings().len(), 1);
    let selected = &conversation.bindings()[0];
    assert_eq!(selected.value, JSON);
    assert_eq!(selected.provenance.kind, ProvenanceKind::Offered);
    assert_eq!(selected.provenance.message, "u2");
    assert!(conversation.unsaved(&["format".into()]).is_some());
    let rows = [
        json!({"kind": "offered", "role": "output_path", "value": JSON,
        "message": "u2", "question": "format", "option": "json"}),
    ];
    let source = format!("nika: summary\n# writes {JSON}\n");
    let reply = conversation.write(
        &lines,
        (source, "JSON figures".into()),
        (&rows, &[]),
        &mut |_| Ok(()),
    );
    assert!(!reply.is_error, "{reply:?}");
    let mut renewed = choice(false);
    renewed["options"][0]["recommended"] = json!(false);
    renewed["options"][1]["recommended"] = json!(true);
    assert_eq!(
        open(&conversation, &lines, &renewed).unwrap_err(),
        "`format`: already settled by the person; leave it out of `open`"
    );
    assert_eq!(
        conversation.propose(&lines, &json!({}), ("scope".into(), Vec::new())),
        Ok(None)
    );
    assert!(conversation.saved().is_none());
}
