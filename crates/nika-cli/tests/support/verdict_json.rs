// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Typed output assertions over preserved JSON tokens. Serde owns JSON syntax;
//! decimal normalization never rounds through a binary float. Unrepresentable
//! exponents are an explicit harness refusal, never equality.

use serde::Deserialize;
use serde_json::value::RawValue;
use std::collections::BTreeMap;

struct Object(BTreeMap<String, Box<RawValue>>);

impl<'de> Deserialize<'de> for Object {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        struct Unique;
        impl<'de> serde::de::Visitor<'de> for Unique {
            type Value = Object;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an object with unique keys")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Object, M::Error> {
                let mut fields = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, Box<RawValue>>()? {
                    if fields.insert(key, value).is_some() {
                        return Err(serde::de::Error::custom("duplicate JSON key"));
                    }
                }
                Ok(Object(fields))
            }
        }
        decoder.deserialize_map(Unique)
    }
}

fn object(raw: &str) -> Result<Object, String> {
    serde_json::from_str(raw).map_err(|error| format!("HARNESS_INVALID: {error}"))
}

/// Absence remains different from a present JSON null.
pub(super) fn member(raw: &str, key: &str) -> Result<Option<String>, String> {
    Ok(object(raw)?
        .0
        .remove(key)
        .map(|value| value.get().to_owned()))
}

#[derive(Debug, PartialEq, Eq)]
enum Json {
    Null,
    Bool(bool),
    Number(bool, String, i64),
    Text(String),
    Array(Vec<Self>),
    Object(BTreeMap<String, Self>),
}

/// One exact form per decimal value, after Serde validated its grammar.
/// The checked exponent bounds the harness; it is not an engine restriction.
fn number(raw: &str) -> Result<Json, String> {
    let invalid =
        || "HARNESS_INVALID: decimal exponent exceeds the exact comparison bound".to_owned();
    let (mantissa, exponent) = raw.split_once(['e', 'E']).unwrap_or((raw, "0"));
    let exponent: i64 = exponent.parse().map_err(|_| invalid())?;
    let negative = mantissa.starts_with('-');
    let unsigned = mantissa.strip_prefix('-').unwrap_or(mantissa);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let mut digits = format!("{whole}{fraction}")
        .trim_start_matches('0')
        .to_owned();
    if digits.is_empty() {
        return Ok(Json::Number(false, String::new(), 0));
    }
    let scale = i64::try_from(fraction.len()).map_err(|_| invalid())?;
    let trailing = digits.len() - digits.trim_end_matches('0').len();
    digits.truncate(digits.len() - trailing);
    let exponent = exponent
        .checked_sub(scale)
        .and_then(|value| value.checked_add(i64::try_from(trailing).ok()?))
        .ok_or_else(invalid)?;
    Ok(Json::Number(negative, digits, exponent))
}

fn read(raw: &str) -> Result<Json, String> {
    let value: &RawValue =
        serde_json::from_str(raw).map_err(|error| format!("HARNESS_INVALID: {error}"))?;
    let raw = value.get();
    let invalid = |error: serde_json::Error| format!("HARNESS_INVALID: {error}");
    match raw.as_bytes().first() {
        Some(b'n') => Ok(Json::Null),
        Some(b't' | b'f') => serde_json::from_str(raw).map(Json::Bool).map_err(invalid),
        Some(b'"') => serde_json::from_str(raw).map(Json::Text).map_err(invalid),
        Some(b'[') => {
            let values: Vec<Box<RawValue>> = serde_json::from_str(raw).map_err(invalid)?;
            values
                .iter()
                .map(|value| read(value.get()))
                .collect::<Result<_, _>>()
                .map(Json::Array)
        }
        Some(b'{') => object(raw)?
            .0
            .into_iter()
            .map(|(key, value)| Ok((key, read(value.get())?)))
            .collect::<Result<_, String>>()
            .map(Json::Object),
        Some(_) => number(raw),
        None => Err("HARNESS_INVALID: empty JSON value".to_owned()),
    }
}

pub(super) fn validate(raw: &str) -> Result<(), String> {
    read(raw).map(|_| ())
}

pub(super) fn equal(left: &str, right: &str) -> Result<bool, String> {
    Ok(read(left)? == read(right)?)
}

/// Substring is a separate assertion. Numeric rendering is not an admitted
/// claim here; current substring fixtures use text. Refuse that unsupported
/// surface rather than silently round a number before searching it.
pub(super) fn contains(raw: &str, needle: &str) -> Result<bool, String> {
    fn text(value: &Json) -> Result<String, String> {
        match value {
            Json::Text(value) => {
                serde_json::to_string(value).map_err(|error| format!("HARNESS_INVALID: {error}"))
            }
            Json::Null => Ok("null".to_owned()),
            Json::Bool(value) => Ok(value.to_string()),
            Json::Number(..) => Err(
                "HARNESS_INVALID: numeric output_contains rendering is not qualified".to_owned(),
            ),
            Json::Array(values) => values
                .iter()
                .map(text)
                .collect::<Result<Vec<_>, _>>()
                .map(|values| format!("[{}]", values.join(","))),
            Json::Object(values) => values
                .iter()
                .map(|(key, value)| {
                    let key = serde_json::to_string(key)
                        .map_err(|error| format!("HARNESS_INVALID: {error}"))?;
                    Ok(format!("{key}:{}", text(value)?))
                })
                .collect::<Result<Vec<_>, String>>()
                .map(|values| format!("{{{}}}", values.join(","))),
        }
    }
    let value = read(raw)?;
    match &value {
        Json::Text(value) => Ok(value.contains(needle)),
        _ => Ok(text(&value)?.contains(needle)),
    }
}

#[test]
fn json_equality_preserves_kinds_structure_and_exact_decimals() {
    for (left, right) in [
        ("null", "null"),
        ("true", "true"),
        ("1", "1.0"),
        ("1.0", "1e0"),
        ("-0", "0"),
        ("0.10000000000000000001", "0.100000000000000000010"),
        (r#""true""#, r#""true""#),
        ("[1,true,null]", "[1.0,true,null]"),
        (r#"{"b":[true],"a":1}"#, r#"{"a":1.0,"b":[true]}"#),
    ] {
        assert_eq!(equal(left, right), Ok(true), "{left} versus {right}");
    }
    for (left, right) in [
        ("true", r#""true""#),
        ("true", "1"),
        ("null", r#""null""#),
        ("0.10000000000000000001", "0.10000000000000000002"),
        ("9007199254740992", "9007199254740993"),
        ("[1,2]", "[2,1]"),
        ("[1]", "[1,2]"),
        (r#"{"a":null}"#, "{}"),
        (r#"{"a":true}"#, r#"{"a":1}"#),
        (r#""answer""#, r#""prefix answer suffix""#),
    ] {
        assert_eq!(equal(left, right), Ok(false), "{left} versus {right}");
    }
}

#[test]
fn malformed_values_and_unbounded_exponents_never_agree() {
    for raw in [
        "{",
        "true false",
        "NaN",
        r#"{"a":1,"a":1}"#,
        r#"{"x":[{"a":1,"a":2}]}"#,
        "1e9223372036854775808",
        "0.1e-9223372036854775808",
    ] {
        let error = equal(raw, raw).expect_err("invalid evidence never agrees with itself");
        assert!(error.starts_with("HARNESS_INVALID:"), "{error}");
    }
    assert_eq!(member("{}", "value"), Ok(None));
    assert_eq!(
        member(r#"{"value":null}"#, "value"),
        Ok(Some("null".to_owned()))
    );
}

#[test]
fn substring_is_explicit_and_never_substitutes_for_exact_output() {
    assert_eq!(contains(r#""before answer after""#, "answer"), Ok(true));
    assert_eq!(contains(r#""before answer after""#, "absent"), Ok(false));
    assert_eq!(contains("true", "true"), Ok(true));
    assert_eq!(
        contains(r#"{"b":true,"a":null}"#, r#"{"a":null,"b":true}"#),
        Ok(true)
    );
    assert!(contains("1.0000000000000000001", "1").is_err());
}
