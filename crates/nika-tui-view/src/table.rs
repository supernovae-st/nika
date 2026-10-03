// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! CSV and TSV as an aligned table. Quoting follows RFC 4180 (a quoted
//! cell may hold the delimiter, a doubled quote or a line break, drawn as
//! its visible mark); the delimiter is the one the first line uses most;
//! the first row is drawn as the header, numeric columns align right, and
//! columns narrow to the width before the last ones are dropped and
//! counted. Every row is read and counted; only the first are kept.

use nika_display::theme::Role;
use ratatui::text::Span;

use super::cells::{self, Sheet, Tally, paint, plain};
use super::{Note, mask, secret};

/// The widest a column grows, in cells.
const WIDEST: usize = 40;
/// The narrowest a column shrinks before columns are dropped.
const NARROWEST: usize = 4;
/// Cells kept per row.
const COLUMNS: usize = 64;
/// Characters kept per cell.
const CELL_CHARS: usize = 256;

/// The delimiter the first line uses most outside quotes (a comma on a
/// tie or when it uses none).
fn delimiter(text: &str) -> char {
    let first = text.lines().next().unwrap_or_default();
    let mut counts = [(',', 0usize), (';', 0), ('\t', 0), ('|', 0)];
    let mut quoted = false;
    for c in first.chars() {
        if c == '"' {
            quoted = !quoted;
        } else if !quoted {
            for (d, n) in &mut counts {
                if c == *d {
                    *n += 1;
                }
            }
        }
    }
    let mut best = (',', 0);
    for (d, n) in counts {
        if n > best.1 {
            best = (d, n);
        }
    }
    best.0
}

/// The rows read: the first `keep` stored, all counted.
#[derive(Debug, Default)]
struct Records {
    rows: Vec<Vec<String>>,
    total: usize,
    fewest: usize,
    most: usize,
}

impl Records {
    fn end_row(&mut self, row: &mut Vec<String>, cells: &mut usize, keep: usize) {
        self.total += 1;
        self.fewest = if self.total == 1 {
            *cells
        } else {
            self.fewest.min(*cells)
        };
        self.most = self.most.max(*cells);
        let row = std::mem::take(row);
        if self.rows.len() < keep {
            self.rows.push(row);
        }
        *cells = 0;
    }
}

/// Close one cell: kept while the row has room, counted always.
fn end_cell(row: &mut Vec<String>, cell: &mut String, count: &mut usize) {
    if row.len() < COLUMNS {
        row.push(std::mem::take(cell));
    }
    cell.clear();
    *count += 1;
}

/// Read the records of `text`.
fn records(text: &str, delim: char, keep: usize) -> Records {
    let mut out = Records::default();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut kept = 0usize;
    let mut count = 0usize;
    let mut fresh = true;
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' && chars.peek() == Some(&'"') {
                chars.next();
            } else if c == '"' {
                quoted = false;
                continue;
            }
        } else if c == '"' && fresh {
            quoted = true;
            fresh = false;
            continue;
        } else if c == delim {
            end_cell(&mut row, &mut cell, &mut count);
            (fresh, kept) = (true, 0);
            continue;
        } else if c == '\r' && chars.peek() == Some(&'\n') {
            continue;
        } else if c == '\n' {
            end_cell(&mut row, &mut cell, &mut count);
            out.end_row(&mut row, &mut count, keep);
            (fresh, kept) = (true, 0);
            continue;
        }
        fresh = false;
        if kept < CELL_CHARS {
            cell.push(c);
            kept += 1;
        }
    }
    if !fresh || !row.is_empty() {
        end_cell(&mut row, &mut cell, &mut count);
        out.end_row(&mut row, &mut count, keep);
    }
    out
}

/// The column widths that fit `width` cells, `sep` cells between columns:
/// natural widths (at most [`WIDEST`]), the widest narrowed first, then the
/// last columns dropped.
fn layout(rows: &[Vec<String>], width: usize, sep: usize) -> Vec<usize> {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut widths: Vec<usize> = (0..columns)
        .map(|c| {
            rows.iter()
                .filter_map(|r| r.get(c))
                .map(|s| cells::width(s))
                .max()
                .unwrap_or(0)
                .clamp(1, WIDEST)
        })
        .collect();
    let total = |w: &[usize]| w.iter().sum::<usize>() + sep * w.len().saturating_sub(1);
    while total(&widths) > width {
        let widest = widths
            .iter()
            .enumerate()
            .filter(|(_, w)| **w > NARROWEST)
            .max_by_key(|(_, w)| **w)
            .map(|(i, _)| i);
        match widest {
            Some(i) => widths[i] -= 1,
            None if widths.len() > 1 => {
                widths.pop();
            }
            None => break,
        }
    }
    widths
}

/// Whether column `c` holds numbers below the header (and at least one).
fn numeric(rows: &[Vec<String>], c: usize) -> bool {
    let mut any = false;
    for value in rows
        .iter()
        .skip(1)
        .filter_map(|r| r.get(c))
        .map(|s| s.as_str().trim())
    {
        if value.is_empty() {
            continue;
        }
        if value.parse::<f64>().is_err() {
            return false;
        }
        any = true;
    }
    any
}

/// Show CSV (or TSV when `tabs`) text as a table.
pub(crate) fn show(sheet: &mut Sheet, text: &str, tabs: bool, protected: bool) {
    let canvas = sheet.body.canvas();
    let delim = if tabs { '\t' } else { delimiter(text) };
    let keep = canvas.limits.lines.saturating_sub(1).max(1);
    let read = records(text, delim, keep);
    let sep = if canvas.ascii { " | " } else { " │ " };
    let widths = layout(&read.rows, sheet.body.width(), 3);
    let numbers: Vec<bool> = (0..widths.len()).map(|c| numeric(&read.rows, c)).collect();
    let secret_columns: Vec<bool> = (0..widths.len())
        .map(|c| {
            read.rows
                .first()
                .and_then(|h| h.get(c))
                .is_some_and(|h| secret::secret_key(h))
        })
        .collect();
    let mut tally = Tally::default();
    let mut masked = 0;
    for (r, row) in read.rows.iter().enumerate() {
        let mut spans: Vec<Span<'static>> = Vec::new();
        for (c, width) in widths.iter().enumerate() {
            if c > 0 {
                spans.push(paint(sep, Role::Dim, canvas.color));
            }
            let raw = row.get(c).map_or("", String::as_str);
            let mut value = tally.line(raw, canvas, false);
            if protected && r > 0 && !value.is_empty() {
                if secret_columns[c] {
                    secret::mask(canvas.ascii).clone_into(&mut value);
                    masked += 1;
                } else {
                    let (shown, count) = mask::text(&value, canvas.ascii);
                    value = shown;
                    masked += count;
                }
            }
            let last = c + 1 == widths.len();
            let right = numbers[c] && r > 0;
            let shown = if last && !right {
                cells::cell(
                    &value,
                    cells::width(&value).min(*width),
                    false,
                    canvas.ascii,
                )
            } else {
                cells::cell(&value, *width, right, canvas.ascii)
            };
            spans.push(if r == 0 {
                paint(shown, Role::Strong, canvas.color)
            } else {
                plain(shown)
            });
        }
        if !sheet.body.push(spans, false) {
            break;
        }
        if r == 0 && read.rows.len() > 1 {
            let (bar, cross) = if canvas.ascii {
                ("-", "-+-")
            } else {
                ("─", "─┼─")
            };
            let rule: Vec<String> = widths.iter().map(|w| bar.repeat(*w)).collect();
            if !sheet.body.push(
                vec![paint(rule.join(cross), Role::Dim, canvas.color)],
                false,
            ) {
                break;
            }
        }
    }
    tally.masked(masked);
    tally.report(&mut sheet.notes);
    facts(sheet, &read, delim, widths.len());
}

/// The facts and notes of the records read: their size, a delimiter other
/// than the comma, ragged rows, and the rows and columns left undrawn
/// (`drawn`: the columns the width holds).
fn facts(sheet: &mut Sheet, read: &Records, delim: char, drawn: usize) {
    let ascii = sheet.body.canvas().ascii;
    let by = if ascii { "x" } else { "×" };
    let rows = cells::count(read.total, "row");
    let columns = cells::count(read.most, "column");
    sheet.facts.push(format!("{rows} {by} {columns}"));
    if delim != ',' {
        let named = match delim {
            '\t' => "tab",
            ';' => "semicolon",
            _ => "pipe",
        };
        sheet.facts.push(format!("{named}-separated"));
    }
    if read.fewest != read.most {
        let dot = cells::sep(ascii);
        sheet.facts.push(format!(
            "rows hold {} to {} cells{dot}ragged",
            read.fewest, read.most
        ));
    }
    if read.rows.len() < read.total {
        sheet.notes.push(Note::RowsCut {
            shown: read.rows.len(),
            total: read.total,
        });
    }
    if drawn < read.most {
        sheet.notes.push(Note::ColumnsCut {
            shown: drawn,
            total: read.most,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Canvas, Format, Limits};

    fn texts(sheet: Sheet) -> Vec<String> {
        let mut notes = Vec::new();
        sheet
            .body
            .finish(&mut notes)
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    #[test]
    fn quoting_follows_rfc_4180() {
        let read = records(
            "a,\"b,c\",\"say \"\"hi\"\"\"\r\n1,\"two\nlines\",3\n",
            ',',
            9,
        );
        assert_eq!(read.total, 2);
        assert_eq!(read.rows[0], ["a", "b,c", "say \"hi\""]);
        assert_eq!(read.rows[1], ["1", "two\nlines", "3"]);
        assert_eq!(delimiter("a;b;c\n"), ';');
        assert_eq!(delimiter("a\tb\n"), '\t');
        assert_eq!(delimiter("\"x;y\",z\n"), ',');
    }

    #[test]
    fn a_table_aligns_numbers_and_draws_a_header_rule() {
        let mut sheet = Sheet::new(Canvas::new(40, true, false), Format::Csv);
        show(&mut sheet, "name,count\nalpha,3\nbe,12\n", false, false);
        assert_eq!(sheet.facts, ["3 rows x 2 columns"]);
        assert_eq!(
            texts(sheet),
            [
                "name  | count",
                "------+------",
                "alpha |     3",
                "be    |    12"
            ]
        );
    }

    #[test]
    fn a_wide_table_narrows_then_drops_columns_and_says_so() {
        let header: Vec<String> = (0..30).map(|i| format!("column_{i}")).collect();
        let text = format!("{}\n", header.join(","));
        let mut sheet = Sheet::new(Canvas::new(80, true, false), Format::Csv);
        show(&mut sheet, &text, false, false);
        assert!(
            matches!(sheet.notes.last(), Some(Note::ColumnsCut { total: 30, .. })),
            "{:?}",
            sheet.notes
        );
        for line in texts(sheet) {
            assert!(cells::width(&line) <= 80, "{line}");
        }
    }

    #[test]
    fn rows_beyond_the_bound_are_counted_and_a_protected_column_masked() {
        let limits = Limits::new(1 << 20, 4, 4096);
        let mut sheet = Sheet::new(
            Canvas::new(60, true, false).with_limits(limits),
            Format::Csv,
        );
        show(
            &mut sheet,
            "user,password\nann,hunter2\nbob,letmein\ncy,x1\ndee,y2\n",
            false,
            true,
        );
        assert!(
            sheet.notes.contains(&Note::RowsCut { shown: 3, total: 5 }),
            "{:?}",
            sheet.notes
        );
        let text = texts(sheet).join("\n");
        assert!(
            !text.contains("hunter2") && !text.contains("letmein"),
            "{text}"
        );
    }
}
