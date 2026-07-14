// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika:decide` — the deterministic decision kernel (spec 11 · G14-G18).
//!
//! `D = Evaluate(Bundle, EvidenceSnapshot)` — deterministic, explained,
//! appealable. **The model never decides**: `infer:`/`agent:` produce
//! closed cited facts; this kernel applies the rubric.
//!
//! The semantics belong to the BUNDLE, not to this engine: the
//! stdlib-Python reference interpreter (`conformance/decision_core.py`
//! in nika-spec) is the conformance oracle, and every receipt here is
//! BYTE-EQUAL canonical JSON against it on the bundle's own fixtures
//! (G18 · the second-evaluator law · the goldens test in
//! `decide/tests.rs`). Mirror discipline, where it bites:
//!
//! - Python `//` is FLOOR division — on a negative dividend it rounds
//!   toward −∞ where Rust `/` truncates toward 0; every fixed-point
//!   division here is `div_euclid` (the divisor is the positive 10000).
//! - duplicate snapshot keys score LAST-wins (the reference's dict
//!   comprehension) while the Conflict detector reads EVERY claim.
//! - a `3.0` float fits the `integer` TYPE (spec 09) yet scores as
//!   UNKNOWN (an interval) — scoring is stricter than fitting, exactly
//!   like the reference's `isinstance(int)` gate.
//! - the missing-keys determination line carries Python's list `repr`
//!   spelling verbatim (`['key']`) — the receipt bytes are the law.
//!
//! Fixed-point everywhere (spec 11 §decision IR): weights and
//! thresholds are integer basis-points; a JSON number with a fraction
//! is refused. Arithmetic rides `i128` internally (Python ints are
//! unbounded); every receipt integer still narrows to `i64` to stay a
//! JSON integer — beyond that is a deterministic refusal, never a wrap.
//!
//! Error planes:
//! - `NIKA-BUILTIN-DECIDE-001` — the ARG plane (bad `bundle:`/`evidence:`
//!   shapes · unreadable bundle path), like every builtin;
//! - `NIKA-DECIDE-001` — the bundle is malformed or violates its own laws;
//! - `NIKA-DECIDE-002` — the snapshot does not satisfy the evidence schema.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use nika_kernel::io::fs::FsReadDyn;
use nika_types::types::parse_type;
use serde_json::{Map, Value, json};

use crate::permits::{FsAccess, FsBoundary};
use crate::{Args, BuiltinFailure, BuiltinOutcome};

#[cfg(test)]
mod tests;

/// The arg-plane code (`NIKA-BUILTIN-<NAME>-NNN` · like every builtin).
const ARG_CODE: &str = "NIKA-BUILTIN-DECIDE-001";
/// The bundle-law namespace code (spec 11 · `validation_error`).
const BUNDLE_CODE: &str = "NIKA-DECIDE-001";
/// The snapshot-law namespace code (spec 11 · `validation_error`).
const SNAPSHOT_CODE: &str = "NIKA-DECIDE-002";

/// The integrity lattice, weakest → strongest (spec 11 §evidence IR).
const INTEGRITY: [&str; 4] = ["untrusted", "observed", "verified", "authoritative"];
/// The closed outcome enum (spec 11 §outcomes · G16).
const OUTCOMES: [&str; 5] = [
    "recommend",
    "defer",
    "human_required",
    "opted_out",
    "overridden",
];
/// The v1 transform kinds (spec 11 §the decision bundle).
const TRANSFORM_KINDS: [&str; 3] = ["clamp", "linear", "bucket"];
/// The closed fixture classes (spec 11 §the decision bundle).
const FIXTURE_CLASSES: [&str; 5] = [
    "positive",
    "negative",
    "ambiguous",
    "contradictory",
    "adversarial",
];
/// The fixed-point denominator — basis points.
const BP: i128 = 10_000;

/// One decision refusal — WHICH law family refused (the two spec codes).
/// Both are deterministic: same inputs, same refusal, both evaluators.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecideError {
    /// `NIKA-DECIDE-001` — the bundle is malformed or violates its own laws.
    Bundle(String),
    /// `NIKA-DECIDE-002` — the snapshot does not satisfy the evidence schema.
    Snapshot(String),
}

impl From<DecideError> for BuiltinFailure {
    fn from(e: DecideError) -> Self {
        match e {
            DecideError::Bundle(msg) => Self::new(BUNDLE_CODE, msg),
            DecideError::Snapshot(msg) => Self::new(SNAPSHOT_CODE, msg),
        }
    }
}

type DResult<T> = Result<T, DecideError>;

/// Scored dimensions — the receipt's `dimensions` map paired with the
/// per-dimension `(lo, hi)` intervals the threshold ladder reads.
type ScoredDimensions = (Map<String, Value>, BTreeMap<String, (i128, i128)>);

fn bundle_law(msg: impl Into<String>) -> DecideError {
    DecideError::Bundle(msg.into())
}

fn snapshot_law(msg: impl Into<String>) -> DecideError {
    DecideError::Snapshot(msg.into())
}

// ─── the builtin entry (arg plane) ──────────────────────────────────────

/// `nika:decide` — resolve the `bundle:` form (a path string, gated by
/// the declared `permits.fs` READ boundary exactly like any fs.read, or
/// the inline bundle object), require the `evidence:` snapshot object,
/// then run the pure kernel. The receipt is the success value.
pub(crate) async fn run<F: FsReadDyn>(
    fs: &F,
    boundary: &FsBoundary,
    args: &Args,
) -> BuiltinOutcome {
    let bundle = match args.get("bundle") {
        Some(Value::String(path)) => {
            // The one effectful form: a bundle path read is declared like
            // any `fs.read` (spec 11 §nika:decide) — the capability
            // boundary, not the I/O, is the gate (NIKA-SEC-004 on escape).
            boundary.enforce(fs, path, FsAccess::Read).await?;
            let text = fs.read_to_string(Path::new(path)).await.map_err(|e| {
                BuiltinFailure::new(
                    ARG_CODE,
                    format!("`bundle:` path `{path}` is unreadable — {e}"),
                )
            })?;
            serde_json::from_str::<Value>(&text).map_err(|e| {
                DecideError::Bundle(format!(
                    "bundle `{path}` · not valid JSON — {e} (a bundle is ONE \
                     JSON document · spec 11 §the decision bundle)"
                ))
            })?
        }
        Some(inline @ Value::Object(_)) => inline.clone(),
        Some(other) => {
            return Err(BuiltinFailure::new(
                ARG_CODE,
                format!(
                    "`bundle:` takes a ./path string or the inline Decision \
                     Bundle object, not {other}"
                ),
            ));
        }
        None => {
            return Err(BuiltinFailure::new(
                ARG_CODE,
                "`bundle:` (a path string or the inline bundle object) is required",
            ));
        }
    };
    let snapshot = match args.get("evidence") {
        Some(snap @ Value::Object(_)) => snap.clone(),
        Some(other) => {
            return Err(BuiltinFailure::new(
                ARG_CODE,
                format!(
                    "`evidence:` is the EvidenceSnapshot object \
                     `{{ t, evidence: [...] }}`, not {other}"
                ),
            ));
        }
        None => {
            return Err(BuiltinFailure::new(
                ARG_CODE,
                "`evidence:` (the EvidenceSnapshot object) is required",
            ));
        }
    };
    Ok(evaluate(&bundle, &snapshot)?)
}

// ─── the kernel · D = Evaluate(Bundle, Snapshot) ────────────────────────

/// `D = Evaluate(Bundle, EvidenceSnapshot)` — the full Decision Receipt,
/// deterministically (spec 11 · the mirror of
/// `decision_core.py::evaluate`, byte-equal on canonical JSON · G18).
///
/// The receipt's canonical form is `serde_json::to_string` of the
/// returned value (sorted keys · compact · raw UTF-8 — exactly the
/// reference's spelling). Same inputs, same bytes, both evaluators.
///
/// # Errors
///
/// [`DecideError::Bundle`] (`NIKA-DECIDE-001`) when the bundle is
/// malformed or violates its own laws (fixed-point · closed rules ·
/// identity invariance · mandatory contradictory fixture · declared
/// monotonicity checked on the bundle's own fixtures) ·
/// [`DecideError::Snapshot`] (`NIKA-DECIDE-002`) when the snapshot does
/// not satisfy the evidence schema (type misfit · unauthorized source ·
/// integrity below the declared floor · undeclared key). Both refusals
/// are deterministic — never partial receipts.
pub fn evaluate(bundle: &Value, snapshot: &Value) -> Result<Value, DecideError> {
    let b = bundle.as_object().ok_or_else(|| {
        bundle_law("bundle · must be a JSON object (spec 11 §the decision bundle)")
    })?;
    let s = snapshot.as_object().ok_or_else(|| {
        snapshot_law("evidence snapshot · must be a JSON object { t, evidence: [...] }")
    })?;
    validate_bundle(b)?;
    let missing = validate_snapshot(b, s)?;
    let conflicts = conflicts(b, s)?;
    // Last-wins on duplicate keys for SCORING (the reference's dict
    // comprehension) — the Conflict detector above read every claim.
    let evidence = snapshot_evidence(s)?;
    let mut items: BTreeMap<&str, &Map<String, Value>> = BTreeMap::new();
    for e in &evidence {
        if let Some(key) = e.get("key").and_then(Value::as_str) {
            items.insert(key, e);
        }
    }
    let (dims_out, intervals) = score_dimensions(b, &items)?;
    let (outcome, determination) = decide_outcome(b, &conflicts, &missing, &intervals)?;
    Ok(json!({
        "decision_receipt_format": 1,
        "bundle": {
            "id": field_or_null(region_object(b, "manifest")?, "id"),
            "version": field_or_null(region_object(b, "manifest")?, "version"),
            "digest": field_or_null(region_object(b, "manifest")?, "digest"),
        },
        "snapshot": {
            "t": s.get("t").cloned().unwrap_or(Value::Null),
            "digests": sorted_digests(&evidence),
            "missing": missing,
        },
        "dimensions": dims_out,
        "conflicts": conflicts,
        "outcome": outcome,
        "determination_provenance": determination,
    }))
}

/// A manifest field, `null` when absent — the reference's `.get(...)`.
fn field_or_null(m: &Map<String, Value>, key: &str) -> Value {
    m.get(key).cloned().unwrap_or(Value::Null)
}

// ─── bundle validation (NIKA-DECIDE-001) ────────────────────────────────

/// The bundle's own laws (spec 11) — region presence, then region by
/// region, then the monotonicity property-check over the fixtures.
fn validate_bundle(b: &Map<String, Value>) -> DResult<()> {
    for region in [
        "manifest",
        "evidence_schema",
        "transforms",
        "rules",
        "governance",
        "fixtures",
    ] {
        if !b.contains_key(region) {
            return Err(bundle_law(format!(
                "bundle.{region} · mandatory region missing"
            )));
        }
    }
    validate_manifest(region_object(b, "manifest")?)?;
    let schema = region_object(b, "evidence_schema")?;
    validate_schema(schema)?;
    let transforms = region_object(b, "transforms")?;
    validate_transforms(transforms)?;
    validate_rules(region_object(b, "rules")?, schema, transforms)?;
    validate_governance(region_object(b, "governance")?)?;
    validate_fixtures(region_array(b, "fixtures")?)?;
    check_monotonicity_on_fixtures(b)
}

/// A mandatory object region (where the reference would attribute-crash
/// on a non-dict, this refuses deterministically — same law family).
fn region_object<'b>(b: &'b Map<String, Value>, key: &str) -> DResult<&'b Map<String, Value>> {
    b.get(key)
        .and_then(Value::as_object)
        .ok_or_else(|| bundle_law(format!("bundle.{key} · must be a JSON object")))
}

/// The mandatory array region (`fixtures`).
fn region_array<'b>(b: &'b Map<String, Value>, key: &str) -> DResult<&'b [Value]> {
    b.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| bundle_law(format!("bundle.{key} · must be a JSON array")))
}

fn validate_manifest(man: &Map<String, Value>) -> DResult<()> {
    for k in ["id", "version", "owner", "license"] {
        if man.get(k).and_then(Value::as_str).is_none_or(str::is_empty) {
            return Err(bundle_law(format!("manifest.{k} · required string")));
        }
    }
    Ok(())
}

fn validate_schema(schema: &Map<String, Value>) -> DResult<()> {
    if schema.is_empty() {
        return Err(bundle_law(
            "evidence_schema · non-empty map of evidence keys",
        ));
    }
    for (key, decl) in schema {
        let decl = decl
            .as_object()
            .ok_or_else(|| bundle_law(format!("evidence_schema.{key} · must be an object")))?;
        let t = decl.get("type").ok_or_else(|| {
            bundle_law(format!(
                "evidence_schema.{key}.type · required (a spec-09 type)"
            ))
        })?;
        // One voice with spec 09 — THE type parser, never a re-roll.
        parse_type(t, &BTreeSet::new(), &format!("evidence_schema.{key}.type"))
            .map_err(|e| bundle_law(e.detail))?;
        let floor = str_or(decl.get("integrity"), "observed");
        if !INTEGRITY.contains(&floor) {
            return Err(bundle_law(format!(
                "evidence_schema.{key}.integrity · not in the lattice: {floor}"
            )));
        }
    }
    Ok(())
}

fn validate_transforms(transforms: &Map<String, Value>) -> DResult<()> {
    for (name, tr) in transforms {
        let tr = tr
            .as_object()
            .ok_or_else(|| bundle_law(format!("transforms.{name} · must be an object")))?;
        let kind = str_or(tr.get("kind"), "");
        if !TRANSFORM_KINDS.contains(&kind) {
            return Err(bundle_law(format!(
                "transforms.{name}.kind · not a v1 transform (clamp · linear · bucket): {kind}"
            )));
        }
        let min = fixed_int(tr.get("min"), &format!("transforms.{name}.min"))?;
        let max = fixed_int(tr.get("max"), &format!("transforms.{name}.max"))?;
        if min > max {
            return Err(bundle_law(format!(
                "transforms.{name} · empty range: min > max"
            )));
        }
        if kind == "linear" {
            fixed_int_or(
                tr.get("scale_bp"),
                BP,
                &format!("transforms.{name}.scale_bp"),
            )?;
            fixed_int_or(tr.get("offset"), 0, &format!("transforms.{name}.offset"))?;
        }
        if kind == "bucket" {
            validate_bucket(name, tr)?;
        }
    }
    Ok(())
}

/// `bucket` law: `edges[n]` + `values[n+1]`, every entry a fixed-point
/// integer, edges sorted (the reference's check order: shape → ints →
/// sortedness).
fn validate_bucket(name: &str, tr: &Map<String, Value>) -> DResult<()> {
    let (Some(edges), Some(values)) = (
        tr.get("edges").and_then(Value::as_array),
        tr.get("values").and_then(Value::as_array),
    ) else {
        return Err(bundle_law(format!(
            "transforms.{name} · bucket needs edges[n] + values[n+1]"
        )));
    };
    if values.len() != edges.len() + 1 {
        return Err(bundle_law(format!(
            "transforms.{name} · bucket needs edges[n] + values[n+1]"
        )));
    }
    let mut parsed = Vec::with_capacity(edges.len());
    for e in edges {
        parsed.push(fixed_int(Some(e), &format!("transforms.{name}.edges[]"))?);
    }
    if parsed.windows(2).any(|w| w[0] > w[1]) {
        return Err(bundle_law(format!(
            "transforms.{name}.edges · must be sorted"
        )));
    }
    for v in values {
        fixed_int(Some(v), &format!("transforms.{name}.values[]"))?;
    }
    Ok(())
}

fn validate_rules(
    rules: &Map<String, Value>,
    schema: &Map<String, Value>,
    transforms: &Map<String, Value>,
) -> DResult<()> {
    let dims = rules
        .get("dimensions")
        .and_then(Value::as_object)
        .filter(|d| !d.is_empty())
        .ok_or_else(|| bundle_law("rules.dimensions · non-empty map"))?;
    for (dname, dim) in dims {
        for (i, term) in dimension_terms(dname, dim)?.iter().enumerate() {
            let where_ = format!("rules.dimensions.{dname}.terms[{i}]");
            let term = term
                .as_object()
                .ok_or_else(|| bundle_law(format!("{where_} · must be an object")))?;
            validate_term(&where_, term, schema, transforms)?;
        }
    }
    validate_thresholds(rules, dims)
}

/// One rule term: evidence key declared (never identity), transform
/// known, weight a fixed-point integer, monotonicity in the closed set.
fn validate_term(
    where_: &str,
    term: &Map<String, Value>,
    schema: &Map<String, Value>,
    transforms: &Map<String, Value>,
) -> DResult<()> {
    let key = str_or(term.get("evidence"), "");
    let Some(decl) = schema.get(key).and_then(Value::as_object) else {
        return Err(bundle_law(format!(
            "{where_}.evidence · undeclared key `{key}` — rules read only \
             evidence_schema keys (spec 11)"
        )));
    };
    // Python's `is True` — EXACTLY the JSON boolean, never truthiness.
    if decl.get("identity") == Some(&Value::Bool(true)) {
        return Err(bundle_law(format!(
            "{where_} · identity key `{key}` feeds a technical dimension — \
             identity counterfactual invariance (spec 11)"
        )));
    }
    let tname = str_or(term.get("transform"), "");
    if !transforms.contains_key(tname) {
        return Err(bundle_law(format!("{where_}.transform · unknown: {tname}")));
    }
    fixed_int(term.get("weight_bp"), &format!("{where_}.weight_bp"))?;
    let mono = str_or(term.get("monotonicity"), "none");
    if !["increases", "decreases", "none"].contains(&mono) {
        return Err(bundle_law(format!(
            "{where_}.monotonicity · not in the closed set: {mono}"
        )));
    }
    Ok(())
}

fn validate_thresholds(rules: &Map<String, Value>, dims: &Map<String, Value>) -> DResult<()> {
    for th in thresholds(rules)? {
        let dim = str_or(th.get("dimension"), "");
        if !dims.contains_key(dim) {
            return Err(bundle_law(format!(
                "rules.thresholds · unknown dimension `{dim}`"
            )));
        }
        fixed_int(
            th.get("recommend_gte_bp"),
            "rules.thresholds[].recommend_gte_bp",
        )?;
    }
    Ok(())
}

/// The `rules.thresholds` list (absent → empty · non-array refused where
/// the reference would crash iterating it).
fn thresholds(rules: &Map<String, Value>) -> DResult<Vec<&Map<String, Value>>> {
    match rules.get("thresholds") {
        None => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|th| {
                th.as_object()
                    .ok_or_else(|| bundle_law("rules.thresholds[] · must be an object"))
            })
            .collect(),
        Some(_) => Err(bundle_law("rules.thresholds · must be an array")),
    }
}

fn validate_governance(gov: &Map<String, Value>) -> DResult<()> {
    let Some(na) = gov.get("never_automatic") else {
        return Ok(());
    };
    let na = na
        .as_array()
        .ok_or_else(|| bundle_law("governance.never_automatic · must be an array of outcomes"))?;
    for o in na {
        if !o.as_str().is_some_and(|s| OUTCOMES.contains(&s)) {
            return Err(bundle_law(format!(
                "governance.never_automatic · not an outcome: {o}"
            )));
        }
    }
    Ok(())
}

/// Fixture laws: classes inside the closed set, and a CONTRADICTORY
/// fixture mandatory — a bundle that cannot prove its Conflict handling
/// is unpublishable (spec 11).
fn validate_fixtures(fixtures: &[Value]) -> DResult<()> {
    let mut classes: BTreeSet<&str> = BTreeSet::new();
    let mut bad: BTreeSet<String> = BTreeSet::new();
    for f in fixtures {
        let m = f
            .as_object()
            .ok_or_else(|| bundle_law("fixtures[] · must be an object"))?;
        match m.get("class").and_then(Value::as_str) {
            Some(c) if FIXTURE_CLASSES.contains(&c) => {
                classes.insert(c);
            }
            Some(c) => {
                bad.insert(c.to_owned());
            }
            None => {
                bad.insert(
                    m.get("class")
                        .map_or_else(|| "null".to_owned(), Value::to_string),
                );
            }
        }
    }
    if !bad.is_empty() {
        let listed: Vec<String> = bad.into_iter().collect();
        return Err(bundle_law(format!(
            "fixtures · unknown class(es): {}",
            listed.join(" · ")
        )));
    }
    if !classes.contains("contradictory") {
        return Err(bundle_law(
            "fixtures · a CONTRADICTORY fixture is mandatory — a bundle that \
             cannot prove its Conflict handling is unpublishable (spec 11)",
        ));
    }
    Ok(())
}

// ─── monotonicity · property-checked on the bundle's own fixtures ──────

/// Declared monotonicity is CHECKED, not prose (spec 11 · refused at
/// publication): for every fixture pair differing on ONE monotone key
/// (all else equal), the dimension's point score must move the declared
/// way. Mirror of `_check_monotonicity_on_fixtures`.
fn check_monotonicity_on_fixtures(b: &Map<String, Value>) -> DResult<()> {
    let dims = region_object(b, "rules")?
        .get("dimensions")
        .and_then(Value::as_object)
        .ok_or_else(|| bundle_law("rules.dimensions · non-empty map"))?;
    let fixtures: Vec<&Map<String, Value>> = region_array(b, "fixtures")?
        .iter()
        .filter_map(Value::as_object)
        .filter(|f| {
            matches!(
                f.get("class").and_then(Value::as_str),
                Some("positive" | "negative" | "ambiguous")
            )
        })
        .collect();
    for (dname, dim) in dims {
        for term in dimension_terms(dname, dim)?
            .iter()
            .filter_map(Value::as_object)
        {
            let mono = str_or(term.get("monotonicity"), "none");
            if mono == "none" {
                continue;
            }
            let key = str_or(term.get("evidence"), "");
            check_monotone_pairs(b, dname, key, mono, &fixtures)?;
        }
    }
    Ok(())
}

/// One (dimension · key · direction) over every fixture pair.
fn check_monotone_pairs(
    b: &Map<String, Value>,
    dname: &str,
    key: &str,
    mono: &str,
    fixtures: &[&Map<String, Value>],
) -> DResult<()> {
    for (i, fa) in fixtures.iter().enumerate() {
        for fb in &fixtures[i + 1..] {
            // Non-object `evidence` regions behave as key-absent (the
            // reference's `.get("evidence", {})` + `in` skip path).
            let (Some(ea), Some(eb)) = (
                fa.get("evidence").and_then(Value::as_object),
                fb.get("evidence").and_then(Value::as_object),
            ) else {
                continue;
            };
            let (Some(va), Some(vb)) = (ea.get(key), eb.get(key)) else {
                continue;
            };
            if json_int_eq(va, vb) {
                continue;
            }
            if !others_equal(ea, eb, key) {
                continue;
            }
            let sa = dimension_point_score(b, dname, ea)?;
            let sb = dimension_point_score(b, dname, eb)?;
            let (Some(sa), Some(sb)) = (sa, sb) else {
                continue;
            };
            // Point-scorable ⇒ the checked key's values are int-ish.
            let (Some(ia), Some(ib)) = (evidence_int(va), evidence_int(vb)) else {
                continue;
            };
            let rising = ib > ia;
            let expect_up = (mono == "increases") == rising;
            if (sb > sa) != expect_up && sb != sa {
                return Err(bundle_law(format!(
                    "rules.dimensions.{dname} · monotonicity({key}={mono}) \
                     violated by the bundle's OWN fixtures ({a}→{z}: {ia}→{ib} \
                     but score {sa}→{sb}) — refused at publication (spec 11)",
                    a = str_or(fa.get("name"), "?"),
                    z = str_or(fb.get("name"), "?"),
                )));
            }
        }
    }
    Ok(())
}

/// A dimension's `terms` list (absent → empty · the reference's
/// `dim.get("terms", [])` · non-list refused where it would crash).
fn dimension_terms<'d>(dname: &str, dim: &'d Value) -> DResult<&'d [Value]> {
    let dim = dim
        .as_object()
        .ok_or_else(|| bundle_law(format!("rules.dimensions.{dname} · must be an object")))?;
    match dim.get("terms") {
        None => Ok(&[]),
        Some(Value::Array(terms)) => Ok(terms.as_slice()),
        Some(_) => Err(bundle_law(format!(
            "rules.dimensions.{dname}.terms · must be an array"
        ))),
    }
}

/// Point score when every term's key is present int-ish (fixture
/// probing) — `None` = not probeable, the reference's
/// `_dimension_point_score`.
fn dimension_point_score(
    b: &Map<String, Value>,
    dname: &str,
    evidence_values: &Map<String, Value>,
) -> DResult<Option<i128>> {
    let rules = region_object(b, "rules")?;
    let transforms = region_object(b, "transforms")?;
    let Some(dim) = rules.get("dimensions").and_then(|d| d.get(dname)) else {
        return Ok(None);
    };
    let mut total: i128 = 0;
    for term in dimension_terms(dname, dim)?
        .iter()
        .filter_map(Value::as_object)
    {
        let key = str_or(term.get("evidence"), "");
        let Some(v) = evidence_values.get(key).and_then(evidence_int) else {
            return Ok(None);
        };
        let Some(tr) = term
            .get("transform")
            .and_then(Value::as_str)
            .and_then(|t| transforms.get(t))
            .and_then(Value::as_object)
        else {
            return Ok(None);
        };
        let t = apply_transform(tr, v)?;
        let w = fixed_int(term.get("weight_bp"), "rules · weight_bp")?;
        total += mul_bp(t, w)?;
    }
    Ok(Some(total))
}

// ─── snapshot validation (NIKA-DECIDE-002) ──────────────────────────────

/// The `NIKA-DECIDE-002` gate — EVERY item judged (a duplicate key is
/// two claims, both judged; a dict would swallow one and blind the
/// Conflict detector). Returns the MISSING required keys, sorted: they
/// are FACTS for abstention, never errors.
fn validate_snapshot(b: &Map<String, Value>, s: &Map<String, Value>) -> DResult<Vec<String>> {
    let schema = region_object(b, "evidence_schema")?;
    let mut present: BTreeSet<&str> = BTreeSet::new();
    for e in snapshot_evidence(s)? {
        let key = str_or(e.get("key"), "");
        let Some(decl) = schema.get(key).and_then(Value::as_object) else {
            return Err(snapshot_law(format!(
                "evidence.{key} · not a declared evidence key (the schema is \
                 the closed surface · NIKA-DECIDE-002)"
            )));
        };
        present.insert(key);
        validate_item(key, e, decl)?;
    }
    Ok(schema
        .iter()
        .filter(|(k, d)| {
            truthy(d.as_object().and_then(|m| m.get("required"))) && !present.contains(k.as_str())
        })
        .map(|(k, _)| k.clone())
        .collect())
}

/// One evidence item against its declaration: type fit (one voice with
/// spec 09), authorized source, integrity at or above the floor.
fn validate_item(key: &str, e: &Map<String, Value>, decl: &Map<String, Value>) -> DResult<()> {
    let ty_expr = decl
        .get("type")
        .ok_or_else(|| snapshot_law(format!("evidence_schema.{key}.type · required")))?;
    let ty = parse_type(
        ty_expr,
        &BTreeSet::new(),
        &format!("evidence_schema.{key}.type"),
    )
    .map_err(|err| snapshot_law(err.detail))?;
    let value = e.get("value").unwrap_or(&Value::Null);
    if !nika_types::types::fits(value, &ty, &BTreeMap::new()) {
        return Err(snapshot_law(format!(
            "evidence.{key} · value does not fit the declared type (spec 09 \
             fit · NIKA-DECIDE-002)"
        )));
    }
    if let Some(srcs) = decl.get("sources").and_then(Value::as_array) {
        let src = e.get("source").unwrap_or(&Value::Null);
        if !srcs.contains(src) {
            return Err(snapshot_law(format!(
                "evidence.{key} · source {src} is not authorized (declared: {})",
                Value::Array(srcs.clone())
            )));
        }
    }
    let floor = str_or(decl.get("integrity"), "observed");
    let got = str_or(e.get("integrity"), "untrusted");
    let Some(got_rank) = INTEGRITY.iter().position(|x| *x == got) else {
        return Err(snapshot_law(format!(
            "evidence.{key} · integrity `{got}` not in the lattice"
        )));
    };
    // The floor was bundle-validated into the lattice; `observed` is the
    // reference's default rank for the (unreachable) fallback.
    let floor_rank = INTEGRITY.iter().position(|x| *x == floor).unwrap_or(1);
    if got_rank < floor_rank {
        return Err(snapshot_law(format!(
            "evidence.{key} · integrity {got} below the declared floor {floor} \
             (NIKA-DECIDE-002)"
        )));
    }
    Ok(())
}

/// The snapshot's evidence items — absent → empty; every item MUST be an
/// object (the evidence IR · where the reference would attribute-crash).
fn snapshot_evidence(s: &Map<String, Value>) -> DResult<Vec<&Map<String, Value>>> {
    match s.get("evidence") {
        None => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|e| {
                e.as_object().ok_or_else(|| {
                    snapshot_law(
                        "evidence[] · every item must be an object (the evidence IR · spec 11)",
                    )
                })
            })
            .collect(),
        Some(_) => Err(snapshot_law(
            "evidence · must be an array of evidence items",
        )),
    }
}

// ─── Belnap Conflict detection ──────────────────────────────────────────

/// authoritative × authoritative disagreement on one key ⇒ Conflict with
/// a witness (both values · both sources · both digests) — the
/// reference's `_conflicts`: key-sorted output, witness source-sorted
/// (stable, like Python's `sorted`).
fn conflicts(b: &Map<String, Value>, s: &Map<String, Value>) -> DResult<Vec<Value>> {
    let schema = region_object(b, "evidence_schema")?;
    let mut by_key: BTreeMap<&str, Vec<&Map<String, Value>>> = BTreeMap::new();
    for e in snapshot_evidence(s)? {
        if let Some(key) = e.get("key").and_then(Value::as_str) {
            by_key.entry(key).or_default().push(e);
        }
    }
    let mut out = Vec::new();
    for (key, items) in by_key {
        let mut auth: Vec<&Map<String, Value>> = items
            .into_iter()
            .filter(|e| e.get("integrity").and_then(Value::as_str) == Some("authoritative"))
            .collect();
        let distinct: BTreeSet<String> = auth
            .iter()
            .map(|e| canonical_string(e.get("value").unwrap_or(&Value::Null)))
            .collect();
        if distinct.len() <= 1 {
            continue;
        }
        auth.sort_by_key(|e| py_str(e.get("source")));
        let witness: Vec<Value> = auth
            .iter()
            .map(|e| {
                json!({
                    "source": e.get("source").cloned().unwrap_or(Value::Null),
                    "value": e.get("value").cloned().unwrap_or(Value::Null),
                    "digest": e.get("digest").cloned().unwrap_or(Value::Null),
                })
            })
            .collect();
        out.push(json!({
            "key": key,
            "class": "unresolved",
            "required": truthy(
                schema
                    .get(key)
                    .and_then(Value::as_object)
                    .and_then(|d| d.get("required"))
            ),
            "witness": witness,
        }));
    }
    Ok(out)
}

// ─── scoring · intervals · contributions ────────────────────────────────

/// Every dimension's interval + term-by-term contributions (the
/// explanation IS the formula). A known int-ish value contributes a
/// point; an Unknown contributes the transform's [lo, hi] interval —
/// never an invented zero (spec 11 §four-valued logic).
fn score_dimensions(
    b: &Map<String, Value>,
    items: &BTreeMap<&str, &Map<String, Value>>,
) -> DResult<ScoredDimensions> {
    let rules = region_object(b, "rules")?;
    let transforms = region_object(b, "transforms")?;
    let dims = rules
        .get("dimensions")
        .and_then(Value::as_object)
        .ok_or_else(|| bundle_law("rules.dimensions · non-empty map"))?;
    let mut dims_out = Map::new();
    let mut intervals = BTreeMap::new();
    // serde_json's map iterates key-sorted — the reference's `sorted()`.
    for (dname, dim) in dims {
        let mut lo_total: i128 = 0;
        let mut hi_total: i128 = 0;
        let mut contributions = Vec::new();
        for term in dimension_terms(dname, dim)?
            .iter()
            .filter_map(Value::as_object)
        {
            let key = str_or(term.get("evidence"), "");
            let tname = str_or(term.get("transform"), "");
            let tr = transforms
                .get(tname)
                .and_then(Value::as_object)
                .ok_or_else(|| bundle_law(format!("transforms.{tname} · unknown")))?;
            let w = fixed_int(term.get("weight_bp"), "rules · weight_bp")?;
            let value = items.get(key).and_then(|e| e.get("value"));
            let (c_lo, c_hi, known) = if let Some(v) = value.and_then(evidence_int) {
                let c = mul_bp(apply_transform(tr, v)?, w)?;
                (c, c, true)
            } else {
                let (r_lo, r_hi) = transform_range(tr)?;
                let a = mul_bp(r_lo, w)?;
                let z = mul_bp(r_hi, w)?;
                (a.min(z), a.max(z), false)
            };
            lo_total += c_lo;
            hi_total += c_hi;
            contributions.push(json!({
                "evidence": key,
                "known": known,
                "contribution": { "lo": receipt_int(c_lo)?, "hi": receipt_int(c_hi)? },
                "weight_bp": receipt_int(w)?,
                "transform": tname,
            }));
        }
        dims_out.insert(
            dname.clone(),
            json!({
                "interval": { "lo": receipt_int(lo_total)?, "hi": receipt_int(hi_total)? },
                "contributions": contributions,
            }),
        );
        intervals.insert(dname.clone(), (lo_total, hi_total));
    }
    Ok((dims_out, intervals))
}

/// The reference's `_apply_transform` — clamp into `[min, max]`, then
/// the kind's arithmetic (Python `//` = floor division = `div_euclid`
/// on the positive fixed-point divisor).
fn apply_transform(tr: &Map<String, Value>, v: i128) -> DResult<i128> {
    let lo = fixed_int(tr.get("min"), "transforms · min")?;
    let hi = fixed_int(tr.get("max"), "transforms · max")?;
    // `max(lo, min(hi, v))` verbatim — total for the validated lo ≤ hi.
    let v = v.min(hi).max(lo);
    match str_or(tr.get("kind"), "") {
        "clamp" => Ok(v),
        "linear" => {
            let scale = fixed_int_or(tr.get("scale_bp"), BP, "transforms · scale_bp")?;
            let offset = fixed_int_or(tr.get("offset"), 0, "transforms · offset")?;
            Ok(mul_bp(v, scale)? + offset)
        }
        "bucket" => bucket_value(tr, v),
        kind => Err(bundle_law(format!(
            "transforms · kind outside the closed set: {kind}"
        ))),
    }
}

/// `bucket` lookup: the first edge strictly above `v` selects its value;
/// past every edge, the last value.
fn bucket_value(tr: &Map<String, Value>, v: i128) -> DResult<i128> {
    let edges = tr
        .get("edges")
        .and_then(Value::as_array)
        .ok_or_else(|| bundle_law("transforms · bucket needs edges[]"))?;
    for (i, e) in edges.iter().enumerate() {
        if v < fixed_int(Some(e), "transforms · edges[]")? {
            return fixed_int(
                tr.get("values").and_then(|vs| vs.get(i)),
                "transforms · values[]",
            );
        }
    }
    fixed_int(
        tr.get("values").and_then(|vs| vs.get(edges.len())),
        "transforms · values[]",
    )
}

/// The transform's output range from its two INPUT endpoints — exactly
/// the reference's `_transform_range` (two samples, min/maxed; a
/// non-monotone bucket's interior is deliberately not probed — the
/// reference is the law, not the mathematician).
fn transform_range(tr: &Map<String, Value>) -> DResult<(i128, i128)> {
    let lo_in = fixed_int(tr.get("min"), "transforms · min")?;
    let hi_in = fixed_int(tr.get("max"), "transforms · max")?;
    let a = apply_transform(tr, lo_in)?;
    let z = apply_transform(tr, hi_in)?;
    Ok((a.min(z), a.max(z)))
}

// ─── outcome · determination provenance ─────────────────────────────────

/// Governance first, then abstention, then thresholds — and
/// `never_automatic` LAST, over whatever outcome emerged. Every
/// determination line is the reference's exact English (receipt bytes).
fn decide_outcome(
    b: &Map<String, Value>,
    conflicts: &[Value],
    missing: &[String],
    intervals: &BTreeMap<String, (i128, i128)>,
) -> DResult<(&'static str, Vec<String>)> {
    let mut determination: Vec<String> = Vec::new();
    let mut outcome: &'static str;
    if conflicts
        .iter()
        .any(|c| c.get("required") == Some(&Value::Bool(true)))
    {
        outcome = "human_required";
        determination
            .push("conflict on a required key forces human_required (Belnap · spec 11)".to_owned());
    } else if missing.is_empty() {
        outcome = threshold_outcome(b, intervals, &mut determination)?;
    } else {
        outcome = "defer";
        determination.push(format!(
            "missing required evidence {} — abstention is a safety property",
            py_repr_list(missing)
        ));
    }
    let gov = region_object(b, "governance")?;
    let never_automatic = gov
        .get("never_automatic")
        .and_then(Value::as_array)
        .is_some_and(|na| na.iter().any(|o| o.as_str() == Some(outcome)));
    if never_automatic {
        determination.push(format!(
            "governance.never_automatic lists {outcome} — human_required"
        ));
        outcome = "human_required";
    }
    Ok((outcome, determination))
}

/// The threshold ladder: `inf ≥ gate` → recommend (robust dominance) ·
/// `sup < gate` → next threshold · straddle → defer (incomparable) ·
/// none admitted → defer.
fn threshold_outcome(
    b: &Map<String, Value>,
    intervals: &BTreeMap<String, (i128, i128)>,
    determination: &mut Vec<String>,
) -> DResult<&'static str> {
    for th in thresholds(region_object(b, "rules")?)? {
        let dim = str_or(th.get("dimension"), "");
        let gate = fixed_int(
            th.get("recommend_gte_bp"),
            "rules.thresholds[].recommend_gte_bp",
        )?;
        // Bundle-validated: every threshold names a scored dimension.
        let Some(&(lo, hi)) = intervals.get(dim) else {
            continue;
        };
        if lo >= gate {
            determination.push(format!(
                "dimension {dim} dominates the threshold (inf {lo} >= {gate} \
                 bp) — robust, not point-estimated"
            ));
            return Ok("recommend");
        }
        if hi < gate {
            continue;
        }
        determination.push(format!(
            "dimension {dim} straddles the threshold ([{lo}, {hi}] vs {gate} \
             bp) — incomparable with the available evidence, never a false order"
        ));
        return Ok("defer");
    }
    determination.push("no threshold admitted — defer".to_owned());
    Ok("defer")
}

// ─── fixed-point + mirror helpers ───────────────────────────────────────

/// The reference's `_int` — the fixed-point law: a REAL JSON integer,
/// never a float, never a bool, never absent. `serde_json` stores
/// integers as i64/u64, exactly Python's `isinstance(int)` judgment for
/// JSON-parsed values (`3.0` and `1e3` parse as floats in both worlds).
fn fixed_int(v: Option<&Value>, where_: &str) -> DResult<i128> {
    if let Some(Value::Number(n)) = v
        && let Some(i) = n
            .as_i64()
            .map(i128::from)
            .or_else(|| n.as_u64().map(i128::from))
    {
        return Ok(i);
    }
    let got = v.map_or_else(|| "absent".to_owned(), Value::to_string);
    Err(bundle_law(format!(
        "{where_} · fixed-point law: integer basis-points only, never a float \
         (spec 11 §decision IR) — got {got}"
    )))
}

/// `fixed_int` with the reference's `.get(key, default)` absence default.
fn fixed_int_or(v: Option<&Value>, default: i128, where_: &str) -> DResult<i128> {
    match v {
        None => Ok(default),
        some => fixed_int(some, where_),
    }
}

/// `(a * b) // 10000` — checked multiply (Python ints never overflow;
/// an i128 overflow here is astronomically out of fixed-point range and
/// refuses deterministically), floor division (Python `//`).
fn mul_bp(a: i128, b: i128) -> DResult<i128> {
    a.checked_mul(b).map(|p| p.div_euclid(BP)).ok_or_else(|| {
        bundle_law("fixed-point overflow — a weighted term exceeds the integer range")
    })
}

/// A receipt integer — narrows the internal `i128` to a JSON-safe `i64`
/// (beyond it is a deterministic refusal, never a wrap).
fn receipt_int(v: i128) -> DResult<Value> {
    i64::try_from(v).map(Value::from).map_err(|_| {
        bundle_law(format!(
            "fixed-point overflow — {v} exceeds the JSON-integer receipt range"
        ))
    })
}

/// The scoring view of one evidence value — the reference's bool→1/0
/// then `isinstance(int)`: a real JSON integer scores; a float (even
/// `3.0`), string or container is UNKNOWN-for-scoring, which the type
/// fit may still have admitted (scoring is stricter than fitting).
fn evidence_int(v: &Value) -> Option<i128> {
    match v {
        Value::Bool(flag) => Some(i128::from(*flag)),
        Value::Number(n) => n
            .as_i64()
            .map(i128::from)
            .or_else(|| n.as_u64().map(i128::from)),
        _ => None,
    }
}

/// A string field with the reference's `.get(key, default)` semantics —
/// absent → `default`; present non-string → `""` (never a match in any
/// closed set, so the same refusal fires).
fn str_or<'v>(v: Option<&'v Value>, default: &'static str) -> &'v str {
    match v {
        None => default,
        Some(value) => value.as_str().unwrap_or(""),
    }
}

/// Python `==` over two evidence values where `True == 1` — both int-ish
/// compare numerically, everything else structurally.
fn json_int_eq(a: &Value, b: &Value) -> bool {
    match (evidence_int(a), evidence_int(b)) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

/// The all-else-equal probe: the two evidence maps agree on every key
/// but the varied one.
fn others_equal(ea: &Map<String, Value>, eb: &Map<String, Value>, key: &str) -> bool {
    let fa: BTreeMap<&String, &Value> = ea.iter().filter(|(k, _)| k.as_str() != key).collect();
    let fb: BTreeMap<&String, &Value> = eb.iter().filter(|(k, _)| k.as_str() != key).collect();
    fa == fb
}

/// Python truthiness — the reference reads `required` (and the digest
/// filter) through `bool()`, so any truthy JSON value counts.
#[allow(clippy::float_cmp)] // Python bool(0.0) is exactly the == 0.0 judgment
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(m)) => !m.is_empty(),
    }
}

/// The receipt's `snapshot.digests` — the reference keeps TRUTHY digests
/// and sorts them: homogeneous strings lexically, homogeneous numbers
/// numerically (a mixed list crashes the reference — any deterministic
/// order is faithful there).
fn sorted_digests(evidence: &[&Map<String, Value>]) -> Vec<Value> {
    let mut digests: Vec<&Value> = evidence
        .iter()
        .filter_map(|e| e.get("digest"))
        .filter(|d| truthy(Some(d)))
        .collect();
    if digests.iter().all(|d| d.is_string()) {
        digests.sort_by_key(|d| d.as_str().unwrap_or_default().to_owned());
    } else if digests.iter().all(|d| d.is_number()) {
        digests.sort_by(|a, b| {
            let (x, y) = (a.as_f64().unwrap_or(0.0), b.as_f64().unwrap_or(0.0));
            x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)
        });
    } else {
        digests.sort_by_key(|d| canonical_string(d));
    }
    digests.into_iter().cloned().collect()
}

/// Canonical JSON of one value — the Conflict detector's distinctness
/// key (`serde_json`'s map is sorted and its compact form is exactly the
/// reference's `sort_keys` + `(",", ":")` + raw UTF-8 spelling).
fn canonical_string(v: &Value) -> String {
    // Serializing a `Value` cannot fail (string keys · no NaN inhabitants).
    serde_json::to_string(v).unwrap_or_default()
}

/// Python `str()` of a JSON scalar — the witness sort key
/// (`key=lambda e: str(e.get("source"))`). Sources are strings in the
/// evidence IR; the scalar spellings cover the reference's non-crashing
/// domain.
fn py_str(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => "None".to_owned(),
        Some(Value::Bool(true)) => "True".to_owned(),
        Some(Value::Bool(false)) => "False".to_owned(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// Python `repr()` of a list of strings — the reference f-strings the
/// missing-keys LIST into its determination line, so the receipt bytes
/// carry Python's list spelling verbatim (`['key_a', 'key_b']`).
fn py_repr_list(items: &[String]) -> String {
    let inner: Vec<String> = items.iter().map(|s| py_repr_str(s)).collect();
    format!("[{}]", inner.join(", "))
}

/// Python `repr()` of one string: single-quoted unless the string holds
/// a single quote and no double quote (then double-quoted), with
/// `CPython`'s backslash/quote/control escapes.
fn py_repr_str(s: &str) -> String {
    use std::fmt::Write as _;
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\x{:02x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}
