// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The io-declaration `TypeExpr` helpers (spec `09-types.md` · R3b ·
//! LAW-GRAMMAR-0211) — the ONE compact rendering and the launch-seam
//! coercion, shared by the checker, the runtime, the CLI and the LSP
//! (never re-implemented per surface). Both ride the one type core
//! (`nika_types::types`) — no second grammar, no second fit.

use std::collections::{BTreeMap, BTreeSet};

use nika_types::types::{NikaType, Primitive, fits, parse_type};
use serde_json::Value;

/// The compact one-line rendering of a raw `TypeExpr` — a scalar name
/// rides bare (`integer`), every other form is its compact JSON
/// (`{"enum":["fast","slow"]}` · `null`). Diagnostics only — the
/// rendering is never re-parsed.
#[must_use]
pub fn type_expr_display(expr: &Value) -> String {
    match expr {
        Value::String(s) => s.clone(),
        // `Value`'s Display IS the compact JSON form (no spacing).
        other => other.to_string(),
    }
}

/// The `NIKA-DEFAULT-001` teaching (R3b · LAW-TYPE-0211 · the engine
/// twin of `values_core.py::_default_errors`) — what was declared, what
/// does not fit, why the hole is closed. One voice: every surface (the
/// check report · the CLI · the LSP) renders THIS text.
#[must_use]
pub fn default_not_conforming_teaching(where_: &str, value: &Value, type_expr: &Value) -> String {
    format!(
        "{where_} · the value {value} does not conform to its declared type `{}` \
         (the P0 soundness hole — a value that passes check and fails at run — \
         is closed · R3b · LAW-TYPE-0211)",
        type_expr_display(type_expr),
    )
}

/// The `--var KEY=VALUE` coercion (the launch seam · spec 01 §inputs
/// « supplying values at launch ») — the declared `TypeExpr` DRIVES the
/// parse: a string-shaped type (the `string` primitive · the string
/// newtypes) takes the raw text verbatim (`--var name=5` is the string
/// `"5"`, never the number), every other type takes the JSON-or-string
/// guess (`--var limit=5` the number · `--var deep=true` the boolean ·
/// else a string). The ONE fit then judges the value — a misfit refuses
/// with the declared form and the offending text.
///
/// An expression that does not parse yields the untyped guess: the
/// grammar refusal is the checker's (`NIKA-TYPE-001` · audit-before-run
/// guarantees a broken declaration never reaches the launch seam, which
/// stays total).
///
/// # Errors
///
/// A one-line message naming the declared type + the offending value,
/// ready for the CLI's `--var <key>: …` frame.
pub fn coerce_declared(
    type_expr: &Value,
    type_names: &BTreeSet<String>,
    named: &BTreeMap<String, NikaType>,
    raw: &str,
) -> Result<Value, String> {
    let guess =
        || serde_json::from_str::<Value>(raw).unwrap_or_else(|_| Value::String(raw.to_owned()));
    let Ok(ty) = parse_type(type_expr, type_names, "inputs") else {
        return Ok(guess());
    };
    let value = match &ty {
        NikaType::Prim(p) if *p == Primitive::String || p.narrows_string() => {
            Value::String(raw.to_owned())
        }
        _ => guess(),
    };
    if fits(&value, &ty, named) {
        Ok(value)
    } else {
        Err(format!(
            "expects `{}`, got `{raw}`",
            type_expr_display(type_expr)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn names(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn env(pairs: &[(&str, NikaType)]) -> BTreeMap<String, NikaType> {
        pairs
            .iter()
            .map(|(k, t)| ((*k).to_owned(), t.clone()))
            .collect()
    }

    #[test]
    fn display_bare_scalars_and_compact_composites() {
        assert_eq!(type_expr_display(&json!("integer")), "integer");
        assert_eq!(type_expr_display(&json!("bool")), "bool");
        assert_eq!(type_expr_display(&Value::Null), "null");
        assert_eq!(
            type_expr_display(&json!({ "enum": ["fast", "slow"] })),
            "{\"enum\":[\"fast\",\"slow\"]}"
        );
        assert_eq!(
            type_expr_display(&json!({ "array": "string" })),
            "{\"array\":\"string\"}"
        );
    }

    #[test]
    fn default_001_teaching_names_place_value_type_and_law() {
        let msg = default_not_conforming_teaching(
            "inputs.count.default",
            &json!("abc"),
            &json!("integer"),
        );
        assert!(msg.contains("inputs.count.default"), "{msg}");
        assert!(
            msg.contains("\"abc\""),
            "the value rides compact JSON: {msg}"
        );
        assert!(msg.contains("`integer`"), "the declared type: {msg}");
        assert!(msg.contains("P0 soundness hole"), "the why: {msg}");
        assert!(msg.contains("LAW-TYPE-0211"), "the law: {msg}");
    }

    #[test]
    fn string_shaped_types_take_the_raw_text_verbatim() {
        let n = names(&[]);
        let e = env(&[]);
        // string · the raw text never JSON-coerces (`5` stays "5").
        assert_eq!(
            coerce_declared(&json!("string"), &n, &e, "5").unwrap(),
            json!("5")
        );
        assert_eq!(
            coerce_declared(&json!("string"), &n, &e, "hi there").unwrap(),
            json!("hi there")
        );
        // the string newtypes ride the same verbatim lane.
        assert_eq!(
            coerce_declared(&json!("uri"), &n, &e, "https://x.io").unwrap(),
            json!("https://x.io")
        );
    }

    #[test]
    fn scalar_types_parse_their_json_form_and_refuse_the_rest() {
        let n = names(&[]);
        let e = env(&[]);
        assert_eq!(
            coerce_declared(&json!("integer"), &n, &e, "42").unwrap(),
            json!(42)
        );
        assert_eq!(
            coerce_declared(&json!("number"), &n, &e, "2.5").unwrap(),
            json!(2.5)
        );
        assert_eq!(
            coerce_declared(&json!("bool"), &n, &e, "true").unwrap(),
            json!(true)
        );
        // a mismatch names the declared form + the offending text.
        let err = coerce_declared(&json!("integer"), &n, &e, "nope").unwrap_err();
        assert!(err.contains("integer") && err.contains("nope"), "{err}");
        let err = coerce_declared(&json!("bool"), &n, &e, "maybe").unwrap_err();
        assert!(err.contains("bool") && err.contains("maybe"), "{err}");
    }

    #[test]
    fn composites_enums_unions_and_named_refs_fit() {
        let n = names(&["Mode"]);
        let e = env(&[("Mode", NikaType::Enum(vec!["fast".to_owned()]))]);
        assert_eq!(
            coerce_declared(&json!({ "array": "string" }), &n, &e, "[\"x\",\"y\"]").unwrap(),
            json!(["x", "y"])
        );
        assert!(
            coerce_declared(&json!({ "array": "string" }), &n, &e, "[1]").is_err(),
            "an element misfit refuses"
        );
        assert_eq!(
            coerce_declared(&json!({ "enum": ["fast", "slow"] }), &n, &e, "fast").unwrap(),
            json!("fast")
        );
        assert!(coerce_declared(&json!({ "enum": ["fast"] }), &n, &e, "medium").is_err());
        assert_eq!(
            coerce_declared(&json!({ "union": ["string", "integer"] }), &n, &e, "5").unwrap(),
            json!(5)
        );
        assert_eq!(
            coerce_declared(&json!("Mode"), &n, &e, "fast").unwrap(),
            json!("fast"),
            "a named reference resolves through the env"
        );
    }

    #[test]
    fn an_unparsable_expression_falls_back_to_the_guess() {
        // The grammar refusal is the checker's — the launch seam stays
        // total (audit-before-run keeps this arm out of a checked run).
        let n = names(&[]);
        let e = env(&[]);
        assert_eq!(
            coerce_declared(&json!("boolean"), &n, &e, "true").unwrap(),
            json!(true),
            "the dead spelling never reaches here from a checked run"
        );
        assert_eq!(
            coerce_declared(&json!("boolean"), &n, &e, "hello").unwrap(),
            json!("hello")
        );
    }
}
