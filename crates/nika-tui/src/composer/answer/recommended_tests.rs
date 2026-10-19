// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A question that recommends an option needs no typing: it opens on the
//! recommendation (selected, or checked in a multiple choice) and lists
//! « Let Nika choose » after its offers. A single choice without one opens on
//! its first row; a decision opens on nothing. Nothing is sent before `Enter`.

#![allow(clippy::panic)]

use super::*;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Three sources, `recommended` marking which the author recommends.
fn sources(recommended: &[usize]) -> Vec<Offer> {
    let offers = [
        ("hn", "Hacker News · news.ycombinator.com"),
        ("tc", "TechCrunch · techcrunch.com/feed"),
        ("verge", "The Verge · theverge.com/rss"),
    ];
    (offers.iter().enumerate())
        .map(|(at, (key, label))| {
            let offer = Offer::new(*key, *label);
            if recommended.contains(&at) {
                offer.recommended()
            } else {
                offer
            }
        })
        .collect()
}

fn question(shape: Shape) -> Waiting {
    Waiting::asked(
        "source",
        Asked::new("Which source?", "", false, shape, "3:q", 3),
    )
}

fn composer(waiting: &Waiting) -> Composer {
    let mut composer = Composer::new();
    composer.set_focused(true);
    composer.follow(waiting);
    composer
}

/// The one recommendation is selected when the question opens, so `Enter`
/// alone accepts it; nothing was sent before that `Enter`.
#[test]
fn a_single_choice_opens_on_its_recommendation_and_enter_accepts_it() {
    let waiting = question(Shape::Choice(sources(&[1])));
    let mut composer = composer(&waiting);
    assert_eq!(composer.offer_selected(&waiting), Some(1));
    assert!(composer.offers_armed(&waiting), "selected, nothing sent");
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Enter)),
        Offered::Answer("tc".into())
    );
    // Two recommendations name no single default: the list opens on its first
    // row, as a question without one does, so `Down` reaches the second.
    let two = question(Shape::Choice(sources(&[1, 2])));
    assert_eq!(self::composer(&two).offer_selected(&two), Some(0));
    let none = question(Shape::Choice(sources(&[])));
    let mut plain = self::composer(&none);
    assert_eq!(plain.offer_selected(&none), Some(0));
    assert_eq!(plain.offer_key(&none, key(KeyCode::Down)), Offered::Moved);
    assert_eq!(
        plain.offer_key(&none, key(KeyCode::Enter)),
        Offered::Answer("tc".into()),
        "Down then Enter applies the second row"
    );
}

/// A multiple choice opens with every recommendation checked, the cursor
/// nowhere; `Enter` validates exactly that set.
#[test]
fn a_multiple_choice_opens_with_its_recommendations_checked() {
    let waiting = question(Shape::Multiple(sources(&[0, 2])));
    let mut composer = composer(&waiting);
    assert_eq!(composer.checked_count(&waiting), 2);
    assert!(composer.offer_checked(&waiting, 0) && composer.offer_checked(&waiting, 2));
    assert_eq!(composer.offer_selected(&waiting), None);
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Enter)),
        Offered::Answer(r#"["hn","verge"]"#.into())
    );
}

/// « Let Nika choose » follows the offers (before a multiple choice's Submit),
/// leaves the choice to Nika on `Enter` (an act, never words), and Space on
/// it checks nothing.
#[test]
fn let_nika_choose_follows_the_offers_and_sends_the_delegation() {
    let single = question(Shape::Choice(sources(&[1])));
    let mut composer = composer(&single);
    assert_eq!(composer.offer_rows(&single), [0, 1, 2, 4]);
    composer.offer_key(&single, key(KeyCode::End));
    assert_eq!(composer.offer_selected(&single), Some(4));
    assert_eq!(
        composer.offer_key(&single, key(KeyCode::Enter)),
        Offered::Delegate
    );
    assert_eq!(composer.answer_returned(false), Some(Sent::Offer));
    let multiple = question(Shape::Multiple(sources(&[0])));
    let mut composer = self::composer(&multiple);
    assert_eq!(composer.offer_rows(&multiple), [0, 1, 2, 4, 3]);
    composer.offer_key(&multiple, key(KeyCode::End));
    composer.offer_key(&multiple, key(KeyCode::Up));
    assert_eq!(composer.offer_selected(&multiple), Some(4));
    assert_eq!(
        composer.offer_key(&multiple, key(KeyCode::Char(' '))),
        Offered::Moved
    );
    assert_eq!(composer.checked_count(&multiple), 1, "Space checks nothing");
    assert_eq!(
        composer.offer_key(&multiple, key(KeyCode::Enter)),
        Offered::Delegate
    );
}

/// An offer whose label reads as the delegation words is an offer like any
/// other: `Enter` on it sends its own key. Only the « Let Nika choose » row
/// leaves the choice to Nika, and both keep their selection when refused.
#[test]
fn a_label_reading_as_the_delegation_is_still_its_own_offer() {
    let offers = vec![
        Offer::new("other", "I leave the choice to Nika"),
        Offer::new("recommended", "Recommended source").recommended(),
    ];
    let waiting = question(Shape::Choice(offers));
    let mut composer = composer(&waiting);
    assert_eq!(composer.offer_rows(&waiting), [0, 1, 3]);
    composer.offer_key(&waiting, key(KeyCode::Home));
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Enter)),
        Offered::Answer("other".into())
    );
    assert_eq!(composer.answer_returned(true), Some(Sent::Offer));
    composer.offer_key(&waiting, key(KeyCode::End));
    assert_eq!(composer.offer_selected(&waiting), Some(3));
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Enter)),
        Offered::Delegate
    );
    assert_eq!(composer.answer_returned(true), Some(Sent::Offer));
}

/// Words typed in the own field go first, on « Let Nika choose » too.
#[test]
fn an_own_reply_goes_before_the_delegation() {
    let waiting = question(Shape::Choice(sources(&[1])));
    let mut composer = composer(&waiting);
    composer.offer_key(&waiting, key(KeyCode::End));
    composer.paste_own("the blog of YC");
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Enter)),
        Offered::Answer("the blog of YC".into())
    );
}

/// Only a question the author asked opens on its recommendation and offers
/// the delegation. A decision and the knowledge choice open on nothing,
/// whatever their offers say; a setting opens on its own first row.
#[test]
fn a_decision_opens_on_nothing_and_a_setting_on_its_first_row() {
    let shape = Shape::Choice(sources(&[1]));
    let asked = Asked::new("Which?", "", false, shape, "3:x", 3);
    let decision = Waiting::Action {
        key: "save".into(),
        asked: asked.clone(),
    };
    for (waiting, opens) in [
        (decision, None),
        (Waiting::selection(asked.clone()), Some(0)),
        (Waiting::knowledge(asked), None),
    ] {
        let composer = composer(&waiting);
        assert_eq!(composer.offer_selected(&waiting), opens, "{waiting:?}");
        assert_eq!(composer.offer_rows(&waiting), [0, 1, 2], "{waiting:?}");
    }
    let plain = question(Shape::Choice(sources(&[])));
    assert_eq!(composer(&plain).offer_rows(&plain), [0, 1, 2]);
    // A proposal's choice is the author's question too.
    let Waiting::QuestionDocument { key, asked } = question(Shape::Choice(sources(&[2]))) else {
        panic!("question")
    };
    let open = Waiting::proposal_choice("proposal-1", key, asked);
    let composer = composer(&open);
    assert_eq!(composer.offer_selected(&open), Some(2));
    assert_eq!(composer.offer_rows(&open), [0, 1, 2, 4]);
}

/// `Ctrl+A` checks every answer of a multiple choice, then none when all are;
/// it sends nothing, and once words are typed it is the field's own key.
#[test]
fn ctrl_a_checks_every_answer_then_none() {
    let waiting = question(Shape::Multiple(sources(&[1])));
    let mut composer = composer(&waiting);
    let all = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
    assert_eq!(composer.offer_key(&waiting, all), Offered::Moved);
    assert_eq!(composer.offers_checked(&waiting), [0, 1, 2]);
    assert!(composer.offers_armed(&waiting), "nothing sent");
    assert_eq!(composer.offer_key(&waiting, all), Offered::Moved);
    assert_eq!(composer.checked_count(&waiting), 0, "all were: none now");
    composer.paste_own("my own");
    assert_eq!(
        composer.offer_key(&waiting, all),
        Offered::Pass,
        "a word key"
    );
    let single = question(Shape::Choice(sources(&[1])));
    let mut composer = self::composer(&single);
    assert_eq!(composer.offer_key(&single, all), Offered::Pass, "no boxes");
}

/// With nothing typed, a digit moves the selection to that offer of a
/// question and sends nothing; a digit past the offers, or any digit once
/// words are typed, is a word.
#[test]
fn a_digit_moves_the_selection_until_words_are_typed() {
    for shape in [Shape::Choice(sources(&[1])), Shape::Multiple(sources(&[1]))] {
        let waiting = question(shape);
        let mut composer = composer(&waiting);
        assert_eq!(
            composer.offer_key(&waiting, key(KeyCode::Char('3'))),
            Offered::Moved
        );
        assert_eq!(composer.offer_selected(&waiting), Some(2));
        assert!(composer.offers_armed(&waiting), "a digit sends nothing");
        let checked = composer.checked_count(&waiting);
        assert_eq!(
            composer.offer_key(&waiting, key(KeyCode::Char('1'))),
            Offered::Moved
        );
        assert_eq!(composer.offer_selected(&waiting), Some(0));
        assert_eq!(composer.checked_count(&waiting), checked, "nothing toggled");
        assert_eq!(
            composer.offer_key(&waiting, key(KeyCode::Char('4'))),
            Offered::Pass
        );
        composer.paste_own("2");
        assert_eq!(
            composer.offer_key(&waiting, key(KeyCode::Char('2'))),
            Offered::Pass,
            "once typed, digits are words"
        );
        assert_eq!(composer.offer_selected(&waiting), Some(0));
    }
}

/// Digits never select in a settings list: they start the expert route
/// there, as words.
#[test]
fn digits_never_select_in_a_settings_list() {
    let asked = Asked::new("Which?", "", false, Shape::Choice(sources(&[])), "3:x", 3);
    let waiting = Waiting::selection(asked);
    let mut composer = composer(&waiting);
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Char('2'))),
        Offered::Pass
    );
    assert_eq!(composer.offer_selected(&waiting), Some(0), "unmoved");
}

/// A decision (Save · Save and run once · Discard · Inspect, a missed run's
/// handling) opens on nothing selected; a digit moves the selection to that
/// act and never acts; only `Enter` does, once. Once words are typed beside
/// a decision that takes them, digits are words.
#[test]
fn a_digit_selects_a_decisions_act_and_never_acts() {
    let offers = vec![
        Offer::new("yes", "Save workflow"),
        Offer::new("save & run", "Save and run once"),
        Offer::new("no", "Discard this proposal"),
        Offer::new("/show", "Inspect the exact changes"),
    ];
    let label = "What should happen to this reviewed proposal?";
    let asked = Asked::new(label, "", true, Shape::Choice(offers), "action:7", 7);
    let decision = Waiting::action("save", asked);
    let mut composer = composer(&decision);
    assert_eq!(composer.offer_selected(&decision), None, "no preselection");
    for (digit, at) in [('2', 1), ('4', 3), ('1', 0)] {
        assert_eq!(
            composer.offer_key(&decision, key(KeyCode::Char(digit))),
            Offered::Moved,
            "{digit}"
        );
        assert_eq!(composer.offer_selected(&decision), Some(at), "{digit}");
        assert!(composer.offers_armed(&decision), "{digit} never acts");
    }
    assert_eq!(
        composer.offer_key(&decision, key(KeyCode::Char('5'))),
        Offered::Pass,
        "past the acts"
    );
    composer.offer_key(&decision, key(KeyCode::Char('2')));
    assert_eq!(
        composer.offer_key(&decision, key(KeyCode::Enter)),
        Offered::Answer("save & run".into()),
        "Enter acts, once"
    );
    let missed = vec![
        Offer::new("1", "Run once when it comes back"),
        Offer::new("2", "Skip the missed run"),
        Offer::new("3", "Replay every missed occurrence"),
        Offer::new("cancel", "Cancel schedule setup"),
    ];
    let mut asked = Asked::new("If missed?", "", true, Shape::Choice(missed), "action:8", 8);
    asked.free_text = false;
    let activation = Waiting::action("project.missed", asked);
    let mut composer = self::composer(&activation);
    assert_eq!(
        composer.offer_key(&activation, key(KeyCode::Char('3'))),
        Offered::Moved
    );
    assert_eq!(composer.offer_selected(&activation), Some(2));
    assert!(
        composer.offers_armed(&activation),
        "the third act is not done"
    );
}
