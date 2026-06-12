// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `${{ }}` resolution + the v0 `when:` gate — the CEL seam.
//!
//! v0 is a REFERENCE resolver over the spec-04 value model, not an
//! expression language. Namespaces (spec 04 §the 5 namespaces, minus
//! the envelope-feature pair): `vars.X` · `with.X` · `item` / `index`
//! (`for_each` locals) · `tasks.<id>.<field>` (the result record's
//! closed field set · defined-null reads). The gate evaluates
//! `<ref> == '<lit>'` · `<ref> != '<lit>'` · bare `<ref>` (typed
//! truthiness). Everything else is LOUD (`NIKA-1702` unknown ref /
//! `NIKA-1703` out-of-subset form) — never a silent literal, never a
//! silently-closed gate. The full CEL evaluator (03-dag) replaces this
//! module behind the same [`render`] / [`eval_when`] seam.
//!
//! Single-pass island scan (NOT blind textual replace): values injected
//! from records are never re-scanned, so task output containing a
//! literal `${{` can never trip the guard nor inject a reference.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::errors::RuntimeError;
use crate::record::{TaskRecord, render_value};

/// The dataflow scope one render sees (spec 04 namespaces).
#[derive(Clone, Copy)]
pub(crate) struct Scope<'a> {
    /// `tasks.<id>.<field>` — the result records of settled tasks.
    pub records: &'a BTreeMap<String, TaskRecord>,
    /// `vars.<key>` — envelope defaults (typed or untyped · JSON values).
    pub vars: &'a BTreeMap<String, Value>,
    /// `with.<key>` — task-local injection (None outside a task body).
    pub with_ns: Option<&'a BTreeMap<String, Value>>,
    /// `item` / `index` — `for_each` locals (None outside an iteration).
    pub item: Option<&'a Value>,
    /// Zero-based position of `item` (spec 03 §`for_each`).
    pub index: Option<usize>,
}

impl<'a> Scope<'a> {
    /// A workflow-level scope (no task-local namespaces).
    pub(crate) fn workflow(
        records: &'a BTreeMap<String, TaskRecord>,
        vars: &'a BTreeMap<String, Value>,
    ) -> Self {
        Self {
            records,
            vars,
            with_ns: None,
            item: None,
            index: None,
        }
    }

    /// Resolve one island reference body to a value.
    ///
    /// # Errors
    ///
    /// [`RuntimeError::UnresolvedTemplate`] (NIKA-1702) for an unknown
    /// name in a known namespace · [`RuntimeError::WhenUnsupported`]
    /// (NIKA-1703) for a form outside the v0 subset (deep paths ·
    /// unknown namespaces · record fields beyond the closed set).
    fn resolve(&self, reference: &str) -> Result<Value, RuntimeError> {
        let unknown = || RuntimeError::UnresolvedTemplate {
            reference: reference.to_owned(),
        };
        let out_of_subset = || RuntimeError::WhenUnsupported {
            expr: reference.to_owned(),
        };

        if reference == "item" {
            return self.item.cloned().ok_or_else(unknown);
        }
        if reference == "index" {
            return self
                .index
                .map(|i| Value::Number(i.into()))
                .ok_or_else(unknown);
        }
        if let Some(key) = reference.strip_prefix("vars.") {
            return resolve_flat(self.vars, key, &unknown, &out_of_subset);
        }
        if let Some(key) = reference.strip_prefix("with.") {
            let ns = self.with_ns.ok_or_else(unknown)?;
            return resolve_flat(ns, key, &unknown, &out_of_subset);
        }
        if let Some(rest) = reference.strip_prefix("tasks.") {
            // `tasks.<id>.<field>` — the record is a CEL object · the
            // bare `tasks.<id>` alias does not exist (spec 04 · "no
            // bare alias") and deep paths are CEL territory (1703).
            let Some((id, field)) = rest.split_once('.') else {
                return Err(out_of_subset());
            };
            if field.contains('.') {
                return Err(out_of_subset());
            }
            let record = self.records.get(id).ok_or_else(unknown)?;
            // Unknown FIELD on the closed set = a form error (1703) ·
            // named jq bindings arrive with `output:` (post-builtin).
            return record.field(field).ok_or_else(out_of_subset);
        }
        Err(out_of_subset())
    }
}

/// Flat-namespace lookup · a dotted remainder is a deep path = out of
/// the v0 subset (1703 · CEL/jq territory) · a missing key is 1702.
fn resolve_flat(
    ns: &BTreeMap<String, Value>,
    key: &str,
    unknown: &dyn Fn() -> RuntimeError,
    out_of_subset: &dyn Fn() -> RuntimeError,
) -> Result<Value, RuntimeError> {
    if key.contains('.') {
        return Err(out_of_subset());
    }
    ns.get(key).cloned().ok_or_else(unknown)
}

/// Render every `${{ <ref> }}` island in `text` from the scope (spec 04
/// §value rendering · scalars natural · containers canonical JSON).
///
/// # Errors
///
/// [`RuntimeError::UnresolvedTemplate`] when an island's reference is
/// unknown (NIKA-1702 · the silent-literal guard) — and for a dangling
/// `${{` with no closing `}}` (same class · a template typo must not
/// ship as literal output). [`RuntimeError::WhenUnsupported`] when the
/// reference form is outside the v0 subset (NIKA-1703).
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
        out.push_str(&render_value(&scope.resolve(reference)?));
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Render every string leaf of a JSON value (invoke `args:` · `with:`).
///
/// A string leaf that is EXACTLY one island (`"${{ ref }}"`) resolves
/// to the referenced VALUE (type-preserving — an array stays an array ·
/// the `for_each` collection + `with:` object passing rely on this) ·
/// any other string renders textually.
pub(crate) fn render_json(value: &Value, scope: &Scope<'_>) -> Result<Value, RuntimeError> {
    Ok(match value {
        Value::String(text) => match single_island(text) {
            Some(reference) => scope.resolve(reference)?,
            None => Value::String(render(text, scope)?),
        },
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| render_json(v, scope))
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| Ok((k.clone(), render_json(v, scope)?)))
                .collect::<Result<_, _>>()?,
        ),
        other => other.clone(),
    })
}

/// `Some(ref)` when the whole string is exactly one `${{ ref }}` island.
fn single_island(text: &str) -> Option<&str> {
    let body = text.trim().strip_prefix("${{")?.strip_suffix("}}")?;
    // A second island inside would mean `}}…${{` — reject (textual).
    if body.contains("${{") || body.contains("}}") {
        return None;
    }
    Some(body.trim())
}

/// Evaluate a `when:` expression body over the v0 subset (spec 03).
///
/// Bare-`<ref>` truthiness is TYPED (the CEL milestone replaces it
/// with bool-typing · deliberately): `null`/`false` → false · numbers
/// → `!= 0` · strings → non-empty and not `"no"`/`"false"`/`"0"` ·
/// containers → non-empty.
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

    if reference.contains(char::is_whitespace) {
        return Err(RuntimeError::WhenUnsupported {
            expr: body.to_owned(),
        });
    }
    let value = scope.resolve(reference)?;

    match literal {
        None => Ok(truthy(&value)),
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
            Ok((render_value(&value) == expected) != negated)
        }
    }
}

/// v0 typed truthiness (see [`eval_when`]).
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty() && !matches!(s.as_str(), "no" | "false" | "0"),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::record::{TaskRecord, TaskStatus};

    fn record(status: TaskStatus, output: Value) -> TaskRecord {
        let mut rec = TaskRecord::unran(status);
        rec.output = output;
        rec
    }

    fn fixture() -> (BTreeMap<String, TaskRecord>, BTreeMap<String, Value>) {
        let records = BTreeMap::from([
            (
                "gather".to_owned(),
                record(TaskStatus::Success, serde_json::json!({"a": 1})),
            ),
            (
                "ghosted".to_owned(),
                record(TaskStatus::Skipped, Value::Null),
            ),
        ]);
        let vars = BTreeMap::from([
            ("publish".to_owned(), Value::String("no".to_owned())),
            ("source".to_owned(), Value::String("./news.json".to_owned())),
            ("urls".to_owned(), serde_json::json!(["a", "b"])),
        ]);
        (records, vars)
    }

    #[test]
    fn render_resolves_vars_and_task_outputs() {
        let (records, vars) = fixture();
        let scope = Scope::workflow(&records, &vars);
        let out = render(
            "read ${{ vars.source }} got ${{ tasks.gather.output }}",
            &scope,
        )
        .expect("renders");
        // Containers render as canonical compact JSON (spec 04).
        assert_eq!(out, "read ./news.json got {\"a\":1}");
    }

    #[test]
    fn render_record_fields_status_and_defined_null() {
        let (records, vars) = fixture();
        let scope = Scope::workflow(&records, &vars);
        assert_eq!(
            render("${{ tasks.gather.status }}", &scope).expect("status"),
            "success"
        );
        // Defined-null (spec 04): a skipped task's output reads as null.
        assert_eq!(
            render("v=${{ tasks.ghosted.output }}", &scope).expect("null read"),
            "v=null"
        );
        assert_eq!(
            render("${{ tasks.gather.error }}", &scope).expect("error of non-failure"),
            "null"
        );
    }

    #[test]
    fn render_unknown_reference_is_1702_and_deep_path_is_1703() {
        let (records, vars) = fixture();
        let scope = Scope::workflow(&records, &vars);
        assert!(matches!(
            render("${{ tasks.ghost.output }}", &scope).expect_err("unknown task"),
            RuntimeError::UnresolvedTemplate { ref reference } if reference == "tasks.ghost.output"
        ));
        assert!(matches!(
            render("${{ vars.nope }}", &scope).expect_err("unknown var"),
            RuntimeError::UnresolvedTemplate { .. }
        ));
        // Deep paths + unknown record fields + bare task alias = 1703.
        for out_of_subset in [
            "${{ tasks.gather.output.a }}",
            "${{ tasks.gather.weird }}",
            "${{ tasks.gather }}",
            "${{ vars.urls.0 }}",
            "${{ env.HOME }}",
        ] {
            assert!(
                matches!(
                    render(out_of_subset, &scope).expect_err(out_of_subset),
                    RuntimeError::WhenUnsupported { .. }
                ),
                "{out_of_subset} must be 1703"
            );
        }
    }

    #[test]
    fn render_dangling_island_is_loud() {
        let (records, vars) = fixture();
        let scope = Scope::workflow(&records, &vars);
        render("oops ${{ vars.source", &scope).expect_err("dangling island");
    }

    #[test]
    fn render_never_rescans_injected_values() {
        // A task output carrying a literal `${{` is DATA · single-pass
        // template scan must pass it through untouched.
        let records = BTreeMap::from([(
            "t".to_owned(),
            record(
                TaskStatus::Success,
                Value::String("${{ not.a.ref }}".into()),
            ),
        )]);
        let vars = BTreeMap::new();
        let scope = Scope::workflow(&records, &vars);
        let out = render("v=${{ tasks.t.output }}", &scope).expect("renders");
        assert_eq!(out, "v=${{ not.a.ref }}");
    }

    #[test]
    fn render_json_single_island_preserves_value_type() {
        let (records, vars) = fixture();
        let scope = Scope::workflow(&records, &vars);
        // Exactly-one-island string leaf → the VALUE (array stays array).
        let v = render_json(&serde_json::json!("${{ vars.urls }}"), &scope).expect("renders");
        assert_eq!(v, serde_json::json!(["a", "b"]));
        // Mixed text → textual render.
        let v = render_json(&serde_json::json!("x ${{ vars.source }}"), &scope).expect("renders");
        assert_eq!(v, serde_json::json!("x ./news.json"));
        // Nested leaves resolve · non-strings untouched.
        let v = render_json(
            &serde_json::json!({"path": "${{ vars.source }}", "n": [7, true]}),
            &scope,
        )
        .expect("renders");
        assert_eq!(
            v,
            serde_json::json!({"path": "./news.json", "n": [7, true]})
        );
    }

    #[test]
    fn with_and_for_each_locals_resolve() {
        let (records, vars) = fixture();
        let with_ns = BTreeMap::from([("page".to_owned(), Value::String("p1".into()))]);
        let item = Value::String("element".to_owned());
        let scope = Scope {
            records: &records,
            vars: &vars,
            with_ns: Some(&with_ns),
            item: Some(&item),
            index: Some(3),
        };
        assert_eq!(
            render("${{ with.page }}/${{ item }}@${{ index }}", &scope).expect("renders"),
            "p1/element@3"
        );
        // Outside an iteration the locals are unknown (1702).
        let bare = Scope::workflow(&records, &vars);
        assert!(matches!(
            render("${{ item }}", &bare).expect_err("no item outside for_each"),
            RuntimeError::UnresolvedTemplate { .. }
        ));
    }

    #[test]
    fn when_equality_open_and_closed() {
        let (records, vars) = fixture();
        let scope = Scope::workflow(&records, &vars);
        assert!(!eval_when("${{ vars.publish == 'yes' }}", &scope).expect("closed"));
        assert!(eval_when("${{ vars.publish == 'no' }}", &scope).expect("open"));
        assert!(eval_when("${{ vars.publish != 'yes' }}", &scope).expect("negated open"));
        assert!(eval_when("vars.publish == \"no\"", &scope).expect("bare body · double quotes"));
        // Status comparisons (spec 04 · the always-pattern's idiom).
        assert!(eval_when("${{ tasks.gather.status == 'success' }}", &scope).expect("status"));
        assert!(eval_when("${{ tasks.ghosted.status != 'success' }}", &scope).expect("skipped"));
    }

    #[test]
    fn when_bare_ref_typed_truthiness_table() {
        let records = BTreeMap::from([
            (
                "ok".to_owned(),
                record(TaskStatus::Success, serde_json::json!(["x"])),
            ),
            ("ghost".to_owned(), record(TaskStatus::Skipped, Value::Null)),
        ]);
        let vars = BTreeMap::from([
            ("yes_ish".to_owned(), Value::String("anything".into())),
            ("no_word".to_owned(), Value::String("no".into())),
            ("false_word".to_owned(), Value::String("false".into())),
            ("zero".to_owned(), Value::String("0".into())),
            ("empty".to_owned(), Value::String(String::new())),
            ("zero_num".to_owned(), serde_json::json!(0)),
            ("one_num".to_owned(), serde_json::json!(1)),
            ("yes_bool".to_owned(), serde_json::json!(true)),
            ("no_bool".to_owned(), serde_json::json!(false)),
            ("empty_list".to_owned(), serde_json::json!([])),
        ]);
        let scope = Scope::workflow(&records, &vars);
        for open in [
            "vars.yes_ish",
            "vars.one_num",
            "vars.yes_bool",
            "tasks.ok.output",
        ] {
            assert!(
                eval_when(&format!("${{{{ {open} }}}}"), &scope).expect("truthy"),
                "{open} must gate open"
            );
        }
        for closed in [
            "vars.no_word",
            "vars.false_word",
            "vars.zero",
            "vars.empty",
            "vars.zero_num",
            "vars.no_bool",
            "vars.empty_list",
            "tasks.ghost.output", // defined-null → false
        ] {
            assert!(
                !eval_when(&format!("${{{{ {closed} }}}}"), &scope).expect("falsy"),
                "{closed} must gate closed"
            );
        }
    }

    #[test]
    fn when_out_of_subset_is_nika_1703() {
        let (records, vars) = fixture();
        let scope = Scope::workflow(&records, &vars);
        for expr in [
            "${{ vars.a && vars.b }}",
            "${{ has(vars.publish) }}",
            "${{ vars.publish == yes }}", // unquoted literal
            "${{ 1 == 1 }}",
            // a tasks.-ref WITHOUT a field is OUT of the subset · it must
            // be 1703 (form invalid) · NOT 1702 (unknown ref).
            "${{ tasks.gather }}",
            // deep output path = CEL territory.
            "${{ tasks.gather.output.a == '1' }}",
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
            let records = BTreeMap::new();
            let vars = BTreeMap::from([(key, Value::String(value))]);
            let scope = Scope::workflow(&records, &vars);
            if let Ok(out) = render(&template, &scope)
                && template.contains("${{")
            {
                proptest::prop_assert!(!out.contains("${{"));
            }
        }

        /// Canonical rendering is deterministic: same value · same string ·
        /// and JSON-reparseable for containers (round-trip law).
        #[test]
        fn prop_render_value_deterministic_and_reparseable(
            keys in proptest::collection::vec("[a-z]{1,6}", 0..6),
        ) {
            let map: serde_json::Map<String, Value> = keys
                .iter()
                .enumerate()
                .map(|(i, k)| (k.clone(), serde_json::json!(i)))
                .collect();
            let v = Value::Object(map);
            let a = render_value(&v);
            let b = render_value(&v);
            proptest::prop_assert_eq!(&a, &b);
            let reparsed: Value = serde_json::from_str(&a).expect("canonical JSON reparses");
            proptest::prop_assert_eq!(reparsed, v);
        }
    }
}
