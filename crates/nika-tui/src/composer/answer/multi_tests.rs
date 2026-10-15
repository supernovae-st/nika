// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::panic)]

use super::*;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn question(witness: &str) -> Waiting {
    Waiting::asked(
        "sources",
        Asked::new(
            "Which sources?",
            "",
            true,
            Shape::Multiple(vec![
                Offer::new("a\"b", "First"),
                Offer::new("é/rss", "Second"),
                Offer::new("third", "Third"),
            ]),
            witness,
            7,
        ),
    )
}

fn composer(waiting: &Waiting) -> Composer {
    let mut composer = Composer::new();
    composer.set_focused(true);
    composer.follow(waiting);
    composer
}

#[test]
fn a_proposal_revision_clears_selection_even_with_identical_question_keys_and_offers() {
    let Waiting::QuestionDocument { key: name, asked } = question("composite-one") else {
        panic!("question")
    };
    let initial = Waiting::proposal_choice("proposal-one", name.clone(), asked.clone());
    let revised = Waiting::proposal_choice("proposal-two", name, asked);
    let mut composer = composer(&initial);
    composer.select_offer(&initial, 0);
    assert_eq!(composer.checked_count(&initial), 1);
    composer.follow(&revised);
    assert_eq!(composer.checked_count(&revised), 0);
    assert_eq!(composer.offer_selected(&revised), None);
    assert_eq!(composer.submit_choices(&revised), Offered::Moved);
}

#[test]
fn checked_answers_require_submit_and_send_exact_keys_once_in_offer_order() {
    let waiting = question("w1");
    let mut composer = composer(&waiting);
    composer.paste("ordinary draft");
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Enter)),
        Offered::Moved
    );
    assert_eq!(composer.checked_count(&waiting), 0);
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Down)),
        Offered::Moved
    );
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Down)),
        Offered::Moved
    );
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Char(' '))),
        Offered::Moved
    );
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Up)),
        Offered::Moved
    );
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Enter)),
        Offered::Moved
    );
    assert_eq!(composer.checked_count(&waiting), 2);
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Tab)),
        Offered::Moved
    );
    assert_eq!(composer.offer_selected(&waiting), Some(3));
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Enter)),
        Offered::Answer(r#"["a\"b","é/rss"]"#.into())
    );
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Enter)),
        Offered::Pass
    );
    assert_eq!(composer.submit_choices(&waiting), Offered::Pass);
    assert_eq!(composer.text(), "ordinary draft");
    assert_eq!(composer.answer_returned(false), Some(Sent::Offer));
    assert_eq!(composer.checked_count(&waiting), 0);
}

#[test]
fn submit_without_checks_does_nothing_and_space_can_remove_a_check() {
    let waiting = question("w1");
    let mut composer = composer(&waiting);
    assert_eq!(composer.submit_choices(&waiting), Offered::Moved);
    assert!(composer.select_offer(&waiting, 1));
    assert_eq!(composer.checked_count(&waiting), 1);
    assert!(composer.select_offer(&waiting, 1));
    assert_eq!(composer.checked_count(&waiting), 0);
    assert_eq!(composer.submit_choices(&waiting), Offered::Moved);
}

#[test]
fn changed_requirement_options_shape_permissions_or_session_clear_pending_checks() {
    let waiting = question("w1");
    let mut base = composer(&waiting);
    base.select_offer(&waiting, 1);
    base.paste_own("keep these words");
    let current = question("w2");
    base.follow(&current);
    assert_eq!(base.checked_count(&current), 1);
    assert_eq!(base.own_reply(), "keep these words");
    for change in 0..6 {
        let mut changed = question("w3");
        if let Waiting::QuestionDocument { asked, .. } = &mut changed {
            match change {
                0 => asked.label = "Which excluded sources?".into(),
                1 => asked.why = "a changed requirement".into(),
                2 => asked.epoch += 1,
                3 => asked.free_text = false,
                4 => {
                    if let Shape::Multiple(offers) = &mut asked.shape {
                        offers.swap(0, 1);
                    }
                }
                _ => {
                    if let Shape::Multiple(offers) = &asked.shape {
                        asked.shape = Shape::Choice(offers.clone());
                    }
                }
            }
        }
        let mut composer = composer(&waiting);
        composer.select_offer(&waiting, 1);
        composer.paste_own("not for the new question");
        composer.follow(&changed);
        assert_eq!(composer.checked_count(&changed), 0, "{change}");
        assert_eq!(composer.offer_selected(&changed), None, "{change}");
        assert_eq!(composer.own_reply(), "", "{change}");
    }
}

#[test]
fn rejected_submission_retains_checks_but_an_accepted_one_spends_them() {
    let waiting = question("w1");
    let mut composer = composer(&waiting);
    composer.select_offer(&waiting, 2);
    assert_eq!(
        composer.submit_choices(&waiting),
        Offered::Answer(r#"["third"]"#.into())
    );
    assert_eq!(composer.answer_returned(true), Some(Sent::Offer));
    composer.follow(&question("w2"));
    assert!(composer.offer_checked(&question("w2"), 2));
    assert_eq!(
        composer.submit_choices(&question("w2")),
        Offered::Answer(r#"["third"]"#.into())
    );
    assert_eq!(composer.answer_returned(false), Some(Sent::Offer));
    composer.follow(&question("w3"));
    assert_eq!(composer.checked_count(&question("w3")), 0);
}

#[test]
fn free_text_stays_exact_and_closed_choices_never_accept_hidden_text() {
    let waiting = question("w1");
    let mut composer = composer(&waiting);
    composer.select_offer(&waiting, 0);
    composer.paste_own("Use my own feed");
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Char(' '))),
        Offered::Pass
    );
    assert!(composer.own_key(key(KeyCode::Char(' '))));
    composer.paste_own("instead.");
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Enter)),
        Offered::Answer("Use my own feed instead.".into())
    );
    let mut closed = question("w3");
    if let Waiting::QuestionDocument { asked, .. } = &mut closed {
        asked.free_text = false;
    }
    composer.answer_returned(false);
    composer.follow(&closed);
    assert!(!composer.own_key(key(KeyCode::Char('x'))));
    composer.paste_own("ignored");
    assert_eq!(composer.own_reply(), "");
}

#[test]
fn settings_suspend_and_restore_the_unsubmitted_question_under_its_new_identity() {
    let waiting = question("w1");
    let mut composer = composer(&waiting);
    composer.paste("ordinary draft");
    composer.select_offer(&waiting, 1);
    composer.paste_own("use both if available");
    let settings = Waiting::Selection {
        asked: Asked::new(
            "Choose intelligence",
            "",
            false,
            Shape::Choice(vec![Offer::new("2 deepseek", "DeepSeek")]),
            "settings",
            7,
        ),
    };
    composer.follow(&settings);
    assert_eq!(composer.offer_selected(&settings), None);
    assert_eq!(composer.own_reply(), "");
    assert_eq!(
        composer.offer_key(&settings, key(KeyCode::Down)),
        Offered::Moved
    );
    assert_eq!(
        composer.offer_key(&settings, key(KeyCode::Enter)),
        Offered::Answer("2 deepseek".into())
    );
    composer.answer_returned(false);
    let current = question("w2");
    composer.follow(&current);
    assert_eq!(composer.offer_selected(&current), Some(1));
    assert!(composer.offer_checked(&current, 1));
    assert_eq!(composer.own_reply(), "use both if available");
    assert_eq!(composer.text(), "ordinary draft");
    composer.follow(&settings);
    assert_eq!(
        composer.offer_key(&settings, key(KeyCode::Esc)),
        Offered::Answer("cancel".into())
    );
    assert_eq!(
        composer.offer_key(&settings, key(KeyCode::Esc)),
        Offered::Pass
    );
}

#[test]
fn a_question_named_intelligence_never_inherits_or_loses_a_settings_selection() {
    let shared = Asked::new(
        "Same visible choices",
        "",
        false,
        Shape::Choice(vec![Offer::new("a", "First"), Offer::new("b", "Second")]),
        "q1",
        7,
    );
    let question = Waiting::asked("intelligence", shared.clone());
    let settings = Waiting::selection(shared.clone());
    let mut composer = composer(&question);
    composer.select_offer(&question, 1);
    composer.paste_own("my answer to the question");
    // Even before following the new wait, no borrowed selection paints.
    assert_eq!(composer.offer_selected(&settings), None);
    composer.follow(&settings);
    assert_eq!(composer.offer_selected(&settings), None);
    assert_eq!(composer.own_reply(), "");
    composer.select_offer(&settings, 0);
    assert_eq!(
        composer.offer_key(&settings, key(KeyCode::Enter)),
        Offered::Answer("a".into())
    );
    composer.answer_returned(false);
    let mut renewed = shared;
    renewed.witness = "q2".into();
    let question = Waiting::asked("intelligence", renewed);
    composer.follow(&question);
    assert_eq!(composer.offer_selected(&question), Some(1));
    assert_eq!(composer.own_reply(), "my answer to the question");
}

#[test]
fn space_on_empty_submit_does_not_create_a_hidden_reply_or_disable_checkboxes() {
    for free_text in [true, false] {
        let mut waiting = question("space-submit");
        if let Waiting::QuestionDocument { asked, .. } = &mut waiting {
            asked.free_text = free_text;
        }
        let mut composer = composer(&waiting);
        assert_eq!(
            composer.offer_key(&waiting, key(KeyCode::Tab)),
            Offered::Moved
        );
        assert_eq!(composer.offer_selected(&waiting), Some(3));
        assert_eq!(
            composer.offer_key(&waiting, key(KeyCode::Enter)),
            Offered::Moved
        );
        assert_eq!(
            composer.offer_key(&waiting, key(KeyCode::Char(' '))),
            Offered::Moved
        );
        assert_eq!(composer.own_reply(), "");
        assert_eq!(composer.checked_count(&waiting), 0);
        assert_eq!(
            composer.offer_key(&waiting, key(KeyCode::Up)),
            Offered::Moved
        );
        assert_eq!(
            composer.offer_key(&waiting, key(KeyCode::Char(' '))),
            Offered::Moved
        );
        assert_eq!(composer.checked_count(&waiting), 1);
        assert_eq!(composer.own_reply(), "");
        assert_eq!(
            composer.submit_choices(&waiting),
            Offered::Answer(r#"["third"]"#.into())
        );
    }
}

#[test]
fn closed_multiple_choices_consume_unfocused_space_without_a_hidden_reply() {
    let mut waiting = question("closed-space");
    if let Waiting::QuestionDocument { asked, .. } = &mut waiting {
        asked.free_text = false;
    }
    let mut composer = composer(&waiting);
    assert_eq!(composer.offer_selected(&waiting), None);
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Char(' '))),
        Offered::Moved
    );
    assert_eq!(composer.own_reply(), "");
    assert_eq!(composer.checked_count(&waiting), 0);
}

#[test]
fn open_free_reply_keeps_leading_and_subsequent_spaces_exact() {
    let waiting = question("free-space");
    let mut composer = composer(&waiting);
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Char(' '))),
        Offered::Pass
    );
    assert!(composer.own_key(key(KeyCode::Char(' '))));
    composer.paste_own("Other");
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Tab)),
        Offered::Moved
    );
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Char(' '))),
        Offered::Pass
    );
    assert!(composer.own_key(key(KeyCode::Char(' '))));
    composer.paste_own("answer");
    assert_eq!(composer.own_reply(), " Other answer");
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Enter)),
        Offered::Answer(" Other answer".into())
    );
}
