// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Structured-output floor — JSON extraction + schema validation.
//!
//! The verb owns the floor layers of the structured-output recipe:
//! extract a JSON candidate from possibly-noisy model text (prose
//! wrapping, code fences), validate it against the task `schema:`, and
//! build the retry message when validation fails. The upper layers
//! (LLM-judge coercion · canary scanning) are engine/Shield scope.

/// Outcome of one extraction + validation pass.
#[derive(Debug)]
pub(crate) enum Validation {
    /// The candidate parsed and satisfied the schema.
    Valid(serde_json::Value),
    /// No JSON candidate could be extracted, or it failed the schema.
    /// Carries the human-readable detail used for the retry message.
    Invalid(String),
}

/// Extract a JSON candidate from model text and validate it.
///
/// Extraction is lenient by design — models wrap JSON in prose and code
/// fences. Strategy: try the whole trimmed text first, then the first
/// fenced block, then the first balanced `{…}` or `[…]` span.
pub(crate) fn extract_and_validate(text: &str, schema: &serde_json::Value) -> Validation {
    let validator = match jsonschema::validator_for(schema) {
        Ok(v) => v,
        Err(e) => return Validation::Invalid(format!("schema itself is invalid: {e}")),
    };

    let Some(candidate) = extract_json(text) else {
        return Validation::Invalid("no JSON value found in the model output".to_owned());
    };

    let rendered: Vec<String> = validator
        .iter_errors(&candidate)
        .take(5)
        .map(|e| {
            let path = e.instance_path.to_string();
            if path.is_empty() {
                e.to_string()
            } else {
                format!("at `{path}`: {e}")
            }
        })
        .collect();
    if rendered.is_empty() {
        Validation::Valid(candidate)
    } else {
        Validation::Invalid(rendered.join("; "))
    }
}

/// The corrective user message appended on a validation retry.
pub(crate) fn retry_message(detail: &str, schema: &serde_json::Value) -> String {
    format!(
        "The previous reply did not satisfy the required output schema. \
         Validation failed with: {detail}. Reply again with ONLY a JSON value \
         that satisfies this JSON Schema, no prose, no code fences:\n{schema}"
    )
}

/// Pull the first plausible JSON value out of free-form model text.
fn extract_json(text: &str) -> Option<serde_json::Value> {
    let trimmed = text.trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Some(v);
    }
    if let Some(fenced) = first_fenced_block(trimmed)
        && let Ok(v) = serde_json::from_str::<serde_json::Value>(fenced)
    {
        return Some(v);
    }
    first_balanced_span(trimmed)
        .and_then(|span| serde_json::from_str::<serde_json::Value>(span).ok())
}

/// The body of the first code fence (with or without a language tag).
fn first_fenced_block(text: &str) -> Option<&str> {
    let start = text.find("```")?;
    let after_ticks = &text[start + 3..];
    let body_start = after_ticks.find('\n').map_or(0, |i| i + 1);
    let body = &after_ticks[body_start..];
    let end = body.find("```")?;
    Some(body[..end].trim())
}

/// The first balanced `{…}` or `[…]` span, string-literal aware.
fn first_balanced_span(text: &str) -> Option<&str> {
    let open_idx = text.find(['{', '['])?;
    let bytes = text.as_bytes();
    let open = bytes[open_idx];
    let close = if open == b'{' { b'}' } else { b']' };
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate().skip(open_idx) {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            _ if b == open => depth += 1,
            _ if b == close => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(&text[open_idx..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn person_schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "age": { "type": "integer", "minimum": 0 }
            },
            "required": ["name", "age"],
            "additionalProperties": false
        })
    }

    #[test]
    fn bare_json_validates() {
        let v = extract_and_validate(r#"{"name":"Ada","age":36}"#, &person_schema());
        match v {
            Validation::Valid(val) => assert_eq!(val["name"], "Ada"),
            Validation::Invalid(d) => panic!("expected valid, got: {d}"),
        }
    }

    #[test]
    fn fenced_json_is_extracted() {
        let text = "Here you go:\n```json\n{\"name\":\"Ada\",\"age\":36}\n```\nDone.";
        assert!(matches!(
            extract_and_validate(text, &person_schema()),
            Validation::Valid(_)
        ));
    }

    #[test]
    fn json_embedded_in_prose_is_extracted() {
        // The mock provider echoes `mock(model) · {prompt}` — the balanced-span
        // layer is what makes the deterministic happy path work end-to-end.
        let text = r#"mock(echo) · {"name":"Ada","age":36}"#;
        assert!(matches!(
            extract_and_validate(text, &person_schema()),
            Validation::Valid(_)
        ));
    }

    #[test]
    fn balanced_span_ignores_braces_inside_strings() {
        let text = r#"noise {"name":"A{d}a","age":1} trail"#;
        match extract_and_validate(text, &person_schema()) {
            Validation::Valid(val) => assert_eq!(val["name"], "A{d}a"),
            Validation::Invalid(d) => panic!("expected valid, got: {d}"),
        }
    }

    #[test]
    fn schema_violation_reports_path() {
        let v = extract_and_validate(r#"{"name":"Ada","age":-3}"#, &person_schema());
        match v {
            Validation::Invalid(d) => assert!(d.contains("age"), "detail names the path: {d}"),
            Validation::Valid(_) => panic!("expected invalid"),
        }
    }

    #[test]
    fn no_json_at_all_is_invalid() {
        assert!(matches!(
            extract_and_validate("plain prose, nothing else", &person_schema()),
            Validation::Invalid(_)
        ));
    }

    #[test]
    fn arrays_are_supported() {
        let schema = json!({ "type": "array", "items": { "type": "integer" } });
        assert!(matches!(
            extract_and_validate("the list is [1, 2, 3] ok", &schema),
            Validation::Valid(_)
        ));
    }

    #[test]
    fn retry_message_carries_detail_and_schema() {
        let msg = retry_message("at `/age`: -3 is less than 0", &person_schema());
        assert!(msg.contains("/age"));
        assert!(msg.contains("\"required\""));
    }

    #[test]
    fn unterminated_span_is_not_extracted() {
        assert!(extract_json("start { \"a\": 1 and never closes").is_none());
    }
}
