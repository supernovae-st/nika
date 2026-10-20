// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::sync::Arc;

use super::*;
use crate::conversation::Acts;
use nika_session_change::outcome::Incarnation;
use nika_session_change::work::{AskedQuestion, Offer, OfferValue, ValueRole};

const MANDATE: &str = "Crée le workflow, enregistre et lance maintenant";

fn request() -> (Conversation, Citations, QuestionId) {
    let asker = Arc::new(Incarnation);
    let id = QuestionId::new("formats".into(), &asker);
    let mut conversation = Conversation::default();
    conversation.asked.push(
        AskedQuestion::new(id.clone(), "formats", "Quels formats faut-il produire ?").with_options(
            vec![
                Offer::new("md", "Markdown", false).with_values(vec![OfferValue::new(
                    ValueRole::Value,
                    "markdown",
                    None,
                )]),
                Offer::new("csv", "Tableau CSV", false).with_values(vec![OfferValue::new(
                    ValueRole::Value,
                    "csv",
                    None,
                )]),
            ],
            true,
            true,
        ),
    );
    let mut citations = Citations::default();
    citations.record(1, Some(("u1".into(), MANDATE.into())));
    (conversation, citations, id)
}

fn prepare(conversation: &mut Conversation, citations: &Citations) {
    let reply = conversation.write(
        citations,
        ("nika: report\ntasks: {}\n".into(), "Report".into()),
        (&[], &[]),
        &mut |_| Ok(()),
    );
    assert!(!reply.is_error, "{reply:?}");
}

fn args() -> Value {
    json!({"acts": ["save", "run"],
        "authorized_by": {"message": "u1", "excerpt": MANDATE}})
}

fn answer(conversation: &mut Conversation, citations: &mut Citations, id: &QuestionId, raw: &str) {
    citations.record(4, Some(("u2".into(), raw.into())));
    conversation.answered_one_by(id, "u2", raw, &BTreeMap::new());
}

#[test]
fn a_bound_protocol_answer_continues_the_initial_mandate() {
    let (mut conversation, mut citations, id) = request();
    answer(&mut conversation, &mut citations, &id, "md");
    prepare(&mut conversation, &citations);
    assert_eq!(
        conversation.propose(&citations, &args(), ("S1".into(), vec![])),
        Ok(Some(Acts {
            save: true,
            run: true,
            revised: true
        }))
    );
    let text = conversation.authorization_text(&citations, "u1").unwrap();
    assert!(text.contains(MANDATE));
    assert!(text.contains("Quels formats faut-il produire ?"));
    assert!(text.contains("Markdown"));
    assert!(text.contains("markdown"));
    assert!(
        !text.contains("Tableau CSV"),
        "unselected options grant nothing"
    );
}

#[test]
fn multiple_choices_keep_their_exact_selected_meaning_in_offer_order() {
    let (mut conversation, mut citations, id) = request();
    answer(&mut conversation, &mut citations, &id, r#"["csv","md"]"#);
    prepare(&mut conversation, &citations);
    assert!(conversation.preparation_continues(&citations, 1));
    let receipt = &conversation.continuations["u2"];
    assert_eq!(receipt.meaning["selected_offers"][0]["key"], "md");
    assert_eq!(receipt.meaning["selected_offers"][1]["key"], "csv");
    assert_eq!(
        receipt.meaning["selected_offers"].as_array().unwrap().len(),
        2
    );
}

#[test]
fn free_sentences_and_semantic_delegation_do_not_continue_a_mandate() {
    for (line, pick) in [
        ("Markdown, mais ne lance rien", Pick::Offer("md".into())),
        ("Choisis, mais ne lance rien", Pick::Delegated),
        (
            "Markdown, puis change la destination",
            Pick::Offer("md".into()),
        ),
    ] {
        let (mut conversation, mut citations, id) = request();
        citations.record(4, Some(("u2".into(), line.into())));
        let read = BTreeMap::from([("formats".into(), pick)]);
        conversation.answered_one_by(&id, "u2", line, &read);
        prepare(&mut conversation, &citations);
        assert!(conversation.continuations.is_empty(), "{line}");
        assert_eq!(conversation.authorization_text(&citations, "u1"), None);
        assert!(
            conversation
                .propose(&citations, &args(), ("S1".into(), vec![]))
                .is_err()
        );
    }
}

#[test]
fn stale_identity_invalid_set_or_changed_reply_cannot_supply_a_receipt() {
    for raw in [r#"["md","md"]"#, r#"["missing"]"#, "[]"] {
        let (mut conversation, mut citations, id) = request();
        answer(&mut conversation, &mut citations, &id, raw);
        assert!(conversation.continuations.is_empty(), "{raw}");
    }
    let (mut conversation, mut citations, _) = request();
    let other = QuestionId::new("formats".into(), &Arc::new(Incarnation));
    answer(&mut conversation, &mut citations, &other, "md");
    assert!(conversation.continuations.is_empty());

    let (mut conversation, mut citations, id) = request();
    answer(&mut conversation, &mut citations, &id, "md");
    citations.record(4, Some(("u2".into(), "md, ne lance rien".into())));
    prepare(&mut conversation, &citations);
    assert!(!conversation.preparation_continues(&citations, 1));
    assert_eq!(conversation.authorization_text(&citations, "u1"), None);
}

#[test]
fn every_intervening_line_must_be_a_bound_choice_written_before_the_candidate() {
    let (mut conversation, mut citations, id) = request();
    prepare(&mut conversation, &citations);
    answer(&mut conversation, &mut citations, &id, "md");
    assert!(!conversation.preparation_continues(&citations, 1));
    prepare(&mut conversation, &citations);
    assert!(conversation.preparation_continues(&citations, 1));
    citations.record(3, Some(("stop".into(), "Ne lance rien".into())));
    assert!(!conversation.preparation_continues(&citations, 1));
    assert_eq!(conversation.authorization_text(&citations, "u1"), None);
}

#[test]
fn reopening_keeps_choice_evidence_without_live_questions_or_replayed_acts() {
    let (mut conversation, mut citations, id) = request();
    answer(&mut conversation, &mut citations, &id, "md");
    prepare(&mut conversation, &citations);
    let kept = conversation.kept();
    let mut restored = Conversation::restored(&kept).unwrap();
    assert!(restored.asked_ids().is_empty());
    assert!(restored.open().is_empty());
    assert!(restored.shown.is_none());
    assert_eq!(restored.written_at, 0);
    assert!(!restored.preparation_continues(&citations, 1));
    assert_eq!(
        restored.authorization_text(&citations, "u1"),
        conversation.authorization_text(&citations, "u1")
    );
    prepare(&mut restored, &citations);
    assert!(
        restored
            .propose(&citations, &args(), ("S1".into(), vec![]))
            .unwrap()
            .is_some()
    );
    let mut after = Conversation::restored(&restored.kept()).unwrap();
    prepare(&mut after, &citations);
    assert!(
        after
            .propose(&citations, &args(), ("S1".into(), vec![]))
            .is_err()
    );
}

#[test]
fn a_new_request_discards_the_previous_continuation_receipts() {
    let (mut conversation, mut citations, id) = request();
    answer(&mut conversation, &mut citations, &id, "md");
    citations.record(7, Some(("u3".into(), "Un autre workflow".into())));
    assert!(
        !conversation
            .replace(
                &citations,
                &json!({"message": "u3", "excerpt": "Un autre workflow"})
            )
            .is_error
    );
    assert!(conversation.continuations.is_empty());
    assert_eq!(conversation.authorization_text(&citations, "u1"), None);
}

#[test]
fn the_final_consent_reading_sees_a_selected_withdrawal_of_run() {
    let (mut conversation, mut citations, id) = request();
    conversation.asked[0].options = vec![Offer::new("save", "Enregistre sans lancer", false)];
    answer(&mut conversation, &mut citations, &id, "save");
    prepare(&mut conversation, &citations);
    let text = conversation.authorization_text(&citations, "u1").unwrap();
    let acts = Acts {
        save: true,
        run: true,
        revised: true,
    };
    let read = |prompt: &str| {
        assert!(prompt.contains(MANDATE));
        assert!(prompt.contains("Enregistre sans lancer"));
        Some("change_then_save".to_owned())
    };
    assert!(!Pick::consented(&text, acts, id.clone(), read));
    assert!(Pick::consented(
        &text,
        Acts { run: false, ..acts },
        id,
        read
    ));
}

#[test]
fn choosing_a_format_does_not_turn_an_initial_refusal_into_authority() {
    let (mut conversation, mut citations, id) = request();
    let instruction = "Prépare le workflow mais ne sauvegarde et ne lance rien";
    citations.record(1, Some(("u1".into(), instruction.into())));
    answer(&mut conversation, &mut citations, &id, "csv");
    prepare(&mut conversation, &citations);
    let text = conversation.authorization_text(&citations, "u1").unwrap();
    let acts = Acts {
        save: true,
        run: true,
        revised: true,
    };
    assert!(!Pick::consented(&text, acts, id, |prompt| {
        assert!(prompt.contains(instruction));
        assert!(prompt.contains("Tableau CSV"));
        Some("neither".to_owned())
    }));
}

#[test]
fn an_initial_mandate_survives_a_proposed_choice_then_a_rewritten_candidate() {
    let (mut conversation, mut citations, id) = request();
    let mut choice = conversation.asked.remove(0);
    choice.options[0].recommended = true;
    prepare(&mut conversation, &citations);
    assert!(
        conversation
            .propose(&citations, &args(), ("S1".into(), vec![choice]))
            .is_err(),
        "the choices have not been seen yet"
    );
    assert_eq!(conversation.consumed_through, 0);
    answer(&mut conversation, &mut citations, &id, "csv");
    assert!(conversation.open().is_empty());
    let before_rewrite = conversation.kept();
    // The host must not apply a candidate still carrying the old default.
    let shown = conversation.shown.clone();
    assert!(
        conversation
            .propose(&citations, &args(), ("S1".into(), vec![]))
            .is_err()
    );
    // Continue from the actual pre-proposal state: an unsuccessful probe above must not
    // stand in for the real candidate rewrite that the author performs.
    let mut revised = Conversation::restored(&before_rewrite).unwrap();
    revised.shown = shown;
    prepare(&mut revised, &citations);
    assert_eq!(
        revised.propose(&citations, &args(), ("clarified-scope".into(), vec![])),
        Ok(Some(Acts {
            save: true,
            run: true,
            revised: true
        }))
    );
    let text = revised.authorization_text(&citations, "u1").unwrap();
    assert!(text.contains("Tableau CSV") && !text.contains("Markdown"));
    let mut reopened = Conversation::restored(&revised.kept()).unwrap();
    prepare(&mut reopened, &citations);
    assert!(
        reopened
            .propose(&citations, &args(), ("clarified-scope".into(), vec![]))
            .is_err()
    );
}

#[test]
fn a_stop_after_a_proposed_choice_invalidates_the_initial_mandate() {
    let (mut conversation, mut citations, id) = request();
    let choice = conversation.asked.remove(0);
    prepare(&mut conversation, &citations);
    assert!(
        conversation
            .propose(&citations, &args(), ("S1".into(), vec![choice]))
            .is_err()
    );
    answer(&mut conversation, &mut citations, &id, "csv");
    citations.record(
        7,
        Some(("u3".into(), "Ne sauvegarde et ne lance rien".into())),
    );
    prepare(&mut conversation, &citations);
    assert!(
        conversation
            .propose(&citations, &args(), ("S2".into(), vec![]))
            .is_err()
    );
    assert_eq!(conversation.authorization_text(&citations, "u1"), None);
}

/// The words a label names continue as that offer. The native delegation act continues as
/// what it binds, the one recommended offer, marked delegated, whatever the words it records
/// would name; with no single recommendation it continues nothing.
#[test]
fn the_delegation_act_continues_as_the_recommendation_it_binds() {
    let asker = Arc::new(Incarnation);
    let question = |recommended: bool| {
        let id = QuestionId::new("source".into(), &asker);
        let offer = |key: &str, label: &str, recommended: bool| {
            Offer::new(key, label, recommended).with_values(vec![OfferValue::new(
                ValueRole::Value,
                key,
                None,
            )])
        };
        AskedQuestion::new(id, "source", "Which source?").with_options(
            vec![
                offer("other", "I leave the choice to Nika", false),
                offer("recommended", "Recommended source", recommended),
            ],
            true,
            false,
        )
    };
    let words = "I leave the choice to Nika.";
    let meaning = |delegated: bool, recommended: bool| {
        let mut conversation = Conversation::default();
        let shown = question(recommended);
        let id = shown.id.clone();
        conversation.asked.push(shown);
        if delegated {
            conversation.delegated_one_by(&id, "u2", words);
        } else {
            conversation.answered_one_by(&id, "u2", words, &BTreeMap::new());
        }
        conversation
            .continuations
            .get("u2")
            .map(|receipt| receipt.meaning.clone())
    };
    let by_words = meaning(false, true).unwrap();
    assert_eq!(by_words["selected_offers"][0]["key"], "other", "{by_words}");
    assert!(by_words.get("delegated").is_none(), "{by_words}");
    let by_act = meaning(true, true).unwrap();
    assert_eq!(
        by_act["selected_offers"][0]["key"], "recommended",
        "{by_act}"
    );
    assert_eq!(by_act["selected_offers"].as_array().unwrap().len(), 1);
    assert_eq!(by_act["delegated"], true, "{by_act}");
    assert_eq!(
        meaning(true, false),
        None,
        "no recommendation: the author chooses"
    );
}

mod citation;
mod shown;
