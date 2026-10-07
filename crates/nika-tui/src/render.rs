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
//!
//! The live area holds the command chooser under the line it fills
//! ([`chooser`]): while it shows, the area may take all but two rows of what
//! it is given, so the transcript and its rule always keep a row.

mod chooser;
#[cfg(test)]
mod chooser_tests;
mod wrapped;

pub(crate) use wrapped::{pages, paint_page, window};

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
        Kind::Reply | Kind::Proposal | Kind::Report | Kind::Activity => ("", Style::default()),
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

/// An activity mark only while the Session reports work. The existing
/// frame drives both the orbit and its accent; no frame means reduced
/// motion, and a stale frame without `busy` never animates an idle view.
pub(crate) fn activity_marker(state: &UiState) -> Option<Span<'static>> {
    state.busy.as_ref()?;
    let (glyph, tone) = match state.spinner {
        Some(frame) => {
            let frame = usize::from(frame);
            let glyph = if state.ascii {
                ASCII_SPINNER[frame % ASCII_SPINNER.len()]
            } else {
                SPINNER[frame % SPINNER.len()]
            };
            let tones = [Role::Accent, Role::VerbInvoke, Role::VerbAgent];
            (glyph, tones[(frame / 3) % tones.len()])
        }
        None => (if state.ascii { '*' } else { '●' }, Role::Accent),
    };
    let style = if state.color {
        role::style(tone, true).add_modifier(Modifier::BOLD)
    } else {
        accent(false)
    };
    Some(Span::styled(format!("{glyph} "), style))
}

/// A working phase can name a model and the last completed phase. Reserve
/// enough rows to read those words rather than clipping them to one line.
fn status_rows(state: &UiState, width: u16) -> u16 {
    if state.busy.is_some() {
        wrapped_rows(&[status_line(state)], width).min(3)
    } else {
        1
    }
}

/// The role of one line of an activity card: its heading, then each row by
/// the Session's glyph (the busy row's reading); a run's step keeps `None`.
fn activity_role(index: usize, text: &str) -> Option<Role> {
    if index == 0 {
        Some(Role::Strong)
    } else if text.starts_with("✓ ") {
        Some(Role::Good)
    } else if text.starts_with("↻ ") {
        Some(Role::Warn)
    } else if text.starts_with("● ") {
        Some(Role::Accent)
    } else {
        None
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
            let tone = (block.kind == Kind::Activity)
                .then(|| activity_role(i, text))
                .flatten()
                .map_or(style, |tone| role::style(tone, color));
            Line::from(vec![
                Span::styled(head, style),
                Span::styled(text.to_owned(), tone),
            ])
        })
        .collect()
}

/// The rows `lines` take at `width` once wrapped, at least one.
#[must_use]
pub fn wrapped_rows(lines: &[Line<'_>], width: u16) -> u16 {
    u16::try_from(content_rows(lines, width)).unwrap_or(u16::MAX)
}

/// The complete content height, before any terminal-coordinate conversion.
pub(crate) fn content_rows(lines: &[Line<'_>], width: u16) -> usize {
    wrapped::height(lines, width)
}

/// Draw a block into a buffer (the `insert_before` callback).
pub fn render_block(block: &Committed, color: bool, ascii: bool, buf: &mut Buffer) {
    Paragraph::new(block_lines(block, color, ascii))
        .wrap(Wrap { trim: false })
        .render(buf.area, buf);
}

/// The rows of the live area at `width`: the lifecycle rail when the
/// session reports one, the status, a draft set aside, the prompt and
/// composer (the palette's search instead while it is open), the chooser,
/// the hint. Clamped so the live area never eats the whole terminal: half of
/// `height` (or, in a short panel such as the Workbench's conversation, what
/// three lines of a multi-line draft need beside the status and the hint),
/// or all but two rows while the chooser shows; never more than all but two.
#[must_use]
pub fn live_rows(state: &UiState, composer: &Composer, width: u16, height: u16) -> u16 {
    let chooser = chooser::rows(state, composer, width);
    let input = if composer.palette_open() {
        1
    } else {
        composer.content_rows(editor_width(state, composer, width))
    };
    // The chooser borrows the rail's row while it shows.
    let rail = usize::from(!state.rail.is_empty() && chooser == 0);
    let aside = usize::from(composer.aside().is_some());
    let hint = usize::from(wrapped_rows(&[hint_line(state, composer, width)], width).min(3));
    let status = usize::from(status_rows(state, width));
    let rows = rail + status + aside + input + chooser + hint;
    // Only a multi-line draft asks past the half; a one-line draft never does.
    let readable = if input > 1 {
        u16::try_from(status + input.min(READABLE_LINES) + hint).unwrap_or(u16::MAX)
    } else {
        0
    };
    let maximum = if chooser > 0 {
        height.saturating_sub(2)
    } else {
        height
            .saturating_div(2)
            .max(readable.min(height.saturating_sub(2)))
    }
    .max(3);
    u16::try_from(rows.clamp(3, usize::from(maximum))).unwrap_or(maximum)
}

/// The lines of a multi-line draft a short live area still shows: the rail
/// yields its row to them.
const READABLE_LINES: usize = 3;

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
        // What the next press does, never « interrupted »: the shell arms
        // only when nothing was interrupted (an interrupted turn and a
        // cancelled decision say so in their own words), so this row may sit
        // under a turn that just succeeded.
        return Line::from(Span::styled(
            own("Ctrl+C again leaves · any key stays", state.ascii),
            accent,
        ));
    }
    if let Some(marker) = activity_marker(state) {
        let label = state.busy.as_deref().unwrap_or_default();
        let mut spans = vec![marker];
        for (index, phase) in label.split(" · ").enumerate() {
            if index > 0 {
                spans.push(Span::styled(own(" · ", state.ascii), dim));
            }
            let tone = if phase.starts_with("✓ ") {
                Role::Good
            } else if phase.starts_with("● ") {
                Role::Accent
            } else if phase.starts_with("↻ ") {
                Role::Warn
            } else {
                Role::Strong
            };
            // These are the Session's exact phase words, merely styled.
            spans.push(Span::styled(
                phase.to_owned(),
                role::style(tone, state.color),
            ));
        }
        if let Some(cue) = earlier_cue(state) {
            spans.push(Span::styled(cue, dim));
        }
        Line::from(spans)
    } else if state.waiting == Waiting::Proposal {
        Line::styled(
            own("Save these changes · Run separately", state.ascii),
            role::style(Role::Warn, state.color).add_modifier(Modifier::BOLD),
        )
    } else {
        // Where the automation stands, then the presentation's own note.
        let mode = match state.presentation {
            Presentation::Inline => "",
            Presentation::Focus => "focus · Esc returns inline · PgUp/PgDn scroll",
            Presentation::Workspace => "workspace · F6 panel · Esc back",
        };
        let mode = own(mode, state.ascii);
        let sep = own(" · ", state.ascii);
        let text = match (state.status.is_empty(), mode.is_empty()) {
            (true, _) => mode,
            (false, true) => state.status.clone(),
            (false, false) => format!("{}{sep}{mode}", state.status),
        };
        let cue = earlier_cue(state).unwrap_or_default();
        Line::from(Span::styled(format!("{text}{cue}"), dim))
    }
}

/// While the full-screen transcript is scrolled back, new activity keeps the
/// reading place; the row says so and names the key back to the latest.
fn earlier_cue(state: &UiState) -> Option<String> {
    (state.focus_scroll > 0 && state.presentation != Presentation::Inline)
        .then(|| own(" · reading earlier messages · End: latest", state.ascii))
}

/// Preparation has Stop and queued corrections; a Run keeps its separate controls.
/// The action's resulting notice still replaces this hint when the human acts.
const WORKING_HINT: &str =
    "Preparing: Ctrl+C requests Stop; correction + Enter. Run: typing waits.";

/// The key that opens the palette of every command and view key: named once,
/// on the idle hint row.
const PALETTE_HINT: &str = "Ctrl+O: commands";

/// The hint row: a notice the last key left, else how to choose while the
/// chooser shows, else what keys do while a turn works, else what the
/// waiting state takes. At a free prompt the row is the one place that names
/// the palette key; the workspace keeps `F6` and `Esc` on its status row and
/// the empty composer invites `/`, so neither repeats here.
fn hint_line(state: &UiState, composer: &Composer, width: u16) -> Line<'static> {
    let choosing = composer
        .listing()
        .map(|listing| chooser::hint(&listing, state.busy.is_some(), state.ascii));
    let idle = if state.busy.is_some() {
        WORKING_HINT.to_owned()
    } else if state.waiting == Waiting::Free
        && state.presentation == Presentation::Workspace
        && crate::workspace::geometry::fits(state.size)
    {
        // One row, even beside a narrow preview; Stop, consent and completion keep priority.
        // Scrolled back, the way to the latest comes first: the status row may clip its cue.
        let hints = if state.focus_scroll > 0 {
            [
                "click chat; End: latest · /intelligence · Ctrl+O: commands",
                "click chat; End: latest · /intelligence · Ctrl+O",
                "click chat; End: latest · /intelligence",
                "End: latest · /intelligence",
            ]
        } else {
            [
                "/intelligence · Ctrl+O: commands · wheel: scroll",
                "/intelligence · Ctrl+O · wheel: scroll",
                "/intelligence · Ctrl+O · wheel:scroll",
                "/intelligence · Ctrl+O",
            ]
        };
        let last = hints[hints.len() - 1];
        hints
            .into_iter()
            .find(|hint| hint.chars().count() <= usize::from(width))
            .unwrap_or(last)
            .to_owned()
    } else if state.waiting == Waiting::Free {
        let hint = format!("{} · {PALETTE_HINT}", state.waiting.hint());
        if hint.chars().count() <= usize::from(width) {
            hint
        } else {
            state.waiting.hint().to_owned()
        }
    } else {
        state.waiting.hint().to_owned()
    };
    let text = match (&state.completion, choosing) {
        (Some(notice), _) => own(notice, state.ascii),
        (None, Some(choosing)) => choosing,
        (None, None) => own(&idle, state.ascii),
    };
    let tone = match state.waiting {
        Waiting::Proposal | Waiting::Gate if state.busy.is_none() => Role::Warn,
        _ => Role::Accent,
    };
    Line::from(Span::styled(text, role::style(tone, state.color)))
}

/// The rows of the live area, top to bottom: the rail, the status, a draft
/// set aside, the line being written (or the palette's search), the chooser
/// under it, a filler, the hint.
struct LiveAreas {
    rail: Rect,
    status: Rect,
    aside: Rect,
    input: Rect,
    chooser: Rect,
    hint: Rect,
}

/// Cut `area` into the live rows. Closed, the line being written takes every
/// spare row, as it always did; while the chooser shows, the line takes its
/// own rows, the chooser what it asks for, and the spare rows go below it.
fn live_areas(state: &UiState, composer: &Composer, area: Rect) -> LiveAreas {
    let chooser_wanted =
        u16::try_from(chooser::rows(state, composer, area.width)).unwrap_or(u16::MAX);
    let choosing = chooser_wanted > 0;
    let hint_wanted = wrapped_rows(&[hint_line(state, composer, area.width)], area.width).min(3);
    let status_wanted = status_rows(state, area.width);
    let lines = if composer.palette_open() {
        1
    } else {
        composer.rows(editor_width(state, composer, area.width))
    };
    let readable = lines.min(u16::try_from(READABLE_LINES).unwrap_or(u16::MAX));
    // The rail takes a row of its own above the status (both are full
    // sentences; one 80-column row cannot hold them side by side), yields it
    // on a terminal too short for four rows or to the first lines of a
    // multi-line draft, and lends it to the chooser.
    let rail_rows = u16::from(
        !state.rail.is_empty()
            && !choosing
            && area.height >= 4
            && (lines < 2 || area.height > hint_wanted + status_wanted + readable),
    );
    let hint_rows = hint_wanted.min(area.height.saturating_sub(rail_rows + 2).max(1));
    let status_rows =
        status_wanted.min(area.height.saturating_sub(rail_rows + hint_rows + 1).max(1));
    let fixed = rail_rows + hint_rows + status_rows;
    let aside_rows = u16::from(composer.aside().is_some() && area.height >= fixed + 2);
    let spare = area.height.saturating_sub(fixed + aside_rows);
    let (input, chooser, filler) = if choosing {
        let input = lines.clamp(1, spare.max(1));
        let chooser = chooser_wanted.min(spare.saturating_sub(input));
        (
            Constraint::Length(input),
            Constraint::Length(chooser),
            Constraint::Min(0),
        )
    } else {
        (
            Constraint::Min(1),
            Constraint::Length(0),
            Constraint::Length(0),
        )
    };
    let [rail, status, aside, input, chooser, _, hint] = Layout::vertical([
        Constraint::Length(rail_rows),
        Constraint::Length(status_rows),
        Constraint::Length(aside_rows),
        input,
        chooser,
        filler,
        Constraint::Length(hint_rows),
    ])
    .areas(area);
    LiveAreas {
        rail,
        status,
        aside,
        input,
        chooser,
        hint,
    }
}

/// The prompt beside the line being written: what the Session waits for, or
/// the palette's search while it is open.
fn prompt_of(state: &UiState, composer: &Composer) -> &'static str {
    if composer.palette_open() {
        chooser::SEARCH_PROMPT
    } else {
        state.waiting.prompt()
    }
}

/// The cells left to the line being written beside its prompt.
fn editor_width(state: &UiState, composer: &Composer, width: u16) -> u16 {
    let prompt = u16::try_from(prompt_of(state, composer).chars().count()).unwrap_or(8);
    width.saturating_sub(prompt).max(8)
}

/// Draw the live area (status · prompt + composer · chooser · hint) into
/// `area`: the same live area inline, under the focus transcript and in the
/// workspace panel.
pub(crate) fn render_live(frame: &mut Frame<'_>, state: &UiState, composer: &Composer, area: Rect) {
    let areas = live_areas(state, composer, area);
    if areas.rail.height > 0 {
        frame.render_widget(Paragraph::new(rail_line(state)), areas.rail);
    }
    frame.render_widget(
        Paragraph::new(status_line(state)).wrap(Wrap { trim: false }),
        areas.status,
    );
    if let Some(aside) = composer.aside().filter(|_| areas.aside.height > 0) {
        let line = chooser::aside_line(aside, areas.aside.width, state.ascii, state.color);
        frame.render_widget(Paragraph::new(line), areas.aside);
    }
    let prompt = prompt_of(state, composer);
    let prompt_width = u16::try_from(prompt.chars().count()).unwrap_or(8);
    let [marker, editor] =
        Layout::horizontal([Constraint::Length(prompt_width), Constraint::Min(8)])
            .areas(areas.input);
    let marker_style = match state.waiting {
        Waiting::Gate | Waiting::Proposal if state.color => role::style(Role::Warn, true),
        _ if state.busy.is_some() => accent(state.color),
        _ => accent(state.color).add_modifier(Modifier::BOLD),
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            own(prompt, state.ascii),
            marker_style,
        ))),
        marker,
    );
    if composer.palette_open() {
        frame.render_widget(
            Paragraph::new(chooser::search_line(composer, state.color)),
            editor,
        );
    } else {
        composer.render(editor, frame.buffer_mut());
    }
    chooser::render(state, composer, areas.chooser, frame.buffer_mut());
    frame.render_widget(
        Paragraph::new(hint_line(state, composer, area.width)).wrap(Wrap { trim: false }),
        areas.hint,
    );
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
    if state.presentation == Presentation::Workspace {
        crate::workspace::cards::render(frame, state, area);
        return;
    }
    let mut lines: Vec<Line<'static>> = Vec::new();
    for block in &state.transcript {
        lines.extend(block_lines(block, state.color, state.ascii));
        lines.push(Line::default());
    }
    let skip = content_rows(&lines, area.width)
        .saturating_sub(usize::from(area.height))
        .saturating_sub(state.focus_scroll);
    window(&lines, area, skip, frame.buffer_mut());
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
            (0, ["/intelligence", "Ctrl+O", "wheel"]),
            (3, ["click chat", "End: latest", "/intelligence"]),
        ] {
            state.focus_scroll = scroll;
            for width in [39, 44, 60, 68, 92] {
                for ascii in [false, true] {
                    state.ascii = ascii;
                    let hint = hint_line(&state, &composer, width);
                    let text = words(hint.clone());
                    for command in commands {
                        assert!(text.contains(command), "{scroll} {width}: {text}");
                    }
                    // `F6` and `Esc` keep their one home, the status row.
                    assert!(!text.contains("F6"), "{scroll} {width}: {text}");
                    assert!(words(status_line(&state)).contains("F6 panel"));
                    assert_eq!(wrapped_rows(&[hint], width), 1, "{scroll} {width}: {text}");
                    if ascii {
                        assert!(text.is_ascii());
                    }
                    assert!(words(status_line(&state)).starts_with(&state.status));
                }
            }
        }
        for waiting in [
            Waiting::Choosing,
            Waiting::Proposal,
            Waiting::Gate,
            Waiting::Question {
                key: "unknown_cost".into(),
            },
        ] {
            state.waiting = waiting;
            assert_eq!(
                words(hint_line(&state, &composer, 44)),
                own(state.waiting.hint(), true)
            );
        }
        state.waiting = Waiting::Free;
        state.busy = Some("checking files locally".into());
        assert_eq!(words(hint_line(&state, &composer, 44)), WORKING_HINT);
        assert!(words(status_line(&state)).contains("checking files locally"));
        state.completion = Some(ENTER.into());
        assert_eq!(words(hint_line(&state, &composer, 44)), ENTER);
        state.completion = None;
        state.busy = None;
        state.spinner = Some(3); // A stale animation frame is not active work.
        assert!(words(status_line(&state)).starts_with(&state.status));
        assert!(!words(status_line(&state)).contains("Idle"));
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
                "/show: inspect",
                "Run separately",
            ] {
                assert!(text.contains(words), "{words}: {text}");
            }
            assert_eq!(
                buffer[(0, 0)].fg,
                if color {
                    ratatui::style::Color::Rgb(242, 193, 125)
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
    #[test]
    fn activity_marker_uses_native_frames_and_never_animates_idle() {
        let mut state = UiState::new(Presentation::Workspace, true, (120, 40));
        state.spinner = Some(3);
        assert!(activity_marker(&state).is_none());
        state.busy = Some("authoring".to_owned());
        let cyan = activity_marker(&state).expect("an observed busy turn");
        assert_eq!(cyan.content, "⠸ ");
        assert_eq!(
            cyan.style.fg,
            Some(ratatui::style::Color::Rgb(106, 216, 226))
        );
        state.spinner = Some(6);
        let purple = activity_marker(&state).expect("busy frame");
        assert_ne!(cyan.content, purple.content);
        assert_ne!(cyan.style.fg, purple.style.fg);
        state.spinner = None;
        let still = activity_marker(&state).expect("reduced motion");
        assert_eq!(still.content, "● ");
        assert_eq!(activity_marker(&state), Some(still));
        state.ascii = true;
        state.color = false;
        for frame in 0..10 {
            state.spinner = Some(frame);
            let mark = activity_marker(&state).expect("ASCII busy marker");
            assert!(mark.content.is_ascii());
            assert_eq!(mark.style.fg, None);
            assert_eq!(mark.style.bg, None);
        }
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
                "✓ recorded 5 requirements · ● authoring · deepseek/deepseek-v4-pro · 3s"
                    .to_owned(),
            );
            state.spinner = Some(3);
            let rows = live_rows(&state, &composer, width, 30);
            assert!(status_rows(&state, width) > 1);
            assert!(status_rows(&state, width) <= 3);
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
        let busy = status_line(&state);
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

    /// A scrolled-back transcript is said on the status row (busy or idle) with the key
    /// back to the latest; at the latest row, and inline (the terminal scrolls), nothing.
    #[test]
    fn a_scrolled_back_transcript_names_the_way_back_to_the_latest() {
        let words = |state: &UiState| -> String {
            status_line(state)
                .spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect()
        };
        for presentation in [Presentation::Focus, Presentation::Workspace] {
            let mut state = UiState::new(presentation, false, (80, 24));
            state.status = "Ready for review".to_owned();
            assert!(!words(&state).contains("End: latest"));
            state.focus_scroll = 3;
            assert!(
                words(&state).ends_with("reading earlier messages · End: latest"),
                "{}",
                words(&state)
            );
            state.apply(Beat::Busy("● authoring · m".to_owned()));
            assert!(words(&state).ends_with("End: latest"), "{}", words(&state));
            state.ascii = true;
            assert!(words(&state).ends_with("reading earlier messages - End: latest"));
        }
        let mut inline = UiState::new(Presentation::Inline, false, (80, 24));
        inline.focus_scroll = 3;
        assert!(!words(&inline).contains("End: latest"));
    }
}
