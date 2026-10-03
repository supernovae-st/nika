// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Fitting words into cells: widths are measured with the same table as the
//! layout, never by counting characters.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// The separator and the cut mark of the glyph column in use.
pub(crate) const fn marks(ascii: bool) -> (&'static str, &'static str) {
    if ascii {
        (" - ", "...")
    } else {
        (" · ", "…")
    }
}

/// `text` fitted to `width` cells: its first `keep` characters stay, then the
/// `cut` mark, then as much of its end as fits; empty when not even the kept
/// start and the mark fit.
pub(crate) fn fit_tail(text: &str, width: usize, keep: usize, cut: &str) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    let head: String = text.chars().take(keep).collect();
    if width < head.width() + cut.width() {
        return String::new();
    }
    let budget = width - head.width() - cut.width();
    let rest: Vec<char> = text.chars().skip(keep).collect();
    let mut tail: Vec<char> = Vec::new();
    let mut used = 0;
    for c in rest.into_iter().rev() {
        let w = c.width().unwrap_or(0);
        if used + w > budget {
            break;
        }
        used += w;
        tail.push(c);
    }
    tail.reverse();
    format!("{head}{cut}{}", tail.into_iter().collect::<String>())
}

/// `text` fitted to `width` cells by cutting its end, marked with `cut`.
pub(crate) fn fit_head(text: &str, width: usize, cut: &str) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    if width < cut.width() {
        return String::new();
    }
    let budget = width - cut.width();
    let mut head = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > budget {
            break;
        }
        used += w;
        head.push(c);
    }
    format!("{head}{cut}")
}

/// `text` broken into rows of at most `width` cells at its spaces; a word
/// wider than a row is cut with `cut`. Nothing when `width` is zero.
pub(crate) fn wrap(text: &str, width: usize, cut: &str) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }
    let mut rows: Vec<String> = Vec::new();
    let mut row = String::new();
    for word in text.split_whitespace() {
        let word = fit_head(word, width, cut);
        if !row.is_empty() && row.width() + 1 + word.width() > width {
            rows.push(std::mem::take(&mut row));
        }
        if !row.is_empty() {
            row.push(' ');
        }
        row.push_str(&word);
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_breaks_at_spaces_and_cuts_a_word_wider_than_a_row() {
        assert_eq!(
            wrap("the file listing needs a Session contract", 20, "…"),
            ["the file listing", "needs a Session", "contract"]
        );
        assert_eq!(
            wrap("unbreakable-identifier here", 8, "..."),
            ["unbre...", "here"]
        );
        assert!(wrap("anything", 0, "…").is_empty());
        for width in 1..24 {
            assert!(
                wrap("conversation d'équipe · 日本語のプロジェクト", width, "…")
                    .iter()
                    .all(|row| row.width() <= width),
                "{width}"
            );
        }
    }

    #[test]
    fn fitting_keeps_within_the_width_on_both_ends() {
        assert_eq!(fit_tail(" · ~/a/b/c/d", 9, 3, "…"), " · …b/c/d");
        assert_eq!(fit_head("release.nika revision 7", 10, "…"), "release.n…");
        assert_eq!(fit_head("short", 10, "…"), "short");
        assert_eq!(fit_tail("abc", 0, 3, "…"), "");
        for width in 0..12 {
            assert!(fit_head("conversation d'équipe", width, "…").width() <= width);
            assert!(fit_tail(" · 日本語のプロジェクト", width, 3, "…").width() <= width);
        }
    }
}
