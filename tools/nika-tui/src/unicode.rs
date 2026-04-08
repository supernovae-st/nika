// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Unicode text handling utilities for TUI
//!
//! Provides display width calculation and text truncation
//! that correctly handles multi-byte characters.

use unicode_width::UnicodeWidthStr;

/// Calculate the display width of a string
///
/// This correctly handles multi-byte Unicode characters and returns
/// the number of terminal columns needed to display the text.
pub fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Truncate a string to fit within a maximum display width
///
/// If the string is longer than `max_width`, it will be truncated
/// and "…" appended. The result will fit within `max_width` columns.
///
/// # Arguments
///
/// * `s` - The string to truncate
/// * `max_width` - Maximum display width in terminal columns
///
/// # Returns
///
/// A truncated string that fits within `max_width` columns
pub fn truncate_to_width(s: &str, max_width: usize) -> String {
    let width = display_width(s);

    if width <= max_width {
        return s.to_string();
    }

    if max_width == 0 {
        return String::new();
    }

    if max_width == 1 {
        return "…".to_string();
    }

    // Reserve space for ellipsis (1 column)
    let target_width = max_width - 1;
    let mut result = String::new();
    let mut current_width = 0;

    for c in s.chars() {
        let char_width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if current_width + char_width > target_width {
            break;
        }
        result.push(c);
        current_width += char_width;
    }

    result.push('…');
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_width_ascii() {
        assert_eq!(display_width("hello"), 5);
        assert_eq!(display_width(""), 0);
        assert_eq!(display_width("a"), 1);
    }

    #[test]
    fn test_display_width_unicode() {
        // Japanese characters are typically 2 columns wide
        assert_eq!(display_width("日本語"), 6);
        // Emoji are typically 2 columns wide
        assert_eq!(display_width("🚀"), 2);
    }

    #[test]
    fn test_display_width_mixed() {
        assert_eq!(display_width("hello世界"), 9); // 5 + 2*2
    }

    #[test]
    fn test_truncate_to_width_no_truncation() {
        assert_eq!(truncate_to_width("hello", 10), "hello");
        assert_eq!(truncate_to_width("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_to_width_basic() {
        assert_eq!(truncate_to_width("hello world", 8), "hello w…");
    }

    #[test]
    fn test_truncate_to_width_edge_cases() {
        assert_eq!(truncate_to_width("hello", 0), "");
        assert_eq!(truncate_to_width("hello", 1), "…");
        assert_eq!(truncate_to_width("hello", 2), "h…");
    }

    #[test]
    fn test_truncate_to_width_unicode() {
        // "日本語" is 6 columns, truncate to 4
        let result = truncate_to_width("日本語", 4);
        // Should be "日…" (2 + 1 = 3 columns, which is <= 4)
        assert!(display_width(&result) <= 4);
    }
}
