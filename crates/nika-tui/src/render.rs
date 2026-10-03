// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Painting. Pure functions from the state to a frame: the same block draws
//! the same way whether it is committed to the scrollback (inline) or listed
//! on the alternate screen (focus). Chrome is dimmer than the workflow text;
//! colour carries a meaning or is absent, always through a theme role resolved
//! here at paint time ([`crate::visual::role`]), never a hue named in a widget:
//! the accent for the prompt marker while computation is active, the warning
//! slot for a gate, a permission, a cost or a boundary, the failure slot for a
//! refusal; the default foreground for everything the human reads.

use nika_display::theme::Role;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

use crate::composer::Composer;
use crate::model::{Committed, Kind, Presentation, UiState, Waiting};
use crate::visual::role;

/// The glyph and the style of one kind of block; `ascii` draws the ASCII twin
/// of each glyph (the theme's glyph column, never the renderer's choice).
fn face(kind: Kind, color: bool, ascii: bool) -> (&'static str, Style) {
    let dim = role::style(Role::Dim, color);
    let strong = role::style(Role::Strong, color);
    let pick = |unicode, twin| if ascii { twin } else { unicode };
    match kind {
        Kind::Banner | Kind::Notice => ("", dim),
        Kind::Human => (pick("› ", "> "), strong),
        Kind::Reply | Kind::Proposal | Kind::Report => ("", Style::default()),
        Kind::Question => ("? ", Style::default()),
        Kind::Run => ("  ", Style::default()),
        Kind::Gate => (pick("⏸ ", "|| "), role::style(Role::Warn, color)),
        Kind::Result => ("", strong),
        Kind::Refusal => (pick("✖ ", "x "), role::style(Role::Bad, color)),
    }
}

/// The renderer's own words in the glyph column in use: under `ascii` its
/// markers and separators (`›`, `·`, `…`) take their ASCII twins. Only text the
/// renderer writes goes through here; the Session's words are never rewritten.
fn own(text: &str, ascii: bool) -> String {
    if ascii {
        text.replace('›', ">").replace('·', "-").replace('…', "...")
    } else {
        text.to_owned()
    }
}

/// The ASCII twin of the loader's orbit, one motion in four frames.
const ASCII_SPINNER: [char; 4] = ['|', '/', '-', '\\'];

/// The accent a live marker wears: the theme's accent slot, or bold when
/// colour is off (a weight, never a hue, marks it then).
fn accent(color: bool) -> Style {
    if color {
        role::style(Role::Accent, color)
    } else {
        role::style(Role::Strong, color)
    }
}

/// The lines of one block, the glyph on its first line only.
#[must_use]
pub fn block_lines(block: &Committed, color: bool, ascii: bool) -> Vec<Line<'static>> {
    let (glyph, style) = face(block.kind, color, ascii);
    let indent = " ".repeat(glyph.chars().count());
    block
        .text
        .lines()
        .enumerate()
        .map(|(i, text)| {
            let head = if i == 0 {
                glyph.to_owned()
            } else {
                indent.clone()
            };
            Line::from(vec![
                Span::styled(head, style),
                Span::styled(text.to_owned(), style),
            ])
        })
        .collect()
}

/// The rows `lines` take at `width` once wrapped, at least one.
#[must_use]
pub fn wrapped_rows(lines: &[Line<'_>], width: u16) -> u16 {
    // Use the same word wrapper as rendering. Cell-count division can
    // underestimate rows and hide the last line of a consent question.
    let rows = Paragraph::new(lines.to_vec())
        .wrap(Wrap { trim: false })
        .line_count(width.max(1));
    u16::try_from(rows.max(1)).unwrap_or(u16::MAX)
}

/// Draw a block into a buffer (the `insert_before` callback).
pub fn render_block(block: &Committed, color: bool, ascii: bool, buf: &mut Buffer) {
    Paragraph::new(block_lines(block, color, ascii))
        .wrap(Wrap { trim: false })
        .render(buf.area, buf);
}

/// The rows of the live area at `width`: the lifecycle rail when the
/// session reports one, the status, the prompt and composer, the hint.
/// Clamped so the live area never eats the whole terminal.
#[must_use]
pub fn live_rows(state: &UiState, composer: &Composer, width: u16, height: u16) -> u16 {
    let prompt = u16::try_from(state.waiting.prompt().chars().count()).unwrap_or(8);
    let composer_rows = composer.rows(width.saturating_sub(prompt).max(8));
    let rail = u16::from(!state.rail.is_empty());
    let rows = rail + 1 + composer_rows + 1;
    rows.clamp(3, height.saturating_div(2).max(3))
}

/// The loader's frames: the theme seam's own braille orbit, one motion for the
/// renderer and the run frames.
pub use nika_display::theme::SPINNER;

/// The lifecycle rail, dim like the chrome: five facts, never one badge.
fn rail_line(state: &UiState) -> Line<'static> {
    Line::from(Span::styled(
        state.rail.clone(),
        Style::default().add_modifier(Modifier::DIM),
    ))
}

fn status_line(state: &UiState) -> Line<'static> {
    let dim = role::style(Role::Dim, state.color);
    let accent = accent(state.color);
    if state.interrupt_armed {
        return Line::from(Span::styled(
            own(
                "interrupted · Ctrl+C again leaves · any key stays",
                state.ascii,
            ),
            accent,
        ));
    }
    if let Some(label) = &state.busy {
        // The marker turns while a turn runs; still (●) under reduced motion.
        let still = if state.ascii { "* " } else { "● " };
        let marker = state.spinner.map_or_else(
            || still.to_owned(),
            |f| {
                let f = usize::from(f);
                let frame = if state.ascii {
                    ASCII_SPINNER[f % ASCII_SPINNER.len()]
                } else {
                    SPINNER[f % SPINNER.len()]
                };
                format!("{frame} ")
            },
        );
        Line::from(vec![
            Span::styled(marker, accent),
            Span::styled(label.clone(), dim),
        ])
    } else {
        // Where the automation stands, then the presentation's own note.
        let mode = match state.presentation {
            Presentation::Inline => "",
            Presentation::Focus => "focus · Esc returns inline · PgUp/PgDn scroll",
            Presentation::Workspace => "workspace · F6 moves the keys · Esc returns inline",
        };
        let mode = own(mode, state.ascii);
        let sep = own(" · ", state.ascii);
        let text = match (state.status.is_empty(), mode.is_empty()) {
            (true, _) => mode,
            (false, true) => state.status.clone(),
            (false, false) => format!("{}{sep}{mode}", state.status),
        };
        Line::from(Span::styled(text, dim))
    }
}

fn hint_line(state: &UiState) -> Line<'static> {
    let text = own(
        state
            .completion
            .as_deref()
            .unwrap_or_else(|| state.waiting.hint()),
        state.ascii,
    );
    Line::from(Span::styled(
        text,
        Style::default().add_modifier(Modifier::DIM),
    ))
}

/// Draw the live area (status · prompt + composer · hint) into `area`: the
/// same live area under the focus transcript and in the workspace panel.
pub(crate) fn render_live(frame: &mut Frame<'_>, state: &UiState, composer: &Composer, area: Rect) {
    // The rail takes a row of its own above the status (both are full
    // sentences; one 80-column row cannot hold them side by side) and
    // yields it on a terminal too short for four rows.
    let rail_rows = u16::from(!state.rail.is_empty() && area.height >= 4);
    let [rail, status, input, hint] = Layout::vertical([
        Constraint::Length(rail_rows),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);
    if rail_rows > 0 {
        frame.render_widget(Paragraph::new(rail_line(state)), rail);
    }
    frame.render_widget(Paragraph::new(status_line(state)), status);
    let prompt = state.waiting.prompt();
    let prompt_width = u16::try_from(prompt.chars().count()).unwrap_or(8);
    let [marker, editor] =
        Layout::horizontal([Constraint::Length(prompt_width), Constraint::Min(8)]).areas(input);
    let marker_style = match state.waiting {
        Waiting::Gate | Waiting::Proposal if state.color => role::style(Role::Warn, true),
        _ if state.busy.is_some() => accent(state.color),
        _ => role::style(Role::Strong, state.color),
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            own(prompt, state.ascii),
            marker_style,
        ))),
        marker,
    );
    composer.render(editor, frame.buffer_mut());
    frame.render_widget(Paragraph::new(hint_line(state)), hint);
}

/// The inline presentation: the frame IS the live area (the transcript is
/// the terminal's own scrollback).
pub fn draw_inline(frame: &mut Frame<'_>, state: &UiState, composer: &Composer) {
    let area = frame.area();
    render_live(frame, state, composer, area);
}

/// The transcript in `area`, scrolled so its end (less the focus scroll) is
/// the last row: the focus presentation and the workspace panel share it.
pub(crate) fn render_transcript(frame: &mut Frame<'_>, state: &UiState, area: Rect) {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let shown = state.transcript.len().saturating_sub(state.focus_scroll);
    for block in state.transcript.iter().take(shown) {
        lines.extend(block_lines(block, state.color, state.ascii));
        lines.push(Line::default());
    }
    let total = wrapped_rows(&lines, area.width);
    let skip = total.saturating_sub(area.height);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((skip, 0)),
        area,
    );
}

/// The focus presentation: the transcript above (scrolled from the end), a
/// rule, the same live area below.
pub fn draw_focus(frame: &mut Frame<'_>, state: &UiState, composer: &Composer) {
    let area = frame.area();
    let live = live_rows(state, composer, area.width, area.height);
    let [transcript, rule, bottom] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(live),
    ])
    .areas(area);
    render_transcript(frame, state, transcript);
    let glyph = if state.ascii { "-" } else { "─" };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            glyph.repeat(usize::from(rule.width)),
            Style::default().add_modifier(Modifier::DIM),
        ))),
        rule,
    );
    render_live(frame, state, composer, bottom);
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::model::{Beat, Script};

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
    fn the_inline_frame_shows_the_prompt_that_names_what_waits() {
        let state = state_after_demo(Presentation::Inline);
        let composer = Composer::new();
        let backend = TestBackend::new(60, 5);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| draw_inline(frame, &state, &composer))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        assert!(
            row(buffer, 1).starts_with("reply ›"),
            "{:?}",
            row(buffer, 1)
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
            row(buffer, 2).starts_with("reply ›"),
            "{:?}",
            row(buffer, 2)
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
            find("answer the question - an empty line").is_some(),
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
            let line = status_line(&state);
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            assert!(text.is_ascii() && text.ends_with("thinking"), "{text:?}");
        }
        state.spinner = None;
        let still: String = status_line(&state)
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(still, "* thinking");
    }
}
