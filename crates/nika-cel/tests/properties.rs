// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Property battery (spec §5.5) — the totality + determinism floor.
//!
//! `parse` and `compute` are PURE and TOTAL: on ANY input (including
//! adversarial bytes) they return `Ok`/`Err`, never panic, never loop
//! (the grammar is non-recursive-descent-unbounded only through `(...)`
//! nesting, which proptest's bounded depth exercises). These properties
//! are the structural proof of "no host can crash the engine with a
//! crafted `${{ }}`".

#![allow(clippy::unwrap_used, clippy::expect_used)]

use nika_cel::{Resolver, compute, compute_bool, parse};
use proptest::prelude::*;
use serde_json::{Value, json};

/// A total resolver — every root resolves to a small mixed namespace, so
/// `compute` exercises object/list/scalar navigation paths.
struct AnyNs;
impl Resolver for AnyNs {
    fn resolve_root(&self, name: &str) -> Option<Value> {
        match name {
            "vars" => Some(json!({ "a": "x", "n": 3, "t": ["p", "q"], "nil": null })),
            "tasks" => Some(json!({ "build": { "status": "success", "output": null } })),
            "item" => Some(json!("elem")),
            "index" => Some(json!(0)),
            _ => None,
        }
    }
}

proptest! {
    /// `parse` is total over arbitrary unicode — never panics.
    #[test]
    fn parse_never_panics_on_arbitrary_input(s in ".*") {
        let _ = parse(&s);
    }

    /// `parse` is total over arbitrary ASCII-ish operator soup (denser in
    /// the grammar's own alphabet · stresses the lexer/parser boundary).
    #[test]
    fn parse_never_panics_on_operator_soup(
        s in proptest::collection::vec(
            prop::sample::select(vec![
                "vars", ".", "a", "==", "!=", "<", ">=", "&&", "||", "!",
                "(", ")", "[", "]", "'x'", "42", "1.5", "true", "null",
                "in", "size", "has", "?", ":", ",", " ", "==", "n",
            ]),
            0..24,
        ).prop_map(|v| v.join(""))
    ) {
        let _ = parse(&s);
    }

    /// `parse` is deterministic — same source, same AST (the "zero parser
    /// drift" guarantee starts with self-consistency).
    #[test]
    fn parse_is_deterministic(s in ".*") {
        prop_assert_eq!(parse(&s).is_ok(), parse(&s).is_ok());
        if let (Ok(a), Ok(b)) = (parse(&s), parse(&s)) {
            prop_assert_eq!(a, b);
        }
    }

    /// `compute` is total — any expression that PARSES computes to a
    /// value or a typed error, never panics, over a total resolver.
    #[test]
    fn compute_never_panics_on_parseable(s in ".*") {
        if let Ok(expr) = parse(&s) {
            let _ = compute(&expr, &AnyNs);
            let _ = compute_bool(&expr, &AnyNs);
        }
    }

    /// The canonical worked forms (03-dag) parse AND compute without
    /// panic — the generated-from-fragments stability floor.
    #[test]
    fn canonical_forms_parse_and_compute(
        lhs in prop::sample::select(vec!["vars.a", "vars.n", "tasks.build.status", "item", "size(vars.t)"]),
        op in prop::sample::select(vec!["==", "!=", "<", ">", "<=", ">="]),
        rhs in prop::sample::select(vec!["'x'", "3", "0", "null", "vars.n"]),
    ) {
        let src = format!("{lhs} {op} {rhs}");
        // Must lex+parse (well-formed by construction); compute may be a
        // typed error (e.g. cross-type) but must not panic.
        let expr = parse(&src).expect("well-formed expression parses");
        let _ = compute(&expr, &AnyNs);
        // A relation is boolean-shaped (the checker's static gate).
        prop_assert!(expr.is_boolean_shaped());
    }

    /// `has(root.field)` — the presence probe over a simple field path —
    /// never errors: a missing root or field is `false`, not a raise. This
    /// covers the `root.field` SUBSET of `has()` only; a VAR-006 nested
    /// inside `has()` (e.g. `has(size(scalar))`) DOES surface by design —
    /// see the `has_swallows_unresolved_but_propagates_type_errors` unit
    /// test. `has` converts the *presence* class, not the *type* class
    /// (spec §3).
    #[test]
    fn has_is_always_total(
        root in prop::sample::select(vec!["vars", "tasks", "item", "ghost"]),
        field in prop::sample::select(vec!["a", "n", "nil", "missing", "output"]),
    ) {
        let src = format!("has({root}.{field})");
        let expr = parse(&src).expect("has(...) parses");
        let got = compute_bool(&expr, &AnyNs);
        prop_assert!(got.is_ok(), "has() must never raise · got {got:?}");
    }
}
