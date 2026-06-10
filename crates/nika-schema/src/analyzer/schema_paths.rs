// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Static binding validation — spec `04-variables.md` §Static binding
//! validation against a declared `schema:` (normative).
//!
//! When the producing task declares a structured-output `schema:`
//! (`infer:` / `agent:`), reference paths INTO `tasks.X.output` are
//! checked at parse time. The contract is **soundness** · only
//! PROVABLY-invalid paths are rejected (`NIKA-VAR-003`) ·
//!
//! 1. a member step on a level declaring `additionalProperties: false`
//!    whose `properties` omit the key;
//! 2. a member step on a level whose `type` excludes `object`;
//! 3. an index step on a level whose `type` excludes `array`.
//!
//! The walk covers the v0.1 subset (`properties` · `items` · `type` ·
//! `additionalProperties`) — any other construct (`$ref` · `oneOf` ·
//! `anyOf` · `allOf` · `patternProperties` · `not` · `if`) makes the
//! level OPEN and the walk stops (nothing beneath is rejected).

use std::fmt::Write as _;

use serde_json::Value;

use crate::error::SchemaError;
use crate::expression::{Expr, Literal};
use crate::source::Span;

use super::scan::WorkflowIndex;

/// One step of an output path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Step {
    /// `.key` or `['key']` — a member access.
    Member(String),
    /// `[3]` — a literal index access.
    Index(u64),
    /// A non-literal index — the rest of the chain is unknowable.
    Dynamic,
}

/// Check one parsed expression island for provably-invalid output paths.
pub(super) fn check_expr(
    expr: &Expr,
    span: Span,
    index: &WorkflowIndex<'_>,
    errors: &mut Vec<SchemaError>,
) {
    let mut paths = Vec::new();
    collect_output_paths(expr, &mut paths);
    for (task_id, steps) in paths {
        let Some(schema) = index.schema_of(&task_id) else {
            continue; // dynamic producer · never statically rejected
        };
        if let Some(reason) = provably_invalid(schema, &steps) {
            errors.push(SchemaError::OutputPathProvablyInvalid {
                task: task_id,
                path: render_path(&steps),
                reason,
                span: Some(span),
            });
        }
    }
}

/// Walk an expression tree collecting every `tasks.<id>.output<steps>`
/// chain. A matched chain's spine is NOT re-visited (no double
/// reporting of its prefixes) — only dynamic index sub-expressions
/// inside it are recursed into.
fn collect_output_paths(e: &Expr, out: &mut Vec<(String, Vec<Step>)>) {
    if let Some((id, steps)) = decompose(e) {
        if !steps.is_empty() {
            out.push((id, steps));
        }
        collect_dynamic_index_subexprs(e, out);
        return;
    }
    match e {
        Expr::Or(a, b) | Expr::And(a, b) => {
            collect_output_paths(a, out);
            collect_output_paths(b, out);
        }
        Expr::Not(a) | Expr::SizeCall(a) | Expr::SizeMethod(a) => collect_output_paths(a, out),
        Expr::Relation { lhs, rhs, .. } => {
            collect_output_paths(lhs, out);
            collect_output_paths(rhs, out);
        }
        Expr::Member { base, .. } => collect_output_paths(base, out),
        Expr::Index { base, index } => {
            collect_output_paths(base, out);
            collect_output_paths(index, out);
        }
        Expr::List(items) => {
            for item in items {
                collect_output_paths(item, out);
            }
        }
        Expr::Ident(_) | Expr::Lit(_) => {}
    }
}

/// Recurse into the NON-literal index expressions along a matched
/// chain's spine (they may themselves contain output paths).
fn collect_dynamic_index_subexprs(e: &Expr, out: &mut Vec<(String, Vec<Step>)>) {
    let mut cur = e;
    loop {
        match cur {
            Expr::Member { base, .. } => cur = base,
            Expr::Index { base, index } => {
                if !matches!(index.as_ref(), Expr::Lit(_)) {
                    collect_output_paths(index, out);
                }
                cur = base;
            }
            _ => return,
        }
    }
}

/// Try to view `e` as `tasks.<id>.output<steps…>`.
fn decompose(e: &Expr) -> Option<(String, Vec<Step>)> {
    let mut steps_rev: Vec<Step> = Vec::new();
    let mut cur = e;
    loop {
        match cur {
            Expr::Member { base, field } => {
                if field == "output"
                    && let Some(id) = task_root(base)
                {
                    steps_rev.reverse();
                    return Some((id.to_string(), steps_rev));
                }
                steps_rev.push(Step::Member(field.clone()));
                cur = base;
            }
            Expr::Index { base, index } => {
                // `tasks['id'].output` index-form roots are handled by
                // task_root on the Member("output") arm above — here we
                // are strictly inside the path suffix.
                let step = match index.as_ref() {
                    Expr::Lit(Literal::Int(i)) if *i >= 0 => {
                        Step::Index(u64::try_from(*i).unwrap_or(u64::MAX))
                    }
                    Expr::Lit(Literal::Str(s)) => Step::Member(s.clone()),
                    _ => Step::Dynamic,
                };
                steps_rev.push(step);
                cur = base;
            }
            _ => return None,
        }
    }
}

/// `tasks.<id>` / `tasks['id']` → the task id.
fn task_root(e: &Expr) -> Option<&str> {
    match e {
        Expr::Member { base, field } if matches!(base.as_ref(), Expr::Ident(r) if r == "tasks") => {
            Some(field)
        }
        Expr::Index { base, index } if matches!(base.as_ref(), Expr::Ident(r) if r == "tasks") => {
            if let Expr::Lit(Literal::Str(id)) = index.as_ref() {
                Some(id)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// The spec's provably-invalid walk (rules 1-3 · v0.1 subset · any
/// non-subset construct opens the level and stops the walk).
fn provably_invalid(schema: &Value, steps: &[Step]) -> Option<String> {
    const OPEN_KEYS: [&str; 7] = [
        "$ref",
        "oneOf",
        "anyOf",
        "allOf",
        "patternProperties",
        "not",
        "if",
    ];
    let mut level = schema;
    for step in steps {
        let Value::Object(map) = level else {
            return None;
        };
        if OPEN_KEYS.iter().any(|k| map.contains_key(*k)) {
            return None;
        }
        let ty = map.get("type");
        let type_excludes = |name: &str| -> bool {
            match ty {
                Some(Value::String(s)) => s != name,
                Some(Value::Array(a)) => !a.iter().any(|v| v.as_str() == Some(name)),
                _ => false,
            }
        };
        match step {
            Step::Member(key) => {
                if type_excludes("object") {
                    return Some(format!(
                        "member step `.{key}` on a level whose type excludes object"
                    ));
                }
                if let Some(Value::Object(props)) = map.get("properties")
                    && let Some(next) = props.get(key)
                {
                    level = next;
                    continue;
                }
                if map.get("additionalProperties") == Some(&Value::Bool(false)) {
                    return Some(format!(
                        "key `{key}` absent from a closed level (additionalProperties: false)"
                    ));
                }
                return None; // open level
            }
            Step::Index(i) => {
                if type_excludes("array") {
                    return Some(format!(
                        "index step `[{i}]` on a level whose type excludes array"
                    ));
                }
                if let Some(items @ Value::Object(_)) = map.get("items") {
                    level = items;
                    continue;
                }
                return None;
            }
            Step::Dynamic => return None,
        }
    }
    None
}

/// Render steps for the error message (`.entities[0].name`).
fn render_path(steps: &[Step]) -> String {
    let mut s = String::new();
    for step in steps {
        match step {
            Step::Member(k) => {
                let _ = write!(s, ".{k}");
            }
            Step::Index(i) => {
                let _ = write!(s, "[{i}]");
            }
            Step::Dynamic => s.push_str("[…]"),
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn closed_schema() -> Value {
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "entities": { "type": "array", "items": { "type": "string" } },
                "count": { "type": "integer" }
            }
        })
    }

    #[test]
    fn valid_property_path_passes() {
        let s = closed_schema();
        assert_eq!(
            provably_invalid(&s, &[Step::Member("entities".into())]),
            None
        );
    }

    #[test]
    fn missing_key_on_closed_level_is_invalid() {
        let s = closed_schema();
        let reason = provably_invalid(&s, &[Step::Member("entitties".into())]);
        assert!(reason.is_some_and(|r| r.contains("closed level")));
    }

    #[test]
    fn missing_key_on_open_level_passes() {
        let s = json!({ "type": "object", "properties": {} });
        assert_eq!(
            provably_invalid(&s, &[Step::Member("anything".into())]),
            None
        );
    }

    #[test]
    fn member_on_scalar_type_is_invalid() {
        let s = closed_schema();
        let reason = provably_invalid(
            &s,
            &[Step::Member("count".into()), Step::Member("value".into())],
        );
        assert!(reason.is_some_and(|r| r.contains("excludes object")));
    }

    #[test]
    fn index_on_object_type_is_invalid() {
        let s = closed_schema();
        let reason = provably_invalid(&s, &[Step::Index(0)]);
        assert!(reason.is_some_and(|r| r.contains("excludes array")));
    }

    #[test]
    fn non_subset_construct_opens_the_level() {
        let s = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": { "result": { "oneOf": [ { "type": "string" } ] } }
        });
        let steps = [Step::Member("result".into()), Step::Member("deep".into())];
        assert_eq!(provably_invalid(&s, &steps), None);
    }

    #[test]
    fn dynamic_step_stops_the_walk() {
        let s = closed_schema();
        let steps = [
            Step::Member("entities".into()),
            Step::Dynamic,
            Step::Member("whatever".into()),
        ];
        assert_eq!(provably_invalid(&s, &steps), None);
    }

    #[test]
    fn type_list_including_object_passes_member_step() {
        let s = json!({ "type": ["object", "null"], "properties": {} });
        assert_eq!(provably_invalid(&s, &[Step::Member("x".into())]), None);
    }

    #[test]
    fn render_path_shapes() {
        assert_eq!(
            render_path(&[Step::Member("a".into()), Step::Index(2), Step::Dynamic]),
            ".a[2][…]"
        );
    }
}
