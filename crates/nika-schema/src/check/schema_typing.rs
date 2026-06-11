// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Dataflow schema typing — `${{ tasks.A.output.field }}` type-checked
//! against A's declared shape (ADR-092 #4).
//!
//! A task that declares a `schema:` (infer/agent structured output) or
//! `output:` bindings (jq) has a KNOWN output address space. Every deep
//! reference into `tasks.<id>.output.<path…>` anywhere in the workflow
//! (prompts · commands · args · `when:` · `with:` · `for_each` ·
//! envelope `outputs:` · `on_finally` cleanups) is resolved against that
//! space — a typo'd field name is caught BEFORE a single token is spent,
//! transitively across the DAG. No Turing-complete engine can do this;
//! ours can because outputs carry declared shapes and references are a
//! closed CEL subset.
//!
//! Sound-by-honesty: a shape that goes statically opaque (`$ref` ·
//! explicit `additionalProperties: true` · an un-schema'd task · a jq
//! binding's inner structure) resolves to "unknown — no finding", never
//! a guess. Findings are only emitted where the declared shape PROVES
//! the path cannot exist.

use serde_json::Value;
use std::collections::BTreeMap;

use crate::expression::{scan_templates, task_output_paths};
use crate::raw::{RawAction, RawTask, RawWorkflow};

/// A deep output reference the declared shape proves invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SchemaTypeFinding {
    /// Where the reference appears (task id · `<id> (on_finally)` ·
    /// `outputs`).
    pub site: String,
    /// The reference, rendered (`tasks.a.output.summray`).
    pub reference: String,
    /// The task whose declared shape rejects it.
    pub target: String,
    /// Why — the failing segment + the keys that DO exist there.
    pub detail: String,
}

/// One task's declared output address space.
enum Shape<'a> {
    /// `schema:` on an infer/agent task — a JSON Schema to descend.
    Schema(&'a Value),
    /// `output:` jq bindings — the address space is the binding names
    /// (their inner structure is jq-shaped, statically opaque).
    Bindings(Vec<&'a str>),
}

/// Scan every expression island in the workflow and type-check deep
/// `tasks.<id>.output.<path…>` references against declared shapes.
#[must_use]
pub(super) fn scan_types(wf: &RawWorkflow) -> Vec<SchemaTypeFinding> {
    let shapes = declared_shapes(wf);
    if shapes.is_empty() {
        return Vec::new(); // nothing is typed → nothing to check
    }
    let mut findings = Vec::new();
    for task in &wf.tasks {
        let id = task.value.id.value.as_str();
        for text in task_texts(&task.value) {
            check_text(id, text, &shapes, &mut findings);
        }
        for cleanup in &task.value.on_finally {
            let site = format!("{id} (on_finally)");
            if let Some(when) = &cleanup.value.when {
                check_text(&site, &when.value, &shapes, &mut findings);
            }
            for text in action_texts(&cleanup.value.action) {
                check_text(&site, text, &shapes, &mut findings);
            }
        }
    }
    for (_, decl) in &wf.outputs {
        check_text("outputs", &decl.value().value, &shapes, &mut findings);
    }
    findings
}

/// Collect each task's declared output address space. `output:` bindings
/// REBIND the output namespace, so they take precedence over `schema:`.
fn declared_shapes(wf: &RawWorkflow) -> BTreeMap<&str, Shape<'_>> {
    let mut shapes = BTreeMap::new();
    for task in &wf.tasks {
        let t = &task.value;
        let id = t.id.value.as_str();
        if !t.output.is_empty() {
            shapes.insert(
                id,
                Shape::Bindings(t.output.iter().map(|(n, _)| n.value.as_str()).collect()),
            );
            continue;
        }
        let schema = match &t.action {
            RawAction::Infer(a) => a.schema.as_ref(),
            RawAction::Agent(a) => a.schema.as_ref(),
            RawAction::Exec(_) | RawAction::Invoke(_) => None,
        };
        if let Some(s) = schema {
            shapes.insert(id, Shape::Schema(&s.value));
        }
    }
    shapes
}

/// Every expression-bearing text of a task (main verb + task-level
/// fields). `output:` binding values are jq programs, not CEL — skipped.
fn task_texts(task: &RawTask) -> Vec<&str> {
    let mut texts = action_texts(&task.action);
    if let Some(when) = &task.when {
        texts.push(&when.value);
    }
    if let Some(f) = &task.for_each
        && let crate::raw::ForEachValue::Expression(src) = &f.value
    {
        texts.push(src);
    }
    for (_, v) in &task.with {
        collect_value_strings(&v.value, &mut texts);
    }
    texts
}

/// Every expression-bearing text of one action (works for main verbs
/// AND `on_finally` cleanup verbs).
fn action_texts(action: &RawAction) -> Vec<&str> {
    match action {
        RawAction::Exec(a) => {
            let mut texts = a.command.text_fragments();
            if let Some(stdin) = &a.stdin {
                texts.push(stdin.value.as_str());
            }
            for (_, v) in &a.env {
                texts.push(v.value.as_str());
            }
            texts
        }
        RawAction::Invoke(a) => {
            let mut texts = Vec::new();
            if let Some(args) = &a.args {
                collect_value_strings(&args.value, &mut texts);
            }
            texts
        }
        RawAction::Infer(a) => {
            let mut texts = vec![a.prompt.value.as_str()];
            if let Some(system) = &a.system {
                texts.push(&system.value);
            }
            texts
        }
        RawAction::Agent(a) => {
            let mut texts = vec![a.prompt.value.as_str()];
            if let Some(system) = &a.system {
                texts.push(&system.value);
            }
            texts
        }
    }
}

/// Every string scalar inside a JSON value (invoke args · with values).
fn collect_value_strings<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
    match value {
        Value::String(s) => out.push(s),
        Value::Array(items) => {
            for item in items {
                collect_value_strings(item, out);
            }
        }
        Value::Object(map) => {
            for item in map.values() {
                collect_value_strings(item, out);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

/// Type-check every island of one text against the declared shapes.
fn check_text(
    site: &str,
    text: &str,
    shapes: &BTreeMap<&str, Shape<'_>>,
    findings: &mut Vec<SchemaTypeFinding>,
) {
    // a malformed island is the parser/analyzer's finding, not ours
    let Ok(islands) = scan_templates(text) else {
        return;
    };
    for island in islands {
        for (target, path) in task_output_paths(&island.expr) {
            if path.is_empty() {
                continue; // whole-output reference · always valid
            }
            let Some(shape) = shapes.get(target.as_str()) else {
                continue; // un-shaped task · output is opaque
            };
            let detail = match shape {
                Shape::Schema(schema) => resolve(schema, &path),
                Shape::Bindings(names) => check_binding(names, &path),
            };
            if let Some(detail) = detail {
                findings.push(SchemaTypeFinding {
                    site: site.to_owned(),
                    reference: format!("tasks.{target}.output.{}", path.join(".")),
                    target: target.clone(),
                    detail,
                });
            }
        }
    }
}

/// Check a deep path against `output:` binding names — the first segment
/// must be a binding; the inner structure is jq-shaped (opaque).
fn check_binding(names: &[&str], path: &[String]) -> Option<String> {
    let first = path.first()?;
    if names.contains(&first.as_str()) {
        return None;
    }
    Some(format!(
        "`{first}` is not one of the declared output bindings [{}]",
        names.join(", ")
    ))
}

/// Resolve a path against a JSON Schema. `Some(detail)` when the schema
/// PROVES the path invalid; `None` when it resolves or goes opaque.
fn resolve(schema: &Value, path: &[String]) -> Option<String> {
    let mut node = schema;
    for (depth, segment) in path.iter().enumerate() {
        // `$ref` indirection is statically opaque (no resolver here)
        if node.get("$ref").is_some() {
            return None;
        }
        // combinators: OK if ANY branch admits the rest of the path
        for key in ["anyOf", "oneOf", "allOf"] {
            if let Some(branches) = node.get(key).and_then(Value::as_array) {
                return branches
                    .iter()
                    .all(|b| resolve(b, &path[depth..]).is_some())
                    .then(|| format!("no `{key}` branch declares `{segment}`"));
            }
        }
        // arrays are transparent: `output[0].title` drops the index hop,
        // so descend `items` until the element schema surfaces
        while node.get("type").and_then(Value::as_str) == Some("array") {
            match node.get("items") {
                Some(items) => node = items,
                None => return None, // un-itemized array · opaque
            }
        }
        let Some(props) = node.get("properties").and_then(Value::as_object) else {
            return match node.get("type").and_then(Value::as_str) {
                // an object with no properties map is opaque
                Some("object") | None => None,
                Some(scalar) => Some(format!(
                    "cannot descend into `{scalar}`-typed value with `.{segment}`"
                )),
            };
        };
        if let Some(next) = props.get(segment.as_str()) {
            node = next;
        } else if is_open_object(node) {
            return None; // explicitly open · unknown keys allowed
        } else {
            return Some(format!(
                "`{segment}` is not in the declared schema — keys here: [{}]",
                props.keys().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
    }
    None
}

/// Whether an object schema explicitly allows undeclared keys
/// (`additionalProperties: true` or a schema object). JSON Schema's
/// DEFAULT is open, but a structured-output `schema:` compiles strict —
/// flagging unknown keys against declared `properties` is the point of
/// the check, so only an EXPLICIT opt-out goes opaque.
fn is_open_object(node: &Value) -> bool {
    match node.get("additionalProperties") {
        Some(Value::Bool(open)) => *open,
        Some(Value::Object(_)) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn findings_of(yaml: &str) -> Vec<SchemaTypeFinding> {
        scan_types(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    /// An infer task with a 2-field object schema, consumed by `use_it`.
    fn schema_wf(consumer_expr: &str) -> String {
        format!(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: extract\n    infer:\n      prompt: \"extract\"\n      max_tokens: 100\n      schema:\n        type: object\n        properties:\n          summary: {{ type: string }}\n          tags:\n            type: array\n            items:\n              type: object\n              properties:\n                name: {{ type: string }}\n        required: [summary]\n  - id: use_it\n    depends_on: [extract]\n    exec: {{ command: \"echo {consumer_expr}\" }}\n"
        )
    }

    #[test]
    fn typo_in_field_name_is_caught() {
        // THE headline: `summray` does not exist — caught with zero tokens.
        let f = findings_of(&schema_wf("${{ tasks.extract.output.summray }}"));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].site, "use_it");
        assert_eq!(f[0].reference, "tasks.extract.output.summray");
        assert!(f[0].detail.contains("summary"), "lists the real keys");
    }

    #[test]
    fn valid_field_is_clean() {
        assert!(findings_of(&schema_wf("${{ tasks.extract.output.summary }}")).is_empty());
    }

    #[test]
    fn array_items_descend_transparently() {
        // `tags[0].name` — the index hop drops, items descends.
        assert!(findings_of(&schema_wf("${{ tasks.extract.output.tags[0].name }}")).is_empty());
        let f = findings_of(&schema_wf("${{ tasks.extract.output.tags[0].nmae }}"));
        assert_eq!(f.len(), 1);
        assert!(f[0].detail.contains("name"), "detail: {}", f[0].detail);
    }

    #[test]
    fn scalar_descent_is_proven_invalid() {
        let f = findings_of(&schema_wf("${{ tasks.extract.output.summary.length }}"));
        assert_eq!(f.len(), 1);
        assert!(
            f[0].detail.contains("string"),
            "names the scalar type: {}",
            f[0].detail
        );
    }

    #[test]
    fn unshaped_task_is_opaque() {
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    exec: { command: \"date\" }\n  - id: b\n    depends_on: [a]\n    exec: { command: \"echo ${{ tasks.a.output.whatever }}\" }\n";
        assert!(findings_of(yaml).is_empty(), "no schema → no claim");
    }

    #[test]
    fn explicitly_open_schema_is_opaque() {
        let yaml = "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        additionalProperties: true\n        properties:\n          known: { type: string }\n  - id: b\n    depends_on: [a]\n    exec: { command: \"echo ${{ tasks.a.output.unknown_key }}\" }\n";
        assert!(findings_of(yaml).is_empty(), "explicit opt-out honored");
    }

    #[test]
    fn output_bindings_rebind_the_address_space() {
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    exec: { command: \"cat data.json\" }\n    output:\n      first: \". | .[0]\"\n  - id: b\n    depends_on: [a]\n    exec: { command: \"echo ${{ tasks.a.output.first }} ${{ tasks.a.output.frist }}\" }\n";
        let f = findings_of(yaml);
        assert_eq!(f.len(), 1, "first ok, frist flagged");
        assert!(f[0].detail.contains("first"), "lists bindings");
    }

    #[test]
    fn prompt_interpolation_is_checked() {
        // The wow case — a typo inside an infer PROMPT caught statically.
        let yaml = "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: extract\n    infer:\n      prompt: \"extract\"\n      max_tokens: 100\n      schema:\n        type: object\n        properties:\n          summary: { type: string }\n  - id: report\n    depends_on: [extract]\n    infer: { prompt: \"report on ${{ tasks.extract.output.sumary }}\", max_tokens: 50 }\n";
        let f = findings_of(yaml);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].site, "report");
    }

    #[test]
    fn envelope_outputs_and_on_finally_are_checked() {
        let yaml = "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: extract\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        properties:\n          summary: { type: string }\n    on_finally:\n      - invoke: { tool: \"nika:log\", args: { message: \"${{ tasks.extract.output.sumamry }}\" } }\noutputs:\n  result: ${{ tasks.extract.output.summry }}\n";
        let f = findings_of(yaml);
        assert_eq!(f.len(), 2, "both surfaces: {f:?}");
        assert!(f.iter().any(|x| x.site == "extract (on_finally)"));
        assert!(f.iter().any(|x| x.site == "outputs"));
    }

    #[test]
    fn any_of_admits_when_one_branch_matches() {
        let yaml = "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        anyOf:\n          - type: object\n            properties:\n              left: { type: string }\n          - type: object\n            properties:\n              right: { type: string }\n  - id: b\n    depends_on: [a]\n    exec: { command: \"echo ${{ tasks.a.output.left }} ${{ tasks.a.output.neither }}\" }\n";
        let f = findings_of(yaml);
        assert_eq!(f.len(), 1, "left admits, neither fails all branches");
        assert!(f[0].reference.ends_with("neither"));
    }
}
