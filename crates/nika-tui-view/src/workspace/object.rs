// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The object in view: the preview on the right of the workspace. An opened workflow shows
//! one face of the look its Session took (its source, its plan, its graph or
//! its check), as the viewers rendered it before the frame (the renderer's
//! own inspection hands those lines over through `Inspected::face_lines`). Any
//! other object is named by its kind's icon and its name with the lines it is
//! given, cut at the edge, never wrapped into a shape the object does not
//! have. With nothing open it welcomes with a compact butterfly above the
//! Session's first words; the conversation keeps the prominent region. The
//! butterfly wears the brand's ice; while it is revealed, what a frame has
//! just lit glows until the next one, and the last frame has settled to ice.

use std::time::Duration;

use nika_display::theme::Role;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use super::text::{fit_head, marks, wrap};
use crate::visual::icon::Icon;
use crate::visual::logomark::{self, Glyphs, REVEAL_AT, Size, WORDMARK};
use crate::visual::role::{self, Depth, Tone};

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
    Size::largest_within(width.saturating_sub(2), rows)
}

/// The lines of the region, `width` × `height` cells.
#[must_use]
pub fn lines(object: &Object, width: u16, height: u16, paint: Paint) -> Vec<Line<'static>> {
    lines_from(object, width, height, paint, 0)
}

/// The lines of the region with an open object scrolled to its line `scroll`
/// (the title row stays); the welcome never scrolls. The brand's tones take
/// the depth the palette assumes ([`Depth::of_palette`]).
#[must_use]
pub fn lines_from(
    object: &Object,
    width: u16,
    height: u16,
    paint: Paint,
    scroll: usize,
) -> Vec<Line<'static>> {
    lines_at(
        object,
        (width, height),
        paint,
        scroll,
        Depth::of_palette(paint.color),
    )
}

/// [`lines_from`] with the brand's tones at `depth`.
fn lines_at(
    object: &Object,
    (width, height): (u16, u16),
    paint: Paint,
    scroll: usize,
    depth: Depth,
) -> Vec<Line<'static>> {
    if let Object::Welcome { words } = object {
        return welcome(words, width, height, paint, depth);
    }
    let total = length(object);
    let rows = content_rows(total, height);
    let visible = usize::from(rows);
    let from = scroll.min(total.saturating_sub(visible));
    let mut out = match object {
        Object::Welcome { .. } => Vec::new(),
        Object::Shown { icon, name, lines } => {
            shown(*icon, name, &lines[from..], width, rows + 1, paint)
        }
        Object::Workflow { title, body } => {
            let mut out = vec![title.clone()];
            out.extend(body[from..].iter().take(visible).cloned());
            out
        }
    };
    if height >= 3 && total > usize::from(height - 1) {
        out.push(continuation(from > 0, from + visible < total, width, paint));
    }
    out.truncate(usize::from(height));
    out
}

/// Rows available to an object's content under its title. Overflow reserves
/// one row for a continuation cue; scrolling and following a picked task
/// use the same budget as painting. Very short regions keep the content.
#[must_use]
pub fn content_rows(lines: usize, height: u16) -> u16 {
    let rows = height.saturating_sub(1);
    rows.saturating_sub(u16::from(height >= 3 && lines > usize::from(rows)))
}

fn continuation(above: bool, below: bool, width: u16, paint: Paint) -> Line<'static> {
    let words = match (above, below, paint.ascii) {
        (true, true, true) => "^ Above / v Below - scroll",
        (true, true, false) => "↑ Above · ↓ Below · scroll",
        (true, false, true) => "^ More above - scroll",
        (true, false, false) => "↑ More above · scroll",
        (_, _, true) => "v More below - scroll",
        (_, _, false) => "↓ More below · scroll",
    };
    Line::styled(
        fit_head(words, usize::from(width), marks(paint.ascii).1),
        role::style(Role::Dim, paint.color),
    )
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
fn welcome(
    words: &[String],
    width: u16,
    height: u16,
    paint: Paint,
    depth: Depth,
) -> Vec<Line<'static>> {
    let (_, cut) = marks(paint.ascii);
    let depth = if paint.color { depth } else { Depth::None };
    let words = welcome_words(words, width, height, cut);
    let mark = welcome_mark(width, height, words.len())
        .map(|size| mark_lines(size, paint, depth))
        .unwrap_or_default();
    let gap = usize::from(!mark.is_empty() && !words.is_empty());
    let used = mark.len() + gap + words.len();
    let top = usize::from(height).saturating_sub(used) / 2;
    let mut out = vec![Line::default(); top];
    out.extend(mark);
    out.extend(std::iter::repeat_n(Line::default(), gap));
    // The words are what the human reads: the default foreground, no role.
    // The wordmark under the mark completes the lockup: the name strong,
    // its dot in the exec hue.
    out.extend(words.iter().map(|word| {
        if word == WORDMARK {
            return logomark::wordmark(role::style(Role::Strong, paint.color), depth);
        }
        let hue = if word.starts_with(['1', '2', '3']) {
            Role::Accent
        } else if word.starts_with("Try:") {
            Role::VerbInvoke
        } else {
            Role::Dim
        };
        Line::styled(
            fit_head(word, usize::from(width), cut),
            role::style(hue, paint.color),
        )
    }));
    out.truncate(usize::from(height));
    out
}

/// The butterfly's rows at its reveal's clock, in braille or, in the ASCII
/// column, the density ramp; each cell in the mark's ice. Before the last
/// frame, a cell its frame has just lit (or filled further) glows. The last
/// frame, held from then on and drawn at once under reduced motion, has
/// settled to ice. A blank cell keeps the run it falls in, so a row changes
/// style only where a glyph does.
fn mark_lines(size: Size, paint: Paint, depth: Depth) -> Vec<Line<'static>> {
    let glyphs = Glyphs::of(paint.ascii);
    let index = logomark::frame_at(paint.elapsed, paint.reduced_motion);
    let rows = size.frame(glyphs, index);
    let settled = index + 1 >= REVEAL_AT.len() || depth == Depth::None;
    let before = if settled || index == 0 {
        Vec::new()
    } else {
        size.frame(glyphs, index - 1)
    };
    let ice = role::tone(Tone::Ice, depth);
    rows.iter()
        .enumerate()
        .map(|(y, row)| {
            if settled {
                return Line::from(Span::styled(*row, ice));
            }
            let mut was = before.get(y).map_or("", |was| *was).chars();
            let mut spans: Vec<Span<'static>> = Vec::new();
            let mut run = String::new();
            let mut lit = false;
            for glyph in row.chars() {
                let prior = was.next().unwrap_or(' ');
                let glows = glyph != prior;
                if glyph != ' ' && glows != lit && !run.is_empty() {
                    let tone = if lit { Tone::Glow } else { Tone::Ice };
                    spans.push(Span::styled(
                        std::mem::take(&mut run),
                        role::tone(tone, depth),
                    ));
                }
                if glyph != ' ' {
                    lit = glows;
                }
                run.push(glyph);
            }
            let tone = if lit { Tone::Glow } else { Tone::Ice };
            spans.push(Span::styled(run, role::tone(tone, depth)));
            Line::from(spans)
        })
        .collect()
}

/// In a short viewport the launch guide keeps its actions ahead of decoration.
/// Other Welcome objects keep their supplied words; no project is observed here.
fn welcome_words(words: &[String], width: u16, height: u16, cut: &str) -> Vec<String> {
    let width = usize::from(width);
    let full: Vec<_> = words
        .iter()
        .flat_map(|word| wrap(word, width, cut))
        .collect();
    if full.len() <= usize::from(height) {
        return full;
    }
    let mut steps: Vec<_> = words
        .iter()
        .filter(|word| {
            ["1  ", "2  ", "3  "]
                .iter()
                .any(|prefix| word.starts_with(*prefix))
        })
        .map(|word| fit_head(word, width, cut))
        .collect();
    if steps.len() != 3 {
        return full;
    }
    for prefix in ["To prepare", "Ways:"] {
        if let Some(word) = words.iter().find(|word| word.starts_with(prefix)) {
            steps.push(fit_head(word, width, cut));
        }
    }
    steps
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

/// Draw the region into `area`, the brand's tones at the palette's depth.
pub fn render(object: &Object, area: Rect, buf: &mut Buffer, paint: Paint) {
    render_from(
        object,
        area,
        buf,
        (paint, Depth::of_palette(paint.color)),
        0,
    );
}

/// Draw the region into `area`, an open object scrolled to its line `scroll`,
/// the brand's tones at `depth` (the terminal's, as its caller read it).
pub fn render_from(
    object: &Object,
    area: Rect,
    buf: &mut Buffer,
    (paint, depth): (Paint, Depth),
    scroll: usize,
) {
    let text = lines_at(object, (area.width, area.height), paint, scroll, depth);
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
        assert_eq!(welcome_mark(37, 38, 1), Some(Size::Launch));
        assert_eq!(welcome_mark(40, 20, 2), Some(Size::Launch));
        assert_eq!(welcome_mark(13, 9, 1), None);
        let rows = text(&lines(&hello(), 80, 11, paint(false)));
        assert_eq!(rows.len(), 10, "mark 8, gap 1, words 1: {rows:?}");
        // The Unicode column draws the braille rendition.
        let mark = Size::Compact.lines(Glyphs::Braille);
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

    /// Each cell of `lines` with the style its span wears.
    fn toned(lines: &[Line<'_>]) -> Vec<Vec<(char, ratatui::style::Style)>> {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .flat_map(|span| span.content.chars().map(move |glyph| (glyph, span.style)))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn what_a_frame_lights_glows_and_the_last_frame_settles_to_ice() {
        for ascii in [true, false] {
            lights_then_settles(ascii);
        }
    }

    /// The reveal's tones in one glyph column.
    fn lights_then_settles(ascii: bool) {
        let size = Size::Launch;
        let glyphs = Glyphs::of(ascii);
        let ice = role::tone(Tone::Ice, Depth::Rgb);
        let glow = role::tone(Tone::Glow, Depth::Rgb);
        let colour = |elapsed, reduced_motion| Paint {
            ascii,
            color: true,
            elapsed,
            reduced_motion,
        };
        let mut lit = 0;
        for (index, at) in REVEAL_AT.into_iter().enumerate() {
            let cells = toned(&mark_lines(size, colour(at, false), Depth::Rgb));
            let now = size.frame(glyphs, index);
            let before = if index == 0 {
                vec![""; now.len()]
            } else {
                size.frame(glyphs, index - 1)
            };
            let last = index + 1 == REVEAL_AT.len();
            for ((row, was), cells) in now.iter().zip(&before).zip(&cells) {
                assert_eq!(&cells.iter().map(|(g, _)| *g).collect::<String>(), row);
                let mut prior = was.chars();
                for (glyph, style) in cells {
                    let old = prior.next().unwrap_or(' ');
                    if *glyph == ' ' {
                        continue;
                    }
                    let new = !last && *glyph != old;
                    lit += usize::from(new);
                    assert_eq!(
                        *style,
                        if new { glow } else { ice },
                        "frame {index}: {row:?}"
                    );
                }
            }
        }
        assert!(lit > 0, "the reveal lit nothing");
        // Under reduced motion the settled mark is drawn at once.
        let still = toned(&mark_lines(size, colour(Duration::ZERO, true), Depth::Rgb));
        assert!(still.iter().flatten().all(|(_, style)| *style == ice));
        // A smaller palette takes its nearest slots; without colour, no hue.
        let first = toned(&mark_lines(
            size,
            colour(Duration::ZERO, false),
            Depth::Ansi256,
        ));
        let glows = role::tone(Tone::Glow, Depth::Ansi256);
        assert!(
            first
                .iter()
                .flatten()
                .all(|(glyph, style)| *glyph == ' ' || *style == glows)
        );
        let plain = Paint {
            color: false,
            ..colour(Duration::ZERO, false)
        };
        let rows = lines(&hello(), 62, 38, plain);
        assert!(
            toned(&rows)
                .iter()
                .flatten()
                .all(|(_, style)| style.fg.is_none())
        );
    }

    #[test]
    fn the_welcome_mark_wears_the_terminal_depth_it_is_given() {
        let settled = Paint {
            ascii: true,
            color: true,
            elapsed: REVEAL_ENDS,
            reduced_motion: false,
        };
        let mark = Size::Board.lines(Glyphs::Ramp);
        let wears = |depth| {
            let rows = lines_at(&hello(), (62, 38), settled, 0, depth);
            let row = rows
                .iter()
                .find(|line| line.to_string() == mark[mark.len() / 2])
                .expect("the mark is drawn");
            row.spans
                .iter()
                .map(|span| span.style.fg)
                .collect::<Vec<_>>()
        };
        assert_eq!(wears(Depth::Rgb), [role::tone(Tone::Ice, Depth::Rgb).fg]);
        assert_eq!(
            wears(Depth::Ansi16),
            [Some(ratatui::style::Color::LightCyan)]
        );
        assert_eq!(wears(Depth::None), [None]);
        // The palette's own depth is 24-bit wherever colour is allowed.
        assert_eq!(
            lines(&hello(), 62, 38, settled),
            lines_at(&hello(), (62, 38), settled, 0, Depth::Rgb)
        );
    }

    #[test]
    fn the_wordmark_under_the_mark_completes_the_lockup() {
        use ratatui::style::{Color, Modifier};
        let object = Object::Welcome {
            words: vec![
                WORDMARK.to_owned(),
                "Turn an intention into a workflow.".to_owned(),
            ],
        };
        for (ascii, color, depth, dot) in [
            (false, true, Depth::Rgb, Some(Color::Rgb(0xff, 0x7a, 0x3c))),
            (false, true, Depth::Ansi256, Some(Color::Indexed(209))),
            (true, true, Depth::Ansi16, Some(Color::LightYellow)),
            (true, false, Depth::Rgb, None),
        ] {
            let paint = Paint {
                ascii,
                color,
                elapsed: REVEAL_ENDS,
                reduced_motion: false,
            };
            let rows = lines_at(&object, (62, 38), paint, 0, depth);
            let at = rows
                .iter()
                .position(|line| line.to_string() == WORDMARK)
                .expect("the wordmark");
            let line = &rows[at];
            assert_eq!(line.spans[0].content, "nika");
            assert!(line.spans[0].style.add_modifier.contains(Modifier::BOLD));
            assert_eq!(
                (line.spans[1].content.as_ref(), line.spans[1].style.fg),
                (".", dot)
            );
            // Under the mark, one blank row between them.
            assert_eq!(rows[at - 1].to_string(), "");
            let mark = Size::Board.lines(Glyphs::of(ascii));
            assert_eq!(rows[at - 2].to_string(), mark[mark.len() - 1]);
        }
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
        assert_eq!(
            rows,
            ["⑂ release.nika", "nika: release", "↓ More below · scroll"]
        );
        let ascii = text(&lines(&object, 24, 4, paint(true)));
        assert_eq!(ascii[0], "[W] release.nika");
        assert_eq!(ascii[3], "  gate: { invoke: { t...");
        let scrolled = text(&lines_from(&object, 24, 3, paint(false), 1));
        assert_eq!(
            scrolled,
            ["⑂ release.nika", "tasks:", "↑ Above · ↓ Below · scr…"]
        );
        assert_eq!(length(&object), 3);
        assert_eq!(
            text(&lines_from(&object, 24, 3, paint(false), 9)),
            [
                "⑂ release.nika",
                "  gate: { invoke: { too…",
                "↑ More above · scroll"
            ]
        );
    }

    #[test]
    fn an_overflowing_object_names_its_continuation_and_keeps_the_last_row_reachable() {
        let object = Object::Workflow {
            title: Line::raw("notes.nika"),
            body: (0..6).map(|n| Line::raw(format!("row {n}"))).collect(),
        };
        for ascii in [false, true] {
            let first = text(&lines(&object, 40, 4, paint(ascii)));
            assert_eq!(&first[..3], ["notes.nika", "row 0", "row 1"]);
            assert!(first[3].contains("More below"), "{first:?}");
            let last = text(&lines_from(&object, 40, 4, paint(ascii), 4));
            assert_eq!(&last[..3], ["notes.nika", "row 4", "row 5"]);
            assert!(last[3].contains("More above"), "{last:?}");
            assert!(!last[3].contains("below"));
            assert!(first.iter().chain(&last).all(|row| row.width() <= 40));
            if ascii {
                assert!(first.iter().chain(&last).all(|row| row.is_ascii()));
            }
        }
        assert_eq!(text(&lines(&object, 40, 7, paint(true))).len(), 7);
    }
}
