// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Structured-output schema lint — the `schema:` block itself verified
//! BEFORE any run (the static half of « structured output works in all
//! cases »).
//!
//! The runtime floor (nika-verb-infer `structured.rs`) validates model
//! output LOCALLY against the compiled schema — provider parity is
//! structural (no provider-native schema mode is relied on). What can
//! still break is the AUTHORED schema: a `required` key that's not in
//! `properties` makes valid-looking output unsatisfiable (every model
//! attempt fails validation, burns every retry, then errors) · a typo'd
//! `type` name fails schema compilation at dispatch · an empty `enum`
//! admits nothing. All of it is decidable here, with the deterministic
//! « did you mean » fix attached.
//!
//! Descends the same composite shapes the dataflow typer walks
//! (`properties` · `items` · `anyOf`/`oneOf`/`allOf`); `$ref` is opaque
//! (no resolver — never a false claim).

use serde_json::Value;

use nika_schema::raw::{RawAction, RawWorkflow};

use nika_types::suggest::{did_you_mean, suggestion_clause};

/// The 7 JSON-Schema type names (draft 2020-12 · what the runtime
/// validator compiles).
const TYPE_NAMES: [&str; 7] = [
    "array", "boolean", "integer", "null", "number", "object", "string",
];

/// A defect in an authored `schema:` block that makes structured output
/// unsatisfiable or un-compilable at runtime.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct SchemaLintFinding {
    /// The task declaring the schema.
    pub task: String,
    /// JSON-pointer-ish location inside the schema (`/properties/tags`).
    pub path: String,
    /// What is wrong, with the « did you mean » fix when one is close.
    pub detail: String,
}

/// Lint every `schema:` block in the workflow (infer + agent tasks).
#[must_use]
pub(super) fn scan_schemas(wf: &RawWorkflow) -> Vec<SchemaLintFinding> {
    let mut findings = Vec::new();
    for task in &wf.tasks {
        let schema = match &task.value.action {
            RawAction::Infer(a) => a.schema.as_ref(),
            RawAction::Agent(a) => a.schema.as_ref(),
            RawAction::Exec(_) | RawAction::Invoke(_) => None,
            #[allow(
                clippy::unreachable,
                reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
            )]
            other => unreachable!("unknown action: {other:?}"),
        };
        if let Some(schema) = schema {
            lint_node(
                task.value.id.value.as_str(),
                &schema.value,
                "",
                &mut findings,
            );
        }
    }
    findings
}

/// Lint one schema node, then descend its composite shapes.
fn lint_node(task: &str, node: &Value, path: &str, out: &mut Vec<SchemaLintFinding>) {
    let Some(obj) = node.as_object() else {
        // `true`/`false` are valid boolean schemas; anything else is not
        // a schema node — the runtime compiler rejects it.
        if !node.is_boolean() {
            out.push(finding(
                task,
                path,
                format!(
                    "a schema node must be an object or boolean, got {}",
                    kind(node)
                ),
            ));
        }
        return;
    };
    if obj.contains_key("$ref") {
        return; // opaque — no resolver, never a false claim
    }

    check_type_names(task, obj, path, out);
    check_required_vs_properties(task, obj, path, out);
    check_enum(task, obj, path, out);
    check_enum_vs_type(task, obj, path, out);
    check_numeric_bounds(task, obj, path, out);

    // descend
    if let Some(props) = obj.get("properties") {
        match props.as_object() {
            Some(map) => {
                for (key, sub) in map {
                    lint_node(task, sub, &format!("{path}/properties/{key}"), out);
                }
            }
            None => out.push(finding(
                task,
                &format!("{path}/properties"),
                format!("`properties` must be an object, got {}", kind(props)),
            )),
        }
    }
    if let Some(items) = obj.get("items") {
        lint_node(task, items, &format!("{path}/items"), out);
    }
    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(branches) = obj.get(key) {
            match branches.as_array() {
                Some(arr) => {
                    for (i, branch) in arr.iter().enumerate() {
                        lint_node(task, branch, &format!("{path}/{key}/{i}"), out);
                    }
                }
                None => out.push(finding(
                    task,
                    &format!("{path}/{key}"),
                    format!(
                        "`{key}` must be an array of schemas, got {}",
                        kind(branches)
                    ),
                )),
            }
        }
    }
}

/// `type:` must name a JSON type (string or array-of-strings form).
fn check_type_names(
    task: &str,
    obj: &serde_json::Map<String, Value>,
    path: &str,
    out: &mut Vec<SchemaLintFinding>,
) {
    let Some(ty) = obj.get("type") else {
        return;
    };
    let names: Vec<&str> = match ty {
        Value::String(s) => vec![s.as_str()],
        Value::Array(arr) => arr.iter().filter_map(Value::as_str).collect(),
        _ => {
            out.push(finding(
                task,
                &format!("{path}/type"),
                format!(
                    "`type` must be a string or array of strings, got {}",
                    kind(ty)
                ),
            ));
            return;
        }
    };
    for name in names {
        if !TYPE_NAMES.contains(&name) {
            let clause = suggestion_clause(did_you_mean(name, TYPE_NAMES));
            out.push(finding(
                task,
                &format!("{path}/type"),
                format!("`{name}` is not a JSON Schema type{clause}"),
            ));
        }
    }
}

/// Every `required` entry must exist in `properties` — the classic
/// authoring bug that makes ALL structured output unsatisfiable (the
/// model can never emit the misspelled key the validator demands).
fn check_required_vs_properties(
    task: &str,
    obj: &serde_json::Map<String, Value>,
    path: &str,
    out: &mut Vec<SchemaLintFinding>,
) {
    let Some(required) = obj.get("required").and_then(Value::as_array) else {
        return;
    };
    // `required` without `properties` is legal JSON Schema (free-form
    // object with mandated keys) — only a declared map can contradict.
    let Some(props) = obj.get("properties").and_then(Value::as_object) else {
        return;
    };
    for entry in required.iter().filter_map(Value::as_str) {
        if !props.contains_key(entry) {
            let clause = suggestion_clause(did_you_mean(entry, props.keys().map(String::as_str)));
            out.push(finding(
                task,
                &format!("{path}/required"),
                format!(
                    "required key `{entry}` is not in `properties` — every model \
                     output will fail validation{clause}"
                ),
            ));
        }
    }
}

/// `enum:` must admit at least one value.
fn check_enum(
    task: &str,
    obj: &serde_json::Map<String, Value>,
    path: &str,
    out: &mut Vec<SchemaLintFinding>,
) {
    if let Some(values) = obj.get("enum").and_then(Value::as_array)
        && values.is_empty()
    {
        out.push(finding(
            task,
            &format!("{path}/enum"),
            "`enum: []` admits NO value — unsatisfiable".to_owned(),
        ));
    }
}

/// `type:` and a non-empty `enum:` both constrain the value (intersection in
/// JSON Schema). If NO `enum` value matches the declared `type`, every model
/// output fails validation — unsatisfiable (e.g. `type: string, enum: [1, 2]`).
/// A partial mismatch is fine (the matching members keep it satisfiable · the
/// dead ones are an author's concern, not an engine-blocking error).
fn check_enum_vs_type(
    task: &str,
    obj: &serde_json::Map<String, Value>,
    path: &str,
    out: &mut Vec<SchemaLintFinding>,
) {
    let Some(values) = obj.get("enum").and_then(Value::as_array) else {
        return;
    };
    if values.is_empty() {
        return; // the empty-enum lint owns this
    }
    let names: Vec<&str> = match obj.get("type") {
        Some(Value::String(s)) => vec![s.as_str()],
        Some(Value::Array(arr)) => arr.iter().filter_map(Value::as_str).collect(),
        _ => return, // no (valid) declared type → nothing to contradict
    };
    if names.is_empty() {
        return;
    }
    if !values
        .iter()
        .any(|v| names.iter().any(|t| json_matches_type(v, t)))
    {
        out.push(finding(
            task,
            &format!("{path}/enum"),
            format!(
                "no `enum` value matches `type: {}` — every model output will fail validation",
                names.join("|")
            ),
        ));
    }
}

/// Whether a literal JSON value matches a JSON-Schema `type` name. `integer`
/// is the number subtype (an integral value matches both `integer` and
/// `number`; a fractional number matches only `number`).
fn json_matches_type(value: &Value, type_name: &str) -> bool {
    match type_name {
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => {
            value.is_i64() || value.is_u64() || value.as_f64().is_some_and(|n| n.fract() == 0.0)
        }
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        "null" => value.is_null(),
        _ => false,
    }
}

/// A lower bound strictly above its paired upper bound is unsatisfiable — no
/// value can be both. Covers the four JSON-Schema min/max pairs (numeric
/// range · string length · array length · object property count).
fn check_numeric_bounds(
    task: &str,
    obj: &serde_json::Map<String, Value>,
    path: &str,
    out: &mut Vec<SchemaLintFinding>,
) {
    for (lo, hi, what) in [
        ("minimum", "maximum", "numeric range"),
        ("minLength", "maxLength", "string length"),
        ("minItems", "maxItems", "array length"),
        ("minProperties", "maxProperties", "object property count"),
    ] {
        // Compare as f64; render from the original `Value` so the message
        // keeps the author's own form (`10`, not `10.0`) — and no f64→int
        // cast (clippy::cast_possible_truncation).
        if let (Some(lv), Some(hv)) = (obj.get(lo), obj.get(hi))
            && let (Some(l), Some(h)) = (lv.as_f64(), hv.as_f64())
            && l > h
        {
            out.push(finding(
                task,
                path,
                format!("`{lo}: {lv}` > `{hi}: {hv}` ({what}) — no value can satisfy both"),
            ));
        }
    }
}

fn finding(task: &str, path: &str, detail: String) -> SchemaLintFinding {
    SchemaLintFinding {
        task: task.to_owned(),
        path: if path.is_empty() {
            "/".to_owned()
        } else {
            path.to_owned()
        },
        detail,
    }
}

/// A short JSON kind name for diagnostics.
pub(crate) fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;
    use serde_json::json;

    fn findings_of(yaml: &str) -> Vec<SchemaLintFinding> {
        scan_schemas(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    fn infer_with_schema(schema_yaml: &str) -> String {
        format!(
            "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  t:\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n{schema_yaml}"
        )
    }

    #[test]
    fn required_key_missing_from_properties_is_the_headline() {
        // THE structured-output killer: every model attempt fails
        // validation, burns all retries, then errors — now caught at
        // check time with the fix.
        let f = findings_of(&infer_with_schema(
            "        type: object\n        properties:\n          summary: { type: string }\n        required: [sumary]\n",
        ));
        assert_eq!(f.len(), 1);
        assert!(f[0].detail.contains("did you mean `summary`"), "{f:?}");
    }

    #[test]
    fn valid_schema_is_clean() {
        let f = findings_of(&infer_with_schema(
            "        type: object\n        properties:\n          summary: { type: string }\n          tags:\n            type: array\n            items: { type: string }\n        required: [summary]\n",
        ));
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn typo_d_type_name_is_caught() {
        let f = findings_of(&infer_with_schema(
            "        type: object\n        properties:\n          n: { type: integre }\n",
        ));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].path, "/properties/n/type");
        assert!(f[0].detail.contains("did you mean `integer`"), "{f:?}");
    }

    #[test]
    fn empty_enum_is_unsatisfiable() {
        let f = findings_of(&infer_with_schema(
            "        type: object\n        properties:\n          status: { enum: [] }\n",
        ));
        assert_eq!(f.len(), 1);
        assert!(f[0].detail.contains("unsatisfiable"));
    }

    #[test]
    fn enum_values_conflicting_the_type_is_unsatisfiable() {
        // type: string + enum: [1, 2] — no enum value is a string, so every
        // model output fails validation (the S5 gap · now caught).
        let f = findings_of(&infer_with_schema(
            "        type: object\n        properties:\n          n: { type: string, enum: [1, 2] }\n",
        ));
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].path, "/properties/n/enum");
        assert!(f[0].detail.contains("no `enum` value matches"), "{f:?}");
    }

    #[test]
    fn enum_partially_matching_the_type_is_clean() {
        // type: string + enum: [1, "ok"] — "ok" satisfies both, so the schema
        // is satisfiable (the dead `1` is the author's concern, not an error).
        let f = findings_of(&infer_with_schema(
            "        type: object\n        properties:\n          s: { type: string, enum: [1, \"ok\"] }\n",
        ));
        assert!(
            f.is_empty(),
            "a partially-matching enum stays satisfiable: {f:?}"
        );
    }

    #[test]
    fn integer_enum_matches_integer_and_number_types() {
        // integer values satisfy both `integer` and `number` — no false flag.
        let f = findings_of(&infer_with_schema(
            "        type: object\n        properties:\n          a: { type: integer, enum: [1, 2] }\n          b: { type: number, enum: [3, 4] }\n",
        ));
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn numeric_minimum_above_maximum_is_unsatisfiable() {
        // minimum: 10 > maximum: 5 — no value fits (the S6 gap · now caught).
        let f = findings_of(&infer_with_schema(
            "        type: object\n        properties:\n          n: { type: integer, minimum: 10, maximum: 5 }\n",
        ));
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].detail.contains("no value can satisfy both"), "{f:?}");
    }

    #[test]
    fn length_and_items_bound_conflicts_are_caught() {
        // minLength > maxLength and minItems > maxItems are both unsatisfiable.
        let f = findings_of(&infer_with_schema(
            "        type: object\n        properties:\n          s: { type: string, minLength: 5, maxLength: 2 }\n          a: { type: array, minItems: 9, maxItems: 1 }\n",
        ));
        assert_eq!(f.len(), 2, "{f:?}");
        assert!(
            f.iter()
                .all(|x| x.detail.contains("no value can satisfy both")),
            "{f:?}"
        );
    }

    #[test]
    fn well_ordered_bounds_are_clean() {
        let f = findings_of(&infer_with_schema(
            "        type: object\n        properties:\n          n: { type: integer, minimum: 1, maximum: 10 }\n",
        ));
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn nested_branches_are_descended() {
        let f = findings_of(&infer_with_schema(
            "        anyOf:\n          - type: object\n            properties:\n              a: { type: string }\n            required: [b]\n          - type: object\n            properties:\n              c: { type: strng }\n",
        ));
        assert_eq!(f.len(), 2, "{f:?}");
        assert!(f.iter().any(|x| x.path == "/anyOf/0/required"));
        assert!(f.iter().any(|x| x.path == "/anyOf/1/properties/c/type"));
    }

    #[test]
    fn non_schema_scalar_node_is_flagged_with_its_kind() {
        // `items: 42` — not an object, not a boolean → finding naming
        // the JSON kind (pins `kind()`'s output, not just existence).
        let f = findings_of(&infer_with_schema(
            "        type: array\n        items: 42\n",
        ));
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].detail.contains("a number"), "{f:?}");
        // boolean schemas are LEGAL (`items: false` = no items admitted)
        let ok = findings_of(&infer_with_schema(
            "        type: array\n        items: false\n",
        ));
        assert!(ok.is_empty(), "boolean schema is valid: {ok:?}");
    }

    #[test]
    fn type_array_form_is_checked_per_entry() {
        // `type: [string, "null"]` is the nullable idiom — every entry
        // of the array form is validated, not just the string form.
        let f = findings_of(&infer_with_schema(
            "        type: object\n        properties:\n          v: { type: [strng, \"null\"] }\n",
        ));
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].detail.contains("did you mean `string`"), "{f:?}");
        let ok = findings_of(&infer_with_schema(
            "        type: object\n        properties:\n          v: { type: [string, \"null\"] }\n",
        ));
        assert!(ok.is_empty(), "{ok:?}");
    }

    #[test]
    fn required_without_properties_is_legal() {
        // free-form object with mandated keys — valid JSON Schema.
        let f = findings_of(&infer_with_schema(
            "        type: object\n        required: [anything]\n",
        ));
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn ref_is_opaque_never_a_false_claim() {
        let f = findings_of(&infer_with_schema(
            "        $ref: \"#/defs/x\"\n        required: [whatever]\n",
        ));
        assert!(f.is_empty(), "no resolver → no claim");
    }

    #[test]
    fn unschema_d_tasks_are_skipped() {
        let f = findings_of("nika: w\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n");
        assert!(f.is_empty());
    }

    #[test]
    fn json_matches_type_recognizes_every_type_name() {
        // One TRUE case per arm — deleting any of the boolean / array /
        // object / null arms (or string / number / integer) makes the
        // value stop matching its own declared type.
        assert!(json_matches_type(&json!("hi"), "string"));
        assert!(json_matches_type(&json!(1.5), "number"));
        assert!(json_matches_type(&json!(7), "integer"));
        assert!(json_matches_type(&json!(true), "boolean"));
        assert!(json_matches_type(&json!(false), "boolean"));
        assert!(json_matches_type(&json!([1, 2]), "array"));
        assert!(json_matches_type(&json!({"k": 1}), "object"));
        assert!(json_matches_type(&json!(null), "null"));
        // A value never matches a foreign type (pins the per-arm verdict,
        // so the TRUE cases above can only pass via the right arm).
        assert!(!json_matches_type(&json!("hi"), "boolean"));
        assert!(!json_matches_type(&json!(true), "string"));
        assert!(!json_matches_type(&json!([1]), "object"));
        assert!(!json_matches_type(&json!(null), "array"));
        assert!(!json_matches_type(&json!({}), "null"));
    }

    #[test]
    fn json_matches_type_integer_covers_the_fract_check() {
        // `integer` = is_i64 || is_u64 || (whole f64). Pin the f64 arm's
        // `n.fract() == 0.0` predicate (the `==`→`!=` mutant): a whole
        // float matches, a fractional one does not.
        // Negative + large-unsigned integers stay integers (both whole).
        assert!(json_matches_type(&json!(-1), "integer"));
        assert!(json_matches_type(
            &json!(9_223_372_036_854_775_808_u64),
            "integer"
        ));
        // Whole-valued float: is_i64 false, is_u64 false, fract == 0 → match.
        // Flipping `==` to `!=` drops this case (0.0 != 0.0 is false).
        assert!(json_matches_type(&json!(3.0), "integer"));
        // Fractional float: no disjunct holds → not an integer. Flipping
        // `==` to `!=` would wrongly let 3.5 match (0.5 != 0.0 is true).
        assert!(!json_matches_type(&json!(3.5), "integer"));
    }

    #[test]
    fn enum_vs_type_handles_the_array_type_form() {
        // `type: [string]` is the array form of the declared type. With
        // an all-non-string enum, no value matches → must flag (deleting
        // the `Value::Array` arm makes the array-form `type` invisible
        // and the conflict slips through silently).
        let f = findings_of(&infer_with_schema(
            "        type: object\n        properties:\n          n: { type: [string], enum: [1, 2] }\n",
        ));
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].path, "/properties/n/enum");
        assert!(f[0].detail.contains("no `enum` value matches"), "{f:?}");
    }

    #[test]
    fn numeric_bound_equal_to_its_pair_is_satisfiable() {
        // minimum == maximum admits exactly one value — satisfiable, so
        // NO finding. The comparison is strictly `>`; relaxing it to `>=`
        // would falsely flag the equal-bounds case.
        let f = findings_of(&infer_with_schema(
            "        type: object\n        properties:\n          n: { type: integer, minimum: 5, maximum: 5 }\n",
        ));
        assert!(f.is_empty(), "equal bounds are satisfiable: {f:?}");
    }
}
