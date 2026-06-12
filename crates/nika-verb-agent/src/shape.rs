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
/// NOTE · this mirrors `nika-verb-infer`'s structured extraction (spec §2
/// `infer.schema:` parity · pinned by the fenced-JSON test). The canonical
/// home for both is a future `nika-verb-common`; until that exists the
/// duplication is intentional and behavior-tested rather than a hidden
/// fork (both reviewers flagged the cross-crate dup · the test is the
/// contract that keeps them in step).
pub(crate) fn extract_json(text: &str) -> Option<serde_json::Value> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text.trim()) {
        return Some(value);
    }
    let span = balanced_span(text)?;
    serde_json::from_str(span).ok()
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
}
