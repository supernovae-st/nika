// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

fn citing(message: &str, excerpt: &str) -> Value {
    json!({"acts": ["save", "run"],
        "authorized_by": {"message": message, "excerpt": excerpt}})
}

fn choice(key: &str, options: Vec<Offer>, multi: bool) -> AskedQuestion {
    AskedQuestion::new(
        QuestionId::new(key.into(), &Arc::new(Incarnation)),
        key,
        format!("Choose {key}"),
    )
    .with_options(options, true, multi)
}

fn picked(
    conversation: &mut Conversation,
    citations: &mut Citations,
    question: &AskedQuestion,
    (at, cite, text): (u64, &str, &str),
) {
    citations.record(at, Some((cite.into(), text.into())));
    conversation.answered_one_by(&question.id, cite, text, &BTreeMap::new());
}

const REPORT_MANDATE: &str =
    "Prepare the support report; after my choices save the workflow and produce the report";

fn three_choices() -> (Conversation, Citations, QuestionId) {
    let mut conversation = Conversation::default();
    let mut citations = Citations::default();
    citations.record(1, Some(("u1".into(), REPORT_MANDATE.into())));
    let detail = choice(
        "detail",
        vec![Offer::new("detailed", "Detailed with deviations", false)],
        false,
    );
    conversation.asked.push(detail.clone());
    picked(
        &mut conversation,
        &mut citations,
        &detail,
        (4, "u2", "detailed"),
    );
    let sections = choice(
        "sections",
        vec![
            Offer::new("status", "Open and closed tickets", false),
            Offer::new("times", "Resolution times by category", false),
            Offer::new("priority", "Priority counts", false),
        ],
        true,
    );
    conversation.asked.push(sections.clone());
    picked(
        &mut conversation,
        &mut citations,
        &sections,
        (7, "u3", r#"["status","times"]"#),
    );
    prepare(&mut conversation, &citations);
    let model = choice(
        "model",
        vec![
            Offer::new("flash", "DeepSeek Flash", true),
            Offer::new("other", "Another model", false),
        ],
        false,
    );
    assert!(
        conversation
            .propose(
                &citations,
                &citing("u1", REPORT_MANDATE),
                ("S1".into(), vec![model.clone()])
            )
            .is_err()
    );
    picked(
        &mut conversation,
        &mut citations,
        &model,
        (10, "u4", "flash"),
    );
    prepare(&mut conversation, &citations);
    (conversation, citations, model.id)
}

#[test]
fn citing_the_last_model_pick_keeps_the_original_mandate_and_all_three_choices() {
    let (mut conversation, citations, id) = three_choices();
    assert_eq!(
        conversation.propose(&citations, &citing("u4", "flash"), ("S2".into(), vec![])),
        Ok(Some(Acts {
            save: true,
            run: true,
            revised: true
        }))
    );
    assert_eq!(
        conversation.consumed_through, 1,
        "the mandate, not the model key, is consumed"
    );
    let text = conversation.authorization_text(&citations, "u4").unwrap();
    assert_eq!(
        text,
        conversation.authorization_text(&citations, "u1").unwrap()
    );
    for phrase in [
        REPORT_MANDATE,
        "u2",
        "u3",
        "u4",
        "Detailed with deviations",
        "Open and closed tickets",
        "Resolution times by category",
        "DeepSeek Flash",
    ] {
        assert!(text.contains(phrase), "missing {phrase}: {text}");
    }
    assert!(!text.contains("Priority counts") && !text.contains("Another model"));
    assert!(Pick::consented(
        &text,
        Acts {
            save: true,
            run: true,
            revised: true
        },
        id,
        |prompt| {
            assert!(prompt.contains(REPORT_MANDATE) && prompt.contains("DeepSeek Flash"));
            Some("change_then_save_and_run".into())
        }
    ));
}

#[test]
fn another_bound_pick_cannot_reuse_the_consumed_initial_mandate() {
    let (mut conversation, mut citations, id) = request();
    answer(&mut conversation, &mut citations, &id, "md");
    prepare(&mut conversation, &citations);
    assert!(
        conversation
            .propose(&citations, &citing("u2", "md"), ("S1".into(), vec![]))
            .unwrap()
            .is_some()
    );
    let later = choice(
        "detail",
        vec![Offer::new("long", "Long report", false)],
        false,
    );
    conversation.asked.push(later.clone());
    picked(&mut conversation, &mut citations, &later, (7, "u3", "long"));
    prepare(&mut conversation, &citations);
    assert!(
        conversation
            .propose(&citations, &citing("u3", "long"), ("S1".into(), vec![]))
            .is_err()
    );
    let mut reopened = Conversation::restored(&conversation.kept()).unwrap();
    prepare(&mut reopened, &citations);
    assert!(
        reopened
            .propose(&citations, &citing("u3", "long"), ("S1".into(), vec![]))
            .is_err()
    );
}

#[test]
fn a_selected_withdrawal_reaches_the_consent_reader_even_when_its_key_is_cited() {
    let (mut conversation, mut citations, id) = request();
    conversation.asked[0].options = vec![Offer::new("save", "Save without running", false)];
    answer(&mut conversation, &mut citations, &id, "save");
    prepare(&mut conversation, &citations);
    let text = conversation.authorization_text(&citations, "u2").unwrap();
    assert!(text.contains(MANDATE) && text.contains("Save without running"));
    assert!(!Pick::consented(
        &text,
        Acts {
            save: true,
            run: true,
            revised: true
        },
        id,
        |prompt| {
            assert!(prompt.contains(MANDATE) && prompt.contains("Save without running"));
            Some("change_then_save".into())
        }
    ));
}

#[test]
fn a_later_free_sentence_prevents_a_choice_citation_from_recovering_the_mandate() {
    let (mut conversation, mut citations, id) = request();
    answer(&mut conversation, &mut citations, &id, "md");
    citations.record(7, Some(("u3".into(), "Do not save or run anything".into())));
    prepare(&mut conversation, &citations);
    assert_eq!(conversation.authorization_text(&citations, "u2"), None);
    assert!(
        conversation
            .propose(&citations, &citing("u2", "md"), ("S1".into(), vec![]))
            .is_err()
    );
    assert_eq!(conversation.consumed_through, 0);
}

#[test]
fn a_new_request_starts_its_own_choice_context_without_the_old_mandate() {
    let (mut conversation, mut citations, id) = request();
    answer(&mut conversation, &mut citations, &id, "md");
    let new = "Prepare a different workflow without saving or running";
    citations.record(7, Some(("u3".into(), new.into())));
    assert!(
        !conversation
            .replace(&citations, &json!({"message":"u3", "excerpt":new}))
            .is_error
    );
    let format = choice("new_format", vec![Offer::new("json", "JSON", false)], false);
    conversation.asked.push(format.clone());
    picked(
        &mut conversation,
        &mut citations,
        &format,
        (10, "u4", "json"),
    );
    prepare(&mut conversation, &citations);
    let text = conversation.authorization_text(&citations, "u4").unwrap();
    assert!(text.starts_with("Human instruction (u3):"));
    assert!(text.contains(new) && !text.contains(MANDATE));
    assert!(!Pick::consented(
        &text,
        Acts {
            save: true,
            run: true,
            revised: true
        },
        format.id,
        |_| Some("neither".into())
    ));
}

#[test]
fn a_forged_excerpt_or_changed_bound_answer_never_resolves_to_the_request() {
    let (mut conversation, mut citations, id) = request();
    answer(&mut conversation, &mut citations, &id, "md");
    prepare(&mut conversation, &citations);
    assert!(
        conversation
            .propose(
                &citations,
                &citing("u2", "save and run"),
                ("S1".into(), vec![])
            )
            .is_err()
    );
    citations.record(4, Some(("u2".into(), "md but do not run".into())));
    assert_eq!(conversation.authorization_text(&citations, "u2"), None);
    assert!(
        conversation
            .propose(&citations, &citing("u2", "md"), ("S1".into(), vec![]))
            .is_err()
    );
    assert_eq!(conversation.consumed_through, 0);
}

#[test]
fn a_greeting_before_the_instruction_does_not_hide_its_later_bound_choices() {
    let (mut conversation, mut citations, id) = request();
    citations.record(1, Some(("u1".into(), "Hello".into())));
    citations.record(3, Some(("u2".into(), MANDATE.into())));
    citations.record(4, Some(("u3".into(), "md".into())));
    conversation.answered_one_by(&id, "u3", "md", &BTreeMap::new());
    prepare(&mut conversation, &citations);
    assert_eq!(
        conversation.propose(&citations, &citing("u3", "md"), ("S1".into(), vec![])),
        Ok(Some(Acts {
            save: true,
            run: true,
            revised: true
        }))
    );
    assert_eq!(conversation.consumed_through, 3);
    let text = conversation.authorization_text(&citations, "u3").unwrap();
    assert!(text.starts_with("Human instruction (u2):"));
    assert!(text.contains(MANDATE) && !text.contains("Hello"));
}

#[test]
fn a_withdrawal_before_the_bound_choice_is_the_latest_instruction_not_the_old_mandate() {
    let (mut conversation, mut citations, id) = request();
    let withdrawal = "Do not save or run; just prepare the preview";
    citations.record(3, Some(("u2".into(), withdrawal.into())));
    citations.record(4, Some(("u3".into(), "md".into())));
    conversation.answered_one_by(&id, "u3", "md", &BTreeMap::new());
    prepare(&mut conversation, &citations);
    let text = conversation.authorization_text(&citations, "u3").unwrap();
    assert!(text.starts_with("Human instruction (u2):"));
    assert!(text.contains(withdrawal) && !text.contains(MANDATE));
    assert!(!Pick::consented(
        &text,
        Acts {
            save: true,
            run: true,
            revised: true
        },
        id,
        |prompt| {
            assert!(prompt.contains(withdrawal) && !prompt.contains(MANDATE));
            Some("neither".into())
        }
    ));
}
