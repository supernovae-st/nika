// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The three relations + join/meet (spec 09 §the relations · §meet) —
//! mirrored law-for-law by the spec's second evaluator
//! (`conformance/type_core.py` · 115 pinned laws) and proven against
//! the committed corpus by `tests/type_differential.rs`. A divergence
//! is a spec bug, never a judgment call.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use super::{NikaType, NumBounds, Primitive};

type Env = BTreeMap<String, NikaType>;

/// `A ⊑ B` — the PARTIAL ORDER on known types (reflexive · transitive ·
/// antisymmetric). `Unknown` is comparable only to itself · `Never` is
/// bottom.
#[must_use]
pub fn subtype(a: &NikaType, b: &NikaType, named: &Env) -> bool {
    if *a == NikaType::Never {
        return true;
    }
    if *a == NikaType::Unknown || *b == NikaType::Unknown {
        return a == b;
    }
    sub(a, b, named, false)
}

/// `A ~ B` — gradual consistency: reflexive · symmetric · NOT
/// transitive. `Unknown ~ T` for every T · structural elsewhere
/// (same-shape modulo Unknown — subsumption does NOT ride here).
#[must_use]
pub fn consistent(a: &NikaType, b: &NikaType, named: &Env) -> bool {
    use NikaType as T;
    if *a == T::Unknown || *b == T::Unknown {
        return true;
    }
    match (a, b) {
        (T::Ref(n), _) => named.get(n).is_some_and(|ra| consistent(ra, b, named)),
        (_, T::Ref(n)) => named.get(n).is_some_and(|rb| consistent(a, rb, named)),
        (T::Union(_), _) | (_, T::Union(_)) => {
            let ams: Vec<&NikaType> = match a {
                T::Union(ms) => ms.iter().collect(),
                other => alloc::vec![other],
            };
            let bms: Vec<&NikaType> = match b {
                T::Union(ms) => ms.iter().collect(),
                other => alloc::vec![other],
            };
            ams.iter()
                .all(|x| bms.iter().any(|y| consistent(x, y, named)))
                && bms
                    .iter()
                    .all(|y| ams.iter().any(|x| consistent(x, y, named)))
        }
        (T::Array(ia), T::Array(ib)) | (T::Map(ia), T::Map(ib)) => consistent(ia, ib, named),
        (T::Object { fields: fa, .. }, T::Object { fields: fb, .. }) => fa
            .iter()
            .filter_map(|(k, af)| fb.get(k).map(|bf| (af, bf)))
            .all(|(af, bf)| consistent(&af.ty, &bf.ty, named)),
        _ => a == b,
    }
}

/// `A ⊑~ B` — consistent subtyping (Siek-Taha): subtyping with
/// `Unknown` accepting at the leaves. THE relation every static judge
/// consumes (the walk · `TYPE-004` · the static fit).
#[must_use]
pub fn assignable(a: &NikaType, b: &NikaType, named: &Env) -> bool {
    if *a == NikaType::Never || *a == NikaType::Unknown || *b == NikaType::Unknown {
        return true;
    }
    sub(a, b, named, true)
}

/// The shared structural walk — `gradual` selects the recursion
/// (assignable lets `Unknown` accept at the leaves).
fn sub(a: &NikaType, b: &NikaType, named: &Env, gradual: bool) -> bool {
    use NikaType as T;
    let rec = |x: &NikaType, y: &NikaType| -> bool {
        if gradual {
            assignable(x, y, named)
        } else {
            subtype(x, y, named)
        }
    };
    match (a, b) {
        (T::Ref(n), _) => named.get(n).is_some_and(|ra| rec(ra, b)),
        (_, T::Ref(n)) => named.get(n).is_some_and(|rb| rec(a, rb)),
        (T::Union(ms), _) => ms.iter().all(|m| rec(m, b)),
        (_, T::Union(ms)) => ms.iter().any(|m| rec(a, m)),
        (x, y) if x == y => true,
        // the one numeric widening · enums and refinements are below
        // their bare primitives · newtypes narrow string
        (
            T::Prim(Primitive::Integer) | T::BoundedInt(_) | T::BoundedNum(_),
            T::Prim(Primitive::Number),
        )
        | (T::Enum(_) | T::RefinedStr(_), T::Prim(Primitive::String))
        | (T::BoundedInt(_), T::Prim(Primitive::Integer)) => true,
        (T::Prim(p), T::Prim(Primitive::String)) if p.narrows_string() => true,
        (T::Enum(ea), T::Enum(eb)) => ea.iter().all(|v| eb.contains(v)),
        (T::BoundedInt(ba), T::BoundedInt(bb) | T::BoundedNum(bb))
        | (T::BoundedNum(ba), T::BoundedNum(bb)) => bounds_nest(ba, bb),
        (T::RefinedStr(ra), T::RefinedStr(rb)) => {
            // regex: syntactic equality only (inclusion never decided)
            (rb.pattern.is_none() || ra.pattern == rb.pattern)
                && (rb.min_len.is_none()
                    || ra.min_len.is_some_and(|m| m >= rb.min_len.unwrap_or(0)))
                && (rb.max_len.is_none()
                    || ra.max_len.zip(rb.max_len).is_some_and(|(am, bm)| am <= bm))
        }
        (T::Array(ia), T::Array(ib)) | (T::Map(ia), T::Map(ib)) => rec(ia, ib),
        (
            T::Object {
                fields: fa,
                additional: open_a,
            },
            T::Object {
                fields: fb,
                additional: open_b,
            },
        ) => {
            // an OPEN a leaves undeclared keys carrying ANY value —
            // only an open b that adds no constraint of its own can
            // admit that (spec 09 §the relations · the openness law)
            if *open_a && (!open_b || fb.keys().any(|k| !fa.contains_key(k))) {
                return false;
            }
            for (k, bf) in fb {
                match fa.get(k) {
                    Some(af) => {
                        // an optional a-field may be ABSENT — it cannot
                        // serve a REQUIRED b-slot (presence, not null)
                        if af.optional && !bf.optional {
                            return false;
                        }
                        if !rec(&af.ty, &bf.ty) {
                            return false;
                        }
                    }
                    None if bf.optional => {}
                    None => return false,
                }
            }
            if !open_b && fa.keys().any(|k| !fb.contains_key(k)) {
                return false;
            }
            true
        }
        _ => false,
    }
}

/// `[a.min, a.max] ⊆ [b.min, b.max]` (absent bound = unbounded).
fn bounds_nest(a: &NumBounds, b: &NumBounds) -> bool {
    let lo = match b.min {
        None => true,
        Some(bm) => a.min.is_some_and(|am| am >= bm),
    };
    let hi = match b.max {
        None => true,
        Some(bm) => a.max.is_some_and(|am| am <= bm),
    };
    lo && hi
}

/// `A ⊔ B` — the language HAS unions, so the join is always expressible.
#[must_use]
pub fn join(a: NikaType, b: NikaType) -> NikaType {
    NikaType::union_of(alloc::vec![a, b])
}

/// `A ⊓ B` — honest three-way (spec 09 §meet): `Some(exact)` when
/// computable · `Some(Never)` when provably disjoint (impossibility) ·
/// `None` when not computed (insufficient information — `Unknown ⊓ T`
/// stays `None`: insufficiency and impossibility are never interchanged).
#[must_use]
pub fn meet(a: &NikaType, b: &NikaType, named: &Env) -> Option<NikaType> {
    use NikaType as T;
    if *a == T::Unknown || *b == T::Unknown {
        return None;
    }
    if *a == T::Never || *b == T::Never {
        return Some(T::Never);
    }
    if subtype(a, b, named) {
        return Some(a.clone());
    }
    if subtype(b, a, named) {
        return Some(b.clone());
    }
    if let (T::Enum(ea), T::Enum(eb)) = (a, b) {
        let inter: Vec<String> = ea.iter().filter(|v| eb.contains(v)).cloned().collect();
        return Some(if inter.is_empty() {
            T::Never
        } else {
            T::Enum(inter)
        });
    }
    let num = |t: &NikaType| -> Option<(bool, NumBounds)> {
        match t {
            T::BoundedInt(b0) => Some((true, *b0)),
            T::BoundedNum(b0) => Some((false, *b0)),
            _ => None,
        }
    };
    if let (Some((ia, ba)), Some((ib, bb))) = (num(a), num(b)) {
        let lo = match (ba.min, bb.min) {
            (Some(x), Some(y)) => Some(x.max(y)),
            (x, y) => x.or(y),
        };
        let hi = match (ba.max, bb.max) {
            (Some(x), Some(y)) => Some(x.min(y)),
            (x, y) => x.or(y),
        };
        if let (Some(l), Some(h)) = (lo, hi)
            && l > h
        {
            return Some(T::Never);
        }
        let bounds = NumBounds::new(lo, hi);
        return Some(if ia || ib {
            T::BoundedInt(bounds)
        } else {
            T::BoundedNum(bounds)
        });
    }
    match (family(a), family(b)) {
        (Some(fa), Some(fb)) if fa != fb => Some(T::Never),
        _ => None,
    }
}

/// The primitive family — two types in DIFFERENT families are provably
/// disjoint (the `Never` branch of the meet).
fn family(t: &NikaType) -> Option<&'static str> {
    use NikaType as T;
    match t {
        T::Prim(Primitive::Integer | Primitive::Number) | T::BoundedInt(_) | T::BoundedNum(_) => {
            Some("numeric")
        }
        T::Prim(p) if *p == Primitive::String || *p == Primitive::Bytes || p.narrows_string() => {
            Some("textual")
        }
        T::Enum(_) | T::RefinedStr(_) => Some("textual"),
        T::Prim(Primitive::Bool) => Some("bool"),
        T::Prim(Primitive::Null) => Some("null"),
        T::Array(_) => Some("array"),
        T::Map(_) | T::Object { .. } => Some("objectish"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::Field;
    use super::*;
    use alloc::borrow::ToOwned;
    use alloc::boxed::Box;
    use alloc::vec;

    fn env() -> Env {
        BTreeMap::new()
    }

    fn prim(p: Primitive) -> NikaType {
        NikaType::Prim(p)
    }

    #[test]
    fn the_order_is_antisymmetric_and_unknown_is_self_only() {
        let n = env();
        assert!(subtype(
            &prim(Primitive::Integer),
            &prim(Primitive::Number),
            &n
        ));
        assert!(!subtype(
            &prim(Primitive::Number),
            &prim(Primitive::Integer),
            &n
        ));
        assert!(!subtype(&NikaType::Unknown, &prim(Primitive::Integer), &n));
        assert!(!subtype(&prim(Primitive::Integer), &NikaType::Unknown, &n));
        assert!(subtype(&NikaType::Unknown, &NikaType::Unknown, &n));
        assert!(
            subtype(&NikaType::Never, &prim(Primitive::Null), &n),
            "never is bottom"
        );
        assert!(!subtype(&prim(Primitive::Null), &NikaType::Never, &n));
    }

    #[test]
    fn consistency_is_symmetric_and_never_launders() {
        let n = env();
        assert!(consistent(&NikaType::Unknown, &prim(Primitive::Bool), &n));
        assert!(consistent(&prim(Primitive::Bool), &NikaType::Unknown, &n));
        assert!(!consistent(
            &prim(Primitive::Null),
            &prim(Primitive::Bool),
            &n
        ));
        assert!(!assignable(
            &prim(Primitive::Null),
            &prim(Primitive::Bool),
            &n
        ));
        // deep unknown leaf
        assert!(consistent(
            &NikaType::Array(Box::new(prim(Primitive::String))),
            &NikaType::Array(Box::new(NikaType::Unknown)),
            &n
        ));
    }

    #[test]
    fn assignability_carries_subsumption_and_unknown_leaves() {
        let n = env();
        assert!(assignable(
            &prim(Primitive::Integer),
            &prim(Primitive::Number),
            &n
        ));
        assert!(assignable(
            &NikaType::Array(Box::new(prim(Primitive::Integer))),
            &NikaType::Array(Box::new(NikaType::Unknown)),
            &n
        ));
    }

    #[test]
    fn optional_field_presence_in_the_order() {
        let n = env();
        let req = |t: NikaType| Field::new(t, false);
        let opt = |t: NikaType| Field::new(t, true);
        let a = NikaType::Object {
            fields: [("a".to_owned(), opt(prim(Primitive::String)))]
                .into_iter()
                .collect(),
            additional: false,
        };
        let b_req = NikaType::Object {
            fields: [("a".to_owned(), req(prim(Primitive::String)))]
                .into_iter()
                .collect(),
            additional: false,
        };
        let b_opt = NikaType::Object {
            fields: [("a".to_owned(), opt(prim(Primitive::String)))]
                .into_iter()
                .collect(),
            additional: false,
        };
        assert!(
            !subtype(&a, &b_req, &n),
            "an optional a-field cannot serve a required b-slot"
        );
        assert!(subtype(&a, &b_opt, &n));
        let empty = NikaType::Object {
            fields: BTreeMap::new(),
            additional: false,
        };
        assert!(
            subtype(&empty, &b_opt, &n),
            "optional b-slot admits absence"
        );
        assert!(!subtype(&empty, &b_req, &n));
    }

    #[test]
    fn meet_is_honest_three_way() {
        let n = env();
        assert_eq!(
            meet(&prim(Primitive::String), &prim(Primitive::Integer), &n),
            Some(NikaType::Never),
            "impossibility, not unknown"
        );
        assert_eq!(meet(&NikaType::Unknown, &prim(Primitive::String), &n), None);
        assert_eq!(
            meet(
                &NikaType::BoundedInt(NumBounds::new(Some(0.0), Some(10.0))),
                &NikaType::BoundedInt(NumBounds::new(Some(5.0), Some(20.0))),
                &n
            ),
            Some(NikaType::BoundedInt(NumBounds::new(Some(5.0), Some(10.0))))
        );
        assert_eq!(
            meet(
                &NikaType::Enum(vec!["a".to_owned()]),
                &NikaType::Enum(vec!["c".to_owned()]),
                &n
            ),
            Some(NikaType::Never)
        );
        assert_eq!(
            meet(
                &NikaType::Enum(vec!["a".to_owned()]),
                &prim(Primitive::String),
                &n
            ),
            Some(NikaType::Enum(vec!["a".to_owned()])),
            "subtype side returns the smaller"
        );
    }

    #[test]
    fn join_is_the_union() {
        let n = env();
        let j = join(prim(Primitive::Integer), prim(Primitive::String));
        assert!(subtype(&prim(Primitive::Integer), &j, &n));
        assert!(subtype(&prim(Primitive::String), &j, &n));
    }
}
