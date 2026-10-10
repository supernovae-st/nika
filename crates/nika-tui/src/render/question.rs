// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The card of a typed question that offers nothing (a text, an exact value)
//! in the live area, in the flow directly above the line that answers it; a
//! typed choice opens the surface above the line instead (`super::surface`),
//! which reads the words and the cap from here. In the workspace, while the
//! latest question block carries the witness of the typed question waiting
//! ([`asked_block`]), the card is the question's live home: an accent title,
//! quiet facts (the shape of the answer, whether it is required), then the
//! Session's exact words (wrapped by cells under their own indent, every row
//! the frame's cap holds; a row that says where the whole question is read,
//! `F2`, only where at least two rows stay unread); the transcript reads one
//! quiet row in their place ([`carried`]). Anywhere else, while a turn works
//! or with too few rows, the card keeps its header and the transcript every
//! word. A surface open above the line hides the card and moves none of its
//! rows. Painting and measuring read one geometry: the live area's rows
//! (`live_areas`) and [`Share`].

use std::fmt::Write as _;

use nika_display::theme::Role;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::chooser::fit;
use super::{accent, own};
use crate::composer::Composer;
use crate::model::{Asked, Kind, Presentation, Shape, UiState, Waiting};
use crate::visual::role;
use crate::workspace::cards::diagnostics::{Shown, shown};

/// The fewest rows of the Session's words the card may show ([`word_cap`]):
/// past its cap the whole question is read in the reader (`F2`), never
/// pushed into the live chrome.
pub(crate) const WORD_ROWS: usize = 6;

/// The rows of the Session's words the card may show on `state`'s frame:
/// [`WORD_ROWS`], or a quarter of a taller frame's rows, so the words a tall
/// frame has room for are read where they are answered. One cap for the
/// card, the typed choice's surface and a decision's demand.
pub(crate) fn word_cap(state: &UiState) -> usize {
    WORD_ROWS.max(usize::from(state.size.1) / 4)
}

/// The last word row when the words do not all show: where they are read.
pub(crate) const WHOLE: &str = "… the whole question: F2";

/// The typed question the card paints: the one [`waiting`], while no turn
/// works, whatever a surface shows over it.
fn painted(state: &UiState) -> Option<&Asked> {
    if state.busy.is_some() {
        return None;
    }
    waiting(state)
}

/// The typed question waiting, when it has a card: a text or an exact value.
/// A typed choice opens the surface instead, and one that offers nothing has
/// neither.
fn waiting(state: &UiState) -> Option<&Asked> {
    let Waiting::QuestionDocument { asked, .. } = &state.waiting else {
        return None;
    };
    matches!(asked.shape, Shape::Text | Shape::Literal).then_some(asked)
}

/// The transcript block tied to the typed question waiting, by index: in the
/// fitting workspace, the latest question block when it carries that question's
/// witness and no summarized block (whose details `F2` reads) follows it. It
/// does not depend on what the live area shows: the reader opens it wherever.
pub(crate) fn asked_block(state: &UiState) -> Option<usize> {
    if state.presentation != Presentation::Workspace
        || !crate::workspace::geometry::fits(state.size)
    {
        return None;
    }
    let Waiting::QuestionDocument { asked, .. } = &state.waiting else {
        return None;
    };
    let (index, block) = (state.transcript.iter().enumerate())
        .rev()
        .find(|(_, block)| block.kind == Kind::Question)?;
    let tied = block.question_witness() == Some(asked.witness.as_str());
    let later =
        (state.transcript.iter().skip(index + 1)).any(|block| !matches!(shown(block), Shown::Said));
    (tied && !later).then_some(index)
}

/// Whether the card stands as the question's live home: a typed question tied
/// to its block is painted in the workspace. The boxed composer then needs no
/// caption: the card names the question right above its line.
pub(crate) fn homed(state: &UiState) -> bool {
    painted(state).is_some() && asked_block(state).is_some()
}

/// How the card's words end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Whole {
    /// Every word shows.
    Shown,
    /// A row of its own says where the whole question is read.
    Row,
    /// Too few rows for that row: the one word row shown ends with where the
    /// whole question is read.
    Inline,
}

/// How a card shares its rows under its title: the word rows it shows and
/// how those words end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Share {
    pub(crate) words: usize,
    pub(crate) whole: Whole,
}

impl Share {
    /// The rows the Session's words ask for when they wrap to `words` rows
    /// under a cap of `cap` rows: every row while at most one passes the cap
    /// (the row that says where the rest is read would take that very row),
    /// else `cap` rows and that row.
    pub(crate) fn asks(words: usize, cap: usize) -> usize {
        words.min(cap + 1)
    }

    /// The share of `rows` rows for words that wrap to `words` rows under a
    /// cap of `cap` word rows: under the title, the first word row always,
    /// then the words what is left. `None` when even the first rows do not
    /// fit: the card cannot carry the words.
    pub(crate) fn of(rows: usize, words: usize, cap: usize) -> Option<Self> {
        if words == 0 {
            return None;
        }
        let left = rows.checked_sub(1)?;
        if left < 1 {
            return None;
        }
        let (words, whole) = if left >= Self::asks(words, cap) {
            if words <= cap + 1 {
                (words, Whole::Shown)
            } else {
                (cap, Whole::Row)
            }
        } else if left >= 2 {
            (left - 1, Whole::Row)
        } else {
            (1, Whole::Inline)
        };
        Some(Self { words, whole })
    }
}

/// Where a card's rows go: the word rows it paints (none unless it carries
/// the Session's words) and how they end.
struct Rows {
    words: Vec<String>,
    whole: Whole,
    home: bool,
}

/// The rows the card paints in `card`: the home's when it carries the
/// Session's words there, else the header alone.
fn layout(state: &UiState, card: Rect) -> Rows {
    let header_only = Rows {
        words: Vec::new(),
        whole: Whole::Shown,
        home: false,
    };
    let Some(block) = asked_block(state).and_then(|index| state.transcript.get(index)) else {
        return header_only;
    };
    let mut words = hang(&block.text, usize::from(card.width));
    let height = usize::from(card.height);
    let cap = word_cap(state);
    let Some(share) = Share::of(height, words.len(), cap) else {
        return header_only;
    };
    words.truncate(share.words);
    Rows {
        words,
        whole: share.whole,
        home: true,
    }
}

/// The one word row of a card too short for a row of its own to say where
/// the whole question is read: that row, cut to make room, then where.
fn inline(row: &str, width: usize, (ascii, color): (bool, bool)) -> Line<'static> {
    let (cue, mark) = (own(" … F2", ascii), if ascii { "..." } else { "…" });
    let room = width.saturating_sub(cue.width());
    let fitted = fit(row, room, ascii);
    let kept = if row.width() > room {
        fitted.strip_suffix(mark).unwrap_or(&fitted)
    } else {
        row
    };
    let cue = Span::styled(cue, role::style(Role::Dim, color));
    Line::from(vec![Span::raw(kept.trim_end().to_owned()), cue])
}

/// The transcript block the live card carries on a live area `live` painted
/// `boxed` or not, read as one quiet row while the card paints its words: the
/// one projection painting and the scroll bounds read.
pub(crate) fn carried(
    state: &UiState,
    composer: &Composer,
    live: Rect,
    boxed: bool,
) -> Option<usize> {
    painted(state)?;
    let card = super::live_areas(state, composer, live, boxed).card;
    if layout(state, card).home {
        asked_block(state)
    } else {
        None
    }
}

/// The rows the card asks for on a live area `width` wide: its title and the
/// Session's words when it may carry them (a bounded prefix).
pub(crate) fn rows(state: &UiState, width: u16) -> u16 {
    painted(state).map_or(0, |_| card_rows(state, width))
}

/// [`rows`] at rest, whatever a turn works or the chooser lists: what a
/// decision's demand reads.
pub(crate) fn rest_rows(state: &UiState, width: u16) -> u16 {
    waiting(state).map_or(0, |_| card_rows(state, width))
}

/// [`homed`] at rest, whatever a turn works or the chooser lists: the card
/// the live area paints once nothing works or lists is the question's live
/// home. A decision's demand reads it.
pub(crate) fn rest_homed(state: &UiState) -> bool {
    waiting(state).is_some() && asked_block(state).is_some()
}

/// The rows the card asks for at `width` ([`rows`]).
fn card_rows(state: &UiState, width: u16) -> u16 {
    let cap = word_cap(state);
    let words = (asked_block(state))
        .and_then(|index| state.transcript.get(index))
        .map_or(0, |block| {
            Share::asks(hang(&block.text, usize::from(width)).len(), cap)
        });
    u16::try_from(1 + words).unwrap_or(u16::MAX)
}

/// Paint the card into `area`: the header row, then the Session's words
/// when the card carries them.
pub(crate) fn render(state: &UiState, area: Rect, buf: &mut Buffer) {
    let Some(asked) = painted(state) else {
        return;
    };
    if area.height == 0 || area.width == 0 {
        return;
    }
    let (ascii, color) = (state.ascii, state.color);
    let width = usize::from(area.width);
    let rows = layout(state, area);
    let line = header(asked, width, rows.home, (ascii, color));
    buf.set_line(area.x, area.y, &line, area.width);
    let mut y = area.y;
    let last = rows.words.len().saturating_sub(1);
    for (at, words) in rows.words.iter().enumerate() {
        y += 1;
        let line = if rows.whole == Whole::Inline && at == last {
            inline(words, width, (ascii, color))
        } else {
            Line::raw(words.clone())
        };
        buf.set_line(area.x, y, &line, area.width);
    }
    if rows.whole == Whole::Row {
        y += 1;
        let whole = fit(&own(WHOLE, ascii), width, ascii);
        let whole = Line::styled(whole, role::style(Role::Dim, color));
        buf.set_line(area.x, y, &whole, area.width);
    }
}

/// What the header says: the question in the accent and the shape of the
/// answer when the card carries the Session's words (`home`), else the shape
/// alone; and whether it is required.
fn header(asked: &Asked, width: usize, home: bool, (ascii, color): (bool, bool)) -> Line<'static> {
    let shape = match &asked.shape {
        Shape::Choice(_) => "Offered answers",
        Shape::Text => "A text answer",
        Shape::Literal => "An exact value",
    };
    let (title, style) = if home {
        ("Question", accent(color))
    } else {
        (shape, role::style(Role::Strong, color))
    };
    if title.width() >= width {
        return Line::styled(fit(title, width, ascii), style);
    }
    let mut facts = String::new();
    if home {
        let _ = write!(facts, " · {shape}");
    }
    if asked.mandatory {
        facts.push_str(" · required");
    }
    let facts = own(&facts, ascii);
    let room = width - title.width();
    let facts = if facts.width() <= room {
        facts
    } else if room > 3 {
        fit(&facts, room, ascii)
    } else {
        String::new()
    };
    Line::from(vec![
        Span::styled(title, style),
        Span::styled(facts, role::style(Role::Dim, color)),
    ])
}

/// The Session's words in rows of at most `width` cells, every character
/// kept: each line breaks at its spaces (a break takes the space it falls
/// on), a word wider than a row continues on the next, and every continuation
/// hangs under its line's own indent while that indent leaves half the row.
pub(crate) fn hang(text: &str, width: usize) -> Vec<String> {
    let mut rows = Vec::new();
    if width == 0 {
        return rows;
    }
    for line in text.lines() {
        let body = line.trim_start();
        let indent = &line[..line.len() - body.len()];
        let indent = if indent.width() * 2 <= width {
            indent
        } else {
            ""
        };
        let mut row = indent.to_owned();
        let mut used = indent.width();
        let mut fresh = true;
        for (gap, word) in words(body) {
            if !fresh && used + gap.width() + word.width() > width {
                rows.push(std::mem::replace(&mut row, indent.to_owned()));
                used = indent.width();
                fresh = true;
            }
            if !fresh {
                row.push_str(gap);
                used += gap.width();
            }
            for glyph in word.chars() {
                let cells = glyph.width().unwrap_or(0);
                if used + cells > width && used > indent.width() {
                    rows.push(std::mem::replace(&mut row, indent.to_owned()));
                    used = indent.width();
                }
                row.push(glyph);
                used += cells;
            }
            fresh = false;
        }
        rows.push(row);
    }
    rows
}

/// The words of `text`, each with the spaces before it, exactly; spaces after
/// the last word end nothing.
fn words(text: &str) -> impl Iterator<Item = (&str, &str)> {
    let mut rest = text;
    std::iter::from_fn(move || {
        let start = rest.find(|c: char| !c.is_whitespace())?;
        let (gap, after) = rest.split_at(start);
        let end = after.find(char::is_whitespace).unwrap_or(after.len());
        let (word, tail) = after.split_at(end);
        rest = tail;
        Some((gap, word))
    })
}
