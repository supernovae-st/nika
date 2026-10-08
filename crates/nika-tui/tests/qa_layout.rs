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
//! the design pass kept as acceptance checks for the component that owns each
//! (ignored while the defect stands, never deleted).
//!
//! The screen is `qa_support::vt::Screen`: what a terminal shows after the
//! bytes, so a proof reads rows and words, not escape fragments.

mod qa_support;

use nika_tui::workspace::geometry::{Arrangement, Geometry, Layout};
use qa_support::vt::Screen;
use qa_support::{
    ANSWER, APPLY, FREE, FREE_HINT, GATE, GATE_HINT, JOURNEY, PROPOSAL, QUESTION, REPLY, RESULT,
    SAVED, SIZES, Step, Term, assert_restored, exit_code,
};
use ratatui::layout::Rect;

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
/// marker the accent from the workspace RGB product palette.
#[test]
fn with_colour_the_gate_wears_the_warning_slot() {
    let mut term = Term::proto(&["--color"], 80, 24);
    term.wait_prompt(FREE);
    term.walk(&JOURNEY[..4]);
    let hues = term.screen.hues();
    assert!(
        hues.contains("38;2;242;193;125"),
        "no warning hue at the gate: {hues:?}"
    );
    assert!(
        hues.contains("38;2;140;177;255"),
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
#[ignore = "defect: under NO_COLOR a forced colour paints no hue (crossterm reads NO_COLOR) and the renderer has dropped the weight substitutes · one colour owner needed (terminal.rs: Colored::set_ansi_color_disabled from Options.color) · terminal rendering and input handling"]
fn a_forced_colour_under_no_color_is_the_colour_the_terminal_sees() {
    let mut term = Term::proto_with(&["--color"], 80, 24, &[("NO_COLOR", "1")]);
    term.wait_prompt(FREE);
    term.walk(&JOURNEY[..4]);
    let hues = term.screen.hues();
    assert!(
        hues.contains("38;2;242;193;125"),
        "no warning hue at the gate: {hues:?}"
    );
    leave(&mut term);
}

/// B1 · the hint sits right under the composer: the live area takes its
/// natural height instead of stretching the editor over the 12-row viewport.
#[test]
#[ignore = "defect: B1 · the inline composer stretches over the 12-row viewport (9 blank rows between prompt and hint at every size) · terminal rendering"]
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
#[ignore = "defect: B2 · the echo reads `› nika › digest my monday notes` (two markers) · terminal rendering"]
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
#[ignore = "defect: B3 · focus puts a blank row between every run line · terminal rendering"]
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
#[ignore = "defect: B4 · after two Ctrl+C the composer, its hint and the interrupted notice stay painted · terminal rendering"]
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
#[ignore = "defect: a width shrink in inline wipes the screen and the waiting proposal leaves it; only apply? and its hint stay (ratatui resize clears on a horizontal shrink and moves the viewport to row 0) · inline viewport handling"]
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

/// The full-screen view keeps the candidate across sizes: a proposal waiting for consent when the
/// terminal shrinks below the workspace's minimum (50x14) and grows back is the same proposal
/// on screen afterwards, and the consent typed then saves it.
#[test]
fn a_waiting_proposal_outlives_a_resize_and_its_consent_saves_it() {
    let mut term = Term::proto(&["--focus"], 120, 40);
    term.wait_prompt(FREE);
    term.walk(&JOURNEY[..2]);
    term.resize(50, 14);
    term.wait_until("the consent prompt at 50x14", |screen| {
        screen.row_starting(APPLY).is_some() && screen.lines().len() == 14
    });
    term.resize(120, 40);
    term.wait_until("the same proposal back at 120x40", |screen| {
        screen.contains(PROPOSAL) && screen.row_starting(APPLY).is_some()
    });
    term.send("yes\r");
    term.wait_until("the proposal saved", |screen| screen.contains(SAVED));
    term.wait_prompt(FREE);
    leave(&mut term);
}

/// The header's layout switch, the layout in view in brackets.
const SESSION_SWITCH: &str = "[Session] Workbench · F4";
const WORKBENCH_SWITCH: &str = "Session [Workbench] · F4";
/// The conversation's title as the rule under the object (Workbench).
const RULE: &str = "── ◌ this conversation";
/// `F4`, `F6` and `Shift+F6` as an xterm sends them (never a lone `Esc`,
/// which a mouse report right behind it could join).
const F4: &str = "\x1bOS";
const F6: &str = "\x1b[17~";
const SHIFT_F6: &str = "\x1b[17;2~";

/// A left press at `from`, a move to `to` with the button held, its release:
/// the SGR reports of an xterm (cells from zero here, from one on the wire).
fn drag(term: &mut Term, from: (u16, u16), to: (u16, u16)) {
    term.send(&format!("\x1b[<0;{};{}M", from.0 + 1, from.1 + 1));
    term.send(&format!("\x1b[<32;{};{}M", to.0 + 1, to.1 + 1));
    term.send(&format!("\x1b[<0;{};{}m", to.0 + 1, to.1 + 1));
}

/// A left click at `at`.
fn click(term: &mut Term, at: (u16, u16)) {
    let (x, y) = (at.0 + 1, at.1 + 1);
    term.send(&format!("\x1b[<0;{x};{y}M\x1b[<0;{x};{y}m"));
}

/// The column where `needle` starts on `row` (one cell per character there).
fn column_of(row: &str, needle: &str) -> u16 {
    let at = row.find(needle).expect("on the row");
    u16::try_from(row[..at].chars().count()).expect("a column")
}

/// The Workbench rule's row.
fn rule_row(screen: &Screen) -> Option<u16> {
    screen.row_of(RULE).and_then(|row| u16::try_from(row).ok())
}

/// The switch is right-aligned at this width. After a grow, old cells can
/// remain visible at their old column until the new frame reaches us.
fn switch_at_width(screen: &Screen, switch: &str, cols: u16) -> bool {
    use unicode_width::UnicodeWidthStr as _;
    screen.lines().first().is_some_and(|row| {
        row.find(switch)
            .is_some_and(|at| row[..at].width() == usize::from(cols).saturating_sub(switch.width()))
    })
}

/// The conversation's rule column stands at `x` beside the object.
fn rule_at(screen: &Screen, x: u16, rows: std::ops::Range<u16>) -> bool {
    let lines = screen.lines();
    rows.filter_map(|y| lines.get(usize::from(y)))
        .filter(|line| line.chars().nth(usize::from(x)) == Some('│'))
        .count()
        >= 10
}

/// One step of the demo journey in the workspace, whose prompts are inset.
fn step_in_workspace(term: &mut Term, step: &Step) {
    term.send(step.send);
    let (shows, prompt) = (step.shows, step.prompt);
    term.wait_until(shows, |screen| {
        screen.seen(shows) && screen.contains(prompt)
    });
}

/// Every view change at 120x40 while `typed` sits in the composer: `F4`, a
/// dragged Workbench rule, the keys that move it (`F6` to the object, `0`,
/// `-`, `Shift+F6` back to the composer), the header's switch, a dragged
/// Session rule and `0`, then three resizes. After each, the switch names
/// the layout in view and the draft is still in the composer.
fn rearrange(term: &mut Term, typed: &str) {
    let area = Rect::new(0, 0, 120, 40);
    term.send(F4);
    // The whole frame, not its first rows: the rule is painted below the header.
    term.wait_until("the Workbench", |s| {
        s.contains(WORKBENCH_SWITCH) && rule_row(s).is_some() && s.contains(typed)
    });
    let rule = rule_row(&term.screen).expect("the Workbench rule");
    drag(term, (60, rule), (60, rule - 3));
    term.wait_until("the rule three rows up", |s| rule_row(s) == Some(rule - 3));
    let workbench = Arrangement::of(Layout::Workbench);
    let automatic = Geometry::arranged(area, false, &workbench).expect("fits");
    term.send(F6);
    term.send("0");
    term.wait_until("the automatic rule", |s| {
        rule_row(s) == Some(automatic.conversation.y)
    });
    term.send("--");
    term.send(SHIFT_F6);
    term.wait_until("two rows to the conversation", |s| {
        rule_row(s) == Some(automatic.conversation.y - 2) && s.contains(typed)
    });
    let header = term.screen.lines()[0].clone();
    click(term, (column_of(&header, "Session") + 2, 0));
    let session = Geometry::of(area, false).expect("fits");
    let edge = session.conversation.right() - 1;
    let rows = session.conversation.y..session.conversation.bottom();
    term.wait_until("the Session", |s| {
        s.contains(SESSION_SWITCH) && rule_at(s, edge, rows.clone()) && rule_row(s).is_none()
    });
    drag(term, (edge, 20), (edge + 6, 21));
    term.wait_until("the rule six columns right", |s| {
        rule_at(s, edge + 6, rows.clone())
    });
    term.send(F6);
    term.send("0");
    term.send(SHIFT_F6);
    term.wait_until("the automatic rule", |s| {
        rule_at(s, edge, rows.clone()) && s.contains(typed)
    });
    for (cols, rows) in [(80, 24), (180, 48), (120, 40)] {
        assert_eq!(term.screen.beyond(), 0, "before resize to {cols}x{rows}");
        term.resize(cols, rows);
        term.wait_until("rearranged at the new size", |s| {
            s.size() == (usize::from(cols), usize::from(rows))
                && switch_at_width(s, SESSION_SWITCH, cols)
                && s.contains(typed)
        });
        prove_settled_bounds(term, cols, rows);
    }
}

/// A frame from the old size may still arrive during SIGWINCH. Judge a
/// complete redraw after the new size settled, without dropping checks of
/// the steady frames before and after that transition.
fn prove_settled_bounds(term: &mut Term, cols: u16, rows: u16) {
    let window = std::time::Duration::from_millis(400);
    term.settle(window);
    term.screen.clear_beyond();
    let mark = term.mark();
    term.send("\x0c");
    term.spin_until_bytes(mark, b"\x1b[2J");
    term.settle(window);
    assert_eq!(term.screen.beyond(), 0, "settled redraw at {cols}x{rows}");
}

/// Layout changes are view changes: with a consent-shaped draft in the
/// composer, neither the proposal nor the gate is answered by `F4`, the
/// header's switch, a dragged separator, the separator keys or a resize; the
/// draft stays unsent until `Enter`, and each decision is then the human's.
#[test]
fn layout_changes_keep_the_draft_unsent_and_the_decisions_waiting() {
    let mut term = Term::proto(&[], 120, 40);
    term.wait_prompt(FREE);
    term.send("\x14");
    term.wait_text(SESSION_SWITCH);
    for step in &JOURNEY[..2] {
        step_in_workspace(&mut term, step);
    }
    term.send("yes");
    rearrange(&mut term, "Save? › yes");
    assert!(
        term.screen.contains(PROPOSAL),
        "the proposal\n{}",
        term.dump()
    );
    assert!(
        !term.screen.seen(SAVED),
        "a view change saved\n{}",
        term.dump()
    );
    term.send("\r");
    term.wait_until(SAVED, |s| s.seen(SAVED) && s.contains(FREE));
    term.send("run it\r");
    term.wait_until(GATE, |s| s.seen(GATE) && s.contains(ANSWER));
    term.send("yes");
    rearrange(&mut term, "answer › yes");
    assert!(term.screen.seen(GATE), "the gate\n{}", term.dump());
    assert!(
        !term.screen.seen(RESULT),
        "a view change answered the gate\n{}",
        term.dump()
    );
    term.send("\r");
    term.wait_until(RESULT, |s| s.seen(RESULT) && s.contains(FREE));
    assert_eq!(term.screen.beyond(), 0, "addressed past the screen");
    leave(&mut term);
}

/// The minimum, the three target sizes and a short pane, in both layouts:
/// the composer's prompt and the switch are on screen, without colour too.
#[test]
fn both_layouts_keep_the_composer_and_the_switch_at_every_size() {
    let mut term = Term::proto_with(&[], 120, 40, &[("NO_COLOR", "1")]);
    term.wait_prompt(FREE);
    term.send("\x14");
    term.wait_text(SESSION_SWITCH);
    term.send("draft here");
    for (switch, key) in [(SESSION_SWITCH, ""), (WORKBENCH_SWITCH, F4)] {
        term.send(key);
        for (cols, rows) in [(60, 16), (80, 24), (120, 40), (180, 48), (100, 32)] {
            term.resize(cols, rows);
            term.wait_until(&format!("{switch} at {cols}x{rows}"), |s| {
                s.size() == (usize::from(cols), usize::from(rows))
                    && switch_at_width(s, switch, cols)
                    && s.contains("nika › draft here")
            });
        }
    }
    assert!(term.screen.hues().is_empty(), "{:?}", term.screen.hues());
    // Below the minimum the focus view stands in: the draft stays, and F4
    // switches nothing there (no layout is drawn). The shell decides F4 at the
    // size it last drew, so F4 waits for a frame drawn at 59x15: the focus
    // view's hint, its last row, which no Workbench paints (cropped by the
    // resize, the 100-column Workbench can keep its composer row, never its
    // switch).
    term.resize(59, 15);
    term.wait_until("the focus view", |s| {
        s.size() == (59, 15)
            && s.contains(FREE_HINT)
            && s.contains("nika › draft here")
            && !s.contains("Workbench")
    });
    // Keys are read in order (one broker): the key typed after F4 shows in the
    // draft only once F4 was read, here, before the grow; erasing it gives back
    // the exact draft.
    term.send(F4);
    term.send("!");
    term.wait_until("F4 read in the focus view", |s| {
        s.lines().iter().any(|row| row == "nika › draft here!")
    });
    term.send("\x7f");
    term.wait_until("the exact draft", |s| {
        s.lines().iter().any(|row| row == "nika › draft here")
    });
    term.resize(120, 40);
    term.wait_until("the same Workbench again", |s| {
        s.contains(WORKBENCH_SWITCH) && s.contains("nika › draft here")
    });
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
