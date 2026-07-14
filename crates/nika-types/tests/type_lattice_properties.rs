// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! Property laws of the type core (spec 09 §the relations ·
//! §normalization · §lowering) — proptest over generated v1 types.
//!
//! The relations are tested SEPARATELY (the soundness checkpoint law):
//! `⊑` is a partial order on knowns (reflexive · transitive ·
//! ANTISYMMETRIC — really transitive now, `Unknown` is incomparable) ·
//! `~` is symmetric and never launders · `⊑~` accepts Unknown leaves.
//! ≥ 512 cases per law (the §8 floor is 300).

use std::collections::BTreeMap;

use nika_types::types::{
    Field, NikaType, NumBounds, Primitive, StrBounds, assignable, consistent, fits, lower, meet,
    subtype,
};
use proptest::prelude::*;

fn arb_primitive() -> impl Strategy<Value = Primitive> {
    prop_oneof![
        Just(Primitive::Null),
        Just(Primitive::Bool),
        Just(Primitive::Integer),
        Just(Primitive::Number),
        Just(Primitive::String),
        Just(Primitive::Bytes),
        Just(Primitive::Uri),
        Just(Primitive::Path),
        Just(Primitive::Duration),
        Just(Primitive::Timestamp),
    ]
}

fn arb_type() -> impl Strategy<Value = NikaType> {
    let leaf = prop_oneof![
        arb_primitive().prop_map(NikaType::Prim),
        proptest::collection::btree_set("[a-c]{1,3}", 1..4)
            .prop_map(|s| NikaType::Enum(s.into_iter().collect())),
        (
            proptest::option::of(-100i32..100),
            proptest::option::of(-100i32..100)
        )
            .prop_map(|(a, b)| {
                let (min, max) = match (a, b) {
                    (Some(x), Some(y)) if x > y => (Some(f64::from(y)), Some(f64::from(x))),
                    (x, y) => (x.map(f64::from), y.map(f64::from)),
                };
                // canonical forms: an unbounded refinement IS its
                // primitive (parse never builds the unbounded form)
                if min.is_none() && max.is_none() {
                    NikaType::Prim(Primitive::Integer)
                } else {
                    NikaType::BoundedInt(NumBounds::new(min, max))
                }
            }),
        (
            proptest::option::of(Just("^x".to_owned())),
            proptest::option::of(0u64..5),
            proptest::option::of(5u64..20)
        )
            .prop_map(|(pattern, min_len, max_len)| {
                if pattern.is_none() && min_len.is_none() && max_len.is_none() {
                    NikaType::Prim(Primitive::String)
                } else {
                    NikaType::RefinedStr(StrBounds::new(pattern, min_len, max_len))
                }
            }),
        Just(NikaType::Unknown),
    ];
    leaf.prop_recursive(3, 24, 4, |inner| {
        prop_oneof![
            inner.clone().prop_map(|t| NikaType::Array(Box::new(t))),
            inner.clone().prop_map(|t| NikaType::Map(Box::new(t))),
            proptest::collection::vec(inner.clone(), 2..4).prop_map(NikaType::union_of),
            (
                proptest::collection::btree_map("[a-c]{1,3}", (inner, any::<bool>()), 0..4),
                any::<bool>()
            )
                .prop_map(|(fields, additional)| NikaType::Object {
                    fields: fields
                        .into_iter()
                        .map(|(k, (ty, optional))| (k, Field::new(ty, optional)))
                        .collect(),
                    additional,
                }),
        ]
    })
}

fn env() -> BTreeMap<String, NikaType> {
    BTreeMap::new()
}

fn has_unknown(t: &NikaType) -> bool {
    match t {
        NikaType::Unknown => true,
        NikaType::Array(i) | NikaType::Map(i) => has_unknown(i),
        NikaType::Union(ms) => ms.iter().any(has_unknown),
        NikaType::Object { fields, .. } => fields.values().any(|f| has_unknown(&f.ty)),
        _ => false,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// ⊑ reflexive on every generated type (Unknown included — self only).
    #[test]
    fn subtype_is_reflexive(t in arb_type()) {
        prop_assert!(subtype(&t, &t, &env()));
    }

    /// ⊑ is REALLY transitive now — no Unknown carve-out needed (the
    /// order leaves Unknown incomparable, so laundering is impossible).
    #[test]
    fn subtype_is_transitive(a in arb_type(), b in arb_type(), c in arb_type()) {
        let n = env();
        if subtype(&a, &b, &n) && subtype(&b, &c, &n) {
            prop_assert!(subtype(&a, &c, &n), "a={a:?}\nb={b:?}\nc={c:?}");
        }
    }

    /// ⊑ antisymmetric on normalized knowns: a ⊑ b ∧ b ⊑ a ⇒ a == b.
    #[test]
    fn subtype_is_antisymmetric(a in arb_type(), b in arb_type()) {
        prop_assume!(!has_unknown(&a) && !has_unknown(&b));
        let n = env();
        if subtype(&a, &b, &n) && subtype(&b, &a, &n) {
            prop_assert_eq!(a, b);
        }
    }

    /// Unknown is incomparable in the ORDER (both directions refuse).
    #[test]
    fn unknown_is_incomparable_in_the_order(t in arb_type()) {
        prop_assume!(t != NikaType::Unknown);
        let n = env();
        prop_assert!(!subtype(&NikaType::Unknown, &t, &n));
        prop_assert!(!subtype(&t, &NikaType::Unknown, &n));
    }

    /// ~ symmetric · Unknown ~ everything · ⊑~ accepts Unknown both
    /// ways — and neither launders (null vs bool stays refused).
    #[test]
    fn gradual_laws(t in arb_type()) {
        let n = env();
        prop_assert!(consistent(&NikaType::Unknown, &t, &n));
        prop_assert!(consistent(&t, &NikaType::Unknown, &n));
        prop_assert!(assignable(&NikaType::Unknown, &t, &n));
        prop_assert!(assignable(&t, &NikaType::Unknown, &n));
        prop_assert!(!consistent(
            &NikaType::Prim(Primitive::Null), &NikaType::Prim(Primitive::Bool), &n));
        prop_assert!(!assignable(
            &NikaType::Prim(Primitive::Null), &NikaType::Prim(Primitive::Bool), &n));
    }

    /// ~ symmetric on arbitrary pairs.
    #[test]
    fn consistency_is_symmetric(a in arb_type(), b in arb_type()) {
        let n = env();
        prop_assert_eq!(consistent(&a, &b, &n), consistent(&b, &a, &n));
    }

    /// Every member ⊑~ its union (⊑ when everything is known — the
    /// union may ABSORB an Unknown member to Unknown, where only the
    /// gradual relations reach) · union_of idempotent.
    #[test]
    fn union_laws(a in arb_type(), b in arb_type()) {
        let n = env();
        let u = NikaType::union_of(vec![a.clone(), b.clone()]);
        prop_assert!(assignable(&a, &u, &n));
        prop_assert!(assignable(&b, &u, &n));
        if !has_unknown(&a) && !has_unknown(&b) {
            prop_assert!(subtype(&a, &u, &n));
            prop_assert!(subtype(&b, &u, &n));
        }
        let twice = NikaType::union_of(vec![u.clone()]);
        prop_assert_eq!(u, twice);
    }

    /// Meet is honest: Unknown at the root gives None (not-computed) ·
    /// the ⊑-comparable side returns the smaller type exactly.
    #[test]
    fn meet_is_honest(a in arb_type(), b in arb_type()) {
        let n = env();
        if a == NikaType::Unknown || b == NikaType::Unknown {
            prop_assert_eq!(meet(&a, &b, &n), None);
        } else if !has_unknown(&a) && !has_unknown(&b) && subtype(&a, &b, &n) {
            prop_assert_eq!(meet(&a, &b, &n), Some(a.clone()));
        }
    }

    /// Lowering is TOTAL and deterministic on the whole grammar.
    #[test]
    fn lowering_is_total_and_deterministic(t in arb_type()) {
        let n = env();
        let a = lower(&t, &n);
        let b = lower(&t, &n);
        prop_assert_eq!(a.to_string(), b.to_string());
    }

    /// Closedness + presence survive lowering: closed bars additional ·
    /// optional fields leave required · no implicit null-union.
    #[test]
    fn closedness_and_presence_survive_lowering(
        fields in proptest::collection::btree_map("[a-c]{1,3}", (arb_type(), any::<bool>()), 0..4),
        additional in any::<bool>(),
    ) {
        let t = NikaType::Object {
            fields: fields
                .iter()
                .map(|(k, (ty, opt))| (k.clone(), Field::new(ty.clone(), *opt)))
                .collect(),
            additional,
        };
        let s = lower(&t, &env());
        if additional {
            prop_assert!(s.get("additionalProperties").is_none());
        } else {
            prop_assert_eq!(&s["additionalProperties"], &serde_json::json!(false));
        }
        let required: Vec<String> = s.get("required")
            .and_then(|r| r.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        for (k, (_ty, opt)) in &fields {
            prop_assert_eq!(!required.contains(k), *opt, "presence rides required, only");
        }
    }

    /// A fitting value stays fitting under ⊑ (fit respects the order).
    #[test]
    fn fit_respects_the_order(a in arb_type(), b in arb_type()) {
        prop_assume!(!has_unknown(&a) && !has_unknown(&b));
        let n = env();
        if subtype(&a, &b, &n) {
            for probe in [serde_json::json!(null), serde_json::json!(true),
                          serde_json::json!(3), serde_json::json!("a"),
                          serde_json::json!(["a"]), serde_json::json!({"a": "a"})] {
                if fits(&probe, &a, &n) {
                    prop_assert!(fits(&probe, &b, &n),
                        "probe {probe} fits {a:?} (⊑ {b:?}) but not the supertype");
                }
            }
        }
    }
}
