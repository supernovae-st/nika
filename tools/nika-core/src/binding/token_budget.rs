// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Token budget estimation for context_budget: field.
//!
//! Uses character-based heuristics (no tiktoken dependency):
//! - Latin/ASCII: ~4 chars per token
//! - CJK/Hangul: ~2 chars per token
//! - Minimum: 1 token for any non-empty string

use serde_json::Value;

/// Characters per token for Latin/ASCII text.
pub(crate) const LATIN_CHARS_PER_TOKEN: usize = 4;

/// Characters per token for CJK text (Chinese, Japanese, Korean).
const CJK_CHARS_PER_TOKEN: usize = 2;

/// Estimate token count for a string, CJK-aware.
///
/// Counts CJK characters separately (higher token density) and
/// uses ceiling division to ensure at least 1 token for non-empty strings.
pub fn estimate_tokens_str(text: &str) -> u64 {
    if text.is_empty() {
        return 0;
    }

    let mut cjk_chars: usize = 0;
    let mut other_chars: usize = 0;

    for ch in text.chars() {
        if is_cjk(ch) {
            cjk_chars += 1;
        } else {
            other_chars += 1;
        }
    }

    let cjk_tokens = cjk_chars.div_ceil(CJK_CHARS_PER_TOKEN);
    let other_tokens = other_chars.div_ceil(LATIN_CHARS_PER_TOKEN);
    let total = cjk_tokens + other_tokens;

    // Minimum 1 token for non-empty strings
    total.max(1) as u64
}

/// Estimate token count for a JSON Value recursively.
///
/// Walks the tree without allocating a serialized string.
pub fn estimate_tokens_value(value: &Value) -> u64 {
    let char_estimate = json_char_estimate(value);
    char_estimate.div_ceil(LATIN_CHARS_PER_TOKEN as u64).max(1)
}

/// Estimate total tokens across all resolved bindings.
pub fn estimate_bindings_tokens<'a>(bindings: impl Iterator<Item = (&'a str, &'a Value)>) -> u64 {
    bindings
        .map(|(alias, value)| {
            // alias contributes tokens too (template key overhead)
            let alias_tokens = estimate_tokens_str(alias);
            let value_tokens = estimate_tokens_value(value);
            alias_tokens + value_tokens
        })
        .sum()
}

/// Check if a character is CJK (Chinese, Japanese, Korean).
#[inline]
fn is_cjk(ch: char) -> bool {
    matches!(ch,
        '\u{4E00}'..='\u{9FFF}'   // CJK Unified Ideographs
        | '\u{3400}'..='\u{4DBF}' // CJK Extension A
        | '\u{3040}'..='\u{309F}' // Hiragana
        | '\u{30A0}'..='\u{30FF}' // Katakana
        | '\u{AC00}'..='\u{D7AF}' // Hangul Syllables
        | '\u{F900}'..='\u{FAFF}' // CJK Compat Ideographs
    )
}

/// Recursive character estimate for JSON values (no allocation).
fn json_char_estimate(value: &Value) -> u64 {
    match value {
        Value::Null => 4,
        Value::Bool(b) => {
            if *b {
                4
            } else {
                5
            }
        }
        Value::Number(n) => {
            // Approximate: digits + sign + decimal point
            let s = n.to_string();
            s.len() as u64
        }
        Value::String(s) => s.len() as u64 + 2, // +2 for quotes
        Value::Array(arr) => {
            let inner: u64 = arr.iter().map(json_char_estimate).sum();
            inner + 2 + arr.len().saturating_sub(1) as u64 // brackets + commas
        }
        Value::Object(obj) => {
            let inner: u64 = obj
                .iter()
                .map(|(k, v)| k.len() as u64 + 3 + json_char_estimate(v)) // key + "": + value
                .sum();
            inner + 2 + obj.len().saturating_sub(1) as u64 // braces + commas
        }
    }
}

/// Truncate a string to approximately `target_tokens` tokens at a word boundary.
///
/// Returns the truncated string. Tries to break at whitespace.
pub fn truncate_to_tokens(text: &str, target_tokens: u64) -> &str {
    if estimate_tokens_str(text) <= target_tokens {
        return text;
    }

    // Approximate target char count
    let target_chars = (target_tokens as usize) * LATIN_CHARS_PER_TOKEN;

    if target_chars >= text.len() {
        return text;
    }

    // Find a char boundary at or before target_chars
    let mut boundary = target_chars;
    while boundary > 0 && !text.is_char_boundary(boundary) {
        boundary -= 1;
    }

    // Try to break at last whitespace before boundary
    if let Some(ws_pos) = text[..boundary].rfind(char::is_whitespace) {
        if ws_pos > boundary / 2 {
            // Only use word boundary if it's in the second half
            return &text[..ws_pos];
        }
    }

    &text[..boundary]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_estimate_tokens_str_english() {
        // "hello world" = 11 chars, ~3 tokens (11/4 ceil = 3)
        assert_eq!(estimate_tokens_str("hello world"), 3);
    }

    #[test]
    fn test_estimate_tokens_str_empty() {
        assert_eq!(estimate_tokens_str(""), 0);
    }

    #[test]
    fn test_estimate_tokens_str_single_char() {
        // 1 char -> 1 token minimum
        assert_eq!(estimate_tokens_str("x"), 1);
    }

    #[test]
    fn test_estimate_tokens_str_cjk() {
        // 4 CJK chars = 4/2 ceil = 2 tokens
        let cjk = "你好世界"; // 4 chars
        let tokens = estimate_tokens_str(cjk);
        assert_eq!(tokens, 2, "4 CJK chars should be ~2 tokens");
    }

    #[test]
    fn test_estimate_tokens_str_mixed_cjk_latin() {
        // "Hello 你好" = 6 Latin + space + 2 CJK
        // Latin: 7 chars / 4 = 2 tokens, CJK: 2 chars / 2 = 1 token => 3
        let mixed = "Hello 你好";
        let tokens = estimate_tokens_str(mixed);
        assert_eq!(tokens, 3, "Mixed CJK/Latin: got {tokens}");
    }

    #[test]
    fn test_estimate_tokens_value_string() {
        let val = json!("hello world");
        let tokens = estimate_tokens_value(&val);
        // "hello world" + 2 quotes = 13 chars / 4 = 4 tokens
        assert_eq!(tokens, 4);
    }

    #[test]
    fn test_estimate_tokens_value_object() {
        let val = json!({"name": "Alice", "age": 30});
        let tokens = estimate_tokens_value(&val);
        assert!(tokens > 0, "Object should have tokens");
        assert!(tokens < 20, "Small object shouldn't be huge: {tokens}");
    }

    #[test]
    fn test_estimate_bindings_tokens() {
        let bindings = [
            ("data", json!("some text here")),
            ("config", json!({"key": "value"})),
        ];
        let total = estimate_bindings_tokens(bindings.iter().map(|(k, v)| (*k, v)));
        assert!(total > 0, "Should have tokens");
    }

    #[test]
    fn test_truncate_to_tokens_under_budget() {
        let text = "short";
        assert_eq!(truncate_to_tokens(text, 100), "short");
    }

    #[test]
    fn test_truncate_to_tokens_over_budget() {
        let text = "This is a fairly long sentence that should be truncated to fit the budget";
        let truncated = truncate_to_tokens(text, 5);
        // 5 tokens * 4 chars = ~20 chars target
        assert!(
            truncated.len() <= 24,
            "Should be truncated: len={}",
            truncated.len()
        );
        assert!(!truncated.is_empty(), "Should not be empty");
    }

    #[test]
    fn test_truncate_word_boundary() {
        let text = "word1 word2 word3 word4 word5 word6 word7 word8";
        let truncated = truncate_to_tokens(text, 3);
        // 3 tokens * 4 chars = ~12 chars
        // Should break at word boundary
        assert!(
            !truncated.ends_with(char::is_alphanumeric) || truncated.len() <= 12,
            "Should break near word boundary: '{truncated}'"
        );
    }
}
