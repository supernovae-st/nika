// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a proof that cannot read its screen fails"
)]
//! Reception proofs of input on the demo fixture (`docs/qa/tui/RECEPTION.md`,
//! J2): a multi-line paste is data at every waiting state, a line typed
//! before a decision is painted never answers it, typeahead into a free
//! prompt is kept, and a resize storm while typing loses no character.
//!
//! "Typed before it is painted" is exact on the fixture: the request and the
//! answer leave in ONE write, so the answer is read before the turn that asks
//! for it has even run.

mod qa_support;

use std::time::Duration;

use qa_support::{
    ANSWER, APPLY, FREE, GATE, JOURNEY, PROPOSAL, QUESTION, REPLY, RESULT, SAVED, Term,
    assert_restored, exit_code,
};

/// A paste that would act three times if it were read as keys.
const PASTE: &str = "yes\nrun it\n/quit";
/// One token per character class the composer meets, no space inside.
const TOKEN: &str = "abcdefghijklmnopqrstuvwxyz0123456789";
/// Where a resize storm starts: a size the storm never visits. The renderer
/// repaints only when the size it reads differs from the last one it knew,
/// so a storm that returned to its starting size before the renderer read
/// any would leave the terminal as the transient sizes cut it (observed
/// under load); starting elsewhere makes the final size always news.
const STORM_START: (u16, u16) = (110, 36);
/// The sizes a resize storm walks through.
const STORM: [(u16, u16); 6] = [
    (100, 32),
    (80, 24),
    (160, 48),
    (90, 30),
    (132, 43),
    (120, 40),
];
/// Long enough for a decision to show once its input has been read.
const SETTLE: Duration = Duration::from_millis(500);

fn leave(term: &mut Term) {
    term.send("\x03");
    term.wait_text("Ctrl+C again leaves");
    term.send("\x03");
    let status = term.finish();
    assert_eq!(exit_code(status), Some(130), "{status:?}\n{}", term.dump());
    assert_restored(term);
}

/// Paste under `prompt`: the three lines land in the composer, nothing acts
/// (`next` never shows, the prompt stays), then the draft is erased.
fn paste_is_data(term: &mut Term, prompt: &str, next: &str) {
    term.paste(PASTE);
    let first = format!("{prompt} yes");
    term.wait_until("the pasted lines in the composer", |screen| {
        screen.row_starting(&first).is_some() && screen.contains("/quit")
    });
    term.settle(SETTLE);
    assert!(
        !term.screen.seen(next),
        "a paste under `{prompt}` acted: {next:?} showed\n{}",
        term.dump()
    );
    assert!(
        term.screen.row_starting(&first).is_some(),
        "{}",
        term.dump()
    );
    term.send(&"\x7f".repeat(PASTE.chars().count()));
    term.wait_until("the composer emptied", |screen| {
        screen.lines().iter().any(|line| line == prompt)
    });
}

#[test]
fn a_multi_line_paste_stays_data_at_every_waiting_state() {
    let mut term = Term::proto(&[], 100, 32);
    term.wait_prompt(FREE);
    let mut prompt = FREE;
    for step in JOURNEY {
        paste_is_data(&mut term, prompt, step.shows);
        term.walk(&[step]);
        prompt = step.prompt;
    }
    leave(&mut term);
}

/// The typeahead law of the design amendment (21:15Z): after a turn that
/// ends on a decision, the keys typed before it was painted go back to the
/// draft and their Enter is dropped. `walked` turns are walked first; then
/// `request` and `typed` leave in ONE write, so `typed` is read before the
/// turn that `request` starts has run. The decision `shows` with `prompt`;
/// `answered` must never show; the draft holds `typed` without its Enter.
fn typeahead_is_defused(
    walked: usize,
    request: &str,
    typed: &str,
    shows: &str,
    prompt: &str,
    answered: &str,
) {
    let mut term = Term::proto(&[], 100, 32);
    term.wait_prompt(FREE);
    term.walk(&JOURNEY[..walked]);
    term.send(&format!("{request}\r{typed}\r"));
    term.wait_until(shows, |screen| screen.seen(shows));
    term.settle(SETTLE);
    assert!(
        !term.screen.seen(answered),
        "`{typed}` typed before `{prompt}` was painted answered it ({answered:?} showed)\n{}",
        term.dump()
    );
    let draft = format!("{prompt} {typed}");
    assert!(
        term.screen.row_starting(&draft).is_some(),
        "the typed words did not come back to the draft\n{}",
        term.dump()
    );
    leave(&mut term);
}

/// Before the question: an answer typed while the intent is read must not
/// answer the question the turn asks.
#[test]
#[ignore = "defect: typeahead answers a question painted after it · app.rs replays deferred keys, only the two cost questions discard them · app.rs input handling"]
fn typeahead_never_answers_a_question_painted_after_it() {
    typeahead_is_defused(
        0,
        "digest my monday notes",
        "./notes/lundi.md",
        QUESTION,
        REPLY,
        PROPOSAL,
    );
}

/// Before the proposal: a `yes` typed while Nika compiles must not consent
/// to the bytes it is about to show. Observed at 513ca8465 (100x32 inline),
/// right under the proposal nobody had seen yet:
///
/// ```text
/// identity · 9f3c1a · these exact bytes, nothing else
/// › apply? › yes
/// saved ./digest-notes.nika · check · VALID · ACCESS READY · CAPACITY FIT · RUN READY
/// say « run it » to run it once · ceiling $0.25
/// nika ›
/// ```
#[test]
#[ignore = "defect: typeahead consents at apply?"]
fn typeahead_never_consents_to_a_proposal_painted_after_it() {
    typeahead_is_defused(1, "./notes/lundi.md", "yes", PROPOSAL, APPLY, SAVED);
}

/// Before the gate: a `yes` typed while the run starts must not answer the
/// gate the run pauses on.
#[test]
#[ignore = "defect: typeahead answers a gate painted after it · same cause · app.rs input handling"]
fn typeahead_never_answers_a_gate_painted_after_it() {
    typeahead_is_defused(3, "run it", "yes", GATE, ANSWER, RESULT);
}

/// A consent typed while the proposal IS on screen is fresh; the words typed
/// right after it land in the next free prompt as an unsent draft: kept, not
/// lost, not sent.
#[test]
fn typeahead_into_a_free_prompt_is_kept_as_an_unsent_draft() {
    let mut term = Term::proto(&[], 100, 32);
    term.wait_prompt(FREE);
    term.walk(&JOURNEY[..2]);
    term.send("yes\rdraft-b");
    let draft = format!("{FREE} draft-b");
    term.wait_until("the draft in the free prompt", |screen| {
        screen.seen(SAVED) && screen.row_starting(&draft).is_some()
    });
    term.settle(SETTLE);
    assert!(
        !term.screen.seen(GATE),
        "the draft was sent\n{}",
        term.dump()
    );
    leave(&mut term);
}

/// A draft of several lines leaves as ONE line: `Alt+Enter` (a terminal
/// sends `ESC CR`) and `Ctrl+J` break the line, only `Enter` sends, and the
/// fixture answers one turn, not three.
#[test]
fn a_multi_line_draft_is_sent_whole() {
    let mut term = Term::proto(&[], 100, 32);
    term.wait_prompt(FREE);
    term.send("read ./notes\x1b\rand digest them\x0athe monday ones\r");
    term.wait_until(QUESTION, |screen| {
        screen.seen(QUESTION) && screen.row_starting(REPLY).is_some()
    });
    term.settle(SETTLE);
    assert!(
        !term.screen.seen(PROPOSAL),
        "a line break sent a line\n{}",
        term.dump()
    );
    let transcript = term.screen.transcript();
    let first = transcript
        .iter()
        .position(|line| line.contains("read ./notes"))
        .expect("the first line of the draft");
    assert!(
        transcript[first + 1].contains("and digest them")
            && transcript[first + 2].contains("the monday ones"),
        "the three lines left as one block\n{}",
        term.dump()
    );
    leave(&mut term);
}

/// Wide glyphs (CJK, emoji) and a combining accent go through whole: the
/// draft shows them, the line sent keeps them, nothing is addressed past
/// the edge.
#[test]
fn wide_and_combining_glyphs_go_through_whole() {
    let text = "日本語のメモ e\u{301}te\u{301} 🦋";
    let mut term = Term::proto(&[], 80, 24);
    term.wait_prompt(FREE);
    term.send(text);
    let draft = format!("{FREE} {text}");
    term.wait_until("the draft", |screen| screen.row_starting(&draft).is_some());
    term.send("\r");
    term.wait_until(QUESTION, |screen| screen.seen(QUESTION));
    assert!(
        term.screen.seen(text),
        "the line sent lost a glyph\n{}",
        term.dump()
    );
    assert_eq!(term.screen.beyond(), 0);
    leave(&mut term);
}

/// A word wider than the composer (a long path, a URL, a hash, a sentence in
/// a script written without spaces) wraps onto the next row and every
/// character typed stays visible, the cursor with it.
#[test]
#[ignore = "defect: a word wider than the composer is clipped at the edge, the rest typed blind · composer.rs uses WrapMode::Word, whose words wider than the viewport are not split (ratatui-textarea 0.9 docs); WordOrGlyph falls back to graphemes · composer.rs sizing"]
fn a_word_wider_than_the_composer_stays_visible() {
    let word: String = TOKEN.repeat(3);
    let mut term = Term::proto(&[], 80, 24);
    term.wait_prompt(FREE);
    term.send(&word);
    term.settle(SETTLE);
    let shown: String = term
        .screen
        .lines()
        .iter()
        .map(|line| line.strip_prefix(FREE).unwrap_or(line).trim())
        .collect();
    assert!(
        shown.contains(&word),
        "the {}-character word is not all on screen\n{}",
        word.len(),
        term.dump()
    );
    leave(&mut term);
}

/// A resize every six characters while a token is typed: every character
/// lands, in order, and the line sent holds the whole token.
fn resize_storm_keeps_every_character(focus: bool) {
    let args: &[&str] = if focus { &["--focus"] } else { &[] };
    let mut term = Term::proto(args, STORM_START.0, STORM_START.1);
    term.wait_prompt(FREE);
    for (index, ch) in TOKEN.chars().enumerate() {
        term.send(ch.encode_utf8(&mut [0; 4]));
        if index % 6 == 5 {
            let (cols, rows) = STORM[(index / 6) % STORM.len()];
            term.resize(cols, rows);
        }
        term.pump();
    }
    let typed = format!("{FREE} {TOKEN}");
    term.wait_until("the whole token in the composer", |screen| {
        screen.row_starting(&typed).is_some()
    });
    // Frames drawn for an older size may have landed after a resize (a
    // terminal clamps them); addressing is judged from the settled size on.
    term.settle(SETTLE);
    term.screen.clear_beyond();
    term.send("\r");
    term.wait_until(QUESTION, |screen| {
        screen.seen(QUESTION) && screen.row_starting(REPLY).is_some()
    });
    assert!(term.screen.seen(TOKEN), "the line sent lost a character");
    assert_eq!(term.screen.beyond(), 0, "addressed past the screen");
    leave(&mut term);
}

#[test]
fn a_resize_storm_while_typing_loses_no_character_inline() {
    resize_storm_keeps_every_character(false);
}

#[test]
fn a_resize_storm_while_typing_loses_no_character_in_focus() {
    resize_storm_keeps_every_character(true);
}
