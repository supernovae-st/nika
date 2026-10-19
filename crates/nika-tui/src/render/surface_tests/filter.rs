// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A settings list narrowed by typed words, as drawn: only the matching rows
//! paint, the title counts them, a press on a painted row lands on that very
//! offer (its own index, never its place in the narrowed list), and a filter
//! matching nothing says what `Enter` does with the words.

use super::*;

/// A long settings list: `count` models, every seventh one a `sonnet`.
fn catalogue(count: usize) -> Waiting {
    let offers = (0..count)
        .map(|n| {
            let name = if n % 7 == 3 { "sonnet" } else { "model" };
            Offer::new(format!("2 p/{name}-{n}"), format!("{name} {n}"))
        })
        .collect();
    Waiting::selection(Asked::new(
        "Choose a model from p",
        "",
        false,
        Shape::Choice(offers),
        "9:intelligence",
        9,
    ))
}

/// Where a search also finds rows the list keeps out of view (every model under every
/// connection), the title counts the matches alone: never « 1 of 657 match » over a list of a
/// few connections (live gauntlet r08 B21).
#[test]
fn a_search_over_rows_out_of_view_counts_its_matches_alone() {
    let mut offers = vec![
        Offer::new("/intelligence models 2 xai", "xai · API · 2 models"),
        Offer::new("/intelligence models 2 openai", "openai · API · 1 model"),
    ];
    offers.extend(
        ["2 xai/grok-4", "2 xai/grok-4.7", "2 openai/gpt-6"]
            .map(|key| Offer::new(key, key.trim_start_matches("2 ")).found_only()),
    );
    let waiting = Waiting::selection(Asked::new(
        "Choose an AI connection",
        "",
        false,
        Shape::Choice(offers),
        "9:intelligence",
        9,
    ));
    let state = state(Presentation::Focus, (100, 30), waiting, false);
    let mut composer = composer(&state.waiting, "");
    composer.paste_own("grok");
    let buffer = draw(&state, &composer);
    let rows: Vec<String> = (0..30).map(|y| row(&buffer, y)).collect();
    assert!(
        rows.iter().any(|r| r.contains("Intelligence · 2 matches")),
        "{rows:#?}"
    );
    assert!(!rows.iter().any(|r| r.contains(" of 5 match")), "{rows:#?}");
}

/// A long list's title pages over its choices: the rows that move within the list (its older
/// models, every connection) are no choice it counts, so « 1-… of 60 » names the 60 models
/// the connection's row named (live gauntlet, 16 October: « 1-50 of 280 » under « 375 models »).
#[test]
fn a_long_lists_title_counts_its_choices_alone() {
    let mut offers: Vec<Offer> = (0..60)
        .map(|n| Offer::new(format!("2 p/model-{n}"), format!("model {n}")))
        .collect();
    offers.push(Offer::new("/intelligence older 2 p", "Older models (5)…").aside());
    offers.push(Offer::new("/intelligence connections", "All connections").aside());
    let asked = Asked::new(
        "Choose a model from p",
        "",
        false,
        Shape::Choice(offers),
        "9:intelligence",
        9,
    );
    let state = state(
        Presentation::Focus,
        (100, 30),
        Waiting::selection(asked),
        false,
    );
    let composer = composer(&state.waiting, "");
    let buffer = draw(&state, &composer);
    let rows: Vec<String> = (0..30).map(|y| row(&buffer, y)).collect();
    let title = (rows.iter())
        .find(|r| r.contains("Intelligence · 1-"))
        .expect("a paged title");
    assert!(title.contains(" of 60"), "{title}");
    assert!(!title.contains(" of 62"), "{title}");
}

#[test]
fn a_filtered_list_paints_its_matches_and_a_press_lands_on_the_offer_painted() {
    for presentation in [Presentation::Inline, Presentation::Focus] {
        for size in [(80, 24), (120, 40)] {
            let state = state(presentation, size, catalogue(30), false);
            let mut composer = composer(&state.waiting, "");
            composer.paste_own("sonnet");
            let buffer = draw(&state, &composer);
            let rows: Vec<String> = (0..size.1).map(|y| row(&buffer, y)).collect();
            let painted: Vec<&String> = rows.iter().filter(|r| r.contains('(')).collect();
            for n in [3, 10, 17, 24] {
                assert!(
                    painted.iter().any(|r| r.contains(&format!("sonnet {n}"))),
                    "{rows:#?}"
                );
            }
            assert!(!rows.iter().any(|r| r.contains("model 0")), "{rows:#?}");
            assert!(
                rows.iter()
                    .any(|r| r.contains("Intelligence · 4 of 30 match")),
                "{rows:#?}"
            );
            let band = band_of(&state, &composer).expect("band");
            let y = (band.0.y..band.0.bottom())
                .find(|y| row(&buffer, *y).contains("sonnet 17"))
                .expect("a painted match");
            assert_eq!(
                hit(
                    &state,
                    &composer,
                    band,
                    Position::new(band.0.x + 3, y),
                    None
                ),
                Some(Hit::Item(17)),
                "the offer's own index"
            );
        }
    }
}

/// A sentence matching no row is a message, never a choice: the row says
/// `Enter` sends it with the intelligence unchanged, or, under a required
/// choice, keeps it until one is made; it never teaches a typed id.
#[test]
fn a_sentence_matching_nothing_is_a_message_and_never_teaches_an_id() {
    for (required, then) in [
        (
            false,
            "Enter sends it as a message · intelligence unchanged",
        ),
        (true, "Enter keeps it as your message until you choose"),
    ] {
        let mut waiting = catalogue(12);
        if let Waiting::Selection { asked } = &mut waiting {
            asked.mandatory = required;
        }
        let state = state(Presentation::Focus, (120, 24), waiting, false);
        let mut composer = composer(&state.waiting, "");
        composer.paste_own("summarise my inbox every morning");
        let buffer = draw(&state, &composer);
        let rows: Vec<String> = (0..24).map(|y| row(&buffer, y)).collect();
        let said = format!("no row matches « summarise my inbox every morning » · {then}");
        assert!(rows.iter().any(|r| r.contains(&said)), "{rows:#?}");
        assert!(!rows.iter().any(|r| r.contains("<model>")), "{rows:#?}");
    }
}

#[test]
fn a_filter_matching_nothing_says_what_enter_does() {
    for ascii in [false, true] {
        let state = state(Presentation::Focus, (80, 24), catalogue(12), ascii);
        let mut composer = composer(&state.waiting, "");
        composer.paste_own("deepseek/v9");
        let buffer = draw(&state, &composer);
        let rows: Vec<String> = (0..24).map(|y| row(&buffer, y)).collect();
        let quoted = if ascii {
            "\"deepseek/v9\""
        } else {
            "« deepseek/v9 »"
        };
        let said = super::super::own(
            &format!("no row matches {quoted} · Enter sends it as typed"),
            ascii,
        );
        assert!(rows.iter().any(|r| r.contains(&said)), "{rows:#?}");
        assert!(
            rows.iter().any(|r| r.contains("0 of 12 match")),
            "{rows:#?}"
        );
        let band = band_of(&state, &composer).expect("band");
        for y in band.0.y..band.0.bottom() {
            let at = hit(
                &state,
                &composer,
                band,
                Position::new(band.0.x + 3, y),
                None,
            );
            assert_ne!(at.map(|hit| matches!(hit, Hit::Item(_))), Some(true));
        }
        if ascii {
            assert!(rows.iter().all(|r| r.is_ascii()), "{rows:#?}");
        }
    }
}
