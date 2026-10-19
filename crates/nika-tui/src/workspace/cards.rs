// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One continuous conversation, with bounded decision and diagnostic cards.
//! Labels follow typed blocks, never parsed prose. Consecutive blocks of one
//! speaker share one bubble ([`bubble`]): the human's at the right end on the
//! raised surface, Nika's at the left. A question, a proposal, a gate and a
//! refusal keep their bounded card. One plan of pieces both measures and
//! paints, so the scroll bounds are the rows drawn; clipping can start inside
//! a piece without losing wrapped text, and no block's words are rewritten.
//! A refusal recognised by the Session's exact sentence reads first as a short
//! summary ([`diagnostics`]), a run's story folded at its commit as one quiet
//! row of its run and state, and the current proposal, tied to the candidate
//! by identity, as its typed review ([`review`]), its identity and reader key
//! on its bottom border; any other proposal is history, quietly titled. Each
//! block keeps the Session's words whole, and measuring and painting use the
//! same lines; a proposal block said beside a proposal that waited no longer
//! reads that it still waits once none does ([`decided`]). The question the
//! live card carries with its words (`render::question`) reads one quiet row
//! where its block stands, the block itself untouched.

// The presenter's file sits beside the root modules; the cards that paint it
// own the module.
#[path = "../diagnostics.rs"]
pub(crate) mod diagnostics;

mod bubble;
pub(crate) mod review;

use std::ops::Range;

use nika_display::theme::Role;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use self::bubble::{Bubble, Form};
use self::diagnostics::{Shown, Summary};
use self::review::Review;
use super::text::{fit_head, marks};

use crate::lang::Voice;
use crate::model::{Committed, Kind, UiState, Waiting};
use crate::render::{block_lines, block_lines_within, content_rows, window};
use crate::visual::role;

/// The name a card or a bubble wears for a block of `kind`, said in `voice`,
/// and the role its title wears.
fn heading(kind: Kind, voice: Voice) -> (&'static str, Role) {
    let (name, role) = match kind {
        Kind::Human => ("You", Role::Dim),
        Kind::Question => ("Question", Role::Warn),
        Kind::Proposal => ("Review before saving", Role::Warn),
        Kind::Gate => ("Approval needed", Role::Warn),
        Kind::Refusal => ("Could not continue", Role::Bad),
        Kind::Run => ("Run", Role::VerbInvoke),
        Kind::Result => ("Result", Role::Good),
        Kind::Report => ("Report", Role::Accent),
        Kind::Activity => ("Activity", Role::Accent),
        Kind::Banner => ("Nika", Role::VerbAgent),
        Kind::Notice | Kind::Reply => ("Nika", Role::Accent),
    };
    (voice.say(name), role)
}

/// The title a card wears: a proposal reads as the decision under review only
/// while the Session waits on it (`pending`); any other proposal is history.
fn card_heading(block: &Committed, pending: bool, voice: Voice) -> (&'static str, Role) {
    if block.kind == Kind::Proposal && !pending {
        (voice.say("Proposal"), Role::Dim)
    } else {
        heading(block.kind, voice)
    }
}

/// The lines one card paints in rows `width` cells wide: the Session's
/// words, or the summary of a recognised refusal; the block itself is never
/// rewritten. A line pointing to a terminal for the check's hints reads the
/// key that shows them here ([`hints_here`]).
fn card_lines(block: &Committed, color: bool, ascii: bool, width: u16) -> Vec<Line<'static>> {
    match diagnostics::shown(block) {
        Shown::Said => match hints_here(&block.text) {
            Some(text) => {
                block_lines_within(&Committed::new(block.kind, text), color, ascii, width)
            }
            None => block_lines_within(block, color, ascii, width),
        },
        Shown::Banner(text) => {
            block_lines_within(&Committed::new(block.kind, text), color, ascii, width)
        }
        Shown::Refusal(summary) => summary_lines(block.kind, summary, color, ascii),
        Shown::Folded(fold) => {
            // How the run ended is the Session's block's to say, never a settlement's word.
            let (sep, dim) = (marks(ascii).0, role::style(Role::Dim, color));
            vec![Line::styled(format!("{}{sep}F2", fold.label), dim)]
        }
    }
}

/// The visible speaker label replaces the compact sent-message marker; the
/// lines are painted in rows `width` cells wide.
fn message_lines(block: &Committed, color: bool, ascii: bool, width: u16) -> Vec<Line<'static>> {
    if block.kind == Kind::Human {
        let said = Committed::new(Kind::Reply, block.text.clone());
        block_lines_within(&said, color, ascii, width)
    } else {
        card_lines(block, color, ascii, width)
    }
}

/// Only a decision or failure needs a bounded surface in the transcript.
fn framed(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Question | Kind::Proposal | Kind::Gate | Kind::Refusal
    )
}

/// The rows a bounded card takes for `rows` of words: its title, its bottom
/// border and one row of air.
fn card_height(rows: usize) -> usize {
    rows.saturating_add(3)
}

/// Where a question the live card carries stood: its words wait below, at the
/// line that answers them.
const CARRIED: (&str, &str) = ("↓ the question waits below", "v the question waits below");

/// The rows the carried question's marker takes: its row and one of air.
const MARKER_ROWS: usize = 2;

/// The carried question's one quiet row, cut to `width` cells.
fn carried_line(width: u16, color: bool, ascii: bool) -> Line<'static> {
    let (text, cut) = if ascii {
        (CARRIED.1, "...")
    } else {
        (CARRIED.0, "…")
    };
    let text = fit_head(text, usize::from(width), cut);
    Line::styled(text, role::style(Role::Dim, color))
}

/// A summary under the block's own glyph and tone: the cause first, then the
/// scope, the way on (strong) and where the details are read (dim), each
/// aligned under the cause.
fn summary_lines(kind: Kind, summary: &Summary, color: bool, ascii: bool) -> Vec<Line<'static>> {
    let mut lines = block_lines(&Committed::new(kind, summary.cause), color, ascii);
    let indent = (lines.first())
        .and_then(|line| line.spans.first())
        .map_or(0, Span::width);
    for (text, tone) in [
        (summary.scope, None),
        (summary.next, Some(Role::Strong)),
        (summary.details, Some(Role::Dim)),
    ] {
        let style = tone.map_or_else(Style::default, |tone| role::style(tone, color));
        lines.push(Line::from(vec![
            Span::raw(" ".repeat(indent)),
            Span::styled(text, style),
        ]));
    }
    lines
}

/// The Session's line of the check's hints, its count and the command that prints them in a
/// terminal, in each language it says it, and how the workspace says where they are shown: the
/// check face of the workflow in view, one key away (live gauntlet 6, r04 and r08 B9: « 2
/// conseils pour l'améliorer · `nika-dev check x.nika` les affiche » sent a person who never
/// opened a terminal to one).
const HINTS: [(&str, &str, &str); 4] = [
    (
        " hints to improve it",
        "` prints them",
        "the check face shows them · F6 then Left/Right",
    ),
    (
        " hint to improve it",
        "` prints it",
        "the check face shows it · F6 then Left/Right",
    ),
    (
        " conseils pour l'améliorer",
        "` les affiche",
        "la face vérification les affiche · F6 puis Gauche/Droite",
    ),
    (
        " conseil pour l'améliorer",
        "` l'affiche",
        "la face vérification l'affiche · F6 puis Gauche/Droite",
    ),
];

/// `text` as the workspace reads it when a line of it is the Session's line of the check's
/// hints ([`HINTS`]): the count kept, the terminal's command replaced by the key that shows
/// them on the check face, in the line's own language; `None` when no line is. The block keeps
/// the Session's words, which its whole words (`F2`) read.
fn hints_here(text: &str) -> Option<String> {
    let pointed = |line: &str| {
        let (head, command) = line.split_once(" · `")?;
        HINTS.iter().find_map(|(phrase, tail, here)| {
            (head.ends_with(phrase) && command.ends_with(tail) && command.contains(" check "))
                .then(|| format!("{head} · {here}"))
        })
    };
    if !text.lines().any(|line| pointed(line).is_some()) {
        return None;
    }
    let lines: Vec<String> = (text.lines())
        .map(|line| pointed(line).unwrap_or_else(|| line.to_owned()))
        .collect();
    Some(lines.join("\n"))
}

/// How a line said beside a waiting proposal begins: « (the proposal still waits · `yes`
/// applies it · `no` discards it) ».
const STILL_WAITS: &str = "(the proposal still waits";

/// Whether the Session waits on a proposal now: its consent, its save decision or one of its
/// open choices.
fn proposal_waits(waiting: &Waiting) -> bool {
    matches!(waiting, Waiting::Proposal | Waiting::ProposalChoice { .. })
        || matches!(waiting, Waiting::Action { key, .. } if key == "save")
}

/// A proposal block said beside a proposal that waited, as its card reads it once no proposal
/// waits: without the line saying it still waits, a decision since taken. `None` keeps the
/// block as said; the block itself keeps every word, and the reader (`F2`) reads them. The
/// focus view reads its blocks the same way.
pub(crate) fn decided(block: &Committed, waiting: &Waiting) -> Option<Committed> {
    if block.kind != Kind::Proposal || proposal_waits(waiting) || !block.text.contains(STILL_WAITS)
    {
        return None;
    }
    let kept: Vec<&str> = (block.text.lines())
        .filter(|line| !line.trim_start().starts_with(STILL_WAITS))
        .collect();
    Some(Committed::new(block.kind, kept.join("\n")))
}

/// The lines `block`, at `index` in the transcript, paints `width` cells
/// wide: the candidate's review for the current proposal (`current`), its
/// identity and reader key on a row of their own where no border carries them
/// (`bare`), else the card's own lines (a proposal block that said it still
/// waits, without that line once no proposal waits).
fn shown_lines(
    state: &UiState,
    (index, block): (usize, &Committed),
    current: Option<(usize, &Review)>,
    width: u16,
    bare: bool,
) -> Vec<Line<'static>> {
    let (color, ascii) = (state.color, state.ascii);
    match current {
        Some((at, review)) if at == index => {
            let mut lines = review.lines(color, ascii, width);
            if bare {
                let foot = review.foot(usize::from(width), ascii);
                lines.push(Line::styled(foot, role::style(Role::Dim, color)));
            }
            lines
        }
        _ => match decided(block, &state.waiting) {
            Some(now) => card_lines(&now, color, ascii, width),
            None => card_lines(block, color, ascii, width),
        },
    }
}

/// What the cards read beside the transcript: the candidate the current
/// proposal is reviewed against, and the block the live question card carries.
pub(crate) type Context<'a> = (Option<&'a Review>, Option<usize>);

/// Every block's lines from block `from` in a pane `width` cells wide too
/// small for cards, the current proposal as its review and the carried
/// question as its row: what the compact fallback paints and measures alike.
fn compact_lines(
    state: &UiState,
    context: Context<'_>,
    width: u16,
    from: usize,
) -> Vec<Line<'static>> {
    let (review, carried) = context;
    let current = review::summarized(state, review);
    (state.transcript.iter().enumerate().skip(from))
        .flat_map(|(index, block)| {
            if carried == Some(index) {
                vec![carried_line(width, state.color, state.ascii)]
            } else {
                shown_lines(state, (index, block), current, width, true)
            }
        })
        .collect()
}

/// A decision or a refusal in its bounded card, as both measured and painted.
struct Card {
    /// Its title and the role its title and edges wear.
    heading: (&'static str, Role),
    /// The lines it paints, and their rows inside the card.
    lines: Vec<Line<'static>>,
    rows: usize,
    /// The current proposal's identity and reader key, for its bottom border.
    foot: Option<String>,
}

/// One piece of the conversation, as both measured and painted.
enum Piece {
    /// A decision or a refusal in its bounded card.
    Card(Card),
    /// One speaker's consecutive blocks.
    Bubble(Bubble),
    /// The question the live card carries: one quiet row where it stood.
    Marker,
}

impl Piece {
    /// Every row it takes, its row of air included.
    fn rows(&self) -> usize {
        match self {
            Self::Card(card) => card_height(card.rows),
            Self::Bubble(bubble) => bubble.rows(),
            Self::Marker => MARKER_ROWS,
        }
    }
}

/// The conversation in `area` from block `from` on, piece by piece: the one
/// plan [`rows_from`], [`height`] and [`render`] read. Consecutive blocks of
/// one speaker share a bubble; a decision or a refusal stands alone in its
/// card (so the pieces from one are those of the whole plan), the current
/// proposal's card reads as the candidate's `review` ([`review::summarized`])
/// wrapped to the card's words, its identity on its bottom border, and the
/// carried block as the live question's one row.
fn plan(state: &UiState, area: Rect, (review, carried): Context<'_>, from: usize) -> Vec<Piece> {
    let form = Form::of(area);
    let (color, ascii, voice) = (state.color, state.ascii, state.voice());
    let current = review::summarized(state, review);
    let pending = review::pending(state);
    let words = area.width.saturating_sub(4);
    let mut pieces = Vec::new();
    let mut group: Vec<&Committed> = Vec::new();
    for (index, block) in state.transcript.iter().enumerate().skip(from) {
        let bound = framed(block.kind);
        let you = block.kind == Kind::Human;
        if group
            .first()
            .is_some_and(|first| bound || (first.kind == Kind::Human) != you)
        {
            pieces.push(Piece::Bubble(Bubble::of(&group, area, form, color, voice)));
            group.clear();
        }
        if bound && carried == Some(index) {
            pieces.push(Piece::Marker);
        } else if bound {
            let lines = shown_lines(state, (index, block), current, words, false);
            let reviewed = current.filter(|(at, _)| *at == index);
            let room = usize::from(area.width).saturating_sub(5);
            pieces.push(Piece::Card(Card {
                heading: card_heading(block, pending == Some(index), voice),
                rows: content_rows(&lines, words),
                lines,
                foot: reviewed.map(|(_, review)| review.foot(room, ascii)),
            }));
        } else {
            group.push(block);
        }
    }
    if !group.is_empty() {
        pieces.push(Piece::Bubble(Bubble::of(&group, area, form, color, voice)));
    }
    pieces
}

/// Paint the visible end of the conversation without allocating off-screen
/// cells, read with `context` (`(None, None)` keeps every block as said). A
/// plan shorter than `area` at its live position stands on the area's last
/// row ([`lift`]): its latest piece touches the decision under it.
pub(crate) fn render(frame: &mut Frame<'_>, state: &UiState, area: Rect, context: Context<'_>) {
    if area.is_empty() {
        return;
    }
    if !room_for_labels(area) {
        compact(frame, state, area, context);
        return;
    }
    let pieces = plan(state, area, context, 0);
    let total: usize = pieces.iter().map(Piece::rows).sum();
    let mut skip = total
        .saturating_sub(usize::from(area.height))
        .saturating_sub(state.focus_scroll);
    let mut y = area.y + lift(state, area, total);
    for piece in &pieces {
        let height = piece.rows();
        if skip >= height {
            skip -= height;
            continue;
        }
        let visible = height
            .saturating_sub(skip)
            .min(usize::from(area.bottom() - y));
        let visible = u16::try_from(visible).unwrap_or(area.height);
        let at = Rect::new(area.x, y, area.width, visible);
        match piece {
            Piece::Card(card) => paint_card(frame, card, at, skip, state),
            Piece::Bubble(bubble) => {
                bubble.paint(frame.buffer_mut(), at, skip, state.color, state.ascii);
            }
            Piece::Marker if skip == 0 => {
                let line = carried_line(at.width, state.color, state.ascii);
                frame.render_widget(Paragraph::new(line), Rect::new(at.x, at.y, at.width, 1));
            }
            // Scrolled past its row, only its air remains.
            Piece::Marker => {}
        }
        y += visible;
        skip = 0;
        if y == area.bottom() {
            break;
        }
    }
}

/// The blocks the conversation painted in `area` with `context` shows on its
/// rows `rows` (from the area's top), by index, the newest first: the plan
/// painting reads, each block in the rows of its piece ([`Bubble::spans`]),
/// so the reading position and a press name one block. A pane too small for
/// labels paints no piece and names none.
pub(crate) fn shown_at(
    state: &UiState,
    area: Rect,
    context: Context<'_>,
    rows: Range<usize>,
) -> Vec<usize> {
    if !room_for_labels(area) {
        return Vec::new();
    }
    let pieces = plan(state, area, context, 0);
    let total: usize = pieces.iter().map(Piece::rows).sum();
    let skip = total
        .saturating_sub(usize::from(area.height))
        .saturating_sub(state.focus_scroll);
    let (from, to) = (rows.start + skip, rows.end + skip);
    // Each block's first row and rows, counted as painted: the lift first.
    let mut at = usize::from(lift(state, area, total));
    let mut spans = Vec::new();
    for piece in &pieces {
        match piece {
            Piece::Bubble(bubble) => spans.extend(bubble.spans().map(|(top, n)| (at + top, n))),
            _ => spans.push((at, piece.rows())),
        }
        at += piece.rows();
    }
    let shown = spans
        .iter()
        .enumerate()
        .filter(|(_, (top, n))| *top < to && top + n > from);
    shown.map(|(block, _)| block).rev().collect()
}

/// Total rendered rows, from the same plan, `context` and compact fallback as
/// painting.
pub(crate) fn height(state: &UiState, area: Rect, context: Context<'_>) -> usize {
    rows_from(state, area, context, 0)
}

/// The rendered rows from block `from` to the end, from the same plan,
/// `context` and compact fallback as painting: what a decision standing at
/// `from` asks of the transcript.
pub(crate) fn rows_from(state: &UiState, area: Rect, context: Context<'_>, from: usize) -> usize {
    if !room_for_labels(area) {
        return content_rows(&compact_lines(state, context, area.width, from), area.width);
    }
    plan(state, area, context, from)
        .iter()
        .map(Piece::rows)
        .sum()
}

/// Very small viewports spend their cells on message content first.
fn room_for_labels(area: Rect) -> bool {
    area.height >= 6 && area.width >= 6
}

/// In a short split, every row belongs to the question or answer, not chrome.
fn compact(frame: &mut Frame<'_>, state: &UiState, area: Rect, context: Context<'_>) {
    let lines = compact_lines(state, context, area.width, 0);
    let rows = content_rows(&lines, area.width);
    let skip = rows
        .saturating_sub(usize::from(area.height))
        .saturating_sub(state.focus_scroll);
    let above = lift(state, area, rows);
    let at = Rect::new(area.x, area.y + above, area.width, area.height - above);
    window(&lines, at, skip, frame.buffer_mut());
}

/// The rows above a plan `rows` long in `area` at its live position: what the
/// area holds beyond it, so its last row stands on the area's last row and
/// the empty rows go above it. Scrolled back, or longer than the area, none:
/// manual reading, the scroll bounds and every offset stay as measured.
fn lift(state: &UiState, area: Rect, rows: usize) -> u16 {
    if state.focus_scroll > 0 {
        return 0;
    }
    let short = usize::from(area.height).saturating_sub(rows);
    u16::try_from(short).unwrap_or(0)
}

/// The heading of a card entered part-way, `above` rows of its words over the
/// first one shown, in `room` cells: its label, cut if need be, and the count
/// whole; `None` when the row cannot hold both.
fn continued(label: &str, above: usize, room: usize, voice: Voice) -> Option<String> {
    let (sep, up, cut) = if voice.ascii {
        ("-", "^", "...")
    } else {
        ("·", "↑", "…")
    };
    let count = format!(" {sep} {up} {}", voice.count(above, "row"));
    let name = room
        .checked_sub(count.width())
        .filter(|cells| *cells > cut.width())?;
    Some(format!("{}{count}", fit_head(label, name, cut)))
}

/// A decision or a refusal on its bounded raised surface: the title in the
/// top border, the words one cell inside each edge, the bottom border (the
/// current proposal's identity on it, [`bottom_border`]), from its row
/// `offset` on. Entered part-way, its first row shown names it and counts the
/// rows of words above ([`continued`]), the words one row lower: no row's
/// cost changes, and one row back shows the words that row covers.
fn paint_card(frame: &mut Frame<'_>, card: &Card, area: Rect, offset: usize, state: &UiState) {
    let (lines, rows) = (&card.lines, card.rows);
    let height = usize::from(area.height);
    let painted = height.min(rows.saturating_add(2).saturating_sub(offset));
    let painted = u16::try_from(painted).unwrap_or(area.height);
    frame.buffer_mut().set_style(
        Rect::new(area.x, area.y, area.width, painted),
        role::surface(state.color, true),
    );
    let (label, tone) = card.heading;
    let style = role::style(tone, state.color);
    let (top, top_end, edge, rule, cut) = if state.ascii {
        ("+-", "+", "|", "-", "...")
    } else {
        ("╭─", "╮", "│", "─", "…")
    };
    let room = usize::from(area.width).saturating_sub(5);
    let entered = if offset > 0 && offset <= rows && height >= 2 {
        continued(label, offset, room, state.voice())
    } else {
        None
    };
    let first = offset.max(1) + usize::from(entered.is_some());
    let label = if offset == 0 {
        Some(fit_head(label, room, cut))
    } else {
        entered
    };
    if let Some(label) = label {
        let title = format!(
            "{top} {label} {}{top_end}",
            rule.repeat(usize::from(area.width).saturating_sub(label.width() + 5))
        );
        frame.render_widget(
            Paragraph::new(Line::styled(title, style)),
            Rect::new(area.x, area.y, area.width, 1),
        );
    }
    let last = offset.saturating_add(height).min(rows.saturating_add(1));
    if last > first {
        let body = Rect::new(
            area.x + 2,
            area.y + u16::try_from(first - offset).unwrap_or(area.height),
            area.width.saturating_sub(4),
            u16::try_from(last - first).unwrap_or(area.height),
        );
        window(lines, body, first - 1, frame.buffer_mut());
        for y in body.y..body.bottom() {
            frame.buffer_mut().set_string(area.x, y, edge, style);
            frame
                .buffer_mut()
                .set_string(area.right() - 1, y, edge, style);
        }
    }
    let foot = rows.saturating_add(1);
    if foot >= offset && foot < offset.saturating_add(height) {
        let line = bottom_border(card.foot.as_deref(), area.width, style, state);
        frame.render_widget(
            Paragraph::new(line),
            Rect::new(
                area.x,
                area.y + u16::try_from(foot - offset).unwrap_or(area.height),
                area.width,
                1,
            ),
        );
    }
}

/// A card's bottom border `width` cells wide, its edges in `style`: the
/// current proposal's identity and reader key (`foot`, quiet) one cell in
/// where they fit whole, the plain rule otherwise.
fn bottom_border(foot: Option<&str>, width: u16, style: Style, state: &UiState) -> Line<'static> {
    let (bottom, end, rule) = if state.ascii {
        ("+", "+", "-")
    } else {
        ("╰", "╯", "─")
    };
    let width = usize::from(width);
    match foot.filter(|words| words.width() + 5 <= width) {
        Some(foot) => Line::from(vec![
            Span::styled(format!("{bottom}{rule} "), style),
            Span::styled(foot.to_owned(), role::style(Role::Dim, state.color)),
            Span::styled(
                format!(" {}{end}", rule.repeat(width - foot.width() - 5)),
                style,
            ),
        ]),
        None => Line::styled(
            format!("{bottom}{}{end}", rule.repeat(width.saturating_sub(2))),
            style,
        ),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests;
