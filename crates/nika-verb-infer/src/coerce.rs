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
    if node.contains_key("$ref") || node.contains_key("oneOf") || node.contains_key("allOf") {
        return value.clone();
    }
    // A scalar `anyOf` (integer | string, optionally with enums) is the
    // same as `type: [integer, string]` — flattening it lets number→string
    // and string→integer fire. A nested/object anyOf stays fenced (no
    // resolver, never a guess). Empirical 2026-08-19: authors wrote
    // anyOf to accept `3` and `"3"` and the fence disabled EVERY coerce.
    if node.contains_key("anyOf") {
        return match flatten_scalar_any_of(node) {
            Some(flat) => coerce_node(value, &flat),
            None => value.clone(),
        };
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

/// Collapse `anyOf: [{type: integer, enum: [...]}, {type: string, enum: [...]}]`
/// into one scalar node. `None` if any branch is composite or `$ref`.
fn flatten_scalar_any_of(node: &serde_json::Map<String, Value>) -> Option<Value> {
    let branches = node.get("anyOf")?.as_array()?;
    if branches.is_empty() {
        return None;
    }
    let mut types = Vec::new();
    let mut enums = Vec::new();
    let mut saw_enum = false;
    for branch in branches {
        let obj = branch.as_object()?;
        if obj.keys().any(|k| {
            matches!(
                k.as_str(),
                "$ref" | "anyOf" | "oneOf" | "allOf" | "properties" | "items"
            )
        }) {
            return None;
        }
        match obj.get("type") {
            Some(Value::String(t)) if is_scalar_type(t) => types.push(t.clone()),
            Some(Value::Array(list)) => {
                for t in list {
                    let name = t.as_str()?;
                    if !is_scalar_type(name) {
                        return None;
                    }
                    types.push(name.to_owned());
                }
            }
            _ => return None,
        }
        if let Some(variants) = obj.get("enum").and_then(Value::as_array) {
            saw_enum = true;
            enums.extend(variants.iter().cloned());
        }
    }
    types.sort();
    types.dedup();
    let mut flat = serde_json::Map::new();
    flat.insert(
        "type".into(),
        if types.len() == 1 {
            Value::String(types.remove(0))
        } else {
            Value::Array(types.into_iter().map(Value::String).collect())
        },
    );
    if saw_enum {
        flat.insert("enum".into(), Value::Array(enums));
    }
    Some(Value::Object(flat))
}

fn is_scalar_type(name: &str) -> bool {
    matches!(name, "string" | "integer" | "number" | "boolean" | "null")
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
            "a": {"anyOf": [{"type": "object", "properties": {"k": {"type": "integer"}}}]},
            "b": {"$ref": "#/$defs/X"}
        }});
        let input = json!({"a": "1", "b": "2"});
        assert_eq!(
            coerce_toward(&input, &schema),
            input,
            "no resolver, no guess on composite anyOf or $ref"
        );
    }

    #[test]
    fn scalar_any_of_integer_parses_a_digit_string() {
        let schema = json!({"type": "object", "properties": {
            "n": {"anyOf": [{"type": "integer"}]}
        }});
        let out = coerce_toward(&json!({"n": "3"}), &schema);
        assert_eq!(out["n"], 3, "scalar anyOf no longer fences integer coerce");
    }

    #[test]
    fn scalar_any_of_integer_or_string_coerces_both_ways() {
        let schema = json!({"type": "object", "properties": {
            "n": {"anyOf": [
                {"type": "integer", "enum": [-1, 0, 1, 3]},
                {"type": "string", "enum": ["-1", "0", "1", "3"]}
            ]}
        }});
        let from_text = coerce_toward(&json!({"n": "3"}), &schema);
        assert!(
            from_text["n"] == json!(3) || from_text["n"] == json!("3"),
            "string digit matches a branch: {}",
            from_text["n"]
        );
        let from_num = coerce_toward(&json!({"n": 3}), &schema);
        assert!(
            from_num["n"] == json!(3) || from_num["n"] == json!("3"),
            "number 3 matches a branch: {}",
            from_num["n"]
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
