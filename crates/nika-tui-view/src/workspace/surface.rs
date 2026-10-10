// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The continuous surface: one opaque band of the conversation panel, from
//! the transcript's first row down to the fixed composer, that holds what is
//! chosen now (the commands, a question's offers). The composer's own rows
//! stay where they stand and carry the surface's filter or answer; the hint
//! row under them carries its action. Opening, filtering, nesting or closing
//! a surface moves none of them, and the transcript behind keeps its rows
//! and its reading position: the band only hides it.
//!
//! One plan ([`plan`]) places the title, the head, the list and the tail in
//! the band; painting, the pointer ([`item_at`]) and the list's own pages
//! ([`page`]) read that same plan. The list is the one part that pages: the
//! title, the head and the tail stay whole or give way whole. Nothing here
//! reads a key, a clock or the environment.

use std::ops::Range;

use nika_display::theme::Role;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Widget};
use unicode_width::UnicodeWidthStr;

use super::text::{fit_head, marks};
use crate::visual::role;

/// The frame's border in the ASCII column.
const ASCII_FRAME: border::Set<'static> = border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

/// What a surface asks of its band under its title row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Ask {
    /// The head rows that show whole or not at all (a question's facts).
    pub head: u16,
    /// Further head rows that give way first (more of a question's words).
    pub more: u16,
    /// The listed items, one row each.
    pub items: usize,
    /// The tail rows under the list, all or none.
    pub tail: u16,
}

impl Ask {
    /// A surface listing `items` under its title.
    #[must_use]
    pub const fn new(items: usize) -> Self {
        Self {
            head: 0,
            more: 0,
            items,
            tail: 0,
        }
    }

    /// With `head` rows that show whole, then `more` that give way first.
    #[must_use]
    pub const fn headed(mut self, head: u16, more: u16) -> Self {
        self.head = head;
        self.more = more;
        self
    }

    /// With `tail` rows under the list.
    #[must_use]
    pub const fn tailed(mut self, tail: u16) -> Self {
        self.tail = tail;
        self
    }
}

/// The fewest list rows a surface keeps before its head and tail take any:
/// three, or every item when there are fewer.
const LEAST: u16 = 3;

/// Where a surface's parts stand in its band: one plan for painting, the
/// pointer and the list's pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Plan {
    /// The whole band, cleared before anything is painted.
    pub band: Rect,
    /// Whether the band stands framed (its sides and top drawn).
    pub framed: bool,
    /// The title row (on the frame's top edge when framed).
    pub title: Rect,
    /// The head rows shown.
    pub head: Rect,
    /// The list rows.
    pub list: Rect,
    /// The tail rows (all of them or none).
    pub tail: Rect,
}

/// The plan of a surface asking `ask` in `band`, `framed` or not. The title
/// takes the first row; under it the list keeps `LEAST` rows (or all its
/// items), the head's whole rows come next, then the tail whole, then the
/// rest of the list, and the head's further rows last. A band of one row
/// holds the title alone.
#[must_use]
pub fn plan(band: Rect, ask: Ask, framed: bool) -> Plan {
    let framed = framed && band.width >= 6;
    // Under the title row: inside the frame's sides, one cell of air each.
    let below = 1_u16.min(band.height);
    let inner = if framed {
        Rect::new(
            band.x + 2,
            band.y + below,
            band.width - 4,
            band.height - below,
        )
    } else {
        Rect::new(band.x, band.y + below, band.width, band.height - below)
    };
    let title = Rect::new(band.x, band.y, band.width, band.height.min(1));
    let items = u16::try_from(ask.items).unwrap_or(u16::MAX);
    let mut room = inner.height;
    let least = items.clamp(1, LEAST).min(room);
    room -= least;
    // The head shows whole or not at all, and its further rows only under it.
    let whole = ask.head <= room;
    let head = if whole { ask.head } else { 0 };
    room -= head;
    let tail = if ask.tail <= room { ask.tail } else { 0 };
    room -= tail;
    let rest = items.saturating_sub(least).min(room);
    room -= rest;
    let more = if whole { ask.more.min(room) } else { 0 };
    let head_rows = head + more;
    let list_rows = least + rest;
    let head_rect = Rect::new(inner.x, inner.y, inner.width, head_rows);
    let list_rect = Rect::new(inner.x, inner.y + head_rows, inner.width, list_rows);
    let tail_rect = Rect::new(inner.x, list_rect.bottom(), inner.width, tail);
    Plan {
        band,
        framed,
        title,
        head: head_rect,
        list: list_rect,
        tail: tail_rect,
    }
}

/// The items a list of `rows` rows shows out of `count`: the whole page that
/// holds `selected`, the first page while nothing is selected. A page moves
/// only when the selection leaves it, so a press never moves the list under
/// the pointer, and painting and the pointer read the same page.
#[must_use]
pub fn page(selected: Option<usize>, count: usize, rows: usize) -> Range<usize> {
    if rows == 0 || count == 0 {
        return 0..0;
    }
    let first = selected.map_or(0, |at| at.min(count - 1) / rows * rows);
    first..count.min(first + rows)
}

/// The item painted at row `y` of `plan`'s list showing `shown`, if any.
#[must_use]
pub fn item_at(plan: &Plan, shown: &Range<usize>, y: u16) -> Option<usize> {
    if y < plan.list.y || y >= plan.list.bottom() {
        return None;
    }
    let index = shown.start + usize::from(y - plan.list.y);
    shown.contains(&index).then_some(index)
}

/// How an item's row is marked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Mark {
    /// The item is the selected one.
    pub selected: bool,
    /// The list takes keys now: the selection is also filled (reversed
    /// without colour), never shown by a hue alone.
    pub active: bool,
}

impl Mark {
    /// The mark of an item `selected` or not in a list `active` or not.
    #[must_use]
    pub const fn new(selected: bool, active: bool) -> Self {
        Self { selected, active }
    }
}

/// One item's row, `width` cells: the selection glyph, the `name` padded to
/// `column` cells, then what it `does`, cut to fit. The selected row of an
/// active list is filled, reversed without colour.
#[must_use]
pub fn row(
    (name, does): (&str, &str),
    column: usize,
    width: u16,
    mark: Mark,
    (ascii, color): (bool, bool),
) -> Line<'static> {
    let (separator, cut) = marks(ascii);
    let width = usize::from(width);
    let glyph = match (mark.selected, ascii) {
        (true, false) => "› ",
        (true, true) => "> ",
        (false, _) => "  ",
    };
    let pad = " ".repeat(column.saturating_sub(name.width()));
    let head = fit_head(&format!("{glyph}{name}{pad}"), width, cut);
    let rest = if does.trim().is_empty() || does == name {
        String::new()
    } else {
        fit_head(
            &format!("{separator}{does}"),
            width.saturating_sub(head.width()),
            cut,
        )
    };
    let filled = mark.selected && mark.active;
    let fill = match (filled, color) {
        (true, true) => role::selection(true),
        (true, false) => Style::default().add_modifier(Modifier::REVERSED),
        (false, _) => Style::default(),
    };
    let name_style = if mark.selected {
        role::style(Role::Strong, color)
    } else {
        role::style(Role::Accent, color)
    };
    let pad = " ".repeat(width.saturating_sub(head.width() + rest.width()));
    let pad = if filled { pad } else { String::new() };
    Line::from(vec![
        Span::styled(head, name_style.patch(fill)),
        Span::styled(rest, role::style(Role::Dim, color).patch(fill)),
        Span::styled(pad, fill),
    ])
}

/// The title row, `width` cells: the surface's `name` in the accent, its
/// `facts` quiet, and what closes it (`close`, such as `Esc`) at the right
/// edge when the row has room for all three.
#[must_use]
pub fn title(
    (name, facts): (&str, &str),
    close: &str,
    width: u16,
    (ascii, color): (bool, bool),
) -> Line<'static> {
    let (_, cut) = marks(ascii);
    let width = usize::from(width);
    let name = fit_head(name, width, cut);
    let room = width.saturating_sub(name.width());
    let facts = fit_head(facts, room, cut);
    let left = name.width() + facts.width();
    let close = if left + 1 + close.width() <= width {
        format!("{}{close}", " ".repeat(width - left - close.width()))
    } else {
        String::new()
    };
    Line::from(vec![
        Span::styled(
            name,
            role::style(Role::Accent, color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(facts, role::style(Role::Dim, color)),
        Span::styled(close, role::style(Role::Dim, color)),
    ])
}

/// Clear `plan`'s band and draw its frame: every cell blank on the raised
/// surface (no cell of what it hides stays), the sides and top edge when it
/// stands framed. The title is painted over the top row afterwards.
pub fn clear(plan: &Plan, buf: &mut Buffer, (ascii, color): (bool, bool)) {
    let ground = role::surface(color, true);
    for y in plan.band.top()..plan.band.bottom() {
        for x in plan.band.left()..plan.band.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.reset();
                cell.set_style(ground);
            }
        }
    }
    if plan.framed {
        let set = if ascii { ASCII_FRAME } else { border::ROUNDED };
        Block::new()
            .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
            .border_set(set)
            .border_style(role::border(color))
            .render(plan.band, buf);
    }
}

/// Paint `line` on the title row of `plan`: inside the frame's corners when
/// framed, one cell of air on each side.
pub fn paint_title(plan: &Plan, line: &Line<'_>, buf: &mut Buffer) {
    let (x, width) = if plan.framed {
        (plan.title.x + 2, plan.title.width.saturating_sub(4))
    } else {
        (plan.title.x, plan.title.width)
    };
    if plan.title.height > 0 && width > 0 {
        buf.set_line(x, plan.title.y, line, width);
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn band(height: u16) -> Rect {
        Rect::new(3, 2, 40, height)
    }

    /// The plan tiles its band from the top: the title, the head, the list,
    /// the tail, each under the last, never past the band.
    #[test]
    fn the_plan_tiles_its_band_in_order_and_never_past_it() {
        for height in 1..30 {
            for framed in [false, true] {
                let ask = Ask::new(12).headed(2, 3).tailed(1);
                let plan = plan(band(height), ask, framed);
                assert_eq!(plan.title.y, plan.band.y);
                assert_eq!(plan.head.y, plan.band.y + 1);
                assert_eq!(plan.list.y, plan.head.bottom());
                assert_eq!(plan.tail.y, plan.list.bottom());
                assert!(
                    plan.tail.bottom() <= plan.band.bottom(),
                    "{height} {framed}"
                );
                assert!(plan.head.height == 0 || plan.head.height >= 2, "head whole");
                assert!(plan.tail.height <= 1, "tail whole");
            }
        }
    }

    /// The list keeps its least rows before the head and the tail take any,
    /// then the head whole, the tail whole, the rest of the list, and the
    /// head's further rows last.
    #[test]
    fn the_list_keeps_its_least_rows_and_further_head_rows_give_way_first() {
        let ask = Ask::new(12).headed(2, 3).tailed(1);
        let short = plan(band(5), ask, false);
        assert_eq!(
            (short.list.height, short.head.height, short.tail.height),
            (3, 0, 1)
        );
        let tall = plan(band(9), ask, false);
        assert_eq!(
            (tall.head.height, tall.tail.height, tall.list.height),
            (2, 1, 5)
        );
        let roomy = plan(band(30), ask, false);
        assert_eq!(
            (roomy.head.height, roomy.list.height, roomy.tail.height),
            (5, 12, 1)
        );
        let few = plan(band(30), Ask::new(2), false);
        assert_eq!(few.list.height, 2, "never more rows than items");
    }

    /// A framed band keeps its sides and its top edge, and its rows one cell
    /// of air inside each side.
    #[test]
    fn a_framed_band_keeps_its_edges_and_air() {
        let plan = plan(band(10), Ask::new(4), true);
        assert!(plan.framed);
        assert_eq!(plan.list.x, plan.band.x + 2);
        assert_eq!(plan.list.width, plan.band.width - 4);
        let narrow = super::plan(Rect::new(0, 0, 5, 10), Ask::new(4), true);
        assert!(!narrow.framed, "too narrow for a frame");
    }

    /// The list pages whole around the selection, and a row maps back to the
    /// very item painted there.
    #[test]
    fn the_list_pages_around_the_selection_and_rows_map_back_to_items() {
        assert_eq!(page(None, 10, 4), 0..4);
        assert_eq!(page(Some(2), 10, 4), 0..4);
        assert_eq!(page(Some(5), 10, 4), 4..8);
        assert_eq!(page(Some(9), 10, 4), 8..10, "the last page holds the rest");
        assert_eq!(page(Some(99), 10, 4), 8..10);
        assert_eq!(page(Some(1), 0, 4), 0..0);
        assert_eq!(page(Some(1), 3, 0), 0..0);
        let plan = plan(band(8), Ask::new(10), false);
        let shown = page(Some(7), 10, usize::from(plan.list.height));
        assert_eq!(shown, 7..10);
        for (row, item) in (plan.list.y..plan.list.bottom()).zip(shown.clone()) {
            assert_eq!(item_at(&plan, &shown, row), Some(item));
        }
        let past = plan.list.y + 3;
        assert_eq!(
            item_at(&plan, &shown, past),
            None,
            "no item under an empty row"
        );
        assert_eq!(item_at(&plan, &shown, plan.title.y), None);
        assert_eq!(item_at(&plan, &shown, plan.list.bottom()), None);
    }

    /// The selected row of an active list is filled, reversed without colour,
    /// and marked by its glyph in both columns; no row passes its width.
    #[test]
    fn a_selected_row_is_marked_without_relying_on_a_hue() {
        let words = ("/status", "Where you are");
        let line = row(words, 12, 30, Mark::new(true, true), (false, true));
        assert!(line.to_string().starts_with("› /status"));
        assert_eq!(line.width(), 30, "the fill spans the row");
        assert_eq!(line.spans[0].style.bg, role::selection(true).bg);
        let plain = row(words, 12, 30, Mark::new(true, true), (true, false));
        assert!(plain.to_string().starts_with("> /status"));
        let style = plain.spans[0].style;
        assert!(style.add_modifier.contains(Modifier::REVERSED));
        assert_eq!((style.fg, style.bg), (None, None));
        let idle = row(words, 12, 30, Mark::new(false, false), (true, false));
        assert!(idle.to_string().starts_with("  /status"));
        for width in 0..40 {
            let words = ("/intelligence", "Choose the AI");
            let line = row(words, 14, width, Mark::new(true, true), (false, true));
            assert!(line.width() <= usize::from(width), "{width}");
        }
    }

    /// The title names the surface, its facts and what closes it, and drops
    /// the closing word before cutting the name.
    #[test]
    fn the_title_keeps_its_name_before_its_closing_word() {
        let line = title(
            ("Commands", " · choose an action"),
            "Esc",
            40,
            (false, true),
        );
        let text = line.to_string();
        assert!(text.starts_with("Commands · choose an action"), "{text}");
        assert!(text.ends_with("Esc"), "{text}");
        assert_eq!(line.width(), 40);
        let tight = title(
            ("Commands", " - choose an action"),
            "Esc",
            12,
            (true, false),
        );
        assert!(tight.to_string().starts_with("Commands"), "{tight}");
        assert!(!tight.to_string().contains("Esc"));
    }

    /// Clearing leaves no cell of what the band hid, and draws its frame.
    #[test]
    fn clearing_leaves_no_ghost_cell_and_draws_the_frame() {
        let area = Rect::new(0, 0, 20, 6);
        let mut buf = Buffer::empty(area);
        for y in 0..6 {
            buf.set_string(0, y, "x".repeat(20), Style::default());
        }
        let plan = plan(area, Ask::new(2), true);
        clear(&plan, &mut buf, (false, false));
        let text: String = (0..6)
            .flat_map(|y| (0..20).map(move |x| (x, y)))
            .map(|(x, y)| buf[(x, y)].symbol().to_owned())
            .collect();
        assert!(!text.contains('x'), "a ghost cell stayed: {text}");
        assert_eq!(buf[(0, 0)].symbol(), "╭");
        assert_eq!(buf[(0, 3)].symbol(), "│");
        let mut ascii = Buffer::empty(area);
        clear(&plan, &mut ascii, (true, false));
        assert_eq!(ascii[(0, 0)].symbol(), "+");
    }
}
