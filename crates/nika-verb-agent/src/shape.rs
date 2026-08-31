// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Final-output shaping — `schema:` validation + JSON extraction from
//! model text (spec §2 `infer.schema:` parity).
//!
//! Two distinct surfaces meet the task `schema:` here (BUG#11):
//!
//! - **`nika:done` carrying a `result`** — the model DELIBERATELY emitted a
//!   structured value; [`shape_output`] validates it directly (the model
//!   committed to that exact shape · no re-ask).
//! - **a free-text final answer** (natural completion · `done` without a
//!   result) — the model answered in prose; the loop must turn that into a
//!   schema-conforming object. The wire enforcement lives in `lib.rs`
//!   (the schema-constrained re-ask), and this module supplies its parts:
//!   [`compile`], [`validate_text`], [`render_schema`], and [`reask_message`].
//!
//! Moved verbatim out of `lib.rs` (the loop file) so the loop file stays
//! within its size budget as the ADR-096 intelligence modules land —
//! behavior is pinned by the tests that moved with it.

use crate::{AgentValue, VerbAgentError};

/// Cap on the schema text re-injected into prompts (the instruction
/// fallback) — an attacker-influenced schema must not be a token-cost
/// amplifier across re-asks (nika-verb-infer review lens 3 · P1 parity).
const SCHEMA_RENDER_CAP: usize = 4096;

/// Compile the task `schema:` once (a schema that does not compile is a
/// task-authoring error mapped to NIKA-464 BEFORE any provider call).
pub(crate) fn compile(schema: &serde_json::Value) -> Result<jsonschema::Validator, VerbAgentError> {
    jsonschema::validator_for(schema).map_err(|e| VerbAgentError::SchemaValidation {
        spend: Box::default(),
        detail: format!("schema does not compile: {e}"),
    })
}

/// Extract a JSON candidate from a free-text final answer and validate it
/// against a compiled schema in ONE step (the agent re-ask's unit · BUG#11).
/// Returns the validated value, or the human-readable failure detail — the
/// caller decides between a re-ask and the NIKA-464 verdict, so a missing
/// candidate and a schema miss collapse to the same `Err(detail)` shape.
pub(crate) fn validate_text(
    text: &str,
    validator: &jsonschema::Validator,
) -> Result<serde_json::Value, String> {
    let Some(candidate) = extract_json(text) else {
        return Err("final message contains no extractable JSON".to_owned());
    };
    match validator.validate(&candidate) {
        Ok(()) => Ok(candidate),
        Err(error) => Err(error.to_string()),
    }
}

/// A `nika:done` `result:` that is a JSON string (or null) cannot
/// satisfy an object schema. Treat it as free text so the loop can
/// re-ask (C07 / issue 1301) instead of spend-then-INFER-002.
#[must_use]
pub(crate) fn string_or_null_may_reask(
    result: &serde_json::Value,
    schema: Option<&serde_json::Value>,
) -> bool {
    schema.is_some() && (result.is_string() || result.is_null())
}

/// If the model stuffed a JSON value inside a string (`"{\"k\":1}"`),
/// unwrap it so `schema:` can judge the inner value with zero extra
/// spend. A plain prose string is left as-is for the re-ask path.
#[must_use]
pub(crate) fn coerce_done_result(result: &serde_json::Value) -> serde_json::Value {
    if let Some(s) = result.as_str()
        && let Ok(inner) = serde_json::from_str::<serde_json::Value>(s)
    {
        return inner;
    }
    result.clone()
}

/// Validate the final value against the task `schema:` when present
/// (spec §2: the schema validates the FINAL output · NIKA-464).
///
/// This is the DIRECT path — the model already committed to a value (a
/// `nika:done` result, or a free-text answer that already parses). A
/// failure here is a verdict, not a prompt to re-ask (the re-ask lives in
/// `lib.rs` and is reserved for the free-text answer that does NOT yet
/// conform · BUG#11).
pub(crate) fn shape_output(
    value: AgentValue,
    schema: Option<&serde_json::Value>,
) -> Result<AgentValue, VerbAgentError> {
    let Some(schema) = schema else {
        return Ok(value);
    };
    let validator = compile(schema)?;
    let candidate = candidate_of(value)?;
    if let Err(error) = validator.validate(&candidate) {
        return Err(VerbAgentError::SchemaValidation {
            detail: error.to_string(),
            spend: Box::default(),
        });
    }
    Ok(AgentValue::Structured(candidate))
}

/// Pull the JSON candidate out of a final value: a structured value is
/// already JSON; a text value is parsed leniently (`infer.schema:` parity:
/// real models wrap JSON in code fences or prose · bare-parse first, then
/// the first balanced span).
pub(crate) fn candidate_of(value: AgentValue) -> Result<serde_json::Value, VerbAgentError> {
    match value {
        AgentValue::Structured(json) => Ok(json),
        AgentValue::Text(text) => {
            extract_json(&text).ok_or_else(|| VerbAgentError::SchemaValidation {
                detail: "final message contains no extractable JSON".to_owned(),
                spend: Box::default(),
            })
        }
    }
}

/// Render the schema for prompt injection, truncated at the cap (parity
/// with nika-verb-infer's `structured::render_schema`).
pub(crate) fn render_schema(schema: &serde_json::Value) -> String {
    let mut rendered = schema.to_string();
    if rendered.len() > SCHEMA_RENDER_CAP {
        let mut cut = SCHEMA_RENDER_CAP;
        while !rendered.is_char_boundary(cut) {
            cut -= 1;
        }
        rendered.truncate(cut);
        rendered.push_str("…(schema truncated)");
    }
    rendered
}

/// The instruction appended to the re-ask turn for providers WITHOUT
/// native `response_format` (the anthropic wire), and as the corrective
/// nudge when a re-ask still misses. Mirrors infer's `retry_message`.
pub(crate) fn reask_message(detail: Option<&str>, schema: &serde_json::Value) -> String {
    let rendered = render_schema(schema);
    match detail {
        Some(detail) => format!(
            "The previous reply did not satisfy the required output schema. \
             Validation failed with: {detail}. Reply again with ONLY a JSON value \
             that satisfies this JSON Schema, no prose, no code fences:\n{rendered}"
        ),
        None => format!(
            "Reply with ONLY a JSON value that satisfies this JSON Schema, \
             no prose, no code fences:\n{rendered}"
        ),
    }
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
    // `+ 1` skips the tag-line newline; the trailing `.trim()` then makes
    // the exact boundary irrelevant (a leading `\n` is whitespace it
    // removes) — so cargo-mutants flags `i + 1` → `i` as an EQUIVALENT
    // mutant. Kept honest: the +1 is the intent, the trim the safety net;
    // degrading the trim to make the mutant killable would be worse code.
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

    #[test]
    fn string_or_null_against_object_schema_may_reask() {
        let object = serde_json::json!({"type": "object", "required": ["findings"]});
        assert!(string_or_null_may_reask(
            &serde_json::json!("a prose final"),
            Some(&object)
        ));
        assert!(string_or_null_may_reask(
            &serde_json::Value::Null,
            Some(&object)
        ));
        assert!(!string_or_null_may_reask(
            &serde_json::json!({"findings": []}),
            Some(&object)
        ));
        assert_eq!(
            coerce_done_result(&serde_json::json!("{\"findings\":[\"a\"]}")),
            serde_json::json!({"findings": ["a"]})
        );
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
