// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `OpenAI` strict-mode schema normalizer — the dialect rewrite
//! [`super::openai_compat`] applies before a schema rides
//! `response_format` with `strict: true` (and ONLY for `provider_id ==
//! "openai"` — compat peers receive the author's schema verbatim).
//!
//! Sibling of `gemini_schema.rs` (the same one-dialect-one-module
//! layout). The rewrite only ever RELAXES the wire schema — every
//! stripped or folded constraint stays enforced by the verb's LOCAL
//! validation + retry. The shape tests live with the wire (they pin the
//! whole `request_body` path, gating included).

use serde_json::Value;

/// Recursively rewrite a JSON Schema into `OpenAI` strict-mode shape.
///
/// On every `"type":"object"` node it (1) injects
/// `additionalProperties:false` and (2) sets `required` to ALL declared
/// property keys — a field the author left out of `required` is made
/// *nullable* (`"type":[T,"null"]`, `OpenAI`'s documented optional
/// workaround) rather than dropped, so optionality survives. On every node
/// it also rewrites `oneOf` → `anyOf` (`OpenAI` strict rejects `oneOf` with
/// HTTP 400 "'oneOf' is not permitted"; a well-formed `anyOf` is accepted
/// by both `OpenAI` and gemini). The walk descends into `properties.*`,
/// `items` (single + tuple/`prefixItems`), every `$defs`/`definitions`
/// entry, and the `anyOf`/`allOf`/`oneOf` branches.
pub(super) fn normalize_strict_schema(schema: &Value) -> Value {
    let mut out = schema.clone();
    normalize_node(&mut out);
    out
}

/// In-place strict-mode rewrite of one schema node and its descendants.
fn normalize_node(node: &mut Value) {
    let Some(obj) = node.as_object_mut() else {
        return;
    };
    descend_subschemas(obj);
    rename_oneof_to_anyof(obj);
    flatten_allof(obj);
    simplify_unsupported(obj);
    if obj.get("type").and_then(Value::as_str) == Some("object") {
        tighten_object(obj);
    }
}

/// Rewrite the keywords `OpenAI` strict does not accept (each 400s at
/// request time — "Unsupported keywords" / "not permitted" · live-verified
/// gpt-4o-mini 2026-07-08):
/// - `const: X` → `enum: [X]` (`X`'s JSON type is preserved verbatim;
///   a one-member `enum` is the equivalent constraint).
/// - `uniqueItems` → stripped (array-validation-only; not expressible in
///   the structured-output dialect).
/// - the negation/conditional/dependency family (`not` · `if`/`then`/
///   `else` · `dependentRequired`/`dependentSchemas`) → stripped.
///
/// Every stripped constraint stays enforced by the verb's LOCAL
/// validation + retry — the wire schema only relaxes, never tightens.
/// (`patternProperties` and `minLength`/`maxLength` are ACCEPTED by the
/// live API despite doc ambiguity — probed 2026-07-08 — and pass through.)
fn simplify_unsupported(obj: &mut serde_json::Map<String, Value>) {
    if let Some(c) = obj.remove("const") {
        obj.entry("enum").or_insert_with(|| Value::Array(vec![c]));
    }
    for kw in [
        "uniqueItems",
        "not",
        "if",
        "then",
        "else",
        "dependentRequired",
        "dependentSchemas",
    ] {
        obj.remove(kw);
    }
}

/// Fold `allOf` away — `OpenAI` strict 400s on it ("'allOf' is not
/// permitted" · live-verified 2026-07-08). The wire schema only needs to
/// RELAX the authored one (local validation enforces the full
/// conjunction), so the branches merge into the node: `properties` union
/// (first-wins per name — the node's own beat the branches') · `required`
/// union (a conjunction requires every branch's keys) · any other key
/// first-wins. Runs AFTER [`descend_subschemas`], so branches arrive
/// already normalized.
fn flatten_allof(obj: &mut serde_json::Map<String, Value>) {
    let Some(Value::Array(branches)) = obj.remove("allOf") else {
        return;
    };
    for branch in branches {
        let Value::Object(branch) = branch else {
            continue;
        };
        for (key, value) in branch {
            merge_allof_key(obj, key, value);
        }
    }
}

/// Merge one key of an `allOf` branch into the host node (see
/// [`flatten_allof`] for the union rules).
fn merge_allof_key(obj: &mut serde_json::Map<String, Value>, key: String, value: Value) {
    match obj.get_mut(&key) {
        None => {
            obj.insert(key, value);
        }
        Some(Value::Object(existing)) if key == "properties" => {
            if let Value::Object(more) = value {
                for (name, prop) in more {
                    existing.entry(name).or_insert(prop);
                }
            }
        }
        Some(Value::Array(existing)) if key == "required" => {
            if let Value::Array(more) = value {
                for req in more {
                    if !existing.contains(&req) {
                        existing.push(req);
                    }
                }
            }
        }
        Some(_) => {} // the node's own key wins — relaxation is safe
    }
}

/// `oneOf` → `anyOf` (`OpenAI` strict permits `anyOf`, not `oneOf`). Runs
/// after [`descend_subschemas`], so the branches are already normalized;
/// this only swaps the keyword. If the node already carries an `anyOf`, the
/// `oneOf` branches are appended to it (both forms must hold under
/// JSON-Schema, and `anyOf`'s "at least one" is the accepted superset of
/// `oneOf`'s "exactly one") — either way no `oneOf` survives.
fn rename_oneof_to_anyof(obj: &mut serde_json::Map<String, Value>) {
    let Some(Value::Array(one_of)) = obj.remove("oneOf") else {
        return;
    };
    match obj.get_mut("anyOf").and_then(Value::as_array_mut) {
        Some(any_of) => any_of.extend(one_of),
        None => {
            obj.insert("anyOf".to_owned(), Value::Array(one_of));
        }
    }
}

/// Recurse into every child that is itself a schema (the composition +
/// container keywords). `properties` optionality is handled by the parent
/// in [`tighten_object`]; here we only recurse to normalize nested objects.
fn descend_subschemas(obj: &mut serde_json::Map<String, Value>) {
    if let Some(props) = obj.get_mut("properties").and_then(Value::as_object_mut) {
        for prop in props.values_mut() {
            normalize_node(prop);
        }
    }
    // `items` is a single subschema (array element) or a tuple array;
    // `prefixItems` (2020-12) is always a tuple array.
    for key in ["items", "prefixItems", "additionalItems"] {
        match obj.get_mut(key) {
            Some(Value::Array(items)) => items.iter_mut().for_each(normalize_node),
            Some(child) => normalize_node(child),
            None => {}
        }
    }
    for key in ["$defs", "definitions"] {
        if let Some(defs) = obj.get_mut(key).and_then(Value::as_object_mut) {
            for def in defs.values_mut() {
                normalize_node(def);
            }
        }
    }
    for key in ["anyOf", "allOf", "oneOf"] {
        if let Some(branches) = obj.get_mut(key).and_then(Value::as_array_mut) {
            branches.iter_mut().for_each(normalize_node);
        }
    }
}

/// Apply the two object-node invariants: `additionalProperties:false` and
/// `required` = all property keys (optionals made nullable first).
fn tighten_object(obj: &mut serde_json::Map<String, Value>) {
    let all_keys: Vec<String> = obj
        .get("properties")
        .and_then(Value::as_object)
        .map(|p| p.keys().cloned().collect())
        .unwrap_or_default();

    // Keys the author already marked required keep their type as written;
    // the rest become nullable so they can stay in `required` (OpenAI's
    // optional shape) without forcing the model to invent a value.
    let already: std::collections::BTreeSet<String> = obj
        .get("required")
        .and_then(Value::as_array)
        .map(|r| {
            r.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if let Some(props) = obj.get_mut("properties").and_then(Value::as_object_mut) {
        for (name, prop) in props.iter_mut() {
            if !already.contains(name) {
                make_nullable(prop);
            }
        }
    }

    obj.insert("additionalProperties".to_owned(), Value::Bool(false));
    obj.insert(
        "required".to_owned(),
        Value::Array(all_keys.into_iter().map(Value::String).collect()),
    );
}

/// Widen a schema node's `type` to include `"null"` (idempotent). A scalar
/// `"type":"string"` becomes `["string","null"]`; an existing array gains
/// `"null"` only if absent; a typeless node (e.g. a `$ref`/`anyOf` form) is
/// left untouched — there is no `type` to widen.
fn make_nullable(prop: &mut Value) {
    let Some(obj) = prop.as_object_mut() else {
        return;
    };
    match obj.get_mut("type") {
        Some(Value::String(t)) => {
            let widened = vec![
                Value::String(std::mem::take(t)),
                Value::String("null".to_owned()),
            ];
            obj.insert("type".to_owned(), Value::Array(widened));
        }
        Some(Value::Array(types)) => {
            if !types.iter().any(|v| v.as_str() == Some("null")) {
                types.push(Value::String("null".to_owned()));
            }
        }
        _ => {}
    }
}
