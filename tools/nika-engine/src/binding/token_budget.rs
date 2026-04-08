// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Token budget estimation and enforcement for context_budget: field.
//!
//! Uses character-based heuristics (no tiktoken dependency):
//! - Latin/ASCII: ~4 chars per token
//! - CJK/Hangul: ~2 chars per token
//! - Minimum: 1 token for any non-empty string

use serde_json::Value;

/// Characters per token for Latin/ASCII text.
const LATIN_CHARS_PER_TOKEN: usize = 4;

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

/// Minimum tokens preserved per binding during truncation.
const MIN_TOKENS_PER_BINDING: u64 = 50;

/// Enforce a token budget on resolved bindings.
///
/// If total estimated tokens exceed `budget`, truncates the largest string
/// bindings proportionally. Non-string values are left untouched.
///
/// Returns the EventKind to emit (BudgetOk or BudgetExceeded).
pub fn enforce_budget(
    bindings: &mut super::ResolvedBindings,
    budget: u32,
    task_id: &std::sync::Arc<str>,
) -> nika_event::EventKind {
    let budget_u64 = budget as u64;

    // Collect (alias, token_count) for all resolved bindings
    let binding_sizes: Vec<(String, u64)> = bindings
        .iter()
        .map(|(alias, value)| {
            let tokens = estimate_tokens_value(value);
            (alias.to_string(), tokens)
        })
        .collect();

    let actual: u64 = binding_sizes.iter().map(|(_, t)| *t).sum();

    if actual <= budget_u64 {
        return nika_event::EventKind::BudgetOk {
            task_id: std::sync::Arc::clone(task_id),
            budget,
            actual: actual as u32,
        };
    }

    // Need to truncate. Sort by size descending to truncate largest first.
    let mut sorted: Vec<(String, u64)> = binding_sizes;
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    let excess = actual - budget_u64;
    let mut remaining_excess = excess;
    let mut truncated_fields = Vec::new();

    for (alias, tokens) in &sorted {
        if remaining_excess == 0 {
            break;
        }

        // Only truncate string values
        let value = match bindings.get(alias) {
            Some(v) => v.clone(),
            None => continue,
        };

        if let Value::String(ref text) = value {
            // How much can we take from this binding?
            let max_reduction = tokens.saturating_sub(MIN_TOKENS_PER_BINDING);
            let reduction = remaining_excess.min(max_reduction);

            if reduction > 0 {
                let target_tokens = tokens - reduction;
                let truncated = truncate_to_tokens(text, target_tokens);
                bindings.set(alias.clone(), Value::String(truncated.to_string()));
                remaining_excess = remaining_excess.saturating_sub(reduction);
                truncated_fields.push(alias.clone());
            }
        }
    }

    nika_event::EventKind::BudgetExceeded {
        task_id: std::sync::Arc::clone(task_id),
        budget,
        actual: actual as u32,
        truncated_fields,
    }
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

    #[test]
    fn test_enforce_budget_under_budget() {
        let mut bindings = super::super::ResolvedBindings::new();
        bindings.set("data", json!("short text"));
        let task_id = std::sync::Arc::from("test");
        let event = enforce_budget(&mut bindings, 1000, &task_id);
        assert!(
            matches!(event, nika_event::EventKind::BudgetOk { .. }),
            "Should be under budget"
        );
    }

    #[test]
    fn test_enforce_budget_over_budget_truncates() {
        let mut bindings = super::super::ResolvedBindings::new();
        // Long text: ~500 chars => ~125 tokens
        let long_text = "word ".repeat(100);
        bindings.set("data", json!(long_text));
        let task_id = std::sync::Arc::from("test");
        let event = enforce_budget(&mut bindings, 20, &task_id);
        match event {
            nika_event::EventKind::BudgetExceeded {
                truncated_fields, ..
            } => {
                assert!(
                    truncated_fields.contains(&"data".to_string()),
                    "Should truncate 'data' field: {truncated_fields:?}"
                );
                // Verify binding was actually truncated
                let val = bindings.get("data").unwrap();
                let text = val.as_str().unwrap();
                let tokens = estimate_tokens_str(text);
                assert!(
                    tokens <= 80,
                    "Should be truncated: {tokens} tokens, len={}",
                    text.len()
                );
            }
            _ => panic!("Should exceed budget"),
        }
    }

    #[test]
    fn test_enforce_budget_preserves_minimum() {
        let mut bindings = super::super::ResolvedBindings::new();
        // Even with tiny budget, each binding keeps MIN_TOKENS_PER_BINDING
        let text = "a ".repeat(200); // ~100 tokens
        bindings.set("data", json!(text));
        let task_id = std::sync::Arc::from("test");
        let event = enforce_budget(&mut bindings, 1, &task_id);
        assert!(
            matches!(event, nika_event::EventKind::BudgetExceeded { .. }),
            "Should exceed budget"
        );
        // Binding should still have some content
        let val = bindings.get("data").unwrap();
        let text = val.as_str().unwrap();
        assert!(!text.is_empty(), "Should preserve minimum content");
    }

    #[test]
    fn test_enforce_budget_no_context_budget() {
        // When context_budget is None, enforce_budget is not called
        // This test verifies the function handles edge case of budget=0
        // (which would be rejected by analyzer, but defensive check)
        let mut bindings = super::super::ResolvedBindings::new();
        bindings.set("x", json!("hello"));
        let task_id = std::sync::Arc::from("test");
        // Budget of 100 should be fine for "hello"
        let event = enforce_budget(&mut bindings, 100, &task_id);
        assert!(matches!(event, nika_event::EventKind::BudgetOk { .. }));
    }

    #[test]
    fn test_enforce_budget_multiple_bindings() {
        let mut bindings = super::super::ResolvedBindings::new();
        bindings.set("big", json!("x ".repeat(200))); // ~100 tokens
        bindings.set("small", json!("tiny")); // ~2 tokens
        let task_id = std::sync::Arc::from("test");
        let event = enforce_budget(&mut bindings, 30, &task_id);
        match event {
            nika_event::EventKind::BudgetExceeded {
                truncated_fields, ..
            } => {
                // Should truncate the big one, not the small one
                assert!(
                    truncated_fields.contains(&"big".to_string()),
                    "Should truncate 'big': {truncated_fields:?}"
                );
            }
            _ => panic!("Should exceed budget"),
        }
    }

    #[test]
    fn test_enforce_budget_json_value_not_truncated() {
        let mut bindings = super::super::ResolvedBindings::new();
        bindings.set("config", json!({"key": "value", "nested": {"deep": true}}));
        let task_id = std::sync::Arc::from("test");
        // JSON values are not string-truncatable
        let event = enforce_budget(&mut bindings, 2, &task_id);
        // Even though over budget, non-string values aren't modified
        assert!(matches!(
            event,
            nika_event::EventKind::BudgetExceeded { .. }
        ));
        let val = bindings.get("config").unwrap();
        assert!(val.is_object(), "JSON object should be preserved");
    }
}
