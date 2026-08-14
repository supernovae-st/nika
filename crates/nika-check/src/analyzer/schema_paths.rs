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
//! 1. a member step on a level that DECLARES `properties` which omit
//!    the key, or declares `additionalProperties: false`;
//! 2. a member step on a level whose `type` excludes `object`;
//! 3. an index step on a level whose `type` excludes `array`.
//!
//! Rule 1's first half is the operator lock of 2026-07-30 (spec 04
//! rewritten · conformance `runner-protocol.md` class D): declaring
//! `properties:` CLOSES a level for binding, because a structured
//! output `schema:` compiles strict and the misspelled-key class is
//! the whole point of the walk. `additionalProperties: true` (or a
//! schema object) reopens it explicitly; a level that declares
//! nothing stays opaque and nothing beneath it is rejected. Before
//! the lock this walk waited for an explicit `additionalProperties:
//! false`, which is why the check-side strict-binding rung had to
//! refuse CODELESS beside it — two voices for one law. One voice now.
//!
//! The walk covers the v0.1 subset (`properties` · `items` · `type` ·
//! `additionalProperties`) — any other construct (`$ref` · `oneOf` ·
//! `anyOf` · `allOf` · `patternProperties` · `not` · `if`) makes the
//! level OPEN and the walk stops (nothing beneath is rejected).

use std::fmt::Write as _;

use serde_json::Value;

use nika_schema::error::SchemaError;
use nika_schema::expression::{Expr, Literal};
use nika_schema::source::Span;
use nika_types::suggest::{did_you_mean, suggestion_clause};

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
        Expr::Not(a) | Expr::SizeCall(a) | Expr::SizeMethod(a) | Expr::HasCall(a) => {
            collect_output_paths(a, out);
        }
        Expr::Ternary { cond, then, else_ } => {
            collect_output_paths(cond, out);
            collect_output_paths(then, out);
            collect_output_paths(else_, out);
        }
        Expr::StringMethod { base, arg, .. } => {
            collect_output_paths(base, out);
            collect_output_paths(arg, out);
        }
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
                let props = map.get("properties").and_then(Value::as_object);
                if let Some(next) = props.and_then(|p| p.get(key)) {
                    level = next;
                    continue;
                }
                // The level does not declare `key`. Only an EXPLICIT
                // opt-out reopens it.
                if matches!(
                    map.get("additionalProperties"),
                    Some(Value::Bool(true) | Value::Object(_))
                ) {
                    return None;
                }
                // The misspelled-key class is the reason this walk
                // exists, so it carries the suggestion the check-side
                // rung used to be kept around for.
                let hint = |p: &serde_json::Map<String, Value>| {
                    suggestion_clause(did_you_mean(key, p.keys().map(String::as_str)))
                };
                if map.get("additionalProperties") == Some(&Value::Bool(false)) {
                    return Some(format!(
                        "key `{key}` absent from a closed level \
                         (additionalProperties: false){}",
                        props.map(hint).unwrap_or_default()
                    ));
                }
                if let Some(props) = props {
                    return Some(format!(
                        "key `{key}` absent from a level that declares its properties \
                         [{}]{} — declaring `properties:` closes a level for binding · \
                         `additionalProperties: true` reopens it",
                        props.keys().cloned().collect::<Vec<_>>().join(", "),
                        hint(props)
                    ));
                }
                return None; // declares nothing here · genuinely opaque
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
    use nika_schema::expression::parse_expression;
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
        // Spec 04 · « a level that declares NO `properties:` at all (a
        // bare `type: object`) is open: nothing is declared, so nothing
        // can be contradicted ». This is the soundness half of the
        // 2026-07-30 lock and it must survive it.
        let s = json!({ "type": "object" });
        assert_eq!(
            provably_invalid(&s, &[Step::Member("anything".into())]),
            None
        );
    }

    #[test]
    fn an_empty_properties_map_still_closes_the_level() {
        // The corner the lock decides against the pre-lock reading:
        // `properties: {}` DECLARES properties and lists nothing, so
        // every key is outside the declaration (spec 04 rule 1 · « the
        // declared empty object »). Before the lock this passed.
        let s = json!({ "type": "object", "properties": {} });
        let reason = provably_invalid(&s, &[Step::Member("anything".into())]);
        assert!(
            reason.is_some_and(|r| r.contains("declares its properties")),
            "an empty declaration is still a declaration"
        );
    }

    #[test]
    fn explicit_false_without_properties_closes_the_level() {
        // The lock's OTHER closed corner (spec 04 rule 1's parenthetical ·
        // « `additionalProperties: false` on a level with no `properties:`
        // also closes it — the declared empty object »). With nothing
        // declared there is no sibling to suggest, so the refusal carries
        // no did-you-mean — the suggestion clause must stay silent rather
        // than invent a candidate.
        let s = json!({ "type": "object", "additionalProperties": false });
        let reason = provably_invalid(&s, &[Step::Member("anything".into())]);
        assert!(
            reason
                .as_ref()
                .is_some_and(|r| r.contains("closed level") && !r.contains("did you mean")),
            "explicit false without properties refuses, suggestion-free: {reason:?}"
        );
    }

    #[test]
    fn an_undeclared_sibling_read_lists_the_declaration_and_the_near_miss() {
        // The misspelled-key class the walk exists for. The refusal owes
        // TWO distinct things, and they come from two different places —
        // so both are pinned: the ENUMERATION of what the level declares
        // (`[entities, summary]` · the reader fixes it without opening
        // the schema) and the near-miss SUGGESTION. Asserting only that
        // the message mentions `entities` would pass on an empty
        // enumeration, since the suggestion clause names it too
        // (mutation-checked: emptying the list must turn this red).
        let s = json!({
            "type": "object",
            "properties": {
                "entities": { "type": "array" },
                "summary": { "type": "string" }
            }
        });
        let reason = provably_invalid(&s, &[Step::Member("entitties".into())]);
        assert!(
            reason.as_ref().is_some_and(
                |r| r.contains("[entities, summary]") && r.contains("did you mean `entities`?")
            ),
            "the refusal enumerates the declaration AND offers the near-miss: {reason:?}"
        );
    }

    #[test]
    fn explicit_additional_properties_reopens_a_declared_level() {
        // The one-line fix the message prescribes has to actually work.
        let s = json!({
            "type": "object",
            "additionalProperties": true,
            "properties": { "entities": { "type": "array" } }
        });
        assert_eq!(
            provably_invalid(&s, &[Step::Member("maybe_extra".into())]),
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
        // What this guards is the TYPE-LIST arm — `["object","null"]`
        // does not exclude object, so rule 2 must not fire. The key is
        // declared on purpose: since the 2026-07-30 lock an undeclared
        // key would refuse under rule 1 and the test would pass for the
        // wrong reason (it used to lean on `properties: {}` being open).
        let s = json!({ "type": ["object", "null"], "properties": { "x": { "type": "string" } } });
        assert_eq!(provably_invalid(&s, &[Step::Member("x".into())]), None);
    }

    #[test]
    fn render_path_shapes() {
        assert_eq!(
            render_path(&[Step::Member("a".into()), Step::Index(2), Step::Dynamic]),
            ".a[2][…]"
        );
    }

    // The collection pipeline (collect_output_paths · collect_dynamic_index_subexprs)
    // was untested — only provably_invalid was. These drive it directly.

    #[test]
    fn collect_gathers_the_output_path_chain() {
        let e = parse_expression("tasks.foo.output.entities[0]").expect("parse");
        let mut out = Vec::new();
        collect_output_paths(&e, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, "foo");
        assert_eq!(
            out[0].1,
            vec![Step::Member("entities".into()), Step::Index(0)]
        );
    }

    #[test]
    fn collect_skips_a_bare_output_with_no_steps() {
        let e = parse_expression("tasks.foo.output").expect("parse");
        let mut out = Vec::new();
        collect_output_paths(&e, &mut out);
        assert!(
            out.is_empty(),
            "bare .output (no steps) must not be collected"
        );
    }

    #[test]
    fn collect_recurses_into_a_dynamic_index_subexpr() {
        // Kills `collect_dynamic_index_subexprs` 120 — delete the
        // `Expr::Member { base, .. }` spine arm.
        //
        // The dynamic index `[tasks.b.output.idx]` sits BEHIND a trailing
        // `.name` Member: the outermost node of the whole expression is the
        // `.name` Member, NOT the Index. So the spine walk must descend
        // THROUGH the Member arm (`cur = base`) to even REACH the Index node
        // that carries `tasks.b`. Deleting the Member arm makes the walk
        // bottom out at `_ => return` on the very first node (`.name`),
        // before the Index → `tasks.b` is never collected. (The round-1
        // input `…items[…]` had the Index as the OUTERMOST node, so the
        // Index arm fired first and the Member arm was never load-bearing —
        // it could be deleted with the chain still collected. The trailing
        // `.name` is what forces the Member walk.)
        let e = parse_expression("tasks.a.output.items[tasks.b.output.idx].name").expect("parse");
        let mut out = Vec::new();
        collect_output_paths(&e, &mut out);
        let ids: std::collections::BTreeSet<&str> = out.iter().map(|(i, _)| i.as_str()).collect();
        assert!(
            ids.contains("a"),
            "the outer chain `tasks.a.output…` is collected, got {ids:?}"
        );
        assert!(
            ids.contains("b"),
            "the dynamic index behind the trailing `.name` Member must be \
             reached by the spine walk (Member arm load-bearing), got {ids:?}"
        );
    }

    // ── decompose · the `tasks.<id>.output<suffix>` view ────────────────

    #[test]
    fn decompose_yields_the_id_and_member_suffix() {
        // The happy path · kills `decompose -> None` / `Some(("",[]))` /
        // `Some(("xyzzy",[]))` (134:5) AND the `field=="output"` arm
        // (138:13) AND the `field=="output"` compare (139:26 `==`→`!=`).
        let e = parse_expression("tasks.foo.output.entities").expect("parse");
        assert_eq!(
            decompose(&e),
            Some(("foo".to_owned(), vec![Step::Member("entities".into())])),
        );
    }

    #[test]
    fn decompose_handles_an_index_suffix() {
        // An `[…]` suffix step · kills the `Expr::Index` arm (148:13) AND
        // confirms a literal int index becomes `Step::Index` (the `*i >= 0`
        // guard true-branch · 153:51 / 153:54).
        let e = parse_expression("tasks.foo.output.rows[3]").expect("parse");
        assert_eq!(
            decompose(&e),
            Some((
                "foo".to_owned(),
                vec![Step::Member("rows".into()), Step::Index(3)],
            )),
        );
    }

    #[test]
    fn decompose_string_index_is_a_member_step() {
        // `['key']` → `Step::Member` · kills the
        // `Expr::Lit(Literal::Str(s))` index arm (156:21).
        let e = parse_expression("tasks.foo.output['key']").expect("parse");
        assert_eq!(
            decompose(&e),
            Some(("foo".to_owned(), vec![Step::Member("key".into())])),
        );
    }

    #[test]
    fn decompose_negative_index_is_dynamic_not_index() {
        // A NEGATIVE literal index fails the `*i >= 0` guard (153:51) and
        // falls through to `Step::Dynamic` — NOT `Step::Index`. Kills the
        // guard mutants `*i >= 0` → true (would keep it an Index) and
        // `>=` → `<` (a positive int would then become Dynamic).
        let e = parse_expression("tasks.foo.output.rows[-1]").expect("parse");
        assert_eq!(
            decompose(&e),
            Some((
                "foo".to_owned(),
                vec![Step::Member("rows".into()), Step::Dynamic],
            )),
        );
    }

    #[test]
    fn decompose_rejects_a_non_tasks_root() {
        // No `tasks.<id>.output` spine → `None`. A `vars.` chain has no
        // output root, so the loop bottoms out at `_ => return None` —
        // the `decompose -> Some(("xyzzy",[]))` / `Some(("",[]))` mutants
        // would wrongly return `Some` here.
        let e = parse_expression("vars.topic.entities").expect("parse");
        assert_eq!(decompose(&e), None);
    }

    #[test]
    fn decompose_member_arm_is_load_bearing() {
        // Deleting the `Expr::Member` arm (138:13) would stop the walk on
        // the FIRST member, so a deeper suffix (`.a.b`) past `.output`
        // could not be peeled. A two-deep member suffix proves the arm
        // recurses through member steps.
        let e = parse_expression("tasks.foo.output.a.b").expect("parse");
        assert_eq!(
            decompose(&e),
            Some((
                "foo".to_owned(),
                vec![Step::Member("a".into()), Step::Member("b".into())],
            )),
        );
    }

    // ── task_root · the `tasks.<id>` / `tasks['id']` root ───────────────

    #[test]
    fn task_root_reads_the_dotted_and_indexed_forms() {
        // `tasks.foo` (Member arm · 170:41) and `tasks['foo']` (Index arm ·
        // 173:40) both resolve to the id `foo`. The full `decompose`
        // covers `task_root -> None` (169:5) implicitly, but assert the
        // index-form root end-to-end here.
        let dotted = parse_expression("tasks.foo.output.x").expect("parse");
        assert_eq!(
            decompose(&dotted),
            Some(("foo".to_owned(), vec![Step::Member("x".into())])),
        );
        let indexed = parse_expression("tasks['foo'].output.x").expect("parse");
        assert_eq!(
            decompose(&indexed),
            Some(("foo".to_owned(), vec![Step::Member("x".into())])),
        );
    }

    #[test]
    fn task_root_rejects_a_non_tasks_index_root() {
        // `vars['foo'].output` is NOT a task root — the Index-arm guard
        // `matches!(base, Ident("tasks"))` (173:40) must reject the `vars`
        // base, so `decompose` returns `None`. Forcing the guard `true`
        // would wrongly accept `vars` and yield `Some(("foo", …))`.
        let e = parse_expression("vars['foo'].output.x").expect("parse");
        assert_eq!(decompose(&e), None);
    }

    #[test]
    fn task_root_rejects_a_non_tasks_member_root() {
        // `a.b.output.x` reaches `task_root` with a MEMBER base
        // (`a.b`) whose own base is `Ident("a")` ≠ `tasks` — the Member-arm
        // guard `matches!(base, Ident("tasks"))` (170:41) rejects it.
        // Forcing the guard `true` would treat `b` as the task id and
        // return `Some(("b", …))`; the correct result is `None`.
        let e = parse_expression("a.b.output.x").expect("parse");
        assert_eq!(decompose(&e), None);
    }

    // ── collect_dynamic_index_subexprs · the `!matches!(index, Lit)` guard

    #[test]
    fn dynamic_index_recursed_literal_index_skipped() {
        // The `!` in `!matches!(index, Lit)` (122:20) gates recursion ONLY
        // into NON-literal spine indices. A literal `[0]` spine index must
        // NOT be recursed (deleting `!` would flip the gate). Build the two
        // spines directly so the guard is exercised in isolation.
        //
        // Dynamic index carrying an output path → recursed → "b" collected.
        let dynamic = parse_expression("tasks.a.output[tasks.b.output.idx]").expect("parse");
        let mut out = Vec::new();
        collect_dynamic_index_subexprs(&dynamic, &mut out);
        let ids: std::collections::BTreeSet<&str> = out.iter().map(|(i, _)| i.as_str()).collect();
        assert!(ids.contains("b"), "dynamic index recursed, got {ids:?}");

        // Literal index `[0]` → NOT recursed (a literal can hold no output
        // path; the guard must keep this branch silent regardless).
        let literal = Expr::Index {
            base: Box::new(Expr::Member {
                base: Box::new(Expr::Member {
                    base: Box::new(Expr::Ident("tasks".into())),
                    field: "a".into(),
                }),
                field: "output".into(),
            }),
            index: Box::new(Expr::Lit(Literal::Int(0))),
        };
        let mut out_lit = Vec::new();
        collect_dynamic_index_subexprs(&literal, &mut out_lit);
        assert!(
            out_lit.is_empty(),
            "a literal spine index is never recursed, got {out_lit:?}"
        );
    }

    // ── provably_invalid · the array `type:` check on an index step ─────

    #[test]
    fn index_on_array_typed_level_descends_into_items() {
        // The `Some(Value::Array(a))` type-list arm (208:17) + its
        // `as_str() == Some(name)` compare (208:71 `==`→`!=`): a level
        // typed `["array","null"]` does NOT exclude array, so an index
        // step is allowed and descends into `items` (here a scalar → a
        // FOLLOWING member step is then provably invalid).
        let s = json!({
            "type": ["array", "null"],
            "items": { "type": "string" }
        });
        // Index alone on an array level is valid …
        assert_eq!(provably_invalid(&s, &[Step::Index(0)]), None);
        // … and descends into `items` (a string) so a member past it is
        // provably invalid — proving the array arm let the walk continue.
        let reason = provably_invalid(&s, &[Step::Index(0), Step::Member("x".into())]);
        assert!(
            reason.is_some_and(|r| r.contains("excludes object")),
            "index descended into items, then member on a string is invalid"
        );
    }

    #[test]
    fn index_on_type_list_excluding_array_is_invalid() {
        // A `type:` list that does NOT contain "array" excludes it — the
        // `Some(Value::Array(a))` arm (208:17) returns the exclusion, and
        // `==`→`!=` (208:71) would invert the membership test.
        let s = json!({ "type": ["object", "string"] });
        let reason = provably_invalid(&s, &[Step::Index(0)]);
        assert!(
            reason.is_some_and(|r| r.contains("excludes array")),
            "an index on a non-array type list is provably invalid"
        );
    }

    // ── end-to-end · the analyzer wires check_expr into the scan ────────

    #[test]
    fn analyze_flags_a_provably_invalid_output_path() {
        // Drives the PUBLIC path `parse → analyze` so `check_expr` (50:5)
        // is reached through the real scan pipeline — replacing its body
        // with `()` makes the OutputPathProvablyInvalid finding vanish.
        // The producer declares a closed schema (`additionalProperties:
        // false` · `properties` omits `nope`); the consumer imports
        // `tasks.prod.output.nope` through `with:` (the binding is the
        // edge — and the boundary is where the path is judged).
        let yaml = "\
nika: t
tasks:
  prod:
    infer:
      prompt: \"produce\"
      schema:
        type: object
        additionalProperties: false
        properties:
          entities: { type: array }
  cons:
    with: { x: \"${{ tasks.prod.output.nope }}\" }
    infer:
      prompt: \"use ${{ with.x }}\"
";
        let wf = nika_schema::parser::parse(
            yaml,
            nika_schema::source::FileId::new(0),
            nika_schema::parser::ParseMode::Strict,
        )
        .expect("parse");
        let errors = crate::analyzer::analyze(&wf).expect_err("provably-invalid path");
        assert!(
            errors.iter().any(|e| matches!(
                e,
                SchemaError::OutputPathProvablyInvalid { task, .. } if task == "prod"
            )),
            "expected an OutputPathProvablyInvalid for `prod`, got {errors:?}"
        );
    }
}
