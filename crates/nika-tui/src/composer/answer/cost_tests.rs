// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Typing over a long settings list stays instant. Each keystroke's own work
//! (the edit, the rows its filter shows, the selection, and one whole frame
//! derived and painted from them) is timed alone, never the suite around it,
//! in the debug build a developer preview ships. The sentence is typed in a
//! few rounds and the quickest round is judged: tests running beside this one
//! or another build on the machine only ever slow a round down, while a
//! per-row search that allocates, or a check that walks the list once per
//! row, slows every round past the budget.

#![allow(clippy::expect_used)]

use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::*;
use crate::model::{Presentation, UiState};

/// The rows of the list typed over: more than a full catalogue today.
const ROWS: usize = 1_000;

/// A sentence typed over the list (39 characters), then one more key.
const TYPED: &str = "Cree un rapport des titres tech du jour!";

/// The most one keystroke's work may take in the quickest round, on average
/// and at its median: generous for a debug build, far under what a person
/// notices (145 ms per key before the search index, about 3 ms after).
const BUDGET: Duration = Duration::from_millis(25);

/// The rounds the sentence is typed in.
const ROUNDS: usize = 3;

/// A home list over every model of 20 connections: most rows listed only
/// while a filter finds them, each with its facts in a note.
fn catalogue() -> Waiting {
    let offers = (0..ROWS)
        .map(|n| {
            let offer = Offer::new(
                format!("2 provider-{}/model-{n}", n % 20),
                format!("Model {n} · provider {}", n % 20),
            )
            .noted("$0.30 in / $1.20 out per MTok · 128K context");
            if n < 20 { offer } else { offer.found_only() }
        })
        .collect();
    Waiting::selection(Asked::new(
        "Choose the intelligence for this conversation",
        "",
        false,
        Shape::Choice(offers),
        "9:intelligence",
        9,
    ))
}

/// One round: the sentence typed over a fresh list, each keystroke's work
/// timed alone; its average and median per keystroke.
fn round(waiting: &Waiting) -> (Duration, Duration) {
    let mut state = UiState::new(Presentation::Focus, false, (120, 40));
    state.waiting = waiting.clone();
    let mut composer = Composer::new();
    composer.set_focused(true);
    composer.follow(waiting);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("terminal");
    let mut took = Vec::new();
    for c in TYPED.chars() {
        let started = Instant::now();
        assert!(composer.own_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)));
        terminal
            .draw(|frame| crate::render::draw_focus(frame, &state, &composer))
            .expect("draw");
        took.push(started.elapsed());
    }
    assert_eq!(took.len(), 40, "forty keystrokes");
    assert_eq!(composer.own_reply(), TYPED, "every key reached the field");
    assert!(
        composer.offer_rows(waiting).is_empty(),
        "a sentence matches no row"
    );
    let average = took.iter().sum::<Duration>() / 40;
    took.sort_unstable();
    (average, took[took.len() / 2])
}

#[test]
fn typing_over_a_long_list_keeps_each_keystroke_under_its_budget() {
    let waiting = catalogue();
    let rounds: Vec<(Duration, Duration)> = (0..ROUNDS).map(|_| round(&waiting)).collect();
    let (average, median) = rounds
        .iter()
        .copied()
        .min_by_key(|(average, _)| *average)
        .expect("rounds");
    assert!(
        average <= BUDGET && median <= BUDGET,
        "per keystroke in the quickest round: average {average:?}, median {median:?} (budget {BUDGET:?}); every round: {rounds:?}"
    );
}
