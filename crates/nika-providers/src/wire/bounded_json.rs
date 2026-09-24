// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Bounded settlement cannot price a JSON object after duplicate fields vanished.
//! Keep `serde_json`'s syntax/depth/number checks, but reject repeated decoded keys.
use serde::de::{Deserialize, Deserializer, Error, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use std::fmt;

pub(super) fn parse(body: &[u8]) -> Result<Value, serde_json::Error> {
    serde_json::from_slice::<Unique>(body).map(|v| v.0)
}

struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(UniqueVisitor)
    }
}
struct UniqueVisitor;
impl<'de> Visitor<'de> for UniqueVisitor {
    type Value = Unique;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JSON without duplicate object fields")
    }
    fn visit_unit<E: Error>(self) -> Result<Unique, E> {
        Ok(Unique(Value::Null))
    }
    fn visit_bool<E: Error>(self, v: bool) -> Result<Unique, E> {
        Ok(Unique(Value::Bool(v)))
    }
    fn visit_i64<E: Error>(self, v: i64) -> Result<Unique, E> {
        Ok(Unique(Value::Number(v.into())))
    }
    fn visit_u64<E: Error>(self, v: u64) -> Result<Unique, E> {
        Ok(Unique(Value::Number(v.into())))
    }
    fn visit_f64<E: Error>(self, v: f64) -> Result<Unique, E> {
        Number::from_f64(v)
            .map(|n| Unique(Value::Number(n)))
            .ok_or_else(|| E::custom("nonfinite JSON number"))
    }
    fn visit_str<E: Error>(self, v: &str) -> Result<Unique, E> {
        Ok(Unique(Value::String(v.to_owned())))
    }
    fn visit_string<E: Error>(self, v: String) -> Result<Unique, E> {
        Ok(Unique(Value::String(v)))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut values: A) -> Result<Unique, A::Error> {
        let mut result = Vec::new();
        while let Some(Unique(value)) = values.next_element()? {
            result.push(value);
        }
        Ok(Unique(Value::Array(result)))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut values: A) -> Result<Unique, A::Error> {
        let mut result = Map::new();
        while let Some(key) = values.next_key::<String>()? {
            if result.contains_key(&key) {
                // Do not expose provider-controlled field content in the error.
                return Err(A::Error::custom("duplicate JSON object field"));
            }
            let Unique(value) = values.next_value()?;
            result.insert(key, value);
        }
        Ok(Unique(Value::Object(result)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unique_objects_preserve_numbers_strings_arrays_and_null() {
        let raw = br#"{"usage":{"prompt_tokens":92,"completion_tokens":31,"total_tokens":123,"prompt_cache_hit_tokens":0,"prompt_cache_miss_tokens":92,"prompt_tokens_details":{"cached_tokens":0},"completion_tokens_details":{"reasoning_tokens":29}},"model":"deepseek-v4-pro","values":[null,true,-1,1.5,18446744073709551615,"OK",{"x":1},{"x":2}],"literal":"{\"x\":1,\"x\":2}"}"#;
        let parsed = parse(raw).expect("unique response");
        assert_eq!(parsed, serde_json::from_slice::<Value>(raw).expect("JSON"));
        assert!(super::super::admission::complete_usage("deepseek", &parsed));
    }
    #[test]
    fn duplicate_keys_are_rejected_recursively_after_unescaping() {
        for raw in [
            r#"{"x":1,"x":1}"#,
            r#"{"x":1,"\u0078":2}"#,
            r#"{"usage":{"cached_tokens":1,"cached_tokens":0}}"#,
            r#"[{"x":1,"x":2}]"#,
            r#"{"x":1} {"x":2}"#,
            r#"{"x":NaN}"#,
        ] {
            assert!(parse(raw.as_bytes()).is_err(), "{raw}");
        }
        let too_deep = "[".repeat(256) + "0" + &"]".repeat(256);
        assert!(parse(too_deep.as_bytes()).is_err());
    }
}
