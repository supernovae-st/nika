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
//! (`chooser`): while it shows, the area may take all but two rows of what
//! it is given, so the transcript and its rule always keep a row. A typed
//! question's card stands above that line (`question`): it may take as much,
//! and it gives way first, so the line that answers keeps its row.

mod chooser;
#[cfg(test)]
mod chooser_tests;
pub(crate) mod question;
#[cfg(test)]
mod question_tests;

pub(crate) use nika_tui_view::workspace::wrapped::{height, pages, paint_page, window};

use nika_display::theme::Role;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget, Wrap};

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
/// markers, separators and arrows (`›`, `·`, `…`, `↑↓`) take their ASCII
/// twins. Only text the renderer writes goes through here; the Session's words
/// are never rewritten.
pub(crate) fn own(text: &str, ascii: bool) -> String {
    if ascii {
        text.replace('›', ">")
            .replace('·', "-")
            .replace('…', "...")
            .replace("↑↓", "Up/Down")
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
/// frame turns the orbit, never its hue: every frame wears the accent, the
/// one hue of active work (the Session's `●` phase), in weight. No frame
/// means reduced motion, and a stale frame without `busy` never animates an
/// idle view.
pub(crate) fn activity_marker(state: &UiState) -> Option<Span<'static>> {
    state.busy.as_ref()?;
    let glyph = match state.spinner {
        Some(frame) if state.ascii => ASCII_SPINNER[usize::from(frame) % ASCII_SPINNER.len()],
        Some(frame) => SPINNER[usize::from(frame) % SPINNER.len()],
        None if state.ascii => '*',
        None => '●',
    };
    // Without colour the weight alone marks it.
    let style = accent(state.color).add_modifier(Modifier::BOLD);
    Some(Span::styled(format!("{glyph} "), style))
}

/// A working phase can name a model and the last completed phase. Reserve
/// enough rows to read those words rather than clipping them to one line;
/// none while the question's live home takes the row over (`lent`, [`Lent`]).
fn status_rows(state: &UiState, choosing: bool, width: u16, lent: bool) -> u16 {
    if lent {
        0
    } else if state.busy.is_some() {
        wrapped_rows(&[status_line(state, choosing, width)], width).min(3)
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
    height(lines, width)
}

/// Draw a block into a buffer (the `insert_before` callback).
pub fn render_block(block: &Committed, color: bool, ascii: bool, buf: &mut Buffer) {
    Paragraph::new(block_lines(block, color, ascii))
        .wrap(Wrap { trim: false })
        .render(buf.area, buf);
}

/// The rows of the live area at `width`: the lifecycle rail when the
/// session reports one, the status, a draft set aside, a typed question's
/// card, the prompt and composer (the palette's search instead while it is
/// open), the chooser, the hint. Clamped so the live area never eats the
/// whole terminal: half of `height` (or, in a short panel such as the
/// Workbench's conversation, what three lines of a multi-line draft need
/// beside the status and the hint), or all but two rows while the chooser or
/// a typed question's card shows; never more than all but two.
#[must_use]
pub fn live_rows(state: &UiState, composer: &Composer, width: u16, height: u16) -> u16 {
    rows_of_live(state, composer, width, height, false)
}

/// [`live_rows`] of the workspace's composer boxed under its caption
/// ([`render_boxed_live`]): the caption and the box's two edges as well, the
/// line wrapped inside the box exactly as it is painted there.
#[must_use]
pub(crate) fn boxed_live_rows(
    state: &UiState,
    composer: &Composer,
    width: u16,
    height: u16,
) -> u16 {
    rows_of_live(state, composer, width, height, true)
}

/// The rows a live area `width` wide asks for within `height`, `boxed` or not.
fn rows_of_live(state: &UiState, composer: &Composer, width: u16, height: u16, boxed: bool) -> u16 {
    let chooser = chooser::rows(state, composer, width);
    let input = if composer.palette_open() {
        1
    } else {
        let inner = inner_width(width, boxed);
        // A framed field keeps a little writing space even before a second
        // line is entered. The palette keeps its compact search row.
        composer
            .content_rows(editor_width(state, composer, inner))
            .max(if boxed { 2 } else { 1 })
    };
    // The chooser borrows the rail's row while it shows; the question's live
    // home takes the rows that only repeat it ([`Lent`]).
    let homed = question::homed(state, composer);
    let lent = Lent::of(state, homed, state.interrupt_armed);
    let rail = usize::from(rail_shown(state) && chooser == 0 && !lent.rail);
    let aside = usize::from(composer.aside().is_some());
    let hint = usize::from(wrapped_rows(&hint_lines(state, composer, width), width).min(3));
    let listing = composer.listing().is_some();
    let status = usize::from(status_rows(state, listing, width, lent.status));
    let edges = if boxed {
        usize::from(box_rows(state, composer))
    } else {
        0
    };
    let card = usize::from(question::rows(state, composer, width));
    let rows = rail + status + aside + card + edges + input + chooser + hint;
    // Only a multi-line draft asks past the half; a one-line draft never does.
    let readable = if input > 1 {
        u16::try_from(status + edges + input.min(READABLE_LINES) + hint).unwrap_or(u16::MAX)
    } else {
        0
    };
    // The chooser and a typed question's card are the decision at hand: they
    // may take all but the two rows the transcript and its rule keep.
    let maximum = if chooser > 0 || card > 0 {
        height.saturating_sub(2)
    } else {
        height
            .saturating_div(2)
            .max(readable.min(height.saturating_sub(2)))
    }
    .max(3);
    u16::try_from(rows.clamp(3, usize::from(maximum))).unwrap_or(maximum)
}

/// The rows a live area `width` wide asks for at rest, as [`live_rows`]
/// counts them with nothing typed, listed, set aside or busy and no room
/// limit: the rail and one status row (but those the question's live home
/// takes over, [`Lent`]), a typed question's whole card, a one-row line and
/// the waiting state's own hint. A decision's demand reads it, so typing, the
/// chooser and work never move the regions.
pub(crate) fn rest_rows(state: &UiState, width: u16) -> u16 {
    let card = question::rest_rows(state, width);
    let words = own(
        waiting_hint(&state.waiting, state.ascii, width),
        state.ascii,
    );
    let lines: Vec<Line<'_>> = words.split('\n').map(Line::raw).collect();
    // At rest no exit is armed: the same rule the live rows read ([`Lent`]).
    let lent = Lent::of(state, question::rest_homed(state), false);
    let rail = u16::from(rail_shown(state) && !lent.rail);
    let status = u16::from(!lent.status);
    rail + status + card + 1 + wrapped_rows(&lines, width).min(3)
}

/// The lifecycle rail of a first question on a new draft: the draft being
/// composed, nothing saved, checked, active or run. Only this exact rail
/// says nothing the question's live home and the object's standing do not.
const FIRST_QUESTION_RAIL: &str = "Draft ● · Saved ○ · Checked ○ · Active ○ · Run ○";

/// The Session's own status while a typed question waits, before the
/// question's label (`SessionRuntime::status_line`): the one status the
/// question's live home may take over, and only for its own label.
const QUESTION_STATUS: &str = "Needs one answer · ";

/// The live area's state rows a homed typed question takes over: the
/// lifecycle rail while it is exactly a first question's
/// ([`FIRST_QUESTION_RAIL`]), and the status while it is exactly the
/// Session's sentence for this question ([`QUESTION_STATUS`] and its label),
/// never while an exit is armed. Any other rail or status (an earlier run, a
/// saved or active workflow, a cost, a gate, words unknown here) keeps its
/// row, and nothing is read out of either. Demand, painting and the rest
/// rows read this one rule.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Lent {
    /// The rail's row goes to the card.
    rail: bool,
    /// The status row goes to the card.
    status: bool,
}

impl Lent {
    /// The rows `state`'s homed question (`homed`) takes over, an exit
    /// `armed` or not.
    fn of(state: &UiState, homed: bool, armed: bool) -> Self {
        let Waiting::QuestionDocument { asked, .. } = &state.waiting else {
            return Self::default();
        };
        if !homed {
            return Self::default();
        }
        Self {
            rail: state.rail == FIRST_QUESTION_RAIL,
            status: !armed && state.status == format!("{QUESTION_STATUS}{}", asked.label),
        }
    }
}

/// The lines of a multi-line draft a short live area still shows: the rail
/// yields its row to them.
const READABLE_LINES: usize = 3;

/// The rows a boxed composer adds: its caption and the box's top and bottom.
const BOX_ROWS: u16 = 3;

/// The rows the boxed composer adds now ([`BOX_ROWS`]): no caption under the
/// question's live home, whose card names the question right above the line,
/// nor while a proposal or a gate waits, whose own prompt names the line.
fn box_rows(state: &UiState, composer: &Composer) -> u16 {
    let named = question::homed(state, composer)
        || matches!(state.waiting, Waiting::Proposal | Waiting::Gate);
    BOX_ROWS - u16::from(named)
}

/// The columns a boxed composer takes around its line: on each side, the
/// box's edge and one cell of air.
const BOX_INSET: u16 = 4;

/// The box's edges in the ASCII glyph column.
const ASCII_BOX: border::Set<'static> = border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

/// The cells left to the prompt and the line being written in a live area
/// `width` wide, inside the box when `boxed`.
const fn inner_width(width: u16, boxed: bool) -> u16 {
    if boxed {
        width.saturating_sub(BOX_INSET)
    } else {
        width
    }
}

/// The slim caption over the boxed composer: what the next line is.
fn caption_of(state: &UiState) -> &'static str {
    if state.waiting == Waiting::Free {
        "Your message"
    } else {
        "Your answer"
    }
}

/// The loader's frames: the theme seam's own braille orbit, one motion for the
/// renderer and the run frames.
pub use nika_display::theme::SPINNER;

/// Whether the lifecycle rail takes its row. The workspace shows it once it
/// says something: a fresh session's fields, every one still pending (`○`),
/// are the opening, not a fact. Elsewhere it shows whenever reported.
fn rail_shown(state: &UiState) -> bool {
    let pending = |field: &str| field.ends_with(" ○");
    !state.rail.is_empty()
        && (state.presentation != Presentation::Workspace || !state.rail.split(" · ").all(pending))
}

/// The lifecycle rail, dim like the chrome: five facts, never one badge, on a
/// row `width` cells wide, cut with a visible mark rather than clipped.
fn rail_line(state: &UiState, width: u16) -> Line<'static> {
    Line::from(Span::styled(
        cut(&state.rail, width, state.ascii),
        Style::default().add_modifier(Modifier::DIM),
    ))
}

/// `text` on a row of `width` cells: whole when it fits, else cut at a cell
/// boundary with a visible mark, never right after a dangling separator.
fn cut(text: &str, width: u16, ascii: bool) -> String {
    let width = usize::from(width);
    if unicode_width::UnicodeWidthStr::width(text) <= width {
        return text.to_owned();
    }
    let mark = if ascii { "..." } else { "…" };
    let fitted = chooser::fit(text, width, ascii);
    let kept = fitted.strip_suffix(mark).unwrap_or(&fitted);
    format!("{}{mark}", kept.trim_end().trim_end_matches('·').trim_end())
}

/// The status of a proposal waiting for consent: not saved yet, and what a
/// plain `yes` does. It claims no other request unavailable.
const PROPOSAL_STATUS: &str = "Not saved yet · yes means Save only";

/// The status row on a row `width` cells wide. While work runs its phases
/// wrap ([`status_rows`]); otherwise it is one row: the shell's own note
/// follows the Session's words only when both fit, and never while a decision
/// waits in the fitting workspace; the Session's words are cut with a visible
/// mark, never rewritten.
fn status_line(state: &UiState, choosing: bool, width: u16) -> Line<'static> {
    let dim = role::style(Role::Dim, state.color);
    let accent = accent(state.color);
    if state.interrupt_armed {
        // What the next press does, never « interrupted »: the shell arms
        // only when nothing was interrupted (an interrupted turn and a
        // cancelled decision say so in their own words), so this row may sit
        // under a turn that just succeeded.
        let words = own("Ctrl+C again leaves · any key stays", state.ascii);
        return Line::from(Span::styled(cut(&words, width, state.ascii), accent));
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
        if !choosing && let Some(cue) = earlier_cue(state) {
            spans.push(Span::styled(cue, dim));
        }
        Line::from(spans)
    } else if state.waiting == Waiting::Proposal {
        // What a plain `yes` does, never what else may be asked for.
        let words = own(PROPOSAL_STATUS, state.ascii);
        let style = role::style(Role::Warn, state.color).add_modifier(Modifier::BOLD);
        Line::styled(cut(&words, width, state.ascii), style)
    } else {
        // Where the automation stands, then the presentation's own note.
        let mode = match state.presentation {
            _ if choosing => "",
            Presentation::Inline => "",
            Presentation::Focus => "focus · Esc returns inline · PgUp/PgDn scroll",
            // Below the minimum the focus view stands in: no panel to move to.
            Presentation::Workspace if !crate::workspace::geometry::fits(state.size) => {
                "workspace needs 60x16 · Esc inline · PgUp/PgDn scroll"
            }
            // A decision waiting names its own keys: the panel's note waits.
            Presentation::Workspace if state.waiting != Waiting::Free => "",
            Presentation::Workspace => "workspace · F6 panel · Esc back",
        };
        let mode = own(mode, state.ascii);
        let sep = own(" · ", state.ascii);
        let text = match (state.status.is_empty(), mode.is_empty()) {
            (true, _) => mode,
            (false, true) => state.status.clone(),
            (false, false) => format!("{}{sep}{mode}", state.status),
        };
        let cue = if choosing {
            String::new()
        } else {
            earlier_cue(state).unwrap_or_default()
        };
        let whole = format!("{text}{cue}");
        let fits = unicode_width::UnicodeWidthStr::width(whole.as_str()) <= usize::from(width);
        let shown = if fits {
            whole
        } else {
            cut(&state.status, width, state.ascii)
        };
        Line::from(Span::styled(shown, dim))
    }
}

/// While the full-screen transcript is scrolled back, new activity keeps the
/// reading place; the row says so and names the key back to the latest.
fn earlier_cue(state: &UiState) -> Option<String> {
    (state.focus_scroll > 0
        && state.presentation != Presentation::Inline
        && !(state.presentation == Presentation::Workspace
            && crate::workspace::geometry::fits(state.size)))
    .then(|| own(" · reading earlier messages · End: latest", state.ascii))
}

/// Preparation has Stop and queued corrections; a Run keeps its separate controls.
/// The action's resulting notice still replaces this hint when the human acts.
const WORKING_HINT: &str =
    "Preparing: Ctrl+C requests Stop; correction + Enter. Run: typing waits.";

/// The key that opens the palette of every command and view key: named once,
/// on the idle hint row, always with what it does.
const PALETTE_HINT: &str = "Ctrl+O: commands";

/// The narrower forms of `waiting`'s hint ([`Waiting::hint`]), longest first,
/// each a whole cue shorter or lower: a row too narrow for one form takes the
/// next, so no key is ever parted from what it does. A `\n` sets the cues
/// after it on a second row rather than drop them.
fn narrower(waiting: &Waiting) -> &'static [&'static str] {
    match waiting {
        Waiting::Question { key } | Waiting::QuestionDocument { key, .. }
            if key == "unknown_cost" || key == "run_cost" =>
        {
            &[
                "yes approves once · no or Ctrl+C cancels",
                "yes approves once · no cancels",
            ]
        }
        Waiting::QuestionDocument { .. } if crate::composer::answer::choice(waiting).is_some() => {
            &[
                "↑↓ choose · Enter answers · cancel drops it",
                "Enter answers · cancel drops it",
                "Enter answers",
            ]
        }
        Waiting::QuestionDocument { .. } => &["Enter answers"],
        Waiting::Question { .. } => &["answer the question above"],
        Waiting::Proposal => &["yes + Enter: Save · no: cancel"],
        Waiting::Gate => &["approve or refuse"],
        Waiting::Choosing => &[
            "1 account · 2 API · 3 local · 4 no AI\ncancel",
            "1 to 4 chooses · cancel",
        ],
        Waiting::Free => &[],
    }
}

/// The form of `waiting`'s hint a row `width` cells wide holds ([`narrower`]),
/// the narrowest when none does.
fn waiting_hint(waiting: &Waiting, ascii: bool, width: u16) -> &'static str {
    let (full, forms) = (waiting.hint(), narrower(waiting));
    (std::iter::once(full).chain(forms.iter().copied()))
        .find(|hint| fits_row(hint, ascii, width))
        .unwrap_or_else(|| forms.last().copied().unwrap_or(full))
}

/// Whether each row of `text` holds in `width` cells as the row paints it: in
/// the glyph column in use, measured with the layout's own width table.
fn fits_row(text: &str, ascii: bool, width: u16) -> bool {
    own(text, ascii)
        .split('\n')
        .all(|row| unicode_width::UnicodeWidthStr::width(row) <= usize::from(width))
}

/// The hint's rows: a notice the last key left, else how to choose while the
/// chooser shows, else what keys do while a turn works, else what the
/// waiting state takes. At a free prompt the row is the one place that names
/// the palette key; the workspace's composer also says how `Enter` sends and
/// `Alt+Enter` breaks a line, and keeps `F6` and `Esc` on its status row and
/// the empty composer invites `/`, so neither repeats here. One row, unless a
/// decision's form sets a whole cue on a second ([`narrower`]).
fn hint_lines(state: &UiState, composer: &Composer, width: u16) -> Vec<Line<'static>> {
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
        // Scrolled back, this row owns the keyboard cue beside the clickable marker. A
        // narrower row drops a whole cue, never the action after the palette key.
        let hints = if state.focus_scroll > 0 {
            [
                "click chat; End: latest · /intelligence · Ctrl+O: commands",
                "click chat; End: latest · /intelligence",
                "End: latest · /intelligence",
            ]
        } else {
            [
                "Enter send · Alt+Enter new line · Ctrl+O: commands",
                "Enter send · Ctrl+O: commands",
                PALETTE_HINT,
            ]
        };
        let last = hints[hints.len() - 1];
        hints
            .into_iter()
            .find(|hint| fits_row(hint, state.ascii, width))
            .unwrap_or(last)
            .to_owned()
    } else if state.waiting == Waiting::Free {
        let hint = format!("{} · {PALETTE_HINT}", state.waiting.hint());
        if fits_row(&hint, state.ascii, width) {
            hint
        } else {
            state.waiting.hint().to_owned()
        }
    } else {
        // A narrower row drops a whole cue, never a key from what it does.
        waiting_hint(&state.waiting, state.ascii, width).to_owned()
    };
    let text = match (&state.completion, choosing) {
        (Some(notice), _) => own(notice, state.ascii),
        (None, Some(choosing)) => choosing,
        (None, None) => own(&idle, state.ascii),
    };
    let tone = match state.waiting {
        Waiting::Proposal | Waiting::Gate if state.busy.is_none() => Role::Warn,
        Waiting::Free if state.busy.is_none() && state.completion.is_none() => Role::Dim,
        _ => Role::Accent,
    };
    let style = role::style(tone, state.color);
    text.split('\n')
        .map(|row| Line::from(Span::styled(row.to_owned(), style)))
        .collect()
}

/// The rows of the live area, top to bottom: the rail, the status, a draft
/// set aside, a typed question's card, the caption and the box when boxed,
/// the line being written (or the palette's search), the chooser under it, a
/// filler, the hint.
struct LiveAreas {
    rail: Rect,
    status: Rect,
    aside: Rect,
    /// The typed question's card (`question`): painting and the pointer
    /// read this one rectangle.
    card: Rect,
    /// The caption and the box (its edges included), when boxed.
    boxed: Option<(Rect, Rect)>,
    /// The prompt and the line being written: inside the box when boxed.
    input: Rect,
    chooser: Rect,
    hint: Rect,
}

/// Cut `area` into the live rows. Closed, the line being written takes every
/// spare row, as it always did; while the chooser shows, the line takes its
/// own rows, the chooser what it asks for, and the spare rows go below it.
/// `boxed`, the line stands in a box under its caption, one cell of air
/// inside each edge: sizing, wrapping and painting read these same cells.
fn live_areas(state: &UiState, composer: &Composer, area: Rect, boxed: bool) -> LiveAreas {
    let chooser_wanted =
        u16::try_from(chooser::rows(state, composer, area.width)).unwrap_or(u16::MAX);
    let choosing = chooser_wanted > 0;
    let hint_wanted = wrapped_rows(&hint_lines(state, composer, area.width), area.width).min(3);
    let homed = question::homed(state, composer);
    let lent = Lent::of(state, homed, state.interrupt_armed);
    let listing = composer.listing().is_some();
    let status_wanted = status_rows(state, listing, area.width, lent.status);
    let inner = inner_width(area.width, boxed);
    let lines = if composer.palette_open() {
        1
    } else {
        composer.rows(editor_width(state, composer, inner))
    };
    let readable = lines.min(u16::try_from(READABLE_LINES).unwrap_or(u16::MAX));
    let edges = if boxed { box_rows(state, composer) } else { 0 };
    let card_wanted = question::rows(state, composer, area.width);
    // The question's live home is short of rows: the rail lends it its row.
    let short =
        homed && area.height < 1 + status_wanted + card_wanted + edges + readable + hint_wanted;
    // The rail takes a row of its own above the status (both are full
    // sentences; one 80-column row cannot hold them side by side), yields it
    // on a terminal too short for four rows or to the first lines of a
    // multi-line draft, and lends it to the chooser, a short live home and a
    // live home it only repeats ([`Lent`]).
    let rail_rows = u16::from(
        rail_shown(state)
            && !choosing
            && !short
            && !lent.rail
            && area.height >= 4 + edges
            && (lines < 2 || area.height > hint_wanted + status_wanted + readable + edges),
    );
    let hint_rows = hint_wanted.min(area.height.saturating_sub(rail_rows + edges + 2).max(1));
    let status_rows = status_wanted.min(
        area.height
            .saturating_sub(rail_rows + edges + hint_rows + 1)
            .max(1),
    );
    let fixed = rail_rows + hint_rows + status_rows + edges;
    let aside_rows = u16::from(composer.aside().is_some() && area.height >= fixed + 2);
    let spare = area.height.saturating_sub(fixed + aside_rows);
    // A typed question's card stands above the line that answers it and gives
    // way first: the line keeps a row of its own.
    let card_rows = card_wanted.min(spare.saturating_sub(1));
    let spare = spare - card_rows;
    let (input, chooser, filler) = if choosing {
        let input = lines.clamp(1, spare.max(1));
        let chooser = chooser_wanted.min(spare.saturating_sub(input));
        (
            Constraint::Length(input + edges),
            Constraint::Length(chooser),
            Constraint::Min(0),
        )
    } else {
        (
            Constraint::Min(1 + edges),
            Constraint::Length(0),
            Constraint::Length(0),
        )
    };
    let [rail, status, aside, card, input, chooser, _, hint] = Layout::vertical([
        Constraint::Length(rail_rows),
        Constraint::Length(status_rows),
        Constraint::Length(aside_rows),
        Constraint::Length(card_rows),
        input,
        chooser,
        filler,
        Constraint::Length(hint_rows),
    ])
    .areas(area);
    // Boxed, the caption takes the first row (none under the live home) and
    // the box the rest, the line one cell of air inside each edge.
    let (boxed, input) = if boxed {
        let caption = edges.saturating_sub(BOX_ROWS - 1);
        let [caption, frame] =
            Layout::vertical([Constraint::Length(caption), Constraint::Min(0)]).areas(input);
        let air = Margin::new(BOX_INSET / 2, 1);
        (Some((caption, frame)), frame.inner(air))
    } else {
        (None, input)
    };
    LiveAreas {
        rail,
        status,
        aside,
        card,
        boxed,
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
/// `area`: the same live area inline, under the focus transcript and in a
/// short workspace panel.
pub(crate) fn render_live(frame: &mut Frame<'_>, state: &UiState, composer: &Composer, area: Rect) {
    render_live_in(frame, state, composer, area, false);
}

/// The workspace's live area with its composer boxed under a slim caption
/// (« Your message », or « Your answer » while a decision waits; none where
/// the question's live home or the decision's own prompt names the line,
/// [`box_rows`]): the same prompt, line, chooser and hint as [`render_live`],
/// in quiet edges.
pub(crate) fn render_boxed_live(
    frame: &mut Frame<'_>,
    state: &UiState,
    composer: &Composer,
    area: Rect,
) {
    render_live_in(frame, state, composer, area, true);
}

fn render_live_in(
    frame: &mut Frame<'_>,
    state: &UiState,
    composer: &Composer,
    area: Rect,
    boxed: bool,
) {
    let areas = live_areas(state, composer, area, boxed);
    if areas.rail.height > 0 {
        let rail = rail_line(state, areas.rail.width);
        frame.render_widget(Paragraph::new(rail), areas.rail);
    }
    let status = status_line(state, composer.listing().is_some(), areas.status.width);
    frame.render_widget(
        Paragraph::new(status).wrap(Wrap { trim: false }),
        areas.status,
    );
    if let Some(aside) = composer.aside().filter(|_| areas.aside.height > 0) {
        let line = chooser::aside_line(aside, areas.aside.width, state.ascii, state.color);
        frame.render_widget(Paragraph::new(line), areas.aside);
    }
    question::render(state, composer, areas.card, frame.buffer_mut());
    if let Some((caption, edges)) = areas.boxed {
        let quiet = role::style(Role::Dim, state.color);
        frame.render_widget(
            Paragraph::new(Line::styled(caption_of(state), quiet)),
            caption,
        );
        let set = if state.ascii {
            ASCII_BOX
        } else {
            border::ROUNDED
        };
        let frame_style = role::border(state.color);
        frame.render_widget(
            Block::bordered().border_set(set).border_style(frame_style),
            edges,
        );
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
        if boxed && state.color && composer.is_blank() {
            frame.buffer_mut().set_style(
                editor,
                role::style(Role::Dim, true).remove_modifier(Modifier::DIM | Modifier::BOLD),
            );
        }
    }
    chooser::render(state, composer, areas.chooser, frame.buffer_mut());
    frame.render_widget(
        Paragraph::new(hint_lines(state, composer, area.width)).wrap(Wrap { trim: false }),
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
        // The focus view standing in for a workspace too small to fit: every
        // block as said, as its scroll bounds measure it.
        crate::workspace::cards::render(frame, state, area, (None, None));
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

/// The focus presentation's rows on `area`: the transcript, its rule, the
/// live area. Painting and the pointer read these same rectangles.
pub(crate) fn focus_areas(area: Rect, state: &UiState, composer: &Composer) -> [Rect; 3] {
    let live = live_rows(state, composer, area.width, area.height);
    Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(live),
    ])
    .areas(area)
}

/// The focus presentation: the transcript above (scrolled from the end), a
/// rule, the same live area below.
pub fn draw_focus(frame: &mut Frame<'_>, state: &UiState, composer: &Composer) {
    let [transcript, rule, bottom] = focus_areas(frame.area(), state, composer);
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
mod tests;
