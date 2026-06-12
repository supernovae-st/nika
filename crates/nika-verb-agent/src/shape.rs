// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Final-output shaping — `schema:` validation + JSON extraction from
//! model text (spec §2 `infer.schema:` parity).
//!
//! Moved verbatim out of `lib.rs` (the loop file) so the loop file stays
//! within its size budget as the ADR-093 intelligence modules land —
//! behavior is pinned by the tests that moved with it.

use crate::{AgentValue, VerbAgentError};

/// Validate the final value against the task `schema:` when present
/// (spec §2: the schema validates the FINAL output · NIKA-464).
pub(crate) fn shape_output(
    value: AgentValue,
    schema: Option<&serde_json::Value>,
) -> Result<AgentValue, VerbAgentError> {
    let Some(schema) = schema else {
        return Ok(value);
    };
    let validator =
        jsonschema::validator_for(schema).map_err(|e| VerbAgentError::SchemaValidation {
            detail: format!("schema does not compile: {e}"),
        })?;
    let candidate = match &value {
        AgentValue::Structured(json) => json.clone(),
        // The `infer.schema:` parity (spec §2): a final message wrapping
        // its JSON in ``` fences or prose is the common real-model case —
        // bare-parse first, then the first balanced span.
        AgentValue::Text(text) => {
            extract_json(text).ok_or_else(|| VerbAgentError::SchemaValidation {
                detail: "final message contains no extractable JSON".to_owned(),
            })?
        }
    };
    if let Err(error) = validator.validate(&candidate) {
        return Err(VerbAgentError::SchemaValidation {
            detail: error.to_string(),
        });
    }
    Ok(AgentValue::Structured(candidate))
}

/// Pull the first JSON value out of model text: a bare parse, else the
/// first balanced `{…}`/`[…]` span (tolerates code fences + prose).
/// String-aware so a brace inside a string literal never miscounts depth.
///
/// THREE layers, IDENTICAL to `nika-verb-infer`'s structured extraction
/// (spec §2 `infer.schema:` parity): bare parse → first fenced block →
/// first balanced span. The fence layer is load-bearing — a model that
/// fences a BARE scalar (` ```json\n"a string"\n``` `) after an unbalanced
/// `{` is extractable ONLY by the fence (the span layer is poisoned, the
/// bare parse fails); without this layer the agent verb would reject under
/// `schema:` a final message the infer verb accepts. The duplication is
/// intentional (the DAG forbids a shared L2 sibling home for 60 LOC, and a
/// dedicated crate isn't worth 12 gates) — kept honest by the SHARED test
/// vectors below, ported verbatim from infer's load-bearing suite.
pub(crate) fn extract_json(text: &str) -> Option<serde_json::Value> {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Some(value);
    }
    if let Some(fenced) = first_fenced_block(trimmed)
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(fenced)
    {
        return Some(value);
    }
    let span = balanced_span(trimmed)?;
    serde_json::from_str(span).ok()
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

/// The first balanced `{…}` or `[…]` substring, honoring string literals.
fn balanced_span(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let start = bytes.iter().position(|&b| b == b'{' || b == b'[')?;
    let (open, close) = if bytes[start] == b'{' {
        (b'{', b'}')
    } else {
        (b'[', b']')
    };
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_str {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            _ if b == open => depth += 1,
            _ if b == close => {
                depth -= 1;
                if depth == 0 {
                    return text.get(start..=i);
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

    #[test]
    fn balanced_span_is_string_aware_and_bracket_typed() {
        // A brace inside a string literal must not miscount depth. The
        // in-string close-brace is written `\u{7d}` so the source stays
        // brace-balanced for the naive expect-ratchet scanner (the runtime
        // value is the same string with a literal close-brace in the
        // middle of the `k` field).
        assert_eq!(
            extract_json("prefix {\"k\": \"a\u{7d}b\", \"n\": [1, {\"x\": 2}]} suffix"),
            Some(serde_json::json!({"k": "a\u{7d}b", "n": [1, {"x": 2}]}))
        );
        // First balanced span wins; a top-level array is found too.
        assert_eq!(
            extract_json("noise [1, 2, 3] more"),
            Some(serde_json::json!([1, 2, 3]))
        );
        // No JSON at all → None (→ SchemaValidation upstream).
        assert_eq!(extract_json("just prose, no braces"), None);
    }

    // ── parity vectors · ported VERBATIM from nika-verb-infer's
    //    structured.rs load-bearing suite (the contract that keeps the
    //    two extractors identical · J1 review fold) ─────────────────────

    #[test]
    fn fenced_json_is_extracted() {
        let text = "Here you go:\n```json\n{\"name\":\"Ada\",\"age\":36}\n```\nDone.";
        assert_eq!(
            extract_json(text),
            Some(serde_json::json!({"name": "Ada", "age": 36}))
        );
    }

    #[test]
    fn fence_layer_is_load_bearing() {
        // An unbalanced `{` BEFORE the fence poisons the balanced-span
        // layer, and the fenced value is a bare string the span layer
        // can't see — only the fence extraction can succeed here. THIS is
        // the message the agent verb used to reject while infer accepted.
        let text = "context { broken\n```json\n\"just a string\"\n```\nafter";
        assert_eq!(extract_json(text), Some(serde_json::json!("just a string")));
    }

    #[test]
    fn fence_with_language_tag_skips_the_tag_line() {
        let text = "x\n```yaml\n[7]\n```";
        assert_eq!(extract_json(text), Some(serde_json::json!([7])));
    }

    #[test]
    fn nested_objects_need_real_depth_counting() {
        let text = r#"reply: {"a":{"b":1},"c":2} end"#;
        assert_eq!(
            extract_json(text),
            Some(serde_json::json!({"a": {"b": 1}, "c": 2}))
        );
    }

    #[test]
    fn unterminated_span_is_not_extracted() {
        assert_eq!(extract_json("start { \"a\": 1 and never closes"), None);
    }
}
