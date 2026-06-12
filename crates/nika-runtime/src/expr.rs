// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `${{ }}` resolution + the v0 `when:` gate — the CEL seam.
//!
//! v0 is a REFERENCE resolver, not an expression language: islands
//! resolve `tasks.<id>.output` and `vars.<key>` · the gate evaluates
//! `<ref> == '<lit>'` · `<ref> != '<lit>'` · bare `<ref>` (truthy).
//! Everything else is LOUD (`NIKA-1702` / `NIKA-1703`) — never a
//! silent literal, never a silently-closed gate (the rehearsal's
//! stand-in dies here). The full CEL evaluator (03-dag) replaces this
//! module behind the same [`render`] / [`eval_when`] seam.
//!
//! Single-pass island scan (NOT blind textual replace): values injected
//! from bindings are never re-scanned, so task output containing a
//! literal `${{` can never trip the guard nor inject a reference.

use std::collections::BTreeMap;

use crate::errors::RuntimeError;

/// The dataflow scope one render sees · task outputs + envelope vars.
pub(crate) struct Scope<'a> {
    /// `tasks.<id>.output` values (terminal Ok tasks only).
    pub bindings: &'a BTreeMap<String, String>,
    /// `vars.<key>` string values from the envelope.
    pub vars: &'a BTreeMap<String, String>,
}

impl Scope<'_> {
    /// Resolve one island reference body.
    fn resolve(&self, reference: &str) -> Option<&str> {
        if let Some(key) = reference.strip_prefix("vars.") {
            return self.vars.get(key).map(String::as_str);
        }
        let id = reference.strip_prefix("tasks.")?.strip_suffix(".output")?;
        self.bindings.get(id).map(String::as_str)
    }
}

/// Render every `${{ <ref> }}` island in `text` from the scope.
///
/// # Errors
///
/// [`RuntimeError::UnresolvedTemplate`] when an island's reference is
/// unknown (NIKA-1702 · the silent-literal guard) — and for a dangling
/// `${{` with no closing `}}` (same class · a template typo must not
/// ship as literal output).
pub(crate) fn render(text: &str, scope: &Scope<'_>) -> Result<String, RuntimeError> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("${{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 3..];
        let Some(end) = after.find("}}") else {
            return Err(RuntimeError::UnresolvedTemplate {
                reference: after.trim().to_owned(),
            });
        };
        let reference = after[..end].trim();
        let Some(value) = scope.resolve(reference) else {
            return Err(RuntimeError::UnresolvedTemplate {
                reference: reference.to_owned(),
            });
        };
        out.push_str(value);
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Render every string leaf of a JSON value (invoke `args:`).
pub(crate) fn render_json(
    value: &serde_json::Value,
    scope: &Scope<'_>,
) -> Result<serde_json::Value, RuntimeError> {
    Ok(match value {
        serde_json::Value::String(text) => serde_json::Value::String(render(text, scope)?),
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .iter()
                .map(|v| render_json(v, scope))
                .collect::<Result<_, _>>()?,
        ),
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(k, v)| Ok((k.clone(), render_json(v, scope)?)))
                .collect::<Result<_, _>>()?,
        ),
        other => other.clone(),
    })
}

/// Evaluate a `when:` expression body (the inside of the island) over
/// the v0 subset (spec §3).
///
/// Truthiness for the bare-`<ref>` form · non-empty AND not one of
/// `"no"` / `"false"` / `"0"`.
///
/// # Errors
///
/// [`RuntimeError::UnresolvedTemplate`] on an unknown reference ·
/// [`RuntimeError::WhenUnsupported`] on any form outside the subset.
pub(crate) fn eval_when(expr: &str, scope: &Scope<'_>) -> Result<bool, RuntimeError> {
    // The analyzer enforces the single-island shape · accept both the
    // wrapped (`${{ … }}`) and bare body forms defensively.
    let body = expr
        .trim()
        .strip_prefix("${{")
        .and_then(|s| s.strip_suffix("}}"))
        .unwrap_or(expr)
        .trim();

    let (reference, literal, negated) = if let Some((l, r)) = body.split_once("==") {
        (l.trim(), Some(r.trim()), false)
    } else if let Some((l, r)) = body.split_once("!=") {
        (l.trim(), Some(r.trim()), true)
    } else {
        (body, None, false)
    };

    if !(reference.starts_with("vars.")
        || (reference.starts_with("tasks.") && reference.ends_with(".output")))
        || reference.contains(char::is_whitespace)
    {
        return Err(RuntimeError::WhenUnsupported {
            expr: body.to_owned(),
        });
    }
    let Some(value) = scope.resolve(reference) else {
        return Err(RuntimeError::UnresolvedTemplate {
            reference: reference.to_owned(),
        });
    };

    match literal {
        None => Ok(!value.is_empty() && !matches!(value, "no" | "false" | "0")),
        Some(lit) => {
            let unquoted = lit
                .strip_prefix('\'')
                .and_then(|s| s.strip_suffix('\''))
                .or_else(|| lit.strip_prefix('"').and_then(|s| s.strip_suffix('"')));
            let Some(expected) = unquoted else {
                return Err(RuntimeError::WhenUnsupported {
                    expr: body.to_owned(),
                });
            };
            Ok((value == expected) != negated)
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn scope_fixture() -> (BTreeMap<String, String>, BTreeMap<String, String>) {
        let bindings = BTreeMap::from([("gather".to_owned(), "{\"a\":1}".to_owned())]);
        let vars = BTreeMap::from([
            ("publish".to_owned(), "no".to_owned()),
            ("source".to_owned(), "./news.json".to_owned()),
        ]);
        (bindings, vars)
    }

    #[test]
    fn render_resolves_vars_and_task_outputs() {
        let (bindings, vars) = scope_fixture();
        let scope = Scope {
            bindings: &bindings,
            vars: &vars,
        };
        let out = render(
            "read ${{ vars.source }} got ${{ tasks.gather.output }}",
            &scope,
        )
        .expect("renders");
        assert_eq!(out, "read ./news.json got {\"a\":1}");
    }

    #[test]
    fn render_unknown_reference_is_loud() {
        let (bindings, vars) = scope_fixture();
        let scope = Scope {
            bindings: &bindings,
            vars: &vars,
        };
        let err = render("${{ tasks.ghost.output }}", &scope).expect_err("must refuse");
        assert!(matches!(
            err,
            RuntimeError::UnresolvedTemplate { ref reference } if reference == "tasks.ghost.output"
        ));
    }

    #[test]
    fn render_dangling_island_is_loud() {
        let (bindings, vars) = scope_fixture();
        let scope = Scope {
            bindings: &bindings,
            vars: &vars,
        };
        render("oops ${{ vars.source", &scope).expect_err("dangling island");
    }

    #[test]
    fn render_never_rescans_injected_values() {
        // A task output carrying a literal `${{` is DATA · single-pass
        // template scan must pass it through untouched.
        let bindings = BTreeMap::from([("t".to_owned(), "${{ not.a.ref }}".to_owned())]);
        let vars = BTreeMap::new();
        let scope = Scope {
            bindings: &bindings,
            vars: &vars,
        };
        let out = render("v=${{ tasks.t.output }}", &scope).expect("renders");
        assert_eq!(out, "v=${{ not.a.ref }}");
    }

    #[test]
    fn render_json_resolves_every_string_leaf() {
        let (bindings, vars) = scope_fixture();
        let scope = Scope {
            bindings: &bindings,
            vars: &vars,
        };
        let value = serde_json::json!({
            "path": "${{ vars.source }}",
            "nested": ["${{ tasks.gather.output }}", 7, true],
        });
        let out = render_json(&value, &scope).expect("renders");
        assert_eq!(
            out,
            serde_json::json!({
                "path": "./news.json",
                "nested": ["{\"a\":1}", 7, true],
            })
        );
    }

    #[test]
    fn when_equality_open_and_closed() {
        let (bindings, vars) = scope_fixture();
        let scope = Scope {
            bindings: &bindings,
            vars: &vars,
        };
        assert!(!eval_when("${{ vars.publish == 'yes' }}", &scope).expect("closed"));
        assert!(eval_when("${{ vars.publish == 'no' }}", &scope).expect("open"));
        assert!(eval_when("${{ vars.publish != 'yes' }}", &scope).expect("negated open"));
        assert!(eval_when("vars.publish == \"no\"", &scope).expect("bare body · double quotes"));
    }

    #[test]
    fn when_bare_ref_truthiness_table() {
        let bindings = BTreeMap::new();
        let vars = BTreeMap::from([
            ("yes_ish".to_owned(), "anything".to_owned()),
            ("no_word".to_owned(), "no".to_owned()),
            ("false_word".to_owned(), "false".to_owned()),
            ("zero".to_owned(), "0".to_owned()),
            ("empty".to_owned(), String::new()),
        ]);
        let scope = Scope {
            bindings: &bindings,
            vars: &vars,
        };
        assert!(eval_when("${{ vars.yes_ish }}", &scope).expect("truthy"));
        for falsy in ["no_word", "false_word", "zero", "empty"] {
            assert!(
                !eval_when(&format!("${{{{ vars.{falsy} }}}}"), &scope).expect("falsy"),
                "vars.{falsy} must gate closed"
            );
        }
    }

    #[test]
    fn when_out_of_subset_is_nika_1703() {
        let (bindings, vars) = scope_fixture();
        let scope = Scope {
            bindings: &bindings,
            vars: &vars,
        };
        for expr in [
            "${{ vars.a && vars.b }}",
            "${{ has(vars.publish) }}",
            "${{ vars.publish == yes }}", // unquoted literal
            "${{ 1 == 1 }}",
            // a tasks.-prefixed ref WITHOUT .output is OUT of the subset ·
            // it must be 1703 (form invalid) · NOT 1702 (unknown ref) —
            // kills the `&&`→`||` mutant on the ref-shape validation.
            "${{ tasks.gather }}",
        ] {
            let err = eval_when(expr, &scope).expect_err(expr);
            assert!(
                matches!(err, RuntimeError::WhenUnsupported { .. }),
                "{expr} → {err}"
            );
        }
    }

    proptest::proptest! {
        #![proptest_config(proptest::test_runner::Config {
            cases: 256,
            ..proptest::test_runner::Config::default()
        })]

        /// Render is total over arbitrary templates + scopes: it returns
        /// Ok or a typed error · never panics · and on Ok the output
        /// carries no unresolved island from the TEMPLATE.
        #[test]
        fn prop_render_total_and_clean(
            template in ".{0,80}",
            key in "[a-z]{1,8}",
            value in ".{0,20}",
        ) {
            let bindings = BTreeMap::new();
            let vars = BTreeMap::from([(key, value)]);
            let scope = Scope { bindings: &bindings, vars: &vars };
            if let Ok(out) = render(&template, &scope) {
                // Every `${{` in the OUTPUT must come from injected data ·
                // with empty bindings + simple vars that means: none ·
                // unless the template itself had no island at all.
                if template.contains("${{") {
                    proptest::prop_assert!(!out.contains("${{"));
                }
            }
        }
    }
}
