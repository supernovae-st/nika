// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The continuous surface on screen: one opaque band above the composer,
//! whose rows stay where they stand ([`super::band`]). It holds what is
//! chosen now, one thing at a time: the commands the chooser lists (its
//! palette or the slash list of a command being typed), else the offers of
//! the typed choice waiting. The band's plan is the viewer's
//! ([`nika_tui_view::workspace::surface`]): a title, a head (the Session's
//! status facts and what waits, or the question's own words), the entries or
//! offers on whole pages, and the selected command's scope and help. The
//! composer's own row carries the palette's search or the typed choice's own
//! reply, and the hint row its keys. Painting and the pointer ([`hit`]) read
//! one plan; nothing here sends, answers or chooses.

use std::fmt::Write as _;
use std::ops::Range;

use nika_display::theme::Role;
use nika_tui_view::workspace::surface::{self, Ask, Mark, Plan};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};
use unicode_width::UnicodeWidthStr;

use super::chooser::{aside_line, context, fit};
use super::question::{WHOLE, asked_block, hang, word_cap};
use super::{own, status_line};
use crate::composer::Composer;
use crate::composer::answer::choice;
use crate::composer::chooser::{Door, Listing};
use crate::model::{Asked, Offer, UiState, Waiting};
use crate::visual::role;

/// At most this many rows carry the selected command's scope and help.
const DETAIL_ROWS: u16 = 2;

/// At most this many entries or offers an inline band grows for; the rest
/// page.
pub(crate) const SHOWN: usize = 8;

/// The command names' column: the widest name, between these bounds.
const NAME_COLUMN: (usize, usize) = (6, 14);

/// The offers' key column: the widest key, up to this; a longer key pushes
/// its label along.
const KEY_COLUMN: usize = 16;

/// What the band holds now.
enum Held<'a> {
    /// The commands the chooser lists, words the palette set aside, what
    /// waits beside them and the selected entry's scope and help.
    Commands {
        listing: Listing<'a>,
        aside: Option<&'a str>,
        context: Option<&'static str>,
        detail: String,
    },
    /// The typed choice waiting: the question, its offers and the one
    /// selected, if any, and whether the offers take keys now.
    Choice {
        asked: &'a Asked,
        offers: &'a [Offer],
        selected: Option<usize>,
        armed: bool,
    },
}

impl<'a> Held<'a> {
    /// What a surface shows on `state` with `composer`: the commands while
    /// the chooser lists (they suspend a typed choice), else the typed choice
    /// waiting while no turn works.
    fn of(state: &'a UiState, composer: &'a Composer) -> Option<Self> {
        if let Some(listing) = composer.listing() {
            let detail = (listing.current()).map_or_else(String::new, |entry| {
                own(&format!("{} · {}", entry.scope, entry.help), state.ascii)
            });
            return Some(Self::Commands {
                listing,
                aside: composer.aside(),
                context: context(state),
                detail,
            });
        }
        if state.busy.is_some() {
            return None;
        }
        let (_, asked, offers) = choice(&state.waiting)?;
        Some(Self::Choice {
            asked,
            offers,
            selected: composer.offer_selected(&state.waiting),
            armed: composer.offers_armed(&state.waiting),
        })
    }

    /// The rows listed: every entry or offer, or the one row saying a search
    /// found nothing.
    fn items(&self) -> usize {
        match self {
            Self::Commands { listing, .. } => listing.entries.len().max(1),
            Self::Choice { offers, .. } => offers.len(),
        }
    }

    /// The entries or offers listed, without the row a search finding
    /// nothing paints.
    fn count(&self) -> usize {
        match self {
            Self::Commands { listing, .. } => listing.entries.len(),
            Self::Choice { offers, .. } => offers.len(),
        }
    }

    /// The selected item, if any.
    fn selected(&self) -> Option<usize> {
        match self {
            Self::Commands { listing, .. } => Some(listing.selected),
            Self::Choice { selected, .. } => *selected,
        }
    }
}

/// The rows of a head, top first, `width` cells wide, and how many of the
/// first ones show whole or not at all (the rest give way first): beside the
/// commands, the Session's status facts, the words the palette set aside and
/// what waits; under a typed choice, what the Session keeps of the request
/// (its goal, the questions still open), then the question's own words, cut
/// at the frame's cap with a last row saying where the whole question is
/// read.
fn head_rows(held: &Held<'_>, state: &UiState, width: u16) -> (Vec<Line<'static>>, u16) {
    let (ascii, color) = (state.ascii, state.color);
    match held {
        Held::Commands { aside, context, .. } => {
            let mut rows = Vec::new();
            let status = status_line(state, true, width, None);
            if status.width() > 0 {
                rows.push(status);
            }
            if let Some(aside) = aside {
                rows.push(aside_line(aside, width, ascii, color));
            }
            if let Some(text) = context {
                let tone = if state.busy.is_some() || state.waiting == Waiting::Free {
                    Role::Accent
                } else {
                    Role::Warn
                };
                let words = fit(&own(text, ascii), usize::from(width), ascii);
                rows.push(Line::styled(words, role::style(tone, color)));
            }
            let whole = u16::try_from(rows.len()).unwrap_or(u16::MAX);
            (rows, whole)
        }
        Held::Choice { asked, .. } => {
            let mut rows = retained_rows(asked, width, (ascii, color));
            let block = asked_block(state).and_then(|index| state.transcript.get(index));
            let text = block.map_or_else(|| asked.label.clone(), |block| block.text.clone());
            let mut words: Vec<Line<'static>> = (hang(&text, usize::from(width)).into_iter())
                .map(Line::raw)
                .collect();
            let cap = word_cap(state);
            if words.len() > cap + 1 {
                words.truncate(cap);
                words.push(whole_row(width, (ascii, color)));
            }
            // The kept request and the question's first row show whole.
            let whole = u16::try_from(rows.len() + 1).unwrap_or(u16::MAX);
            rows.extend(words);
            (rows, whole)
        }
    }
}

/// What the Session keeps of the request beside a typed choice, one quiet
/// labelled row each: its goal, then the questions still open.
fn retained_rows(asked: &Asked, width: u16, (ascii, color): (bool, bool)) -> Vec<Line<'static>> {
    let Some(retained) = &asked.retained else {
        return Vec::new();
    };
    let width = usize::from(width);
    let quiet = role::style(Role::Dim, color);
    let row = |label: &str, words: &str| {
        let label = own(label, ascii);
        let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
        let words = fit(&words, width.saturating_sub(label.width()), ascii);
        Line::from(vec![Span::styled(label, quiet), Span::raw(words)])
    };
    let mut rows = Vec::new();
    if let Some(goal) = &retained.goal {
        rows.push(row("Request · ", goal.as_str()));
    }
    if !retained.open.is_empty() {
        let open = retained.open.join(" · ");
        rows.push(row("Open · ", open.as_str()));
    }
    rows
}

/// The row that says where the whole question is read.
fn whole_row(width: u16, (ascii, color): (bool, bool)) -> Line<'static> {
    let words = fit(&own(WHOLE, ascii), usize::from(width), ascii);
    Line::styled(words, role::style(Role::Dim, color))
}

/// What the band asks at `width` cells under its title.
fn ask(held: &Held<'_>, state: &UiState, width: u16) -> Ask {
    let (rows, whole) = head_rows(held, state, width);
    let height = u16::try_from(rows.len()).unwrap_or(u16::MAX);
    let tail = match held {
        Held::Commands {
            listing, detail, ..
        } if !listing.entries.is_empty() => {
            let rows = super::content_rows(&[Line::raw(detail.as_str())], width);
            u16::try_from(rows)
                .unwrap_or(DETAIL_ROWS)
                .clamp(1, DETAIL_ROWS)
        }
        Held::Commands { .. } | Held::Choice { .. } => 0,
    };
    Ask::new(held.items())
        .headed(whole.min(height), height.saturating_sub(whole))
        .tailed(tail)
}

/// Whether a surface is open on `state` with `composer`.
pub(crate) fn open(state: &UiState, composer: &Composer) -> bool {
    Held::of(state, composer).is_some()
}

/// Whether the typed choice waiting holds the surface (choice mode): its own
/// reply's field then stands in the line's place.
pub(crate) fn choosing(state: &UiState, composer: &Composer) -> bool {
    matches!(Held::of(state, composer), Some(Held::Choice { .. }))
}

/// The rows an inline band asks for at `width` cells (a full screen paints
/// its band over the transcript and asks for none): the title, the head, at
/// most [`SHOWN`] items and the detail.
pub(crate) fn demand(state: &UiState, composer: &Composer, width: u16) -> u16 {
    Held::of(state, composer).map_or(0, |held| {
        let ask = ask(&held, state, width);
        let shown = u16::try_from(held.items().min(SHOWN)).unwrap_or(1);
        1 + ask.head + ask.more + shown + ask.tail
    })
}

/// The width a band's rows take inside its frame.
const fn inner(band: Rect, framed: bool) -> u16 {
    if framed {
        band.width.saturating_sub(4)
    } else {
        band.width
    }
}

/// The plan of `held` in `band`, `framed` or not, and the page it shows.
fn plan_of(held: &Held<'_>, state: &UiState, (band, framed): (Rect, bool)) -> (Plan, Range<usize>) {
    let plan = surface::plan(band, ask(held, state, inner(band, framed)), framed);
    let page = surface::page(held.selected(), held.count(), usize::from(plan.list.height));
    (plan, page)
}

/// Where a press at a point lands on the open surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hit {
    /// On this listed command, or this offer of the typed choice.
    Item(usize),
    /// On the band, away from any item: it takes the press and does nothing.
    Band,
}

/// What a press at `point` lands on, for the surface open in `band`: the
/// same plan and page the band paints. `None` outside the band.
pub(crate) fn hit(
    state: &UiState,
    composer: &Composer,
    band: (Rect, bool),
    point: Position,
) -> Option<Hit> {
    let held = Held::of(state, composer)?;
    if !band.0.contains(point) {
        return None;
    }
    let (plan, page) = plan_of(&held, state, band);
    Some(surface::item_at(&plan, &page, point.y).map_or(Hit::Band, Hit::Item))
}

/// Paint the surface open in `band`, `framed` or not: cleared, its frame and
/// title, its head, the page of items, the selected command's detail.
pub(crate) fn render(state: &UiState, composer: &Composer, band: (Rect, bool), buf: &mut Buffer) {
    let Some(held) = Held::of(state, composer) else {
        return;
    };
    if band.0.height == 0 || band.0.width == 0 {
        return;
    }
    let glyphs = (state.ascii, state.color);
    let (plan, page) = plan_of(&held, state, band);
    surface::clear(&plan, buf, glyphs);
    let title = title(&held, &page, inner(band.0, band.1), state);
    surface::paint_title(&plan, &title, buf);
    head(&held, &plan, state, buf);
    match &held {
        Held::Commands {
            listing, detail, ..
        } => {
            commands(listing, (&plan, page), state, buf);
            if plan.tail.height > 0 {
                let words = Line::styled(detail.clone(), role::style(Role::Dim, state.color));
                Paragraph::new(words)
                    .wrap(Wrap { trim: false })
                    .render(plan.tail, buf);
            }
        }
        Held::Choice {
            offers,
            selected,
            armed,
            ..
        } => {
            let column = (offers.iter().map(|offer| offer.key.width()).max())
                .unwrap_or(0)
                .clamp(1, KEY_COLUMN);
            let width = plan.list.width;
            for (y, index) in (plan.list.y..plan.list.bottom()).zip(page) {
                let Some(offer) = offers.get(index) else {
                    break;
                };
                let mark = Mark::new(*selected == Some(index), *armed);
                let words = (offer.key.as_str(), offer.label.as_str());
                let line = surface::row(words, column, width, mark, glyphs);
                buf.set_line(plan.list.x, y, &line, width);
            }
        }
    }
}

/// The head's rows on `plan`: when the question's words do not all show,
/// the last row shown says where the whole question is read (or, with one
/// row only, its end does).
fn head(held: &Held<'_>, plan: &Plan, state: &UiState, buf: &mut Buffer) {
    let glyphs = (state.ascii, state.color);
    let (mut rows, _) = head_rows(held, state, plan.head.width);
    let shown = usize::from(plan.head.height);
    if matches!(held, Held::Choice { .. }) && shown < rows.len() {
        rows.truncate(shown);
        if shown >= 2 {
            rows[shown - 1] = whole_row(plan.head.width, glyphs);
        } else if let Some(first) = rows.first_mut() {
            let cue = own(" … F2", state.ascii);
            let room = usize::from(plan.head.width).saturating_sub(cue.width());
            let kept = fit(&first.to_string(), room, state.ascii);
            *first = Line::from(vec![
                Span::raw(kept),
                Span::styled(cue, role::style(Role::Dim, state.color)),
            ]);
        }
    }
    for (y, row) in (plan.head.y..plan.head.bottom()).zip(rows) {
        buf.set_line(plan.head.x, y, &row, plan.head.width);
    }
}

/// The title row: what the surface holds and its facts, and what closes it.
fn title(held: &Held<'_>, page: &Range<usize>, width: u16, state: &UiState) -> Line<'static> {
    let glyphs = (state.ascii, state.color);
    match held {
        Held::Commands { listing, .. } => {
            let facts = match (listing.door, listing.entries.is_empty()) {
                (_, true) => " · nothing matches",
                (Door::Palette, false) => " · choose an action",
                (Door::Slash, false) => " · beginning with what you typed",
            };
            surface::title(("Commands", &own(facts, state.ascii)), "Esc", width, glyphs)
        }
        Held::Choice { asked, offers, .. } => {
            let knowledge = matches!(state.waiting, Waiting::Knowledge { .. });
            let mut facts = String::new();
            if knowledge {
                facts.push_str(" · your message waits");
            } else if asked.mandatory {
                facts.push_str(" · required");
            }
            if page.len() < offers.len() {
                let _ = write!(
                    facts,
                    " · {}-{} of {}",
                    page.start + 1,
                    page.end,
                    offers.len()
                );
            }
            let name = if knowledge { "Knowledge" } else { "Question" };
            surface::title((name, &own(&facts, state.ascii)), "", width, glyphs)
        }
    }
}

/// The page of commands on `plan`'s list rows; a search that finds nothing
/// says so on the first.
fn commands(
    listing: &Listing<'_>,
    (plan, page): (&Plan, Range<usize>),
    state: &UiState,
    buf: &mut Buffer,
) {
    let (ascii, color) = (state.ascii, state.color);
    let width = plan.list.width;
    if listing.entries.is_empty() {
        let query = listing.query.unwrap_or_default();
        let (open, close) = if ascii { ("\"", "\"") } else { ("« ", " »") };
        let said = own(
            &format!("nothing matches {open}{query}{close} · Backspace edits the search"),
            ascii,
        );
        let words = fit(&said, usize::from(width), ascii);
        let line = Line::styled(words, role::style(Role::Dim, color));
        buf.set_line(plan.list.x, plan.list.y, &line, width);
        return;
    }
    let column = (listing.entries.iter())
        .map(|entry| entry.name.width())
        .max()
        .unwrap_or(0)
        .clamp(NAME_COLUMN.0, NAME_COLUMN.1);
    for (y, index) in (plan.list.y..plan.list.bottom()).zip(page) {
        let Some(entry) = listing.entries.get(index) else {
            break;
        };
        let mark = Mark::new(index == listing.selected, true);
        let effect = own(&entry.effect, ascii);
        let words = (entry.name.as_str(), effect.as_str());
        let line = surface::row(words, column, width, mark, (ascii, color));
        buf.set_line(plan.list.x, y, &line, width);
    }
}
