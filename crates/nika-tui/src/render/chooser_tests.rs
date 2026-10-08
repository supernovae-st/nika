// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The chooser as drawn in the live area: exact rows at the qualified
//! widths, what waits said above the list, the selection kept in view, and
//! the ASCII column.

#![allow(clippy::expect_used)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

use super::{draw_inline, live_rows, render_live};
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

fn inline(state: &UiState, composer: &Composer, width: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, 12)).expect("terminal");
    let _ = terminal.draw(|frame| draw_inline(frame, state, composer));
    rows(terminal.backend().buffer())
}

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
            assert_eq!(
                shown[0], "Ready for review",
                "{presentation:?}, palette {palette}"
            );
            assert!(shown.last().expect("hint row").contains("Esc"));
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

#[test]
fn a_slash_lists_the_commands_under_the_line_with_the_selection_and_its_detail() {
    let state = UiState::new(Presentation::Inline, false, (80, 12));
    let mut composer = composer();
    composer.paste("/");
    composer.choose(key(KeyCode::Down));
    let shown = inline(&state, &composer, 80);
    assert_eq!(
        shown[..8],
        [
            "",
            "nika › /",
            "  /help          What you can ask, and every command",
            "› /status        Where you are",
            "  /show          The proposal's exact bytes",
            "  /intelligence  Choose the AI this session reasons with",
            "any time · reads only · Project root, intelligence, authoring seat.",
            "",
        ]
    );
    assert_eq!(
        shown[11],
        "↑↓ choose · Tab or Enter inserts · Esc hides the list"
    );
}

#[test]
fn a_whole_command_says_enter_sends_it_and_waiting_states_say_nothing_answers() {
    let mut state = UiState::new(Presentation::Inline, false, (80, 12));
    let mut composer = composer();
    composer.paste("/show");
    let shown = inline(&state, &composer, 80);
    // A name column of six cells at least, then two spaces.
    assert_eq!(shown[2], "› /show   The proposal's exact bytes");
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
        // (The choice screen's own prompt is `› `: the entry is found whole.)
        let entry = shown
            .iter()
            .position(|row| row == "› /show   The proposal's exact bytes");
        assert!(
            context.is_some_and(|at| entry == Some(at + 1)),
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
    assert_eq!(shown[1], "commands › model");
    assert_eq!(shown[2], "› /status        Where you are");
    assert_eq!(
        shown[3],
        "  /intelligence  Choose the AI this session reasons with"
    );
    assert!(!shown.iter().any(|row| row.contains("my draft stays")));
    assert_eq!(
        shown[11],
        "↑↓ choose · Enter inserts, never sends · Esc: back to your draft"
    );
    composer.choose(key(KeyCode::Esc));
    let back = inline(&state, &composer, 80);
    assert_eq!(back[1], "nika › my draft stays");
}

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
    assert_eq!(shown[2], "nika › /status");
}

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
        shown.contains(&"> /status  Where you are".to_owned()),
        "{shown:#?}"
    );
    assert!(
        shown.contains(&"Up/Down choose - Tab or Enter inserts - Esc hides the list".to_owned()),
        "{shown:#?}"
    );
}

/// The live area grows for the chooser, never past all but two rows of what
/// it is given; closed, it keeps its half.
#[test]
fn the_live_area_grows_for_the_chooser_and_keeps_two_rows_for_the_transcript() {
    let state = UiState::new(Presentation::Focus, false, (80, 24));
    let mut composer = composer();
    let closed = live_rows(&state, &composer, 80, 24);
    composer.paste("/");
    let open = live_rows(&state, &composer, 80, 24);
    // status, line, four entries, two detail rows at most, hint.
    assert!(open > closed, "{open} > {closed}");
    assert!(open <= 9, "{open}");
    for height in [6, 8, 10] {
        assert!(live_rows(&state, &composer, 80, height) <= height - 2);
    }
    composer.choose(key(KeyCode::Esc));
    assert_eq!(live_rows(&state, &composer, 80, 24), closed);
}

/// A short live area (a stacked workspace panel) keeps the selected entry in
/// view as the selection moves past the rows it holds.
#[test]
fn a_short_area_scrolls_the_list_with_the_selection() {
    let mut state = UiState::new(Presentation::Workspace, false, (80, 24));
    state.waiting = Waiting::Proposal;
    let mut composer = composer();
    composer.paste("/");
    for _ in 0..3 {
        composer.choose(key(KeyCode::Down));
    }
    let mut terminal = Terminal::new(TestBackend::new(60, 6)).expect("terminal");
    let _ = terminal.draw(|frame| render_live(frame, &state, &composer, frame.area()));
    let shown = rows(terminal.backend().buffer());
    assert_eq!(
        shown,
        [
            "Save these changes · Run separately",
            "Save? › /",
            "A proposal waits · no command answers it · yes + Enter: Save",
            "› /intelligence  Choose the AI this session reasons with",
            "this session · your next line chooses · Shows the",
            "↑↓ choose · Tab or Enter inserts · Esc hides the list",
        ]
    );
}
