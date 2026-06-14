// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Gemini `responseSchema` adapter — the JSON-Schema → OpenAPI-3.0-subset
//! rewrite that the [`super::gemini`] wire applies to a structured-output
//! schema before sending. Split out of the wire module (file-size budget);
//! `gemini.rs` keeps the request/response/streaming machinery and calls
//! [`adapt_gemini_schema`] from `generation_config`.
//!
//! This is the **complement of the `openai` strict normalizer** in
//! [`super::openai_compat`] — each wire owns its dialect rewrite and they
//! never share a path (the transforms are gated per-provider). The repairs
//! here close the cross-provider parity gaps where gemini 400s a schema
//! openai accepts: `$ref`/`$defs` (inlined), `additionalProperties`
//! (stripped), multi-type `type` arrays (→ scalar `+nullable` or `anyOf`),
//! non-string enums (members stringified — gemini's `enum` is a repeated
//! STRING regardless of the node's `type`), `const` (→ one-member `enum`),
//! and `uniqueItems` (stripped).

use nika_kernel::ai::provider::ProviderError;
use serde_json::{Value, json};

/// Rewrite an author JSON Schema into Gemini's `responseSchema` shape (an
/// OpenAPI-3.0 subset). Keywords that 400 the request are repaired (the
/// complement of the `openai` strict normalizer):
///
/// - `$ref`/`$defs` — "Unknown name $defs/$ref" (Gemini's `responseSchema`
///   has no ref machinery) → every `$ref` is **inlined** (resolved against
///   the document root by JSON Pointer) and the `$defs`/`definitions`
///   blocks dropped. A cyclic `$ref` (a recursive type) cannot be
///   represented in the flat proto and returns a clean typed error rather
///   than looping forever.
/// - `additionalProperties` — "Unknown name additionalProperties" →
///   **stripped** (Gemini has no such field).
/// - `"type":[T,"null"]` — the JSON-Schema nullable form → "Proto field is
///   not repeating" → `"type":T` + `"nullable":true` (the OpenAPI-3.0
///   spelling).
/// - non-string `enum` members — `"enum[i] (TYPE_STRING), 200"` → every
///   member is rewritten to its STRING form (`200` → `"200"`), since
///   gemini's `enum` is a repeated STRING. The node's `type` is left as
///   authored (an `integer` enum keeps `type:integer` · gemini coerces the
///   chosen member back to that type in the response).
///
/// `format` · `required` · `properties` · `items` are kept as-is. The walk
/// descends into `properties.*` and `items` (single + tuple).
pub(super) fn adapt_gemini_schema(schema: &Value) -> Result<Value, ProviderError> {
    // Phase 1 · resolve every `$ref` against the document root, producing a
    // ref-free tree (or a typed error on a cycle). The root is the author's
    // schema as written — `$defs`/`definitions` still present for lookup.
    let root = schema.clone();
    let mut out = schema.clone();
    inline_refs(&mut out, &root, &mut Vec::new())?;
    // Drop the now-inlined definition blocks (Gemini rejects them).
    if let Some(obj) = out.as_object_mut() {
        obj.remove("$defs");
        obj.remove("definitions");
    }
    // Phase 2 · the per-node OpenAPI-subset rewrite (ref-free tree).
    adapt_node(&mut out);
    Ok(out)
}

/// In-place: replace every `$ref` node with a deep copy of its target,
/// resolved against `root` by JSON Pointer. `path` is the stack of ref
/// pointers currently being expanded — re-entering one is a cycle.
///
/// A `$ref` node is `{"$ref": "#/..."}`; per JSON-Schema it carries no
/// sibling keywords, so the whole node is replaced by the resolved target
/// (which is then recursively de-ref'd, so chains and nested refs resolve).
fn inline_refs(
    node: &mut Value,
    root: &Value,
    path: &mut Vec<String>,
) -> Result<(), ProviderError> {
    if let Some(pointer) = node.as_object().and_then(ref_pointer) {
        if path.iter().any(|p| p == &pointer) {
            return Err(cyclic_ref(&pointer));
        }
        let mut target = resolve_pointer(root, &pointer)?;
        path.push(pointer);
        inline_refs(&mut target, root, path)?;
        path.pop();
        *node = target;
        return Ok(());
    }
    match node {
        Value::Object(obj) => {
            for child in obj.values_mut() {
                inline_refs(child, root, path)?;
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                inline_refs(item, root, path)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// The `$ref` string if this node is a JSON-Schema reference (a local
/// fragment pointer `#/...`). Remote refs are not resolvable here.
fn ref_pointer(obj: &serde_json::Map<String, Value>) -> Option<String> {
    obj.get("$ref")
        .and_then(Value::as_str)
        .filter(|r| r.starts_with('#'))
        .map(ToOwned::to_owned)
}

/// Resolve a `#/...` fragment against the document root, returning a deep
/// copy of the target. An unresolvable ref (typo / external) is a typed
/// error — Gemini would 400 on the leftover `$ref` anyway.
fn resolve_pointer(root: &Value, reference: &str) -> Result<Value, ProviderError> {
    let pointer = reference.trim_start_matches('#');
    root.pointer(pointer)
        .cloned()
        .ok_or_else(|| ProviderError::Other {
            reason: format!("gemini schema adapter: unresolvable $ref '{reference}'"),
        })
}

/// A recursive type cannot be flattened into Gemini's proto schema.
fn cyclic_ref(pointer: &str) -> ProviderError {
    ProviderError::Other {
        reason: format!(
            "gemini schema adapter: cyclic $ref '{pointer}' — a recursive \
             type cannot be expressed in gemini's responseSchema (flat \
             proto); use a non-recursive schema for this provider"
        ),
    }
}

/// In-place Gemini rewrite of one schema node and its descendants. The tree
/// is already ref-free (see [`inline_refs`]), so there is no `$defs` to
/// descend into.
fn adapt_node(node: &mut Value) {
    let Some(obj) = node.as_object_mut() else {
        return;
    };
    obj.remove("additionalProperties");
    simplify_unsupported(obj);
    normalize_type_union(obj);
    stringify_enum_members(obj);
    if let Some(props) = obj.get_mut("properties").and_then(Value::as_object_mut) {
        for prop in props.values_mut() {
            adapt_node(prop);
        }
    }
    for key in ["items", "prefixItems", "additionalItems"] {
        match obj.get_mut(key) {
            Some(Value::Array(items)) => items.iter_mut().for_each(adapt_node),
            Some(child) => adapt_node(child),
            None => {}
        }
    }
}

/// Rewrite two keywords Gemini's `responseSchema` does not accept (both
/// also rejected by openai strict, repaired symmetrically there):
/// - `const: X` → `enum: [X]` (Gemini has no `const`; a one-member `enum`
///   is the equivalent constraint). The member is then stringified by
///   [`stringify_enum_members`] downstream (an integer const lands
///   `enum:["N"]` — gemini's `enum` is a repeated STRING).
/// - `uniqueItems` → stripped (array-validation-only; not expressible in
///   the structured-output dialect).
fn simplify_unsupported(obj: &mut serde_json::Map<String, Value>) {
    if let Some(c) = obj.remove("const") {
        obj.entry("enum").or_insert_with(|| Value::Array(vec![c]));
    }
    obj.remove("uniqueItems");
}

/// Rewrite every `enum` member to its STRING form, leaving the node's
/// `type` exactly as authored.
///
/// Gemini's `responseSchema.enum` is a **repeated STRING** (proto
/// `TYPE_STRING`) — the members must be JSON strings *regardless* of the
/// node's declared `type`. So an `integer` enum 400s
/// "enum\[0\] (`TYPE_STRING`), 200" until each member is stringified
/// (`200` → `"200"`, `true` → `"true"`, `1.5` → `"1.5"`). The `type` stays
/// as authored — gemini reads the constraint against the string members
/// and coerces the chosen value back to that `type` in the response (an
/// `integer` enum still yields a numeric field). A string enum is a no-op
/// (its members are already strings). Verified against real
/// `gemini-2.5-flash`: `{type:integer, enum:["200","404","500"]}` →
/// `{"code":404}`.
fn stringify_enum_members(obj: &mut serde_json::Map<String, Value>) {
    let Some(members) = obj.get_mut("enum").and_then(Value::as_array_mut) else {
        return;
    };
    for m in members.iter_mut() {
        if let Some(s) = scalar_to_string(m) {
            *m = Value::String(s);
        }
    }
}

/// The wire-string form of a scalar enum member: a string stays its own
/// content (not the quoted JSON), numbers and booleans render as their JSON
/// literal. `None` for null / array / object — those are not scalar enum
/// members gemini can represent, so they are left untouched (the caller's
/// risk, never silently mangled).
fn scalar_to_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

/// Rewrite a `type` ARRAY (a JSON-Schema multi-type union) into Gemini's
/// scalar-`type` proto, which rejects a repeating `type` ("Proto field is
/// not repeating"). The `"null"` member maps to the `OpenAPI` `nullable`
/// flag; the remaining types decide the shape:
///
/// - `[T]` / `[T,"null"]` → `"type":T` (+ `"nullable":true` if null) —
///   one real type stays a scalar (the common nullable case).
/// - `[T1,T2,…]` (+ optional `"null"`) → `"anyOf":[{"type":T1},…]`
///   (+ `"nullable":true` if null), and the repeating `type` is removed —
///   Gemini accepts `anyOf` of scalar-typed branches.
///
/// A plain scalar `type` (not an array) is left untouched. Sibling
/// keywords on the node are preserved (an `anyOf` alongside `enum`/
/// `description` is legal and both constraints hold).
fn normalize_type_union(obj: &mut serde_json::Map<String, Value>) {
    let Some(types) = obj.get("type").and_then(Value::as_array) else {
        return;
    };
    let has_null = types.iter().any(|v| v.as_str() == Some("null"));
    let non_null: Vec<String> = types
        .iter()
        .filter_map(Value::as_str)
        .filter(|t| *t != "null")
        .map(ToOwned::to_owned)
        .collect();

    match non_null.as_slice() {
        // Nothing actionable (empty, or a single scalar already): only the
        // null→nullable rewrite applies, and only when paired with a type.
        [] => {}
        [scalar] => {
            obj.insert("type".to_owned(), Value::String(scalar.clone()));
            if has_null {
                obj.insert("nullable".to_owned(), Value::Bool(true));
            }
        }
        many => {
            let branches: Vec<Value> = many.iter().map(|t| json!({ "type": t })).collect();
            obj.remove("type");
            obj.insert("anyOf".to_owned(), Value::Array(branches));
            if has_null {
                obj.insert("nullable".to_owned(), Value::Bool(true));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── BUG#6 · gemini structured-output schema adapter (Gate 2 parity) ──

    /// Adapt a schema, asserting it does not error (the common case — the
    /// fallible path is exercised by the cyclic-`$ref` test below).
    fn adapt(input: &Value) -> Value {
        adapt_gemini_schema(input).expect("adaptable schema")
    }

    #[test]
    fn adapter_strips_additional_properties() {
        let out = adapt(&json!({
            "type": "object",
            "additionalProperties": false,
            "properties": { "name": {"type": "string"} }
        }));
        assert!(
            out.get("additionalProperties").is_none(),
            "additionalProperties stripped at the root"
        );
        // a kept keyword survives
        assert_eq!(out["properties"]["name"]["type"], "string");
    }

    #[test]
    fn adapter_converts_nullable_type_union() {
        let out = adapt(&json!({
            "type": "object",
            "properties": { "note": {"type": ["string", "null"]} }
        }));
        let note = &out["properties"]["note"];
        assert_eq!(note["type"], "string", "scalar type extracted");
        assert_eq!(note["nullable"], true, "nullable flag set");
        // null-first order works too
        let out2 = adapt(&json!({"type": ["null", "integer"]}));
        assert_eq!(out2["type"], "integer");
        assert_eq!(out2["nullable"], true);
    }

    #[test]
    fn adapter_keeps_enum_and_scalar_type() {
        let out = adapt(&json!({
            "type": "string",
            "enum": ["a", "b", "c"]
        }));
        assert_eq!(out["enum"], json!(["a", "b", "c"]), "enum preserved");
        assert_eq!(out["type"], "string", "plain scalar type untouched");
        assert!(out.get("nullable").is_none(), "no spurious nullable");
    }

    #[test]
    fn adapter_recurses_into_nested_objects_and_arrays() {
        let out = adapt(&json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "addr": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": { "zip": {"type": ["string", "null"]} }
                },
                "tags": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": { "v": {"type": ["integer", "null"]} }
                    }
                }
            },
            "required": ["addr"]
        }));
        // nested object: additionalProperties stripped, nullable converted
        let addr = &out["properties"]["addr"];
        assert!(addr.get("additionalProperties").is_none());
        assert_eq!(addr["properties"]["zip"]["type"], "string");
        assert_eq!(addr["properties"]["zip"]["nullable"], true);
        // array element: same repairs reach inside `items`
        let elem = &out["properties"]["tags"]["items"];
        assert!(elem.get("additionalProperties").is_none());
        assert_eq!(elem["properties"]["v"]["type"], "integer");
        assert_eq!(elem["properties"]["v"]["nullable"], true);
        // a kept keyword is intact
        assert_eq!(out["required"], json!(["addr"]));
    }

    // ── BUG#8 · $ref/$defs inlined for gemini (Gate 2 parity) ──

    #[test]
    fn adapter_inlines_top_level_defs_and_drops_the_block() {
        // openai accepts $defs/$ref; gemini 400s "Unknown name $defs/$ref".
        // The refs must be inlined into the tree and the $defs block dropped.
        let out = adapt(&json!({
            "type": "object",
            "required": ["person", "location"],
            "properties": {
                "person": {"$ref": "#/$defs/Person"},
                "location": {"$ref": "#/$defs/Location"}
            },
            "$defs": {
                "Person": {
                    "type": "object",
                    "required": ["name", "age"],
                    "properties": {
                        "name": {"type": "string"},
                        "age": {"type": "integer"}
                    }
                },
                "Location": {
                    "type": "object",
                    "required": ["city"],
                    "properties": { "city": {"type": "string"} }
                }
            }
        }));
        assert!(out.get("$defs").is_none(), "$defs block dropped");
        // the ref nodes are replaced by their resolved targets in place.
        let person = &out["properties"]["person"];
        assert!(person.get("$ref").is_none(), "no $ref survives: {person}");
        assert_eq!(person["type"], "object");
        assert_eq!(person["properties"]["name"]["type"], "string");
        assert_eq!(person["properties"]["age"]["type"], "integer");
        assert_eq!(person["required"], json!(["name", "age"]));
        assert_eq!(
            out["properties"]["location"]["properties"]["city"]["type"],
            "string"
        );
    }

    #[test]
    fn adapter_inlines_nested_refs_through_definitions() {
        // A def that itself $refs another def (a chain), via the older
        // `definitions` keyword. Both hops must resolve; no $ref survives.
        let out = adapt(&json!({
            "type": "object",
            "properties": { "root": {"$ref": "#/definitions/Outer"} },
            "definitions": {
                "Outer": {
                    "type": "object",
                    "properties": { "inner": {"$ref": "#/definitions/Inner"} }
                },
                "Inner": {
                    "type": "object",
                    "properties": { "leaf": {"type": "string"} }
                }
            }
        }));
        assert!(
            out.get("definitions").is_none(),
            "definitions block dropped"
        );
        let leaf = &out["properties"]["root"]["properties"]["inner"]["properties"]["leaf"];
        assert_eq!(leaf["type"], "string", "nested ref chain fully inlined");
    }

    #[test]
    fn adapter_errors_on_a_cyclic_ref_instead_of_looping() {
        // A recursive type ($ref pointing back up its own chain) cannot be
        // flattened into gemini's proto schema → a clean typed error, not a
        // stack overflow.
        let cyclic = json!({
            "type": "object",
            "properties": { "node": {"$ref": "#/$defs/Node"} },
            "$defs": {
                "Node": {
                    "type": "object",
                    "properties": {
                        "value": {"type": "string"},
                        "next": {"$ref": "#/$defs/Node"}
                    }
                }
            }
        });
        let err = adapt_gemini_schema(&cyclic).expect_err("cyclic must error");
        match err {
            ProviderError::Other { reason } => {
                assert!(
                    reason.contains("cyclic"),
                    "reason names the cycle: {reason}"
                );
            }
            other => panic!("expected Other for a cyclic ref, got {other:?}"),
        }
    }

    #[test]
    fn adapter_errors_on_an_unresolvable_ref() {
        // A ref to a missing def is a typed error (gemini would 400 on the
        // leftover $ref anyway) — fail loud, not silently send garbage.
        let dangling = json!({
            "type": "object",
            "properties": { "x": {"$ref": "#/$defs/DoesNotExist"} }
        });
        let err = adapt_gemini_schema(&dangling).expect_err("dangling ref errors");
        assert!(
            matches!(err, ProviderError::Other { .. }),
            "unresolvable ref → Other: {err:?}"
        );
    }

    #[test]
    fn adapter_inlines_then_repairs_the_resolved_target() {
        // The inlined target still goes through the OpenAPI-subset repairs:
        // a def carrying additionalProperties + a [T,null] union is fixed
        // after inlining (phase 1 deref → phase 2 adapt).
        let out = adapt(&json!({
            "type": "object",
            "properties": { "node": {"$ref": "#/$defs/Node"} },
            "$defs": {
                "Node": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": { "label": {"type": ["string", "null"]} }
                }
            }
        }));
        let node = &out["properties"]["node"];
        assert!(node.get("$ref").is_none());
        assert!(
            node.get("additionalProperties").is_none(),
            "inlined target's additionalProperties stripped"
        );
        assert_eq!(node["properties"]["label"]["type"], "string");
        assert_eq!(node["properties"]["label"]["nullable"], true);
    }

    #[test]
    fn adapter_handles_an_openai_strict_output_shape() {
        // The exact shape BUG#4 produces (additionalProperties:false on
        // every object + optionals as `[T,"null"]`) must land legal for
        // gemini — the complement of the openai normalizer.
        let openai_strict = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["id", "note"],
            "properties": {
                "id": {"type": "string"},
                "note": {"type": ["string", "null"]}
            }
        });
        let out = adapt(&openai_strict);
        assert!(out.get("additionalProperties").is_none());
        assert_eq!(out["properties"]["id"]["type"], "string");
        assert_eq!(out["properties"]["note"]["type"], "string");
        assert_eq!(out["properties"]["note"]["nullable"], true);
    }

    // ── BUG#9 · multi-type unions other than [T,null] for gemini ──

    #[test]
    fn adapter_maps_two_type_union_without_null_to_anyof() {
        // `[string,integer]` 400s "type Proto field is not repeating".
        // No null → pure anyOf of scalar branches, no nullable flag.
        let out = adapt(&json!({
            "type": "object",
            "required": ["id"],
            "properties": { "id": {"type": ["string", "integer"]} }
        }));
        let id = &out["properties"]["id"];
        assert!(id.get("type").is_none(), "repeating type removed: {id}");
        assert!(id.get("nullable").is_none(), "no null member → no nullable");
        let branches = id["anyOf"].as_array().expect("anyOf array");
        assert_eq!(branches.len(), 2);
        assert_eq!(branches[0]["type"], "string");
        assert_eq!(branches[1]["type"], "integer");
    }

    #[test]
    fn adapter_maps_three_type_union_to_anyof() {
        // `[string,number,boolean]` → 3-branch anyOf (the k7 repro).
        let out = adapt(&json!({
            "type": "object",
            "required": ["value"],
            "properties": { "value": {"type": ["string", "number", "boolean"]} }
        }));
        let value = &out["properties"]["value"];
        assert!(value.get("type").is_none());
        let kinds: Vec<&str> = value["anyOf"]
            .as_array()
            .expect("anyOf")
            .iter()
            .map(|b| b["type"].as_str().expect("scalar type"))
            .collect();
        assert_eq!(kinds, vec!["string", "number", "boolean"]);
    }

    #[test]
    fn adapter_maps_multi_type_union_with_null_to_anyof_plus_nullable() {
        // `[string,integer,null]` → anyOf of the two real types + nullable.
        let out = adapt(&json!({"type": ["string", "integer", "null"]}));
        assert!(out.get("type").is_none());
        assert_eq!(out["nullable"], true, "null member → nullable flag");
        let kinds: Vec<&str> = out["anyOf"]
            .as_array()
            .expect("anyOf")
            .iter()
            .map(|b| b["type"].as_str().expect("scalar type"))
            .collect();
        assert_eq!(kinds, vec!["string", "integer"], "null is not a branch");
    }

    #[test]
    fn adapter_two_type_union_with_null_still_maps_to_scalar_nullable() {
        // The [T,null] case is unchanged: one real type stays a SCALAR
        // `type` + nullable (NOT a one-branch anyOf) — the common nullable
        // field, the BUG#6 behavior preserved.
        let out = adapt(&json!({"type": ["string", "null"]}));
        assert_eq!(out["type"], "string", "single real type stays scalar");
        assert_eq!(out["nullable"], true);
        assert!(out.get("anyOf").is_none(), "no anyOf for a lone type");
    }

    // ── BUG#10 · non-string enum members stringified for gemini ──
    //
    // Gemini's `responseSchema.enum` is a repeated STRING — a numeric member
    // 400s "enum[0] (TYPE_STRING), 200" until it is stringified. Verified
    // against real gemini-2.5-flash: `{type:integer, enum:["200","404",
    // "500"]}` → `{"code":404}` (the type is kept · gemini coerces the
    // chosen string member back to the declared type in the response).

    #[test]
    fn adapter_stringifies_integer_enum_members_keeping_the_type() {
        // `{type:integer, enum:[200,404,500]}` → members become STRINGS, the
        // explicit `type:integer` is left as authored (the k10 repro · the
        // exact shape real gemini accepted).
        let out = adapt(&json!({
            "type": "object",
            "required": ["status"],
            "properties": {
                "status": {"type": "integer", "enum": [200, 404, 500]}
            }
        }));
        let status = &out["properties"]["status"];
        assert_eq!(status["type"], "integer", "authored type untouched");
        assert_eq!(
            status["enum"],
            json!(["200", "404", "500"]),
            "members stringified"
        );
        // every member is now a JSON string (gemini's repeated-STRING enum).
        for m in status["enum"].as_array().expect("enum array") {
            assert!(m.is_string(), "enum member is a string: {m}");
        }
    }

    #[test]
    fn adapter_string_enum_is_unchanged() {
        // a string enum keeps its (already-string) members and type — the
        // stringify is a no-op (the control case · works on both wires).
        let out = adapt(&json!({
            "type": "string",
            "enum": ["ok", "not_found", "error"]
        }));
        assert_eq!(out["type"], "string");
        assert_eq!(out["enum"], json!(["ok", "not_found", "error"]));
    }

    #[test]
    fn adapter_stringifies_a_typeless_int_enum_without_inventing_a_type() {
        // A typeless integer enum (e.g. a `const:200` rewritten to
        // `enum:[200]`) would 400 on the numeric member — stringify it. No
        // `type` is invented (gemini reads the constraint off the strings).
        let out = adapt(&json!({"enum": [200, 404]}));
        assert!(out.get("type").is_none(), "no type invented");
        assert_eq!(out["enum"], json!(["200", "404"]), "members stringified");
    }

    #[test]
    fn adapter_stringifies_enum_members_of_every_scalar_kind() {
        // number (non-integer), bool, and string all reach the wire as
        // strings; a string member is left byte-identical.
        assert_eq!(
            adapt(&json!({"enum": [1.5, 2.5]}))["enum"],
            json!(["1.5", "2.5"])
        );
        assert_eq!(
            adapt(&json!({"enum": [true, false]}))["enum"],
            json!(["true", "false"])
        );
        assert_eq!(
            adapt(&json!({"enum": ["a", "b"]}))["enum"],
            json!(["a", "b"])
        );
        // a mixed enum is fine too — each scalar member stringifies.
        assert_eq!(
            adapt(&json!({"enum": [1, "two"]}))["enum"],
            json!(["1", "two"])
        );
    }

    #[test]
    fn enum_repr_is_the_gemini_side_of_the_parity_gap() {
        // The cross-provider asymmetry the fix relies on: gemini's `enum`
        // is a repeated STRING, so the SAME author schema reaches this wire
        // with members STRINGIFIED — whereas the openai strict normalizer
        // keeps native typed members (asserted by `openai_compat`'s
        // `strict_rewrites_const_to_single_member_enum_preserving_type` +
        // `strict_required_field_keeps_its_scalar_type`). Each wire owns its
        // dialect rewrite; the two transforms never share a path.
        let author = json!({"type": "integer", "enum": [200, 404]});
        let gemini = adapt(&author);
        assert_eq!(gemini["type"], "integer", "authored type kept");
        assert_eq!(
            gemini["enum"],
            json!(["200", "404"]),
            "gemini stringifies enum members"
        );
        for m in gemini["enum"].as_array().expect("gemini enum") {
            assert!(m.is_string(), "gemini enum member is a string: {m}");
        }
    }

    // ── const + uniqueItems · gemini (Gate 2 parity) ──

    #[test]
    fn adapter_rewrites_const_to_a_stringified_enum() {
        // `const:1` → `enum:[1]`, and the member is then stringified to
        // `"1"` (gemini's enum is a repeated STRING) — the full chain. No
        // `type` is invented (the k12 repro · adapted to the real shape).
        let out = adapt(&json!({
            "type": "object",
            "required": ["version", "name"],
            "properties": {
                "version": {"const": 1},
                "name": {"type": "string"}
            }
        }));
        let version = &out["properties"]["version"];
        assert!(version.get("const").is_none(), "no const survives");
        assert_eq!(
            version["enum"],
            json!(["1"]),
            "const → one-member string enum"
        );
        assert!(version["enum"][0].is_string(), "member stringified");
        assert!(version.get("type").is_none(), "no type invented");
    }

    #[test]
    fn adapter_rewrites_string_const_to_a_one_member_enum() {
        let out = adapt(&json!({"const": "v1"}));
        assert!(out.get("const").is_none());
        assert_eq!(out["enum"], json!(["v1"]));
    }

    #[test]
    fn adapter_strips_unique_items() {
        // `uniqueItems` is array-only validation, not in the dialect → drop.
        let out = adapt(&json!({
            "type": "object",
            "required": ["fruits"],
            "properties": {
                "fruits": {
                    "type": "array",
                    "uniqueItems": true,
                    "items": {"type": "string"}
                }
            }
        }));
        let fruits = &out["properties"]["fruits"];
        assert!(fruits.get("uniqueItems").is_none(), "uniqueItems stripped");
        assert_eq!(fruits["type"], "array");
        assert_eq!(fruits["items"]["type"], "string");
    }
}
