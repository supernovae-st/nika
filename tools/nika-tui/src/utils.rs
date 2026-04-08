// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! TUI utility functions
//!
//! Shared formatting and helper functions used across TUI modules.

use std::borrow::Cow;

/// Format number with thousands separator
///
/// # Examples
///
/// ```ignore
/// assert_eq!(format_number(1234567), "1,234,567");
/// assert_eq!(format_number(0), "0");
/// assert_eq!(format_number(999), "999");
/// ```
pub fn format_number(n: u32) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Format number with thousands separator (u64 version)
pub fn format_number_u64(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Format number with smart suffixes (K, M, B)
///
/// # Examples
///
/// ```ignore
/// assert_eq!(format_number_compact(500), "500");
/// assert_eq!(format_number_compact(1500), "1.5K");
/// assert_eq!(format_number_compact(1500000), "1.5M");
/// ```
pub fn format_number_compact(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.1}B", n as f64 / 1_000_000_000.0)
    } else if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Safely truncate a string to a maximum number of characters
///
/// This function handles UTF-8 correctly by counting characters, not bytes.
/// It adds "..." suffix when truncation occurs.
/// Returns `Cow::Borrowed` when no truncation is needed (zero allocation).
///
/// # Examples
///
/// ```ignore
/// assert_eq!(truncate_str("hello world", 5), "he...");
/// assert_eq!(truncate_str("hi", 10), "hi");
/// assert_eq!(truncate_str("日本語テスト", 4), "日...");  // UTF-8 safe
/// ```
pub fn truncate_str(s: &str, max_chars: usize) -> Cow<'_, str> {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        Cow::Borrowed(s)
    } else if max_chars <= 3 {
        // Not enough room for any content + "..."
        Cow::Owned(s.chars().take(max_chars).collect())
    } else {
        // Take max_chars - 3 characters and add "..."
        let truncated: String = s.chars().take(max_chars - 3).collect();
        Cow::Owned(format!("{}...", truncated))
    }
}

/// Wrap text to fit within a given width (character count)
///
/// Performs simple word wrapping, trying to break at spaces when possible.
/// If a single word exceeds the width, it will be split.
///
/// # Examples
///
/// ```ignore
/// let lines = wrap_text("Hello world, this is a test", 10);
/// assert_eq!(lines, vec!["Hello", "world,", "this is a", "test"]);
/// ```
pub fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }

    let mut result = Vec::new();

    for line in text.lines() {
        if line.is_empty() {
            result.push(String::new());
            continue;
        }

        let mut current_line = String::new();
        let mut current_width = 0;

        for word in line.split_whitespace() {
            let word_width = word.chars().count();

            if word_width > max_width {
                // Word is too long, need to split it
                if !current_line.is_empty() {
                    result.push(current_line);
                    current_line = String::new();
                    current_width = 0;
                }
                // Split long word
                let mut chars = word.chars().peekable();
                while chars.peek().is_some() {
                    let chunk: String = chars.by_ref().take(max_width).collect();
                    if !chunk.is_empty() {
                        result.push(chunk);
                    }
                }
            } else if current_width == 0 {
                // First word on line
                current_line = word.to_string();
                current_width = word_width;
            } else if current_width + 1 + word_width <= max_width {
                // Word fits with space
                current_line.push(' ');
                current_line.push_str(word);
                current_width += 1 + word_width;
            } else {
                // Word doesn't fit, start new line
                result.push(current_line);
                current_line = word.to_string();
                current_width = word_width;
            }
        }

        if !current_line.is_empty() {
            result.push(current_line);
        }
    }

    if result.is_empty() {
        result.push(String::new());
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(123), "123");
        assert_eq!(format_number(1234), "1,234");
        assert_eq!(format_number(12345), "12,345");
        assert_eq!(format_number(123456), "123,456");
        assert_eq!(format_number(1234567), "1,234,567");
    }

    #[test]
    fn test_format_number_u64() {
        assert_eq!(format_number_u64(0), "0");
        assert_eq!(format_number_u64(1234567890), "1,234,567,890");
    }

    #[test]
    fn test_format_number_compact() {
        assert_eq!(format_number_compact(500), "500");
        assert_eq!(format_number_compact(1000), "1.0K");
        assert_eq!(format_number_compact(1500), "1.5K");
        assert_eq!(format_number_compact(1000000), "1.0M");
        assert_eq!(format_number_compact(1500000), "1.5M");
        assert_eq!(format_number_compact(1000000000), "1.0B");
    }

    #[test]
    fn test_truncate_str_ascii() {
        assert_eq!(truncate_str("hello world", 20), "hello world");
        assert_eq!(truncate_str("hello world", 11), "hello world");
        assert_eq!(truncate_str("hello world", 8), "hello...");
        assert_eq!(truncate_str("hello world", 5), "he...");
        assert_eq!(truncate_str("hi", 10), "hi");
    }

    #[test]
    fn test_truncate_str_utf8() {
        // Japanese characters (3 bytes each in UTF-8)
        assert_eq!(truncate_str("日本語テスト", 10), "日本語テスト");
        assert_eq!(truncate_str("日本語テスト", 6), "日本語テスト");
        assert_eq!(truncate_str("日本語テスト", 5), "日本...");
        assert_eq!(truncate_str("日本語テスト", 4), "日...");

        // Emojis (4 bytes each in UTF-8)
        assert_eq!(truncate_str("🎉🎊🎁🎂", 10), "🎉🎊🎁🎂");
        assert_eq!(truncate_str("🎉🎊🎁🎂", 4), "🎉🎊🎁🎂");
        assert_eq!(truncate_str("🎉🎊🎁🎂", 3), "🎉🎊🎁"); // Edge case: exactly 3 chars

        // Mixed content
        assert_eq!(truncate_str("Hello 世界!", 10), "Hello 世界!");
        assert_eq!(truncate_str("Hello 世界!", 8), "Hello...");
    }

    #[test]
    fn test_truncate_str_edge_cases() {
        assert_eq!(truncate_str("", 10), "");
        assert_eq!(truncate_str("abc", 3), "abc");
        assert_eq!(truncate_str("abcd", 3), "abc"); // max_chars <= 3: no room for "..."
        assert_eq!(truncate_str("abcde", 4), "a...");
    }
}
