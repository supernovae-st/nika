// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One speaker's bubble: consecutive ordinary blocks of one side under one
//! label. The human's lines stand at the right end on the raised surface,
//! Nika's at the left on the ground. Inside a bubble a quiet sublabel names a
//! change to a kind with a name of its own (a run, a result, a report,
//! activity), a run's consecutive task lines share one, and a row of air
//! separates two messages of one kind.
//!
//! The transcript's rectangle alone chooses the form: a short one keeps slabs
//! (the label row, the lines, a row of air), a tall one draws quiet rounded
//! outlines with the label set in the top edge and a cell of air inside each
//! edge. A bubble is as wide as its widest row within what the row allows,
//! measured in cells; from [`RESERVE_FROM`] columns it leaves part of the row
//! to the other side. The same rows measure and paint, every block's words
//! are painted as said, nothing moves, no hue is painted without colour and
//! the ASCII column draws ASCII edges.

use nika_display::theme::Role;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::{heading, message_lines};
use crate::model::{Committed, Kind};
use crate::render::{content_rows, window};
use crate::visual::role;
use crate::workspace::text::{fit_head, marks};

/// The transcript rows from which bubbles stand in quiet outlines. A shorter
/// transcript keeps slabs: their label row and row of air cost no more than
/// an outline's edges, and the words keep the edges' columns.
const OUTLINED_ROWS: u16 = 16;

/// The narrowest transcript that draws outlines: inside the edges and their
/// air the words keep twenty columns.
const OUTLINED_WIDTH: u16 = 24;

/// From this width a bubble leaves an eighth of the row, at most
/// [`RESERVE_MAX`] columns, to the other side, so the speaker's side reads at
/// a glance; a narrower row gives its words every column.
const RESERVE_FROM: u16 = 60;

/// The most columns a bubble leaves to the other side.
const RESERVE_MAX: u16 = 12;

/// How the transcript's rectangle shows bubbles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Form {
    /// The label row, the lines and a row of air.
    Slab,
    /// A quiet rounded outline, the label set in its top edge.
    Outlined,
}

impl Form {
    /// The form of a transcript painted in `area`: never the conversation's
    /// content, so measuring and painting the same rectangle agree.
    pub(super) fn of(area: Rect) -> Self {
        if area.height >= OUTLINED_ROWS && area.width >= OUTLINED_WIDTH {
            Self::Outlined
        } else {
            Self::Slab
        }
    }
}

/// What separates a block from the one before it inside a bubble.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lead {
    /// Nothing: the bubble's first block, or a run's next task line.
    Tight,
    /// A row of air between two messages.
    Air,
    /// The quiet name of the kind it changes to.
    Sublabel(&'static str, Role),
}

impl Lead {
    /// The rows it takes.
    fn rows(self) -> usize {
        usize::from(self != Self::Tight)
    }

    /// The columns its words take.
    fn width(self) -> usize {
        match self {
            Self::Sublabel(words, _) => words.width(),
            Self::Tight | Self::Air => 0,
        }
    }
}

/// One block inside a bubble: what separates it from the one before, its
/// lines and their rows at the bubble's text width.
struct Part {
    lead: Lead,
    lines: Vec<Line<'static>>,
    rows: usize,
}

/// A speaker's consecutive blocks, placed: everything both its measure and
/// its paint read.
pub(super) struct Bubble {
    /// The human's bubble (at the right end, raised) rather than Nika's.
    you: bool,
    form: Form,
    /// The speaker label; none when the opening banner, which names itself,
    /// leads.
    label: Option<Line<'static>>,
    parts: Vec<Part>,
    /// The bubble's first column and its width.
    x: u16,
    width: u16,
    /// The first column and the width of its words.
    text_x: u16,
    text_width: u16,
}

impl Bubble {
    /// The bubble of `blocks` (consecutive, one side, at least one) in a
    /// transcript painted in `area` as `form`.
    pub(super) fn of(
        blocks: &[&Committed],
        area: Rect,
        form: Form,
        color: bool,
        ascii: bool,
    ) -> Self {
        let you = blocks
            .first()
            .is_some_and(|block| block.kind == Kind::Human);
        let label = blocks
            .first()
            .and_then(|block| label(block.kind, you, ascii, color));
        let mut said = Vec::with_capacity(blocks.len());
        let mut was = None;
        for block in blocks {
            let lead = was.map_or(Lead::Tight, |previous| lead(previous, block.kind, you));
            said.push((lead, message_lines(block, color, ascii)));
            was = Some(block.kind);
        }
        let widest = said
            .iter()
            .flat_map(|(lead, lines)| lines.iter().map(Line::width).chain([lead.width()]))
            .max()
            .unwrap_or(0);
        let least = label.as_ref().map_or(0, Line::width);
        let (x, width, text_x, text_width) = columns(area, form, you, widest, least);
        let parts = said
            .into_iter()
            .map(|(lead, lines)| {
                let rows = content_rows(&lines, text_width);
                Part { lead, lines, rows }
            })
            .collect();
        Self {
            you,
            form,
            label,
            parts,
            x,
            width,
            text_x,
            text_width,
        }
    }

    /// The rows above its words: a slab's label row, an outline's top edge;
    /// none for a slab the banner leads.
    fn head(&self) -> usize {
        usize::from(self.form == Form::Outlined || self.label.is_some())
    }

    /// The rows of its words, their sublabels and air included.
    fn body(&self) -> usize {
        self.parts
            .iter()
            .map(|part| part.lead.rows() + part.rows)
            .sum()
    }

    /// The rows below its words: an outline's bottom edge.
    fn foot(&self) -> usize {
        usize::from(self.form == Form::Outlined)
    }

    /// Each block's first row in the bubble and its rows, its sublabel or
    /// air included, in order: where the reading position or a press finds it.
    pub(super) fn spans(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        let mut at = self.head();
        self.parts.iter().map(move |part| {
            let rows = part.lead.rows() + part.rows;
            at += rows;
            (at - rows, rows)
        })
    }

    /// Every row it takes, the row of air after it included.
    pub(super) fn rows(&self) -> usize {
        self.head() + self.body() + self.foot() + 1
    }

    /// Paint its rows from its row `offset` on into `at`, whose height says
    /// how many show: the raised surface under the human's, the label or the
    /// top edge, each part under its sublabel, an outline's other edges.
    pub(super) fn paint(
        &self,
        buf: &mut Buffer,
        at: Rect,
        offset: usize,
        color: bool,
        ascii: bool,
    ) {
        let height = usize::from(at.height);
        let (head, body) = (self.head(), self.body());
        if self.you
            && let Some((top, _, count)) = shown(0, head + body + self.foot(), offset, height)
        {
            let cells = Rect::new(self.x, at.y + top, self.width, count);
            buf.set_style(cells, role::surface(color, true));
        }
        if head > 0
            && let Some((top, _, _)) = shown(0, 1, offset, height)
        {
            self.paint_head(buf, at.y + top, color, ascii);
        }
        let mut start = head;
        for part in &self.parts {
            if let Lead::Sublabel(words, tone) = part.lead
                && let Some((top, _, _)) = shown(start, 1, offset, height)
            {
                let words = fit_head(words, usize::from(self.text_width), marks(ascii).1);
                buf.set_string(self.text_x, at.y + top, words, role::style(tone, color));
            }
            start += part.lead.rows();
            if let Some((top, skip, count)) = shown(start, part.rows, offset, height) {
                let cells = Rect::new(self.text_x, at.y + top, self.text_width, count);
                window(&part.lines, cells, skip, buf);
            }
            start += part.rows;
        }
        if self.form == Form::Outlined {
            self.paint_edges(buf, at, offset, color, ascii);
        }
    }

    /// A slab's label row (at the right end of the human's words, at the left
    /// of Nika's) or an outline's top edge with the label set in it.
    fn paint_head(&self, buf: &mut Buffer, row: u16, color: bool, ascii: bool) {
        let cut = marks(ascii).1;
        if self.form == Form::Slab {
            if let Some(label) = &self.label {
                let label = fitted(label, usize::from(self.text_width), cut);
                let used = u16::try_from(label.width()).unwrap_or(self.text_width);
                let shift = if self.you {
                    self.text_width.saturating_sub(used)
                } else {
                    0
                };
                buf.set_line(self.text_x + shift, row, &label, self.text_width);
            }
            return;
        }
        let edge = role::border(color);
        let (corner, rule, end) = if ascii {
            ("+", "-", "+")
        } else {
            ("╭", "─", "╮")
        };
        let mut spans = Vec::new();
        if let Some(label) = &self.label {
            let room = usize::from(self.width.saturating_sub(5));
            let label = fitted(label, room, cut);
            let rest = rule.repeat(room.saturating_sub(label.width()));
            spans.push(Span::styled(format!("{corner}{rule} "), edge));
            spans.extend(label.spans);
            spans.push(Span::styled(format!(" {rest}{end}"), edge));
        } else {
            let rest = rule.repeat(usize::from(self.width.saturating_sub(2)));
            spans.push(Span::styled(format!("{corner}{rest}{end}"), edge));
        }
        buf.set_line(self.x, row, &Line::from(spans), self.width);
    }

    /// An outline's side edges beside its words and its bottom edge, where
    /// they show.
    fn paint_edges(&self, buf: &mut Buffer, at: Rect, offset: usize, color: bool, ascii: bool) {
        let height = usize::from(at.height);
        let (head, body) = (self.head(), self.body());
        let edge = role::border(color);
        let (side, corner, rule, end) = if ascii {
            ("|", "+", "-", "+")
        } else {
            ("│", "╰", "─", "╯")
        };
        if let Some((top, _, count)) = shown(head, body, offset, height) {
            for row in at.y + top..at.y + top + count {
                buf.set_string(self.x, row, side, edge);
                buf.set_string(self.x + self.width - 1, row, side, edge);
            }
        }
        if let Some((top, _, _)) = shown(head + body, 1, offset, height) {
            let rest = rule.repeat(usize::from(self.width.saturating_sub(2)));
            buf.set_string(self.x, at.y + top, format!("{corner}{rest}{end}"), edge);
        }
    }
}

/// Where a bubble stands in `area` and where its words go: as wide as its
/// widest row with its edges and air, keeping its label (`least` columns)
/// whole where it can, within the row less the reserve; the human's at the
/// right end, Nika's at the left. A slab of Nika's has no surface of its own:
/// its words take the whole row.
fn columns(area: Rect, form: Form, you: bool, widest: usize, least: usize) -> (u16, u16, u16, u16) {
    let (inset, labelled): (u16, usize) = match (form, you) {
        (Form::Outlined, _) => (4, least + 5),
        (Form::Slab, true) => (2, least + 2),
        (Form::Slab, false) => return (area.x, area.width, area.x, area.width),
    };
    let reserve = if area.width >= RESERVE_FROM {
        (area.width / 8).min(RESERVE_MAX)
    } else {
        0
    };
    let most = usize::from(area.width - reserve);
    let wanted = widest.saturating_add(usize::from(inset)).max(labelled);
    let width = u16::try_from(wanted.min(most)).unwrap_or(area.width);
    let x = if you { area.right() - width } else { area.x };
    (x, width, x + inset / 2, width.saturating_sub(inset))
}

/// The label of a bubble whose first block is of kind `first`: the speaker
/// (`You`, `Nika`) and, when that block is not plain speech, its kind
/// (`Nika · Run`); none when the opening banner leads, as it names itself.
fn label(first: Kind, you: bool, ascii: bool, color: bool) -> Option<Line<'static>> {
    if first == Kind::Banner {
        return None;
    }
    let (speaker, tone) = speaker(you);
    let (kind, kind_tone) = heading(first);
    let mut spans = vec![Span::styled(speaker, role::style(tone, color))];
    if kind != speaker {
        spans.push(Span::styled(marks(ascii).0, role::style(Role::Dim, color)));
        spans.push(Span::styled(kind, role::style(kind_tone, color)));
    }
    Some(Line::from(spans))
}

/// The speaker's name and tone: the human's, or Nika's.
fn speaker(you: bool) -> (&'static str, Role) {
    heading(if you { Kind::Human } else { Kind::Reply })
}

/// What separates a block of kind `kind` from the one before it, of kind
/// `was`, inside one bubble: the quiet name of a kind with a name of its own
/// when the kind changes to it, nothing between a run's task lines, otherwise
/// a row of air.
fn lead(was: Kind, kind: Kind, you: bool) -> Lead {
    let (before, _) = heading(was);
    let (name, tone) = heading(kind);
    if name != before && name != speaker(you).0 {
        Lead::Sublabel(name, tone)
    } else if name == before && kind == Kind::Run {
        Lead::Tight
    } else {
        Lead::Air
    }
}

/// `label` within `room` columns: whole, else its speaker alone, cut.
fn fitted(label: &Line<'static>, room: usize, cut: &str) -> Line<'static> {
    if label.width() <= room {
        return label.clone();
    }
    let Some(speaker) = label.spans.first() else {
        return Line::default();
    };
    let words = fit_head(&speaker.content, room, cut);
    Line::from(Span::styled(words, speaker.style))
}

/// The rows of `[start, start + len)` (counted from a piece's first row)
/// that a piece painted from its row `offset` in `height` rows shows: the
/// first of them, counted from the first painted row, how many of the
/// section's rows are skipped above it, and how many show.
fn shown(start: usize, len: usize, offset: usize, height: usize) -> Option<(u16, usize, u16)> {
    let from = start.max(offset);
    let to = start.saturating_add(len).min(offset.saturating_add(height));
    let count = u16::try_from(to.checked_sub(from)?).ok()?;
    let top = u16::try_from(from - offset).ok()?;
    (count > 0).then_some((top, from - start, count))
}
