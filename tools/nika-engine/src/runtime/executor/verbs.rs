//! Shared helpers used by verb implementations
//!
//! Free functions used across the per-verb modules (infer, exec, fetch, invoke, agent).
//! All verb `impl TaskExecutor` methods have been extracted into their own modules.

/// Estimate token count from character length using ceiling division.
///
/// Uses ~4 chars/token heuristic. Ceiling division ensures non-empty strings
/// always produce at least 1 token (fixes off-by-one where `len / 4 == 0`
/// for strings shorter than 4 characters).
#[inline]
pub(crate) fn estimate_tokens(char_len: usize) -> u64 {
    char_len.div_ceil(4) as u64
}

/// Estimate the serialized size of a JSON Value without allocating a String.
///
/// Walks the Value tree recursively. Approximate (doesn't account for escaping
/// or exact number formatting) but avoids the allocation from `value.to_string()`.
/// Used by ContextAssembled event to estimate token counts (H10 fix).
pub(crate) fn json_value_size_estimate(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Null => 4,
        serde_json::Value::Bool(b) => {
            if *b {
                4
            } else {
                5
            }
        }
        serde_json::Value::Number(n) => {
            // Most numbers are 1-20 chars
            let s = n.to_string();
            s.len()
        }
        serde_json::Value::String(s) => s.len() + 2, // +2 for quotes
        serde_json::Value::Array(arr) => {
            2 + arr.iter().map(json_value_size_estimate).sum::<usize>()
                + arr.len().saturating_sub(1) // commas
        }
        serde_json::Value::Object(obj) => {
            2 + obj
                .iter()
                .map(|(k, v)| k.len() + 3 + json_value_size_estimate(v))
                .sum::<usize>()
                + obj.len().saturating_sub(1)
        }
    }
}

/// Coerce string values that look like numbers/booleans back to native JSON types.
///
/// Template resolution is string-based: `{{with.count}}` resolving to 42 produces
/// `"42"` (string), not `42` (number). MCP tools expecting integers/booleans fail.
/// This walks the Value tree and converts unambiguous string representations back
/// to their native JSON types.
pub(crate) fn coerce_json_types(value: &mut serde_json::Value) {
    use serde_json::Value;
    match value {
        Value::Object(map) => {
            for v in map.values_mut() {
                coerce_json_types(v);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                coerce_json_types(v);
            }
        }
        Value::String(s) => {
            // Try JSON array/object first — template resolution turns `{{with.x | to_json}}`
            // into a string like `["a","b"]` or `{"k":"v"}`. Parse back to native JSON
            // so fetch: json: sends the correct type to APIs.
            let trimmed = s.trim();
            if (trimmed.starts_with('[') && trimmed.ends_with(']'))
                || (trimmed.starts_with('{') && trimmed.ends_with('}'))
            {
                if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                    *value = parsed;
                    return;
                }
            }
            if let Ok(n) = s.parse::<i64>() {
                *value = Value::Number(n.into());
            } else if let Ok(n) = s.parse::<u64>() {
                *value = Value::Number(n.into());
            } else if let Ok(n) = s.parse::<f64>() {
                if n.is_finite() {
                    if let Some(num) = serde_json::Number::from_f64(n) {
                        *value = Value::Number(num);
                    }
                }
            } else if s == "true" {
                *value = Value::Bool(true);
            } else if s == "false" {
                *value = Value::Bool(false);
            }
            // Note: "null" is NOT coerced to Value::Null — the string "null" is
            // a valid data value and should be preserved as-is.
        }
        _ => {}
    }
}

/// Redact secrets and truncate resolved template for event logging.
///
/// 1. Pattern-matches known API key formats and replaces with `[REDACTED]`.
/// 2. Truncates to 200 bytes with UTF-8-safe boundary.
pub(crate) fn redact_for_event(s: &str) -> String {
    let redacted = crate::util::redact_secrets(s);
    if redacted.len() <= 200 {
        redacted
    } else {
        let mut boundary = 200;
        while boundary > 0 && !redacted.is_char_boundary(boundary) {
            boundary -= 1;
        }
        format!("{}... ({} bytes)", &redacted[..boundary], redacted.len())
    }
}

/// Detect image MIME type from magic bytes and return rig-core ImageMediaType.
pub(crate) fn detect_image_media_type(
    data: &[u8],
) -> Option<rig::completion::message::ImageMediaType> {
    use rig::completion::message::ImageMediaType;
    if data.len() < 4 {
        return None;
    }
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        Some(ImageMediaType::PNG)
    } else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(ImageMediaType::JPEG)
    } else if data.starts_with(b"GIF8") {
        Some(ImageMediaType::GIF)
    } else if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some(ImageMediaType::WEBP)
    } else {
        None
    }
}

/// Extract `<think>...</think>` and `<thinking>...</thinking>` blocks from LLM responses.
///
/// Reasoning models (Qwen, DeepSeek-R1) emit thinking blocks wrapped in these tags.
/// This function extracts the thinking content for observability and returns the
/// cleaned text without think tags. Case-insensitive to handle `<Think>`, `<THINK>`,
/// `<thinking>`, etc.
///
/// Returns `(thinking_content, clean_text)`:
/// - `thinking_content`: `Some(String)` if non-empty thinking blocks were found, `None` otherwise.
/// - `clean_text`: The original text with all think/thinking tags and their contents removed.
pub(crate) fn extract_thinking_tags(text: &str) -> (Option<String>, String) {
    let lower = text.to_lowercase();

    // Fast path: no think tags at all (case-insensitive check)
    if !lower.contains("<think") {
        return (None, text.to_string());
    }

    // Iterate using char_indices on the ORIGINAL text to stay on char boundaries.
    // Compare against lowercase on-the-fly for case-insensitive tag matching.
    let mut result = String::with_capacity(text.len());
    let mut thinking_parts: Vec<String> = Vec::new();
    let mut chars = text.char_indices().peekable();

    while let Some(&(i, _)) = chars.peek() {
        // Check if remaining text starts with a think tag (case-insensitive)
        let remaining = &text[i..];
        let remaining_lower_start: String = remaining
            .chars()
            .take(10)
            .collect::<String>()
            .to_lowercase();

        let tag_match = if remaining_lower_start.starts_with("<thinking>") {
            Some(("<thinking>".len(), "</thinking>"))
        } else if remaining_lower_start.starts_with("<think>") {
            Some(("<think>".len(), "</think>"))
        } else {
            None
        };

        if let Some((open_tag_chars, close_tag)) = tag_match {
            // Skip open tag characters
            for _ in 0..open_tag_chars {
                chars.next();
            }
            // Find close tag in remaining text (case-insensitive)
            let after_open = chars.peek().map(|&(idx, _)| idx).unwrap_or(text.len());
            let search_region = &text[after_open..];
            let search_lower = search_region.to_lowercase();
            if let Some(close_pos_in_lower) = search_lower.find(&close_tag.to_lowercase()) {
                // Extract the thinking content (original case preserved)
                let thinking_content = &search_region[..close_pos_in_lower];
                let trimmed = thinking_content.trim();
                if !trimmed.is_empty() {
                    thinking_parts.push(trimmed.to_string());
                }
                // Map the position back to the original text by counting chars
                let chars_to_skip = search_region[..close_pos_in_lower].chars().count();
                let close_tag_chars = close_tag.chars().count();
                for _ in 0..(chars_to_skip + close_tag_chars) {
                    chars.next();
                }
            } else {
                // Unclosed tag — extract everything after the open tag as thinking
                let unclosed_content = &text[after_open..];
                let trimmed = unclosed_content.trim();
                if !trimmed.is_empty() {
                    thinking_parts.push(trimmed.to_string());
                }
                break;
            }
            continue;
        }

        // Not inside a tag — copy char from original text
        let (_, ch) = chars.next().unwrap();
        result.push(ch);
    }

    let thinking = if thinking_parts.is_empty() {
        None
    } else {
        Some(thinking_parts.join("\n\n"))
    };

    (thinking, result.trim().to_string())
}

/// Strip `<think>...</think>` and `<thinking>...</thinking>` blocks from LLM responses.
///
/// Reasoning models (Qwen, DeepSeek-R1) emit thinking blocks that should
/// not appear in the final task output. Case-insensitive to handle
/// `<Think>`, `<THINK>`, `<thinking>`, etc. Applied to ALL providers.
///
/// This is a convenience wrapper around [`extract_thinking_tags`] that discards
/// the extracted thinking content and returns only the cleaned text.
pub(crate) fn strip_think_tags(text: &str) -> String {
    let (_thinking, clean) = extract_thinking_tags(text);
    clean
}

#[cfg(test)]
mod tests {
    use super::{extract_thinking_tags, strip_think_tags};

    // ========================================================================
    // extract_thinking_tags tests
    // ========================================================================

    #[test]
    fn extract_simple() {
        let (t, c) = extract_thinking_tags("<think>reasoning</think>answer");
        assert_eq!(t.unwrap(), "reasoning");
        assert_eq!(c, "answer");
    }

    #[test]
    fn extract_no_tags() {
        let (t, c) = extract_thinking_tags("just text");
        assert!(t.is_none());
        assert_eq!(c, "just text");
    }

    #[test]
    fn extract_empty_think() {
        let (t, c) = extract_thinking_tags("<think></think>answer");
        assert!(t.is_none());
        assert_eq!(c, "answer");
    }

    #[test]
    fn extract_multiple_blocks() {
        let (t, c) = extract_thinking_tags("<think>first</think>A<think>second</think>B");
        assert_eq!(t.unwrap(), "first\n\nsecond");
        assert_eq!(c, "AB");
    }

    #[test]
    fn extract_thinking_variant() {
        let (t, c) = extract_thinking_tags("<thinking>DeepSeek reasoning</thinking>The answer.");
        assert_eq!(t.unwrap(), "DeepSeek reasoning");
        assert_eq!(c, "The answer.");
    }

    #[test]
    fn extract_case_insensitive() {
        let (t, c) = extract_thinking_tags("<THINK>loud reasoning</THINK>Result");
        assert_eq!(t.unwrap(), "loud reasoning");
        assert_eq!(c, "Result");
    }

    #[test]
    fn extract_unclosed_tag() {
        let (t, c) = extract_thinking_tags("<think>thinking forever...");
        assert_eq!(t.unwrap(), "thinking forever...");
        assert_eq!(c, "");
    }

    // ========================================================================
    // strip_think_tags tests (backward compatibility)
    // ========================================================================

    #[test]
    fn strip_think_tags_no_tags() {
        assert_eq!(strip_think_tags("Hello world"), "Hello world");
    }

    #[test]
    fn strip_think_tags_simple() {
        let input = "<think>Let me reason about this...</think>The answer is 42.";
        assert_eq!(strip_think_tags(input), "The answer is 42.");
    }

    #[test]
    fn strip_think_tags_multiple() {
        let input = "<think>first</think>A<think>second</think>B";
        assert_eq!(strip_think_tags(input), "AB");
    }

    #[test]
    fn strip_think_tags_with_newlines() {
        let input = "<think>\nLet me think step by step:\n1. First\n2. Second\n</think>\nThe result is correct.";
        assert_eq!(strip_think_tags(input), "The result is correct.");
    }

    #[test]
    fn strip_think_tags_unclosed() {
        let input = "<think>thinking forever...";
        assert_eq!(strip_think_tags(input), "");
    }

    #[test]
    fn strip_think_tags_case_insensitive() {
        assert_eq!(strip_think_tags("<Think>reasoning</Think>Answer"), "Answer");
        assert_eq!(
            strip_think_tags("<THINK>loud thinking</THINK>Result"),
            "Result"
        );
    }

    #[test]
    fn strip_thinking_tags() {
        let input = "<thinking>DeepSeek-R1 style reasoning block</thinking>The answer.";
        assert_eq!(strip_think_tags(input), "The answer.");
    }

    #[test]
    fn strip_thinking_tags_case_insensitive() {
        assert_eq!(strip_think_tags("<Thinking>mixed case</Thinking>OK"), "OK");
    }

    #[test]
    fn strip_think_preserves_utf8() {
        let input = "<think>reflexion en francais</think>Reponse: cafe et creme brulee";
        assert_eq!(strip_think_tags(input), "Reponse: cafe et creme brulee");
    }

    #[test]
    fn strip_think_turkish_i_no_panic() {
        // U+0130 İ (2 bytes) lowercases to i + U+0307 (3 bytes) — byte lengths diverge
        let input = "İstanbul<think>reasoning</think>answer";
        assert_eq!(strip_think_tags(input), "İstanbulanswer");
    }

    #[test]
    fn strip_think_german_sharp_s_no_panic() {
        // ß (2 bytes) uppercases to SS but lowercase is stable; test mixed
        let input = "Straße<THINK>thinking</THINK>Ergebnis";
        assert_eq!(strip_think_tags(input), "StraßeErgebnis");
    }

    #[test]
    fn strip_think_emoji_before_tag() {
        let input = "🦋<think>reasoning</think>butterfly";
        assert_eq!(strip_think_tags(input), "🦋butterfly");
    }

    #[test]
    fn resource_text_non_json_returns_string() {
        let text = "Hello, this is plain text from a resource";
        let content_text: Option<String> = Some(text.to_string());
        let result: serde_json::Value = content_text
            .map(|t| serde_json::from_str(&t).unwrap_or(serde_json::Value::String(t)))
            .unwrap_or(serde_json::Value::Null);
        assert!(
            result.is_string(),
            "Non-JSON text should be String, not Null"
        );
        assert_eq!(result.as_str().unwrap(), text);
    }

    #[test]
    fn resource_text_json_returns_parsed() {
        let text = r#"{"key": "value"}"#;
        let content_text: Option<String> = Some(text.to_string());
        let result: serde_json::Value = content_text
            .map(|t| serde_json::from_str(&t).unwrap_or(serde_json::Value::String(t)))
            .unwrap_or(serde_json::Value::Null);
        assert!(result.is_object());
    }

    #[test]
    fn resource_text_none_returns_null() {
        let content_text: Option<String> = None;
        let result: serde_json::Value = content_text
            .map(|t| serde_json::from_str(&t).unwrap_or(serde_json::Value::String(t)))
            .unwrap_or(serde_json::Value::Null);
        assert!(result.is_null());
    }

    // ========================================================================
    // coerce_json_types tests
    // ========================================================================

    use super::coerce_json_types;

    #[test]
    fn coerce_json_types_i64() {
        let mut v = serde_json::json!("42");
        coerce_json_types(&mut v);
        assert_eq!(v, serde_json::json!(42));
    }

    #[test]
    fn coerce_json_types_large_u64() {
        // u64::MAX = 18446744073709551615, i64::MAX = 9223372036854775807
        // This value is > i64::MAX but valid u64 — must not lose precision via f64
        let mut v = serde_json::json!("9999999999999999999");
        coerce_json_types(&mut v);
        assert!(v.is_number(), "should be coerced to a number");
        assert_eq!(
            v.as_u64(),
            Some(9_999_999_999_999_999_999u64),
            "large u64 must preserve full precision"
        );
    }

    #[test]
    fn coerce_json_types_f64() {
        let mut v = serde_json::json!("3.14");
        coerce_json_types(&mut v);
        #[allow(clippy::approx_constant)]
        let expected = serde_json::json!(3.14);
        assert_eq!(v, expected);
    }

    #[test]
    fn coerce_json_types_bool_true() {
        let mut v = serde_json::json!("true");
        coerce_json_types(&mut v);
        assert_eq!(v, serde_json::json!(true));
    }

    #[test]
    fn coerce_json_types_bool_false() {
        let mut v = serde_json::json!("false");
        coerce_json_types(&mut v);
        assert_eq!(v, serde_json::json!(false));
    }

    #[test]
    fn coerce_preserves_null_string() {
        // "null" should remain as string, NOT be coerced to Value::Null
        let mut v = serde_json::json!("null");
        coerce_json_types(&mut v);
        assert_eq!(v, serde_json::json!("null"));
    }

    #[test]
    fn coerce_json_string_to_array() {
        // BUG-5: {{with.terms | to_json}} produces a JSON array as string
        let mut v = serde_json::json!(r#"["hello","world"]"#);
        coerce_json_types(&mut v);
        assert!(v.is_array(), "JSON array string should be coerced to array");
        assert_eq!(v, serde_json::json!(["hello", "world"]));
    }

    #[test]
    fn coerce_json_string_to_object() {
        let mut v = serde_json::json!(r#"{"key":"value","n":42}"#);
        coerce_json_types(&mut v);
        assert!(
            v.is_object(),
            "JSON object string should be coerced to object"
        );
        assert_eq!(v["key"], serde_json::json!("value"));
        assert_eq!(v["n"], serde_json::json!(42));
    }

    #[test]
    fn coerce_json_string_invalid_json_stays_string() {
        // Strings that look like JSON but aren't should stay as strings
        let mut v = serde_json::json!("[not valid json");
        coerce_json_types(&mut v);
        assert!(v.is_string(), "Invalid JSON should remain as string");
    }

    #[test]
    fn coerce_json_nested_with_array_value() {
        // Real use case: json: { texts: "{{with.terms | to_json}}" }
        let mut v = serde_json::json!({"texts": "[\"bonjour\",\"merci\"]", "target": "fr"});
        coerce_json_types(&mut v);
        assert!(v["texts"].is_array(), "texts should be coerced to array");
        assert_eq!(v["texts"], serde_json::json!(["bonjour", "merci"]));
        assert_eq!(v["target"], serde_json::json!("fr")); // stays string
    }

    #[test]
    fn coerce_json_types_nested_object() {
        let mut v = serde_json::json!({"count": "100", "active": "true"});
        coerce_json_types(&mut v);
        assert_eq!(v["count"], serde_json::json!(100));
        assert_eq!(v["active"], serde_json::json!(true));
    }

    // ========================================================================
    // TDD tests for agent response extraction fix
    // ========================================================================

    #[test]
    fn agent_response_preserves_json_object() {
        let mut output = serde_json::Map::new();
        let obj = serde_json::json!({"title": "Hello", "score": 42});
        output.insert("response".to_string(), obj.clone());
        let final_output = serde_json::Value::Object(output);

        let response = match final_output.get("response") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        };

        // Compare as parsed JSON (key order is non-deterministic)
        let parsed: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed, serde_json::json!({"title": "Hello", "score": 42}));
    }

    #[test]
    fn agent_response_preserves_json_array() {
        let mut output = serde_json::Map::new();
        let arr = serde_json::json!(["item1", "item2", "item3"]);
        output.insert("response".to_string(), arr);
        let final_output = serde_json::Value::Object(output);

        let response = match final_output.get("response") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        };

        assert_eq!(response, r#"["item1","item2","item3"]"#);
    }

    #[test]
    fn agent_response_preserves_string() {
        let mut output = serde_json::Map::new();
        output.insert(
            "response".to_string(),
            serde_json::Value::String("Hello world".to_string()),
        );
        let final_output = serde_json::Value::Object(output);

        let response = match final_output.get("response") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        };

        assert_eq!(response, "Hello world");
    }

    #[test]
    fn agent_response_handles_number() {
        let mut output = serde_json::Map::new();
        output.insert("response".to_string(), serde_json::json!(42));
        let final_output = serde_json::Value::Object(output);

        let response = match final_output.get("response") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        };

        assert_eq!(response, "42");
    }

    #[test]
    fn agent_response_handles_boolean() {
        let mut output = serde_json::Map::new();
        output.insert("response".to_string(), serde_json::json!(true));
        let final_output = serde_json::Value::Object(output);

        let response = match final_output.get("response") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        };

        assert_eq!(response, "true");
    }

    #[test]
    fn agent_response_handles_null() {
        let mut output = serde_json::Map::new();
        output.insert("response".to_string(), serde_json::Value::Null);
        let final_output = serde_json::Value::Object(output);

        let response = match final_output.get("response") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        };

        assert_eq!(response, "null");
    }

    #[test]
    fn agent_response_handles_missing_key() {
        let output = serde_json::Map::new();
        let final_output = serde_json::Value::Object(output);

        let response = match final_output.get("response") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        };

        assert_eq!(response, "");
    }

    // =========================================================================
    // Vision Helper Tests
    // =========================================================================

    #[test]
    fn detect_image_media_type_png() {
        let data = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let result = super::detect_image_media_type(&data);
        assert_eq!(result, Some(rig::completion::message::ImageMediaType::PNG));
    }

    #[test]
    fn detect_image_media_type_jpeg() {
        let data = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        let result = super::detect_image_media_type(&data);
        assert_eq!(result, Some(rig::completion::message::ImageMediaType::JPEG));
    }

    #[test]
    fn detect_image_media_type_gif() {
        let data = b"GIF89a\x00\x00";
        let result = super::detect_image_media_type(data);
        assert_eq!(result, Some(rig::completion::message::ImageMediaType::GIF));
    }

    #[test]
    fn detect_image_media_type_webp() {
        let data = b"RIFF\x00\x00\x00\x00WEBP";
        let result = super::detect_image_media_type(data);
        assert_eq!(result, Some(rig::completion::message::ImageMediaType::WEBP));
    }

    #[test]
    fn detect_image_media_type_unknown() {
        let data = [0x00, 0x01, 0x02, 0x03];
        let result = super::detect_image_media_type(&data);
        assert_eq!(result, None);
    }

    #[test]
    fn detect_image_media_type_too_small() {
        let data = [0x89, 0x50];
        let result = super::detect_image_media_type(&data);
        assert_eq!(result, None);
    }

    // ========================================================================
    // redact_for_event tests
    // ========================================================================

    use super::redact_for_event;

    #[test]
    fn redact_for_event_redacts_anthropic_key() {
        let input = "Authorization: sk-ant-api03-abcdef1234567890abcdef";
        let result = redact_for_event(input);
        assert!(
            result.contains("[REDACTED]"),
            "Anthropic key not redacted: {result}"
        );
        assert!(!result.contains("sk-ant-api03"));
    }

    #[test]
    fn redact_for_event_redacts_openai_key() {
        let input = "key=sk-proj-abc123def456ghi789";
        let result = redact_for_event(input);
        assert!(
            result.contains("[REDACTED]"),
            "OpenAI key not redacted: {result}"
        );
        assert!(!result.contains("sk-proj-abc"));
    }

    #[test]
    fn redact_for_event_redacts_bearer_token() {
        let input = "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
        let result = redact_for_event(input);
        assert!(
            result.contains("[REDACTED]"),
            "Bearer token not redacted: {result}"
        );
        assert!(!result.contains("eyJhbGci"));
    }

    #[test]
    fn redact_for_event_redacts_github_token() {
        let input = "token: ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij";
        let result = redact_for_event(input);
        assert!(
            result.contains("[REDACTED]"),
            "GitHub PAT not redacted: {result}"
        );
    }

    #[test]
    fn redact_for_event_preserves_safe_strings() {
        let input = "Hello world, this is a normal prompt";
        assert_eq!(redact_for_event(input), input);
    }

    #[test]
    fn redact_for_event_truncates_long_strings() {
        let input = "a".repeat(300);
        let result = redact_for_event(&input);
        assert!(result.contains("... (300 bytes)"));
    }
}
