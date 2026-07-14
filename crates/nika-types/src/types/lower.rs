// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ONE lowering — Nika type → JSON Schema 2020-12 (spec 09
//! §lowering · one direction, total).
//!
//! Presence law: an optional FIELD leaves `required` and lowers as
//! `lower(T)` unchanged — NEVER an implicit null-union (`required`
//! carries presence · `type: null` carries nullability · never
//! blurred). `Never` lowers to `{"not": {}}` (the honest empty type) ·
//! `Unknown` to `{}` (accept-anything). Named refs INLINE (acyclicity
//! makes this total · no `$ref`). There is no `raise()`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use serde_json::{Map, Value, json};

use super::{NikaType, Primitive};

/// The Go-duration pattern (the quoted contract of 01-envelope).
const DURATION_PATTERN: &str =
    r"^[0-9]+(\.[0-9]+)?(ns|us|µs|ms|s|m|h)([0-9]+(\.[0-9]+)?(ns|us|µs|ms|s|m|h))*$";

/// `lower(T)` — total on every normalized [`NikaType`]; a dangling
/// `Ref` (refused upstream) lowers to the honest floor `{}`.
#[must_use]
pub fn lower(t: &NikaType, named: &BTreeMap<String, NikaType>) -> Value {
    match t {
        NikaType::Ref(n) => named.get(n).map_or_else(|| json!({}), |r| lower(r, named)),
        NikaType::Unknown => json!({}),
        NikaType::Never => json!({"not": {}}),
        NikaType::Prim(p) => lower_prim(*p),
        NikaType::Enum(values) => json!({ "type": "string", "enum": values }),
        NikaType::BoundedInt(b) => bounded("integer", b.min, b.max),
        NikaType::BoundedNum(b) => bounded("number", b.min, b.max),
        NikaType::RefinedStr(b) => {
            let mut out = Map::new();
            out.insert("type".into(), json!("string"));
            if let Some(p) = &b.pattern {
                out.insert("pattern".into(), json!(p));
            }
            if let Some(m) = b.min_len {
                out.insert("minLength".into(), json!(m));
            }
            if let Some(m) = b.max_len {
                out.insert("maxLength".into(), json!(m));
            }
            Value::Object(out)
        }
        NikaType::Array(inner) => json!({ "type": "array", "items": lower(inner, named) }),
        NikaType::Map(inner) => {
            json!({ "type": "object", "additionalProperties": lower(inner, named) })
        }
        NikaType::Object { fields, additional } => {
            let mut props = Map::new();
            let mut required: Vec<&str> = Vec::new();
            for (name, field) in fields {
                props.insert(name.clone(), lower(&field.ty, named));
                // presence ONLY — a required-but-nullable field stays
                // required (its anyOf carries the null)
                if !field.optional {
                    required.push(name);
                }
            }
            let mut out = Map::new();
            out.insert("type".into(), json!("object"));
            out.insert("properties".into(), Value::Object(props));
            if !required.is_empty() {
                out.insert("required".into(), json!(required));
            }
            if !additional {
                out.insert("additionalProperties".into(), json!(false));
            }
            Value::Object(out)
        }
        NikaType::Union(members) => {
            json!({ "anyOf": members.iter().map(|m| lower(m, named)).collect::<Vec<_>>() })
        }
    }
}

fn bounded(kind: &str, min: Option<f64>, max: Option<f64>) -> Value {
    let mut out = Map::new();
    out.insert("type".into(), json!(kind));
    if let Some(m) = min {
        out.insert("minimum".into(), num(m));
    }
    if let Some(m) = max {
        out.insert("maximum".into(), num(m));
    }
    Value::Object(out)
}

/// An integral bound serializes as a JSON integer (`0`, never `0.0`) —
/// byte-parity with the reference evaluator's canonical form.
#[allow(clippy::cast_possible_truncation)] // guarded by the fract()==0 test
fn num(m: f64) -> Value {
    if m.fract() == 0.0 && m.abs() < 9_007_199_254_740_992.0 {
        json!(m as i64)
    } else {
        json!(m)
    }
}

fn lower_prim(p: Primitive) -> Value {
    match p {
        Primitive::Null => json!({ "type": "null" }),
        Primitive::Bool => json!({ "type": "boolean" }),
        Primitive::Integer => json!({ "type": "integer" }),
        Primitive::Number => json!({ "type": "number" }),
        // path lowers as a bare string — no portable JSON format exists
        Primitive::String | Primitive::Path => json!({ "type": "string" }),
        Primitive::Bytes => json!({ "type": "string", "contentEncoding": "base64" }),
        Primitive::Uri => json!({ "type": "string", "format": "uri" }),
        Primitive::Duration => json!({ "type": "string", "pattern": DURATION_PATTERN }),
        Primitive::Timestamp => json!({ "type": "string", "format": "date-time" }),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Field, NumBounds};
    use super::*;
    use alloc::borrow::ToOwned;
    use alloc::boxed::Box;
    use alloc::vec;

    fn env() -> BTreeMap<String, NikaType> {
        BTreeMap::new()
    }

    #[test]
    fn presence_and_nullability_never_blur() {
        let t = NikaType::Object {
            fields: [
                (
                    "a".to_owned(),
                    Field::new(NikaType::Prim(Primitive::String), false),
                ),
                (
                    "b".to_owned(),
                    Field::new(NikaType::Prim(Primitive::Integer), true),
                ),
                (
                    "x".to_owned(),
                    Field::new(
                        NikaType::union_of(vec![
                            NikaType::Prim(Primitive::String),
                            NikaType::Prim(Primitive::Null),
                        ]),
                        false,
                    ),
                ),
            ]
            .into_iter()
            .collect(),
            additional: false,
        };
        let s = lower(&t, &env());
        assert_eq!(
            s["required"],
            json!(["a", "x"]),
            "optional leaves required · nullable stays"
        );
        assert_eq!(
            s["properties"]["b"],
            json!({"type": "integer"}),
            "no implicit null-union"
        );
        assert!(
            s["properties"]["x"]["anyOf"].is_array(),
            "nullability rides the union"
        );
        assert_eq!(s["additionalProperties"], json!(false));
        let open = NikaType::Object {
            fields: BTreeMap::new(),
            additional: true,
        };
        assert!(lower(&open, &env()).get("additionalProperties").is_none());
    }

    #[test]
    fn the_table_rows_hold() {
        let n = env();
        assert_eq!(
            lower(&NikaType::Prim(Primitive::Bytes), &n)["contentEncoding"],
            json!("base64")
        );
        assert_eq!(
            lower(&NikaType::Prim(Primitive::Timestamp), &n)["format"],
            json!("date-time")
        );
        assert_eq!(
            lower(&NikaType::Enum(vec!["x".to_owned(), "y".to_owned()]), &n),
            json!({"type": "string", "enum": ["x", "y"]})
        );
        let b = lower(&NikaType::BoundedInt(NumBounds::new(Some(0.0), None)), &n);
        assert_eq!(
            b["minimum"],
            json!(0),
            "integral bounds render as JSON integers"
        );
        assert!(b.get("maximum").is_none(), "absent bound omitted");
        assert_eq!(lower(&NikaType::Never, &n), json!({"not": {}}));
        assert_eq!(lower(&NikaType::Unknown, &n), json!({}));
    }

    #[test]
    fn named_refs_inline_without_dollar_ref() {
        let mut n = env();
        n.insert(
            "Inner".to_owned(),
            NikaType::Object {
                fields: [(
                    "x".to_owned(),
                    Field::new(NikaType::Prim(Primitive::String), false),
                )]
                .into_iter()
                .collect(),
                additional: false,
            },
        );
        let s = lower(
            &NikaType::Array(Box::new(NikaType::Ref("Inner".to_owned()))),
            &n,
        );
        let rendered = s.to_string();
        assert!(!rendered.contains("$ref"), "{rendered}");
        assert!(rendered.contains("\"x\""), "{rendered}");
        assert_eq!(lower(&NikaType::Ref("Ghost".to_owned()), &n), json!({}));
    }
}
