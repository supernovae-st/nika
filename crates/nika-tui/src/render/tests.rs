// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The live area and the transcript as drawn: the busy row, the hint, the
//! rail, the boxed composer, the ASCII column and the scroll cues.

#![allow(clippy::expect_used)]

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::*;
use crate::model::{Asked, Beat, Offer, Script, Shape};

fn state_after_demo(presentation: Presentation) -> UiState {
    let mut state = UiState::new(presentation, false, (80, 24));
    let mut script = Script::demo();
    for beat in script.open() {
        state.apply(beat);
    }
    for beat in script.submit("digest my notes") {
        state.apply(beat);
    }
    state
}

fn row(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol().to_owned())
        .collect::<String>()
        .trim_end()
        .to_owned()
}

#[test]
fn a_paste_beyond_terminal_height_keeps_its_text_and_draws_without_overflow() {
    let text = "input row\n".repeat(70_000) + "last input";
    let mut composer = Composer::new();
    composer.paste(&text);
    assert_eq!(composer.content_rows(80), 70_001);
    for (width, height) in [(12, 4), (80, 24), (180, 48)] {
        let mut state = UiState::new(Presentation::Focus, false, (width, height));
        state.rail = "Draft · Saved · Checked".to_owned();
        let live = live_rows(&state, &composer, width, height);
        assert!(live <= height.saturating_div(2).max(3));
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        terminal
            .draw(|frame| draw_focus(frame, &state, &composer))
            .expect("large paste draws");
        assert_eq!(
            composer.text(),
            text,
            "drawing must not truncate the user's draft"
        );
    }
}

#[test]
fn a_block_keeps_its_glyph_on_the_first_line_only() {
    let block = Committed::new(Kind::Gate, "write ./digest.md\noverwrite?");
    let lines = block_lines(&block, false, false);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].spans[0].content.as_ref(), "⏸ ");
    assert_eq!(lines[1].spans[0].content.as_ref(), "  ");
    assert_eq!(wrapped_rows(&lines, 80), 2);
    // Word wrapping needs five rows; cell division used to clip the last one.
    assert_eq!(wrapped_rows(&lines, 10), 5);
}

#[test]
fn word_wrapping_preserves_the_final_confirmation_line() {
    let block = Committed::new(
        Kind::Question,
        "alpha bravo charlie delta echo foxtrot échéance alpha bravo charlie delta echo foxtrot\nContinue once? yes / no",
    );
    for width in [12, 20, 40, 80] {
        let rows = wrapped_rows(&block_lines(&block, false, false), width);
        let mut buffer = Buffer::empty(Rect::new(0, 0, width, rows));
        render_block(&block, false, false, &mut buffer);
        let shown = (0..rows)
            .map(|y| row(&buffer, y))
            .collect::<Vec<_>>()
            .join(" ");
        let words = shown.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            words.ends_with("Continue once? yes / no"),
            "width {width}: {shown:?}"
        );
    }
}

/// The busy row's marker turns with the loader's frame and stays the
/// still dot when no frame is set (reduced motion).
#[test]
fn the_busy_row_turns_the_loader_and_stays_still_without_a_frame() {
    let mut state = UiState::new(Presentation::Inline, false, (60, 5));
    state.busy = Some("working through your words · 3s".to_owned());
    state.spinner = Some(3);
    let composer = Composer::new();
    let backend = TestBackend::new(60, 5);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| draw_inline(frame, &state, &composer))
        .expect("draw");
    let turning = row(terminal.backend().buffer(), 0);
    assert!(turning.starts_with("⠸ working"), "{turning:?}");
    state.spinner = None;
    terminal
        .draw(|frame| draw_inline(frame, &state, &composer))
        .expect("draw");
    let still = row(terminal.backend().buffer(), 0);
    assert!(still.starts_with("● working"), "{still:?}");
}

#[test]
fn the_busy_hint_names_preparation_controls_and_keeps_run_distinct() {
    for width in [44, 60, 80] {
        for ascii in [false, true] {
            let mut state = UiState::new(Presentation::Inline, false, (width, 8));
            state.ascii = ascii;
            state.busy = Some("preparing your workflow".into());
            let mut terminal = Terminal::new(TestBackend::new(width, 8)).expect("terminal");
            terminal
                .draw(|frame| draw_inline(frame, &state, &Composer::new()))
                .expect("draw");
            let shown = (0..8)
                .map(|y| row(terminal.backend().buffer(), y))
                .collect::<Vec<_>>()
                .join(" ");
            let shown = shown.split_whitespace().collect::<Vec<_>>().join(" ");
            for instruction in [
                "Preparing: Ctrl+C requests Stop",
                "correction + Enter",
                "Run: typing waits",
            ] {
                assert!(shown.contains(instruction), "{width}: {shown}");
            }
            assert!(!shown.contains("Ctrl+C twice leaves"));
            if ascii {
                assert!(shown.is_ascii());
            }
        }
    }
}

#[test]
fn workspace_navigation_fits_one_row_and_yields_to_the_current_action() {
    let mut state = UiState::new(Presentation::Workspace, false, (120, 40));
    let words = |line: Line<'_>| -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    };
    // The Session's words keep leading the status row: no prefix pushes them aside.
    state.status = "Last Run · Done · the run failed · `release.nika`".into();
    let composer = Composer::new();
    for (scroll, commands) in [
        // The composer says how `Enter` sends, as the approved design does.
        (0, ["Enter send", "Ctrl+O", "Enter send"]),
        (3, ["click chat", "End: latest", "/intelligence"]),
    ] {
        state.focus_scroll = scroll;
        for width in [39, 44, 49, 50, 60, 68, 92] {
            for ascii in [false, true] {
                state.ascii = ascii;
                let hint = hint_line(&state, &composer, width);
                let text = words(hint.clone());
                for command in commands {
                    assert!(text.contains(command), "{scroll} {width}: {text}");
                }
                if scroll == 0 && width >= 50 {
                    // The new line keeps its compatible key where every cue fits.
                    assert!(text.contains("Alt+Enter new line"), "{width}: {text}");
                }
                if text.contains("Ctrl+O") {
                    assert!(text.contains(PALETTE_HINT), "{scroll} {width}: {text}");
                }
                // `F6` and `Esc` keep their one home, the status row.
                assert!(!text.contains("F6"), "{scroll} {width}: {text}");
                assert!(words(status_line(&state, false, u16::MAX, None)).contains("F6 panel"));
                assert_eq!(wrapped_rows(&[hint], width), 1, "{scroll} {width}: {text}");
                if ascii {
                    assert!(text.is_ascii());
                }
                assert!(
                    words(status_line(&state, false, u16::MAX, None)).starts_with(&state.status)
                );
            }
        }
    }
    for (waiting, narrow) in [
        // The four choices keep their row; `cancel` takes the next.
        (
            Waiting::Choosing,
            "1 account - 2 API - 3 local - 4 no AI\ncancel",
        ),
        (Waiting::Proposal, "yes + Enter: Save - no: cancel"),
        (Waiting::Gate, "approve or refuse"),
        (
            Waiting::Question {
                key: "unknown_cost".into(),
            },
            "yes approves once - no or Ctrl+C cancels",
        ),
    ] {
        state.waiting = waiting;
        let whole = own(state.waiting.hint(), true);
        assert_eq!(words(hint_line(&state, &composer, 92)), whole);
        // Narrower, a whole cue goes or moves down: every key keeps what it does.
        assert_eq!(hint_text(&state, &composer, 44), narrow);
    }
    state.waiting = Waiting::Free;
    state.busy = Some("checking files locally".into());
    assert_eq!(words(hint_line(&state, &composer, 44)), WORKING_HINT);
    assert!(words(status_line(&state, false, u16::MAX, None)).contains("checking files locally"));
    state.completion = Some(ENTER.into());
    assert_eq!(words(hint_line(&state, &composer, 44)), ENTER);
    state.completion = None;
    state.busy = None;
    state.spinner = Some(3); // A stale animation frame is not active work.
    assert!(words(status_line(&state, false, u16::MAX, None)).starts_with(&state.status));
    assert!(!words(status_line(&state, false, u16::MAX, None)).contains("Idle"));
    state.focus_scroll = 0;
    state.presentation = Presentation::Inline;
    // The free prompt names the palette key once, where the row holds it.
    assert_eq!(
        words(hint_line(&state, &composer, 80)),
        "describe work - /help - Run: run <file>.nika - Ctrl+O: commands"
    );
    state.presentation = Presentation::Workspace;
    state.size = (40, 12); // Focus fallback has no workspace panel navigation.
    assert_eq!(
        words(hint_line(&state, &composer, 40)),
        own(state.waiting.hint(), true)
    );
}

/// The palette key keeps its action wherever the free hint names it: a
/// row too narrow for every cue drops one whole, never the words after
/// `Ctrl+O`, and each form is chosen by the cells it paints.
#[test]
fn the_palette_key_keeps_its_action_in_every_free_hint() {
    let composer = Composer::new();
    let words = |line: Line<'_>| -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    };
    for (scroll, width, expected) in [
        (0, 50, "Enter send · Alt+Enter new line · Ctrl+O: commands"),
        (0, 49, "Enter send · Ctrl+O: commands"),
        (0, 29, "Enter send · Ctrl+O: commands"),
        (0, 28, PALETTE_HINT),
        (
            3,
            58,
            "click chat; End: latest · /intelligence · Ctrl+O: commands",
        ),
        (3, 57, "click chat; End: latest · /intelligence"),
        (3, 38, "End: latest · /intelligence"),
    ] {
        for ascii in [false, true] {
            let mut state = UiState::new(Presentation::Workspace, false, (120, 40));
            state.focus_scroll = scroll;
            state.ascii = ascii;
            let shown = words(hint_line(&state, &composer, width));
            let at = format!("{scroll} {width} ascii={ascii}");
            assert_eq!(shown, own(expected, ascii), "{at}");
            let cells = unicode_width::UnicodeWidthStr::width(shown.as_str());
            assert!(cells <= usize::from(width), "{at}: {shown}");
        }
    }
}

/// Below the workspace's minimum the focus view stands in: the status row
/// names only what works there (Esc back inline, the page keys) and the
/// size it waits for, never the panel key it cannot honour; from the
/// minimum the workspace's own note returns.
#[test]
fn below_the_minimum_the_status_row_names_only_keys_that_work() {
    let words = |line: Line<'_>| -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    };
    let (width, height) = crate::workspace::geometry::MIN_SIZE;
    let minimum = format!("{width}x{height}");
    for (size, fits) in [
        ((59, 15), false),
        ((59, 40), false),
        ((200, 15), false),
        ((60, 16), true),
        ((120, 40), true),
    ] {
        for ascii in [false, true] {
            let mut state = UiState::new(Presentation::Workspace, false, size);
            state.ascii = ascii;
            let note = words(status_line(&state, false, u16::MAX, None));
            assert_eq!(note.contains("F6 panel"), fits, "{size:?}: {note}");
            assert_eq!(note.contains(&minimum), !fits, "{size:?}: {note}");
            let small = note.contains("Esc inline") && note.contains("PgUp/PgDn");
            assert_eq!(small, !fits, "{size:?}: {note}");
            assert_eq!(note.is_ascii(), ascii, "{size:?}: {note}");
        }
    }
}

/// The armed row says what a second press does and claims no
/// interruption, in both glyph columns.
#[test]
fn the_armed_row_says_what_a_second_press_does_and_claims_no_interruption() {
    let mut state = UiState::new(Presentation::Inline, false, (60, 5));
    state.interrupt_armed = true;
    let composer = Composer::new();
    let mut terminal = Terminal::new(TestBackend::new(60, 5)).expect("test terminal");
    for ascii in [false, true] {
        state.ascii = ascii;
        terminal
            .draw(|frame| draw_inline(frame, &state, &composer))
            .expect("draw");
        let armed = row(terminal.backend().buffer(), 0);
        let sep = if ascii { " - " } else { " · " };
        assert_eq!(armed, format!("Ctrl+C again leaves{sep}any key stays"));
    }
}

#[test]
fn the_inline_frame_shows_the_prompt_that_names_what_waits() {
    let state = state_after_demo(Presentation::Inline);
    let composer = Composer::new();
    let backend = TestBackend::new(60, 5);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| draw_inline(frame, &state, &composer))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    // The line stands right above its hint; a frame taller than the live
    // area leaves its spare rows above the line, never under it.
    assert!(
        row(buffer, 3).starts_with("reply ›"),
        "{:?}",
        row(buffer, 3)
    );
    assert!(
        row(buffer, 4).contains("answer the question"),
        "{:?}",
        row(buffer, 4)
    );
}

/// The lifecycle rail sits on a row of its own above the status, the
/// prompt keeps its place below, and the live area grows by that row.
#[test]
fn the_rail_sits_above_the_status_row() {
    let composer = Composer::new();
    let mut fresh = UiState::new(Presentation::Inline, false, (80, 40));
    let before = live_rows(&fresh, &composer, 80, 40);
    fresh.apply(Beat::Rail("Draft ○ · Saved ○".to_owned()));
    assert_eq!(live_rows(&fresh, &composer, 80, 40), before + 1);
    let mut state = state_after_demo(Presentation::Inline);
    state.apply(Beat::Rail(
        "Draft ✓ · Saved ○ · Checked ○ · Active ○ · Run ○".to_owned(),
    ));
    let backend = TestBackend::new(60, 6);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| draw_inline(frame, &state, &composer))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    assert!(
        row(buffer, 0).starts_with("Draft ✓ · Saved ○ · Checked ○"),
        "{:?}",
        row(buffer, 0)
    );
    assert!(
        row(buffer, 4).starts_with("reply ›"),
        "{:?}",
        row(buffer, 4)
    );
}

#[test]
fn the_focus_frame_lists_the_transcript_above_the_same_live_area() {
    let state = state_after_demo(Presentation::Focus);
    let composer = Composer::new();
    let backend = TestBackend::new(70, 14);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| draw_focus(frame, &state, &composer))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..14).map(|y| row(buffer, y)).collect();
    assert!(
        rows.iter()
            .any(|r| r.contains("Which file holds the notes")),
        "{rows:#?}"
    );
    assert!(rows.iter().any(|r| r.starts_with("reply ›")), "{rows:#?}");
    assert!(rows.iter().any(|r| r.starts_with("───")), "{rows:#?}");
    assert!(
        rows.iter()
            .any(|r| r.contains("focus · Esc returns inline")),
        "{rows:#?}"
    );
}

#[test]
fn save_and_its_answer_stay_visible_in_the_narrow_conversation() {
    for color in [false, true] {
        let mut state = UiState::new(Presentation::Workspace, color, (35, 8));
        state.waiting = Waiting::Proposal;
        let composer = Composer::new();
        let height = live_rows(&state, &composer, 35, 20);
        let mut terminal = Terminal::new(TestBackend::new(35, height)).expect("test terminal");
        terminal
            .draw(|frame| render_live(frame, &state, &composer, frame.area()))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let text = (0..height)
            .map(|y| row(buffer, y))
            .collect::<Vec<_>>()
            .join(" ");
        for words in [
            "Save?",
            "yes + Enter: Save",
            "no: cancel",
            "yes means Save only",
        ] {
            assert!(text.contains(words), "{words}: {text}");
        }
        // What a plain yes does, never a claim that another request is unavailable.
        assert!(!text.contains("Run separately"), "{text}");
        // A key too wide for the row leaves whole, never parted from its effect.
        assert!(
            !text.contains("/show") || text.contains("/show: inspect"),
            "{text}"
        );
        assert_eq!(
            buffer[(0, 0)].fg,
            if color {
                ratatui::style::Color::Rgb(233, 191, 126)
            } else {
                ratatui::style::Color::Reset
            }
        );
    }
}

/// A short live area (the Workbench's compact conversation) keeps the
/// first lines of a multi-line draft readable: the rail yields its row to
/// them, and two rows stay for the transcript and its rule.
#[test]
fn a_short_live_area_keeps_a_multi_line_draft_readable() {
    let mut state = UiState::new(Presentation::Workspace, false, (80, 24));
    state.rail = "Draft ○ · Saved ○ · Checked ○ · Active ○ · Run ○".to_owned();
    let mut composer = Composer::new();
    composer.paste("keep this\nexact draft");
    // Six rows given: half is three, the draft and its rows need four.
    let live = live_rows(&state, &composer, 60, 6);
    assert_eq!(live, 4);
    let mut terminal = Terminal::new(TestBackend::new(60, live)).expect("terminal");
    terminal
        .draw(|frame| render_live(frame, &state, &composer, frame.area()))
        .expect("draw");
    let rows: Vec<String> = (0..live)
        .map(|y| row(terminal.backend().buffer(), y))
        .collect();
    assert_eq!(rows[1], "nika › keep this");
    assert_eq!(rows[2], "       exact draft");
    assert!(!rows.iter().any(|r| r.contains("Draft ○")), "{rows:#?}");
    // One line keeps the rail's half: three rows of six.
    let mut one = Composer::new();
    one.paste("one line");
    assert_eq!(live_rows(&state, &one, 60, 6), 3);
}

#[test]
fn the_live_area_never_takes_more_than_half_the_screen() {
    let mut state = UiState::new(Presentation::Inline, false, (80, 24));
    state.apply(Beat::Wait(Waiting::Free));
    let mut composer = Composer::new();
    composer.paste(&"x".repeat(2000));
    assert_eq!(live_rows(&state, &composer, 80, 24), 12);
    assert_eq!(live_rows(&state, &composer, 80, 4), 3);
}

#[test]
fn the_ascii_column_gives_the_renderer_glyphs_their_twins_and_keeps_the_session_words() {
    let mut state = state_after_demo(Presentation::Focus);
    state.ascii = true;
    let composer = Composer::new();
    let mut terminal = Terminal::new(TestBackend::new(80, 30)).expect("test terminal");
    terminal
        .draw(|frame| draw_focus(frame, &state, &composer))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..30).map(|y| row(buffer, y)).collect();
    let find = |needle: &str| rows.iter().position(|r| r.contains(needle));
    let prompt = find("reply >").expect("the prompt marker in ASCII");
    let rule = rows
        .iter()
        .position(|r| r.starts_with("---"))
        .expect("rule");
    for y in [rule, prompt, prompt + 1] {
        assert!(rows[y].is_ascii(), "row {y}: {:?}", rows[y]);
    }
    assert!(find("focus - Esc returns inline").is_some(), "{rows:#?}");
    assert!(
        find("answer the question above - cancel to stop").is_some(),
        "{rows:#?}"
    );
    // The Session's own words are never rewritten by the glyph column.
    assert!(rows.iter().any(|r| r.contains(" · ")), "{rows:#?}");
    let human = Committed::new(Kind::Human, "digest my notes");
    let refusal = Committed::new(Kind::Refusal, "not allowed");
    let gate = Committed::new(Kind::Gate, "approve the write?");
    let text = |block: &Committed| -> String {
        block_lines(block, false, true)[0]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect()
    };
    assert_eq!(text(&human), "> digest my notes");
    assert_eq!(text(&refusal), "x not allowed");
    assert_eq!(text(&gate), "|| approve the write?");
}

#[test]
fn the_ascii_loader_turns_through_ascii_frames() {
    let mut state = UiState::new(Presentation::Inline, false, (40, 4));
    state.ascii = true;
    state.apply(Beat::Busy("thinking".to_owned()));
    for frame in 0..8u8 {
        state.spinner = Some(frame);
        let line = status_line(&state, false, u16::MAX, None);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.is_ascii() && text.ends_with("thinking"), "{text:?}");
    }
    state.spinner = None;
    let still: String = status_line(&state, false, u16::MAX, None)
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert_eq!(still, "* thinking");
}

/// While a turn works the marker turns through the theme's own frames in
/// one steady style, the accent of active work set in weight: a frame
/// moves the glyph, never the hue. Reduced motion is the still dot in that
/// style, no colour leaves the weight alone, and an idle view (even with a
/// stale frame) has no marker.
#[test]
fn activity_marker_turns_native_frames_in_one_steady_hue_and_never_animates_idle() {
    let mut state = UiState::new(Presentation::Workspace, true, (120, 40));
    state.spinner = Some(3);
    assert!(activity_marker(&state).is_none());
    state.busy = Some("authoring".to_owned());
    let steady = role::style(Role::Accent, true).add_modifier(Modifier::BOLD);
    // Two whole turns of the orbit: each frame its own glyph, one style.
    for frame in 0..20_u8 {
        state.spinner = Some(frame);
        let mark = activity_marker(&state).expect("an observed busy turn");
        let glyph = SPINNER[usize::from(frame) % SPINNER.len()];
        assert_eq!(mark.content, format!("{glyph} "), "frame {frame}");
        assert_eq!(mark.style, steady, "frame {frame}");
    }
    state.spinner = None;
    let still = activity_marker(&state).expect("reduced motion");
    assert_eq!(still.content, "● ");
    assert_eq!(still.style, steady);
    assert_eq!(activity_marker(&state), Some(still));
    state.ascii = true;
    state.color = false;
    let weight = Style::default().add_modifier(Modifier::BOLD);
    for frame in 0..10_u8 {
        state.spinner = Some(frame);
        let mark = activity_marker(&state).expect("ASCII busy marker");
        let glyph = ASCII_SPINNER[usize::from(frame) % ASCII_SPINNER.len()];
        assert_eq!(mark.content, format!("{glyph} "), "frame {frame}");
        assert_eq!(mark.style, weight, "frame {frame}");
    }
    state.spinner = None;
    let still = activity_marker(&state).expect("reduced motion in ASCII");
    assert_eq!(still.content, "* ");
    assert_eq!(still.style, weight);
    state.busy = None;
    state.waiting = Waiting::Gate;
    assert!(activity_marker(&state).is_none());
}

#[test]
fn busy_phases_wrap_and_keep_the_model_visible() {
    let composer = Composer::new();
    for width in [37, 44, 50] {
        let mut state = UiState::new(Presentation::Workspace, true, (width, 30));
        state.busy = Some(
            "✓ recorded 5 requirements · ● authoring · deepseek/deepseek-v4-pro · 3s".to_owned(),
        );
        state.spinner = Some(3);
        let rows = live_rows(&state, &composer, width, 30);
        assert!(status_rows(&state, false, width, false) > 1);
        assert!(status_rows(&state, false, width, false) <= 3);
        let mut terminal = Terminal::new(TestBackend::new(width, rows)).expect("test terminal");
        terminal
            .draw(|frame| render_live(frame, &state, &composer, frame.area()))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let words = (0..rows)
            .map(|y| row(buffer, y))
            .collect::<Vec<_>>()
            .join(" ");
        for fact in ["authoring", "deepseek/deepseek-v4-pro", "nika ›"] {
            assert!(words.contains(fact), "width {width}: {words}");
        }
    }
}

/// An activity card's rows wear the tone of the Session's glyph, as the busy row
/// does; a run's step and the words themselves are untouched, and no hue without colour.
#[test]
fn activity_rows_wear_their_glyph_tone_and_keep_their_words() {
    let block = Committed::new(
        Kind::Activity,
        "Repairing\n✓ recorded 6 requirements\n● authoring · m\n↻ a stronger model reads it\n✔ a · 3 ms · 1/2",
    );
    let lines = block_lines(&block, true, false);
    let tone = |i: usize| lines[i].spans[1].style;
    assert_eq!(tone(0), role::style(Role::Strong, true));
    assert_eq!(tone(1), role::style(Role::Good, true));
    assert_eq!(tone(2), role::style(Role::Accent, true));
    assert_eq!(tone(3), role::style(Role::Warn, true));
    assert_eq!(
        tone(4),
        Style::default(),
        "a run's step keeps its block style"
    );
    let words: Vec<String> = (lines.iter())
        .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();
    assert_eq!(words.join("\n"), block.text);
    for line in block_lines(&block, false, false) {
        assert!(line.spans.iter().all(|s| s.style.fg.is_none()), "{line:?}");
    }
    // Another kind is never toned by its words.
    let report = Committed::new(Kind::Report, "✓ looks like a phase");
    assert_eq!(
        block_lines(&report, true, false)[0].spans[1].style,
        Style::default()
    );
    let mut state = UiState::new(Presentation::Workspace, true, (80, 4));
    state.apply(Beat::Busy("↻ a stronger model reads it".to_owned()));
    let busy = status_line(&state, false, u16::MAX, None);
    assert_eq!(busy.spans[1].style, role::style(Role::Warn, true));
}

/// While a turn works, the hint says what typing and Ctrl+C do then, in words; the
/// hint of what waits returns with the turn's end. A completion still wins.
#[test]
fn the_hint_says_what_keys_do_while_a_turn_works() {
    let mut state = UiState::new(Presentation::Inline, true, (80, 5));
    state.waiting = Waiting::Proposal;
    let composer = Composer::new();
    let text = |state: &UiState| -> String {
        hint_line(state, &composer, 80)
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect()
    };
    assert!(text(&state).starts_with("yes + Enter: Save"));
    state.apply(Beat::Busy("reviewing your reply".to_owned()));
    let working = text(&state);
    for words in [
        "Preparing: Ctrl+C requests Stop",
        "correction + Enter",
        "Run: typing waits",
    ] {
        assert!(working.contains(words), "{working}");
    }
    assert!(!working.contains("Save"), "{working}");
    assert_eq!(
        hint_line(&state, &composer, 80).spans[0].style,
        role::style(Role::Accent, true),
        "no consent tone while nothing can be consented to"
    );
    assert!(working.chars().count() <= 80, "one row at 80 columns");
    state.ascii = true;
    assert!(text(&state).is_ascii());
    state.completion = Some(ENTER.to_owned());
    assert_eq!(text(&state), ENTER);
    state.completion = None;
    state.apply(Beat::Wait(Waiting::Proposal));
    assert!(text(&state).starts_with("yes + Enter: Save"));
}

const ENTER: &str = "Nika is working - Enter sends when it is your turn";

/// Focus and the small workspace fallback name End on the status row. A
/// fitting workspace owns that cue beside its clickable transcript marker.
#[test]
fn a_scrolled_back_transcript_names_the_way_back_to_the_latest() {
    let words = |state: &UiState| -> String {
        status_line(state, false, u16::MAX, None)
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect()
    };
    for (presentation, size, status_cue) in [
        (Presentation::Focus, (80, 24), true),
        (Presentation::Workspace, (50, 14), true),
        (Presentation::Workspace, (80, 24), false),
    ] {
        let mut state = UiState::new(presentation, false, size);
        state.status = "Ready for review".to_owned();
        assert!(!words(&state).contains("End: latest"));
        state.focus_scroll = 3;
        assert!(
            words(&state).ends_with("reading earlier messages · End: latest") == status_cue,
            "{}",
            words(&state)
        );
        state.apply(Beat::Busy("● authoring · m".to_owned()));
        assert_eq!(
            words(&state).ends_with("End: latest"),
            status_cue,
            "{}",
            words(&state)
        );
        state.ascii = true;
        assert_eq!(
            words(&state).ends_with("reading earlier messages - End: latest"),
            status_cue
        );
    }
    let mut inline = UiState::new(Presentation::Inline, false, (80, 24));
    inline.focus_scroll = 3;
    assert!(!words(&inline).contains("End: latest"));
}

/// The workspace says the lifecycle once it says something: a fresh
/// session's five pending fields take no row there, a field reached (or
/// an earlier result) does, and the inline view keeps its rail as before.
#[test]
fn an_untouched_lifecycle_takes_no_workspace_row() {
    let composer = Composer::new();
    let fresh = "Draft ○ · Saved ○ · Checked ○ · Active ○ · Run ○";
    let begun = "Draft ● · Saved ○ · Checked ○ · Active ○ · Run ○";
    let earlier = format!("{fresh} (earlier ✓)");
    for presentation in [Presentation::Workspace, Presentation::Inline] {
        let mut state = UiState::new(presentation, false, (120, 40));
        let bare = live_rows(&state, &composer, 80, 40);
        let quiet = presentation == Presentation::Workspace;
        for (rail, shown) in [(fresh, !quiet), (begun, true), (earlier.as_str(), true)] {
            state.apply(Beat::Rail(rail.to_owned()));
            let rows = live_rows(&state, &composer, 80, 40);
            assert_eq!(rows, bare + u16::from(shown), "{presentation:?} {rail}");
        }
    }
}

/// The boxed composer asks for its caption and its two edges, and wraps
/// its line at the box's inner width: a line the plain row holds takes
/// two rows inside the box.
#[test]
fn a_boxed_composer_asks_its_caption_edges_and_inner_rows() {
    let state = UiState::new(Presentation::Workspace, false, (120, 40));
    let mut short = Composer::new();
    short.paste("short");
    let plain = live_rows(&state, &short, 40, 40);
    assert_eq!(panel_rows(&state, &short, 40, 40, BOXED), plain + 4);
    let mut long = Composer::new();
    long.paste(&"x".repeat(31));
    assert_eq!(live_rows(&state, &long, 40, 40), plain);
    assert_eq!(panel_rows(&state, &long, 40, 40, BOXED), plain + 4);
}

/// The words of `line`, its spans joined.
fn spans(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

/// The hint's one row, where it takes one.
fn hint_line(state: &UiState, composer: &Composer, width: u16) -> Line<'static> {
    let mut lines = hint_lines(state, composer, width, (None, true));
    assert_eq!(lines.len(), 1, "one hint row at {width}");
    lines.remove(0)
}

/// The hint's rows, each row's words, a `\n` between rows.
fn hint_text(state: &UiState, composer: &Composer, width: u16) -> String {
    let lines = hint_lines(state, composer, width, (None, true));
    lines.iter().map(spans).collect::<Vec<_>>().join("\n")
}

/// At 44 cells the rail and the status row cut visibly: each holds in its
/// cells and ends on the cut mark, never on a dangling separator, and the
/// shell's own note comes only when it fits whole beside the Session's words.
#[test]
fn the_rail_and_the_status_cut_visibly_on_a_narrow_row() {
    let composer = Composer::new();
    for ascii in [false, true] {
        let mut state = UiState::new(Presentation::Workspace, false, (120, 40));
        state.ascii = ascii;
        state.rail = "Draft ● · Saved ○ · Checked ○ · Active ○ · Run ○".into();
        state.status =
            "Needs one answer · the currency code · the compiler cannot invent it".into();
        let live = Rect::new(0, 0, 44, 8);
        let mut terminal = Terminal::new(TestBackend::new(44, 8)).expect("test terminal");
        terminal
            .draw(|frame| render_live(frame, &state, &composer, live))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let areas = live_areas(&state, &composer, live, false);
        let cut = if ascii { "..." } else { "…" };
        for (what, area) in [("rail", areas.rail), ("status", areas.status)] {
            let text = row(buffer, area.y);
            assert!(text.ends_with(cut), "{what}: {text}");
            let kept = text.strip_suffix(cut).expect("the cut mark");
            assert!(!kept.trim_end().ends_with('·'), "{what}: {text}");
            let cells = unicode_width::UnicodeWidthStr::width(text.as_str());
            assert!(cells <= 44, "{what}: {text}");
        }
        let status = row(buffer, areas.status.y);
        assert!(
            !status.contains("F6"),
            "the note only when it fits: {status}"
        );
        state.status = "Needs one answer".into();
        let note = spans(&status_line(&state, false, 60, None));
        let whole = own("Needs one answer · workspace · F6 panel · Esc back", ascii);
        assert_eq!(note, whole);
    }
}

/// A decision's hint too wide for its row drops a whole cue, or the choices
/// set `cancel` on a second row: what stays is whole rows whose every key
/// keeps what it does, never a key at a row's end.
#[test]
fn a_decision_keeps_each_key_with_its_effect_on_narrow_rows() {
    let composer = Composer::new();
    let cost = Waiting::Question {
        key: "unknown_cost".into(),
    };
    let prose = Waiting::Question {
        key: "const.notes".into(),
    };
    for waiting in [
        Waiting::Proposal,
        cost,
        Waiting::Gate,
        prose,
        Waiting::Choosing,
    ] {
        for width in [35_u16, 44, 92] {
            let mut state = UiState::new(Presentation::Workspace, false, (120, 40));
            state.waiting = waiting.clone();
            let hint = hint_lines(&state, &composer, width, (None, true));
            let text = hint_text(&state, &composer, width);
            let at = format!("{waiting:?} {width}: {text}");
            // Every row holds whole: none wraps.
            assert_eq!(usize::from(wrapped_rows(&hint, width)), hint.len(), "{at}");
            let second = waiting == Waiting::Choosing && width == 44;
            assert_eq!(hint.len(), 1 + usize::from(second), "{at}");
            let full = state.waiting.hint();
            let mut forms = std::iter::once(full).chain(narrower(&waiting).to_vec());
            assert!(forms.any(|form| form == text), "{at}");
            for row in text.split('\n') {
                assert!(!row.ends_with(':') && !row.ends_with('·'), "{at}");
            }
        }
    }
}

/// A typed question's hint stays one row: how Enter answers, and that
/// `cancel` drops it wherever that fits; never « stop », never « above ».
#[test]
fn a_typed_question_hint_names_enter_and_cancel_on_one_row() {
    let composer = Composer::new();
    let choice = Shape::Choice(vec![Offer::new("eur", "Euro")]);
    for shape in [choice, Shape::Text, Shape::Literal] {
        let asked = Asked::new("Which?", "", true, shape, "3:q", 3);
        let mut state = UiState::new(Presentation::Workspace, false, (120, 40));
        state.waiting = Waiting::asked("const.currency", asked);
        for width in [13_u16, 31, 44, 60, 76, 100] {
            for ascii in [false, true] {
                state.ascii = ascii;
                let hint = hint_line(&state, &composer, width);
                let text = spans(&hint);
                let at = format!("{width} ascii={ascii}: {text}");
                assert_eq!(wrapped_rows(&[hint], width), 1, "{at}");
                assert!(text.contains("Enter answers"), "{at}");
                assert!(width < 31 || text.contains("cancel drops it"), "{at}");
                assert!(!text.contains("stop") && !text.contains("above"), "{at}");
            }
        }
    }
}

/// Under the question's live home the boxed composer needs no caption, the
/// card naming the question right above its line; a question in prose keeps
/// its caption.
#[test]
fn the_live_home_needs_no_caption_and_prose_keeps_it() {
    let composer = Composer::new();
    let live = Rect::new(0, 0, 44, 20);
    let asked = Asked::new("the currency code", "", true, Shape::Text, "3:q", 3);
    let mut state = UiState::new(Presentation::Workspace, false, (120, 40));
    state
        .transcript
        .push(Committed::question("3:q", "the currency code"));
    state.waiting = Waiting::asked("const.currency", asked);
    let caption = |state: &UiState| {
        live_areas(state, &composer, live, true)
            .boxed
            .map(|(caption, _)| caption.height)
    };
    assert_eq!(caption(&state), Some(0), "the live home");
    state.waiting = Waiting::Question {
        key: "const.currency".to_owned(),
    };
    assert_eq!(caption(&state), Some(1), "a question in prose");
}

/// A proposal's and a gate's own prompt names the line (`Save? ›`,
/// `answer ›`): the boxed composer takes no caption there and its demand
/// counts none, the rows going to the conversation; the free prompt keeps
/// « Your message ».
#[test]
fn a_decision_prompt_names_its_line_without_a_caption() {
    let composer = Composer::new();
    let live = Rect::new(0, 0, 44, 20);
    let caption = |state: &UiState| {
        live_areas(state, &composer, live, true)
            .boxed
            .map(|(caption, _)| caption.height)
    };
    let mut state = UiState::new(Presentation::Workspace, false, (120, 40));
    assert_eq!(caption(&state), Some(1), "the free prompt");
    for waiting in [Waiting::Proposal, Waiting::Gate] {
        state.waiting = waiting;
        let at = format!("{:?}", state.waiting);
        assert_eq!(caption(&state), Some(0), "{at}");
        let plain = live_rows(&state, &composer, 44, 40);
        let boxed = panel_rows(&state, &composer, 44, 40, BOXED);
        // The box's two edges and its second writing row, no caption.
        assert_eq!(boxed, plain + 3, "{at}");
    }
}

/// While a decision waits in the fitting workspace the status row carries
/// the Session's words alone: the panel's note waits for the free prompt,
/// and below the minimum the recovery note stays.
#[test]
fn a_decision_waiting_leaves_the_panel_note_to_the_free_prompt() {
    let prose = Waiting::Question {
        key: "const.notes".to_owned(),
    };
    for waiting in [Waiting::Proposal, Waiting::Gate, prose] {
        let mut state = UiState::new(Presentation::Workspace, false, (120, 40));
        state.status = "Ready for review".to_owned();
        state.waiting = waiting;
        let at = format!("{:?}", state.waiting);
        let note = spans(&status_line(&state, false, u16::MAX, None));
        assert!(!note.contains("F6 panel"), "{at}: {note}");
        if state.waiting != Waiting::Proposal {
            assert_eq!(note, "Ready for review", "{at}");
            state.size = (59, 15);
            let small = spans(&status_line(&state, false, u16::MAX, None));
            let recovery = small.contains("60x16") && small.contains("Esc inline");
            assert!(recovery, "{at}: {small}");
        }
    }
    let free = UiState::new(Presentation::Workspace, false, (120, 40));
    let note = spans(&status_line(&free, false, u16::MAX, None));
    assert!(note.contains("F6 panel"), "the free prompt: {note}");
}

/// The question's live home takes over the rail and the status only while
/// they repeat it: the rail of a first question (nothing saved, checked,
/// active or run) and the Session's own sentence for this question. A rail
/// with any other fact or legacy words, another status, an armed exit, a
/// question in prose and a proposal keep their rows; demand, painting and
/// the rest rows read the one rule.
#[test]
fn a_homed_question_takes_over_only_the_rows_that_repeat_it() {
    let composer = Composer::new();
    let live = Rect::new(0, 0, 44, 20);
    let asked = Asked::new("the currency code", "", true, Shape::Literal, "3:q", 3);
    let homed = Waiting::asked("const.currency", asked);
    let sentence = format!("{QUESTION_STATUS}the currency code");
    let earlier = format!("{FIRST_QUESTION_RAIL} (earlier ✓)");
    let saved = "Draft ● · Saved ✓ · Checked ✓ · Active ○ · Run ○";
    let declared = "Draft ● · Saved ✓ · Checked ○ · Active ◐ · Run ✓";
    let other = "Needs one answer · another value";
    let cost = "Waiting for a one-time unknown-cost decision";
    let rows = |state: &UiState| {
        let areas = live_areas(state, &composer, live, false);
        (areas.rail.height, areas.status.height)
    };
    for (rail, status, shown) in [
        (FIRST_QUESTION_RAIL, sentence.as_str(), (0, 0)),
        (saved, sentence.as_str(), (1, 0)),
        (declared, sentence.as_str(), (1, 0)),
        (earlier.as_str(), sentence.as_str(), (1, 0)),
        (FIRST_QUESTION_RAIL, other, (0, 1)),
        (FIRST_QUESTION_RAIL, cost, (0, 1)),
    ] {
        let at = format!("{rail} | {status}");
        let mut state = UiState::new(Presentation::Workspace, false, (120, 40));
        state
            .transcript
            .push(Committed::question("3:q", "the currency code"));
        state.waiting = homed.clone();
        state.rail = rail.to_owned();
        state.status = status.to_owned();
        assert_eq!(rows(&state), shown, "{at}");
        // Demand asks exactly the rows painting gives: the card and the line keep theirs.
        let asked = live_rows(&state, &composer, 44, 40);
        let fit = live_areas(&state, &composer, Rect::new(0, 0, 44, asked), false);
        assert_eq!((fit.rail.height, fit.status.height), shown, "{at}");
        assert_eq!((fit.card.height, fit.input.height), (2, 1), "{at}");
        // At rest the same rows: the rail and status shown, the card, the line, the hint.
        let rest = shown.0 + shown.1 + 4;
        assert_eq!(rest_rows(&state, 44, None), rest, "{at}");
        state.interrupt_armed = true;
        assert_eq!(rows(&state).1, 1, "{at}: an armed exit keeps its row");
        assert_eq!(rest_rows(&state, 44, None), rest, "{at}: armed");
    }
    let mut prose = UiState::new(Presentation::Workspace, false, (120, 40));
    prose.waiting = Waiting::Question {
        key: "const.currency".to_owned(),
    };
    prose.rail = FIRST_QUESTION_RAIL.to_owned();
    prose.status = sentence.clone();
    assert_eq!(rows(&prose), (1, 1), "a question in prose");
    let mut proposal = UiState::new(Presentation::Workspace, false, (120, 40));
    proposal.waiting = Waiting::Proposal;
    proposal.rail = "Draft ✓ · Saved ○ · Checked ○ · Active ○ · Run ○".to_owned();
    assert_eq!(rows(&proposal), (1, 1), "the proposal keeps its rail");
}

/// The workspace panel's boxed composer, reviewing nothing.
const BOXED: Panel<'static> = Panel {
    boxed: true,
    consent: None,
};

/// The fixture's candidate as the conversation reviews it, a `save & run` of
/// it admitted by the Session's typed method or not (`runs`).
fn reviewed_candidate(runs: bool, ascii: bool) -> Review {
    use crate::workspace::candidate::RunAfter;
    use crate::workspace::cards::review::fixture;
    let after = runs.then_some(RunAfter::Saved);
    let candidate = fixture::candidate(fixture::id(), false).running(after);
    candidate.review(ascii).expect("a consent can name it")
}

/// A proposal waiting in the workspace under the Session's rail `rail`.
fn proposing(rail: &str, ascii: bool) -> UiState {
    let mut state = UiState::new(Presentation::Workspace, false, (120, 40));
    state.waiting = Waiting::Proposal;
    state.rail = rail.to_owned();
    state.ascii = ascii;
    state
}

/// While the reviewed proposal's `save & run` is admitted, its decision row
/// names every consent word with what it does, `save & run` first, in the
/// longest whole form the row holds: a narrower row drops a whole cue, never
/// an action, down to the narrowest panel, one row in both glyph columns,
/// the row the rest rows measure. Refused, or with no review, the waiting
/// state's own hint stays.
#[test]
fn a_reviewed_proposal_names_every_consent_word_on_its_row() {
    let composer = Composer::new();
    for ascii in [false, true] {
        let review = reviewed_candidate(true, ascii);
        let state = proposing("", ascii);
        for (width, form) in [
            (120, 0),
            (85, 0),
            (84, 1),
            (77, 1),
            (76, 2),
            (56, 2),
            (55, 3),
            (41, 3),
            (40, 4),
            (37, 4),
            (36, 4),
        ] {
            let at = format!("{width} ascii={ascii}");
            let hint = hint_lines(&state, &composer, width, (Some(&review), true));
            assert_eq!(hint.len(), 1, "{at}");
            let text = spans(&hint[0]);
            assert_eq!(text, own(SAVE_RUN_HINTS[form], ascii), "{at}");
            assert_eq!(wrapped_rows(&hint, width), 1, "{at}");
            for word in ["save & run", "yes: ", "no: discard"] {
                assert!(text.contains(word), "{at}: {text}");
            }
            assert!(!ascii || text.is_ascii(), "{at}: {text}");
            assert_eq!(rest_rows(&state, width, Some(&review)), 3, "{at}");
        }
        let refused = reviewed_candidate(false, ascii);
        for width in [44_u16, 92] {
            let hint = hint_lines(&state, &composer, width, (Some(&refused), true));
            let text = spans(&hint[0]);
            assert!(text.starts_with("yes + Enter: Save"), "{width}: {text}");
            assert!(!text.contains("save & run"), "{width}: {text}");
            assert_eq!(
                hint_lines(&state, &composer, width, (None, true)),
                hint,
                "{width}"
            );
        }
    }
}

/// The reviewed proposal lends the rail only while it is exactly the waiting
/// proposal's own: a saved, checked, run or earlier rail keeps its row, and
/// so does any rail with no review. The status keeps the standing, scoped to
/// this proposal (never a session-wide « nothing has run »), in its longest
/// whole form; demand, painting and the rest rows read the one rule.
#[test]
fn a_reviewed_proposal_lends_only_its_exact_empty_rail() {
    let composer = Composer::new();
    let live = Rect::new(0, 0, 44, 20);
    let empty = "Draft ✓ · Saved ○ · Checked ○ · Active ○ · Run ○";
    let earlier = format!("{empty} (earlier ✓)");
    let ran = "Draft ✓ · Saved ✓ · Checked ✓ · Active ○ · Run ✓";
    let saved = "Draft ✓ · Saved ✓ · Checked ○ · Active ○ · Run ○";
    for ascii in [false, true] {
        let review = reviewed_candidate(true, ascii);
        for (rail, reviewed, lent) in [
            (empty, true, true),
            (saved, true, false),
            (ran, true, false),
            (earlier.as_str(), true, false),
            (empty, false, false),
        ] {
            let at = format!("{rail} reviewed={reviewed} ascii={ascii}");
            let state = proposing(rail, ascii);
            let consent = reviewed.then_some(&review);
            let panel = Panel {
                boxed: false,
                consent,
            };
            let areas = areas_of(&state, &composer, live, panel);
            assert_eq!(areas.rail.height, u16::from(!lent), "{at}");
            assert_eq!(areas.status.height, 1, "{at}: the standing keeps its row");
            let asked = panel_rows(&state, &composer, 44, 40, panel);
            assert_eq!(asked, rest_rows(&state, 44, consent), "{at}: at rest");
            let fit = areas_of(&state, &composer, Rect::new(0, 0, 44, asked), panel);
            let shown = (fit.rail.height, fit.status.height, fit.input.height);
            assert_eq!(shown, (u16::from(!lent), 1, 1), "{at}");
            let status = spans(&status_line(&state, false, 44, consent));
            let standing = if reviewed {
                STANDING[0]
            } else {
                PROPOSAL_STATUS
            };
            assert_eq!(status, own(standing, ascii), "{at}");
            assert!(!status.contains("nothing has run"), "{at}");
        }
        let state = proposing(empty, ascii);
        let narrow = spans(&status_line(&state, false, 40, Some(&review)));
        assert_eq!(narrow, "Not saved yet", "a whole shorter form");
    }
}

/// A rail too wide for its row keeps its newest stages: a leading done stage
/// the next done stage implies gives way under one mark at its head, only as
/// far as the row needs, then the end is cut as before; a rail that fits, or
/// whose head no later stage implies, reads as it did. Painting reads it.
#[test]
fn a_cut_rail_keeps_its_newest_stages() {
    let ran = "Draft ✓ · Saved ✓ · Checked ✓ · Active ○ · Run ✓";
    let kept = "Saved ✓ · Checked ✓ · Active ○ · Run ✓";
    assert_eq!(newest(ran, 60, false), ran);
    assert_eq!(newest(ran, 44, false), format!("… {kept}"));
    assert_eq!(newest(ran, 36, false), "… Checked ✓ · Active ○ · Run ✓");
    assert_eq!(newest(ran, 44, true), format!("... {kept}"));
    let asked = "Draft ● · Saved ○ · Checked ○ · Active ○ · Run ○";
    let cut = "Draft ● · Saved ○ · Checked ○ · Active ○…";
    assert_eq!(newest(asked, 44, false), cut, "no stage implies its head");
    for width in [20_u16, 30, 36, 40, 44] {
        for ascii in [false, true] {
            let shown = newest(ran, width, ascii);
            let cells = unicode_width::UnicodeWidthStr::width(shown.as_str());
            assert!(cells <= usize::from(width), "{width}: {shown}");
        }
    }
    let mut state = UiState::new(Presentation::Workspace, false, (120, 40));
    state.rail = ran.to_owned();
    state.status = "Last Run · Done · the run succeeded".to_owned();
    let mut terminal = Terminal::new(TestBackend::new(44, 6)).expect("terminal");
    terminal
        .draw(|frame| render_live(frame, &state, &Composer::new(), frame.area()))
        .expect("draw");
    let first = row(terminal.backend().buffer(), 0);
    assert_eq!(first, format!("… {kept}"));
}
