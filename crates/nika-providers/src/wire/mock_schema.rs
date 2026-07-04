// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Schema-driven synthesis for the `mock` provider — the offline-CI story.
//!
//! When a request carries `response_format: JsonSchema`, the mock stops
//! echoing and SYNTHESIZES a minimal instance of the schema (F3 · field
//! report 2026-07-04): the echo can never satisfy a schema, so every
//! structured workflow on `mock/echo` burned its retry budget and died
//! NIKA-INFER-002 — there was no offline story for exactly the workflows
//! that need CI most. With synthesis, `nika run --model mock/echo`
//! dry-runs ANY structured workflow offline (zero key · zero network).
//!
//! The generator is TOTAL and deterministic (byte-stable output for the
//! same schema — the mock contract) but deliberately MINIMAL: it honors
//! the keywords workflow authors actually write (`type` · `enum` ·
//! `const` · `default` · `required`/`properties` · `items` · numeric and
//! length bounds · first-branch `oneOf`/`anyOf`/`allOf`) and degrades to
//! `null` elsewhere. A schema it cannot satisfy (e.g. `pattern` ·
//! unresolved `$ref`) simply fails the verb's validation — the honest
//! signal, never a panic.

use serde_json::{Map, Value};

/// Recursion ceiling — a pathological/deeply-nested schema degrades to
/// `null` leaves instead of unbounded descent (`$ref` is NOT resolved,
/// so a cyclic schema cannot trap the generator).
const MAX_DEPTH: u8 = 16;

/// Ceiling on generated array length — a hostile `minItems: 10_000_000`
/// must not buy an allocation (the instance then honestly fails the
/// verb's validation instead).
const MAX_ITEMS: usize = 64;

/// Synthesize a minimal, deterministic instance of `schema`.
pub(crate) fn synthesize(schema: &Value) -> Value {
    instance(schema, 0)
}

fn instance(schema: &Value, depth: u8) -> Value {
    if depth >= MAX_DEPTH {
        return Value::Null;
    }
    let Some(obj) = schema.as_object() else {
        // `true` (anything) · `false` (unsatisfiable) · non-object form —
        // null is the minimal candidate either way.
        return Value::Null;
    };
    // Authored fixed points win over type synthesis.
    if let Some(c) = obj.get("const") {
        return c.clone();
    }
    if let Some(first) = obj
        .get("enum")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
    {
        return first.clone();
    }
    if let Some(d) = obj.get("default") {
        return d.clone();
    }
    // Composition — the FIRST branch is the minimal instance (allOf with
    // genuinely conflicting branches then fails validation · honest).
    for key in ["oneOf", "anyOf", "allOf"] {
        if let Some(branch) = obj
            .get(key)
            .and_then(Value::as_array)
            .and_then(|a| a.first())
        {
            return instance(branch, depth + 1);
        }
    }
    match type_of(obj) {
        Some("string") => Value::String(string_within(obj)),
        Some("integer") => integer_within(obj),
        Some("number") => number_within(obj),
        Some("boolean") => Value::Bool(false),
        Some("array") => array_within(obj, depth),
        Some("object") => object_within(obj, depth),
        // "null" · unknown/future types · no type and no structural
        // hint → null (validates against an unconstrained schema).
        _ => Value::Null,
    }
}

/// The effective `type` — a bare string, the FIRST entry of a type
/// array (`["string","null"]`), or inferred from structural keywords
/// when absent (an author writing `properties:` means an object).
fn type_of(obj: &Map<String, Value>) -> Option<&str> {
    match obj.get("type") {
        Some(Value::String(t)) => Some(t.as_str()),
        Some(Value::Array(ts)) => ts.iter().find_map(Value::as_str),
        _ => {
            if obj.contains_key("properties") || obj.contains_key("required") {
                Some("object")
            } else if obj.contains_key("items") {
                Some("array")
            } else {
                None
            }
        }
    }
}

/// `"mock"`, padded up to `minLength` / truncated to `maxLength` (the
/// base is ASCII so byte truncation is char-safe). Contradictory bounds
/// (`maxLength < minLength`) are unsatisfiable — the instance then fails
/// validation, honestly.
fn string_within(obj: &Map<String, Value>) -> String {
    let mut s = String::from("mock");
    if let Some(min) = obj.get("minLength").and_then(Value::as_u64) {
        while (s.len() as u64) < min {
            s.push_str("mock");
        }
        s.truncate(usize::try_from(min).unwrap_or(s.len()).max(4));
    }
    if let Some(max) = obj.get("maxLength").and_then(Value::as_u64) {
        let max = usize::try_from(max).unwrap_or(usize::MAX);
        if s.len() > max {
            s.truncate(max);
        }
    }
    s
}

/// `minimum` ?? `exclusiveMinimum`+1 ?? (`maximum` when negative) ?? 0.
fn integer_within(obj: &Map<String, Value>) -> Value {
    if let Some(min) = int_bound(obj.get("minimum")) {
        return Value::from(min);
    }
    if let Some(xmin) = int_bound(obj.get("exclusiveMinimum")) {
        return Value::from(xmin.saturating_add(1));
    }
    if let Some(max) = int_bound(obj.get("maximum"))
        && max < 0
    {
        return Value::from(max);
    }
    Value::from(0)
}

/// An integer bound that may be authored as a float (`minimum: 1.0` on
/// an integer type is legal JSON Schema) — ceil so the bound holds.
fn int_bound(v: Option<&Value>) -> Option<i64> {
    let v = v?;
    #[allow(clippy::cast_possible_truncation)] // ceil'd bound · schema-authored magnitudes
    v.as_i64().or_else(|| v.as_f64().map(|f| f.ceil() as i64))
}

/// `minimum` ?? `exclusiveMinimum`+1 ?? (`maximum` when negative) ?? 0.0.
fn number_within(obj: &Map<String, Value>) -> Value {
    let bound = |key: &str| obj.get(key).and_then(Value::as_f64);
    let candidate = bound("minimum")
        .or_else(|| bound("exclusiveMinimum").map(|x| x + 1.0))
        .or_else(|| bound("maximum").filter(|m| *m < 0.0))
        .unwrap_or(0.0);
    serde_json::Number::from_f64(candidate).map_or(Value::Null, Value::Number)
}

/// One representative item (per the F3 contract) unless `minItems`
/// demands more, capped at [`MAX_ITEMS`] and clamped to `maxItems`.
fn array_within(obj: &Map<String, Value>, depth: u8) -> Value {
    let min = obj.get("minItems").and_then(Value::as_u64).unwrap_or(0);
    let max = obj
        .get("maxItems")
        .and_then(Value::as_u64)
        .unwrap_or(u64::MAX);
    let n = usize::try_from(min.max(1).min(max))
        .unwrap_or(1)
        .min(MAX_ITEMS);
    let item = obj
        .get("items")
        .map_or(Value::Null, |items| instance(items, depth + 1));
    Value::Array(std::iter::repeat_with(|| item.clone()).take(n).collect())
}

/// Exactly the `required` keys, each synthesized from its `properties`
/// schema (a required key WITHOUT a property schema is unconstrained →
/// null). Optional properties are omitted — minimal instance.
fn object_within(obj: &Map<String, Value>, depth: u8) -> Value {
    let empty = Map::new();
    let props = obj
        .get("properties")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let mut out = Map::new();
    if let Some(required) = obj.get("required").and_then(Value::as_array) {
        for key in required.iter().filter_map(Value::as_str) {
            let value = props
                .get(key)
                .map_or(Value::Null, |s| instance(s, depth + 1));
            out.insert(key.to_owned(), value);
        }
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The instance must VALIDATE against its own schema — the whole
    /// point of synthesis (asserted with the same `jsonschema` crate the
    /// verb's validation floor uses).
    fn assert_conformant(schema: &Value) -> Value {
        let v = synthesize(schema);
        let validator = jsonschema::validator_for(schema).expect("test schema compiles");
        let errors: Vec<String> = validator.iter_errors(&v).map(|e| e.to_string()).collect();
        assert!(
            errors.is_empty(),
            "synthesized instance must validate:\n  schema: {schema}\n  instance: {v}\n  errors: {errors:?}"
        );
        v
    }

    #[test]
    fn atlas_style_review_schema_validates() {
        // The exact class the field report hit (payload-review): enum
        // severity + bounded integer + required arrays of typed objects.
        let schema = json!({
            "type": "object",
            "required": ["verdict", "score", "findings"],
            "properties": {
                "verdict": { "type": "string", "enum": ["P0", "P1", "P2", "P3"] },
                "score": { "type": "integer", "minimum": 0, "maximum": 12 },
                "findings": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["severity", "detail"],
                        "properties": {
                            "severity": { "type": "string", "enum": ["P0", "P1"] },
                            "detail": { "type": "string" }
                        }
                    }
                }
            }
        });
        let v = assert_conformant(&schema);
        assert_eq!(v["verdict"], "P0", "enum honors its first entry");
        assert_eq!(v["score"], 0, "bounded integer sits on its minimum");
        assert_eq!(v["findings"][0]["severity"], "P0");
        assert_eq!(v["findings"][0]["detail"], "mock");
    }

    #[test]
    fn scalar_defaults_per_type() {
        assert_eq!(synthesize(&json!({ "type": "string" })), json!("mock"));
        assert_eq!(synthesize(&json!({ "type": "integer" })), json!(0));
        assert_eq!(synthesize(&json!({ "type": "number" })), json!(0.0));
        assert_eq!(synthesize(&json!({ "type": "boolean" })), json!(false));
        assert_eq!(synthesize(&json!({ "type": "null" })), Value::Null);
    }

    #[test]
    fn numeric_bounds_are_honored() {
        assert_conformant(&json!({ "type": "integer", "minimum": 5 }));
        assert_eq!(
            synthesize(&json!({ "type": "integer", "minimum": 5 })),
            json!(5)
        );
        // float-authored bound on an integer type → ceil'd
        assert_eq!(
            synthesize(&json!({ "type": "integer", "minimum": 4.2 })),
            json!(5)
        );
        assert_eq!(
            synthesize(&json!({ "type": "integer", "exclusiveMinimum": 3 })),
            json!(4)
        );
        // negative-only range: 0 would violate → the maximum is the instance
        assert_conformant(&json!({ "type": "integer", "maximum": -3 }));
        assert_eq!(
            synthesize(&json!({ "type": "integer", "maximum": -3 })),
            json!(-3)
        );
        assert_conformant(&json!({ "type": "number", "minimum": 1.5 }));
    }

    #[test]
    fn string_length_bounds_are_honored() {
        let long = assert_conformant(&json!({ "type": "string", "minLength": 10 }));
        assert!(long.as_str().expect("string").len() >= 10);
        let short = assert_conformant(&json!({ "type": "string", "maxLength": 2 }));
        assert_eq!(short, json!("mo"));
    }

    #[test]
    fn fixed_points_win_over_type_synthesis() {
        assert_eq!(
            synthesize(&json!({ "type": "integer", "const": 42 })),
            json!(42)
        );
        assert_eq!(
            synthesize(&json!({ "type": "string", "default": "hi" })),
            json!("hi")
        );
        // enum of any type — first entry verbatim
        assert_eq!(
            synthesize(&json!({ "enum": [{ "k": 1 }, "x"] })),
            json!({ "k": 1 })
        );
    }

    #[test]
    fn type_arrays_and_inferred_shapes() {
        assert_eq!(
            synthesize(&json!({ "type": ["string", "null"] })),
            json!("mock")
        );
        // no `type` but `properties`/`required` → object inferred
        let v = assert_conformant(&json!({
            "required": ["name"],
            "properties": { "name": { "type": "string" } }
        }));
        assert_eq!(v, json!({ "name": "mock" }));
        // no `type` but `items` → array inferred
        assert_eq!(
            synthesize(&json!({ "items": { "type": "integer" } })),
            json!([0])
        );
    }

    #[test]
    fn composition_takes_the_first_branch() {
        assert_eq!(
            synthesize(&json!({ "oneOf": [{ "type": "integer" }, { "type": "string" }] })),
            json!(0)
        );
        assert_eq!(
            synthesize(&json!({ "anyOf": [{ "const": "a" }, { "const": "b" }] })),
            json!("a")
        );
    }

    #[test]
    fn min_items_replicates_within_the_ceiling() {
        let v = assert_conformant(&json!({
            "type": "array", "minItems": 3, "items": { "type": "string" }
        }));
        assert_eq!(v, json!(["mock", "mock", "mock"]));
        // maxItems: 0 → the empty array (min.max(1) clamped down)
        assert_eq!(
            synthesize(&json!({ "type": "array", "maxItems": 0 })),
            json!([])
        );
        // hostile minItems is capped — allocation-bounded, honestly invalid
        let huge = synthesize(&json!({ "type": "array", "minItems": 1_000_000 }));
        assert_eq!(huge.as_array().expect("array").len(), MAX_ITEMS);
    }

    #[test]
    fn deterministic_byte_stable() {
        let schema = json!({
            "type": "object",
            "required": ["b", "a"],
            "properties": { "a": { "type": "integer" }, "b": { "type": "string" } }
        });
        assert_eq!(
            synthesize(&schema).to_string(),
            synthesize(&schema).to_string(),
            "the mock contract: byte-stable output for identical input"
        );
    }

    #[test]
    fn total_on_degenerate_schemas() {
        // Non-object forms · unresolved $ref · unknown types — total, null.
        assert_eq!(synthesize(&json!(true)), Value::Null);
        assert_eq!(synthesize(&json!(false)), Value::Null);
        assert_eq!(synthesize(&json!({ "$ref": "#/defs/x" })), Value::Null);
        assert_eq!(synthesize(&json!({ "type": "wormhole" })), Value::Null);
        // deep nesting degrades to null leaves instead of unbounded descent
        let mut deep = json!({ "type": "string" });
        for _ in 0..40 {
            deep = json!({ "type": "object", "required": ["k"], "properties": { "k": deep } });
        }
        let _ = synthesize(&deep); // must terminate
    }
}
