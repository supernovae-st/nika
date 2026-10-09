// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Fitting words into cells: widths are measured with the same table as the
//! layout, never by counting characters. The renderer fits its own rows with
//! the same helpers, so a separator, a cut mark or a wrapped word reads the
//! same in every region.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// The separator and the cut mark of the glyph column in use.
#[must_use]
pub const fn marks(ascii: bool) -> (&'static str, &'static str) {
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
#[must_use]
pub fn fit_head(text: &str, width: usize, cut: &str) -> String {
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

/// Words the Session wrote for the Unicode column (its separators, quotes,
/// marks and arrows) in the glyph column in use; any other character is
/// content and stays.
#[must_use]
pub fn twins(text: &str, ascii: bool) -> String {
    if !ascii || text.is_ascii() {
        return text.to_owned();
    }
    text.replace('·', "-")
        .replace('…', "...")
        .replace('→', "->")
        .replace(['—', '–'], "-")
        .replace(['«', '»'], "\"")
        .replace('✔', "ok")
        .replace('✖', "X")
        .replace('○', "-")
}

/// `text` broken into rows of at most `width` cells at its spaces; a word
/// wider than a row is cut with `cut`. Nothing when `width` is zero.
#[must_use]
pub fn wrap(text: &str, width: usize, cut: &str) -> Vec<String> {
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

/// Binds a count to its unit (`61 lines`): [`hang`] never parts them, and
/// paints a plain space between them.
pub const KEEP: char = '\u{a0}';

/// The cells every row of a fact after its first hangs in ([`hang`]): deeper
/// than any first row, so a continuation never reads as a fact of its own.
pub const HANG: usize = 4;

/// `words` broken at their plain spaces into rows of at most `width` cells:
/// the first row `indent` cells in, every further row [`HANG`] cells in. A
/// word stays whole on a row where it fits one (a [`KEEP`] binds a count to
/// its unit); a wider one breaks between its characters, never inside one.
/// Widths are the layout's cells, and no row is cut.
#[must_use]
pub fn hang(words: &str, indent: usize, width: usize) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }
    let hung = HANG.min(width - 1);
    let start = indent.min(hung);
    let mut rows = Vec::new();
    let mut row = " ".repeat(start);
    let (mut used, mut from, mut bare) = (start, start, true);
    for word in words.split(' ').filter(|word| !word.is_empty()) {
        if !bare && used + 1 + word.width() <= width {
            row.push(' ');
            row.push_str(word);
            used += 1 + word.width();
            continue;
        }
        if !bare {
            rows.push(std::mem::replace(&mut row, " ".repeat(hung)));
            (used, from) = (hung, hung);
        }
        for glyph in word.chars() {
            let cells = glyph.width().unwrap_or(0);
            if used + cells > width && used > from {
                rows.push(std::mem::replace(&mut row, " ".repeat(hung)));
                (used, from) = (hung, hung);
            }
            row.push(glyph);
            used += cells;
        }
        bare = false;
    }
    if !bare {
        rows.push(row);
    }
    rows.into_iter().map(|row| row.replace(KEEP, " ")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hanging_keeps_each_word_whole_where_a_row_holds_it() {
        let words = format!("creates notes/digest.nika (61{KEEP}lines) over-a-very-long-name");
        let rows = hang(&words, 0, 24);
        assert_eq!(rows[0], "creates");
        assert_eq!(rows[1], "    notes/digest.nika");
        assert_eq!(rows[2], "    (61 lines)");
        assert!(
            rows[3..].iter().all(|row| row.starts_with("    ")),
            "{rows:?}"
        );
        assert_eq!(rows[3..].concat().replace(' ', ""), "over-a-very-long-name");
        assert!(rows.iter().all(|row| row.width() <= 24), "{rows:?}");
        assert_eq!(hang("a b", 2, 10), ["  a b"]);
        assert!(hang("anything", 0, 0).is_empty());
    }

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
