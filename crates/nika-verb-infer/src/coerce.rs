// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Schema-driven coercion — the repair ladder that runs BEFORE a paid
//! retry (SAP-lite · the schema-aligned-parsing pattern BAML productionized;
//! CRAFT per ADR-001, concept not code).
//!
//! When a candidate parses but misses the schema, one deterministic
//! coercion pass walks value and schema together and repairs the
//! near-misses weak models actually emit: string-encoded scalars
//! (`"42"` where an integer is declared), a scalar where a
//! single-element array is declared, a case/whitespace-drifted `enum`
//! variant, a number/bool where a string is declared. Every rule is
//! lossless or unambiguous, and the result is only USED if the FULL
//! schema validates afterwards (`structured::extract_and_validate`
//! re-runs the validator) — a coercion that does not reach full
//! validity changes nothing and the retry proceeds on the original
//! errors. Each landed coercion deletes an entire paid round-trip.
//!
//! Fences: composition and reference nodes (`$ref` · `anyOf` · `oneOf`
//! · `allOf`) are left untouched — no resolver, never a guess. Keys are
//! never renamed, members never added or dropped. Recursion mirrors the
//! candidate's own depth, which the parser already bounds.

use serde_json::Value;

/// One coercion pass over the whole candidate (pure). Returns the
/// (possibly unchanged) repaired value — the caller decides by
/// re-validating in full.
pub(crate) fn coerce_toward(value: &Value, schema: &Value) -> Value {
    coerce_node(value, schema)
}

/// Repair one node, then its children.
fn coerce_node(value: &Value, schema: &Value) -> Value {
    let Some(node) = schema.as_object() else {
        return value.clone();
    };
    if ["$ref", "anyOf", "oneOf", "allOf"]
        .iter()
        .any(|k| node.contains_key(*k))
    {
        return value.clone();
    }
    let types = declared_types(node);

    // A declared array around a non-array candidate: wrap ONE element —
    // but only when the candidate's own type is not itself declared
    // (types ["array","string"] must keep a bare string as-is).
    if types.iter().any(|t| t == "array")
        && !value.is_array()
        && !types.iter().any(|t| t == json_type_name(value))
        && let Some(items) = node.get("items").filter(|i| !i.is_array())
    {
        return Value::Array(vec![coerce_node(value, items)]);
    }

    match value {
        Value::Object(members) => {
            let props = node.get("properties").and_then(Value::as_object);
            let repaired = members
                .iter()
                .map(|(key, member)| {
                    let child = props.and_then(|p| p.get(key));
                    let value = child.map_or_else(|| member.clone(), |s| coerce_node(member, s));
                    (key.clone(), value)
                })
                .collect();
            Value::Object(repaired)
        }
        Value::Array(elements) => match node.get("items").filter(|i| !i.is_array()) {
            Some(items) => Value::Array(elements.iter().map(|e| coerce_node(e, items)).collect()),
            None => value.clone(),
        },
        scalar => coerce_scalar(scalar, node, &types),
    }
}

/// The scalar rungs: string→number/integer/bool · number/bool→string ·
/// enum case/whitespace snap. Each fires only when the value's own type
/// is NOT declared (a type that already fits is never rewritten).
fn coerce_scalar(value: &Value, node: &serde_json::Map<String, Value>, types: &[String]) -> Value {
    let own_type_declared = types.iter().any(|t| t == json_type_name(value));
    match value {
        Value::String(text) if !own_type_declared => {
            let trimmed = text.trim();
            if types.iter().any(|t| t == "integer")
                && let Ok(n) = trimmed.parse::<i64>()
            {
                return Value::Number(n.into());
            }
            if types.iter().any(|t| t == "number")
                && let Ok(f) = trimmed.parse::<f64>()
                && let Some(n) = serde_json::Number::from_f64(f)
            {
                return Value::Number(n);
            }
            if types.iter().any(|t| t == "boolean") {
                if trimmed.eq_ignore_ascii_case("true") {
                    return Value::Bool(true);
                }
                if trimmed.eq_ignore_ascii_case("false") {
                    return Value::Bool(false);
                }
            }
            value.clone()
        }
        Value::String(text) => enum_snap(text, node).unwrap_or_else(|| value.clone()),
        Value::Number(n) if !own_type_declared && types.iter().any(|t| t == "string") => {
            Value::String(n.to_string())
        }
        Value::Bool(b) if !own_type_declared && types.iter().any(|t| t == "string") => {
            Value::String(b.to_string())
        }
        _ => value.clone(),
    }
}

/// Snap a string onto the ONE `enum` variant it matches after trim +
/// case-fold. An exact member stays untouched; an ambiguous fold (two
/// variants differing only by case) never snaps — no guessing.
fn enum_snap(text: &str, node: &serde_json::Map<String, Value>) -> Option<Value> {
    let variants = node.get("enum")?.as_array()?;
    if variants.iter().any(|v| v.as_str() == Some(text)) {
        return None;
    }
    let folded = text.trim().to_lowercase();
    let mut hits = variants
        .iter()
        .filter_map(Value::as_str)
        .filter(|v| v.trim().to_lowercase() == folded);
    let first = hits.next()?;
    if hits.next().is_some() {
        return None;
    }
    Some(Value::String(first.to_owned()))
}

/// The `type:` names a node declares (scalar or union form).
fn declared_types(node: &serde_json::Map<String, Value>) -> Vec<String> {
    match node.get("type") {
        Some(Value::String(t)) => vec![t.clone()],
        Some(Value::Array(list)) => list
            .iter()
            .filter_map(|t| t.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

/// The candidate value's own JSON-Schema type name.
fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn string_encoded_scalars_coerce_when_the_type_disagrees() {
        let schema = json!({"type": "object", "properties": {
            "age": {"type": "integer"},
            "score": {"type": "number"},
            "ok": {"type": "boolean"},
            "label": {"type": "string"}
        }});
        let out = coerce_toward(
            &json!({"age": " 42 ", "score": "3.5", "ok": "TRUE", "label": "keep"}),
            &schema,
        );
        assert_eq!(
            out,
            json!({"age": 42, "score": 3.5, "ok": true, "label": "keep"})
        );
    }

    #[test]
    fn declared_string_never_rewrites_a_numeric_looking_string() {
        // "42" against type:string stays a string — a fitting type is
        // never touched, and a union declaring string keeps the string.
        let schema = json!({"type": "object", "properties": {
            "id": {"type": "string"},
            "either": {"type": ["string", "integer"]}
        }});
        let input = json!({"id": "42", "either": "7"});
        assert_eq!(coerce_toward(&input, &schema), input);
    }

    #[test]
    fn number_and_bool_stringify_when_a_string_is_declared() {
        let schema = json!({"type": "object", "properties": {
            "code": {"type": "string"},
            "flag": {"type": "string"}
        }});
        let out = coerce_toward(&json!({"code": 42, "flag": true}), &schema);
        assert_eq!(out, json!({"code": "42", "flag": "true"}));
    }

    #[test]
    fn scalar_wraps_into_the_declared_single_element_array() {
        let schema = json!({"type": "object", "properties": {
            "tags": {"type": "array", "items": {"type": "string"}},
            "ids": {"type": "array", "items": {"type": "integer"}}
        }});
        // the wrapped element itself runs the ladder ("7" → 7).
        let out = coerce_toward(&json!({"tags": "solo", "ids": "7"}), &schema);
        assert_eq!(out, json!({"tags": ["solo"], "ids": [7]}));
    }

    #[test]
    fn union_declaring_the_own_type_never_wraps() {
        let schema = json!({"type": "object", "properties": {
            "v": {"type": ["array", "string"], "items": {"type": "string"}}
        }});
        let input = json!({"v": "bare"});
        assert_eq!(coerce_toward(&input, &schema), input);
    }

    #[test]
    fn enum_snaps_on_unique_fold_never_on_ambiguity() {
        let schema = json!({"type": "object", "properties": {
            "field": {"type": "string", "enum": ["physics", "chemistry"]},
            "twin": {"type": "string", "enum": ["Ab", "aB"]}
        }});
        let out = coerce_toward(&json!({"field": " Physics ", "twin": "ab"}), &schema);
        assert_eq!(out["field"], "physics", "unique fold snaps");
        assert_eq!(out["twin"], "ab", "ambiguous fold never guesses");
    }

    #[test]
    fn nested_arrays_and_objects_recurse() {
        let schema = json!({"type": "object", "properties": {
            "rows": {"type": "array", "items": {
                "type": "object",
                "properties": {"n": {"type": "integer"}}
            }}
        }});
        let out = coerce_toward(&json!({"rows": [{"n": "1"}, {"n": "2"}]}), &schema);
        assert_eq!(out, json!({"rows": [{"n": 1}, {"n": 2}]}));
    }

    #[test]
    fn composition_and_ref_nodes_stay_untouched() {
        let schema = json!({"type": "object", "properties": {
            "a": {"anyOf": [{"type": "integer"}]},
            "b": {"$ref": "#/$defs/X"}
        }});
        let input = json!({"a": "1", "b": "2"});
        assert_eq!(
            coerce_toward(&input, &schema),
            input,
            "no resolver, no guess"
        );
    }

    #[test]
    fn unknown_members_and_unparseable_strings_pass_through() {
        let schema = json!({"type": "object", "properties": {
            "n": {"type": "integer"}
        }});
        let input = json!({"n": "not-a-number", "extra": "kept"});
        assert_eq!(coerce_toward(&input, &schema), input);
    }
}
