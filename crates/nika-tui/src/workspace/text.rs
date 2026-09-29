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

#[cfg(test)]
mod tests {
    use super::*;

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
