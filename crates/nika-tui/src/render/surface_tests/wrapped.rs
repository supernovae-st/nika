// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A question's or a decision's offer whose words do not fit the band, as drawn: wrapped whole
//! under its own indent (live gauntlet 5, r02 B6: « …avant d'écrire python-version.txt / re… »),
//! every row of it the offer a press lands on and filled when it is selected; a settings list
//! keeps one row per offer.

#![allow(clippy::panic)]

use super::*;

/// Two long offers around a short one, the way the Session words a question's answers.
fn long_offers() -> Vec<Offer> {
    vec![
        Offer::new(
            "check",
            "Vérifier la version de Python avant d'écrire python-version.txt, puis relancer le contrôle",
        ),
        Offer::new("skip", "Passer"),
        Offer::new(
            "ping",
            "Écrire les fichiers et pinger httpbin pour confirmer que le réseau répond bien",
        ),
    ]
}

/// The rows of the band painted on `state`, top first, with the first row of each offer.
fn painted_offers(state: &UiState, composer: &Composer) -> (Buffer, Vec<String>, Vec<usize>) {
    let buffer = draw(state, composer);
    let rows: Vec<String> = (0..state.size.1).map(|y| row(&buffer, y)).collect();
    let firsts = (1..=3)
        .map(|digit| {
            (rows.iter())
                .position(|row| {
                    ["( )", "(●)", "(*)"]
                        .iter()
                        .any(|glyph| row.contains(&format!("{digit} {glyph}")))
                })
                .unwrap_or_else(|| panic!("offer {digit}: {rows:#?}"))
        })
        .collect();
    (buffer, rows, firsts)
}

/// Every word of a long offer reads on its rows, nothing cut; each of its rows lands a press on
/// it; the selected offer is filled on every row. In both glyph columns, at two widths, under a
/// question (its first answer selected) and a decision (nothing selected, then its first act).
#[test]
fn a_long_offer_wraps_whole_and_every_row_of_it_is_that_offer() {
    let question = || {
        let asked = Asked::new(
            "Que faire ?",
            "",
            true,
            Shape::Choice(long_offers()),
            "q:1",
            1,
        );
        Waiting::asked("next", asked)
    };
    let decision = || {
        let asked = Asked::new(
            "Que faire ?",
            "",
            true,
            Shape::Choice(long_offers()),
            "a:1",
            1,
        );
        Waiting::action("judgment", asked)
    };
    for (waiting, opened) in [(question(), Some(0)), (decision(), None)] {
        for (size, ascii) in [((60, 24), false), ((60, 24), true), ((80, 24), false)] {
            let state = state(Presentation::Focus, size, waiting.clone(), ascii);
            let mut composer = composer(&state.waiting, "");
            let at = format!("{:?} {size:?} ascii={ascii}", state.waiting.prompt());
            assert_eq!(composer.offer_selected(&state.waiting), opened, "{at}");
            select(&mut composer, &state.waiting, 0);
            let (buffer, rows, firsts) = painted_offers(&state, &composer);
            let band = band_of(&state, &composer).expect("the band");
            for (offer, (first, next)) in long_offers().iter().zip(
                firsts
                    .iter()
                    .zip(firsts.iter().skip(1).chain([&usize::MAX])),
            ) {
                let end = (*next).min(*first + 3);
                let said = rows[*first..end].join(" ");
                for word in offer.label.split_whitespace() {
                    assert!(said.contains(word), "{at}: {word:?} in {said:?}");
                }
                assert!(
                    !said.contains('…') && !said.contains("..."),
                    "{at}: {said:?}"
                );
            }
            // Every row from an offer's first to the next one's is that offer.
            for (index, (first, next)) in firsts.iter().zip(firsts.iter().skip(1)).enumerate() {
                for y in *first..*next {
                    let press = Position::new(band.0.x + 1, u16::try_from(y).expect("a row"));
                    let landed = hit(&state, &composer, band, press, None);
                    assert_eq!(landed, Some(Hit::Item(index)), "{at} row {y}: {rows:#?}");
                }
            }
            // The selected offer is filled on its every row, reversed without colour.
            let reversed = |y: usize| {
                let x = u16::try_from(rows[y].chars().count().saturating_sub(1)).expect("a cell");
                let y = u16::try_from(y).expect("a row");
                buffer[(x, y)].modifier.contains(Modifier::REVERSED)
            };
            assert!(
                (firsts[0]..firsts[1]).all(reversed),
                "{at}: the selection's every row: {rows:#?}"
            );
            assert!(
                firsts[1] - firsts[0] > 1,
                "{at}: the first offer wrapped: {rows:#?}"
            );
        }
    }
}

/// A settings list keeps one row per offer, its long label cut with a visible mark: a model's
/// facts are a list to scan, never paragraphs.
#[test]
fn a_settings_list_keeps_one_row_per_offer() {
    let asked = Asked::new("Choose", "", true, Shape::Choice(long_offers()), "s:1", 1);
    let waiting = Waiting::selection(asked);
    let state = state(Presentation::Focus, (60, 24), waiting, false);
    let composer = composer(&state.waiting, "");
    let rows = rows_of(&state, &composer);
    let first = (rows.iter())
        .position(|row| row.contains("Vérifier la version"))
        .expect("its row");
    assert!(rows[first].ends_with('…'), "{rows:#?}");
    assert!(rows[first + 1].contains("Passer"), "{rows:#?}");
}

fn rows_of(state: &UiState, composer: &Composer) -> Vec<String> {
    let buffer = draw(state, composer);
    (0..state.size.1).map(|y| row(&buffer, y)).collect()
}
