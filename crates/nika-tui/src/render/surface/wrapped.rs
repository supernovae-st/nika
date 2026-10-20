// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A question's or a decision's offer whose words do not fit the band's width, wrapped under
//! its own indent rather than cut at the edge (live gauntlet 5, r02 B6: « …avant d'écrire
//! python-version.txt / re… »): at most [`WRAPPED`] rows, the last one cut with a visible mark
//! where more words follow. The list's pages and the pointer then read each item's height, the
//! same heights painting uses, so a press on any row of an offer lands on that offer.

use std::ops::Range;

use nika_display::theme::Role;
use nika_tui_view::workspace::surface::{Mark, Plan};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::visual::role;
use nika_tui_view::workspace::text::{fit_head, marks, wrap};

/// The most rows one offer's words take; the last one kept is cut where more follow.
pub(super) const WRAPPED: usize = 3;

/// One offer's rows, `width` cells wide: the selection glyph and its `lead` (the digit and the
/// radio), then its `name` and its quiet `facts`, wrapped under the indent the lead sets, the
/// facts after the name on its last row where they fit, else on rows of their own; at most
/// [`WRAPPED`] rows. The selected row of an active list is filled on every row, reversed
/// without colour, as a one-row item is.
pub(super) fn rows(
    (lead, name, facts): (&str, &str, &str),
    width: u16,
    mark: Mark,
    (ascii, color): (bool, bool),
) -> Vec<Line<'static>> {
    let (separator, cut) = marks(ascii);
    let width = usize::from(width);
    let glyph = match (mark.selected, ascii) {
        (true, false) => "› ",
        (true, true) => "> ",
        (false, _) => "  ",
    };
    let head = format!("{glyph}{lead}");
    let indent = " ".repeat(head.width());
    let room = width.saturating_sub(head.width()).max(1);
    let parts = parts((name, facts), room, (separator, cut));
    let filled = mark.selected && mark.active;
    let fill = match (filled, color) {
        (true, true) => role::selection(true),
        (true, false) => Style::default().add_modifier(Modifier::REVERSED),
        (false, _) => Style::default(),
    };
    let named = if mark.selected {
        role::style(Role::Strong, color)
    } else {
        role::style(Role::Accent, color)
    };
    let quiet = role::style(Role::Dim, color);
    (parts.into_iter().enumerate())
        .map(|(at, (words, said))| {
            let start = if at == 0 {
                head.clone()
            } else {
                indent.clone()
            };
            let used = start.width() + words.width() + said.width();
            let pad = if filled {
                " ".repeat(width.saturating_sub(used))
            } else {
                String::new()
            };
            Line::from(vec![
                Span::styled(start, named.patch(fill)),
                Span::styled(words, named.patch(fill)),
                Span::styled(said, quiet.patch(fill)),
                Span::styled(pad, fill),
            ])
        })
        .collect()
}

/// The rows of `name` then `facts`, `room` cells each, as (the name's words, the facts' words)
/// per row: each fact (the facts are parted by `separator`) after the row before it where it
/// fits, else on rows of its own, so a fact's words never part at its separator; past
/// [`WRAPPED`] rows, the last kept row ends with the `cut` mark.
fn parts(
    (name, facts): (&str, &str),
    room: usize,
    (separator, cut): (&str, &str),
) -> Vec<(String, String)> {
    let mut parts: Vec<(String, String)> = (wrap(name, room, cut).into_iter())
        .map(|words| (words, String::new()))
        .collect();
    if parts.is_empty() {
        parts.push((String::new(), String::new()));
    }
    let said = (facts.split(separator).map(str::trim)).filter(|fact| !fact.is_empty());
    for fact in said {
        let used = parts
            .last()
            .map_or(0, |(words, said)| words.width() + said.width());
        match parts.last_mut() {
            Some(last) if used == 0 => last.1.push_str(fact),
            Some(last) if used + separator.width() + fact.width() <= room => {
                last.1.push_str(separator);
                last.1.push_str(fact);
            }
            _ => parts.extend(
                wrap(fact, room, cut)
                    .into_iter()
                    .map(|said| (String::new(), said)),
            ),
        }
    }
    let more = parts
        .get(WRAPPED)
        .map(|(words, said)| format!("{words}{said}"));
    parts.truncate(WRAPPED);
    if let (Some(more), Some(last)) = (more, parts.last_mut()) {
        // The last row kept says more follows: its words and the next ones, cut at the room.
        if last.1.is_empty() {
            last.0 = cut_before(&last.0, &more, room, cut);
        } else {
            last.1 = cut_before(&last.1, &more, room.saturating_sub(last.0.width()), cut);
        }
    }
    parts
}

/// `words` then `more`, cut to `room` cells with the `cut` mark right after the last word kept.
fn cut_before(words: &str, more: &str, room: usize, cut: &str) -> String {
    let head = fit_head(&format!("{words} {more}"), room, cut);
    match head.strip_suffix(cut) {
        Some(kept) => format!("{}{cut}", kept.trim_end()),
        None => head,
    }
}

/// The items a list of `rows` rows shows, out of items `heights` rows tall: the whole page that
/// holds `selected`, the first while nothing is selected, as the one-row pages
/// ([`nika_tui_view::workspace::surface::page`]) are, a page ending before the item that no
/// longer fits in it. A page moves only when the selection leaves it; an item taller than the
/// list stands alone on its page and shows its first rows.
pub(super) fn page(selected: Option<usize>, heights: &[usize], rows: usize) -> Range<usize> {
    if rows == 0 || heights.is_empty() {
        return 0..0;
    }
    let target = selected.map_or(0, |at| at.min(heights.len() - 1));
    let (mut start, mut used) = (0, 0);
    for (at, height) in heights.iter().enumerate() {
        if at > start && used + height > rows {
            if target < at {
                return start..at;
            }
            (start, used) = (at, 0);
        }
        used += height;
    }
    start..heights.len()
}

/// The item painted at row `y` of `plan`'s list showing `shown`, items `heights` rows tall, if
/// any: every row an item takes lands on it.
pub(super) fn item_at(
    plan: &Plan,
    shown: &Range<usize>,
    heights: &[usize],
    y: u16,
) -> Option<usize> {
    if y < plan.list.y || y >= plan.list.bottom() {
        return None;
    }
    let row = usize::from(y - plan.list.y);
    let mut top = 0;
    for place in shown.clone() {
        top += heights.get(place).copied().unwrap_or(1);
        if row < top {
            return Some(place);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One-row items page as the one-row pages do; taller items end a page before the item
    /// that no longer fits, and an item taller than the list stands alone.
    #[test]
    fn pages_of_taller_items_end_before_the_item_that_no_longer_fits() {
        let ones = [1; 29];
        for selected in [
            None,
            Some(0),
            Some(7),
            Some(8),
            Some(10),
            Some(28),
            Some(40),
        ] {
            assert_eq!(
                page(selected, &ones, 8),
                nika_tui_view::workspace::surface::page(selected, 29, 8),
                "{selected:?}"
            );
        }
        let heights = [1, 3, 2, 3, 1];
        assert_eq!(page(None, &heights, 4), 0..2);
        assert_eq!(page(Some(1), &heights, 4), 0..2);
        assert_eq!(page(Some(2), &heights, 4), 2..3);
        assert_eq!(page(Some(3), &heights, 4), 3..5);
        assert_eq!(page(Some(4), &heights, 4), 3..5);
        assert_eq!(
            page(Some(0), &[5, 1], 3),
            0..1,
            "alone, its first rows shown"
        );
        assert_eq!(page(Some(0), &[], 3), 0..0);
        assert_eq!(page(Some(0), &heights, 0), 0..0);
    }

    /// Past the most rows an offer takes, the last row kept ends with the cut mark; facts that
    /// fit after the name's last row stand there.
    #[test]
    fn an_offer_wraps_to_three_rows_at_most_its_last_cut() {
        let long = "one two three four five six seven eight nine ten eleven twelve";
        let kept = parts((long, ""), 10, (" · ", "…"));
        assert_eq!(kept.len(), WRAPPED, "{kept:?}");
        assert!(kept[2].0.ends_with('…'), "{kept:?}");
        assert!(
            kept.iter()
                .all(|(words, said)| words.width() + said.width() <= 10)
        );
        let short = parts(("yes", "recommended"), 20, (" · ", "…"));
        assert_eq!(short, [("yes".to_owned(), " · recommended".to_owned())]);
        let after = parts(("yes do it now", "recommended"), 14, (" · ", "…"));
        assert_eq!(
            after,
            [
                ("yes do it now".to_owned(), String::new()),
                (String::new(), "recommended".to_owned())
            ]
        );
        // A fact never parts at its separator: the next one opens a row of its own.
        let facts = parts(
            ("TechCrunch", "techcrunch.com/feed · recommended"),
            24,
            (" · ", "…"),
        );
        assert_eq!(
            facts,
            [
                ("TechCrunch".to_owned(), String::new()),
                (String::new(), "techcrunch.com/feed".to_owned()),
                (String::new(), "recommended".to_owned())
            ]
        );
    }
}
