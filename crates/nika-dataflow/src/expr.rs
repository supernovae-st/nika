// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `${{ }}` resolution + the v0 `when:` gate — the CEL seam.
//!
//! v0 is a REFERENCE resolver over the spec-04 value model, not an
//! expression language. Namespaces (spec 04 §the 5 namespaces, minus
//! the envelope-feature pair): `inputs.X` · `with.X` · `item` / `index`
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
//!
//! # CEL milestone (this module)
//!
//! `nika-cel` (the `cel-subset/0.1` engine · L0) is now bound behind
//! the seam. Both value substitution AND `when:` evaluate the FULL
//! `cel-subset/0.1` grammar (spec 03-dag): comparisons
//! (`== != < <= > >=`), deep paths (`tasks.x.output.y`),
//! booleans/numbers, `size()`, `.contains()`/`.startsWith()`, the
//! ternary `? :`, membership `in`. The namespaces are bound as CEL
//! variables — `inputs` · `config` · `const` · `with` · `tasks` · `secrets` plus the
//! `for_each` locals `item` / `index`.
//!
//! Injection safety is PRESERVED + made structural: the author's
//! expression text is parsed ONCE; runtime values are bound as TYPED
//! CEL VARIABLES (data returned by the [`Resolver`]), never spliced
//! into expression text before parsing. A task output carrying literal
//! `${{ … }}` is a string Value inside the `tasks` object — read as
//! data, never re-evaluated.

use std::collections::BTreeMap;

use nika_cel::{Resolver, compute, compute_bool, parse};
use nika_schema::types::Permits;
use nika_tmpl::{scan_islands, single_island};
use serde_json::Value;

use crate::errors::DataflowError;
use crate::record::{TaskRecord, render_value};

/// An empty secrets namespace — the no-secrets binding used by the
/// 2-arg [`Scope::workflow`] test constructor (production always threads the
/// run-resolved map via [`Scope::workflow_with_secrets`]). When the composer
/// injected no resolver the run's map is simply empty, so a `secrets.X`
/// reference resolves to `None` → NIKA-1702 (the loud unresolved class).
#[cfg(any(test, feature = "testing"))]
static EMPTY_SECRETS: std::sync::LazyLock<BTreeMap<String, Value>> =
    std::sync::LazyLock::new(BTreeMap::new);

#[cfg(any(test, feature = "testing"))]
static EMPTY_ENV: std::sync::LazyLock<BTreeMap<String, Value>> =
    std::sync::LazyLock::new(BTreeMap::new);

/// The dataflow scope one render sees (spec 04 namespaces).
#[derive(Clone, Copy)]
pub struct Scope<'a> {
    /// `tasks.<id>.<field>` — the result records of settled tasks.
    pub records: &'a BTreeMap<String, TaskRecord>,
    /// `inputs.<key>` — typed input defaults + the operator's `--var`
    /// overrides (JSON values).
    pub inputs: &'a BTreeMap<String, Value>,
    /// `const.<key>` — named constants (bare literals + typed `{type,
    /// value}` values · always bound, never overridable).
    pub consts: &'a BTreeMap<String, Value>,
    /// `secrets.<name>` — RESOLVED secret values (spec 01 §secrets ·
    /// MINOR-B). Empty when the composer injected no resolver — then a
    /// `secrets.X` reference is unresolved (NIKA-1702), never a silent
    /// empty. A resolved value flows ONLY where the IFC sanctions it (the
    /// `nika check` secret-flow analysis · ADR-092) and is NEVER emitted to
    /// the event stream (notes carry the program/model, not field values).
    pub secrets: &'a BTreeMap<String, Value>,
    /// `with.<key>` — task-local injection (None outside a task body).
    pub with_ns: Option<&'a BTreeMap<String, Value>>,
    /// `item` / `index` — `for_each` locals (None outside an iteration).
    pub item: Option<&'a Value>,
    /// Zero-based position of `item` (spec 03 §`for_each`).
    pub index: Option<usize>,
    /// `permits:` — the workflow's declared capability boundary (spec 01
    /// §permits). `None` = no boundary declared (F-O8 « absent = zero
    /// authority » · every effect refused at the gates · the always-on
    /// floors stay on top). Set on the task-dispatch scopes so the exec
    /// sink can enforce `permits.exec` (NIKA-SEC-004); `None` elsewhere.
    pub permits: Option<&'a Permits>,
}

impl<'a> Scope<'a> {
    /// A workflow-level scope with NO secrets — the unit-test constructor
    /// (production always threads the run-resolved secrets map via
    /// [`Self::workflow_with_secrets`]).
    #[cfg(any(test, feature = "testing"))]
    #[must_use]
    pub fn workflow(
        records: &'a BTreeMap<String, TaskRecord>,
        inputs: &'a BTreeMap<String, Value>,
    ) -> Self {
        Self::workflow_with_value_authorities(records, inputs, &EMPTY_ENV, &EMPTY_SECRETS)
    }

    /// A workflow-level scope carrying the resolved `secrets:` namespace
    /// (the production gate / `outputs:` / cleanup path · MINOR-B).
    #[cfg(any(test, feature = "testing"))]
    #[must_use]
    pub fn workflow_with_secrets(
        records: &'a BTreeMap<String, TaskRecord>,
        inputs: &'a BTreeMap<String, Value>,
        secrets: &'a BTreeMap<String, Value>,
    ) -> Self {
        Self::workflow_with_value_authorities(records, inputs, &EMPTY_ENV, secrets)
    }

    /// A workflow-level scope carrying the three-authority value
    /// environment + resolved `secrets:` values. This is the production
    /// constructor.
    #[must_use]
    pub fn workflow_with_value_authorities(
        records: &'a BTreeMap<String, TaskRecord>,
        inputs: &'a BTreeMap<String, Value>,
        consts: &'a BTreeMap<String, Value>,
        secrets: &'a BTreeMap<String, Value>,
    ) -> Self {
        Self {
            records,
            inputs,
            consts,
            secrets,
            with_ns: None,
            item: None,
            index: None,
            permits: None,
        }
    }

    /// Evaluate one `${{ … }}` body (an island reference OR a richer
    /// expression) to a value through [`nika_cel`].
    ///
    /// `reference` is the AUTHOR's text — parsed exactly once by
    /// [`parse`]; the runtime values are bound as DATA by
    /// `ScopeResolver` (private · named, not linked, so this public doc
    /// does not depend on `--document-private-items`), never re-parsed
    /// (the injection-safe boundary ·
    /// see module docs). `pub`: the runtime's recover-await classifier
    /// probes islands through THIS seam, so park-vs-fail agrees with the
    /// render by construction (spec 05 §recover · zero drift).
    ///
    /// # Errors
    ///
    /// [`DataflowError::UnresolvedTemplate`] (NIKA-1702) for an unknown
    /// reference (`NIKA-VAR-001`) · [`DataflowError::WhenUnsupported`]
    /// (NIKA-1703) for a static-grammar violation (`NIKA-VAR-005`) ·
    /// [`DataflowError::CelEval`] (`NIKA-VAR-006`) for an evaluation type
    /// error (cross-type compare · `size()` of a scalar · …).
    pub fn resolve_expr(&self, reference: &str) -> Result<Value, DataflowError> {
        let expr = parse(reference).map_err(|e| DataflowError::from_cel(&e, reference))?;
        compute(&expr, &ScopeResolver(self)).map_err(|e| DataflowError::from_cel(&e, reference))
    }
}

/// A [`Resolver`] over a [`Scope`] — resolves a CEL ROOT identifier to
/// the WHOLE namespace value; the CEL computer owns `.field` / `[i]`
/// navigation on top. This is the injection-safe binding: every value
/// returned here is DATA (a `serde_json::Value`), never expression text.
///
/// `None` ⇒ the root is unbound ⇒ `nika-cel` raises `NIKA-VAR-001`
/// (which the seam maps to NIKA-1702). `env` / `secrets` are valid CEL
/// roots but carry no runtime binding in the v0 runtime — they resolve
/// to `None` (unresolved · loud), never a silent empty value.
struct ScopeResolver<'s, 'a>(&'s Scope<'a>);

impl Resolver for ScopeResolver<'_, '_> {
    fn resolve_root(&self, name: &str) -> Option<Value> {
        let scope = self.0;
        match name {
            // The two `for_each` loop-locals (None outside an iteration).
            "item" => scope.item.cloned(),
            "index" => scope.index.map(|i| Value::Number(i.into())),
            // The value authorities → a CEL object each (the computer
            // indexes it). The dead `vars`/`env`/`config` roots fall
            // through to unresolved (check refuses them statically, and
            // an empty namespace object would answer where nothing lives).
            "inputs" => Some(ns_object(scope.inputs)),
            "const" => Some(ns_object(scope.consts)),
            "with" => scope.with_ns.map(ns_object),
            // `tasks` → an object of per-id RECORD objects (the record's
            // closed field set · defined-null reads · spec 04). The bare
            // `tasks.<id>` now resolves to that record object (CEL owns
            // the `.field` step), so `tasks.x.output.y` deep paths work.
            "tasks" => Some(tasks_object(scope.records)),
            // `secrets.<name>` → the RESOLVED secret values (MINOR-B). The
            // composer injects them (env/file · the sanctioned boundary);
            // when none was injected the map is empty and `.field` raises
            // NIKA-VAR-001 → 1702 (the loud unresolved class). A resolved
            // value is DATA bound here, never expression text (injection-safe
            // · same boundary as every other namespace).
            "secrets" if !scope.secrets.is_empty() => Some(ns_object(scope.secrets)),
            // an unbound root falls here → unresolved (1702).
            _ => None,
        }
    }
}

/// A flat namespace map (a value authority · `with`) as a CEL object value.
fn ns_object(ns: &BTreeMap<String, Value>) -> Value {
    Value::Object(ns.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
}

/// The `tasks` namespace as a CEL object — one entry per settled task,
/// each the record's full object (`output` · `status` · `error` ·
/// `started_at` · `ended_at` · `duration_ms` · spec 04). A missing task
/// id is simply absent (CEL's `.field` then raises `NIKA-VAR-001`).
fn tasks_object(records: &BTreeMap<String, TaskRecord>) -> Value {
    Value::Object(
        records
            .iter()
            .map(|(id, rec)| (id.clone(), record_object(rec)))
            .collect(),
    )
}

/// One [`TaskRecord`] as its CEL object (the closed reserved field set ·
/// spec 04 · defined-null: absent values are `null`, never missing keys,
/// so `tasks.x.error` reads `null` rather than raising on a non-failure)
/// PLUS the task's `output:` named bindings as sibling keys
/// (`tasks.x.<name>` · spec 04 §Output binding · the dual-accessible
/// surface). Reserved-name collisions are a parse error (the checker),
/// so a binding never overwrites a reserved field.
fn record_object(rec: &TaskRecord) -> Value {
    const FIELDS: [&str; 7] = [
        "output",
        "status",
        "cause",
        "error",
        "started_at",
        "ended_at",
        "duration_ms",
    ];
    let mut map: serde_json::Map<String, Value> = FIELDS
        .iter()
        .map(|f| ((*f).to_owned(), rec.field(f).unwrap_or(Value::Null)))
        .collect();
    for (name, value) in &rec.named {
        map.insert(name.clone(), value.clone());
    }
    Value::Object(map)
}

/// Emit `s` into `out`, applying the `\${{` literal-escape (spec 04 §Escaping):
/// each `\${{` becomes a literal `${{`. This is render's escape-STRIPPING — the
/// checker does not need it. Island FINDING + escape DETECTION is
/// `nika_tmpl::scan_islands`'s job (the ONE lexer shared with the checker · zero
/// drift), so this only handles the text BETWEEN real islands.
fn push_unescaped(out: &mut String, s: &str) {
    let mut rest = s;
    while let Some(p) = rest.find("\\${{") {
        out.push_str(&rest[..p]);
        out.push_str("${{");
        rest = &rest[p + 4..];
    }
    out.push_str(rest);
}

/// Render every `${{ <ref> }}` island in `text` from the scope (spec 04
/// §value rendering · scalars natural · containers canonical JSON).
///
/// # Errors
///
/// [`DataflowError::UnresolvedTemplate`] when an island's reference is
/// unknown (NIKA-1702 · the silent-literal guard) — and for a dangling
/// `${{` with no closing `}}` (same class · a template typo must not
/// ship as literal output). [`DataflowError::WhenUnsupported`] when the
/// reference form is outside the v0 subset (NIKA-1703).
pub fn render(text: &str, scope: &Scope<'_>) -> Result<String, DataflowError> {
    // Island finding (open + `\${{` escape + quote-aware close) is delegated to
    // `nika_tmpl::scan_islands` — the SAME lexer the checker uses, so `check` ⇄
    // `run` agree on island bounds by construction (the 2026-06-18 escape-drift
    // bug is structurally impossible now). Scanning runs over the AUTHOR's text
    // ENTIRELY before any resolution, so resolved values are never re-scanned
    // (injection-safe · sister invariant to render_never_rescans_injected_values).
    let islands = scan_islands(text).map_err(|e| DataflowError::UnresolvedTemplate {
        reference: text[e.offset() + 3..].trim().to_owned(),
    })?;
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for isl in islands {
        push_unescaped(&mut out, &text[cursor..isl.start]);
        out.push_str(&render_value(&scope.resolve_expr(isl.body.trim())?));
        cursor = isl.end;
    }
    push_unescaped(&mut out, &text[cursor..]);
    Ok(out)
}

/// Render every string leaf of a JSON value (invoke `args:` · `with:`).
///
/// A string leaf that is EXACTLY one island (`"${{ ref }}"`) resolves
/// to the referenced VALUE (type-preserving — an array stays an array ·
/// the `for_each` collection + `with:` object passing rely on this) ·
/// any other string renders textually.
///
/// # Errors
///
/// Whatever [`Scope::resolve_expr`] raises for the first leaf that fails —
/// [`DataflowError::UnresolvedTemplate`] (`NIKA-VAR-001`) ·
/// [`DataflowError::WhenUnsupported`] (`NIKA-VAR-005`) ·
/// [`DataflowError::CelEval`] (`NIKA-VAR-006`). Leaves are walked in
/// document order, so the error names the FIRST offending reference.
pub fn render_json(value: &Value, scope: &Scope<'_>) -> Result<Value, DataflowError> {
    Ok(match value {
        Value::String(text) => match single_island(text) {
            Some(reference) => scope.resolve_expr(reference)?,
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

/// Evaluate a `when:` expression body to a boolean (spec 03 · the full
/// `cel-subset/0.1` gate).
///
/// The body is the AUTHOR's text — parsed once, then [`compute_bool`]
/// runs it over the private `ScopeResolver` (named, not linked — a public
/// doc must not depend on `--document-private-items`). A `when:` MUST
/// evaluate to a
/// boolean (spec 03 §when shape): a non-boolean result is `NIKA-VAR-006`
/// (carried as [`DataflowError::CelEval`]). Static-shape rejection of a
/// non-boolean ROOT (`NIKA-VAR-005`) is the checker's job; the runtime
/// is the defensive backstop and accepts any spec-valid expression.
///
/// # Errors
///
/// [`DataflowError::UnresolvedTemplate`] (NIKA-1702) on an unknown
/// reference · [`DataflowError::WhenUnsupported`] (NIKA-1703) on a
/// static-grammar violation · [`DataflowError::CelEval`] (NIKA-VAR-006)
/// on an evaluation type error (incl. a non-boolean result).
pub fn eval_when(expr: &str, scope: &Scope<'_>) -> Result<bool, DataflowError> {
    // Accept both the wrapped (`${{ … }}`) and bare body forms — the
    // analyzer enforces the single-island shape upstream; strip the
    // wrapper defensively so a bare body (the checker's normalized form
    // and the test idiom) parses identically.
    let body = expr
        .trim()
        .strip_prefix("${{")
        .and_then(|s| s.strip_suffix("}}"))
        .unwrap_or(expr)
        .trim();
    let parsed = parse(body).map_err(|e| DataflowError::from_cel(&e, body))?;
    compute_bool(&parsed, &ScopeResolver(scope)).map_err(|e| DataflowError::from_cel(&e, body))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::record::{TaskRecord, TaskStatus, TerminalCause};

    fn record(status: TaskStatus, output: Value) -> TaskRecord {
        let cause = match status {
            TaskStatus::Success => TerminalCause::Normal,
            TaskStatus::Failure => TerminalCause::VerbError,
            TaskStatus::Cancelled => TerminalCause::Upstream,
            TaskStatus::Skipped => TerminalCause::Gate,
        };
        let mut rec = TaskRecord::unran(status, cause);
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
            "read ${{ inputs.source }} got ${{ tasks.gather.output }}",
            &scope,
        )
        .expect("renders");
        // Containers render as canonical compact JSON (spec 04).
        assert_eq!(out, "read ./news.json got {\"a\":1}");
    }

    #[test]
    fn render_resolves_a_deployment_supplied_input() {
        let (records, mut inputs) = fixture();
        inputs.insert(
            "API_BASE".to_owned(),
            Value::String("https://odin.example".to_owned()),
        );
        let scope =
            Scope::workflow_with_value_authorities(&records, &inputs, &EMPTY_ENV, &EMPTY_SECRETS);
        assert_eq!(
            render("${{ inputs.API_BASE }}", &scope).expect("the input resolves"),
            "https://odin.example"
        );
        // The dead `config` root answers NOTHING — an empty namespace
        // object would have resolved where no authority lives.
        assert!(render("${{ config.API_BASE }}", &scope).is_err());
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
    fn render_unknown_reference_is_1702() {
        // CEL milestone: an unknown reference (NIKA-VAR-001) maps to
        // NIKA-1702 · the `reference` carried is the AUTHOR's text.
        let (records, vars) = fixture();
        let scope = Scope::workflow(&records, &vars);
        assert!(matches!(
            render("${{ tasks.ghost.output }}", &scope).expect_err("unknown task"),
            DataflowError::UnresolvedTemplate { ref reference } if reference == "tasks.ghost.output"
        ));
        assert!(matches!(
            render("${{ inputs.nope }}", &scope).expect_err("unknown var"),
            DataflowError::UnresolvedTemplate { .. }
        ));
        // A deep path INTO an unknown task is still 1702 (the root id
        // doesn't resolve · CEL's `.field` then raises VAR-001).
        assert!(matches!(
            render("${{ tasks.ghost.output.x }}", &scope).expect_err("unknown deep"),
            DataflowError::UnresolvedTemplate { .. }
        ));
        // `env` is a valid CEL root but UNBOUND → unresolved (1702 · loud).
        // `secrets` with no resolver injected is unbound too (the fixture
        // scope carries an empty secrets map · fail-closed).
        assert!(matches!(
            render("${{ env.HOME }}", &scope).expect_err("env unbound"),
            DataflowError::UnresolvedTemplate { .. }
        ));
        assert!(matches!(
            render("${{ secrets.api_key }}", &scope).expect_err("secrets unbound"),
            DataflowError::UnresolvedTemplate { .. }
        ));
    }

    #[test]
    fn secrets_namespace_resolves_when_bound() {
        // MINOR-B: when the composer resolved the `secrets:` namespace, a
        // `${{ secrets.X }}` reference reads its value (DATA · injection-safe
        // · the same binding boundary as every other namespace).
        let records = BTreeMap::new();
        let vars = BTreeMap::new();
        let secrets = BTreeMap::from([(
            "api_key".to_owned(),
            Value::String("sk-resolved".to_owned()),
        )]);
        let scope = Scope::workflow_with_secrets(&records, &vars, &secrets);
        assert_eq!(
            render("Bearer ${{ secrets.api_key }}", &scope).expect("resolves"),
            "Bearer sk-resolved"
        );
        // An UNDECLARED secret is still unresolved (1702 · fail-closed).
        assert!(matches!(
            render("${{ secrets.absent }}", &scope).expect_err("absent secret"),
            DataflowError::UnresolvedTemplate { .. }
        ));
        // A secret value that LOOKS like an expression is DATA, never
        // re-evaluated (injection-safe · the load-bearing invariant).
        let evil = BTreeMap::from([(
            "x".to_owned(),
            Value::String("${{ inputs.admin == true }}".to_owned()),
        )]);
        let evil_scope = Scope::workflow_with_secrets(&records, &vars, &evil);
        assert_eq!(
            render("${{ secrets.x }}", &evil_scope).expect("data"),
            "${{ inputs.admin == true }}",
            "a secret value is bound as data, never re-parsed"
        );
    }

    #[test]
    fn render_deep_paths_and_indexing_resolve() {
        // The CEL milestone supersedes the v0 "deep path = 1703" rule:
        // `tasks.<id>.output.<field>` and `[i]` now RESOLVE.
        let records = BTreeMap::from([(
            "gather".to_owned(),
            record(
                TaskStatus::Success,
                serde_json::json!({"a": 1, "nested": {"b": "deep"}, "list": [10, 20]}),
            ),
        )]);
        let vars = BTreeMap::from([("urls".to_owned(), serde_json::json!(["x", "y"]))]);
        let scope = Scope::workflow(&records, &vars);
        // Deep object path.
        assert_eq!(
            render("${{ tasks.gather.output.a }}", &scope).expect("deep a"),
            "1"
        );
        assert_eq!(
            render("${{ tasks.gather.output.nested.b }}", &scope).expect("deeper"),
            "deep"
        );
        // Index access on a deep array + a flat var array.
        assert_eq!(
            render("${{ tasks.gather.output.list[1] }}", &scope).expect("deep index"),
            "20"
        );
        assert_eq!(
            render("${{ inputs.urls[0] }}", &scope).expect("flat index"),
            "x"
        );
    }

    #[test]
    fn render_unknown_field_and_grammar_errors_are_loud() {
        // A missing FIELD on a resolved object is unresolved (1702 ·
        // VAR-001) · a genuine GRAMMAR violation is 1703 (VAR-005).
        let (records, vars) = fixture();
        let scope = Scope::workflow(&records, &vars);
        // `tasks.gather.output` is {"a":1}; `.weird` is absent → 1702.
        assert!(matches!(
            render("${{ tasks.gather.output.weird }}", &scope).expect_err("missing field"),
            DataflowError::UnresolvedTemplate { .. }
        ));
        // A genuinely malformed expression → 1703 (out of grammar).
        assert!(matches!(
            render("${{ inputs.a && }}", &scope).expect_err("malformed"),
            DataflowError::WhenUnsupported { .. }
        ));
        assert!(matches!(
            render("${{ inputs.a @ vars.b }}", &scope).expect_err("bad token"),
            DataflowError::WhenUnsupported { .. }
        ));
    }

    #[test]
    fn render_dangling_island_is_loud() {
        let (records, vars) = fixture();
        let scope = Scope::workflow(&records, &vars);
        render("oops ${{ inputs.source", &scope).expect_err("dangling island");
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
    fn render_honors_backslash_escape() {
        // spec 04 §Escaping ("the engine MUST honor this") + conformance
        // fixture core/variables/010-valid-escaped-literal: `\${{ ref }}`
        // renders a LITERAL `${{ ref }}` (backslash consumed · the inner
        // reference NEVER resolved). Previously the runtime kept the `\`
        // and resolved the ref — a check⇄runtime drift, since the static
        // analyzer already skips escaped openers (nika-schema template scan).
        let (records, vars) = fixture();
        let scope = Scope::workflow(&records, &vars);

        // The fixture-010 documentation case: `vars.x` is UNKNOWN, but the
        // escape means it is never looked up — so this renders cleanly
        // (pre-fix it raised NIKA-1702 on the undefined reference).
        assert_eq!(
            render("The syntax \\${{ inputs.x }} references variables.", &scope)
                .expect("escaped ref is documentation text, not resolved"),
            "The syntax ${{ inputs.x }} references variables."
        );

        // The escape wins even when the reference WOULD resolve — a literal
        // `${{` is the author's intent, never a substitution.
        assert_eq!(
            render("literal \\${{ inputs.source }}", &scope).expect("literal"),
            "literal ${{ inputs.source }}"
        );

        // Escaped + real island mix: the escape stays literal, the real
        // island resolves (mirrors the static analyzer's mixed-scan test).
        assert_eq!(
            render("esc \\${{ a }} real ${{ inputs.source }}", &scope).expect("mix"),
            "esc ${{ a }} real ./news.json"
        );

        // SECURITY: an injected value that CONTAINS `\${{` is DATA — it is
        // pushed out, never re-scanned, so the escape is NOT applied to it
        // (the single-pass injection-safe boundary is unchanged).
        let injected = BTreeMap::from([(
            "t".to_owned(),
            record(TaskStatus::Success, Value::String("\\${{ x }}".into())),
        )]);
        let iscope = Scope::workflow(&injected, &vars);
        assert_eq!(
            render("${{ tasks.t.output }}", &iscope).expect("injected data"),
            "\\${{ x }}",
            "an injected backslash-opener is verbatim data, never escape-processed"
        );
    }

    #[test]
    fn shared_scanner_is_wired_and_quote_aware() {
        // Smoke that the runtime resolves islands through the ONE shared lexer
        // (exhaustively tested in nika-tmpl); a `}}` inside a literal is body.
        assert_eq!(nika_tmpl::find_island_close(" vars.x }}", 0), Some(8));
        assert_eq!(nika_tmpl::find_island_close(" '}}' }}", 0), Some(6));
        assert_eq!(nika_tmpl::find_island_close(" \"}}\" }}", 0), Some(6));
        assert_eq!(nika_tmpl::find_island_close(" no real close ", 0), None);
    }

    #[test]
    fn structural_error_precedes_semantic_on_doubly_malformed_template() {
        // Gate-11 (rust-pro P1 · ACCEPTED as intentional · pinned): a template
        // with BOTH a resolvable-but-erroring island AND a later UNTERMINATED
        // opener now surfaces the STRUCTURAL fault first — `render` pre-scans via
        // `nika_tmpl::scan_islands`, which errors on the unterminated opener
        // before any island is resolved. Structural-before-semantic is standard
        // parser discipline; both are hard errors either way, and a passed
        // `nika check` never reaches this (the checker rejects unterminated
        // islands). This locks the ordering as intentional, not accidental.
        let (records, vars) = fixture();
        let scope = Scope::workflow(&records, &vars);
        // island #1 `vars.nope` is unknown (a semantic fault); island #2 is
        // unterminated (a structural fault). The structural fault wins, and the
        // surfaced reference is the DANGLING body — not `vars.nope`.
        assert!(
            matches!(
                render("${{ inputs.nope }} ${{ dangling", &scope)
                    .expect_err("doubly-malformed template must error"),
                DataflowError::UnresolvedTemplate { ref reference } if reference == "dangling"
            ),
            "the structural (unterminated) fault must win over the earlier semantic one"
        );
    }

    #[test]
    fn render_close_is_quote_aware() {
        // check⇄runtime drift (sister to render_honors_backslash_escape): a
        // `}}` INSIDE a CEL string literal must NOT close the island early.
        // The static analyzer scans quote-aware (nika-schema template tests);
        // the runtime's naive `find("}}")` truncated `${{ … "}}" … }}`
        // mid-literal → a parse error on a template that checked clean.
        let (records, vars) = fixture(); // vars.source == "./news.json"
        let scope = Scope::workflow(&records, &vars);

        // RHS string literal carrying `}}` resolves (island not truncated).
        assert_eq!(
            render("${{ inputs.source == '}}' }}", &scope).expect("compare vs }} literal"),
            "false"
        );
        // `.contains('}}')` — the literal `}}` is method-arg data, not a close.
        assert_eq!(
            render("${{ inputs.source.contains('}}') }}", &scope).expect("contains }}"),
            "false"
        );
        // The REAL close (after the literal) is found, so a trailing real
        // island still scans: a literal-bearing island, then a normal one.
        assert_eq!(
            render(
                "${{ inputs.source != '}}' }} :: ${{ inputs.source }}",
                &scope
            )
            .expect("literal-island then real island"),
            "true :: ./news.json"
        );
    }

    #[test]
    fn injected_value_is_cel_data_never_re_evaluated() {
        // SECURITY (the load-bearing invariant): a task output / var that
        // CONTAINS expression-looking text is bound as a typed CEL DATA
        // value · the author's expression is parsed once · the injected
        // string is NEVER re-parsed as an expression. A payload that
        // WOULD resolve to something else (or trip a guard / inject a
        // reference) if re-evaluated must come back as the literal bytes.
        let payload = "${{ inputs.secret_admin == true }}"; // attacker-controlled
        let records = BTreeMap::from([(
            "evil".to_owned(),
            record(TaskStatus::Success, Value::String(payload.into())),
        )]);
        let vars = BTreeMap::from([
            (
                "tainted".to_owned(),
                Value::String("'; DROP ${{ x }}".into()),
            ),
            ("secret_admin".to_owned(), serde_json::json!(false)),
        ]);
        let scope = Scope::workflow(&records, &vars);

        // render(): the island resolves to the OUTPUT VALUE (the payload
        // string verbatim) · the `${{` inside is data, not re-scanned.
        assert_eq!(
            render("${{ tasks.evil.output }}", &scope).expect("data"),
            payload,
            "an injected `${{` payload renders as the literal value"
        );

        // render_json() single-island: same · the value is type-preserved
        // and NOT re-evaluated (it does NOT become a bool from the payload).
        let v = render_json(&serde_json::json!("${{ tasks.evil.output }}"), &scope).expect("data");
        assert_eq!(v, Value::String(payload.to_owned()));

        // eval_when(): a gate that READS the tainted/payload value as DATA
        // (string comparison) evaluates against the literal bytes — the
        // embedded `${{ x }}` is never resolved, so the equality is honest.
        assert!(
            eval_when(
                "${{ tasks.evil.output == \"${{ inputs.secret_admin == true }}\" }}",
                &scope
            )
            .expect("compares the literal payload bytes"),
            "the gate compares the payload as DATA · the inner island is never evaluated"
        );
        // And a tainted var compared as data is a plain (false) string compare,
        // never a re-parse of its `${{ x }}` fragment.
        assert!(!eval_when("${{ inputs.tainted == 'admin' }}", &scope).expect("string compare"),);
    }

    #[test]
    fn render_json_for_each_deep_path_array_resolves() {
        // `for_each: ${{ tasks.files.output }}` over a deep-path array
        // (the bug: a deep-path array resolved to a string → VAR-006).
        // render_json single-island preserves the ARRAY type.
        let records = BTreeMap::from([(
            "files".to_owned(),
            record(
                TaskStatus::Success,
                serde_json::json!({"paths": ["a.md", "b.md", "c.md"]}),
            ),
        )]);
        let vars = BTreeMap::new();
        let scope = Scope::workflow(&records, &vars);
        // A flat task-output array.
        let flat = BTreeMap::from([(
            "list".to_owned(),
            record(TaskStatus::Success, serde_json::json!(["x", "y"])),
        )]);
        let flat_scope = Scope::workflow(&flat, &vars);
        assert_eq!(
            render_json(&serde_json::json!("${{ tasks.list.output }}"), &flat_scope)
                .expect("flat array"),
            serde_json::json!(["x", "y"])
        );
        // A DEEP-path array stays an array (type-preserving · the fan-out
        // collection relies on this).
        assert_eq!(
            render_json(
                &serde_json::json!("${{ tasks.files.output.paths }}"),
                &scope
            )
            .expect("deep array"),
            serde_json::json!(["a.md", "b.md", "c.md"])
        );
    }

    #[test]
    fn item_field_access_resolves_in_for_each_body() {
        // The t3-localization shape: `${{ item.path }}` / `${{ item.text }}`
        // — for_each locals are objects too · field access works.
        let (records, vars) = fixture();
        let item = serde_json::json!({"path": "docs/intro.md", "text": "Hello"});
        let scope = Scope {
            records: &records,
            inputs: &vars,
            consts: &EMPTY_ENV,
            secrets: &EMPTY_SECRETS,
            with_ns: None,
            item: Some(&item),
            index: Some(0),
            permits: None,
        };
        assert_eq!(
            render("${{ item.path }}", &scope).expect("item.path"),
            "docs/intro.md"
        );
        assert_eq!(
            render("${{ item.text }}", &scope).expect("item.text"),
            "Hello"
        );
    }

    #[test]
    fn ternary_value_substitution_resolves() {
        // A ternary in a VALUE position (model/prompt/path selection).
        let vars = BTreeMap::from([("env".to_owned(), Value::String("prod".into()))]);
        let records = BTreeMap::new();
        let scope = Scope::workflow(&records, &vars);
        assert_eq!(
            render(
                "${{ inputs.env == 'prod' ? 'anthropic/claude' : 'ollama/llama3' }}",
                &scope
            )
            .expect("ternary value"),
            "anthropic/claude"
        );
        // has() guard form (the `prompt:` default-fallback idiom).
        assert_eq!(
            render("${{ has(vars.style) ? vars.style : 'be concise' }}", &scope)
                .expect("has fallback"),
            "be concise"
        );
    }

    #[test]
    fn render_json_single_island_preserves_value_type() {
        let (records, vars) = fixture();
        let scope = Scope::workflow(&records, &vars);
        // Exactly-one-island string leaf → the VALUE (array stays array).
        let v = render_json(&serde_json::json!("${{ inputs.urls }}"), &scope).expect("renders");
        assert_eq!(v, serde_json::json!(["a", "b"]));
        // Mixed text → textual render.
        let v = render_json(&serde_json::json!("x ${{ inputs.source }}"), &scope).expect("renders");
        assert_eq!(v, serde_json::json!("x ./news.json"));
        // Nested leaves resolve · non-strings untouched.
        let v = render_json(
            &serde_json::json!({"path": "${{ inputs.source }}", "n": [7, true]}),
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
            inputs: &vars,
            consts: &EMPTY_ENV,
            secrets: &EMPTY_SECRETS,
            with_ns: Some(&with_ns),
            item: Some(&item),
            index: Some(3),
            permits: None,
        };
        assert_eq!(
            render("${{ with.page }}/${{ item }}@${{ index }}", &scope).expect("renders"),
            "p1/element@3"
        );
        // Outside an iteration the locals are unknown (1702).
        let bare = Scope::workflow(&records, &vars);
        assert!(matches!(
            render("${{ item }}", &bare).expect_err("no item outside for_each"),
            DataflowError::UnresolvedTemplate { .. }
        ));
    }

    #[test]
    fn when_equality_open_and_closed() {
        let (records, vars) = fixture();
        let scope = Scope::workflow(&records, &vars);
        assert!(!eval_when("${{ inputs.publish == 'yes' }}", &scope).expect("closed"));
        assert!(eval_when("${{ inputs.publish == 'no' }}", &scope).expect("open"));
        assert!(eval_when("${{ inputs.publish != 'yes' }}", &scope).expect("negated open"));
        assert!(eval_when("inputs.publish == \"no\"", &scope).expect("bare body · double quotes"));
        // Status comparisons (spec 04 · the always-pattern's idiom).
        assert!(eval_when("${{ tasks.gather.status == 'success' }}", &scope).expect("status"));
        assert!(eval_when("${{ tasks.ghosted.status != 'success' }}", &scope).expect("skipped"));
    }

    #[test]
    fn when_full_cel_gate_forms_evaluate() {
        // The CEL milestone: the gate is the full cel-subset/0.1 — not a
        // bare-ref truthiness table. Comparisons (`<` `>=` `!=`), deep
        // boolean paths, `size()`, `.contains()`, booleans, membership,
        // and the ternary all evaluate.
        let records = BTreeMap::from([(
            "check".to_owned(),
            record(
                TaskStatus::Success,
                serde_json::json!({"valid": true, "coverage": 92, "tags": ["a", "b"]}),
            ),
        )]);
        let vars = BTreeMap::from([
            ("env".to_owned(), Value::String("production".into())),
            ("threshold".to_owned(), serde_json::json!(80)),
            ("msg".to_owned(), Value::String("error: boom".into())),
        ]);
        let scope = Scope::workflow(&records, &vars);
        // Deep-path boolean compare (the NIKA-1703 bug this fix closes).
        assert!(eval_when("${{ tasks.check.output.valid == true }}", &scope).expect("deep bool"));
        // Numeric comparisons.
        assert!(eval_when("${{ tasks.check.output.coverage > 80 }}", &scope).expect("> "));
        assert!(eval_when("${{ inputs.threshold >= 80 }}", &scope).expect(">="));
        assert!(eval_when("${{ tasks.check.output.coverage != 0 }}", &scope).expect("!="));
        assert!(!eval_when("${{ tasks.check.output.coverage < 80 }}", &scope).expect("<"));
        // size() + .contains() + membership + boolean ops + ternary.
        assert!(eval_when("${{ size(tasks.check.output.tags) > 0 }}", &scope).expect("size"));
        assert!(eval_when("${{ inputs.msg.contains('error') }}", &scope).expect("contains"));
        assert!(
            eval_when(
                "${{ inputs.env == 'production' && tasks.check.output.valid }}",
                &scope
            )
            .expect("&&")
        );
        assert!(eval_when("${{ inputs.env in ['staging', 'production'] }}", &scope).expect("in"));
        assert!(
            eval_when("${{ inputs.env == 'production' ? true : false }}", &scope).expect("ternary")
        );
        // The bare YAML-boolean idiom (a wrapped `true`/`false` literal).
        assert!(eval_when("${{ true }}", &scope).expect("literal true"));
    }

    #[test]
    fn when_non_boolean_value_is_var_006() {
        // Spec 03 §when shape: a `when:` that evaluates non-boolean is
        // NIKA-VAR-006 (carried as CelEval) — NOT a silent skip, NOT
        // the old string-truthiness. (A non-boolean ROOT is the
        // checker's NIKA-VAR-005; the runtime sees the eval-time class.)
        let vars = BTreeMap::from([
            ("count".to_owned(), serde_json::json!(3)),
            ("name".to_owned(), Value::String("nika".into())),
        ]);
        let records = BTreeMap::new();
        let scope = Scope::workflow(&records, &vars);
        for non_bool in [
            "${{ inputs.count }}",
            "${{ inputs.name }}",
            "${{ size(inputs.name) }}",
        ] {
            let err = eval_when(non_bool, &scope).expect_err(non_bool);
            assert!(
                matches!(&err, DataflowError::CelEval { code, .. } if *code == "NIKA-VAR-006"),
                "{non_bool} → {err}"
            );
        }
        // A cross-type compare is also VAR-006 (no silent `false`).
        let err = eval_when("${{ inputs.count == 'three' }}", &scope).expect_err("cross-type");
        assert!(matches!(
            &err,
            DataflowError::CelEval { code, .. } if *code == "NIKA-VAR-006"
        ));
    }

    #[test]
    fn when_grammar_violation_is_nika_1703() {
        // Genuinely-out-of-grammar forms (NIKA-VAR-005) are 1703 — the
        // runtime backstop (the checker is the primary site). Spec-VALID
        // forms must NEVER reach here (that is the bug this fix closes).
        let (records, vars) = fixture();
        let scope = Scope::workflow(&records, &vars);
        for malformed in [
            "${{ inputs.a < vars.b < vars.c }}", // chained relation
            "${{ frobnicate(vars.x) }}",         // unknown function
            "${{ inputs.a && }}",                // dangling operator
            "${{ inputs.a @ vars.b }}",          // bad token
        ] {
            let err = eval_when(malformed, &scope).expect_err(malformed);
            assert!(
                matches!(err, DataflowError::WhenUnsupported { .. }),
                "{malformed} → {err}"
            );
        }
        // An UNQUOTED right-hand `yes` is a valid IDENT (an unbound root)
        // → unresolved (1702), NOT a grammar error · `yes`/`no` are not
        // reserved words in cel-subset/0.1 (only true/false/null/in are).
        assert!(matches!(
            eval_when("${{ inputs.publish == yes }}", &scope).expect_err("unbound rhs ident"),
            DataflowError::UnresolvedTemplate { .. }
        ));
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
            // A `\${{` escape legitimately renders a LITERAL `${{` (spec 04
            // §Escaping), so the no-leak invariant only holds when the
            // template carries no escape — a real island is always resolved
            // away or errs, so a surviving `${{` then means a silent leak.
            if let Ok(out) = render(&template, &scope)
                && template.contains("${{")
                && !template.contains("\\${{")
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
