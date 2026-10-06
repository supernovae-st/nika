// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(clippy::expect_used, clippy::panic)]
// The suite drives the real binary through a PTY: the renderer refuses
// anything that is not a terminal (the carve-out of the other PTY suites).
#![allow(clippy::disallowed_types)]
//! The typeahead law on a real PTY: `yes⏎` typed while Nika works never
//! answers the decision the turn ends on. `nika-tui-proto --demo-pace` holds
//! each scripted turn busy; the words typed meanwhile show in the composer at
//! once, an `Enter` sends nothing and says so, the draft stays in the box
//! once the question, the proposal or the gate is painted, a dim notice says
//! so, and only the human's own `Enter`, after the decision is on screen,
//! sends it. In the focus view the page keys scroll while Nika works.
//!
//! The decisions are read from the composed terminal screen
//! (`qa_support::Term`), never from words in the raw stream: the renderer
//! writes only the cells a frame changes, so a word that shares cells with
//! the previous frame never arrives whole (`Save? ›` keeps the ` ›` of
//! `reply ›`, and the proposal's `/show` keeps the `o` of the question's
//! `cancel to stop`). Each decision is identified by its prompt row and by
//! the phrase its turn alone prints; a decision answered by typeahead would
//! print the next turn's phrase within the window watched here. The focus
//! proof below also reads the composed screen, including its retained scroll.

use std::time::Duration;

mod qa_support;

use qa_support::{
    ANSWER, APPLY, FREE, GATE, JOURNEY, PROPOSAL, QUESTION, REPLY, RESULT, SAVED, Term,
    assert_restored, exit_code,
};

/// How long each scripted turn stays busy.
const PACE_MS: &str = "900";
/// The dim notice that says where the typeahead went.
const NOTICE: &str = "it is in the box, not sent";
/// How long a decision is watched for an answer it must not get.
const WATCH: Duration = Duration::from_millis(1500);
/// The busy row of a paced turn: what is typed after it shows is typed
/// while Nika works.
const HOLDS: &str = "the demo holds this turn";

/// The paced proto inline on an 80 × 24 PTY, read as a composed screen,
/// at its free prompt.
fn paced() -> Term {
    let mut term = Term::proto_with(&["--demo-pace", PACE_MS], 80, 24, &[("NO_COLOR", "1")]);
    term.wait_prompt(FREE);
    term
}

/// Send `line` with `Enter`, then `yes⏎` once the turn it starts is busy.
fn send_then_type_ahead(term: &mut Term, line: &str) {
    term.send(&format!("{line}\r"));
    term.wait_text(HOLDS);
    term.send("yes\r");
}

/// The turn ends on the decision whose turn prints `shows`, at `prompt`,
/// with the typeahead in its box and the dim notice saying so. (The notice
/// is committed before the frame that draws the prompt, so the prompt row
/// is awaited, not read at once.)
fn decision_holds_the_draft(term: &mut Term, shows: &str, prompt: &str) {
    term.wait_until(NOTICE, |screen| screen.seen(NOTICE));
    let draft = format!("{prompt} yes");
    term.wait_until(&format!("{shows} · {draft}"), |screen| {
        screen.seen(shows) && screen.row_starting(&draft).is_some()
    });
}

/// Watch the decision at `prompt` for [`WATCH`]: the next turn's phrase
/// `next` is never printed and the decision's prompt stays.
fn never_answered(term: &mut Term, prompt: &str, next: &str, why: &str) {
    term.settle(WATCH);
    assert!(
        !term.screen.seen(next) && term.screen.row_starting(prompt).is_some(),
        "{why}\n{}",
        term.dump()
    );
}

/// The human's own `Enter` sends the draft: its echo is `› <prompt> yes`,
/// and the next turn prints `next` and leaves `then`.
fn the_human_sends_it(term: &mut Term, prompt: &str, next: &str, then: &str) {
    term.send("\r");
    term.wait_until(next, |screen| {
        screen.seen(next) && screen.row_starting(then).is_some()
    });
    let echo = format!("› {prompt} yes");
    assert!(
        term.screen.seen(&echo),
        "the draft the human sent is the typeahead\n{}",
        term.dump()
    );
}

/// Two `Ctrl+C` leave with 130 and the terminal handed back.
fn leave_term(term: &mut Term) {
    term.send("\x03");
    term.wait_text("Ctrl+C again leaves");
    term.send("\x03");
    let status = term.finish();
    assert_eq!(exit_code(status), Some(130), "{status:?}\n{}", term.dump());
    assert_restored(term);
}

/// A question: the typeahead lands in the box and the question waits; the
/// human's own `Enter` answers it with that very draft.
#[test]
fn yes_typed_while_a_question_is_prepared_never_answers_it() {
    let mut term = paced();
    send_then_type_ahead(&mut term, "digest my notes");
    decision_holds_the_draft(&mut term, QUESTION, REPLY);
    never_answered(
        &mut term,
        REPLY,
        PROPOSAL,
        "the typeahead answered the question",
    );
    the_human_sends_it(&mut term, REPLY, PROPOSAL, APPLY);
    leave_term(&mut term);
}

/// A proposal: `yes⏎` typed while it is prepared never consents; the free
/// prompt comes back only after the human's own `Enter` on the draft.
#[test]
fn yes_typed_while_a_proposal_is_prepared_never_consents() {
    let mut term = paced();
    // The question, answered once it is on screen.
    term.walk(&JOURNEY[..1]);
    send_then_type_ahead(&mut term, "./notes/lundi.md");
    decision_holds_the_draft(&mut term, PROPOSAL, APPLY);
    never_answered(
        &mut term,
        APPLY,
        SAVED,
        "the typeahead consented to the proposal",
    );
    the_human_sends_it(&mut term, APPLY, SAVED, FREE);
    leave_term(&mut term);
}

/// A gate: `yes⏎` typed while the run reaches its gate never answers it.
#[test]
fn yes_typed_while_a_run_reaches_its_gate_never_answers_it() {
    let mut term = paced();
    // The question, the proposal and the human's consent, each sent once
    // its decision is on screen: the workflow is saved.
    term.walk(&JOURNEY[..3]);
    send_then_type_ahead(&mut term, "run it");
    decision_holds_the_draft(&mut term, GATE, ANSWER);
    never_answered(&mut term, ANSWER, RESULT, "the typeahead answered the gate");
    the_human_sends_it(&mut term, ANSWER, RESULT, FREE);
    leave_term(&mut term);
}

/// While Nika works the composer stays live: pasted words show at once, a
/// bare `Enter` sends nothing and the hint row says when it will; the turn
/// ends on the question with the draft in the box, unsent, until the human
/// presses `Enter` once the question is on screen.
#[test]
fn words_typed_while_nika_works_show_at_once_and_enter_waits() {
    // Read the composed terminal screen: a diff may keep the middle letters
    // of "sends" from the previous hint and write only its changed cells.
    let mut term = paced();
    term.send("digest my notes\r");
    term.wait_text(HOLDS);
    term.send("\x1b[200~livedraft\x1b[201~");
    term.wait_text("livedraft");
    term.send("\r");
    term.wait_text("Nika is working · Enter sends when it is your turn");
    term.wait_prompt(REPLY);
    term.wait_text(NOTICE);
    term.wait_text("answer the question above");
    assert!(
        term.screen.contains("livedraft"),
        "the draft remains in the composer\n{}",
        term.dump()
    );
    term.settle(WATCH);
    assert!(
        !term.screen.seen(PROPOSAL),
        "Enter during work must not answer the question\n{}",
        term.dump()
    );
    term.send("\r");
    term.wait_prompt(APPLY);
    term.wait_text(PROPOSAL);
    term.send("\x03");
    term.wait_text("Ctrl+C again leaves");
    term.send("\x03");
    assert_eq!(exit_code(term.finish()), Some(130));
    assert_restored(&term);
}

/// Paging uses rendered rows with one row of overlap. At 60 × 8 the
/// two-line busy hint leaves three transcript rows, so two `PageUp` gestures
/// reach the first line. Finishing the turn preserves that reading position;
/// End explicitly returns to the proposal.
#[test]
fn the_page_keys_scroll_the_focus_transcript_while_nika_works() {
    let mut term = Term::proto_with(
        &["--demo-pace", PACE_MS, "--focus"],
        60,
        8,
        &[("NO_COLOR", "1")],
    );
    term.wait_prompt(FREE);
    term.send("qzxjqzxj\r");
    term.wait_prompt(REPLY);
    term.wait_text("const.source_path");
    term.send("./notes/lundi.md\r");
    term.wait_text(HOLDS);
    assert!(
        !term.screen.contains("qzxjqzxj"),
        "the reply echo pushed the first line above the viewport\n{}",
        term.dump()
    );
    term.send("\x1b[5~\x1b[5~");
    term.wait_until(
        "the first line is visible while the turn is busy",
        |screen| screen.contains("qzxjqzxj") && screen.contains(HOLDS),
    );
    term.wait_prompt(APPLY);
    assert!(
        term.screen.contains("qzxjqzxj") && !term.screen.contains(PROPOSAL),
        "the completed turn must preserve the earlier reading position\n{}",
        term.dump()
    );
    term.send("\x1b[F");
    term.wait_text(PROPOSAL);
    term.wait_prompt(APPLY);
    leave_term(&mut term);
}
