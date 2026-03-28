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
            } else if s == "null" {
                *value = Value::Null;
            }
        }
        _ => {}
    }
}

/// Truncate resolved template for event logging (avoids leaking secrets from $env).
#[inline]
pub(crate) fn redact_for_event(s: &str) -> String {
    if s.len() <= 200 {
        s.to_string()
    } else {
        // Find the last valid char boundary at or before byte 200
        let mut boundary = 200;
        while boundary > 0 && !s.is_char_boundary(boundary) {
            boundary -= 1;
        }
        format!("{}... ({} bytes)", &s[..boundary], s.len())
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

/// Strip `<think>...</think>` and `<thinking>...</thinking>` blocks from LLM responses.
///
/// Reasoning models (Qwen, DeepSeek-R1) emit thinking blocks that should
/// not appear in the final task output. Case-insensitive to handle
/// `<Think>`, `<THINK>`, `<thinking>`, etc. Applied to ALL providers.
pub(crate) fn strip_think_tags(text: &str) -> String {
    let lower = text.to_lowercase();

    // Fast path: no think tags at all (case-insensitive check)
    if !lower.contains("<think") {
        return text.to_string();
    }

    // Use a simple state machine on the lowercase version to find tag positions,
    // then slice from the original text to preserve non-tag content.
    let mut result = String::with_capacity(text.len());
    let bytes = lower.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Try to match opening tag: <think> or <thinking>
        if bytes[i] == b'<' {
            let tag_end = if lower[i..].starts_with("<thinking>") {
                Some(("</thinking>", 10)) // open tag len
            } else if lower[i..].starts_with("<think>") {
                Some(("</think>", 7)) // open tag len
            } else {
                None
            };

            if let Some((close_tag, open_len)) = tag_end {
                // Find the matching close tag
                if let Some(close_pos) = lower[i + open_len..].find(close_tag) {
                    // Skip the entire block: open tag + content + close tag
                    i = i + open_len + close_pos + close_tag.len();
                    continue;
                } else {
                    // Unclosed tag — strip everything from here
                    break;
                }
            }
        }

        // Safe: we're iterating byte-by-byte but the tags are ASCII.
        // Copy char-by-char from original text to preserve UTF-8.
        let ch = text[i..].chars().next().unwrap();
        result.push(ch);
        i += ch.len_utf8();
    }

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::strip_think_tags;

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
        assert_eq!(
            strip_think_tags("<Think>reasoning</Think>Answer"),
            "Answer"
        );
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
        assert_eq!(
            strip_think_tags("<Thinking>mixed case</Thinking>OK"),
            "OK"
        );
    }

    #[test]
    fn strip_think_preserves_utf8() {
        let input = "<think>reflexion en francais</think>Reponse: cafe et creme brulee";
        assert_eq!(strip_think_tags(input), "Reponse: cafe et creme brulee");
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
        assert_eq!(v, serde_json::json!(3.14));
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
    fn coerce_json_types_null() {
        let mut v = serde_json::json!("null");
        coerce_json_types(&mut v);
        assert_eq!(v, serde_json::Value::Null);
    }

    #[test]
    fn coerce_json_types_nested_object() {
        let mut v = serde_json::json!({"count": "100", "active": "true"});
        coerce_json_types(&mut v);
        assert_eq!(v["count"], serde_json::json!(100));
        assert_eq!(v["active"], serde_json::json!(true));
    }

    // ========================================================================
    // Wave 2: Deep Audit - Bug-Proving Tests
    // ========================================================================

    // ---- BUG: run_agent response extraction loses non-string JSON ----
    // In verbs.rs lines 1153-1158:
    //   let response = result.final_output
    //       .get("response")
    //       .and_then(|v| v.as_str())
    //       .unwrap_or("");
    //
    // If the agent's response is a JSON object (e.g., from structured output),
    // `as_str()` returns None because it's not a string, and the entire response
    // is silently replaced with an empty string.
    //
    // FIX: Use a more robust extraction:
    //   let response = match result.final_output.get("response") {
    //       Some(Value::String(s)) => s.clone(),
    //       Some(v) => v.to_string(), // Serialize non-string JSON
    //       None => String::new(),
    //   };
    #[test]
    fn wave2_run_agent_response_extraction_loses_json_objects() {
        // Simulate what run_agent does when extracting the response
        // The agent wraps its output as: serde_json::json!({ "response": response })

        // Case 1: String response - works fine
        let output_string = serde_json::json!({ "response": "Hello world" });
        let extracted_string = output_string
            .get("response")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(extracted_string, "Hello world", "String extraction works");

        // Case 2: JSON object response - BUG: silently lost
        let output_object = serde_json::json!({
            "response": {
                "title": "AI Blog Post",
                "content": "This is a structured response",
                "metadata": { "word_count": 42 }
            }
        });
        let extracted_object = output_object
            .get("response")
            .and_then(|v| v.as_str()) // Returns None for objects!
            .unwrap_or("");

        // BUG PROVEN: The entire structured response is lost, replaced with ""
        assert_eq!(
            extracted_object, "",
            "BUG PROVEN: JSON object response is silently replaced with empty string. \
             The response field exists and contains valid JSON, but as_str() returns None \
             for non-string JSON values."
        );

        // Show what the correct extraction would look like
        let correct_extraction = output_object
            .get("response")
            .map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_default();
        assert!(
            !correct_extraction.is_empty(),
            "Correct extraction should preserve the JSON object"
        );
        assert!(
            correct_extraction.contains("AI Blog Post"),
            "Correct extraction should contain the title"
        );

        // Case 3: Array response - also lost
        let output_array = serde_json::json!({
            "response": ["item1", "item2", "item3"]
        });
        let extracted_array = output_array
            .get("response")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(
            extracted_array, "",
            "BUG PROVEN: Array responses are also silently lost"
        );

        // Case 4: Numeric response - also lost
        let output_number = serde_json::json!({ "response": 42 });
        let extracted_number = output_number
            .get("response")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(
            extracted_number, "",
            "BUG PROVEN: Numeric responses are also silently lost"
        );

        // Case 5: Boolean response - also lost
        let output_bool = serde_json::json!({ "response": true });
        let extracted_bool = output_bool
            .get("response")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(
            extracted_bool, "",
            "BUG PROVEN: Boolean responses are also silently lost"
        );
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
}
