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
fn typeahead_never_consents_to_a_proposal_painted_after_it() {
    typeahead_is_defused(1, "./notes/lundi.md", "yes", PROPOSAL, APPLY, SAVED);
}

/// Before the gate: a `yes` typed while the run starts must not answer the
/// gate the run pauses on.
#[test]
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

/// The workspace's own status note: the presentation is the workspace.
const WORKSPACE: &str = "workspace · F6 panel";
/// The two sizes the chooser is qualified at.
const CHOOSER_SIZES: [(u16, u16); 2] = [(80, 24), (120, 40)];
/// `Esc`, the arrows, `F6` and `Ctrl+O` as a terminal sends them.
const ESC: &str = "\x1b";
const DOWN: &str = "\x1b[B";
const UP: &str = "\x1b[A";
const F6: &str = "\x1b[17~";
const CTRL_O: &str = "\x0f";

/// The demo at its free prompt, inline or in the workspace (`Ctrl+T`).
fn opened(cols: u16, rows: u16, workspace: bool) -> Term {
    let mut term = Term::proto(&[], cols, rows);
    term.wait_prompt(FREE);
    if workspace {
        term.send("\x14");
        term.wait_text(WORKSPACE);
    }
    term
}

/// Nothing was sent: the demo answers any line with its question, which
/// never shows within the watch.
fn nothing_sent(term: &mut Term, what: &str) {
    term.settle(SETTLE);
    assert!(
        !term.screen.seen(QUESTION),
        "{what}: a line was sent\n{}",
        term.dump()
    );
}

/// The slash list, inline and in the workspace at both sizes: `/s` lists
/// the commands it begins, the arrows move the selection, `Tab` inserts the
/// selected command whole, the list then says `Enter` sends it, and `Esc`
/// hides the list with the exact draft kept. Nothing is ever sent.
#[test]
fn the_slash_list_chooses_and_inserts_and_esc_keeps_the_exact_draft() {
    for (cols, rows) in CHOOSER_SIZES {
        for workspace in [false, true] {
            let mut term = opened(cols, rows, workspace);
            let at = format!("{cols}x{rows} workspace={workspace}");
            term.send("/s");
            term.wait_until(&format!("{at}: the slash list"), |screen| {
                screen.contains("› /status") && screen.contains("  /show")
            });
            term.send(DOWN);
            term.wait_text("› /show");
            term.send(UP);
            term.wait_text("› /status");
            term.send("\t");
            term.wait_until(&format!("{at}: /status inserted whole"), |screen| {
                screen.contains("nika › /status") && screen.contains("Enter sends /status")
            });
            term.send(ESC);
            term.wait_until(&format!("{at}: the list hidden"), |screen| {
                !screen.contains("Enter sends /status") && screen.contains("nika › /status")
            });
            assert_eq!(
                term.screen.on_alt(),
                workspace,
                "{at}: Esc left\n{}",
                term.dump()
            );
            nothing_sent(&mut term, &at);
            leave(&mut term);
        }
    }
}

/// `Ctrl+O` opens the palette from any region, its search apart from the
/// draft (out of view meanwhile); `Esc` returns to the exact multi-line draft
/// and gives the keys back where they were: the composer inline, the preview
/// in the workspace, whose own `Esc` then returns to the composer, where the
/// next letter lands at the draft's end.
#[test]
fn the_palette_returns_to_the_exact_draft_and_its_focus() {
    for (cols, rows) in CHOOSER_SIZES {
        for workspace in [false, true] {
            let mut term = opened(cols, rows, workspace);
            let at = format!("{cols}x{rows} workspace={workspace}");
            palette_returns_to_the_draft(&mut term, &at, workspace);
        }
    }
}

/// The same proof with the terminal read in 64-byte slices at the size whose
/// workspace palette frame (its band over the conversation) outgrows the
/// process's line buffer: the draft is judged on the frame the palette
/// paints, never between its writes, whatever the scheduling.
#[test]
fn the_palette_hides_the_draft_on_a_complete_frame_read_in_slices() {
    for workspace in [false, true] {
        let mut term = Term::proto(&[], 120, 40).reading_at_most(64);
        term.wait_prompt(FREE);
        if workspace {
            term.send("\x14");
            term.wait_text(WORKSPACE);
        }
        let at = format!("120x40 workspace={workspace} read in slices");
        palette_returns_to_the_draft(&mut term, &at, workspace);
    }
}

fn palette_returns_to_the_draft(term: &mut Term, at: &str, workspace: bool) {
    term.send("line one\x1b\rline two");
    term.wait_until(&format!("{at}: the draft"), |screen| {
        screen.contains("nika › line one") && screen.contains("line two")
    });
    if workspace {
        // The keys leave the composer first: the palette still opens.
        term.send(F6);
        term.settle(Duration::from_millis(200));
    }
    term.send(CTRL_O);
    if workspace {
        // The palette's frame paints its band over the conversation and
        // reaches the terminal in more than one write: one complete native
        // frame says where the draft is, never the rows a first write leaves
        // stale.
        term.wait_workspace_frame(&format!("{at}: the palette"), |screen| {
            screen.contains("commands ›")
        });
    } else {
        term.wait_text("commands ›");
    }
    assert!(
        !term.screen.contains("line one"),
        "{at}: the draft is out of view under the palette\n{}",
        term.dump()
    );
    term.send("model");
    term.wait_until(&format!("{at}: the search"), |screen| {
        screen.contains("commands › model")
            && screen.contains("› /status")
            && screen.contains("/intelligence")
    });
    term.send(ESC);
    term.wait_until(&format!("{at}: the exact draft back"), |screen| {
        screen.contains("nika › line one")
            && screen.contains("line two")
            && !screen.contains("commands ›")
    });
    if workspace {
        // The preview has the keys again; its own Esc returns to the composer.
        term.send(ESC);
        term.settle(Duration::from_millis(200));
    }
    term.send("!");
    term.wait_text("line two!");
    nothing_sent(term, at);
    leave(term);
}

const ASIDE: &str = "Nika · Files";

/// Cancelling the palette gives the keys back to the region that held them,
/// the preview or the project aside, with the draft exact and nothing sent:
/// a letter typed after `Esc` never reaches the draft, `F6` moves on from that
/// region, and the aside opened before the palette is still over the object.
#[test]
fn cancelling_the_palette_returns_the_keys_to_the_region_that_held_them() {
    let mut term = opened(80, 24, true);
    term.send("keep me");
    term.wait_text("nika › keep me");
    // The preview holds the keys, then the palette opens over the composer.
    term.send(F6);
    term.send(CTRL_O);
    term.wait_text("commands ›");
    term.send(ESC);
    term.wait_until("the palette closed", |screen| {
        !screen.contains("commands ›") && screen.contains("nika › keep me")
    });
    term.send("x");
    term.settle(SETTLE);
    assert!(
        !term.screen.contains("keep mex"),
        "Esc gave the keys to the composer, not back to the preview\n{}",
        term.dump()
    );
    // From the preview, F6 reaches the aside, drawn over the object.
    term.send(F6);
    term.wait_text(ASIDE);
    term.send(CTRL_O);
    term.wait_text("commands ›");
    term.send(CTRL_O);
    term.wait_until("the aside back with the keys", |screen| {
        !screen.contains("commands ›") && screen.contains(ASIDE)
    });
    term.send("x");
    term.settle(SETTLE);
    assert!(!term.screen.contains("keep mex"), "{}", term.dump());
    // Esc in the aside returns to the composer, the draft and cursor exact.
    term.send(ESC);
    term.wait_until("the composer", |screen| !screen.contains(ASIDE));
    term.send("!");
    term.wait_text("nika › keep me!");
    nothing_sent(&mut term, "cancelling the palette");
    leave(&mut term);
}

/// A pointer deliberately choosing another panel closes the palette and
/// keeps that panel's keys. Its draft returns exact; the click never inserts
/// a command or sends an input, and cancellation never jumps to an old focus.
#[test]
fn clicking_another_panel_closes_the_palette_and_keeps_the_clicked_focus() {
    let mut term = opened(120, 40, true);
    term.send("keep pointer draft");
    term.wait_text("nika › keep pointer draft");
    term.send(F6);
    term.send(F6); // The aside has the keys when the palette opens.
    term.send(CTRL_O);
    term.wait_text("commands ›");
    let geometry = nika_tui::workspace::geometry::Geometry::of(
        ratatui::layout::Rect::new(0, 0, 120, 40),
        false,
    )
    .expect("workspace geometry");
    term.send(&format!(
        "\x1b[<0;{};{}M",
        geometry.object.x + 3,
        geometry.object.y + 3,
    ));
    term.wait_until("the palette dismissed by the object click", |screen| {
        !screen.contains("commands ›") && screen.contains("nika › keep pointer draft")
    });
    term.send(F6); // From the clicked object, F6 reaches the aside.
    term.send("x");
    term.settle(SETTLE);
    assert!(
        !term.screen.contains("keep pointer draftx"),
        "the palette restored its old aside focus over the click: {}",
        term.dump()
    );
    term.send(ESC);
    term.settle(Duration::from_millis(200)); // Separate Escape from a terminal Alt chord.
    term.send("!");
    term.wait_text("nika › keep pointer draft!");
    nothing_sent(&mut term, "clicking out of the palette");
    leave(&mut term);
}

/// A view key chosen in the palette acts from the region that held the
/// keys (`F6` from the preview reaches the aside); a command inserted with
/// `Tab` moves the keys to the composer, so the separate `Enter` is the
/// human's.
#[test]
fn a_view_key_chosen_acts_from_the_region_that_held_the_keys() {
    let mut term = opened(80, 24, true);
    term.send(F6);
    term.send(CTRL_O);
    term.wait_text("commands ›");
    term.send("next panel");
    term.wait_text("› F6");
    term.send("\r");
    term.wait_until("F6 pressed from the preview: the aside", |screen| {
        !screen.contains("commands ›") && screen.contains(ASIDE)
    });
    term.send(CTRL_O);
    term.wait_text("commands ›");
    term.send("status\t");
    term.wait_until("/status inserted, the keys on the composer", |screen| {
        screen.contains("nika › /status") && !screen.contains(ASIDE)
    });
    term.send("\x7f");
    term.wait_text(&format!("nika › {}", "/status".trim_end_matches('s')));
    nothing_sent(&mut term, "choosing in the palette");
    leave(&mut term);
}

/// `Enter` on a command in the palette runs it once, as its line, the draft
/// untouched: the demo plays one turn per line, so a second line (a repeat,
/// or the draft) would show its proposal. An `Enter` repeated in the same
/// write counts once; paced, it says the turn works. Once the turn's answer
/// is painted and the repeat window has passed, the draft's own `Enter`
/// sends it.
#[test]
fn a_command_run_from_the_palette_runs_once_and_keeps_the_draft() {
    for (paced, workspace) in [(false, false), (false, true), (true, false)] {
        let at = format!("paced={paced} workspace={workspace}");
        let args: &[&str] = if paced { &["--demo-pace", "900"] } else { &[] };
        let mut term = Term::proto(args, 100, 32);
        term.wait_prompt(FREE);
        if workspace {
            term.send("\x14");
            term.wait_text(WORKSPACE);
        }
        term.send("keep me");
        term.wait_text("nika › keep me");
        term.send(CTRL_O);
        term.wait_text("commands ›");
        term.send("status");
        term.wait_text("› /status");
        term.send("\r\r");
        if paced {
            term.wait_until(&format!("{at}: the repeat waits for the turn"), |screen| {
                screen.contains("Nika is working · Enter sends when it is your turn")
            });
        }
        term.wait_until(&format!("{at}: /status ran"), |screen| {
            screen.seen(QUESTION)
                && screen
                    .lines()
                    .iter()
                    .any(|row| row.contains(&format!("{REPLY} keep me")))
        });
        term.settle(SETTLE);
        assert!(
            !term.screen.seen(PROPOSAL),
            "{at}: a second line was sent\n{}",
            term.dump()
        );
        term.send("\r");
        term.wait_until(
            &format!("{at}: the draft sent by its own Enter"),
            |screen| screen.seen(PROPOSAL),
        );
        leave(&mut term);
    }
}

/// A paste is data in the palette's search and in a slash draft alike: a
/// pasted `yes`, `run it` and `/quit` act on nothing.
#[test]
fn a_paste_into_the_palette_or_a_slash_draft_is_data() {
    for (cols, rows) in CHOOSER_SIZES {
        let mut term = Term::proto(&[], cols, rows);
        term.wait_prompt(FREE);
        term.send(CTRL_O);
        term.wait_text("commands ›");
        term.paste(PASTE);
        term.wait_text("commands › yes run it /quit");
        nothing_sent(&mut term, "a paste into the palette");
        assert!(term.screen.contains("commands ›"), "{}", term.dump());
        term.send(ESC);
        term.wait_until("the empty draft back", |screen| {
            screen.lines().iter().any(|line| line == FREE)
        });
        term.send("/s");
        term.wait_text("› /status");
        term.paste(PASTE);
        term.wait_until("the paste in the draft", |screen| {
            screen.row_starting("nika › /syes").is_some() && screen.contains("/quit")
        });
        nothing_sent(&mut term, "a paste into a slash draft");
        leave(&mut term);
    }
}

/// At a run's gate the chooser says no command answers it; `Tab` in the slash
/// list or the palette fills the draft and the gate keeps waiting.
#[test]
fn choosing_at_a_gate_fills_the_draft_and_never_answers_it() {
    for (cols, rows) in CHOOSER_SIZES {
        let mut term = Term::proto(&[], cols, rows);
        term.wait_prompt(FREE);
        term.walk(&JOURNEY[..4]);
        term.send("/");
        term.wait_until("the gate's context above the list", |screen| {
            screen.contains("A gate waits · no command answers it") && screen.contains("› /help")
        });
        term.send("\t");
        term.wait_until("/help inserted", |screen| {
            screen.row_starting(&format!("{ANSWER} /help")).is_some()
        });
        term.send(CTRL_O);
        term.wait_text("commands ›");
        term.send("inspect\t");
        term.wait_until("/show inserted over /help", |screen| {
            screen.row_starting(&format!("{ANSWER} /show")).is_some()
        });
        term.settle(SETTLE);
        assert!(
            !term.screen.seen(RESULT),
            "choosing answered the gate\n{}",
            term.dump()
        );
        leave(&mut term);
    }
}

/// The palette names the conversation explicitly: from the preview or the
/// aside, its `PgUp` and `End` entries must move that transcript, give the
/// conversation the keys, keep the draft and leave a pending Save unanswered.
///
/// Each check waits for one complete native frame. A frame larger than the
/// process's line buffer leaves it in several writes, and a read between
/// them shows the new transcript above a stale composer row, so a wait that
/// holds before the frame's end judges a half-painted screen.
fn palette_conversation_navigation(term: &mut Term) {
    term.wait_prompt(FREE);
    term.send("\x14");
    term.wait_text(WORKSPACE);
    term.walk(&JOURNEY[..2]);
    term.send("keep my navigation draft");
    term.wait_text("Save? › keep my navigation draft");
    for origin in [1, 2] {
        for _ in 0..origin {
            term.send(F6);
        }
        term.send(CTRL_O);
        term.wait_text("commands ›");
        term.send("earlier messages");
        term.wait_text("› PgUp");
        term.send("\r");
        term.wait_workspace_frame("the conversation scrolled from another region", |screen| {
            !screen.contains("commands ›")
                && screen.contains("↓ latest")
                && screen.contains("Save? › keep my navigation draft")
                && screen.lines()[0].contains("[Conversation]")
        });
        for _ in 0..origin {
            term.send(F6);
        }
        term.send(CTRL_O);
        term.wait_text("commands ›");
        term.send("back to the latest message");
        term.wait_text("› End");
        term.send("\r");
        term.wait_workspace_frame("the conversation returned to its latest row", |screen| {
            !screen.contains("commands ›")
                && !screen.contains("↓ latest")
                && screen.contains("Save? › keep my navigation draft")
                && screen.lines()[0].contains("[Conversation]")
        });
        term.settle(SETTLE);
        assert!(
            !term.screen.seen(SAVED),
            "a navigation entry saved: {}",
            term.dump()
        );
    }
    leave(term);
}

#[test]
fn palette_conversation_navigation_reaches_the_transcript_from_other_regions() {
    palette_conversation_navigation(&mut Term::proto(&[], 80, 24));
}

/// The same proof with the terminal read in 64-byte slices: every wait is
/// judged inside the frames too, so only a complete frame can satisfy it,
/// whatever the scheduling (the race the whole-read run only shows under load).
#[test]
fn palette_conversation_navigation_holds_when_frames_arrive_in_slices() {
    palette_conversation_navigation(&mut Term::proto(&[], 80, 24).reading_at_most(64));
}
