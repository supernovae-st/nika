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
    /// Carries one rendered line per validation error (≤5) — the terminal
    /// detail joins them; the retry message numbers them as repair
    /// instructions (see [`retry_message`]).
    Invalid(Vec<String>),
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
pub(crate) fn extract_and_validate(
    text: &str,
    validator: &jsonschema::Validator,
    schema: &serde_json::Value,
) -> Validation {
    let Some(candidate) = extract_json(text) else {
        return Validation::Invalid(vec!["no JSON value found in the model output".to_owned()]);
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
        return Validation::Valid(candidate);
    }
    // SAP-lite rescue: one deterministic coercion pass (string-encoded
    // scalars · single-element array wrap · enum case-snap), accepted
    // ONLY when the FULL schema validates afterwards — each landed
    // rescue deletes an entire paid retry round-trip. A rescue that
    // falls short changes nothing: the retry proceeds on the ORIGINAL
    // errors (the model is shown its own text, not our repair).
    let repaired = crate::coerce::coerce_toward(&candidate, schema);
    if repaired != candidate && validator.iter_errors(&repaired).next().is_none() {
        return Validation::Valid(repaired);
    }
    Validation::Invalid(rendered)
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

/// The corrective user message appended on a validation retry — a NUMBERED
/// repair list plus the localized-edit framing (« fix exactly these · keep
/// everything else identical »), not a prose dump.
///
/// The shape is evidence-picked: machine-readable per-error repair
/// instructions beat raw validator prose by +37-40pp task completion in
/// the only direct eval (Self-Reflective APIs · arxiv.org/abs/2606.05037),
/// and localized critique beats generic feedback (arxiv.org/abs/2407.02397)
/// — a full regeneration invites NEW errors in the parts that were already
/// right. The schema still renders (capped): on the native path it
/// traveled server-side, so the retry is the model's only in-context look.
pub(crate) fn retry_message(errors: &[String], schema: &serde_json::Value) -> String {
    let rendered = render_schema(schema);
    let list = errors
        .iter()
        .enumerate()
        .map(|(i, e)| format!("{}. {e}", i + 1))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "The previous reply did not satisfy the required output schema.\n\
         Repair instructions — fix exactly these, keep everything else identical:\n\
         {list}\n\
         Reply again with ONLY the corrected JSON value, no prose, no code \
         fences. The schema:\n{rendered}"
    )
}

/// The most candidate spans `extract_json` will try before giving up —
/// bounds the balanced-span scan at O(CAP · n) on adversarial input (a
/// 1 MB tool result full of `{` must not become O(n²)). Real model output
/// carries the value near the start or in the first fence; 16 starts covers
/// prose-wrapped replies without opening a `DoS` surface.
const MAX_SPAN_CANDIDATES: usize = 16;

/// Pull the first plausible JSON value out of free-form model text.
///
/// Layered, most-specific first: the whole trimmed text, the first fenced
/// block, then EACH balanced `{…}`/`[…]` span in turn (not just the first —
/// leading prose containing a stray `{`/`[` used to hide the real value that
/// followed · review lens 4). Every candidate is tried STRICT then through a
/// conservative repair (trailing commas) — the near-miss JSON local/open-weight
/// models emit (`PromptPort` · arxiv.org/abs/2601.06151) costs a wasted paid
/// retry otherwise.
fn extract_json(text: &str) -> Option<serde_json::Value> {
    let trimmed = text.trim();
    if let Some(v) = parse_lenient(trimmed) {
        return Some(v);
    }
    if let Some(fenced) = first_fenced_block(trimmed)
        && let Some(v) = parse_lenient(fenced)
    {
        return Some(v);
    }
    balanced_spans(trimmed).into_iter().find_map(parse_lenient)
}

/// Parse a candidate STRICT, then — only if that fails — through one
/// conservative repair pass. The repair is deterministic and never runs on
/// already-valid JSON (a valid value returns at the strict branch), so the
/// strict contract is never weakened for well-formed providers.
fn parse_lenient(candidate: &str) -> Option<serde_json::Value> {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(candidate) {
        return Some(v);
    }
    let repaired = strip_trailing_commas(candidate);
    // Only reparse if the repair actually changed something — otherwise it is
    // the same strict failure, not a near-miss.
    if repaired.len() != candidate.len() {
        return serde_json::from_str::<serde_json::Value>(&repaired).ok();
    }
    None
}

/// Drop trailing commas (`,]` / `,}`) — the single most common near-miss
/// JSON weak local models emit (`PromptPort` canonicalization ·
/// arxiv.org/abs/2601.06151). String-literal-aware, so a comma inside a
/// value is never touched; applied ONLY after a strict parse already failed,
/// and its result is reparsed strict, so a genuinely malformed value still
/// fails. (Smart-quote / single-quote repair is deliberately NOT done —
/// those are ambiguous against string content and risk corrupting a value.)
fn strip_trailing_commas(candidate: &str) -> String {
    let bytes = candidate.as_bytes();
    // Copy bytes through unchanged and skip ONLY the ASCII `,` we drop — the
    // comma byte (0x2C) never appears inside a UTF-8 multibyte sequence, so
    // multibyte string content is preserved byte-for-byte and the result is
    // always valid UTF-8.
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut in_string = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate() {
        if in_string {
            out.push(b);
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        if b == b'"' {
            in_string = true;
            out.push(b);
        } else if b == b',' && next_nonspace_closes(bytes, i + 1) {
            // trailing comma before `]`/`}` — drop it
        } else {
            out.push(b);
        }
    }
    // SAFETY-EQUIVALENT: only whole ASCII bytes were removed from valid UTF-8.
    String::from_utf8(out).unwrap_or_else(|_| candidate.to_owned())
}

/// Is the next non-whitespace byte a closing `]` or `}`? (trailing-comma test)
/// ASCII-only scan is correct: JSON structural whitespace and the closers are
/// all ASCII, and multibyte UTF-8 continuation bytes are never `< 0x80`.
fn next_nonspace_closes(bytes: &[u8], from: usize) -> bool {
    bytes[from..]
        .iter()
        .find(|&&b| !b.is_ascii_whitespace())
        .is_some_and(|&b| b == b']' || b == b'}')
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

/// Every balanced `{…}`/`[…]` span, in source order, string-literal aware —
/// bounded to [`MAX_SPAN_CANDIDATES`] start positions. Trying each span (not
/// just the first) is what lets a real value survive leading prose that
/// carries a stray `{`/`[`: the caller parses candidates in order and takes
/// the first that succeeds.
fn balanced_spans(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut search_from = 0;
    while spans.len() < MAX_SPAN_CANDIDATES {
        let Some(rel) = text[search_from..].find(['{', '[']) else {
            break;
        };
        let open_idx = search_from + rel;
        if let Some(end) = balanced_end(bytes, open_idx) {
            spans.push(&text[open_idx..=end]);
        }
        // Advance past this opener (ASCII · a char boundary) to the next
        // candidate — a span that never closed is skipped, not abandoned.
        search_from = open_idx + 1;
    }
    spans
}

/// The index of the matching close for the delimiter at `open_idx`, or
/// `None` if the span never balances. String-literal aware: braces/brackets
/// inside a `"…"` value never move the depth.
fn balanced_end(bytes: &[u8], open_idx: usize) -> Option<usize> {
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
                    return Some(i);
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
        let v = extract_and_validate(
            r#"{"name":"Ada","age":36}"#,
            &validator(&person_schema()),
            &person_schema(),
        );
        match v {
            Validation::Valid(val) => assert_eq!(val["name"], "Ada"),
            Validation::Invalid(d) => panic!("expected valid, got: {d:?}"),
        }
    }

    #[test]
    fn fenced_json_is_extracted() {
        let text = "Here you go:\n```json\n{\"name\":\"Ada\",\"age\":36}\n```\nDone.";
        assert!(matches!(
            extract_and_validate(text, &validator(&person_schema()), &person_schema()),
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
            extract_and_validate(text, &validator(&person_schema()), &person_schema()),
            Validation::Valid(_)
        ));
    }

    #[test]
    fn balanced_span_ignores_braces_inside_strings() {
        let text = r#"noise {"name":"A{d}a","age":1} trail"#;
        match extract_and_validate(text, &validator(&person_schema()), &person_schema()) {
            Validation::Valid(val) => assert_eq!(val["name"], "A{d}a"),
            Validation::Invalid(d) => panic!("expected valid, got: {d:?}"),
        }
    }

    #[test]
    fn schema_violation_reports_path() {
        let v = extract_and_validate(
            r#"{"name":"Ada","age":-3}"#,
            &validator(&person_schema()),
            &person_schema(),
        );
        match v {
            Validation::Invalid(d) => {
                assert!(d.iter().any(|e| e.contains("age")), "path named: {d:?}");
            }
            Validation::Valid(_) => panic!("expected invalid"),
        }
    }

    #[test]
    fn no_json_at_all_is_invalid() {
        assert!(matches!(
            extract_and_validate(
                "plain prose, nothing else",
                &validator(&person_schema()),
                &person_schema(),
            ),
            Validation::Invalid(_)
        ));
    }

    #[test]
    fn arrays_are_supported() {
        let schema = json!({ "type": "array", "items": { "type": "integer" } });
        assert!(matches!(
            extract_and_validate("the list is [1, 2, 3] ok", &validator(&schema), &schema),
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
        let msg = retry_message(
            &["at `/age`: -3 is less than 0".to_owned()],
            &person_schema(),
        );
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
        // `[` opens an array span; a stray `}` must not close it. The first
        // `[` never balances (its only closer is `]`, reached inside a longer
        // unbalanced run) so it yields no parseable candidate — but the
        // extractor now advances to the NEXT candidate `[3, 4]` and returns
        // it (multi-candidate · review lens 4). A stray `}` still never
        // mis-pairs an array.
        let text = "data [1, 2} oops [3, 4] real";
        assert_eq!(extract_json(text), Some(serde_json::json!([3, 4])));
    }

    // ── near-miss repair (PromptPort · local-model robustness) ────────

    #[test]
    fn trailing_comma_in_object_is_repaired() {
        // The dominant local-model near-miss: a trailing comma before `}`.
        let v = extract_and_validate(
            r#"{"name":"Ada","age":36,}"#,
            &validator(&person_schema()),
            &person_schema(),
        );
        match v {
            Validation::Valid(val) => assert_eq!(val["age"], 36),
            Validation::Invalid(d) => panic!("expected repair to valid, got: {d:?}"),
        }
    }

    #[test]
    fn trailing_comma_in_array_and_nested_is_repaired() {
        let schema = json!({ "type": "array", "items": { "type": "integer" } });
        assert!(matches!(
            extract_and_validate("[1, 2, 3, ]", &validator(&schema), &schema),
            Validation::Valid(_)
        ));
        // Nested + whitespace before the closer.
        assert_eq!(
            extract_json("{\"a\":[1,2,],\n}"),
            Some(serde_json::json!({"a":[1,2]}))
        );
    }

    #[test]
    fn a_comma_inside_a_string_is_never_stripped() {
        // The repair is string-aware: a comma that is real content stays, and
        // a genuine trailing comma in the SAME value is still dropped.
        assert_eq!(
            extract_json(r#"{"s":"a, b","n":2,}"#),
            Some(serde_json::json!({"s":"a, b","n":2}))
        );
    }

    #[test]
    fn repair_preserves_multibyte_utf8() {
        // The byte-level comma strip must not corrupt multibyte string
        // content (café · 🦋) — it copies every non-dropped byte verbatim.
        assert_eq!(
            extract_json(r#"{"who":"café 🦋","k":1,}"#),
            Some(serde_json::json!({"who":"café 🦋","k":1}))
        );
    }

    #[test]
    fn a_valid_value_never_reaches_the_repair() {
        // Well-formed JSON with a legitimate `,]`-looking sequence INSIDE a
        // string parses strict on the first try — the repair branch (which
        // could only see it post-failure) is never consulted.
        assert_eq!(
            extract_json(r#"{"note":"ends with ,]"}"#),
            Some(serde_json::json!({"note":"ends with ,]"}))
        );
    }

    #[test]
    fn a_later_candidate_survives_leading_prose_brackets() {
        // Leading prose carrying a stray `[maybe]` (not JSON) must not hide
        // the real object that follows — the multi-candidate scan reaches it.
        let text = r#"I think [maybe] the answer is {"name":"Ada","age":36}."#;
        assert!(matches!(
            extract_and_validate(text, &validator(&person_schema()), &person_schema()),
            Validation::Valid(_)
        ));
    }

    #[test]
    fn a_genuinely_malformed_value_still_fails() {
        // The repair only strips trailing commas; a missing colon is still a
        // hard parse failure (no silent acceptance of structural garbage).
        assert!(extract_json(r#"{"a" 1, "b" 2}"#).is_none());
    }
}
