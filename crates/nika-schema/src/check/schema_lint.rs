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

use crate::raw::{RawAction, RawWorkflow};

use super::suggest::{did_you_mean, suggestion_clause};

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
fn kind(v: &Value) -> &'static str {
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
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn findings_of(yaml: &str) -> Vec<SchemaLintFinding> {
        scan_schemas(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    fn infer_with_schema(schema_yaml: &str) -> String {
        format!(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: t\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n{schema_yaml}"
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
    fn nested_branches_are_descended() {
        let f = findings_of(&infer_with_schema(
            "        anyOf:\n          - type: object\n            properties:\n              a: { type: string }\n            required: [b]\n          - type: object\n            properties:\n              c: { type: strng }\n",
        ));
        assert_eq!(f.len(), 2, "{f:?}");
        assert!(f.iter().any(|x| x.path == "/anyOf/0/required"));
        assert!(f.iter().any(|x| x.path == "/anyOf/1/properties/c/type"));
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
        let f = findings_of(
            "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    exec: { command: \"true\" }\n",
        );
        assert!(f.is_empty());
    }
}
