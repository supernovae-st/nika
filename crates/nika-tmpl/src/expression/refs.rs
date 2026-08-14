// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Reference extraction + boolean-shape classification.
//!
//! `expr_refs` walks an expression and classifies every ROOT identifier
//! against the 5 namespaces + 2 loop-locals (spec `04-variables.md`
//! §Resolution order) — the analyzer's input for `NIKA-VAR-001`
//! (unresolved refs) and `NIKA-VAR-021` (`tasks.*` confined to the
//! boundary — `with:` bindings ARE the edges since W2 « the flow »).
//!
//! `is_boolean_shaped` is the static `when:` gate (spec `03-dag.md`
//! §when · « an engine MAY additionally reject statically-non-boolean-
//! shaped roots » · we DO · the spec NIKA-VAR-005 class).

use super::ast::{Expr, Literal, NamespaceRef};

/// Collect + classify every root reference in an expression.
///
/// Roots are returned in source order, including roots inside list
/// elements, `size()` arguments, index expressions and parentheses.
#[must_use]
pub fn expr_refs(expr: &Expr) -> Vec<NamespaceRef> {
    let mut out = Vec::new();
    walk_chains(expr, &mut |root, path| out.push(classify_root(root, path)));
    out
}

/// Every `tasks.<id>.output.<path…>` access chain with its FULL path
/// after `output` — the dataflow schema-typing surface (ADR-092 #4).
/// A bare `tasks.x.output` (no deeper path) yields an empty path; a
/// `status`/`error` field access yields nothing (not the output).
/// Numeric/dynamic index hops (`output[0]`) are not path segments —
/// the schema resolver descends `items` for them.
#[must_use]
pub fn task_output_paths(expr: &Expr) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    walk_chains(expr, &mut |root, path| {
        if root == "tasks"
            && path.len() >= 2
            && path[1] == "output"
            && let Some(id) = path.first()
        {
            out.push((id.clone(), path[2..].to_vec()));
        }
    });
    out
}

/// Every task referenced as a BARE envelope — `tasks.<id>` with no
/// field hop at all. The envelope (status · timestamps · output) is
/// legitimate plumbing in gates; bound into `outputs:` it is the
/// golden-drift trap the `envelope-output` hint teaches.
#[must_use]
pub fn bare_task_refs(expr: &Expr) -> Vec<String> {
    let mut out = Vec::new();
    walk_chains(expr, &mut |root, path| {
        if root == "tasks"
            && let [id] = path
        {
            out.push(id.clone());
        }
    });
    out
}

/// Every `with.<alias>.<path…>` access chain with its FULL path after
/// the alias — the aliased twin of [`task_output_paths`]: a `with:`
/// binding can hold a task's whole output, so a deep read THROUGH the
/// alias is the same dataflow hop as the direct deep reference (F3's
/// own repro binds `with.bill` then reads `with.bill.total_usd`).
#[must_use]
pub fn with_alias_paths(expr: &Expr) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    walk_chains(expr, &mut |root, path| {
        if root == "with"
            && path.len() >= 2
            && let Some(alias) = path.first()
        {
            out.push((alias.clone(), path[1..].to_vec()));
        }
    });
    out
}

/// Whether the expression ROOT is boolean-shaped (spec `03-dag.md`
/// §when valid/invalid lists) · `||` · `&&` · `!` · any relation · or a
/// bool literal. A bare path / non-bool literal root is NOT.
#[must_use]
pub fn is_boolean_shaped(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Or(_, _)
            | Expr::And(_, _)
            | Expr::Not(_)
            | Expr::Relation { .. }
            | Expr::Lit(Literal::Bool(_))
            // a ternary selecting values is accepted as a when:-shape ·
            // a non-boolean RESULT is a runtime error (NIKA-VAR-006)
            | Expr::Ternary { .. }
            // the presence macro + string predicates are boolean-valued
            | Expr::HasCall(_)
            | Expr::StringMethod { .. }
    )
}

/// Recursive walk · invoke `on_chain(root, path)` for every member/index
/// chain rooted at an identifier — the ONE chain-flattening core every
/// extractor ([`expr_refs`] · [`task_output_paths`]) shares, so they
/// cannot drift on the subtle rules (string-literal indices are path
/// segments per CEL · dynamic indices recurse as independent expressions).
fn walk_chains(expr: &Expr, on_chain: &mut dyn FnMut(&str, &[String])) {
    match expr {
        Expr::Or(lhs, rhs) | Expr::And(lhs, rhs) | Expr::Relation { lhs, rhs, .. } => {
            walk_chains(lhs, on_chain);
            walk_chains(rhs, on_chain);
        }
        Expr::Not(inner)
        | Expr::SizeCall(inner)
        | Expr::SizeMethod(inner)
        | Expr::HasCall(inner) => {
            walk_chains(inner, on_chain);
        }
        Expr::Ternary { cond, then, else_ } => {
            walk_chains(cond, on_chain);
            walk_chains(then, on_chain);
            walk_chains(else_, on_chain);
        }
        Expr::StringMethod { base, arg, .. } => {
            walk_chains(base, on_chain);
            walk_chains(arg, on_chain);
        }
        Expr::Member { .. } | Expr::Index { .. } | Expr::Ident(_) => {
            flatten_chain(expr, on_chain);
        }
        Expr::List(items) => {
            for item in items {
                walk_chains(item, on_chain);
            }
        }
        Expr::Lit(_) => {}
    }
}

/// Flatten a member/index access chain rooted at an identifier.
///
/// `tasks.build.status` → root `tasks` + path `[build, status]`. Index
/// segments with a STRING literal count as path segments
/// (`tasks['build'].status` ≡ `tasks.build.status` per CEL); dynamic
/// indices recurse as independent expressions.
fn flatten_chain(expr: &Expr, on_chain: &mut dyn FnMut(&str, &[String])) {
    let mut path: Vec<String> = Vec::new();
    let mut current = expr;
    loop {
        match current {
            Expr::Member { base, field } => {
                path.push(field.clone());
                current = base;
            }
            Expr::Index { base, index } => {
                if let Expr::Lit(Literal::Str(key)) = index.as_ref() {
                    path.push(key.clone());
                } else {
                    // Dynamic / numeric index · not a path segment ·
                    // its own roots still count.
                    walk_chains(index, on_chain);
                }
                current = base;
            }
            Expr::Ident(root) => {
                path.reverse();
                on_chain(root, &path);
                return;
            }
            // A chain rooted at a non-identifier (a list · a size()
            // call · a parenthesized relation) — no root to classify ·
            // recurse for inner roots.
            other => {
                walk_chains(other, on_chain);
                return;
            }
        }
    }
}

/// Map a root identifier + access path to a [`NamespaceRef`].
fn classify_root(root: &str, path: &[String]) -> NamespaceRef {
    let first = path.first().cloned();
    match root {
        "vars" => NamespaceRef::Vars(first.unwrap_or_default()),
        "inputs" => NamespaceRef::Inputs(first.unwrap_or_default()),
        "const" => NamespaceRef::Const(first.unwrap_or_default()),
        "with" => NamespaceRef::With(first.unwrap_or_default()),
        "env" => NamespaceRef::Env(first.unwrap_or_default()),
        "secrets" => NamespaceRef::Secrets(first.unwrap_or_default()),
        "tasks" => NamespaceRef::Tasks {
            id: first.unwrap_or_default(),
            field: path.get(1).cloned(),
        },
        "group" => NamespaceRef::Group(first.unwrap_or_default()),
        "item" => NamespaceRef::Item,
        "index" => NamespaceRef::Index,
        other => NamespaceRef::Unknown(other.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expression::parser::parse_expression;

    fn refs(src: &str) -> Vec<NamespaceRef> {
        expr_refs(&parse_expression(src).expect("parse"))
    }

    #[test]
    fn classify_the_five_namespaces() {
        assert_eq!(refs("vars.topic"), vec![NamespaceRef::Vars("topic".into())]);
        assert_eq!(
            refs("with.content"),
            vec![NamespaceRef::With("content".into())]
        );
        assert_eq!(refs("env.HOME"), vec![NamespaceRef::Env("HOME".into())]);
        assert_eq!(
            refs("secrets.api_key"),
            vec![NamespaceRef::Secrets("api_key".into())]
        );
        assert_eq!(
            refs("tasks.build.status"),
            vec![NamespaceRef::Tasks {
                id: "build".into(),
                field: Some("status".into()),
            }]
        );
    }

    #[test]
    fn bare_task_refs_finds_envelopes_only() {
        let bare = |src: &str| bare_task_refs(&parse_expression(src).expect("parse"));
        assert_eq!(bare("tasks.brief"), vec!["brief".to_owned()]);
        // Any field hop — output, status, deep — is NOT a bare envelope.
        assert!(bare("tasks.brief.output").is_empty());
        assert!(bare("tasks.brief.status").is_empty());
        assert!(bare("tasks.brief.output.title").is_empty());
        // Other namespaces never match.
        assert!(bare("vars.brief").is_empty());
    }

    #[test]
    fn classify_loop_locals() {
        assert_eq!(refs("item"), vec![NamespaceRef::Item]);
        assert_eq!(refs("index"), vec![NamespaceRef::Index]);
        assert_eq!(refs("item.url"), vec![NamespaceRef::Item]);
    }

    #[test]
    fn classify_unknown_root() {
        assert_eq!(
            refs("ghost.field"),
            vec![NamespaceRef::Unknown("ghost".into())]
        );
        assert_eq!(
            refs("obj['key-with-dash']"),
            vec![NamespaceRef::Unknown("obj".into())]
        );
    }

    #[test]
    fn task_ref_without_field() {
        assert_eq!(
            refs("tasks.build"),
            vec![NamespaceRef::Tasks {
                id: "build".into(),
                field: None,
            }]
        );
    }

    #[test]
    fn task_ref_via_string_index() {
        // tasks['build'].status ≡ tasks.build.status per CEL.
        assert_eq!(
            refs("tasks['build'].status"),
            vec![NamespaceRef::Tasks {
                id: "build".into(),
                field: Some("status".into()),
            }]
        );
    }

    #[test]
    fn refs_inside_compound_expressions() {
        assert_eq!(
            refs("tasks.a.status == 'success' && tasks.b.status == 'success'"),
            vec![
                NamespaceRef::Tasks {
                    id: "a".into(),
                    field: Some("status".into()),
                },
                NamespaceRef::Tasks {
                    id: "b".into(),
                    field: Some("status".into()),
                },
            ]
        );
    }

    #[test]
    fn refs_inside_size_list_and_index() {
        assert_eq!(
            refs("size(vars.items) > 0"),
            vec![NamespaceRef::Vars("items".into())]
        );
        assert_eq!(
            refs("tasks.deploy.status in ['success', 'skipped']"),
            vec![NamespaceRef::Tasks {
                id: "deploy".into(),
                field: Some("status".into()),
            }]
        );
        // Dynamic index roots count too.
        assert_eq!(
            refs("vars.list[index]"),
            vec![NamespaceRef::Index, NamespaceRef::Vars("list".into())]
        );
    }

    #[test]
    fn numeric_index_is_not_a_path_segment() {
        assert_eq!(
            refs("tasks.list.output[0]"),
            vec![NamespaceRef::Tasks {
                id: "list".into(),
                field: Some("output".into()),
            }]
        );
    }

    #[test]
    fn no_refs_in_pure_literals() {
        assert!(refs("true").is_empty());
        assert!(refs("42 > 0").is_empty());
        assert!(refs("'a' in ['a', 'b']").is_empty());
    }

    // ── Boolean shape (spec §when valid/invalid lists) ──────────────

    #[test]
    fn spec_when_valid_examples_are_boolean_shaped() {
        for src in [
            "vars.env == \"production\"",
            "tasks.upstream.status == \"success\"",
            "tasks.scan.alerts.size() > 0",
            "vars.dry_run == false && tasks.check.passed",
            "tasks.X.output != null",
            "!(tasks.test.status == 'failure')",
            "tasks.deploy.status in ['success', 'skipped']",
            "true",
        ] {
            let e = parse_expression(src).expect("parse");
            assert!(is_boolean_shaped(&e), "should be boolean-shaped: {src}");
        }
    }

    #[test]
    fn spec_when_invalid_examples_are_not_boolean_shaped() {
        // 03-dag.md §when invalid · « returns integer/object/string ·
        // not bool ».
        for src in [
            "vars.threshold",
            "tasks.X.output",
            "vars.message",
            "42",
            "'literal'",
            "null",
            "size(vars.items)",
            "vars.items.size()",
            "[1, 2]",
        ] {
            let e = parse_expression(src).expect("parse");
            assert!(!is_boolean_shaped(&e), "must NOT be boolean-shaped: {src}");
        }
    }
}
