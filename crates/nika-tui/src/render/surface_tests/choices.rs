// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A question that needs no typing, as drawn: its recommendation is selected
//! and says so in the quiet column, and « Let Nika choose » follows the
//! offers with what it does, in both glyph columns.

#![allow(clippy::panic)]

use super::*;

/// Which source, `TechCrunch` recommended, as a single or multiple choice.
fn sources(multiple: bool) -> Waiting {
    let offers = vec![
        Offer::new("hn", "Hacker News"),
        Offer::new("tc", "TechCrunch").recommended(),
        Offer::new("verge", "The Verge"),
    ];
    let shape = if multiple {
        Shape::Multiple(offers)
    } else {
        Shape::Choice(offers)
    };
    Waiting::asked(
        "source",
        Asked::new("Which source?", "", false, shape, "2:q", 2),
    )
}

fn rows(state: &UiState, composer: &Composer) -> Vec<String> {
    let buffer = draw(state, composer);
    (0..state.size.1).map(|y| row(&buffer, y)).collect()
}

fn find<'a>(rows: &'a [String], words: &str) -> &'a str {
    rows.iter()
        .find(|row| row.contains(words))
        .unwrap_or_else(|| panic!("no row says {words}: {rows:#?}"))
}

#[test]
fn the_recommendation_is_selected_and_let_nika_choose_follows_the_offers() {
    for ascii in [false, true] {
        for size in [(80, 24), (120, 40)] {
            let state = state(Presentation::Focus, size, sources(false), ascii);
            let composer = composer(&state.waiting, "");
            let rows = rows(&state, &composer);
            let picked = find(&rows, "TechCrunch");
            let (cursor, radio) = if ascii {
                ("> ", "(*)")
            } else {
                ("› ", "(●)")
            };
            assert!(
                picked.contains(cursor) && picked.contains(radio),
                "{picked}"
            );
            assert!(picked.contains("recommended"), "{picked}");
            assert!(!find(&rows, "Hacker News").contains("recommended"));
            let delegate = find(&rows, "Let Nika choose");
            assert!(
                delegate.contains("Nika takes the recommended answer"),
                "{delegate}"
            );
            let place = |words: &str| rows.iter().position(|row| row.contains(words));
            assert!(place("The Verge") < place("Let Nika choose"), "{rows:#?}");
            if ascii {
                assert!(rows.iter().all(|row| row.is_ascii()), "{rows:#?}");
            }
        }
    }
}

#[test]
fn a_multiple_choice_paints_its_recommendation_checked_before_submit() {
    let state = state(Presentation::Focus, (100, 30), sources(true), false);
    let composer = composer(&state.waiting, "");
    let rows = rows(&state, &composer);
    assert!(find(&rows, "TechCrunch").contains("[x]"), "{rows:#?}");
    assert!(find(&rows, "Hacker News").contains("[ ]"), "{rows:#?}");
    let place = |words: &str| rows.iter().position(|row| row.contains(words));
    assert!(place("Let Nika choose") < place("Submit"), "{rows:#?}");
}

/// An offer whose own words already say it is recommended (« recommandé »,
/// « Recommended », « recommandée ») reads it once: no second « recommended »
/// in its quiet column; one that does not still says it.
#[test]
fn a_recommendation_its_label_says_reads_once() {
    for label in [
        "claude-sonnet-5-5 (recommandé)",
        "claude-sonnet-5-5 (Recommended)",
        "la version RECOMMANDÉE",
    ] {
        let offers = vec![
            Offer::new("anthropic/claude-sonnet-5-5", label).recommended(),
            Offer::new("deepseek/deepseek-flash", "deepseek-flash"),
        ];
        let asked = Asked::new("Which model?", "", false, Shape::Choice(offers), "2:m", 2);
        let state = state(
            Presentation::Focus,
            (100, 30),
            Waiting::asked("model", asked),
            false,
        );
        let composer = composer(&state.waiting, "");
        let rows = rows(&state, &composer);
        let row = find(&rows, label);
        assert!(!row.contains("· recommended"), "{label}: {row}");
        assert!(!row.contains(" recommended"), "{label}: {row}");
    }
    let state = state(Presentation::Focus, (100, 30), sources(false), false);
    let composer = composer(&state.waiting, "");
    let rows = rows(&state, &composer);
    assert!(
        find(&rows, "TechCrunch").contains("recommended"),
        "{rows:#?}"
    );
}

/// A decision's selected act is named on the hint row, `Enter acts: …`, and its
/// row is filled even when the list does not hold the keys: the act `Enter`
/// does is never only a mark beside a row. Nothing selected, nothing named.
#[test]
fn the_hint_names_the_act_enter_does() {
    let offers = vec![
        Offer::new("yes", "Save workflow"),
        Offer::new("save & run", "Save and run once"),
        Offer::new("no", "Discard this proposal"),
    ];
    let asked = Asked::new("Save it?", "", true, Shape::Choice(offers), "action:3", 3);
    let save = state(
        Presentation::Focus,
        (100, 30),
        Waiting::action("save", asked),
        false,
    );
    let mut composer = composer(&save.waiting, "");
    let opened = rows(&save, &composer);
    assert!(
        !opened.iter().any(|row| row.contains("Enter acts: ")),
        "{opened:#?}"
    );
    for _ in 0..3 {
        composer.offer_key(&save.waiting, key(KeyCode::Down));
    }
    let chosen = rows(&save, &composer);
    assert!(
        (chosen.iter()).any(|row| row.contains("Enter acts: Discard this proposal")),
        "{chosen:#?}"
    );
    assert!(
        find(&chosen, "Discard").contains("› 3 (●) Discard this proposal"),
        "{chosen:#?}"
    );
}

/// A decision numbers its acts with the digit that selects them, and opens
/// with none selected.
#[test]
fn a_decisions_acts_wear_their_digit_and_none_is_selected() {
    let offers = vec![
        Offer::new("yes", "Save workflow"),
        Offer::new("no", "Discard this proposal"),
    ];
    let asked = Asked::new("Save it?", "", true, Shape::Choice(offers), "action:2", 2);
    let state = state(
        Presentation::Focus,
        (100, 30),
        Waiting::action("save", asked),
        false,
    );
    let composer = composer(&state.waiting, "");
    let rows = rows(&state, &composer);
    assert!(
        find(&rows, "Save workflow").contains("1 ( ) Save workflow"),
        "{rows:#?}"
    );
    assert!(
        find(&rows, "Discard").contains("2 ( ) Discard this proposal"),
        "{rows:#?}"
    );
    assert!(!rows.iter().any(|row| row.contains("(●)")), "{rows:#?}");
}

/// A radio is filled only where `Enter` applies it. A question without a
/// recommendation opens on its first row, filled; once its own reply holds
/// words, `Enter` sends them, so no radio is filled and the cursor stays. A
/// decision's act selected by its digit is filled until words are typed.
#[test]
fn a_radio_is_filled_only_where_enter_applies_it() {
    for ascii in [false, true] {
        let (cursor, filled) = if ascii {
            ("> ", "(*)")
        } else {
            ("› ", "(●)")
        };
        let offers = vec![Offer::new("06:00", "6h00"), Offer::new("07:00", "7h00")];
        let asked = Asked::new("At what time?", "", true, Shape::Choice(offers), "1:h", 1);
        let hour = state(
            Presentation::Focus,
            (100, 30),
            Waiting::asked("hour", asked),
            ascii,
        );
        let mut composer = composer(&hour.waiting, "");
        let opened = rows(&hour, &composer);
        let first = format!("{cursor}1 {filled} 6h00");
        assert!(find(&opened, "6h00").contains(&first), "{opened:#?}");
        assert!(
            find(&opened, "7h00").contains("  2 ( ) 7h00"),
            "{opened:#?}"
        );
        composer.paste_own("vers 7h30");
        let worded = rows(&hour, &composer);
        let hollow = format!("{cursor}1 ( ) 6h00");
        assert!(find(&worded, "6h00").contains(&hollow), "{worded:#?}");
        assert!(
            !worded.iter().any(|row| row.contains(filled)),
            "{worded:#?}"
        );

        let offers = vec![
            Offer::new("yes", "Save workflow"),
            Offer::new("no", "Discard this proposal"),
        ];
        let asked = Asked::new("Save it?", "", true, Shape::Choice(offers), "action:2", 2);
        let save = state(
            Presentation::Focus,
            (100, 30),
            Waiting::action("save", asked),
            ascii,
        );
        let mut composer = self::composer(&save.waiting, "");
        let moved = composer.offer_key(&save.waiting, key(KeyCode::Char('2')));
        assert_eq!(moved, crate::composer::answer::Offered::Moved);
        let chosen = rows(&save, &composer);
        let discard = format!("{cursor}2 {filled} Discard");
        assert!(find(&chosen, "Discard").contains(&discard), "{chosen:#?}");
        composer.paste_own("rename it first");
        let worded = rows(&save, &composer);
        assert!(
            !worded.iter().any(|row| row.contains(filled)),
            "{worded:#?}"
        );
        // Words naming an act fill that act's radio: `Enter` picks it.
        let mut composer = self::composer(&save.waiting, "");
        composer.paste_own("save workflow");
        let named = rows(&save, &composer);
        let saved = format!("  1 {filled} Save workflow");
        assert!(find(&named, "Save workflow").contains(&saved), "{named:#?}");
        let discard = "  2 ( ) Discard this proposal";
        assert!(find(&named, "Discard").contains(discard), "{named:#?}");
    }
}

/// A question numbers its first nine offers with the digit that selects
/// them, and a multiple choice counts its checked answers in its title.
#[test]
fn offers_wear_their_digit_and_a_multiple_choice_counts_in_its_title() {
    let state = state(Presentation::Focus, (100, 30), sources(true), false);
    let mut composer = composer(&state.waiting, "");
    let rows = rows(&state, &composer);
    assert!(
        find(&rows, "Hacker News").contains("1 [ ] Hacker News"),
        "{rows:#?}"
    );
    assert!(
        find(&rows, "The Verge").contains("3 [ ] The Verge"),
        "{rows:#?}"
    );
    assert!(
        find(&rows, "Question").contains("1 of 3 selected"),
        "{rows:#?}"
    );
    let all = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
    composer.offer_key(&state.waiting, all);
    let rows = self::rows(&state, &composer);
    assert!(
        find(&rows, "Question").contains("3 of 3 selected"),
        "{rows:#?}"
    );
    assert!(
        find(&rows, "Submit").contains("Enter validates the checked answers"),
        "{rows:#?}"
    );
}

/// Sources whose labels name a site, then its address after « · ».
fn sites() -> Waiting {
    let offers = vec![
        Offer::new("hn", "Hacker News · news.ycombinator.com"),
        Offer::new("tc", "TechCrunch · techcrunch.com/feed").recommended(),
        Offer::new("blog", "YC blog"),
    ];
    Waiting::asked(
        "source",
        Asked::new("Which source?", "", false, Shape::Choice(offers), "2:s", 2),
    )
}

/// A question's label holding « · » reads its name first and the rest, its
/// address, dimmed in one column aligned over the list; a narrow row wraps the
/// address under the name's indent rather than cut it (live gauntlet 5, r02
/// B6), each fact whole on its row.
#[test]
fn a_sources_address_reads_dim_in_its_own_column() {
    for ascii in [false, true] {
        let state = state(Presentation::Focus, (100, 30), sites(), ascii);
        let composer = composer(&state.waiting, "");
        let buffer = draw(&state, &composer);
        let rows: Vec<String> = (0..30).map(|y| row(&buffer, y)).collect();
        let sep = if ascii { " - " } else { " · " };
        let hn = find(&rows, "Hacker News");
        let tc = find(&rows, "TechCrunch");
        assert!(hn.contains(&format!("{sep}news.ycombinator.com")), "{hn}");
        assert!(
            tc.contains(&format!("{sep}techcrunch.com/feed{sep}recommended")),
            "{tc}"
        );
        let column = |row: &str, words: &str| row.find(words).expect("the address");
        assert_eq!(
            column(hn, "news.ycombinator.com") - column(hn, "Hacker News"),
            column(tc, "techcrunch.com/feed") - column(tc, "TechCrunch"),
            "one column: {rows:#?}"
        );
        assert!(!find(&rows, "YC blog").contains(sep.trim()), "{rows:#?}");
        // The address is dim; the name is not.
        let y = (0..30)
            .find(|y| row(&buffer, *y).contains("Hacker News"))
            .expect("row");
        let x = |words: &str| {
            let text = row(&buffer, y);
            let at = text.find(words).expect("words");
            u16::try_from(text[..at].chars().count()).expect("cells")
        };
        let dim = |x: u16| buffer[(x, y)].modifier.contains(Modifier::DIM);
        assert!(dim(x("news")) && !dim(x("Hacker")), "{ascii}");
        if ascii {
            assert!(hn.is_ascii() && tc.is_ascii(), "{hn}\n{tc}");
        }
    }
    let state = state(Presentation::Focus, (36, 20), sites(), false);
    let composer = composer(&state.waiting, "");
    let rows = rows(&state, &composer);
    let at = (rows.iter())
        .position(|row| row.contains("2 (●) TechCrunch"))
        .expect("its row");
    let cells = |row: &str, words: &str| row.find(words).map(|at| row[..at].chars().count());
    assert_eq!(
        cells(&rows[at + 1], "techcrunch.com/feed"),
        cells(&rows[at], "TechCrunch"),
        "under the name: {rows:#?}"
    );
    assert!(
        rows[at + 2].trim_end().ends_with("recommended"),
        "{rows:#?}"
    );
    assert!(
        rows[at..at + 3].iter().all(|row| !row.contains('…')),
        "{rows:#?}"
    );
}

/// An activation's question is its block's first line: the decision's head
/// says it once, then the line under it, never the question twice in a row.
#[test]
fn an_activation_question_is_said_once_in_its_decision() {
    let sentence = "If this machine is off at 08:00 (Europe/Paris), what should happen?";
    let said = format!(
        "{sentence}\n  this changes what may run later · it does not change the workflow itself"
    );
    let offers = vec![
        Offer::new("1", "Run once when it comes back"),
        Offer::new("2", "Skip the missed run"),
        Offer::new("cancel", "Cancel schedule setup"),
    ];
    let mut asked = Asked::new(sentence, "", true, Shape::Choice(offers), "action:1", 1);
    asked.free_text = true;
    let waiting = Waiting::action("project.missed", asked);
    let mut state = state(Presentation::Focus, (100, 30), waiting, false);
    state.transcript.push(Committed::new(Kind::Question, said));
    let composer = composer(&state.waiting, "");
    let rows = rows(&state, &composer);
    let twice =
        (rows.windows(2)).any(|pair| pair[0].contains(sentence) && pair[1].contains(sentence));
    assert!(!twice, "{rows:#?}");
    let head = rows.iter().position(|row| row.contains("Decision"));
    let below = head.map_or(&rows[..], |at| &rows[at..]);
    assert_eq!(
        below.iter().filter(|row| row.contains(sentence)).count(),
        1,
        "{rows:#?}"
    );
    assert!(
        below
            .iter()
            .any(|row| row.contains("this changes what may run later"))
    );
}

/// A question's choice opens on what Nika said since the person's last line: the surface covers
/// the conversation, so the reply that led to the question reads above it (r08 B9).
#[test]
fn a_choice_opens_on_the_reply_that_led_to_it() {
    let mut state = state(Presentation::Focus, (100, 30), sources(false), false);
    state
        .transcript
        .push(Committed::new(Kind::Human, "digest my notes"));
    state.transcript.push(Committed::new(
        Kind::Reply,
        "J'ai lu tes notes : deux sources sont possibles.",
    ));
    let composer = composer(&state.waiting, "");
    let rows = rows(&state, &composer);
    let said = (rows.iter()).position(|row| row.contains("J'ai lu tes notes"));
    let asked = (rows.iter()).position(|row| row.contains("Which source?"));
    assert!(
        said.zip(asked).is_some_and(|(said, asked)| said < asked),
        "the reply above the question: {rows:#?}"
    );
}
