// SPDX-License-Identifier: AGPL-3.0-or-later AND MIT
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Windows over arbitrarily long text, measured in content rows, not terminal coordinates.
//! Ratatui's public Paragraph scroll offset is u16. Its private word wrapper is adapted
//! here for the one wrapping mode the conversation uses (`trim: false`), emitting rows
//! immediately instead of retaining every wrapped row of a long logical line.
//!
//! Wrapping algorithm adapted from ratatui-widgets 0.3.2, src/reflow.rs (MIT):
//! Copyright (c) 2016-2022 Florian Dehau
//! Copyright (c) 2023-2025 The Ratatui Developers
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.

use std::collections::VecDeque;

use ratatui::buffer::{Buffer, CellWidth};
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span, StyledGrapheme};

/// Measure the same wrapped stream without retaining off-screen rows.
pub(crate) fn height(lines: &[Line<'_>], width: u16) -> usize {
    let mut rows = 0usize;
    visit(lines, width.max(1), |_, _| {
        rows += 1;
        true
    });
    rows.max(1)
}

/// Draw only the visible rows. Neither the content offset nor height is narrowed to u16.
pub(crate) fn window(lines: &[Line<'_>], area: Rect, skip: usize, buffer: &mut Buffer) {
    if area.is_empty() {
        return;
    }
    let mut row = 0usize;
    let mut y = area.y;
    visit(lines, area.width, |graphemes, alignment| {
        if row >= skip {
            paint_row(
                graphemes,
                alignment,
                Rect::new(area.x, y, area.width, 1),
                buffer,
            );
            y = y.saturating_add(1);
        }
        row += 1;
        y < area.bottom()
    });
}

/// Paint a page of already wrapped rows without applying a second truncation policy.
pub(crate) fn paint_page(lines: &[Line<'_>], buffer: &mut Buffer) {
    let area = buffer.area;
    for (line, y) in lines.iter().zip(area.y..area.bottom()) {
        let glyphs = line.styled_graphemes(Style::default()).collect::<Vec<_>>();
        paint_row(
            &glyphs,
            line.alignment.unwrap_or(Alignment::Left),
            Rect::new(area.x, y, area.width, 1),
            buffer,
        );
    }
}

/// Match Paragraph's wrapped-line painter. A wide glyph whose start is in the last
/// column is kept by that painter; passing the row through `LineTruncator` would drop it.
fn paint_row(glyphs: &[StyledGrapheme<'_>], alignment: Alignment, area: Rect, buffer: &mut Buffer) {
    let width = glyphs
        .iter()
        .map(|g| usize::from(g.symbol.cell_width()))
        .sum::<usize>();
    let columns = usize::from(area.width);
    let mut x = match alignment {
        Alignment::Center => (columns / 2).saturating_sub(width / 2),
        Alignment::Right => columns.saturating_sub(width),
        Alignment::Left => 0,
    };
    for g in glyphs {
        let cells = usize::from(g.symbol.cell_width());
        if cells == 0 {
            continue;
        }
        if x >= columns {
            break;
        }
        let column = area
            .x
            .saturating_add(u16::try_from(x).unwrap_or(area.width));
        if let Some(cell) = buffer.cell_mut((column, area.y)) {
            cell.set_symbol(g.symbol).set_style(g.style);
        }
        x += cells;
    }
}

/// Inline scrollback uses bounded terminal buffers, with no omitted tail or rewrapping.
/// The visitor can fail once; it is never retried and no later page is emitted.
pub(crate) fn pages<'a, E>(
    lines: &'a [Line<'_>],
    width: u16,
    height: u16,
    mut draw: impl FnMut(&[Line<'a>]) -> Result<(), E>,
) -> Result<(), E> {
    if lines.is_empty() {
        return draw(&[Line::default()]);
    }
    let mut page = Vec::new();
    let mut error = None;
    visit(lines, width, |graphemes, alignment| {
        page.push(as_line(graphemes, alignment));
        if page.len() == usize::from(height.max(1)) {
            if let Err(cause) = draw(&page) {
                error = Some(cause);
                return false;
            }
            page.clear();
        }
        true
    });
    if let Some(error) = error {
        return Err(error);
    }
    if !page.is_empty() {
        draw(&page)?;
    }
    Ok(())
}

fn as_line<'a>(graphemes: &[StyledGrapheme<'a>], alignment: Alignment) -> Line<'a> {
    Line::from(
        graphemes
            .iter()
            .map(|g| Span::styled(g.symbol, g.style))
            .collect::<Vec<_>>(),
    )
    .alignment(alignment)
}

fn visit<'a>(
    lines: &'a [Line<'_>],
    width: u16,
    mut row: impl FnMut(&[StyledGrapheme<'a>], Alignment) -> bool,
) {
    if width == 0 {
        return;
    }
    for line in lines {
        let alignment = line.alignment.unwrap_or(Alignment::Left);
        if !reflow(line, usize::from(width), &mut |g| row(g, alignment)) {
            break;
        }
    }
}

/// Keep only the current row, pending word and whitespace. Width arithmetic uses usize,
/// so adding a wide grapheme at the largest terminal width cannot overflow either.
fn reflow<'a>(
    line: &'a Line<'_>,
    width: usize,
    row: &mut impl FnMut(&[StyledGrapheme<'a>]) -> bool,
) -> bool {
    let (mut current, mut word) = (Vec::new(), Vec::new());
    let mut spaces = VecDeque::<StyledGrapheme<'a>>::new();
    let (mut used, mut word_width, mut space_width) = (0usize, 0usize, 0usize);
    let (mut was_word, mut emitted) = (false, false);
    for g in line.styled_graphemes(Style::default()) {
        let whitespace = g.is_whitespace();
        let cells = usize::from(g.symbol.cell_width());
        if cells > width {
            continue;
        }
        if (was_word && whitespace)
            || (current.is_empty() && word_width + space_width + cells > width)
        {
            current.extend(spaces.drain(..));
            current.append(&mut word);
            used += space_width + word_width;
            (space_width, word_width) = (0, 0);
        }
        if used >= width || (cells > 0 && used + space_width + word_width >= width) {
            if !row(&current) {
                return false;
            }
            emitted = true;
            current.clear();
            let mut remaining = width.saturating_sub(used);
            used = 0;
            while let Some(g) = spaces.front() {
                let cells = usize::from(g.symbol.cell_width());
                if cells > remaining {
                    break;
                }
                space_width -= cells;
                remaining -= cells;
                spaces.pop_front();
            }
            if whitespace && spaces.is_empty() {
                continue;
            }
        }
        if whitespace {
            space_width += cells;
            spaces.push_back(g);
        } else {
            word_width += cells;
            word.push(g);
        }
        was_word = !whitespace;
    }
    current.extend(spaces);
    current.extend(word);
    if !current.is_empty() || !emitted {
        return row(&current);
    }
    true
}

#[cfg(test)]
mod tests;
