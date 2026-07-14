// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The runtime fit — does a decoded VALUE inhabit a type? (spec 09 ·
//! the `NIKA-TYPE-101` judgment · mirrors `type_core.py::fits`).
//!
//! Presence law: an ABSENT optional field is fine; a PRESENT null is
//! refused unless the field's type admits null (absence ≠ null).
//! String newtypes check the FAMILY only (formats are the static
//! contract); refined-string patterns are static-only here (no regex
//! engine dependency — both evaluators agree on the honest gap).

use alloc::collections::BTreeMap;
use alloc::string::String;

use serde_json::Value;

use super::{NikaType, Primitive};

/// `value ∈ T` — the run-time half of the contract.
#[must_use]
pub fn fits(value: &Value, t: &NikaType, named: &BTreeMap<String, NikaType>) -> bool {
    use NikaType as T;
    match t {
        T::Ref(n) => named.get(n).is_some_and(|rt| fits(value, rt, named)),
        T::Unknown => true,
        T::Never => false,
        T::Prim(Primitive::Null) => value.is_null(),
        T::Union(ms) => ms.iter().any(|m| fits(value, m, named)),
        T::Prim(Primitive::Bool) => value.is_boolean(),
        T::Prim(Primitive::Integer) => is_integerish(value),
        T::Prim(Primitive::Number) => value.is_number(),
        T::Prim(_) => value.is_string(), // string + newtypes + bytes(base64)
        T::Enum(values) => value
            .as_str()
            .is_some_and(|s| values.iter().any(|v| v == s)),
        T::BoundedInt(b) => {
            is_integerish(value)
                && value
                    .as_f64()
                    .is_some_and(|n| b.min.is_none_or(|m| n >= m) && b.max.is_none_or(|m| n <= m))
        }
        T::BoundedNum(b) => value
            .as_f64()
            .is_some_and(|n| b.min.is_none_or(|m| n >= m) && b.max.is_none_or(|m| n <= m)),
        T::RefinedStr(b) => value.as_str().is_some_and(|s| {
            let len = s.chars().count() as u64;
            b.min_len.is_none_or(|m| len >= m) && b.max_len.is_none_or(|m| len <= m)
            // pattern: static-only (the honest gap · both evaluators)
        }),
        T::Array(inner) => value
            .as_array()
            .is_some_and(|xs| xs.iter().all(|v| fits(v, inner, named))),
        T::Map(inner) => value
            .as_object()
            .is_some_and(|m| m.values().all(|v| fits(v, inner, named))),
        T::Object { fields, additional } => {
            let Some(obj) = value.as_object() else {
                return false;
            };
            for (k, f) in fields {
                match obj.get(k) {
                    Some(v) => {
                        if !fits(v, &f.ty, named) {
                            return false;
                        }
                    }
                    None if f.optional => {}
                    None => return false,
                }
            }
            if !additional && obj.keys().any(|k| !fields.contains_key(k)) {
                return false;
            }
            true
        }
    }
}

/// An integer value — a JSON int, or a float with zero fractional part
/// (`3.0` inhabits `integer` · `3.5` does not · a bool never does).
fn is_integerish(value: &Value) -> bool {
    if value.is_boolean() {
        return false;
    }
    if value.is_i64() || value.is_u64() {
        return true;
    }
    value
        .as_f64()
        .is_some_and(|f| f.fract() == 0.0 && f.is_finite())
}

#[cfg(test)]
mod tests {
    use super::super::Field;
    use super::*;
    use alloc::borrow::ToOwned;
    use alloc::vec;
    use serde_json::json;

    fn env() -> BTreeMap<String, NikaType> {
        BTreeMap::new()
    }

    fn obj(pairs: &[(&str, NikaType, bool)], additional: bool) -> NikaType {
        NikaType::Object {
            fields: pairs
                .iter()
                .map(|(k, t, o)| ((*k).to_owned(), Field::new(t.clone(), *o)))
                .collect(),
            additional,
        }
    }

    #[test]
    fn presence_four_ways() {
        let n = env();
        let story = obj(
            &[
                ("headline", NikaType::Prim(Primitive::String), false),
                ("byline", NikaType::Prim(Primitive::String), true),
                (
                    "score",
                    NikaType::union_of(vec![
                        NikaType::Prim(Primitive::Integer),
                        NikaType::Prim(Primitive::Null),
                    ]),
                    false,
                ),
            ],
            false,
        );
        assert!(
            fits(&json!({"headline": "h", "score": 1}), &story, &n),
            "absent optional ok"
        );
        assert!(
            !fits(
                &json!({"headline": "h", "byline": null, "score": 1}),
                &story,
                &n
            ),
            "PRESENT null on a non-nullable optional field refused"
        );
        assert!(fits(
            &json!({"headline": "h", "byline": "b", "score": 1}),
            &story,
            &n
        ));
        assert!(
            fits(&json!({"headline": "h", "score": null}), &story, &n),
            "required nullable: null ok"
        );
        assert!(
            !fits(&json!({"headline": "h"}), &story, &n),
            "required nullable: ABSENT refused"
        );
    }

    #[test]
    fn closedness_numbers_enums_and_bottom() {
        let n = env();
        let t = obj(
            &[("count", NikaType::Prim(Primitive::Integer), false)],
            false,
        );
        assert!(fits(&json!({"count": 3}), &t, &n));
        assert!(!fits(&json!({"count": "three"}), &t, &n));
        assert!(
            !fits(&json!({"count": 3, "x": 1}), &t, &n),
            "closed refuses extras"
        );
        let open = obj(
            &[("count", NikaType::Prim(Primitive::Integer), false)],
            true,
        );
        assert!(fits(&json!({"count": 3, "x": 1}), &open, &n));
        assert!(
            fits(&json!(3.0), &NikaType::Prim(Primitive::Integer), &n),
            "3.0 is an integer"
        );
        assert!(!fits(&json!(3.5), &NikaType::Prim(Primitive::Integer), &n));
        assert!(
            !fits(&json!(true), &NikaType::Prim(Primitive::Integer), &n),
            "bool is not an integer"
        );
        let e = NikaType::Enum(vec!["hi".to_owned(), "lo".to_owned()]);
        assert!(fits(&json!("hi"), &e, &n) && !fits(&json!("mid"), &e, &n));
        assert!(
            !fits(&json!("x"), &NikaType::Never, &n) && !fits(&json!(null), &NikaType::Never, &n)
        );
        assert!(fits(&json!({"any": 1}), &NikaType::Unknown, &n));
    }

    #[test]
    fn arrays_maps_and_null_discipline() {
        let n = env();
        let arr = NikaType::Array(alloc::boxed::Box::new(NikaType::Prim(Primitive::String)));
        assert!(fits(&json!(["a"]), &arr, &n) && !fits(&json!(["a", 1]), &arr, &n));
        assert!(
            fits(
                &json!(null),
                &NikaType::union_of(vec![
                    NikaType::Prim(Primitive::String),
                    NikaType::Prim(Primitive::Null)
                ]),
                &n
            ),
            "nullable is a union"
        );
        assert!(!fits(&json!(null), &NikaType::Prim(Primitive::String), &n));
    }
}
