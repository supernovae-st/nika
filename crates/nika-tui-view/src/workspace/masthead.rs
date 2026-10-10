// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The header's composition: one quiet row with the brand, the project's
//! identity and, one blank cell before its right end, the intelligence
//! selected for preparation, its one home on the screen (the region names
//! follow it while the project list is folded). A two-row header draws a
//! thin rule under that row, or that intelligence whole where the first row
//! cannot hold it. The location, Git and the governing file's facts are
//! `/status`'s; a refused `nika.yaml` stays, it needs action.

use nika_display::theme::Role;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use crate::visual::icon::Icon;
use crate::visual::role;
use crate::workspace::header::{self, Manifest, Place};
use crate::workspace::text::marks;

/// The cells of a name kept at least before the cut mark.
const NAME_KEPT: usize = 4;

/// The blank cells between the place and the intelligence.
const GAP: usize = 2;

/// The blank cell the header's words keep before what ends their row: the
/// terminal's edge, or the region names a folded project list puts there.
const AIR: u16 = 1;

/// The words before the intelligence selected for preparation.
const PREPARE_WITH: &str = "Prepare with: ";

/// What a refused `nika.yaml` keeps on the header.
const REFUSED: &str = "  nika.yaml refused";

/// The command that says the intelligence, the location and the facts whole.
const DETAILS: &str = "/status";

/// The brand opening the header, then its air.
const fn brand(ascii: bool) -> &'static str {
    if ascii { "NIKA  " } else { "◆ NIKA  " }
}

/// The cells of the project's glyph and whole name.
fn named(place: &Place, ascii: bool) -> usize {
    let name = place.project.as_deref().unwrap_or("no project");
    Icon::Project.glyph(ascii).width() + 1 + name.width()
}

/// The fewest cells the place keeps on the header's first row: its glyph
/// and the start of its name with the cut mark (the project list names it
/// whole).
#[must_use]
pub fn least_place(place: &Place, ascii: bool) -> usize {
    let name = place.project.as_deref().unwrap_or("no project");
    let start = name.width().min(NAME_KEPT) + marks(ascii).1.width();
    (Icon::Project.glyph(ascii).width() + 1 + start).min(named(place, ascii))
}

/// The cells a refused `nika.yaml` adds to the place.
fn refusal(place: &Place) -> usize {
    if place.manifest == Some(Manifest::Refused) {
        REFUSED.width()
    } else {
        0
    }
}

/// The place in at most `room` cells: the brand where the glyph and the
/// whole name still fit after it, the project's identity in the header's
/// own words (the name strong, what it is quieter: that gives way first,
/// then the brand, then the end of the name, marked as cut) and a refusal.
fn place_line(place: &Place, room: usize, ascii: bool, color: bool) -> Line<'static> {
    let rest = room.saturating_sub(refusal(place));
    let lead = brand(ascii);
    let branded = rest >= lead.width() + named(place, ascii);
    let kept = rest - if branded { lead.width() } else { 0 };
    let kept = u16::try_from(kept).unwrap_or(u16::MAX);
    let identity = header::lines(place, kept, 2, ascii, color)
        .into_iter()
        .next();
    let mut spans = Vec::new();
    if branded {
        spans.push(Span::styled(lead, role::style(Role::Accent, color)));
    }
    spans.extend(identity.unwrap_or_default().spans);
    if refusal(place) > 0 {
        spans.push(Span::styled(REFUSED, role::style(Role::Warn, color)));
    }
    Line::from(spans)
}

/// The intelligence selected for preparation in at most `room` cells, the
/// longest truthful form first: `Prepare with: <seat>`; under its label, the
/// seat's first whole words cut at their end beside `/status`, which says it
/// whole; `/status` alone. A seat is never cut inside a word nor shown
/// without its label, and it is a fact, not a control: it wears the header's
/// own ink. Nothing selected is said, `not selected`, with `/intelligence` to
/// choose where it fits. `None` where no form fits.
fn intelligence_line(
    seat: Option<&str>,
    room: usize,
    ascii: bool,
    color: bool,
) -> Option<Line<'static>> {
    let (sep, cut) = marks(ascii);
    let quiet = role::style(Role::Dim, color);
    let label = || Span::styled(PREPARE_WITH, quiet);
    let forms = if let Some(seat) = seat {
        let details = || Span::styled(format!("{sep}{DETAILS}"), quiet);
        let tail = sep.width() + DETAILS.len();
        let budget = room.saturating_sub(PREPARE_WITH.len() + tail);
        let mut forms = vec![vec![label(), Span::raw(seat.to_owned())]];
        forms.extend(
            words_head(seat, budget, cut).map(|start| vec![label(), Span::raw(start), details()]),
        );
        forms.push(vec![Span::styled(DETAILS, quiet)]);
        forms
    } else {
        let none = || Span::styled("not selected", role::style(Role::Strong, color));
        let choose = || Span::styled(format!("{sep}/intelligence"), quiet);
        vec![
            vec![label(), none(), choose()],
            vec![none(), choose()],
            vec![none()],
        ]
    };
    forms
        .into_iter()
        .map(Line::from)
        .find(|line| line.width() <= room)
}

/// The start of `seat` in at most `budget` cells, marked as cut with `cut`:
/// its first whole words, without the separator they end on (a comma, a
/// dash); `None` where not even its first word fits.
fn words_head(seat: &str, budget: usize, cut: &str) -> Option<String> {
    let room = budget.checked_sub(cut.width())?;
    let start = seat
        .match_indices(char::is_whitespace)
        .filter_map(|(at, _)| seat.get(..at))
        .map(|words| words.trim_end_matches(|c: char| c.is_whitespace() || ",;:-·".contains(c)))
        .rfind(|words| !words.is_empty() && words.width() <= room)?;
    Some(format!("{start}{cut}"))
}

/// What the header's rows hold: the place, the intelligence after it on the
/// first row, or that intelligence on the second.
struct Composed {
    place: Line<'static>,
    beside: Option<Line<'static>>,
    under: Option<Line<'static>>,
}

/// Compose a header `width` cells wide of `rows` rows whose first row keeps
/// `room` cells before the region names (all of them when none are drawn).
/// The intelligence stands whole beside the place where the glyph and the
/// whole name keep their cells; else whole on a second row; else, on a
/// one-row header, in the longest form the place's name (or its start)
/// leaves.
fn compose(
    (place, seat): (&Place, Option<&str>),
    room: usize,
    (width, rows): (usize, u16),
    (ascii, color): (bool, bool),
) -> Composed {
    let whole = intelligence_line(seat, usize::MAX, ascii, color);
    let cells = whole.as_ref().map_or(0, Line::width);
    let floor = refusal(place) + GAP;
    if room >= named(place, ascii) + floor + cells {
        return Composed {
            place: place_line(place, room - GAP - cells, ascii, color),
            beside: whole,
            under: None,
        };
    }
    if rows >= 2 {
        // The second row keeps a blank cell before the words and its air after.
        let under = width.saturating_sub(1 + usize::from(AIR));
        return Composed {
            place: place_line(place, room, ascii, color),
            beside: None,
            under: intelligence_line(seat, under, ascii, color),
        };
    }
    let fit =
        |kept: usize| intelligence_line(seat, room.saturating_sub(kept + floor), ascii, color);
    let beside = fit(named(place, ascii)).or_else(|| fit(least_place(place, ascii)));
    let used = beside.as_ref().map_or(0, |line| line.width() + GAP);
    Composed {
        place: place_line(place, room.saturating_sub(used), ascii, color),
        beside,
        under: None,
    }
}

/// Paint the header into `area`, its words ending one blank cell before
/// `names` (the region names' cells, which the caller paints and the pointer
/// reads) when they are drawn, else one blank cell before the edge.
pub fn render(
    (place, seat): (&Place, Option<&str>),
    area: Rect,
    names: Option<Rect>,
    buf: &mut Buffer,
    (ascii, color): (bool, bool),
) {
    if area.height == 0 {
        return;
    }
    let end = names
        .map_or(area.right(), |names| names.x)
        .saturating_sub(AIR)
        .max(area.x);
    let room = end - area.x;
    let size = (usize::from(area.width), area.height);
    let composed = compose((place, seat), usize::from(room), size, (ascii, color));
    Paragraph::new(composed.place).render(Rect::new(area.x, area.y, room, 1), buf);
    if let Some(line) = composed.beside {
        let cells = u16::try_from(line.width()).unwrap_or(room).min(room);
        Paragraph::new(line).render(Rect::new(end - cells, area.y, cells, 1), buf);
    }
    if area.height < 2 {
        return;
    }
    let y = area.y + 1;
    let rule = if ascii { "-" } else { "─" };
    let line = Line::styled(rule.repeat(usize::from(area.width)), role::border(color));
    buf.set_line(area.x, y, &line, area.width);
    if let Some(line) = composed.under {
        // The rule runs up to one blank cell before the words, which keep
        // their air before the edge.
        let air = " ".repeat(usize::from(AIR));
        let cells = u16::try_from(1 + line.width() + air.len())
            .unwrap_or(area.width)
            .min(area.width);
        let at = Rect::new(area.right() - cells, y, cells, 1);
        let mut words = vec![Span::raw(" ")];
        words.extend(line.spans);
        words.push(Span::raw(air));
        Paragraph::new(Line::from(words)).render(at, buf);
    }
}
