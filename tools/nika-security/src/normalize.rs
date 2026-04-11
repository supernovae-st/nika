// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Unicode normalization and quote-aware string matching for blocklist checks.

use unicode_normalization::UnicodeNormalization;

/// Zero-width and invisible characters to strip before blocklist check.
///
/// These characters are invisible but can be used to break up keywords:
/// - U+200B: Zero Width Space
/// - U+200C: Zero Width Non-Joiner
/// - U+200D: Zero Width Joiner
/// - U+FEFF: Zero Width No-Break Space (BOM)
/// - U+00AD: Soft Hyphen
/// - U+2060: Word Joiner
/// - U+180E: Mongolian Vowel Separator
const ZERO_WIDTH_CHARS: &[char] = &[
    '\u{200B}', // Zero Width Space
    '\u{200C}', // Zero Width Non-Joiner
    '\u{200D}', // Zero Width Joiner
    '\u{FEFF}', // Zero Width No-Break Space (BOM)
    '\u{00AD}', // Soft Hyphen
    '\u{2060}', // Word Joiner
    '\u{180E}', // Mongolian Vowel Separator
];

/// Normalize a string using NFKC for blocklist comparison.
///
/// This function performs two operations:
/// 1. NFKC normalization (Compatibility Decomposition + Canonical Composition)
///    - Fullwidth `ｒｍ` → `rm`
///    - Math bold `𝐬𝐮𝐝𝐨` → `sudo`
///    - Subscript/superscript variants → base characters
///    - Ligatures (e.g., ﬁ) → component characters
///
/// 2. Stripping of zero-width/invisible characters that NFKC preserves:
///    - Zero Width Space (U+200B)
///    - Zero Width Joiner (U+200D)
///    - Zero Width Non-Joiner (U+200C)
///    - Soft Hyphen (U+00AD)
///
/// This prevents attackers from bypassing the blocklist with visually
/// similar but technically different Unicode characters, or by inserting
/// invisible characters to break up blocked patterns.
pub(crate) fn normalize_for_blocklist(s: &str) -> String {
    s.nfkc()
        .filter(|c| !ZERO_WIDTH_CHARS.contains(c))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Normalize the first token of a command to its basename.
///
/// `/usr/bin/sudo rm -rf /` → `sudo rm -rf /`
/// `/usr/local/bin/python3 -c "..."` → `python3 -c "..."`
///
/// This prevents bypass via absolute path to blocked binaries.
pub(crate) fn normalize_first_token_basename(cmd: &str) -> String {
    let trimmed = cmd.trim();
    if let Some(space_pos) = trimmed.find(' ') {
        let first_token = &trimmed[..space_pos];
        if first_token.contains('/') || first_token.contains('\\') {
            let basename = std::path::Path::new(first_token)
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or(first_token);
            return format!("{} {}", basename, &trimmed[space_pos + 1..]);
        }
    }
    cmd.to_string()
}

/// Check whether `haystack` contains `needle` outside of quoted regions.
///
/// Tracks shell quoting state (single quotes, double quotes) while scanning.
/// Inside double quotes, `\"` is recognized as an escaped quote (not a closer).
/// Single quotes have no escape mechanism in POSIX shell.
///
/// Returns `true` only when `needle` appears at a position that is NOT inside
/// a quoted string.
pub(crate) fn contains_unquoted(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }

    #[derive(Clone, Copy, PartialEq)]
    enum QuoteState {
        None,
        Single,
        Double,
    }

    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    let mut state = QuoteState::None;
    let mut i = 0;

    while i < h.len() {
        let c = h[i];

        match state {
            QuoteState::None => {
                if c == b'\'' {
                    state = QuoteState::Single;
                    i += 1;
                    continue;
                }
                if c == b'"' {
                    state = QuoteState::Double;
                    i += 1;
                    continue;
                }
                // Check for needle match at this position (outside quotes)
                if i + n.len() <= h.len() && &h[i..i + n.len()] == n {
                    return true;
                }
            }
            QuoteState::Single => {
                if c == b'\'' {
                    state = QuoteState::None;
                }
            }
            QuoteState::Double => {
                if c == b'\\' && i + 1 < h.len() {
                    i += 2; // skip escaped char
                    continue;
                }
                if c == b'"' {
                    state = QuoteState::None;
                }
            }
        }
        i += 1;
    }

    false
}
