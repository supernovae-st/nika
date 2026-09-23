// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Painting. Pure functions from the state to a frame: the same block draws
//! the same way whether it is committed to the scrollback (inline) or listed
//! on the alternate screen (focus). Chrome is dimmer than the workflow text;
//! colour carries a meaning or is absent (the theme decides, never here):
//! blue for the prompt marker when computation is active, yellow for a gate,
//! a permission, a cost or a boundary; the default foreground for everything
//! the human reads.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

use crate::composer::Composer;
use crate::model::{Committed, Kind, Presentation, UiState, Waiting};

/// The glyph and the style of one kind of block.
fn face(kind: Kind, color: bool) -> (&'static str, Style) {
    let dim = Style::default().add_modifier(Modifier::DIM);
    let strong = Style::default().add_modifier(Modifier::BOLD);
    let paint = |c: Color| {
        if color {
            Style::default().fg(c)
        } else {
            Style::default()
        }
    };
    match kind {
        Kind::Banner | Kind::Notice => ("", dim),
        Kind::Human => ("› ", strong),
        Kind::Reply | Kind::Proposal | Kind::Report => ("", Style::default()),
        Kind::Question => ("? ", Style::default()),
        Kind::Run => ("  ", Style::default()),
        Kind::Gate => ("⏸ ", paint(Color::Yellow)),
        Kind::Result => ("", strong),
        Kind::Refusal => ("✖ ", paint(Color::Red)),
    }
}

/// The lines of one block, the glyph on its first line only.
#[must_use]
pub fn block_lines(block: &Committed, color: bool) -> Vec<Line<'static>> {
    let (glyph, style) = face(block.kind, color);
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
    let width = usize::from(width.max(1));
    let rows: usize = lines
        .iter()
        .map(|line| {
            let cells: usize = line
                .spans
                .iter()
                .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()))
                .sum();
            cells.div_ceil(width).max(1)
        })
        .sum();
    u16::try_from(rows.max(1)).unwrap_or(u16::MAX)
}

/// Draw a block into a buffer (the `insert_before` callback).
pub fn render_block(block: &Committed, color: bool, buf: &mut Buffer) {
    Paragraph::new(block_lines(block, color))
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

/// The loader's frames (braille dots, the usual terminal spinner).
pub const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// The lifecycle rail, dim like the chrome: five facts, never one badge.
fn rail_line(state: &UiState) -> Line<'static> {
    Line::from(Span::styled(
        state.rail.clone(),
        Style::default().add_modifier(Modifier::DIM),
    ))
}

fn status_line(state: &UiState) -> Line<'static> {
    let dim = Style::default().add_modifier(Modifier::DIM);
    let accent = if state.color {
        Style::default().fg(Color::Blue)
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    };
    if state.interrupt_armed {
        return Line::from(Span::styled(
            "interrupted · Ctrl+C again leaves · any key stays",
            accent,
        ));
    }
    if let Some(label) = &state.busy {
        // The marker turns while a turn runs; still (●) under reduced motion.
        let marker = state.spinner.map_or_else(
            || "● ".to_owned(),
            |f| format!("{} ", SPINNER[usize::from(f) % SPINNER.len()]),
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
        };
        let text = match (state.status.is_empty(), mode.is_empty()) {
            (true, _) => mode.to_owned(),
            (false, true) => state.status.clone(),
            (false, false) => format!("{} · {mode}", state.status),
        };
        Line::from(Span::styled(text, dim))
    }
}

fn hint_line(state: &UiState) -> Line<'static> {
    let text = state
        .completion
        .clone()
        .unwrap_or_else(|| state.waiting.hint().to_owned());
    Line::from(Span::styled(
        text,
        Style::default().add_modifier(Modifier::DIM),
    ))
}

/// Draw the live area (status · prompt + composer · hint) into `area`.
fn render_live(frame: &mut Frame<'_>, state: &UiState, composer: &Composer, area: Rect) {
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
        Waiting::Gate | Waiting::Proposal if state.color => Style::default().fg(Color::Yellow),
        _ if state.busy.is_some() && state.color => Style::default().fg(Color::Blue),
        _ => Style::default().add_modifier(Modifier::BOLD),
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(prompt.to_owned(), marker_style))),
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
    let mut lines: Vec<Line<'static>> = Vec::new();
    let shown = state.transcript.len().saturating_sub(state.focus_scroll);
    for block in state.transcript.iter().take(shown) {
        lines.extend(block_lines(block, state.color));
        lines.push(Line::default());
    }
    let total = wrapped_rows(&lines, transcript.width);
    let skip = total.saturating_sub(transcript.height);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((skip, 0)),
        transcript,
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(usize::from(rule.width)),
            Style::default().add_modifier(Modifier::DIM),
        ))),
        rule,
    );
    render_live(frame, state, composer, bottom);
}

#[cfg(test)]
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
        let lines = block_lines(&block, false);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].spans[0].content.as_ref(), "⏸ ");
        assert_eq!(lines[1].spans[0].content.as_ref(), "  ");
        assert_eq!(wrapped_rows(&lines, 80), 2);
        assert_eq!(wrapped_rows(&lines, 10), 4);
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
}
