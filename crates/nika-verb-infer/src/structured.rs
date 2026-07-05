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

/// Cap on the schema text re-injected into prompts (retry + instruction
/// fallback) — an attacker-influenced schema must not be a token-cost
/// amplifier across retries (review lens 3 · P1).
const SCHEMA_RENDER_CAP: usize = 4096;

/// Compile the task `schema:` once per `run()` (review lenses 1+3 · P1).
///
/// A schema that does not compile is a task-authoring error — the caller
/// maps it to `InvalidParam` BEFORE any provider call is spent.
pub(crate) fn compile_schema(schema: &serde_json::Value) -> Result<jsonschema::Validator, String> {
    jsonschema::validator_for(schema).map_err(|e| e.to_string())
}

/// Render the schema for prompt injection, truncated at the cap.
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

/// Extract a JSON candidate from model text and validate it.
///
/// Extraction is lenient by design — models wrap JSON in prose and code
/// fences. Strategy: try the whole trimmed text first, then the first
/// fenced block, then the first balanced `{…}` or `[…]` span.
pub(crate) fn extract_and_validate(text: &str, validator: &jsonschema::Validator) -> Validation {
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

/// Is the task schema UNDERSPECIFIED for a strict provider mode? — any
/// node typed `object` without `properties`, or typed `array` without
/// `items`, anywhere in the tree (F2 · ADR-098).
///
/// The strict wire modes reject exactly this class (`OpenAI`'s
/// `json_schema`+`strict` 400s on it); the verb then requests the
/// provider's native JSON mode instead and validates LOCALLY against the
/// user schema — the schema means "free-form here", which no strict mode
/// can express. Iterative walk (an explicit worklist · no recursion), so
/// a deeply-nested authored schema can never overflow the stack.
pub(crate) fn is_underspecified(schema: &serde_json::Value) -> bool {
    let mut stack = vec![schema];
    while let Some(node) = stack.pop() {
        let Some(obj) = node.as_object() else {
            // Boolean schemas (`true`/`false`) carry no object/array
            // shape for a strict mode to choke on.
            continue;
        };
        if node_lacks_shape(obj) {
            return true;
        }
        push_subschemas(obj, &mut stack);
    }
    false
}

/// One node's verdict: declares the type but not its shape.
fn node_lacks_shape(obj: &serde_json::Map<String, serde_json::Value>) -> bool {
    (declares_type(obj, "object") && !obj.contains_key("properties"))
        || (declares_type(obj, "array") && !obj.contains_key("items"))
}

/// `"type": "object"` or `"type": ["object", "null"]` — both forms count.
fn declares_type(obj: &serde_json::Map<String, serde_json::Value>, name: &str) -> bool {
    match obj.get("type") {
        Some(serde_json::Value::String(t)) => t == name,
        Some(serde_json::Value::Array(list)) => list.iter().any(|t| t.as_str() == Some(name)),
        _ => false,
    }
}

/// Push every child subschema of one node onto the worklist.
fn push_subschemas<'a>(
    obj: &'a serde_json::Map<String, serde_json::Value>,
    stack: &mut Vec<&'a serde_json::Value>,
) {
    for key in ["properties", "patternProperties", "$defs", "definitions"] {
        if let Some(map) = obj.get(key).and_then(serde_json::Value::as_object) {
            stack.extend(map.values());
        }
    }
    for key in ["anyOf", "allOf", "oneOf", "prefixItems"] {
        if let Some(list) = obj.get(key).and_then(serde_json::Value::as_array) {
            stack.extend(list.iter());
        }
    }
    match obj.get("items") {
        // Tuple form (draft-07) — each position is a subschema.
        Some(serde_json::Value::Array(list)) => stack.extend(list.iter()),
        Some(single) => stack.push(single),
        None => {}
    }
    // `additionalProperties: { …schema… }` shapes the open remainder —
    // the bool form is a gate, not a subschema.
    if let Some(extra) = obj.get("additionalProperties")
        && extra.is_object()
    {
        stack.push(extra);
    }
}

/// The corrective user message appended on a validation retry.
pub(crate) fn retry_message(detail: &str, schema: &serde_json::Value) -> String {
    let rendered = render_schema(schema);
    format!(
        "The previous reply did not satisfy the required output schema. \
         Validation failed with: {detail}. Reply again with ONLY a JSON value \
         that satisfies this JSON Schema, no prose, no code fences:\n{rendered}"
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

    fn validator(schema: &serde_json::Value) -> jsonschema::Validator {
        compile_schema(schema).expect("test schema compiles")
    }

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
        let v = extract_and_validate(r#"{"name":"Ada","age":36}"#, &validator(&person_schema()));
        match v {
            Validation::Valid(val) => assert_eq!(val["name"], "Ada"),
            Validation::Invalid(d) => panic!("expected valid, got: {d}"),
        }
    }

    #[test]
    fn fenced_json_is_extracted() {
        let text = "Here you go:\n```json\n{\"name\":\"Ada\",\"age\":36}\n```\nDone.";
        assert!(matches!(
            extract_and_validate(text, &validator(&person_schema())),
            Validation::Valid(_)
        ));
    }

    #[test]
    fn json_embedded_in_prose_is_extracted() {
        // Real models wrap JSON in prose (`Here's the data: {...}`) — the
        // balanced-span layer digs it out. (The mock provider synthesizes
        // pure JSON for schema requests since F3; this layer serves the
        // REAL providers' noisy replies.)
        let text = r#"mock(echo) · {"name":"Ada","age":36}"#;
        assert!(matches!(
            extract_and_validate(text, &validator(&person_schema())),
            Validation::Valid(_)
        ));
    }

    #[test]
    fn balanced_span_ignores_braces_inside_strings() {
        let text = r#"noise {"name":"A{d}a","age":1} trail"#;
        match extract_and_validate(text, &validator(&person_schema())) {
            Validation::Valid(val) => assert_eq!(val["name"], "A{d}a"),
            Validation::Invalid(d) => panic!("expected valid, got: {d}"),
        }
    }

    #[test]
    fn schema_violation_reports_path() {
        let v = extract_and_validate(r#"{"name":"Ada","age":-3}"#, &validator(&person_schema()));
        match v {
            Validation::Invalid(d) => assert!(d.contains("age"), "detail names the path: {d}"),
            Validation::Valid(_) => panic!("expected invalid"),
        }
    }

    #[test]
    fn no_json_at_all_is_invalid() {
        assert!(matches!(
            extract_and_validate("plain prose, nothing else", &validator(&person_schema())),
            Validation::Invalid(_)
        ));
    }

    #[test]
    fn arrays_are_supported() {
        let schema = json!({ "type": "array", "items": { "type": "integer" } });
        assert!(matches!(
            extract_and_validate("the list is [1, 2, 3] ok", &validator(&schema)),
            Validation::Valid(_)
        ));
    }

    // ── F2 · underspecified-schema detection (ADR-098) ───────────────

    #[test]
    fn bare_object_and_bare_array_are_underspecified() {
        // The field repro: translate-payload's "return the SAME free-form
        // object" — OpenAI strict 400s "object schema missing properties".
        assert!(is_underspecified(&json!({ "type": "object" })));
        assert!(is_underspecified(&json!({ "type": "array" })));
        // The type-array form counts too.
        assert!(is_underspecified(&json!({ "type": ["object", "null"] })));
    }

    #[test]
    fn underspecified_is_detected_anywhere_in_the_tree() {
        // The field repro's SECOND rung: head/sections declared but
        // themselves shapeless — "array schema missing items".
        assert!(is_underspecified(&json!({
            "type": "object",
            "properties": {
                "head": { "type": "object" },
                "sections": { "type": "array" }
            },
            "required": ["head", "sections"]
        })));
        // Nested through items · $defs · anyOf.
        assert!(is_underspecified(&json!({
            "type": "array",
            "items": { "type": "object" }
        })));
        assert!(is_underspecified(&json!({
            "type": "object",
            "properties": { "x": { "$ref": "#/$defs/free" } },
            "$defs": { "free": { "type": "object" } }
        })));
        assert!(is_underspecified(&json!({
            "anyOf": [{ "type": "string" }, { "type": "object" }]
        })));
        // `additionalProperties: {…}` subschema is walked.
        assert!(is_underspecified(&json!({
            "type": "object",
            "properties": { "k": { "type": "string" } },
            "additionalProperties": { "type": "array" }
        })));
    }

    #[test]
    fn fully_specified_schemas_are_not_flagged() {
        // The person schema + the atlas-style review schema keep today's
        // strict path untouched.
        assert!(!is_underspecified(&person_schema()));
        assert!(!is_underspecified(&json!({
            "type": "object",
            "required": ["verdict", "findings"],
            "properties": {
                "verdict": { "type": "string", "enum": ["P0", "P1"] },
                "findings": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": { "detail": { "type": "string" } }
                    }
                }
            }
        })));
        // Scalars + boolean gates never trip the rule.
        assert!(!is_underspecified(&json!({ "type": "string" })));
        assert!(!is_underspecified(&json!({
            "type": "object",
            "properties": { "k": { "type": "integer" } },
            "additionalProperties": false
        })));
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

    #[test]
    fn fence_layer_is_load_bearing() {
        // An unbalanced `{` BEFORE the fence poisons the balanced-span
        // layer, and the fenced value is a bare string the span layer
        // can't see — only the fence extraction can succeed here.
        let text = "context { broken\n```json\n\"just a string\"\n```\nafter";
        assert_eq!(extract_json(text), Some(serde_json::json!("just a string")));
    }

    #[test]
    fn fence_with_language_tag_skips_the_tag_line() {
        // Fence not at offset 0 + a language tag: the index arithmetic
        // (start+3 · newline+1) must land exactly on the body.
        let text = "x\n```yaml\n[7]\n```";
        assert_eq!(extract_json(text), Some(serde_json::json!([7])));
    }

    #[test]
    fn nested_objects_need_real_depth_counting() {
        // A short-circuiting depth counter would cut at the inner `}`
        // and yield invalid JSON.
        let text = r#"reply: {"a":{"b":1},"c":2} end"#;
        assert_eq!(
            extract_json(text),
            Some(serde_json::json!({"a":{"b":1},"c":2}))
        );
    }

    #[test]
    fn closing_brace_inside_string_does_not_close_the_span() {
        // Without string-awareness the `}` inside the value closes the
        // span early and the candidate fails to parse.
        let text = r#"out: {"s":"}"} done"#;
        assert_eq!(extract_json(text), Some(serde_json::json!({"s":"}"})));
    }

    #[test]
    fn escaped_quote_inside_string_stays_in_string() {
        // The `\"` must not flip the in-string flag — the `}` after it is
        // still string content.
        let text = r#"{"s":"a\"}x"}"#;
        assert_eq!(extract_json(text), Some(serde_json::json!({"s":"a\"}x"})));
    }

    #[test]
    fn array_span_must_close_with_a_bracket() {
        // `[` opens an array span; a stray `}` must not close it.
        let text = "data [1, 2} oops [3, 4] real";
        // The first `[` span is `[1, 2} oops [3, 4]` — unparseable. The
        // extractor deliberately does NOT backtrack to later candidates
        // (first-plausible-span contract); it returns None rather than
        // mis-pairing delimiters.
        assert_eq!(extract_json(text), None);
    }
}
