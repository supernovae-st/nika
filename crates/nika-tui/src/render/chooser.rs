// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The chooser in the live area, under the line it fills: what waits, when
//! something does (whatever is chosen, none of it answers); the listed
//! entries, each a name and what it does, the selection marked by a glyph
//! and by reverse video, never by colour alone; then the selected entry's
//! scope and help, on up to two rows. The palette's search takes the
//! composer's row, the draft waiting out of view, and a draft the palette
//! set aside is named on a row of its own until it returns.

use nika_display::theme::Role;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::own;
use crate::composer::Composer;
use crate::composer::chooser::{Door, Entry, Listing};
use crate::model::{UiState, Waiting};
use crate::visual::role;

/// At most this many entries show at once; the selection scrolls the rest.
pub(crate) const SHOWN: usize = 8;

/// The prompt of the palette's search, in place of the waiting prompt.
pub(crate) const SEARCH_PROMPT: &str = "commands › ";

/// What waits, said above the list when something does; `None` at a free
/// prompt. Every claim is the Session's own law: a command-shaped line is
/// never read as an answer, a consent or a gate's answer, and while Nika
/// works a command waits in the box (it is never a correction).
pub(crate) fn context(state: &UiState) -> Option<&'static str> {
    if state.busy.is_some() {
        return Some("Nika is working · a command waits in the box until your turn");
    }
    match &state.waiting {
        Waiting::Free => None,
        Waiting::Proposal => Some("A proposal waits · no command answers it · yes + Enter: Save"),
        Waiting::Gate => Some("A gate waits · no command answers it · approve or refuse"),
        Waiting::Choosing => Some("The intelligence choice waits · no command answers it"),
        Waiting::Question { key } | Waiting::QuestionDocument { key, .. }
            if key == "unknown_cost" || key == "run_cost" =>
        {
            Some("A cost decision waits · no command approves it · only your yes does")
        }
        Waiting::Question { .. } | Waiting::QuestionDocument { .. } => {
            Some("A question waits · no command answers it · your reply does")
        }
    }
}

/// At most this many rows carry the selected entry's scope and help.
const DETAIL_ROWS: usize = 2;

/// The rows the chooser asks for at `width` (none while it is closed).
pub(crate) fn rows(state: &UiState, composer: &Composer, width: u16) -> usize {
    composer.listing().map_or(0, |listing| {
        let detail = detail(&listing, state.ascii);
        let detail = super::content_rows(&[Line::raw(detail)], width).clamp(1, DETAIL_ROWS);
        usize::from(context(state).is_some()) + listing.entries.len().clamp(1, SHOWN) + detail
    })
}

/// The selected entry's scope, then its help.
fn detail(listing: &Listing<'_>, ascii: bool) -> String {
    listing.current().map_or_else(String::new, |entry| {
        own(&format!("{} · {}", entry.scope, entry.help), ascii)
    })
}

/// The hint row while the chooser shows: how to choose, that choosing never
/// sends, and what `Enter` does with a whole command: it sends it, or, while
/// Nika works, keeps it in the box for the human's turn.
pub(crate) fn hint(listing: &Listing<'_>, busy: bool, ascii: bool) -> String {
    let text = match (listing.door, listing.current()) {
        (Door::Palette, _) => {
            "↑↓ choose · Enter inserts, never sends · Esc: back to your draft".to_owned()
        }
        (Door::Slash, Some(entry)) if listing.whole && busy => {
            format!(
                "{} waits for your turn · ↑↓ choose · Esc hides the list",
                entry.name
            )
        }
        (Door::Slash, Some(entry)) if listing.whole => {
            format!(
                "Enter sends {} · ↑↓ choose · Esc hides the list",
                entry.name
            )
        }
        (Door::Slash, _) => "↑↓ choose · Tab or Enter inserts · Esc hides the list".to_owned(),
    };
    arrows(&own(&text, ascii), ascii)
}

/// The renderer's arrows in the glyph column in use.
fn arrows(text: &str, ascii: bool) -> String {
    if ascii {
        text.replace("↑↓", "Up/Down")
    } else {
        text.to_owned()
    }
}

/// `text` cut to `width` cells, the cut marked.
pub(super) fn fit(text: &str, width: usize, ascii: bool) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    let cut = if ascii { "..." } else { "…" };
    let room = width.saturating_sub(cut.width());
    let mut kept = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let cells = ch.width().unwrap_or(0);
        if used + cells > room {
            break;
        }
        used += cells;
        kept.push(ch);
    }
    kept.push_str(cut);
    kept
}

/// Paint the chooser into `area`: the context row when three rows or more
/// allow it, the entries around the selection, then the selected entry's
/// scope and help on the rows left for them (at most two).
pub(crate) fn render(state: &UiState, composer: &Composer, area: Rect, buf: &mut Buffer) {
    let Some(listing) = composer.listing() else {
        return;
    };
    if area.height == 0 || area.width == 0 {
        return;
    }
    let (ascii, color) = (state.ascii, state.color);
    let width = usize::from(area.width);
    let mut y = area.y;
    let mut left = area.height;
    if let Some(text) = context(state).filter(|_| left >= 3) {
        let tone = if state.busy.is_some() || state.waiting == Waiting::Free {
            Role::Accent
        } else {
            Role::Warn
        };
        let line = Line::styled(
            fit(&own(text, ascii), width, ascii),
            role::style(tone, color),
        );
        buf.set_line(area.x, y, &line, area.width);
        y += 1;
        left -= 1;
    }
    let help = detail(&listing, ascii);
    let wanted = super::content_rows(&[Line::raw(help.as_str())], area.width).clamp(1, DETAIL_ROWS);
    // The list keeps up to three entries before the detail takes a second row.
    let least = u16::try_from(listing.entries.len().clamp(1, 3)).unwrap_or(3);
    let detail_rows = if left >= least + 2 {
        u16::try_from(wanted).unwrap_or(1).min(left - least)
    } else {
        u16::from(left >= 2)
    };
    let list = usize::from(left - detail_rows);
    for line in &entry_lines(&listing, list, width, ascii, color) {
        buf.set_line(area.x, y, line, area.width);
        y += 1;
    }
    if detail_rows > 0 {
        let rows = Rect::new(area.x, area.bottom() - detail_rows, area.width, detail_rows);
        Paragraph::new(Line::styled(help, role::style(Role::Dim, color)))
            .wrap(Wrap { trim: false })
            .render(rows, buf);
    }
}

/// The rows of the listed entries, at most `rows` of them, the window kept
/// around the selection; a palette search that finds nothing says so.
fn entry_lines(
    listing: &Listing<'_>,
    rows: usize,
    width: usize,
    ascii: bool,
    color: bool,
) -> Vec<Line<'static>> {
    if rows == 0 {
        return Vec::new();
    }
    if listing.entries.is_empty() {
        let query = listing.query.unwrap_or_default();
        let said = own(
            &format!("nothing matches « {query} » · Backspace edits the search"),
            ascii,
        );
        let said = if ascii {
            said.replace("« ", "\"").replace(" »", "\"")
        } else {
            said
        };
        return vec![Line::styled(
            fit(&said, width, ascii),
            role::style(Role::Dim, color),
        )];
    }
    let shown = rows.min(listing.entries.len());
    let first = listing
        .selected
        .saturating_sub(shown - 1)
        .min(listing.entries.len() - shown);
    let name_width = listing
        .entries
        .iter()
        .map(|entry| entry.name.width())
        .max()
        .unwrap_or(0)
        .clamp(6, 14);
    (first..first + shown)
        .filter_map(|index| {
            let entry = listing.entries.get(index)?;
            Some(entry_line(
                entry,
                index == listing.selected,
                name_width,
                width,
                ascii,
                color,
            ))
        })
        .collect()
}

/// One entry: the selection mark, the name, what it does.
fn entry_line(
    entry: &Entry,
    selected: bool,
    name_width: usize,
    width: usize,
    ascii: bool,
    color: bool,
) -> Line<'static> {
    let mark = match (selected, ascii) {
        (true, false) => "› ",
        (true, true) => "> ",
        (false, _) => "  ",
    };
    let name = format!("{mark}{:<name_width$}  ", entry.name);
    let rest = fit(
        &own(&entry.effect, ascii),
        width.saturating_sub(name.width()),
        ascii,
    );
    let reverse = Style::default().add_modifier(Modifier::REVERSED);
    let (name_style, rest_style) = if selected {
        (
            role::style(Role::Strong, color).patch(reverse),
            Style::default().patch(reverse),
        )
    } else {
        (role::style(Role::Accent, color), Style::default())
    };
    let name = fit(&name, width, ascii);
    Line::from(vec![
        Span::styled(name, name_style),
        Span::styled(rest, rest_style),
    ])
}

/// The palette's search row: what was typed, then the cursor.
pub(crate) fn search_line(composer: &Composer, color: bool) -> Line<'static> {
    let query = composer
        .listing()
        .and_then(|listing| listing.query.map(str::to_owned))
        .unwrap_or_default();
    Line::from(vec![
        Span::styled(query, role::style(Role::Strong, color)),
        Span::styled(" ", Style::default().add_modifier(Modifier::REVERSED)),
    ])
}

/// The row naming a draft the palette set aside: its start, and how it
/// comes back.
pub(crate) fn aside_line(aside: &str, width: u16, ascii: bool, color: bool) -> Line<'static> {
    const SHOWN_CHARS: usize = 24;
    let words = aside.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut start: String = words.chars().take(SHOWN_CHARS).collect();
    let (open, close, cut) = if ascii {
        ("\"", "\"", "...")
    } else {
        ("« ", " »", "…")
    };
    if words.chars().count() > SHOWN_CHARS {
        start = format!("{}{cut}", start.trim_end());
    }
    let text = own(
        &format!("set aside: {open}{start}{close} · back once this line is sent · Esc: now"),
        ascii,
    );
    Line::styled(
        fit(&text, usize::from(width), ascii),
        role::style(Role::Dim, color),
    )
}
