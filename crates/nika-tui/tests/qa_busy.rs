// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a proof that cannot read its screen fails"
)]
//! Reception proofs of input WHILE a turn runs (`docs/qa/tui/RECEPTION.md`,
//! J2/J3), on the real shell over a conversation this suite controls
//! (`qa_support/child.rs`): the first turn stays busy, the spinner turning,
//! until the proof releases it.
//!
//! What must hold: nothing typed during the turn is lost, and nothing typed
//! during the turn answers the gate the turn ends on. What the target adds
//! (the design pack: « peut-on continuer à écrire pendant un run ? Oui dans
//! la cible »): the composer stays live, typed keys show at once.

#[path = "qa_support/child.rs"]
mod child;
mod qa_support;

use std::time::{Duration, Instant};

use child::{AFTER_GATE, BUSY, DONE, GATE_SAYS, Release, SECOND};
use qa_support::{ANSWER, FREE, Term, assert_restored, exit_code};

/// Long enough for the shell to read what was typed.
const SETTLE: Duration = Duration::from_millis(400);
/// What « at once » means for an echo while the spinner turns.
const LIVE: Duration = Duration::from_millis(300);

/// The child side, re-invoked by the proofs below; nothing in a plain run.
#[test]
fn qa_child_host() {
    child::host();
}

fn leave(term: &mut Term) {
    term.send("\x03");
    term.wait_text("Ctrl+C again leaves");
    term.send("\x03");
    let status = term.finish();
    assert_eq!(exit_code(status), Some(130), "{status:?}\n{}", term.dump());
    assert_restored(term);
}

/// Start the held turn and wait until its busy row shows.
fn busy(mode: &str, release: &Release) -> Term {
    let mut term = child::spawn(mode, Some(release.path()), 100, 32);
    term.wait_prompt(FREE);
    term.send("work\r");
    term.wait_text(BUSY);
    term
}

/// Reduced motion keeps the actual turn's facts while its marker stays still.
/// A Stop request and its settlement remain distinct, and idle remains silent.
#[test]
fn reduced_motion_keeps_elapsed_stop_and_draft_facts_without_an_idle_tick() {
    let release = Release::new("reduced-facts");
    let mut term = child::spawn(
        "slow-stop:0:workspace:reduced",
        Some(release.path()),
        120,
        40,
    );
    term.wait_text("release.nika");
    term.send("work\r");
    term.wait_text(BUSY);
    let mark = term.mark();
    term.send("draft retained");
    for seconds in [2, 3] {
        let measured = format!("{BUSY} · {seconds}s");
        term.wait_text(&measured);
        let row = term
            .screen
            .lines()
            .into_iter()
            .find(|line| line.contains(&measured))
            .expect("the current busy row");
        assert!(row.contains(&format!("● {measured}")), "{row}");
        assert!(term.screen.contains("draft retained"), "{}", term.dump());
    }
    term.send("\x03");
    term.wait_until("Stop requested with the measured elapsed time", |screen| {
        screen.contains("stopping the preparation")
            && screen.contains("Ctrl+C again leaves now")
            && screen.contains(BUSY)
            && (3..=5).any(|seconds| screen.contains(&format!(" · {seconds}s")))
    });
    assert!(!term.screen.contains(child::STOPPED), "{}", term.dump());
    assert!(term.screen.contains("draft retained"), "{}", term.dump());
    release.open();
    term.wait_workspace_frame("Stop settled once with the draft retained", |screen| {
        screen.seen(child::STOPPED)
            && screen.contains("Stopped by you")
            && screen
                .lines()
                .iter()
                .any(|line| line.contains("draft retained"))
            && !screen.contains("Ctrl+C again leaves now")
            && !screen.contains("stopping the preparation")
    });
    assert!(!term.screen.seen(SECOND), "{}", term.dump());
    assert!(
        !term.bytes_since(mark).contains(&7),
        "a reduced-motion bell rang"
    );
    let idle = term.mark();
    term.settle(Duration::from_millis(1200));
    assert!(term.bytes_since(idle).is_empty(), "idle drew a timer frame");
    leave(&mut term);
}

#[test]
fn keys_typed_during_a_busy_turn_are_kept_for_after_it() {
    let release = Release::new("kept");
    let mut term = busy("slow-free", &release);
    term.send("draft-b");
    term.settle(SETTLE);
    release.open();
    let draft = format!("{FREE} draft-b");
    term.wait_until("the draft after the turn", |screen| {
        screen.seen(DONE) && screen.row_starting(&draft).is_some()
    });
    term.settle(SETTLE);
    assert!(
        !term.screen.seen(SECOND),
        "the draft was sent\n{}",
        term.dump()
    );
    leave(&mut term);
}

/// One `Ctrl+C` while the turn runs warns « Ctrl+C again leaves now »; a
/// turn that ends before the second press does not take that back: the
/// idle row says a second press leaves, and it leaves with 130.
#[test]
fn a_ctrl_c_heard_while_the_turn_runs_still_leaves_once_it_ends() {
    let release = Release::new("armed");
    let mut term = busy("slow-free", &release);
    term.send("\x03");
    term.wait_text("Ctrl+C again leaves now");
    release.open();
    // « any key stays » is the idle prompt's armed row: the busy row warns
    // in its own words, so this row shows only once the turn has ended.
    term.wait_until("the turn's end, the warning kept", |screen| {
        screen.seen(DONE) && screen.contains("any key stays")
    });
    term.send("\x03");
    let status = term.finish();
    assert_eq!(exit_code(status), Some(130), "{status:?}\n{}", term.dump());
    assert!(
        !term.screen.seen(SECOND),
        "a second turn ran\n{}",
        term.dump()
    );
    assert_restored(&term);
}

#[test]
fn keys_typed_during_a_busy_turn_show_at_once() {
    let release = Release::new("live");
    let mut term = busy("slow-free", &release);
    term.send("draft-b");
    let draft = format!("{FREE} draft-b");
    let deadline = Instant::now() + LIVE;
    let mut shown = false;
    while Instant::now() < deadline && !shown {
        term.pump();
        shown = term.screen.row_starting(&draft).is_some();
        std::thread::sleep(Duration::from_millis(5));
    }
    let still_busy = term.screen.contains(BUSY);
    release.open();
    assert!(
        still_busy,
        "the turn ended before the check\n{}",
        term.dump()
    );
    assert!(
        shown,
        "a key typed during a busy turn did not show within {LIVE:?}\n{}",
        term.dump()
    );
    leave(&mut term);
}

/// A resize while the turn runs is drawn at once at the new size (the
/// spinner's own frames re-lay the screen out), in both presentations.
fn resize_during_a_busy_turn(
    mode: &str,
    tag: &str,
    laid_out: impl Fn(&qa_support::vt::Screen) -> bool,
) {
    let release = Release::new(tag);
    let mut term = busy(mode, &release);
    let redraw_from = term.mark();
    term.resize(80, 24);
    if mode == "slow-free" {
        // Shrinking the emulator can leave the old busy row and prompt
        // visible. Wait for the shell's inline resize clear before releasing
        // the turn, so its completion is painted in the new viewport.
        let deadline = Instant::now() + qa_support::WAIT;
        while !term
            .bytes_since(redraw_from)
            .windows(4)
            .any(|bytes| bytes == b"\x1b[2J")
        {
            term.pump();
            assert!(Instant::now() < deadline, "{}", term.dump());
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    term.wait_until("the busy screen at 80x24", |screen| {
        screen.contains(BUSY) && laid_out(screen)
    });
    release.open();
    term.wait_text(DONE);
    leave(&mut term);
}

#[test]
fn a_resize_during_a_busy_turn_redraws_at_the_new_size_inline() {
    resize_during_a_busy_turn("slow-free", "resize-inline", |screen| {
        screen.row_starting(FREE).is_some()
    });
}

#[test]
fn a_resize_during_a_busy_turn_redraws_at_the_new_size_in_focus() {
    let rule = "─".repeat(80);
    resize_during_a_busy_turn("slow-free:0:focus", "resize-focus", |screen| {
        screen.lines().contains(&rule)
    });
}

/// A resize while the turn runs reaches the workspace at once: its regions
/// and the keys they route follow the new size, not the size the turn began
/// at. With the keys parked in the aside, a terminal shrunk below the
/// workspace's minimum mid-turn draws the focus view, and the words typed
/// then reach the draft (the focus view's composer region), never a region
/// that is no longer on screen; the size coming back restores the workspace.
#[test]
fn a_resize_during_a_busy_turn_reaches_the_workspace_regions_at_once() {
    let release = Release::new("resize-workspace");
    let mut term = child::spawn("slow-free:0:workspace", Some(release.path()), 160, 48);
    term.wait_text("release.nika");
    term.send("work\r");
    term.wait_text(BUSY);
    term.send("\x1b[17~");
    term.settle(SETTLE);
    let mark = term.mark();
    term.resize(50, 14);
    // A fresh complete redraw proves the shell saw the new size; resized old
    // cells alone could still route keys to the now-hidden workspace aside.
    term.spin_until_bytes(mark, b"\x1b[2J");
    term.wait_workspace_frame("the focus view below the minimum", |screen| {
        screen.size() == (50, 14)
            && screen.contains(BUSY)
            && !screen.contains("release.nika")
            && screen.lines().iter().any(|line| line.starts_with(FREE))
            && screen.contains("Run: typing waits.")
    });
    term.send("xyz");
    term.settle(SETTLE);
    term.resize(160, 48);
    term.wait_workspace_frame("the workspace back at 160x48", |screen| {
        screen.contains(BUSY) && screen.contains("release.nika")
    });
    release.open();
    term.wait_until("the draft typed below the minimum", |screen| {
        screen.seen(DONE)
            && screen
                .lines()
                .iter()
                .any(|l| l.contains(FREE) && l.contains("xyz"))
    });
    term.settle(SETTLE);
    assert!(
        !term.screen.seen(SECOND),
        "the draft was sent\n{}",
        term.dump()
    );
    leave(&mut term);
}

/// The transcript scrolls while a turn runs: `PgUp` in focus moves the view
/// at once, not after the turn.
#[test]
fn scrolling_during_a_busy_turn_moves_the_view_at_once() {
    let release = Release::new("scroll");
    let mut term = child::spawn("slow-free:200:focus", Some(release.path()), 100, 32);
    term.wait_prompt(FREE);
    term.wait_text("item 00199");
    term.send("work\r");
    term.wait_text(BUSY);
    term.send("\x1b[5~\x1b[5~\x1b[5~");
    let deadline = Instant::now() + LIVE;
    let mut moved = false;
    while Instant::now() < deadline && !moved {
        term.pump();
        moved = !term.screen.contains("item 00199");
        std::thread::sleep(Duration::from_millis(5));
    }
    release.open();
    assert!(
        moved,
        "PgUp did not move the view within {LIVE:?}\n{}",
        term.dump()
    );
    leave(&mut term);
}

/// A `yes` typed while the turn runs, before any gate exists, must not
/// answer the gate the turn ends on.
#[test]
fn typeahead_during_a_busy_turn_never_answers_the_gate_it_ends_on() {
    let release = Release::new("gate");
    let mut term = busy("slow-gate", &release);
    term.send("yes\r");
    term.settle(SETTLE);
    release.open();
    term.wait_until("the gate", |screen| screen.seen(GATE_SAYS));
    term.settle(SETTLE);
    assert!(
        !term.screen.seen(AFTER_GATE),
        "a yes typed before the gate answered it\n{}",
        term.dump()
    );
    assert!(
        term.screen.row_starting(ANSWER).is_some(),
        "{}",
        term.dump()
    );
    leave(&mut term);
}

/// The same law in the workspace: a `yes` typed while the turn runs, before
/// the gate exists, never answers it, whichever region holds the keys.
#[test]
fn typeahead_during_a_busy_turn_never_answers_the_gate_in_the_workspace() {
    let release = Release::new("gate-workspace");
    let mut term = child::spawn("slow-gate:0:workspace", Some(release.path()), 120, 40);
    term.wait_text("release.nika");
    term.send("work\r");
    term.wait_text(BUSY);
    term.send("yes\r");
    term.send("\x1b[17~");
    term.send("yes\r");
    term.settle(SETTLE);
    release.open();
    term.wait_until("the gate", |screen| screen.seen(GATE_SAYS));
    term.settle(SETTLE);
    assert!(
        !term.screen.seen(AFTER_GATE),
        "a yes typed before the gate answered it\n{}",
        term.dump()
    );
    leave(&mut term);
}

/// The control of the proof above: a `yes` typed once the gate IS painted is
/// heard, so a silent gate there means discarded, not deaf.
#[test]
fn a_fresh_answer_after_the_gate_is_painted_is_heard() {
    let release = Release::new("fresh");
    let mut term = busy("slow-gate", &release);
    release.open();
    term.wait_until("the gate", |screen| {
        screen.seen(GATE_SAYS) && screen.row_starting(ANSWER).is_some()
    });
    term.send("yes\r");
    term.wait_until(AFTER_GATE, |screen| screen.seen(AFTER_GATE));
    leave(&mut term);
}

/// The shell already knows how to discard typeahead: a conversation that
/// asks for fresh input at its gate gets the busy-turn `yes` dropped and the
/// fresh one heard (the mechanism the defect above needs at every gate).
#[test]
fn a_gate_that_asks_for_fresh_input_drops_the_typeahead() {
    let release = Release::new("asks");
    let mut term = busy("slow-gate-fresh", &release);
    term.send("yes\r");
    term.settle(SETTLE);
    release.open();
    term.wait_until("the gate", |screen| {
        screen.seen(GATE_SAYS) && screen.row_starting(ANSWER).is_some()
    });
    term.settle(SETTLE);
    assert!(
        !term.screen.seen(AFTER_GATE),
        "the discard failed\n{}",
        term.dump()
    );
    term.send("yes\r");
    term.wait_until(AFTER_GATE, |screen| screen.seen(AFTER_GATE));
    leave(&mut term);
}

/// A fresh spending answer cannot recover pre-question words with Escape.
/// The controlled conversation exercises the production shell's fresh-input
/// law, without a provider or any billing effect.
#[test]
fn a_fresh_question_keeps_palette_aside_words_only_in_the_transcript() {
    let release = Release::new("palette-fresh");
    let mut term = busy("slow-gate-fresh", &release);
    term.send("yes");
    term.wait_text("nika › yes");
    term.send("\x0f");
    term.wait_text("commands ›");
    term.send("status\r");
    term.wait_text("nika › /status");
    release.open();
    term.wait_prompt(ANSWER);
    term.send("\x1b");
    term.settle(SETTLE);
    term.wait_until("the fresh answer remains empty after Escape", |screen| {
        screen.lines().iter().any(|row| row == ANSWER)
    });
    assert!(!term.screen.contains("set aside:"), "{}", term.dump());
    assert!(
        term.screen.seen("you typed « yes /status »")
            && term.screen.seen("kept in this conversation, whole:"),
        "{}",
        term.dump()
    );
    term.send("\r");
    term.settle(SETTLE);
    assert!(
        !term.screen.seen(AFTER_GATE),
        "old words answered a fresh question: {}",
        term.dump()
    );
    term.send("no\r");
    term.wait_text(AFTER_GATE);
    assert!(term.screen.seen("qa fresh answer: no"), "{}", term.dump());
    leave(&mut term);
}
