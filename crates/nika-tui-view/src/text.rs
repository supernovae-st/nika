// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Plain text and code. Text wraps to the width, its own line breaks kept;
//! code never wraps (a cut line ends with the cut mark) and carries a dim
//! gutter of line numbers, so a finding's line can be found by eye.

use nika_display::theme::Role;

use super::cells::{self, Sheet, Tally, paint, plain};
use super::data::line_count;

/// Show plain text: every line wrapped, blank lines kept.
pub(crate) fn prose(sheet: &mut Sheet, text: &str, protected: bool) {
    let canvas = sheet.body.canvas();
    let mut tally = Tally::default();
    for (raw, more) in cells::lines(text, canvas.limits.line_bytes) {
        let line = tally.line(raw, canvas, protected);
        let kept = if line.is_empty() {
            sheet.body.push(Vec::new(), false)
        } else {
            sheet.body.wrap(&[plain(line)], &[], &[], more)
        };
        if !kept {
            sheet.body.overflow();
            break;
        }
    }
    tally.report(&mut sheet.notes);
    sheet.facts.push(cells::count(line_count(text), "line"));
}

/// Show code: a gutter of line numbers, every line cut, never wrapped.
pub(crate) fn code(sheet: &mut Sheet, text: &str, protected: bool) {
    let canvas = sheet.body.canvas();
    let total = line_count(text);
    let last = total.min(canvas.limits.lines);
    let digits = last.to_string().len();
    let mut tally = Tally::default();
    for (number, (raw, more)) in cells::lines(text, canvas.limits.line_bytes).enumerate() {
        let line = tally.line(raw, canvas, protected);
        let gutter = paint(format!("{:>digits$} ", number + 1), Role::Dim, canvas.color);
        if !sheet.body.push(vec![gutter, plain(line)], more) {
            break;
        }
    }
    tally.report(&mut sheet.notes);
    sheet.facts.push(cells::count(total, "line"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Canvas, Format, Limits};

    fn texts(sheet: Sheet) -> (Vec<String>, Vec<crate::Note>) {
        let mut notes = sheet.notes;
        let lines = sheet
            .body
            .finish(&mut notes)
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        (lines, notes)
    }

    #[test]
    fn prose_wraps_and_keeps_blank_lines() {
        let mut sheet = Sheet::new(Canvas::new(10, true, false), Format::Text);
        prose(&mut sheet, "one two three\n\nfour", false);
        assert_eq!(texts(sheet).0, ["one two", "three", "", "four"]);
    }

    #[test]
    fn code_has_a_gutter_and_cuts_instead_of_wrapping() {
        let limits = Limits::new(1 << 20, 3, 4096);
        let mut sheet = Sheet::new(
            Canvas::new(12, true, false).with_limits(limits),
            Format::Code,
        );
        code(&mut sheet, "fn main() {}\nx\ny\nz\n", false);
        let (lines, notes) = texts(sheet);
        assert_eq!(lines, ["1 fn main...", "2 x", "3 y"]);
        assert!(
            notes.contains(&crate::Note::LinesCut { shown: 3 }),
            "{notes:?}"
        );
    }
}
