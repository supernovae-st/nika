// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The chooser as drawn on its surface above the line: exact rows at the
//! qualified widths, the Session's status facts and what waits said above
//! the list, a draft set aside named there too, the selection kept on its
//! page, the palette's search in the composer's own row, and the ASCII
//! column.

#![allow(clippy::expect_used)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

use super::{draw_focus, draw_inline};
use crate::composer::Composer;
use crate::composer::chooser::Entry;
use crate::model::{Presentation, UiState, Waiting};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// The Session's commands, as the catalog words them (a short set).
fn composer() -> Composer {
    let mut composer = Composer::new();
    composer.offer(vec![
        Entry::command(
            "/help",
            "What you can ask, and every command",
            "any time · reads only",
            "The Session's help card.",
            "help",
        ),
        Entry::command(
            "/status",
            "Where you are",
            "any time · reads only",
            "Project root, intelligence, authoring seat.",
            "model effort",
        ),
        Entry::command(
            "/show",
            "The proposal's exact bytes",
            "while a proposal waits · reads only",
            "The exact bytes a yes would save.",
            "inspect",
        ),
        Entry::command(
            "/intelligence",
            "Choose the AI this session reasons with",
            "this session · your next line chooses",
            "Shows the intelligence choices again.",
            "model",
        ),
    ]);
    composer.set_focused(true);
    composer
}

fn rows(buffer: &Buffer) -> Vec<String> {
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol().to_owned())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

/// An inline frame `width` cells wide and twelve rows tall: the line on row
/// 10, the hint on row 11, the surface's band above.
fn inline(state: &UiState, composer: &Composer, width: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, 12)).expect("terminal");
    let _ = terminal.draw(|frame| draw_inline(frame, state, composer));
    rows(terminal.backend().buffer())
}

/// An entry's row as the surface paints it: the selection glyph, the name
/// padded to the list's column, then what it does.
fn entry(selected: bool, name: &str, column: usize, does: &str) -> String {
    let glyph = if selected { "›" } else { " " };
    format!("{glyph} {name:<column$} · {does}")
}

/// An open chooser keeps the Session's status facts, above the list on its
/// surface, without the presentation's key instructions; closed, the status
/// row says both again.
#[test]
fn an_open_chooser_keeps_status_facts_without_competing_key_instructions() {
    for presentation in [Presentation::Focus, Presentation::Workspace] {
        for palette in [false, true] {
            let mut state = UiState::new(presentation, false, (120, 40));
            state.status = "Ready for review".to_owned();
            let mut composer = composer();
            if palette {
                composer.toggle_palette();
            } else {
                composer.paste("/");
            }
            let shown = inline(&state, &composer, 120);
            assert!(shown[0].starts_with("Commands"), "{shown:#?}");
            assert_eq!(
                shown[1], "Ready for review",
                "{presentation:?}, palette {palette}"
            );
            assert!(shown[11].contains("Esc"), "{shown:#?}");
            composer.choose(key(KeyCode::Esc));
            let shown = inline(&state, &composer, 120);
            let mode = match presentation {
                Presentation::Focus => "focus · Esc returns inline · PgUp/PgDn scroll",
                _ => "workspace · F6 panel · Esc back",
            };
            assert_eq!(shown[0], format!("Ready for review · {mode}"));
        }
    }
}

/// A slash lists the commands above the line, the selection marked and its
/// scope and help under the list; the line keeps its row.
#[test]
fn a_slash_lists_the_commands_above_the_line_with_the_selection_and_its_detail() {
    let state = UiState::new(Presentation::Inline, false, (80, 12));
    let mut composer = composer();
    composer.paste("/");
    composer.choose(key(KeyCode::Down));
    let shown = inline(&state, &composer, 80);
    assert!(
        shown[0].starts_with("Commands · beginning with what you typed"),
        "{shown:#?}"
    );
    assert!(shown[0].ends_with("Esc"), "{shown:#?}");
    assert_eq!(
        shown[1..6],
        [
            entry(false, "/help", 13, "What you can ask, and every command"),
            entry(true, "/status", 13, "Where you are"),
            entry(false, "/show", 13, "The proposal's exact bytes"),
            entry(
                false,
                "/intelligence",
                13,
                "Choose the AI this session reasons with"
            ),
            "any time · reads only · Project root, intelligence, authoring seat.".to_owned(),
        ]
    );
    assert_eq!(shown[10], "nika › /");
    assert_eq!(
        shown[11],
        "↑↓ choose · Tab or Enter inserts · Esc hides the list"
    );
}

/// A whole command says `Enter` sends it, and each waiting state says above
/// the list that no command answers it; while Nika works the command waits
/// in the box.
#[test]
fn a_whole_command_says_enter_sends_it_and_waiting_states_say_nothing_answers() {
    let mut state = UiState::new(Presentation::Inline, false, (80, 12));
    let mut composer = composer();
    composer.paste("/show");
    let shown = inline(&state, &composer, 80);
    // A name column of six cells at least.
    let whole = entry(true, "/show", 6, "The proposal's exact bytes");
    assert_eq!(shown[1], whole);
    assert_eq!(
        shown[11],
        "Enter sends /show · ↑↓ choose · Esc hides the list"
    );
    for (waiting, said) in [
        (
            Waiting::Proposal,
            "A proposal waits · no command answers it · yes + Enter: Save",
        ),
        (
            Waiting::Gate,
            "A gate waits · no command answers it · approve or refuse",
        ),
        (
            Waiting::Question {
                key: "run_cost".into(),
            },
            "A cost decision waits · no command approves it · only your yes does",
        ),
        (
            Waiting::Choosing,
            "The intelligence choice waits · no command answers it",
        ),
    ] {
        state.waiting = waiting;
        let shown = inline(&state, &composer, 80);
        let context = shown.iter().position(|row| row == said);
        let listed = shown.iter().position(|row| *row == whole);
        assert!(
            context.is_some_and(|at| listed == Some(at + 1)),
            "{said}: {shown:#?}"
        );
    }
    // While Nika works, Enter keeps the command in the box: the rows say so.
    state.waiting = Waiting::Free;
    state.busy = Some("● authoring".into());
    let shown = inline(&state, &composer, 80);
    assert!(
        shown.contains(&"Nika is working · a command waits in the box until your turn".to_owned()),
        "{shown:#?}"
    );
    assert_eq!(
        shown[11],
        "/show waits for your turn · ↑↓ choose · Esc hides the list"
    );
    assert!(
        !shown.iter().any(|row| row.contains("Enter sends")),
        "{shown:#?}"
    );
}

/// The palette's search takes the composer's own row while the draft waits
/// out of view; `Esc` brings the draft back on that row.
#[test]
fn the_palette_takes_the_composer_row_and_keeps_the_draft_out_of_view() {
    let state = UiState::new(Presentation::Inline, false, (80, 12));
    let mut composer = composer();
    composer.paste("my draft stays");
    composer.toggle_palette();
    for c in "model".chars() {
        composer.choose(key(KeyCode::Char(c)));
    }
    let shown = inline(&state, &composer, 80);
    assert_eq!(shown[10], "commands › model");
    assert_eq!(shown[1], entry(true, "/status", 13, "Where you are"));
    assert_eq!(
        shown[2],
        entry(
            false,
            "/intelligence",
            13,
            "Choose the AI this session reasons with"
        )
    );
    assert!(!shown.iter().any(|row| row.contains("my draft stays")));
    assert_eq!(
        shown[11],
        "↑↓ choose · Enter inserts, never sends · Esc: back to your draft"
    );
    composer.choose(key(KeyCode::Esc));
    let back = inline(&state, &composer, 80);
    assert_eq!(back[10], "nika › my draft stays");
}

/// Words the palette set aside to insert a command are named on the surface
/// the command's own slash list opens, above it, until they return.
#[test]
fn a_draft_set_aside_is_named_until_it_returns() {
    let state = UiState::new(Presentation::Inline, false, (80, 12));
    let mut composer = composer();
    composer.paste("read ./notes and digest the monday ones please");
    composer.toggle_palette();
    for c in "status".chars() {
        composer.choose(key(KeyCode::Char(c)));
    }
    composer.choose(key(KeyCode::Enter));
    let shown = inline(&state, &composer, 80);
    assert_eq!(
        shown[1],
        "set aside: « read ./notes and digest… » · back once this line is sent · Esc: now"
    );
    assert_eq!(shown[10], "nika › /status");
}

/// Every row the chooser's surface paints stays ASCII in the ASCII column.
#[test]
fn the_ascii_column_draws_every_chooser_row_in_ascii() {
    let mut state = UiState::new(Presentation::Inline, false, (80, 12));
    state.ascii = true;
    state.waiting = Waiting::Proposal;
    let mut composer = composer();
    composer.paste("/s");
    let shown = inline(&state, &composer, 80);
    for row in &shown {
        assert!(row.is_ascii(), "{row:?}");
    }
    assert!(
        shown.contains(&"> /status - Where you are".to_owned()),
        "{shown:#?}"
    );
    assert!(
        shown.contains(&"Up/Down choose - Tab or Enter inserts - Esc hides the list".to_owned()),
        "{shown:#?}"
    );
}

/// A short band pages its list whole around the selection: the selected
/// entry stays in view and the facts above it stay whole.
#[test]
fn a_short_band_pages_the_list_with_the_selection() {
    let mut state = UiState::new(Presentation::Focus, false, (60, 8));
    state.waiting = Waiting::Proposal;
    let mut composer = composer();
    composer.paste("/");
    for _ in 0..3 {
        composer.choose(key(KeyCode::Down));
    }
    let mut terminal = Terminal::new(TestBackend::new(60, 8)).expect("terminal");
    let _ = terminal.draw(|frame| draw_focus(frame, &state, &composer));
    let shown = rows(terminal.backend().buffer());
    assert!(shown[0].starts_with("Commands"), "{shown:#?}");
    assert_eq!(shown[1], "Not saved yet · yes means Save only");
    // Sixty cells: the context fits its row whole.
    assert_eq!(
        shown[2],
        "A proposal waits · no command answers it · yes + Enter: Save"
    );
    assert!(
        shown[3].starts_with("› /intelligence · Choose the AI"),
        "{shown:#?}"
    );
    assert!(!shown.iter().any(|row| row.contains("/help")), "{shown:#?}");
}
