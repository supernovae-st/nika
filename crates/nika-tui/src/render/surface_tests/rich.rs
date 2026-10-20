// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Offers with a note (a model's prices, context window and marks) read it in
//! one quiet column after their labels, aligned over the list, cut before the
//! label; an offer without one keeps its label alone.

#![allow(clippy::panic)]

use super::*;
use unicode_width::UnicodeWidthStr;

fn models() -> Waiting {
    let offers = vec![
        Offer::new("2 anthropic/claude-sonnet-5-5", "Claude Sonnet 5.5")
            .noted("current · $2 in / $10 out per MTok · 1M context")
            .in_use(),
        Offer::new("2 deepseek/deepseek-flash", "DeepSeek Flash")
            .noted("$0.30 in / $1.20 out per MTok · 1M context"),
        Offer::new("/intelligence", "All connections"),
    ];
    Waiting::selection(Asked::new(
        "Choose a model",
        "",
        false,
        Shape::Choice(offers),
        "4:intelligence",
        4,
    ))
}

/// A settings list's filled radio is the value in use, wherever the cursor stands: moved to
/// another model, the cursor says where `Enter` goes and the radio stays on the current one
/// (live gauntlet r02 B2, r06 B9: « › (●) GPT-6 Astra » while Claude was in use).
#[test]
fn a_settings_radio_marks_the_value_in_use_and_the_cursor_where_enter_goes() {
    let offers = vec![
        Offer::new("2 openai/gpt-6-astra", "GPT-6 Astra").noted("$3 in / $12 out per MTok"),
        Offer::new("2 anthropic/claude-sonnet-5-5", "Claude Sonnet 5.5")
            .noted("current · $2 in / $10 out per MTok")
            .in_use(),
    ];
    let waiting = Waiting::selection(Asked::new(
        "Choose a model",
        "",
        false,
        Shape::Choice(offers),
        "5:intelligence",
        5,
    ));
    let state = state(Presentation::Focus, (100, 24), waiting.clone(), false);
    let mut composer = composer(&waiting, "");
    select(&mut composer, &waiting, 0);
    let buffer = draw(&state, &composer);
    let rows: Vec<String> = (0..24).map(|y| row(&buffer, y)).collect();
    assert!(
        rows.iter().any(|r| r.contains("› ( ) GPT-6 Astra")),
        "the cursor, an empty radio: {rows:#?}"
    );
    assert!(
        rows.iter().any(|r| r.contains("  (●) Claude Sonnet 5.5")),
        "the value in use keeps its radio: {rows:#?}"
    );
}

#[test]
fn notes_read_in_one_aligned_quiet_column_after_the_labels() {
    for ascii in [false, true] {
        let state = state(Presentation::Focus, (100, 24), models(), ascii);
        let composer = composer(&state.waiting, "");
        let buffer = draw(&state, &composer);
        let rows: Vec<String> = (0..24).map(|y| row(&buffer, y)).collect();
        let sep = if ascii { " - " } else { " · " };
        // The list opens on its first row, the one `Enter` applies.
        let opened = if ascii {
            "> (*) Claude Sonnet 5.5"
        } else {
            "› (●) Claude Sonnet 5.5"
        };
        let sonnet = rows
            .iter()
            .find(|r| r.contains(opened))
            .unwrap_or_else(|| panic!("{rows:#?}"));
        let flash = rows
            .iter()
            .find(|r| r.contains("  ( ) DeepSeek Flash"))
            .unwrap_or_else(|| panic!("{rows:#?}"));
        let note = super::super::own("current · $2 in / $10 out per MTok", ascii);
        assert!(sonnet.contains(&format!("{sep}{note}")), "{sonnet}");
        // Columns are cells: the selected row's glyphs are wider in bytes.
        let at = |r: &str, words: &str| r[..r.find(words).expect("words")].width();
        assert_eq!(
            at(sonnet, &format!("{sep}current")),
            at(flash, &format!("{sep}$0.30")),
            "one column: {rows:#?}"
        );
        let back = rows
            .iter()
            .find(|r| r.contains("All connections"))
            .expect("the row back");
        assert!(!back.contains(sep.trim()), "no note, no separator: {back}");
        if ascii {
            assert!(rows.iter().all(|r| r.is_ascii()), "{rows:#?}");
        }
    }
}

#[test]
fn a_narrow_band_cuts_the_note_before_the_label() {
    let state = state(Presentation::Focus, (44, 16), models(), false);
    let composer = composer(&state.waiting, "");
    let buffer = draw(&state, &composer);
    let rows: Vec<String> = (0..16).map(|y| row(&buffer, y)).collect();
    let sonnet = rows
        .iter()
        .find(|r| r.contains("Claude Sonnet 5.5"))
        .unwrap_or_else(|| panic!("{rows:#?}"));
    assert!(sonnet.contains('…'), "the note is cut: {sonnet}");
    assert!(sonnet.width() <= 44, "{sonnet}");
}
