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
//! What the chooser lists, and the offers of a typed choice, open as one
//! surface above the line (`surface`): a band from the transcript's first
//! row down to the composer. The live area is sized by facts at rest, so
//! opening, searching or closing a surface moves no row and the transcript
//! keeps its reading position under the band ([`band`]); an inline frame
//! grows for its band instead. The card of a typed question that offers
//! nothing stands above the line (`question`): it may take all but two rows
//! of what it is given, and it gives way first, so the line that answers
//! keeps its row. In the workspace's conversation the current proposal's
//! review (`Consent`) gives the live area its standing and the one row that
//! names every consent word.

mod chooser;
#[cfg(test)]
mod chooser_tests;
pub(crate) mod question;
#[cfg(test)]
mod question_tests;
pub(crate) mod surface;
#[cfg(test)]
mod surface_tests;

pub(crate) use nika_tui_view::workspace::text::own;
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
use crate::workspace::cards::review::Review;

/// The current proposal the workspace's conversation reviews
/// (`review::consent`): what the live area's standing, rail and decision row
/// read. `None` wherever no review is painted: inline, focus, and any held,
/// untagged, older or other proposal or other waiting state.
pub(crate) type Consent<'a> = Option<&'a Review>;

/// How the workspace's conversation panel draws its live area: its composer
/// boxed under a slim caption or plain, and the [`Consent`] it reads. Demand,
/// painting, the rest rows and the pointer take the same one.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Panel<'a> {
    /// The composer stands in a box under its caption ([`render_panel_live`]).
    pub(crate) boxed: bool,
    /// The current proposal the conversation reviews.
    pub(crate) consent: Consent<'a>,
}

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
        // Work names its phases, never a standing: no review is read.
        wrapped_rows(&[status_line(state, choosing, width, None)], width).min(3)
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
    panel_rows(state, composer, width, height, Panel::default())
}

/// [`live_rows`] of the workspace's conversation panel
/// ([`render_panel_live`]): boxed, the caption and the box's two edges as
/// well, the line wrapped inside the box exactly as it is painted there; the
/// rail, the status and the hint as the panel's [`Consent`] reads them.
#[must_use]
pub(crate) fn panel_rows(
    state: &UiState,
    composer: &Composer,
    width: u16,
    height: u16,
    Panel { boxed, consent }: Panel<'_>,
) -> u16 {
    // An inline frame grows for a surface's band; a full screen paints the
    // band over its transcript and keeps every row ([`band`]).
    let band = if state.presentation == Presentation::Inline {
        usize::from(surface::demand(state, composer, width))
    } else {
        0
    };
    // The line's own rows beside the waiting prompt, whatever a surface
    // shows: a framed field keeps a little writing space even before a
    // second line is entered.
    let inner = inner_width(width, boxed);
    let input = composer
        .content_rows(editor_width(state, inner))
        .max(if boxed { 2 } else { 1 });
    // The question's live home and the reviewed proposal take the rows that
    // only repeat them ([`Lent`]).
    let homed = question::homed(state);
    let lent = Lent::of(state, homed, state.interrupt_armed, consent);
    let rail = usize::from(rail_shown(state) && !lent.rail);
    let aside = usize::from(composer.aside().is_some());
    let hint = hint_lines(state, composer, width, (consent, false));
    let hint = usize::from(wrapped_rows(&hint, width).min(3));
    let status = usize::from(status_rows(state, false, width, lent.status));
    let edges = if boxed {
        usize::from(box_rows(state))
    } else {
        0
    };
    let card = usize::from(question::rows(state, width));
    let rows = band + rail + status + aside + card + edges + input + hint;
    // Only a multi-line draft asks past the half; a one-line draft never does.
    let readable = if input > 1 {
        u16::try_from(status + edges + input.min(READABLE_LINES) + hint).unwrap_or(u16::MAX)
    } else {
        0
    };
    // An inline band and a typed question's card are the decision at hand:
    // they may take all but the two rows the transcript and its rule keep.
    let maximum = if band > 0 || card > 0 {
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
/// or the reviewed proposal takes over, [`Lent`]), a typed question's whole
/// card, a one-row line and the waiting state's own hint ([`decision_hint`],
/// read with `consent`). A decision's demand reads it, so typing, the
/// chooser and work never move the regions.
pub(crate) fn rest_rows(state: &UiState, width: u16, consent: Consent<'_>) -> u16 {
    let card = question::rest_rows(state, width);
    let hint = decision_hint(&state.waiting, consent, state.ascii, width);
    let words = own(hint, state.ascii);
    let lines: Vec<Line<'_>> = words.split('\n').map(Line::raw).collect();
    // At rest no exit is armed: the same rule the live rows read ([`Lent`]).
    let lent = Lent::of(state, question::rest_homed(state), false, consent);
    let rail = u16::from(rail_shown(state) && !lent.rail);
    let status = u16::from(!lent.status);
    rail + status + card + 1 + wrapped_rows(&lines, width).min(3)
}

/// The lifecycle rail of a first question on a new draft: the draft being
/// composed, nothing saved, checked, active or run. Only this exact rail
/// says nothing the question's live home and the object's standing do not.
const FIRST_QUESTION_RAIL: &str = "Draft ● · Saved ○ · Checked ○ · Active ○ · Run ○";

/// The lifecycle rail while a proposal waits and nothing else stands: the
/// draft proposed, nothing saved, checked, active or run. Only this exact
/// rail says nothing the reviewed proposal's standing does not.
const PROPOSAL_RAIL: &str = "Draft ✓ · Saved ○ · Checked ○ · Active ○ · Run ○";

/// The Session's own status while a typed question waits, before the
/// question's label (`SessionRuntime::status_line`): the one status the
/// question's live home may take over, and only for its own label.
const QUESTION_STATUS: &str = "Needs one answer · ";

/// The live area's state rows a homed typed question or the reviewed proposal
/// takes over: the lifecycle rail while it is exactly a first question's
/// ([`FIRST_QUESTION_RAIL`]) or exactly the waiting proposal's own
/// ([`PROPOSAL_RAIL`], its standing on the status row), and, for the
/// question, the status while it is exactly the Session's sentence for it
/// ([`QUESTION_STATUS`] and its label), never while an exit is armed. Any
/// other rail or status (an earlier run, a saved or active workflow, a cost,
/// a gate, words unknown here) keeps its row, and nothing is read out of
/// either. Demand, painting and the rest rows read this one rule.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Lent {
    /// The rail's row goes to the card.
    rail: bool,
    /// The status row goes to the card.
    status: bool,
}

impl Lent {
    /// The rows `state`'s homed question (`homed`) or reviewed proposal
    /// (`consent`) takes over, an exit `armed` or not.
    fn of(state: &UiState, homed: bool, armed: bool, consent: Consent<'_>) -> Self {
        if consent.is_some() && state.waiting == Waiting::Proposal {
            return Self {
                rail: state.rail == PROPOSAL_RAIL,
                status: false,
            };
        }
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
fn box_rows(state: &UiState) -> u16 {
    let named =
        question::homed(state) || matches!(state.waiting, Waiting::Proposal | Waiting::Gate);
    BOX_ROWS - u16::from(named)
}

/// The columns a boxed composer takes around its line: on each side, the
/// box's edge and one cell of air.
const BOX_INSET: u16 = 4;

/// A box's edges in the ASCII glyph column: the composer's, the reader's.
pub(crate) const ASCII_BOX: border::Set<'static> = border::Set {
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
/// row `width` cells wide, cut with a visible mark rather than clipped
/// ([`newest`]).
fn rail_line(state: &UiState, width: u16) -> Line<'static> {
    Line::from(Span::styled(
        newest(&state.rail, width, state.ascii),
        Style::default().add_modifier(Modifier::DIM),
    ))
}

/// `rail` on a row of `width` cells, its newest stages kept: whole when it
/// fits; else each leading done stage that the next done stage implies (a
/// save after its draft, a check after its save) gives way under one mark at
/// its head, only while the rest still does not fit, then the end is cut
/// ([`cut`]). Its fixed fields are read as [`rail_shown`] reads them.
fn newest(rail: &str, width: u16, ascii: bool) -> String {
    let done = |field: &str| field.ends_with(" ✓");
    let mark = if ascii { "... " } else { "… " };
    let mut fields: Vec<&str> = rail.split(" · ").collect();
    let mut head = "";
    let fits = |head: &str, fields: &[&str]| {
        let text = format!("{head}{}", fields.join(" · "));
        unicode_width::UnicodeWidthStr::width(text.as_str()) <= usize::from(width)
    };
    while fields.len() > 1 && done(fields[0]) && done(fields[1]) && !fits(head, &fields) {
        fields.remove(0);
        head = mark;
    }
    cut(&format!("{head}{}", fields.join(" · ")), width, ascii)
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

/// The reviewed proposal's standing, longest first: its decision row names
/// what each word does, so the status keeps the standing alone, scoped to
/// this proposal (an earlier Run keeps its own facts on the rail).
const STANDING: [&str; 2] = ["Not saved yet · this proposal has not run", "Not saved yet"];

/// The status row on a row `width` cells wide. While work runs its phases
/// wrap ([`status_rows`]); otherwise it is one row: the shell's own note
/// follows the Session's words only when both fit, and never while a decision
/// waits in the fitting workspace; the Session's words are cut with a visible
/// mark, never rewritten. A proposal waiting says its standing, alone where
/// the conversation reviews it (`consent`).
fn status_line(state: &UiState, choosing: bool, width: u16, consent: Consent<'_>) -> Line<'static> {
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
        // Reviewed, the standing alone in its longest whole form; else what a
        // plain `yes` does, never what else may be asked for.
        let words = match consent {
            Some(_) => STANDING
                .into_iter()
                .find(|words| fits_row(words, state.ascii, width))
                .unwrap_or(STANDING[1]),
            None => PROPOSAL_STATUS,
        };
        let words = own(words, state.ascii);
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

/// The decision row while the reviewed proposal's `save & run` is admitted
/// ([`Review::runs`]), longest first: each consent word typed at `Save? ›`
/// with what it does, the combined act first. A narrower row drops a whole
/// cue (the key, an effect, `/show`), never an action: the last form still
/// names all three words in 36 cells, in either glyph column.
const SAVE_RUN_HINTS: [&str; 5] = [
    "save & run + Enter: Save, then Run once · yes: Save only · no: discard · /show: bytes",
    "save & run: Save, then Run once · yes: Save only · no: discard · /show: bytes",
    "save & run · yes: Save only · no: discard · /show: bytes",
    "save & run · yes: Save only · no: discard",
    "save & run · yes: save · no: discard",
];

/// The decision row's words on a row `width` cells wide: the reviewed
/// proposal's every consent word while its `save & run` is admitted
/// ([`SAVE_RUN_HINTS`]); else `waiting`'s own hint ([`waiting_hint`]). Words
/// only: nothing here sends, saves or runs.
fn decision_hint(waiting: &Waiting, consent: Consent<'_>, ascii: bool, width: u16) -> &'static str {
    if *waiting != Waiting::Proposal || !consent.is_some_and(Review::runs) {
        return waiting_hint(waiting, ascii, width);
    }
    let last = SAVE_RUN_HINTS[SAVE_RUN_HINTS.len() - 1];
    (SAVE_RUN_HINTS.into_iter())
        .find(|hint| fits_row(hint, ascii, width))
        .unwrap_or(last)
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
/// decision's form sets a whole cue on a second ([`narrower`]); the reviewed
/// proposal's row names every consent word ([`decision_hint`]).
fn hint_lines(
    state: &UiState,
    composer: &Composer,
    width: u16,
    (consent, listed): (Consent<'_>, bool),
) -> Vec<Line<'static>> {
    let choosing = (composer.listing())
        .filter(|_| listed)
        .map(|listing| chooser::hint(&listing, state.busy.is_some(), state.ascii, width));
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
        decision_hint(&state.waiting, consent, state.ascii, width).to_owned()
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
/// set aside, a typed question's card, the rows an inline surface's band
/// takes, the caption and the box when boxed, the line being written (or the
/// palette's search, or a typed choice's own reply), the hint.
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
    hint: Rect,
}

/// [`areas_of`] a live area reviewing nothing: the question's card and the
/// line read the same rectangles everywhere.
fn live_areas(state: &UiState, composer: &Composer, area: Rect, boxed: bool) -> LiveAreas {
    let panel = Panel {
        boxed,
        consent: None,
    };
    areas_of(state, composer, area, panel)
}

/// Cut `area` into the live rows, read from facts at rest whatever a surface
/// shows: the line being written takes its own rows right above the hint,
/// and the rows left stand above it, where an inline surface's band paints.
/// `boxed`, the line stands in a box under its caption, one cell of air
/// inside each edge: sizing, wrapping and painting read these same cells.
fn areas_of(state: &UiState, composer: &Composer, area: Rect, panel: Panel<'_>) -> LiveAreas {
    let Panel { boxed, consent } = panel;
    let hint = hint_lines(state, composer, area.width, (consent, false));
    let hint_wanted = wrapped_rows(&hint, area.width).min(3);
    let homed = question::homed(state);
    let lent = Lent::of(state, homed, state.interrupt_armed, consent);
    let status_wanted = status_rows(state, false, area.width, lent.status);
    let inner = inner_width(area.width, boxed);
    let lines = composer.rows(editor_width(state, inner));
    let readable = lines.min(u16::try_from(READABLE_LINES).unwrap_or(u16::MAX));
    let edges = if boxed { box_rows(state) } else { 0 };
    let card_wanted = question::rows(state, area.width);
    // The question's live home is short of rows: the rail lends it its row.
    let short =
        homed && area.height < 1 + status_wanted + card_wanted + edges + readable + hint_wanted;
    // The rail takes a row of its own above the status (both are full
    // sentences; one 80-column row cannot hold them side by side), yields it
    // on a terminal too short for four rows or to the first lines of a
    // multi-line draft, and lends it to a short live home and a live home or
    // a reviewed proposal's standing it only repeats ([`Lent`]).
    let rail_rows = u16::from(
        rail_shown(state)
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
    // The line keeps the rows the live area reserves for it at rest (two
    // inside a box); a frame taller than that leaves its rows above it.
    let input = lines.max(if boxed { 2 } else { 1 }).clamp(1, spare.max(1));
    let [rail, status, aside, card, _, input, hint] = Layout::vertical([
        Constraint::Length(rail_rows),
        Constraint::Length(status_rows),
        Constraint::Length(aside_rows),
        Constraint::Length(card_rows),
        Constraint::Min(0),
        Constraint::Length(input + edges),
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
        hint,
    }
}

/// A boxed composer's edges under an open surface: its top corners join the
/// band's sides, so the band and the box read as one shape.
const JOINED: border::Set<'static> = border::Set {
    top_left: "├",
    top_right: "┤",
    ..border::ROUNDED
};

/// The rows a surface paints over for the live area `live` drawn as
/// `panel`, under a view whose first row is `ceiling` (the transcript's, or
/// the live area's own inline): from there down to the composer (its box's
/// top edge when boxed) across the live area's columns, and whether the band
/// stands framed, joined to that box. `None` while no surface is open or no
/// row is left. The band hides what it covers and moves nothing.
pub(crate) fn band(
    state: &UiState,
    composer: &Composer,
    ceiling: u16,
    live: Rect,
    panel: Panel<'_>,
) -> Option<(Rect, bool)> {
    if !surface::open(state, composer) {
        return None;
    }
    let areas = areas_of(state, composer, live, panel);
    let floor = areas.boxed.map_or(areas.input.y, |(_, frame)| frame.y);
    let rows = floor.saturating_sub(ceiling);
    (rows > 0).then_some((Rect::new(live.x, ceiling, live.width, rows), panel.boxed))
}

/// Paint the surface open over the live area `live` drawn as `panel` from
/// row `ceiling` ([`band`]), after everything it covers.
pub(crate) fn paint_band(
    frame: &mut Frame<'_>,
    (state, composer): (&UiState, &Composer),
    ceiling: u16,
    live: Rect,
    panel: Panel<'_>,
) {
    if let Some(band) = band(state, composer, ceiling, live, panel) {
        surface::render(state, composer, band, frame.buffer_mut());
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

/// The cells left to the line being written beside the waiting prompt: the
/// palette's search paints in the same row without changing how the line
/// wraps, so opening it moves nothing.
fn editor_width(state: &UiState, width: u16) -> u16 {
    let prompt = u16::try_from(state.waiting.prompt().chars().count()).unwrap_or(8);
    width.saturating_sub(prompt).max(8)
}

/// Draw the live area (status · prompt + composer · chooser · hint) into
/// `area`: the same live area inline and under the focus transcript.
pub(crate) fn render_live(frame: &mut Frame<'_>, state: &UiState, composer: &Composer, area: Rect) {
    render_panel_live(frame, state, composer, area, Panel::default());
}

/// The live area of the workspace's conversation panel: its composer boxed
/// under a slim caption where `panel` says so (« Your message », or « Your
/// answer » while a decision waits; none where the question's live home or
/// the decision's own prompt names the line, [`box_rows`]), its standing,
/// rail and decision row as the panel's [`Consent`] reads them: the same
/// prompt, line, chooser and hint as [`render_live`].
pub(crate) fn render_panel_live(
    frame: &mut Frame<'_>,
    state: &UiState,
    composer: &Composer,
    area: Rect,
    panel: Panel<'_>,
) {
    let (boxed, consent) = (panel.boxed, panel.consent);
    let areas = areas_of(state, composer, area, panel);
    if areas.rail.height > 0 {
        let rail = rail_line(state, areas.rail.width);
        frame.render_widget(Paragraph::new(rail), areas.rail);
    }
    let choosing = composer.listing().is_some();
    let status = status_line(state, choosing, areas.status.width, consent);
    frame.render_widget(
        Paragraph::new(status).wrap(Wrap { trim: false }),
        areas.status,
    );
    if let Some(aside) = composer.aside().filter(|_| areas.aside.height > 0) {
        let line = chooser::aside_line(aside, areas.aside.width, state.ascii, state.color);
        frame.render_widget(Paragraph::new(line), areas.aside);
    }
    question::render(state, areas.card, frame.buffer_mut());
    if let Some((caption, edges)) = areas.boxed {
        let quiet = role::style(Role::Dim, state.color);
        frame.render_widget(
            Paragraph::new(Line::styled(caption_of(state), quiet)),
            caption,
        );
        let set = match (state.ascii, surface::open(state, composer)) {
            (true, _) => ASCII_BOX,
            (false, true) => JOINED,
            (false, false) => border::ROUNDED,
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
    } else if surface::choosing(state, composer) {
        // The typed choice holds the line: its own reply's field stands in
        // the draft's place, the draft waiting untouched out of view.
        composer.render_own(editor, frame.buffer_mut());
    } else {
        composer.render(editor, frame.buffer_mut());
        if boxed && state.color && composer.is_blank() {
            frame.buffer_mut().set_style(
                editor,
                role::style(Role::Dim, true).remove_modifier(Modifier::DIM | Modifier::BOLD),
            );
        }
    }
    let hint = hint_lines(state, composer, area.width, (consent, true));
    frame.render_widget(Paragraph::new(hint).wrap(Wrap { trim: false }), areas.hint);
}

/// The inline presentation: the frame IS the live area (the transcript is
/// the terminal's own scrollback).
pub fn draw_inline(frame: &mut Frame<'_>, state: &UiState, composer: &Composer) {
    let area = frame.area();
    render_live(frame, state, composer, area);
    paint_band(frame, (state, composer), area.y, area, Panel::default());
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
    paint_band(
        frame,
        (state, composer),
        transcript.y,
        bottom,
        Panel::default(),
    );
}

#[cfg(test)]
mod tests;
