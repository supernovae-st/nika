// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A unified diff or patch: file headers strong, hunk headers in the
//! accent, added lines green and removed lines red. The `+` and `-` column
//! stays in every line, so the change reads the same without colour.

use nika_display::theme::Role;

use super::cells::{self, Sheet, Tally, paint, plain};

/// The role of one diff line, by its first characters.
fn role(line: &str) -> Option<Role> {
    let header = [
        "diff --git",
        "index ",
        "new file",
        "deleted file",
        "similarity",
        "rename ",
        "+++ ",
        "--- ",
    ];
    if header.iter().any(|h| line.starts_with(h)) {
        Some(Role::Strong)
    } else if line.starts_with("@@") {
        Some(Role::Accent)
    } else if line.starts_with('+') {
        Some(Role::Good)
    } else if line.starts_with('-') {
        Some(Role::Bad)
    } else if line.starts_with('\\') {
        Some(Role::Dim)
    } else {
        None
    }
}

/// What the whole diff changes: files, added lines, removed lines.
fn totals(text: &str) -> (usize, usize, usize) {
    let (mut files, mut added, mut removed) = (0, 0, 0);
    for line in text.lines() {
        if line.starts_with("+++ ") {
            files += 1;
        } else if line.starts_with('+') {
            added += 1;
        } else if line.starts_with('-') && !line.starts_with("--- ") {
            removed += 1;
        }
    }
    (files, added, removed)
}

/// Show a unified diff.
pub(crate) fn show(sheet: &mut Sheet, text: &str, protected: bool) {
    let canvas = sheet.body.canvas();
    let mut tally = Tally::default();
    for (raw, more) in cells::lines(text, canvas.limits.line_bytes) {
        let line = tally.changed(raw, canvas, protected);
        let span = match role(&line) {
            Some(tone) => paint(line, tone, canvas.color),
            None => plain(line),
        };
        if !sheet.body.push(vec![span], more) {
            break;
        }
    }
    tally.report(&mut sheet.notes);
    let (files, added, removed) = totals(text);
    let minus = if canvas.ascii { "-" } else { "−" };
    let dot = cells::sep(canvas.ascii);
    let files = cells::count(files, "file");
    sheet
        .facts
        .push(format!("{files}{dot}+{added} {minus}{removed} lines"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Canvas, Format};

    #[test]
    fn every_diff_line_keeps_its_column_and_role() {
        assert_eq!(role("diff --git a/x b/x"), Some(Role::Strong));
        assert_eq!(role("--- a/x"), Some(Role::Strong));
        assert_eq!(role("@@ -1,2 +1,2 @@ fn"), Some(Role::Accent));
        assert_eq!(role("+added"), Some(Role::Good));
        assert_eq!(role("-removed"), Some(Role::Bad));
        assert_eq!(role("\\ No newline at end of file"), Some(Role::Dim));
        assert_eq!(role(" context"), None);
        let mut sheet = Sheet::new(Canvas::new(40, true, false), Format::Diff);
        show(
            &mut sheet,
            "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-old\n+new\n+more\n",
            false,
        );
        assert_eq!(sheet.facts, ["1 file - +2 -1 lines"]);
    }
}
