// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A conversation reply's Markdown, read as a person reads it: one line of spans for each line
//! of the reply, so a host measures and paints the reply's own lines. Strong and emphasis lose
//! their markers where a weight or a slant carries them, a code span keeps its backticks (the
//! only sign of code once colour is gone), a link reads as its text and its target, a heading
//! as its title in weight, a list item behind a bullet (`•`, `-` in the ASCII column), and a
//! simple table as aligned columns, its separator row a rule. A table keeps one line for each
//! of its rows within the row it is given ([`lines_within`]): where its columns are wider, the
//! widest narrow and a cell wider than its column is cut on the cut mark, so a row of the table
//! never wraps. Fenced code and anything inside it stay as written, between quiet rules where
//! its fences stood, the first naming the code's language when the fence does (live gauntlet
//! 5, r08 B12: a file's content read framed by its literal fence lines). Nothing else is dropped:
//! every word of the reply outside a cut cell and a fence's backticks is painted. Under
//! `NO_COLOR` the weights and the marks still read; the ASCII column draws ASCII bullets, rules
//! and cut marks.

use ratatui::text::Span;
use unicode_width::UnicodeWidthStr;

use crate::Canvas;
use crate::cells::{cut_mark, fit, paint, plain};
use crate::markdown::{inline, list_item, opens_fence};
use nika_display::theme::Role;

/// The spans of each line of the reply `text`, in order: as many lines as `text` holds, its
/// tables at their natural width.
#[must_use]
pub fn lines(text: &str, color: bool, ascii: bool) -> Vec<Vec<Span<'static>>> {
    lines_within(text, color, ascii, usize::MAX)
}

/// The spans of each line of the reply `text` painted in rows `width` cells wide: the same
/// lines as [`lines`], but a table wider than `width` narrows its widest columns and cuts the
/// cells wider than their column, each cut cell ending on the cut mark, so none of its rows is
/// wider than `width` and none wraps. Prose and code keep their words for the host to wrap.
#[must_use]
pub fn lines_within(text: &str, color: bool, ascii: bool, width: usize) -> Vec<Vec<Span<'static>>> {
    let raw: Vec<&str> = text.lines().collect();
    let mut out = Vec::with_capacity(raw.len());
    let mut fence: Option<(char, usize)> = None;
    let mut at = 0;
    while let Some(line) = raw.get(at) {
        let trimmed = line.trim_start();
        if let Some((c, n)) = fence {
            let closes = opens_fence(trimmed).is_some_and(|(x, m)| x == c && m >= n)
                && trimmed.trim_start_matches(c).trim().is_empty();
            if closes {
                fence = None;
                out.push(vec![fence_rule("", color, ascii)]);
            } else {
                out.push(vec![plain((*line).to_owned())]);
            }
            at += 1;
            continue;
        }
        if let Some(open) = opens_fence(trimmed) {
            fence = Some(open);
            let info = trimmed.trim_start_matches(open.0).trim();
            out.push(vec![fence_rule(info, color, ascii)]);
            at += 1;
            continue;
        }
        let rows = table_rows(&raw[at..]);
        if rows > 0 {
            out.extend(table(&raw[at..at + rows], color, ascii, width));
            at += rows;
            continue;
        }
        out.push(line_spans(line, color, ascii));
        at += 1;
    }
    out
}

/// The quiet rule a code fence reads as, naming the code's language (`info`) when the fence does.
fn fence_rule(info: &str, color: bool, ascii: bool) -> Span<'static> {
    let rule = if ascii { "---" } else { "───" };
    let words = if info.is_empty() {
        rule.to_owned()
    } else {
        format!("{rule} {info}")
    };
    paint(words, Role::Dim, color)
}

/// One line that is no table and no code: a heading, a list item, a quote, or prose.
fn line_spans(line: &str, color: bool, ascii: bool) -> Vec<Span<'static>> {
    let indent = &line[..line.len() - line.trim_start().len()];
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
        let mut spans = vec![plain(indent.to_owned())];
        spans.extend(
            inline(trimmed[hashes..].trim(), color)
                .into_iter()
                .map(|span| {
                    let style = span.style.patch(crate::role::style(Role::Strong, color));
                    Span::styled(span.content, style)
                }),
        );
        return spans;
    }
    if let Some((marker, text)) = list_item(trimmed, ascii) {
        let mut spans = vec![
            plain(indent.to_owned()),
            paint(format!("{marker} "), Role::Dim, color),
        ];
        spans.extend(inline(text, color));
        return spans;
    }
    if let Some(rest) = trimmed.strip_prefix("> ") {
        let mut spans = vec![plain(indent.to_owned()), paint("> ", Role::Dim, color)];
        spans.extend(inline(rest, color));
        return spans;
    }
    inline(line, color)
}

/// How many lines from the first of `lines` form a simple table: a row of cells between pipes,
/// a separator row of dashes right under it, then the rows that follow; none when the first two
/// lines are not such a header and separator.
fn table_rows(lines: &[&str]) -> usize {
    let row = |line: &&str| {
        let trimmed = line.trim();
        trimmed.starts_with('|') && trimmed.len() > 1 && trimmed.ends_with('|')
    };
    match lines {
        [head, rule, ..] if row(head) && row(rule) && separator(rule) => {
            2 + lines[2..].iter().take_while(|line| row(line)).count()
        }
        _ => 0,
    }
}

/// Whether `line` is a table's separator row: cells of dashes, colons allowed at their ends.
fn separator(line: &str) -> bool {
    let cells = cells(line);
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let dashes = cell.trim_matches(':');
            dashes.len() >= 3 && dashes.chars().all(|c| c == '-')
        })
}

/// The cells of a table row, trimmed, the outer pipes aside.
fn cells(line: &str) -> Vec<String> {
    let inner = line.trim().trim_start_matches('|').trim_end_matches('|');
    inner
        .split('|')
        .map(|cell| cell.trim().to_owned())
        .collect()
}

/// The cells `spans` take.
fn wide(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|span| span.content.width()).sum()
}

/// The table `rows` (a header, its separator, its body) as aligned columns within `width`
/// cells: each cell's inline marks read, padded to its column's width, the separator a rule,
/// the header in weight. Columns keep their natural width where the table fits; where it does
/// not, they narrow ([`narrowed`]) and a cell wider than its column is cut on the cut mark;
/// where even their narrowest widths do not fit, each row is cut at `width`. No row is wider
/// than `width`: a row of the table never wraps.
fn table(rows: &[&str], color: bool, ascii: bool, width: usize) -> Vec<Vec<Span<'static>>> {
    let parsed: Vec<Vec<Vec<Span<'static>>>> = (rows.iter())
        .map(|row| cells(row).iter().map(|cell| inline(cell, color)).collect())
        .collect();
    let columns = parsed.iter().map(Vec::len).max().unwrap_or(0);
    let natural: Vec<usize> = (0..columns)
        .map(|c| {
            (parsed.iter().enumerate())
                .filter(|(at, _)| *at != 1)
                .filter_map(|(_, row)| row.get(c))
                .map(|cell| wide(cell))
                .max()
                .unwrap_or(0)
        })
        .collect();
    let (bar, cross, dash) = if ascii {
        (" | ", "-+-", "-")
    } else {
        (" │ ", "─┼─", "─")
    };
    let bars = bar.width() * columns.saturating_sub(1);
    // A narrowed column keeps one character before its cut mark.
    let least = cut_mark(ascii).width() + 1;
    let widths = narrowed(&natural, width.saturating_sub(bars), least);
    let canvas = Canvas::new(u16::try_from(width).unwrap_or(u16::MAX), ascii, color);
    let cut = |spans: Vec<Span<'static>>, room: usize| fit(spans, room, false, canvas).0.spans;
    let mut out = Vec::with_capacity(rows.len());
    for (at, row) in parsed.iter().enumerate() {
        if at == 1 {
            let rule: Vec<String> = widths.iter().map(|w| dash.repeat((*w).max(1))).collect();
            // A rule cut at the row's end needs no cut mark: it is no word.
            let rule: String = rule.join(cross).chars().take(width).collect();
            out.push(vec![paint(rule, Role::Dim, color)]);
            continue;
        }
        let mut spans = Vec::new();
        for (c, column) in widths.iter().enumerate() {
            if c > 0 {
                spans.push(paint(bar, Role::Dim, color));
            }
            let cell: Vec<Span<'static>> = (row.get(c).cloned().unwrap_or_default().into_iter())
                .map(|span| {
                    if at == 0 {
                        let style = span.style.patch(crate::role::style(Role::Strong, color));
                        Span::styled(span.content, style)
                    } else {
                        span
                    }
                })
                .collect();
            let cell = if wide(&cell) > *column {
                cut(cell, *column)
            } else {
                cell
            };
            let pad = column.saturating_sub(wide(&cell));
            spans.extend(cell);
            if pad > 0 && c + 1 < columns {
                spans.push(plain(" ".repeat(pad)));
            }
        }
        if wide(&spans) > width {
            spans = cut(spans, width);
        }
        out.push(spans);
    }
    out
}

/// The widths of a table's columns whose cells need `natural` cells, within `room` cells (its
/// bars aside): the natural widths where they fit; else the widest columns narrowed to one
/// width, the narrower ones whole, the cells left over shared out from the left, a narrowed
/// column never under `least` cells. Their sum fits `room` unless even `least` cells for each
/// narrowed column do not.
fn narrowed(natural: &[usize], room: usize, least: usize) -> Vec<usize> {
    let capped = |cap: usize| natural.iter().map(|w| (*w).min(cap)).sum::<usize>();
    let widest = natural.iter().copied().max().unwrap_or(0);
    if capped(widest) <= room {
        return natural.to_vec();
    }
    // The widest cap that fits: `low` fits (or is the least), `high` does not.
    let (mut low, mut high) = (least.min(widest), widest);
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        if capped(mid) <= room {
            low = mid;
        } else {
            high = mid;
        }
    }
    let mut widths: Vec<usize> = natural.iter().map(|w| (*w).min(low)).collect();
    let mut spare = room.saturating_sub(capped(low));
    for (width, natural) in widths.iter_mut().zip(natural) {
        if spare == 0 {
            break;
        }
        if *width < *natural {
            *width += 1;
            spare -= 1;
        }
    }
    widths
}

#[cfg(test)]
mod tests;
