// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The decidable type core (spec `09-types.md`) — ONE truth for the
//! whole estate.
//!
//! A [`NikaType`] is the normalized form of a v1 type expression:
//! parsing, the three relations and the JSON-Schema lowering all live
//! HERE, and every consumer (the schema checker · the runtime fit · the
//! structured-output compiler · the LSP hover) projects from this
//! module — no re-implementation, anywhere, ever (the `analyzer::edges`
//! pattern applied to types).
//!
//! The soundness core (spec 09 §the relations · the W3 checkpoint) ·
//!
//! - **`subtype` · `A ⊑ B`** — a PARTIAL ORDER on known types:
//!   reflexive · transitive · antisymmetric. `Unknown` is comparable
//!   only to itself; `Never` is bottom.
//! - **`consistent` · `A ~ B`** — gradual consistency: reflexive ·
//!   symmetric · NOT transitive · `Unknown ~ T` for every T.
//! - **`assignable` · `A ⊑~ B`** — consistent subtyping (Siek-Taha):
//!   what every static judge consumes.
//!
//! Normalization (spec 09 §normalization): unions flatten / dedup /
//! sort / collapse; **field optionality is NOT a type** — it lives on
//! the field slot ([`Field::optional`]) and never rewrites into the
//! value type (absence ≠ null). Two equal types are structurally equal.
//!
//! One lowering — the [`lower()`](lower()) function is the single projection to JSON Schema
//! 2020-12; there is no `raise()`. Independence: the spec's second
//! evaluator (`conformance/type_core.py`) implements the same judgments
//! BY HAND; `tests/type_differential.rs` proves agreement over the
//! committed corpus — the two share no code, a common bug would have to
//! be born twice.

mod fit;
mod lattice;
mod lower;
mod parse;

pub use fit::fits;
pub use lattice::{assignable, consistent, join, meet, subtype};
pub use lower::lower;
pub use parse::{
    ParseTypeError, is_type_name, parse_type, regex_dialect_violation, type_name_refs,
};

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// The 10 primitive types (spec 09 §grammar). `money` is NOT here — it
/// is reserved for the decision core (W-DEC · fixed-point + ISO-4217 ·
/// never binary floats).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
#[non_exhaustive]
pub enum Primitive {
    /// The null type (exactly the JSON `null`).
    Null,
    /// Boolean.
    Bool,
    /// Whole number (`integer ⊑ number` — the one widening).
    Integer,
    /// IEEE double.
    Number,
    /// UTF-8 string.
    String,
    /// Opaque bytes (lowers as base64 string).
    Bytes,
    /// A URI (string newtype).
    Uri,
    /// A filesystem path (string newtype · no portable JSON format).
    Path,
    /// A quoted Go-duration (string newtype).
    Duration,
    /// RFC 3339 timestamp (string newtype).
    Timestamp,
}

impl Primitive {
    /// The spec spelling (spec 09 §grammar · the YAML surface).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool => "bool",
            Self::Integer => "integer",
            Self::Number => "number",
            Self::String => "string",
            Self::Bytes => "bytes",
            Self::Uri => "uri",
            Self::Path => "path",
            Self::Duration => "duration",
            Self::Timestamp => "timestamp",
        }
    }

    /// Parse a primitive name (lowercase spec spelling).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "null" => Some(Self::Null),
            "bool" => Some(Self::Bool),
            "integer" => Some(Self::Integer),
            "number" => Some(Self::Number),
            "string" => Some(Self::String),
            "bytes" => Some(Self::Bytes),
            "uri" => Some(Self::Uri),
            "path" => Some(Self::Path),
            "duration" => Some(Self::Duration),
            "timestamp" => Some(Self::Timestamp),
            _ => None,
        }
    }

    /// Is this a string newtype (`⊑ string` · spec 09 §lattice)?
    #[must_use]
    pub fn narrows_string(self) -> bool {
        matches!(
            self,
            Self::Uri | Self::Path | Self::Duration | Self::Timestamp
        )
    }
}

/// Inclusive numeric bounds (`{ integer: {min, max} }` refinements).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct NumBounds {
    /// Inclusive lower bound (`None` = unbounded).
    pub min: Option<f64>,
    /// Inclusive upper bound (`None` = unbounded).
    pub max: Option<f64>,
}

impl NumBounds {
    /// Construct bounds (invariant #19 — `#[non_exhaustive]` ships a constructor).
    #[must_use]
    pub const fn new(min: Option<f64>, max: Option<f64>) -> Self {
        Self { min, max }
    }
}

/// String refinements (`{ string: {pattern, min_len, max_len} }`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct StrBounds {
    /// Regular expression in the LOCKED dialect (spec 09 §the regex
    /// dialect · validated at parse by [`regex_dialect_violation`]) —
    /// compared by SYNTACTIC equality only (inclusion never decided).
    pub pattern: Option<String>,
    /// Minimum length (chars).
    pub min_len: Option<u64>,
    /// Maximum length (chars).
    pub max_len: Option<u64>,
}

impl StrBounds {
    /// Construct refinements (invariant #19).
    #[must_use]
    pub const fn new(pattern: Option<String>, min_len: Option<u64>, max_len: Option<u64>) -> Self {
        Self {
            pattern,
            min_len,
            max_len,
        }
    }
}

/// One field of a closed object — `optional` is the **field-presence
/// modifier** (spec 09 §optional is presence, not null): the field may
/// be ABSENT; when present its value must inhabit `ty` (a present null
/// is refused unless `ty` itself admits null).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct Field {
    /// The field's value type (never carries presence).
    pub ty: NikaType,
    /// Declared `{ optional: … }` at the field slot.
    pub optional: bool,
}

impl Field {
    /// Construct a field (invariant #19).
    #[must_use]
    pub const fn new(ty: NikaType, optional: bool) -> Self {
        Self { ty, optional }
    }
}

/// A normalized v1 type (spec 09 §grammar · §normalization).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum NikaType {
    /// A primitive.
    Prim(Primitive),
    /// A closed string enumeration (sorted · deduped · non-empty).
    Enum(Vec<String>),
    /// Bounded integer (`{ integer: {min, max} }`).
    BoundedInt(NumBounds),
    /// Bounded number.
    BoundedNum(NumBounds),
    /// Refined string.
    RefinedStr(StrBounds),
    /// Homogeneous array.
    Array(alloc::boxed::Box<NikaType>),
    /// String-keyed homogeneous map.
    Map(alloc::boxed::Box<NikaType>),
    /// Record — CLOSED unless `additional`.
    Object {
        /// Field name → typed field (`BTreeMap`: deterministic order).
        fields: BTreeMap<String, Field>,
        /// `additional: true` — unknown members admitted.
        additional: bool,
    },
    /// Untagged union (normalized: flat · deduped · sorted · len ≥ 2).
    /// Nullability is spelled HERE (`union: [T, null]`).
    Union(Vec<NikaType>),
    /// A reference to a named type (acyclic upstream · `NIKA-TYPE-002`).
    Ref(String),
    /// The gradual type — absence of static information. In `⊑` it is
    /// comparable only to itself; permissiveness lives in `~`/`⊑~`.
    Unknown,
    /// Bottom — no value inhabits it. INTERNAL (never authorable): the
    /// honest result of a provably-disjoint meet.
    Never,
}

/// An integral bound renders as a JSON integer in the canon key
/// (python emits `10`, never `10.0`).
fn canon_num(m: f64) -> alloc::string::String {
    #[allow(clippy::cast_possible_truncation)] // guarded by fract()==0
    if m.fract() == 0.0 && m.abs() < 9_007_199_254_740_992.0 {
        alloc::format!("{}", m as i64)
    } else {
        alloc::format!("{m}")
    }
}

/// Emit `{"bounds":{…},"refined":"<kind>"}` — the sorted-key canonical
/// form of a numeric refinement (max before min: JSON key order).
fn write_canon_num_bounds(out: &mut String, b: &NumBounds, kind: &str) {
    use core::fmt::Write as _;
    out.push_str("{\"bounds\":{");
    let mut first = true;
    if let Some(m) = b.max {
        let _ = write!(out, "\"max\":{}", canon_num(m));
        first = false;
    }
    if let Some(m) = b.min {
        if !first {
            out.push(',');
        }
        let _ = write!(out, "\"min\":{}", canon_num(m));
    }
    let _ = write!(out, "}},\"refined\":\"{kind}\"}}");
}

/// Emit the canonical refined-string form (sorted keys · `max_len` ·
/// `min_len` · `pattern`).
fn write_canon_str_bounds(out: &mut String, b: &StrBounds) {
    use core::fmt::Write as _;
    out.push_str("{\"bounds\":{");
    let mut first = true;
    if let Some(m) = b.max_len {
        let _ = write!(out, "\"max_len\":{m}");
        first = false;
    }
    if let Some(m) = b.min_len {
        if !first {
            out.push(',');
        }
        let _ = write!(out, "\"min_len\":{m}");
        first = false;
    }
    if let Some(p) = &b.pattern {
        if !first {
            out.push(',');
        }
        let _ = write!(out, "\"pattern\":{}", serde_json::json!(p));
    }
    out.push_str("},\"refined\":\"string\"}");
}

impl NikaType {
    /// Normalize a union member list: flatten nested unions, dedup
    /// structurally, ABSORB `Unknown`, COLLAPSE subsumed ref-free
    /// members, sort canonically, collapse single members (spec 09
    /// §normalization — canonical forms make `⊑` antisymmetric AS
    /// EQUALITY). The canonical order = [`NikaType::canon_key`] —
    /// byte-identical to the reference evaluator's.
    #[must_use]
    pub fn union_of(members: Vec<NikaType>) -> NikaType {
        let mut flat: Vec<NikaType> = Vec::new();
        for m in members {
            match m {
                NikaType::Union(inner) => flat.extend(inner),
                other => flat.push(other),
            }
        }
        // a union with an Unknown member knows NOTHING more than
        // Unknown — joining with no information is no information
        if flat.contains(&NikaType::Unknown) {
            return NikaType::Unknown;
        }
        let mut uniq: Vec<NikaType> = Vec::new();
        for m in flat {
            if !uniq.contains(&m) {
                uniq.push(m);
            }
        }
        // subsumption collapse — drop m when some OTHER member already
        // admits every m-value (ref-free only: a ref is nominal here)
        let empty = BTreeMap::new();
        let mut kept: Vec<NikaType> = Vec::new();
        for (i, m) in uniq.iter().enumerate() {
            let subsumed = m.is_ref_free()
                && uniq
                    .iter()
                    .enumerate()
                    .any(|(j, n)| j != i && n.is_ref_free() && lattice::subtype(m, n, &empty));
            if !subsumed {
                kept.push(m.clone());
            }
        }
        kept.sort_by_key(NikaType::canon_key);
        if kept.len() == 1 {
            return kept.remove(0);
        }
        NikaType::Union(kept)
    }

    /// No `Ref` anywhere inside — the precondition for normalization-
    /// time subsumption (refs resolve only in the relations).
    #[must_use]
    pub fn is_ref_free(&self) -> bool {
        match self {
            NikaType::Ref(_) => false,
            NikaType::Array(i) | NikaType::Map(i) => i.is_ref_free(),
            NikaType::Union(ms) => ms.iter().all(NikaType::is_ref_free),
            NikaType::Object { fields, .. } => fields.values().all(|f| f.ty.is_ref_free()),
            _ => true,
        }
    }

    /// The canonical sort/dedup key — canonical JSON (sorted keys · no
    /// spaces · raw UTF-8) of the reference evaluator's INTERNAL form
    /// (`{"prim": …}` · `{"enum": […]}` · `{"refined": …, "bounds": …}`
    /// · `{"array"/"map"/"object"/"union"/"ref"/"unknown"/"never": …}`)
    /// — the shared total order both evaluators sort unions by.
    #[must_use]
    pub fn canon_key(&self) -> String {
        let mut out = String::new();
        self.write_canon(&mut out);
        out
    }

    fn write_canon(&self, out: &mut String) {
        use core::fmt::Write as _;
        match self {
            NikaType::Prim(p) => {
                let _ = write!(out, "{{\"prim\":\"{}\"}}", p.as_str());
            }
            NikaType::Enum(vs) => {
                out.push_str("{\"enum\":[");
                for (i, v) in vs.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    let _ = write!(out, "{}", serde_json::json!(v));
                }
                out.push_str("]}");
            }
            NikaType::BoundedInt(b) | NikaType::BoundedNum(b) => {
                let kind = if matches!(self, NikaType::BoundedInt(_)) {
                    "integer"
                } else {
                    "number"
                };
                write_canon_num_bounds(out, b, kind);
            }
            NikaType::RefinedStr(b) => write_canon_str_bounds(out, b),
            NikaType::Array(inner) => {
                out.push_str("{\"array\":");
                inner.write_canon(out);
                out.push('}');
            }
            NikaType::Map(inner) => {
                out.push_str("{\"map\":");
                inner.write_canon(out);
                out.push('}');
            }
            NikaType::Object { fields, additional } => {
                // python form: {"additional": true?, "object": {name: FIELD}}
                // where FIELD = {"opt": bool, "ty": T} — keys sorted
                out.push('{');
                if *additional {
                    out.push_str("\"additional\":true,");
                }
                out.push_str("\"object\":{");
                for (i, (k, f)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    let _ = write!(
                        out,
                        "{}:{{\"opt\":{},\"ty\":",
                        serde_json::json!(k),
                        f.optional
                    );
                    f.ty.write_canon(out);
                    out.push_str("}}");
                }
                out.push_str("}}");
            }
            NikaType::Union(ms) => {
                out.push_str("{\"union\":[");
                for (i, m) in ms.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    m.write_canon(out);
                }
                out.push_str("]}");
            }
            NikaType::Ref(n) => {
                let _ = write!(out, "{{\"ref\":{}}}", serde_json::json!(n));
            }
            NikaType::Unknown => out.push_str("{\"unknown\":true}"),
            NikaType::Never => out.push_str("{\"never\":true}"),
        }
    }

    /// Does this (normalized) type admit `null`?
    #[must_use]
    pub fn admits_null(&self) -> bool {
        match self {
            NikaType::Prim(Primitive::Null) | NikaType::Unknown => true,
            NikaType::Union(ms) => ms.iter().any(NikaType::admits_null),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::borrow::ToOwned;
    use alloc::vec;

    #[test]
    fn union_normalizes_flat_dedup_sorted_collapsed() {
        let a = NikaType::union_of(vec![
            NikaType::Union(vec![
                NikaType::Prim(Primitive::String),
                NikaType::Prim(Primitive::Null),
            ]),
            NikaType::Prim(Primitive::Integer),
            NikaType::Prim(Primitive::String),
        ]);
        let b = NikaType::union_of(vec![
            NikaType::Prim(Primitive::Integer),
            NikaType::Prim(Primitive::Null),
            NikaType::Prim(Primitive::String),
        ]);
        assert_eq!(a, b, "flatten + dedup + sort → structural equality");
        let one = NikaType::union_of(vec![
            NikaType::Prim(Primitive::String),
            NikaType::Prim(Primitive::String),
        ]);
        assert_eq!(
            one,
            NikaType::Prim(Primitive::String),
            "one member collapses"
        );
    }

    #[test]
    fn nullable_is_a_union_and_admits_null() {
        let t = NikaType::union_of(vec![
            NikaType::Prim(Primitive::String),
            NikaType::Prim(Primitive::Null),
        ]);
        assert!(t.admits_null());
        assert!(matches!(&t, NikaType::Union(ms) if ms.len() == 2));
    }

    #[test]
    fn primitive_names_round_trip_and_money_is_gone() {
        for p in [
            Primitive::Null,
            Primitive::Bool,
            Primitive::Integer,
            Primitive::Number,
            Primitive::String,
            Primitive::Bytes,
            Primitive::Uri,
            Primitive::Path,
            Primitive::Duration,
            Primitive::Timestamp,
        ] {
            assert_eq!(Primitive::from_name(p.as_str()), Some(p));
        }
        assert_eq!(
            Primitive::from_name("money"),
            None,
            "money is reserved (W-DEC)"
        );
        assert_eq!(
            Primitive::from_name("boolean"),
            None,
            "flat-enum spelling is not the core's"
        );
        let _ = "field slot".to_owned();
    }
}
