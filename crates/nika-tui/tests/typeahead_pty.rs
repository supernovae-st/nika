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

/// While Nika works the slash list chooses without reaching the turn: it
/// says a command waits in the box, `Tab` inserts it, `Enter` keeps it for
/// the human's turn, and once the question is painted the command is in the
/// box, unsent, under the typeahead law.
#[test]
fn a_command_chosen_while_nika_works_waits_in_the_box_and_answers_nothing() {
    for (cols, rows) in [(80, 24), (120, 40)] {
        let mut term =
            Term::proto_with(&["--demo-pace", PACE_MS], cols, rows, &[("NO_COLOR", "1")]);
        term.wait_prompt(FREE);
        term.send("digest my notes\r");
        term.wait_text(HOLDS);
        term.send("/st");
        term.wait_until("the list while Nika works", |screen| {
            screen.contains("Nika is working · a command waits in the box until your turn")
                && screen.contains("› /status")
        });
        term.send("\t");
        term.wait_text("/status waits for your turn");
        term.send("\r");
        decision_holds_the_draft_as(&mut term, QUESTION, REPLY, "/status");
        never_answered(
            &mut term,
            REPLY,
            PROPOSAL,
            "a command chosen while Nika worked answered the question",
        );
        leave_term(&mut term);
    }
}

/// [`decision_holds_the_draft`] for any typed draft.
fn decision_holds_the_draft_as(term: &mut Term, shows: &str, prompt: &str, typed: &str) {
    term.wait_until(NOTICE, |screen| screen.seen(NOTICE));
    let draft = format!("{prompt} {typed}");
    term.wait_until(&format!("{shows} · {draft}"), |screen| {
        screen.seen(shows) && screen.row_starting(&draft).is_some()
    });
}

/// The active-gate law with the palette: opened while the run reaches its
/// gate, a command chosen and `Enter` pressed before the gate is painted,
/// the gate never takes it; the human's draft waits in the box.
#[test]
fn the_palette_while_a_run_reaches_its_gate_never_answers_it() {
    let mut term = paced();
    term.walk(&JOURNEY[..3]);
    term.send("run it\r");
    term.wait_text(HOLDS);
    term.send("\x0f");
    term.wait_text("commands ›");
    term.send("help\r\r");
    decision_holds_the_draft_as(&mut term, GATE, ANSWER, "/help");
    never_answered(
        &mut term,
        ANSWER,
        RESULT,
        "a chosen command answered the gate",
    );
    leave_term(&mut term);
}

/// While Nika works, cancelling the palette opened from the preview gives the
/// keys back to the preview: a letter typed then never reaches the draft
/// typed earlier, which waits unsent in the box of the question.
#[test]
fn cancelling_the_palette_while_nika_works_returns_the_keys_to_the_preview() {
    let mut term = Term::proto_with(&["--demo-pace", PACE_MS], 80, 24, &[("NO_COLOR", "1")]);
    term.wait_prompt(FREE);
    term.send("\x14");
    term.wait_text("workspace · F6 panel");
    term.send("digest my notes\r");
    term.wait_text(HOLDS);
    term.send("kept");
    term.wait_text("nika › kept");
    term.send("\x1b[17~\x0f");
    term.wait_text("commands ›");
    term.send("\x1b");
    term.wait_until("the palette closed", |screen| {
        !screen.contains("commands ›")
    });
    term.send("x");
    term.wait_until("the question with the draft", |screen| {
        screen.seen(QUESTION) && screen.contains("reply › kept")
    });
    term.settle(WATCH);
    assert!(
        !term.screen.contains("keptx") && !term.screen.seen(PROPOSAL),
        "the keys went to the composer, or the draft was sent\n{}",
        term.dump()
    );
    leave_term(&mut term);
}

/// A switch of presentation while typing, and one asked while Nika works,
/// keep the exact draft (two lines) and send nothing: the line typed during
/// the turn waits in the box of the question that ends it.
#[test]
fn a_presentation_switch_while_typing_or_working_keeps_the_exact_draft() {
    for (cols, rows) in [(80, 24), (120, 40)] {
        let mut term =
            Term::proto_with(&["--demo-pace", PACE_MS], cols, rows, &[("NO_COLOR", "1")]);
        term.wait_prompt(FREE);
        term.send("keep this\x1b\rexact draft");
        term.wait_until("the draft", |screen| {
            screen.row_starting("nika › keep this").is_some() && screen.contains("exact draft")
        });
        term.send("\x14");
        term.wait_until("the workspace with the draft", |screen| {
            screen.on_alt() && screen.contains("nika › keep this") && screen.contains("exact draft")
        });
        term.send("\x14");
        term.wait_until("inline with the draft", |screen| {
            !screen.on_alt()
                && screen.row_starting("nika › keep this").is_some()
                && screen.contains("exact draft")
        });
        term.settle(Duration::from_millis(300));
        assert!(
            !term.screen.seen(QUESTION),
            "a switch sent the draft\n{}",
            term.dump()
        );
        term.send(&"\x7f".repeat("keep this\nexact draft".chars().count()));
        term.wait_until("the draft erased", |screen| {
            screen.lines().iter().any(|line| line == FREE)
        });
        term.send("digest my notes\r");
        term.wait_text(HOLDS);
        term.send("typed while busy");
        term.wait_text("typed while busy");
        term.send("\x14");
        term.wait_until(
            "the question, in the workspace, the draft unsent",
            |screen| {
                screen.on_alt()
                    && screen.seen(QUESTION)
                    && screen.contains("reply › typed while busy")
            },
        );
        term.settle(WATCH);
        assert!(
            !term.screen.seen(PROPOSAL),
            "the draft typed while Nika worked was sent\n{}",
            term.dump()
        );
        leave_term(&mut term);
    }
}

/// `F4` as a terminal sends it.
const F4: &str = "\x1bOS";

/// The Session / Workbench switch (`F4`) rearranges the same conversation:
/// while typing, the two-line draft stays exact and nothing is sent; while
/// Nika works it acts at once, and the words typed meanwhile wait, unsent,
/// in the box of the question the turn ends on.
#[test]
fn the_layout_switch_while_typing_or_working_keeps_the_exact_draft() {
    for (cols, rows) in [(80, 24), (120, 40)] {
        let mut term =
            Term::proto_with(&["--demo-pace", PACE_MS], cols, rows, &[("NO_COLOR", "1")]);
        term.wait_prompt(FREE);
        term.send("\x14");
        term.wait_text("[Session] Workbench");
        term.send("keep this\x1b\rexact draft");
        term.wait_until("the draft", |screen| {
            screen.contains("nika › keep this") && screen.contains("exact draft")
        });
        term.send(F4);
        term.wait_until("the Workbench with the draft", |screen| {
            screen.contains("Session [Workbench]")
                && screen.contains("nika › keep this")
                && screen.contains("exact draft")
        });
        term.send(F4);
        term.wait_until("the Session again with the draft", |screen| {
            screen.contains("[Session] Workbench")
                && screen.contains("nika › keep this")
                && screen.contains("exact draft")
        });
        term.settle(Duration::from_millis(300));
        assert!(
            !term.screen.seen(QUESTION),
            "a switch sent the draft\n{}",
            term.dump()
        );
        term.send(&"\x7f".repeat("keep this\nexact draft".chars().count()));
        term.wait_until("the draft erased", |screen| !screen.contains("keep this"));
        term.send("digest my notes\r");
        term.wait_text(HOLDS);
        term.send("typed while busy");
        term.wait_text("typed while busy");
        term.send(F4);
        term.wait_until("the Workbench while Nika works", |screen| {
            screen.contains("Session [Workbench]") && screen.contains(HOLDS)
        });
        // The compact conversation may scroll the question itself out of
        // view: its reply prompt and the typeahead notice name it.
        term.wait_until("the question's prompt, the draft in its box", |screen| {
            screen.contains("reply › typed while busy") && screen.contains(NOTICE)
        });
        term.settle(WATCH);
        assert!(
            term.screen.contains("reply › typed while busy")
                && !term.screen.contains(APPLY)
                && !term.screen.seen(PROPOSAL),
            "the draft typed while Nika worked was sent\n{}",
            term.dump()
        );
        leave_term(&mut term);
    }
}

/// The three transcript rows a 60 × 8 focus view shows while Nika works.
fn reading_rows(term: &Term) -> Vec<String> {
    term.screen.lines().into_iter().take(3).collect()
}

/// Words typed while a scrolled focus turn works are set aside when it ends
/// on the proposal. The notice saying where they went lands below the rows
/// being read: the reading position stays, End shows the notice, and the
/// draft waits in the box, unsent.
#[test]
fn typeahead_set_aside_keeps_the_focus_reading_position() {
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
    // More pages than the transcript holds: the reading position is its first
    // row (the banner, above the sent line), marked by the reading hint.
    term.send("\x1b[5~\x1b[5~\x1b[5~\x1b[5~wvkx");
    term.wait_until(
        "scrolled back to the first line with a draft typed while Nika works",
        |screen| {
            screen.contains("reading earlier messages")
                && screen.contains("wvkx")
                && screen.contains(HOLDS)
        },
    );
    let before = reading_rows(&term);
    term.wait_prompt(APPLY);
    // Any later frame of the same turn end is read too.
    term.settle(Duration::from_millis(300));
    assert_eq!(
        reading_rows(&term),
        before,
        "the typeahead notice moved the reading position\n{}",
        term.dump()
    );
    assert!(
        term.screen.row_starting(&format!("{APPLY} wvkx")).is_some() && !term.screen.seen(SAVED),
        "the typeahead waits in the box and consents to nothing\n{}",
        term.dump()
    );
    term.send("\x1b[F");
    term.wait_text("you typed");
    term.wait_prompt(APPLY);
    leave_term(&mut term);
}
