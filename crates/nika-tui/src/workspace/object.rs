// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The object in view: the preview on the right of the workspace. An opened workflow shows
//! one face of the look its Session took (its source, its plan, its graph or
//! its check), as the viewers rendered it before the frame
//! ([`super::inspect::Inspected::face_lines`]). Any
//! other object is named by its kind's icon and its name with the lines it is
//! given, cut at the edge, never wrapped into a shape the object does not
//! have. With nothing open it welcomes with a compact butterfly above the
//! Session's first words; the conversation keeps the prominent region.

use std::time::Duration;

use nika_display::theme::Role;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use super::text::{fit_head, marks, wrap};
use crate::visual::icon::Icon;
use crate::visual::logomark::Size;
use crate::visual::role;

/// What the preview shows, as the Session projects it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Object {
    /// Nothing is open: the butterfly and the Session's first words.
    Welcome {
        /// The words under the mark, one per row.
        words: Vec<String>,
    },
    /// An open object, named by its kind and its name, with the lines to show.
    Shown {
        /// The kind of object (workflow, file, run...).
        icon: Icon,
        /// Its name.
        name: String,
        /// The lines of its content, top first.
        lines: Vec<String>,
    },
    /// A workflow opened from the listing: one face of the look its Session
    /// took, rendered by the viewers before the frame (never while drawing).
    Workflow {
        /// The title row: the icon, the name, the faces, the one in view marked.
        title: Line<'static>,
        /// The face's rows: the viewer's facts and notes, the bytes shown, the body.
        body: Vec<Line<'static>>,
    },
}

/// How the region is painted: the glyph column, colour and the reveal clock
/// the caller reads (this module never reads a clock).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Paint {
    /// Draw the ASCII twins instead of the Unicode glyphs.
    pub ascii: bool,
    /// Let roles carry their hues.
    pub color: bool,
    /// Time since the welcome reveal began.
    pub elapsed: Duration,
    /// Show the final mark at once, never a reveal.
    pub reduced_motion: bool,
}

/// The rows kept free between the mark and the words, and around them.
const GAP: u16 = 1;

/// The mark that fits the welcome of a `width` × `height` region above
/// `words` rows, with a blank row between them and a column of margin on each
/// side; none when even the smallest rendition does not fit.
#[must_use]
pub fn welcome_mark(width: u16, height: u16, words: usize) -> Option<Size> {
    let words = u16::try_from(words).unwrap_or(u16::MAX);
    let rows = height.saturating_sub(words.saturating_add(if words > 0 { GAP } else { 0 }));
    Size::largest_within(width.saturating_sub(2).min(Size::Compact.columns()), rows)
}

/// The lines of the region, `width` × `height` cells.
#[must_use]
pub fn lines(object: &Object, width: u16, height: u16, paint: Paint) -> Vec<Line<'static>> {
    lines_from(object, width, height, paint, 0)
}

/// The lines of the region with an open object scrolled to its line `scroll`
/// (the title row stays); the welcome never scrolls.
#[must_use]
pub fn lines_from(
    object: &Object,
    width: u16,
    height: u16,
    paint: Paint,
    scroll: usize,
) -> Vec<Line<'static>> {
    match object {
        Object::Welcome { words } => welcome(words, width, height, paint),
        Object::Shown { icon, name, lines } => {
            let from = scroll.min(lines.len());
            shown(*icon, name, &lines[from..], width, height, paint)
        }
        Object::Workflow { title, body } => {
            let from = scroll.min(body.len());
            let mut out = vec![title.clone()];
            out.extend(body[from..].iter().cloned());
            out.truncate(usize::from(height));
            out
        }
    }
}

/// The lines an open object holds under its title row (none for the
/// welcome): what a scroll runs over.
#[must_use]
pub fn length(object: &Object) -> usize {
    match object {
        Object::Welcome { .. } => 0,
        Object::Shown { lines, .. } => lines.len(),
        Object::Workflow { body, .. } => body.len(),
    }
}

/// The welcome: the mark (when one fits) then the words, the block centred.
fn welcome(words: &[String], width: u16, height: u16, paint: Paint) -> Vec<Line<'static>> {
    let (_, cut) = marks(paint.ascii);
    let words: Vec<String> = words
        .iter()
        .flat_map(|word| wrap(word, usize::from(width), cut))
        .collect();
    let mark = welcome_mark(width, height, words.len())
        .map(|size| size.at(paint.elapsed, paint.reduced_motion))
        .unwrap_or_default();
    let gap = usize::from(!mark.is_empty() && !words.is_empty());
    let used = mark.len() + gap + words.len();
    let top = usize::from(height).saturating_sub(used) / 2;
    let mut out = vec![Line::default(); top];
    out.extend(
        mark.into_iter()
            .map(|row| Line::styled(row.to_owned(), role::style(Role::Accent, paint.color))),
    );
    out.extend(std::iter::repeat_n(Line::default(), gap));
    // The words are what the human reads: the default foreground, no role.
    out.extend(
        words
            .iter()
            .map(|w| Line::from(fit_head(w, usize::from(width), cut))),
    );
    out.truncate(usize::from(height));
    out
}

/// An open object: its title row, then as many of its lines as fit.
fn shown(
    icon: Icon,
    name: &str,
    body: &[String],
    width: u16,
    height: u16,
    paint: Paint,
) -> Vec<Line<'static>> {
    let (_, cut) = marks(paint.ascii);
    let width = usize::from(width);
    let glyph = icon.glyph(paint.ascii);
    let head = format!("{glyph} ");
    let title = fit_head(name, width.saturating_sub(head.width()), cut);
    let mut out = vec![Line::from(vec![
        Span::styled(head, role::style(Role::Dim, paint.color)),
        Span::styled(title, role::style(Role::Strong, paint.color)),
    ])];
    let room = usize::from(height).saturating_sub(1);
    out.extend(
        body.iter()
            .take(room)
            .map(|row| Line::from(fit_head(row, width, cut))),
    );
    out
}

/// Draw the region into `area`.
pub fn render(object: &Object, area: Rect, buf: &mut Buffer, paint: Paint) {
    render_from(object, area, buf, paint, 0);
}

/// Draw the region into `area`, an open object scrolled to its line `scroll`.
pub fn render_from(object: &Object, area: Rect, buf: &mut Buffer, paint: Paint, scroll: usize) {
    let text = lines_from(object, area.width, area.height, paint, scroll);
    let alignment = match object {
        Object::Welcome { .. } => Alignment::Center,
        Object::Shown { .. } | Object::Workflow { .. } => Alignment::Left,
    };
    Paragraph::new(text).alignment(alignment).render(area, buf);
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::visual::logomark::REVEAL_ENDS;

    fn paint(ascii: bool) -> Paint {
        Paint {
            ascii,
            color: false,
            elapsed: REVEAL_ENDS,
            reduced_motion: false,
        }
    }

    fn text(lines: &[Line<'_>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    fn hello() -> Object {
        Object::Welcome {
            words: vec!["Describe the work you want to automate.".to_owned()],
        }
    }

    #[test]
    fn the_welcome_stays_compact_and_leaves_room_for_the_words() {
        // 80x24 stacked: the object keeps 11 rows; the composer comes first.
        assert_eq!(welcome_mark(80, 11, 1), Some(Size::Compact));
        // Extra preview space belongs to the content, never a larger brand mark.
        assert_eq!(welcome_mark(37, 38, 1), Some(Size::Compact));
        assert_eq!(welcome_mark(40, 20, 2), Some(Size::Compact));
        assert_eq!(welcome_mark(13, 9, 1), None);
        let rows = text(&lines(&hello(), 80, 11, paint(false)));
        assert_eq!(rows.len(), 10, "mark 8, gap 1, words 1: {rows:?}");
        let mark = Size::Compact.lines();
        let first = rows.iter().position(|r| r == mark[0]).expect("mark drawn");
        assert_eq!(&rows[first..first + mark.len()], mark.as_slice());
        assert_eq!(rows[first + mark.len()], "");
        assert_eq!(
            rows[first + mark.len() + 1],
            "Describe the work you want to automate."
        );
    }

    #[test]
    fn the_reveal_is_drawn_once_and_reduced_motion_shows_the_final_mark() {
        let at = |elapsed, reduced_motion| {
            let paint = Paint {
                ascii: false,
                color: false,
                elapsed,
                reduced_motion,
            };
            text(&lines(&hello(), 62, 38, paint))
        };
        let start = at(Duration::ZERO, false);
        let end = at(REVEAL_ENDS, false);
        assert_ne!(start, end, "the first frame is not the final mark");
        assert_eq!(at(Duration::ZERO, true), end);
        assert_eq!(at(Duration::from_secs(60), false), end);
    }

    #[test]
    fn a_region_too_small_for_any_mark_keeps_the_words() {
        let rows = text(&lines(&hello(), 30, 3, paint(true)));
        assert_eq!(rows.join(" "), "Describe the work you want to automate.");
        assert!(rows.iter().all(|row| row.width() <= 30));
    }

    #[test]
    fn an_open_object_is_named_by_its_kind_and_cut_at_the_edge() {
        let object = Object::Shown {
            icon: Icon::Workflow,
            name: "release.nika".to_owned(),
            lines: vec![
                "nika: release".to_owned(),
                "tasks:".to_owned(),
                "  gate: { invoke: { tool: \"nika:prompt\" } }".to_owned(),
            ],
        };
        let rows = text(&lines(&object, 24, 3, paint(false)));
        assert_eq!(rows, ["⑂ release.nika", "nika: release", "tasks:"]);
        let ascii = text(&lines(&object, 24, 4, paint(true)));
        assert_eq!(ascii[0], "[W] release.nika");
        assert_eq!(ascii[3], "  gate: { invoke: { t...");
        let scrolled = text(&lines_from(&object, 24, 3, paint(false), 1));
        assert_eq!(
            scrolled,
            ["⑂ release.nika", "tasks:", "  gate: { invoke: { too…"]
        );
        assert_eq!(length(&object), 3);
        assert_eq!(
            text(&lines_from(&object, 24, 3, paint(false), 9)),
            ["⑂ release.nika"]
        );
    }
}
