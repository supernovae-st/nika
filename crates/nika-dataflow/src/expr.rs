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

use nika_cap::Permits;
use nika_cel::{Resolver, compute, compute_bool, parse};
use nika_tmpl::{scan_islands, single_island};
use serde_json::Value;

use crate::ValueBound;
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
///
/// Authority maps are private and every production scope starts at
/// [`Self::workflow_with_value_authorities`]. Task-local context can only be
/// layered onto that authority-bearing value through
/// [`Self::with_task_context`]; an external crate cannot forge a scope with a
/// struct literal or replace an authority map after construction.
///
/// ```compile_fail
/// use nika_dataflow::Scope;
///
/// fn forge<'a>(valid: Scope<'a>) -> Scope<'a> {
///     Scope { ..valid }
/// }
/// ```
#[derive(Clone, Copy)]
#[non_exhaustive]
pub struct Scope<'a> {
    /// `tasks.<id>.<field>` — the result records of settled tasks.
    records: &'a BTreeMap<String, TaskRecord>,
    /// `inputs.<key>` — typed input defaults + the operator's `--var`
    /// overrides (JSON values).
    inputs: &'a BTreeMap<String, Value>,
    /// `const.<key>` — named constants (bare literals + typed `{type,
    /// value}` values · always bound, never overridable).
    consts: &'a BTreeMap<String, Value>,
    /// `secrets.<name>` — RESOLVED secret values (spec 01 §secrets ·
    /// MINOR-B). Empty when the composer injected no resolver — then a
    /// `secrets.X` reference is unresolved (NIKA-1702), never a silent
    /// empty. A resolved value flows ONLY where the IFC sanctions it (the
    /// `nika check` secret-flow analysis · ADR-092) and is NEVER emitted to
    /// the event stream (notes carry the program/model, not field values).
    secrets: &'a BTreeMap<String, Value>,
    /// `with.<key>` — task-local injection (None outside a task body).
    with_ns: Option<&'a BTreeMap<String, Value>>,
    /// `item` / `index` — `for_each` locals (None outside an iteration).
    item: Option<&'a Value>,
    /// Zero-based position of `item` (spec 03 §`for_each`).
    index: Option<usize>,
    /// `permits:` — the workflow's declared capability boundary (spec 01
    /// §permits). `None` = no boundary declared (F-O8 « absent = zero
    /// authority » · every effect refused at the gates · the always-on
    /// floors stay on top). Set on the task-dispatch scopes so the exec
    /// sink can enforce `permits.exec` (NIKA-SEC-004); `None` elsewhere.
    permits: Option<&'a Permits>,
    /// `group.<name>` — the fan-in fold's DECLARED membership (spec 03
    /// §group): group name → its member task ids in DECLARATION order
    /// (the source order of `tasks:`, never completion order, so the
    /// fold's shape is stable across re-runs). Set on the `with:` render
    /// scopes — the fold's ONE legal door (spec 04 §the reference
    /// boundary). `None` = nothing bound: a `group.X` read is unresolved
    /// (NIKA-1702 · loud), never a silent empty array.
    groups: Option<&'a BTreeMap<String, Vec<String>>>,
    /// The bound a rehearsal room puts on every value this scope's evaluations build;
    /// `None` outside a room.
    bound: Option<&'a ValueBound>,
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
            groups: None,
            bound: None,
        }
    }

    /// Add the task-local namespace, loop locals, and effect boundary to an
    /// authority-bearing workflow scope.
    ///
    /// This consumes `self`, so these contextual values can never be used to
    /// bypass the required records/inputs/constants/secrets constructor.
    #[must_use]
    pub fn with_task_context(
        mut self,
        with_ns: Option<&'a BTreeMap<String, Value>>,
        item: Option<&'a Value>,
        index: Option<usize>,
        permits: Option<&'a Permits>,
    ) -> Self {
        self.with_ns = with_ns;
        self.item = item;
        self.index = index;
        self.permits = permits;
        self
    }

    /// Count every value this scope's evaluations build against `bound`, a rehearsal room's: a
    /// value over it is refused ([`DataflowError::ValueBound`]); `None` builds as before.
    #[must_use]
    pub fn with_value_bound(mut self, bound: Option<&'a ValueBound>) -> Self {
        self.bound = bound;
        self
    }

    /// Bind the fan-in fold's DECLARED membership (spec 03 §group) —
    /// group name → member task ids in declaration order — so a
    /// `${{ group.<name> }}` island resolves to one array of member
    /// records. Membership is declared, never matched: the caller
    /// derives it from the `group:` keys of `tasks:`, exactly as the
    /// checker derives the `fan-in` edges. Only the `with:` renders bind
    /// it (the fold's one door); everywhere else the checker already
    /// refused the reference, and an unbound root stays loud.
    #[must_use]
    pub fn with_groups(mut self, groups: &'a BTreeMap<String, Vec<String>>) -> Self {
        self.groups = Some(groups);
        self
    }

    /// The settled task records bound at construction.
    #[must_use]
    pub const fn records(&self) -> &'a BTreeMap<String, TaskRecord> {
        self.records
    }

    /// The input values bound at construction.
    #[must_use]
    pub const fn inputs(&self) -> &'a BTreeMap<String, Value> {
        self.inputs
    }

    /// The constant values bound at construction.
    #[must_use]
    pub const fn consts(&self) -> &'a BTreeMap<String, Value> {
        self.consts
    }

    /// The resolved secret values bound at construction.
    #[must_use]
    pub const fn secrets(&self) -> &'a BTreeMap<String, Value> {
        self.secrets
    }

    /// The optional task-local `with:` namespace.
    #[must_use]
    pub const fn with_namespace(&self) -> Option<&'a BTreeMap<String, Value>> {
        self.with_ns
    }

    /// The optional `for_each` item.
    #[must_use]
    pub const fn item(&self) -> Option<&'a Value> {
        self.item
    }

    /// The optional `for_each` index.
    #[must_use]
    pub const fn index(&self) -> Option<usize> {
        self.index
    }

    /// The optional workflow capability boundary.
    #[must_use]
    pub const fn permits(&self) -> Option<&'a Permits> {
        self.permits
    }

    /// The bound a rehearsal room put on the values this scope builds, if any.
    #[must_use]
    pub const fn value_bound(&self) -> Option<&'a ValueBound> {
        self.bound
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
            // `group` → an object of per-name FOLDS: one array of member
            // records in DECLARATION order (spec 03 §the member record).
            // The plural reader of `tasks`, bound only where a `with:`
            // renders (`None` elsewhere → unresolved · loud). A fold with
            // an unsettled member is ABSENT from the object, so `.name`
            // raises NIKA-VAR-001 — never a silently smaller array.
            "group" => scope
                .groups
                .map(|groups| groups_object(groups, scope.records)),
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

/// The `group` root as a CEL object — one entry per declared group whose
/// EVERY member has settled, each the fold: the member records in
/// DECLARATION order (spec 03 §the member record). A group with no
/// members, or with a member still unsettled, is left out — the fold
/// can never harvest a partial set and read as clean (the checker's
/// waves order every member before the fold, so a miss here is a
/// schedule breach and surfaces as NIKA-VAR-001, loud).
fn groups_object(
    groups: &BTreeMap<String, Vec<String>>,
    records: &BTreeMap<String, TaskRecord>,
) -> Value {
    let mut out = serde_json::Map::new();
    for (name, members) in groups {
        if members.is_empty() {
            continue;
        }
        let fold: Option<Vec<Value>> = members
            .iter()
            .map(|id| records.get(id).map(|rec| member_record(id, rec)))
            .collect();
        if let Some(fold) = fold {
            out.insert(name.clone(), Value::Array(fold));
        }
    }
    Value::Object(out)
}

/// One member's record inside a fold (spec 03 §the member record · the
/// set is CLOSED at v1): `id` is the one field a fold cannot reconstruct
/// from the others; `status` and `output` are both required and neither
/// is redundant (a skipped leg's red/green fact survives only inside the
/// output); `duration_ms` and `error` read defined-null.
fn member_record(id: &str, rec: &TaskRecord) -> Value {
    const FIELDS: [&str; 4] = ["status", "output", "duration_ms", "error"];
    let mut map: serde_json::Map<String, Value> = FIELDS
        .iter()
        .map(|f| ((*f).to_owned(), rec.field(f).unwrap_or(Value::Null)))
        .collect();
    map.insert("id".to_owned(), Value::String(id.to_owned()));
    Value::Object(map)
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
        crate::bound::fits(scope.bound, out.len())?;
        cursor = isl.end;
    }
    push_unescaped(&mut out, &text[cursor..]);
    crate::bound::fits(scope.bound, out.len())?;
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
    let built = build_json(value, scope)?;
    if scope.bound.is_some() {
        crate::bound::fits(scope.bound, crate::bound::json_size(&built))?;
    }
    Ok(built)
}

/// [`render_json`]'s walk: each leaf rendered, the containers rebuilt around them.
fn build_json(value: &Value, scope: &Scope<'_>) -> Result<Value, DataflowError> {
    Ok(match value {
        Value::String(text) => match single_island(text) {
            Some(reference) => scope.resolve_expr(reference)?,
            None => Value::String(render(text, scope)?),
        },
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| build_json(v, scope))
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| Ok((k.clone(), build_json(v, scope)?)))
                .collect::<Result<_, _>>()?,
        ),
        other => other.clone(),
    })
}

/// What a template shows, structured ([`shown`]): each value an island renders, by key, and the
/// keys whose value was withheld, each with why (`secret`, `size`, `unresolved`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Shown {
    /// The typed values shown, by key.
    pub values: serde_json::Map<String, Value>,
    /// The keys withheld, each with why.
    pub withheld: BTreeMap<String, String>,
}

/// The values `text` renders over `scope`, one per `${{ … }}` island, each resolved to its typed
/// value as [`render_json`] binds a lone island. Its key is the last name its reference reads
/// (`with.request.amount` → `amount`), else the reference's own text: when the reference is no
/// plain path, or that name is another reference's. A value whose text `secret` recognizes is
/// withheld as `secret`; one past `bounds.0` bytes of compact JSON, or past `bounds.1` with the
/// values already kept, as `size`; an island that does not resolve as `unresolved`. A template
/// that does not scan shows nothing.
#[must_use]
pub fn shown(
    text: &str,
    scope: &Scope<'_>,
    bounds: (usize, usize),
    secret: &dyn Fn(&str) -> bool,
) -> Shown {
    let (max_each, max_all) = bounds;
    let mut shown = Shown::default();
    let (mut taken, mut kept) = (BTreeMap::<String, String>::new(), 0);
    for island in scan_islands(text).unwrap_or_default() {
        let reference = island.body.trim();
        let ident = |s: &str| {
            s.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        };
        let last = reference.rsplit('.').next().unwrap_or(reference);
        let name = if reference.split('.').all(ident) {
            last
        } else {
            reference
        };
        let key = match taken.get(name) {
            Some(other) if other != reference => reference.to_owned(),
            _ => name.to_owned(),
        };
        taken
            .entry(key.clone())
            .or_insert_with(|| reference.to_owned());
        if shown.values.contains_key(&key) || shown.withheld.contains_key(&key) {
            continue;
        }
        let why = match scope.resolve_expr(reference) {
            Err(_) => "unresolved",
            Ok(value) => {
                let json = value.to_string();
                if secret(json.as_str()) || value.as_str().is_some_and(secret) {
                    "secret"
                } else if json.len() > max_each || kept + json.len() > max_all {
                    "size"
                } else {
                    kept += json.len();
                    shown.values.insert(key, value);
                    continue;
                }
            }
        };
        shown.withheld.insert(key, why.to_owned());
    }
    shown
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
mod tests;
