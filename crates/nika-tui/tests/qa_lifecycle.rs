// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a proof that cannot read its screen fails"
)]
//! Reception proofs of the terminal lifecycle (`docs/qa/tui/RECEPTION.md`,
//! J4/J6): the exit paths `tests/pty_restore.rs` does not reach (`SIGTERM`
//! inline, a panic in focus, two `Ctrl+C` in focus, `SIGINT` as a signal,
//! `TERM=dumb` on a real terminal), and silence: an idle renderer writes
//! nothing at all.
//!
//! "Restored" is the terminal's final state as the screen model reads it
//! after the last byte: paste and focus reports off, cursor shown, main
//! screen on.

mod qa_support;

use std::time::Duration;

use expectrl::process::unix::Signal;
use qa_support::{ALT_OFF, FREE, JOURNEY, QUESTION, Term, assert_restored, exit_code};

/// Finite work a future opening may play (the butterfly reveal is 760 ms)
/// before idleness is judged.
const SETTLE: Duration = Duration::from_millis(1500);
/// The idle window judged byte by byte.
const IDLE: Duration = Duration::from_secs(5);

fn leave(term: &mut Term) {
    term.send("\x03");
    term.wait_text("Ctrl+C again leaves");
    term.send("\x03");
    let status = term.finish();
    assert_eq!(exit_code(status), Some(130), "{status:?}\n{}", term.dump());
    assert_restored(term);
}

#[test]
fn sigterm_restores_the_inline_terminal_and_leaves_with_143() {
    let mut term = Term::proto(&[], 80, 24);
    term.wait_prompt(FREE);
    term.walk(&JOURNEY[..1]);
    term.signal(Signal::SIGTERM);
    let status = term.finish();
    assert_eq!(exit_code(status), Some(143), "{status:?}\n{}", term.dump());
    assert_restored(&term);
    assert!(term.screen.seen(QUESTION), "the record survives the exit");
}

/// The panic hook leaves the alternate screen BEFORE the message prints, so
/// the message lands on the main screen where the human reads it.
#[test]
fn a_panic_in_focus_leaves_the_alternate_screen_before_the_message() {
    let mut term = Term::proto(&["--focus", "--panic-after", "1"], 80, 24);
    term.wait_prompt(FREE);
    term.send("boom\r");
    let status = term.finish();
    assert_eq!(exit_code(status), Some(101), "{status:?}\n{}", term.dump());
    assert_restored(&term);
    let raw = term.raw_text();
    let message = raw.find("panic requested").expect("the panic message");
    assert!(
        raw[..message].contains(ALT_OFF),
        "the message printed on the alternate screen\n{}",
        term.dump()
    );
    assert!(
        term.screen.seen("panic requested after 1 line(s)"),
        "the message is not on the main screen\n{}",
        term.dump()
    );
}

#[test]
fn two_control_c_in_focus_leave_with_130_and_restore() {
    let mut term = Term::proto(&["--focus"], 100, 32);
    term.wait_prompt(FREE);
    term.walk(&JOURNEY[..1]);
    leave(&mut term);
}

/// `SIGINT` delivered as a signal (raw mode turns the key into a key, not a
/// signal) walks the same ladder as the key: the first arms, the second
/// leaves.
#[test]
fn a_sigint_signal_arms_then_leaves_like_two_control_c() {
    let mut term = Term::proto(&[], 80, 24);
    term.wait_prompt(FREE);
    term.signal(Signal::SIGINT);
    term.wait_text("Ctrl+C again leaves");
    term.signal(Signal::SIGINT);
    let status = term.finish();
    assert_eq!(exit_code(status), Some(130), "{status:?}\n{}", term.dump());
    assert_restored(&term);
}

/// `TERM=dumb` on a real terminal is refused before any mode is touched:
/// exit 2, not one escape sequence.
#[test]
fn term_dumb_on_a_real_terminal_is_refused_without_an_escape_sequence() {
    let mut term = Term::proto_with(&[], 80, 24, &[("TERM", "dumb")]);
    let status = term.finish();
    assert_eq!(exit_code(status), Some(2), "{status:?}\n{}", term.dump());
    let raw = term.raw_text();
    assert!(
        !raw.contains('\x1b'),
        "an escape sequence under TERM=dumb: {raw:?}"
    );
    assert!(
        raw.contains("TERM=dumb"),
        "the refusal names its reason: {raw:?}"
    );
}

/// `SIGHUP` (the terminal hung up, a supervisor, a killed tmux pane) leaves
/// through the same restore path as `SIGTERM`, with 129, so the session
/// behind the renderer ends as cleanly as it does on `SIGTERM`.
#[test]
#[ignore = "defect: SIGHUP kills the renderer with no restore and no clean exit · events.rs watches only SIGTERM and SIGINT · lane keys (wave 2) or ws"]
fn sighup_restores_and_leaves_with_129() {
    let mut term = Term::proto(&["--focus"], 80, 24);
    term.wait_prompt(FREE);
    term.walk(&JOURNEY[..1]);
    term.signal(Signal::SIGHUP);
    let status = term.finish();
    assert_eq!(exit_code(status), Some(129), "{status:?}\n{}", term.dump());
    assert_restored(&term);
}

/// Pump through `SETTLE`, then judge a silent window: nothing written.
fn quiet(term: &mut Term, after: &str, window: Duration) {
    term.settle(SETTLE);
    let mark = term.mark();
    let written = term.settle(window);
    assert_eq!(
        written,
        0,
        "{after}: still writing when idle: {:?}",
        term.raw_since(mark)
    );
}

/// Every effect the base can play ends in silence: the opening, each turn of
/// the journey (the busy row turns, then stops), the switch to focus and
/// back, a resize.
#[test]
fn every_effect_ends_in_silence() {
    let window = Duration::from_secs(2);
    let mut term = Term::proto(&[], 100, 32);
    term.wait_prompt(FREE);
    quiet(&mut term, "the opening", window);
    for step in JOURNEY {
        term.walk(&[step]);
        quiet(&mut term, step.shows, window);
    }
    term.send("\x14");
    term.wait_text("focus · Esc returns inline");
    quiet(&mut term, "the switch to focus", window);
    term.send("\x1b");
    term.wait_until("back inline", |screen| !screen.on_alt());
    quiet(&mut term, "the return inline", window);
    term.resize(80, 24);
    quiet(&mut term, "a resize", window);
    leave(&mut term);
}

/// A focus report (tmux `focus-events`, a window switch) costs at most one
/// frame and starts nothing that keeps drawing.
#[test]
fn a_focus_report_costs_at_most_one_frame_then_silence() {
    const ONE_FRAME: usize = 64;
    let mut term = Term::proto(&[], 80, 24);
    term.wait_prompt(FREE);
    term.settle(SETTLE);
    for report in ["\x1b[O", "\x1b[I"] {
        let mark = term.mark();
        term.send(report);
        term.settle(Duration::from_millis(300));
        let cost = term.mark() - mark;
        assert!(
            cost <= ONE_FRAME,
            "{report:?} cost {cost} bytes: {:?}",
            term.raw_since(mark)
        );
        let mark = term.mark();
        let after = term.settle(Duration::from_secs(2));
        assert_eq!(
            after,
            0,
            "{report:?} left a loop: {:?}",
            term.raw_since(mark)
        );
    }
    leave(&mut term);
}

/// An idle inline prompt writes nothing: no tick, no cursor blink, no redraw.
#[test]
fn an_idle_inline_prompt_writes_nothing_for_five_seconds() {
    let mut term = Term::proto(&[], 80, 24);
    term.wait_prompt(FREE);
    term.settle(SETTLE);
    let mark = term.mark();
    let written = term.settle(IDLE);
    assert_eq!(written, 0, "idle output: {:?}", term.raw_since(mark));
    leave(&mut term);
}

/// After a turn (the busy row turned, then stopped) the focus screen writes
/// nothing either: the loader stops with the turn.
#[test]
fn an_idle_focus_screen_after_a_turn_writes_nothing_for_five_seconds() {
    let mut term = Term::proto(&["--focus"], 120, 40);
    term.wait_prompt(FREE);
    term.walk(&JOURNEY[..1]);
    term.settle(SETTLE);
    let mark = term.mark();
    let written = term.settle(IDLE);
    assert_eq!(written, 0, "idle output: {:?}", term.raw_since(mark));
    leave(&mut term);
}
