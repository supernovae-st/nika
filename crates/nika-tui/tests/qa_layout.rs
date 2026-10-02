// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a proof that cannot read its screen fails"
)]
//! Reception proofs of layout (`docs/qa/tui/RECEPTION.md`, J6): the demo
//! journey read off a real screen at the four qualified sizes in both
//! presentations, colour and its absence, and the baseline defects B1-B4 of
//! the design pass kept as acceptance checks for the lane that owns each
//! (ignored while the defect stands, never deleted).
//!
//! The screen is `qa_support::vt::Screen`: what a terminal shows after the
//! bytes, so a proof reads rows and words, not escape fragments.

mod qa_support;

use qa_support::vt::Screen;
use qa_support::{
    ANSWER, APPLY, FREE, FREE_HINT, GATE, GATE_HINT, JOURNEY, PROPOSAL, QUESTION, REPLY, SIZES,
    Term, assert_restored, exit_code,
};

/// Two `Ctrl+C` from an idle prompt: the terminal comes back with 130.
fn leave(term: &mut Term) {
    term.send("\x03");
    term.wait_text("Ctrl+C again leaves");
    term.send("\x03");
    let status = term.finish();
    assert_eq!(exit_code(status), Some(130), "{status:?}\n{}", term.dump());
    assert_restored(term);
}

/// The journey at one size in one presentation: every prompt lands on the
/// screen, the gate and its hint read whole, the composer keeps the bottom
/// half, nothing is addressed past the edge, a short turn never rings, and
/// the terminal comes back.
fn journey_fits(cols: u16, rows: u16, focus: bool) {
    let args: &[&str] = if focus { &["--focus"] } else { &[] };
    let mut term = Term::proto(args, cols, rows);
    term.wait_prompt(FREE);
    term.walk(&JOURNEY[..4]);
    let screen = &term.screen;
    let at = format!("{cols}x{rows} focus={focus}");
    assert!(screen.contains(GATE), "{at}: gate cut\n{}", term.dump());
    assert!(
        screen.contains(GATE_HINT),
        "{at}: gate hint cut\n{}",
        term.dump()
    );
    let prompt = screen.row_starting(ANSWER).expect("the gate prompt row");
    assert!(
        prompt >= usize::from(rows) / 2,
        "{at}: the composer left the bottom half (row {prompt})\n{}",
        term.dump()
    );
    if focus {
        let rule = "─".repeat(usize::from(cols));
        assert!(
            screen.lines().contains(&rule),
            "{at}: the focus rule does not span the width\n{}",
            term.dump()
        );
    }
    term.walk(&JOURNEY[4..]);
    assert_eq!(term.screen.beyond(), 0, "{at}: addressed past the screen");
    assert_eq!(term.screen.bells(), 0, "{at}: a short turn rang");
    leave(&mut term);
}

#[test]
fn inline_journey_fits_80x24() {
    journey_fits(80, 24, false);
}

#[test]
fn inline_journey_fits_100x32() {
    journey_fits(100, 32, false);
}

#[test]
fn inline_journey_fits_120x40() {
    journey_fits(120, 40, false);
}

#[test]
fn inline_journey_fits_160x48() {
    journey_fits(160, 48, false);
}

#[test]
fn focus_journey_fits_80x24() {
    journey_fits(80, 24, true);
}

#[test]
fn focus_journey_fits_100x32() {
    journey_fits(100, 32, true);
}

#[test]
fn focus_journey_fits_120x40() {
    journey_fits(120, 40, true);
}

#[test]
fn focus_journey_fits_160x48() {
    journey_fits(160, 48, true);
}

/// A tmux split of a classic terminal: one row for the status line, one
/// column for the border.
#[test]
fn inline_journey_fits_a_79x23_pane() {
    journey_fits(79, 23, false);
}

#[test]
fn inline_journey_fits_an_80x23_pane() {
    journey_fits(80, 23, false);
}

#[test]
fn focus_journey_fits_a_79x23_pane() {
    journey_fits(79, 23, true);
}

#[test]
fn focus_journey_fits_an_80x23_pane() {
    journey_fits(80, 23, true);
}

/// Without colour no hue is painted anywhere on the way to the gate, and the
/// meaning stays: the gate glyph and its words, the prompt that names what
/// waits, weights (bold, dim) instead of hues.
#[test]
fn without_colour_no_hue_is_painted_and_the_marks_remain() {
    let mut term = Term::proto_with(&[], 80, 24, &[("NO_COLOR", "1")]);
    term.wait_prompt(FREE);
    term.walk(&JOURNEY[..4]);
    assert!(
        term.screen.hues().is_empty(),
        "a hue without colour: {:?}",
        term.screen.hues()
    );
    assert!(term.screen.weights() > 0, "no weight carries the meaning");
    assert!(
        term.screen.seen("⏸ write ./digest.md"),
        "the gate lost its glyph\n{}",
        term.dump()
    );
    assert!(term.screen.row_starting(ANSWER).is_some());
    leave(&mut term);
}

/// With colour the gate and its prompt wear the warning slot and the busy
/// marker the accent: the theme's ANSI-16 slots, never a named hue.
#[test]
fn with_colour_the_gate_wears_the_warning_slot() {
    let mut term = Term::proto(&["--color"], 80, 24);
    term.wait_prompt(FREE);
    term.walk(&JOURNEY[..4]);
    let hues = term.screen.hues();
    assert!(
        hues.contains("38;5;3"),
        "no warning hue at the gate: {hues:?}"
    );
    assert!(
        hues.contains("38;5;6"),
        "no accent on the busy marker: {hues:?}"
    );
    leave(&mut term);
}

/// The caller's colour decision is the one the terminal sees. nika-cli lets
/// `CLICOLOR_FORCE` and `--color always` win over `NO_COLOR` (no-color.org:
/// an explicit argument overrides the variable) and passes `color: true`;
/// crossterm then strips every hue because it reads `NO_COLOR` itself, while
/// the renderer, told colour is on, has dropped the weights that carry the
/// meaning without it: the gate prompt keeps neither hue nor weight.
#[test]
#[ignore = "defect: under NO_COLOR a forced colour paints no hue (crossterm reads NO_COLOR) and the renderer has dropped the weight substitutes · one colour owner needed (terminal.rs: Colored::set_ansi_color_disabled from Options.color) · lane cards or ws"]
fn a_forced_colour_under_no_color_is_the_colour_the_terminal_sees() {
    let mut term = Term::proto_with(&["--color"], 80, 24, &[("NO_COLOR", "1")]);
    term.wait_prompt(FREE);
    term.walk(&JOURNEY[..4]);
    let hues = term.screen.hues();
    assert!(
        hues.contains("38;5;3"),
        "no warning hue at the gate: {hues:?}"
    );
    leave(&mut term);
}

/// B1 · the hint sits right under the composer: the live area takes its
/// natural height instead of stretching the editor over the 12-row viewport.
#[test]
#[ignore = "defect: B1 · the inline composer stretches over the 12-row viewport (9 blank rows between prompt and hint at every size) · lane cards"]
fn inline_hint_sits_right_under_the_composer() {
    for (cols, rows) in SIZES {
        let mut term = Term::proto(&[], cols, rows);
        term.wait_prompt(FREE);
        let prompt = term.screen.row_starting(FREE).expect("the prompt row");
        let hint = term.screen.row_of(FREE_HINT).expect("the hint row");
        assert_eq!(
            hint,
            prompt + 1,
            "{cols}x{rows}: {} blank rows between the prompt and its hint\n{}",
            hint.saturating_sub(prompt + 1),
            term.dump()
        );
        leave(&mut term);
    }
}

/// B2 · a line the human sent is echoed with one marker.
#[test]
#[ignore = "defect: B2 · the echo reads `› nika › digest my monday notes` (two markers) · lane cards"]
fn a_human_line_is_echoed_with_one_marker() {
    let mut term = Term::proto(&[], 80, 24);
    term.wait_prompt(FREE);
    term.walk(&JOURNEY[..1]);
    let echo = term
        .screen
        .transcript()
        .into_iter()
        .find(|line| line.contains("digest my monday notes"))
        .expect("the echo");
    assert_eq!(echo.matches('›').count(), 1, "{echo:?}");
    leave(&mut term);
}

/// B3 · the lines of one run stay together in focus: space between blocks,
/// not inside one run story.
#[test]
#[ignore = "defect: B3 · focus puts a blank row between every run line · lane cards"]
fn focus_keeps_the_lines_of_one_run_together() {
    let mut term = Term::proto(&["--focus"], 120, 40);
    term.wait_prompt(FREE);
    term.walk(&JOURNEY[..4]);
    let read = term.screen.row_of("read     ✓").expect("the read line");
    let draft = term.screen.row_of("draft    ✓").expect("the draft line");
    assert_eq!(draft, read + 1, "\n{}", term.dump());
    leave(&mut term);
}

/// B4 · leaving inline clears the live area: the blocks already committed
/// stay in the scrollback (they are the record), the composer, its hint and
/// the interrupted notice do not stay painted under the shell prompt.
#[test]
#[ignore = "defect: B4 · after two Ctrl+C the composer, its hint and the interrupted notice stay painted · lane cards"]
fn leaving_inline_clears_the_live_area() {
    let mut term = Term::proto(&[], 80, 24);
    term.wait_prompt(FREE);
    term.walk(&JOURNEY[..1]);
    leave(&mut term);
    assert!(term.screen.seen(QUESTION), "the record was erased");
    for leftover in ["Ctrl+C again leaves", "answer the question", REPLY] {
        assert!(
            !term.screen.contains(leftover),
            "{leftover:?} stays painted after the exit\n{}",
            term.dump()
        );
    }
}

/// A consent is given to what is on screen: after the terminal narrows
/// (120 to 80 columns) while a proposal waits, the proposal is still
/// visible above `apply? ›`, not only in a scrollback some terminals keep.
#[test]
#[ignore = "defect: a width shrink in inline wipes the screen and the waiting proposal leaves it; only apply? and its hint stay (ratatui resize clears on a horizontal shrink and moves the viewport to row 0) · lane cards or ws (inline viewport)"]
fn a_width_shrink_keeps_the_waiting_proposal_on_screen() {
    let mut term = Term::proto(&[], 120, 40);
    term.wait_prompt(FREE);
    term.walk(&JOURNEY[..2]);
    term.resize(80, 24);
    term.settle(std::time::Duration::from_millis(800));
    assert!(
        term.screen.row_starting(APPLY).is_some(),
        "the consent prompt\n{}",
        term.dump()
    );
    assert!(
        term.screen.contains(PROPOSAL),
        "the proposal waiting for consent left the screen\n{}",
        term.dump()
    );
    leave(&mut term);
}

/// The screen model reads what crossterm and ratatui write: absolute moves,
/// wide glyphs, erase, a scroll region feeding the history, the two queries,
/// colour and weight, the alternate screen.
#[test]
fn the_screen_model_reads_what_the_renderer_writes() {
    let mut screen = Screen::new(10, 4);
    screen.feed(b"\x1b[2;3Hab\x1b[1;1Hx");
    assert_eq!(screen.lines(), ["x", "  ab", "", ""]);
    screen.feed("\x1b[3;1H漢字!".as_bytes());
    assert_eq!(screen.lines()[2], "漢字!");
    screen.feed(b"\x1b[3;2Hz");
    assert_eq!(screen.lines()[2], " z字!", "a broken wide glyph is blanked");
    screen.feed(b"\x1b[2;1H\x1b[K");
    assert_eq!(screen.lines()[1], "");
    screen.feed(b"\x1b[1;3r\x1b[1S\x1b[r");
    assert_eq!(screen.history(), ["x"]);
    assert_eq!(screen.lines(), ["", " z字!", "", ""]);
    screen.feed(b"\x1b[c\x1b[4;5H\x1b[6n");
    assert_eq!(
        screen.take_replies(),
        [b"\x1b[?62;22c".to_vec(), b"\x1b[4;5R".to_vec()]
    );
    screen.feed(b"\x1b[1m\x1b[38;5;3mX\x1b[39m\x1b[0m");
    assert!(screen.hues().contains("38;5;3"));
    assert_eq!(screen.weights(), 1);
    screen.feed(b"\x1b[?1049h\x1b[1;1HALT");
    assert!(screen.on_alt() && screen.contains("ALT"));
    screen.feed(b"\x1b[?1049l");
    assert!(!screen.on_alt() && !screen.contains("ALT"));
    assert_eq!(screen.beyond(), 0);
    screen.feed(b"\x1b[9;99H");
    assert_eq!(screen.beyond(), 1, "a move past the edge is counted");
}

/// A resize keeps the cursor's line on screen and sends what is above it to
/// the history; columns are cut, never reflowed.
#[test]
fn the_screen_model_resizes_like_xterm() {
    let mut screen = Screen::new(6, 3);
    screen.feed(b"one\r\ntwo\r\nthree!");
    screen.resize(4, 2);
    assert_eq!(screen.history(), ["one"]);
    assert_eq!(screen.lines(), ["two", "thre"]);
    assert_eq!(screen.cursor(), (1, 3));
    screen.resize(8, 3);
    assert_eq!(screen.lines(), ["two", "thre", ""]);
}
