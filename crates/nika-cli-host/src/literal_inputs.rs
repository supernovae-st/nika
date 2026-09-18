// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Bounded literal API values. This channel never interprets operator syntax.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;

use nika_schema::{raw::RawWorkflow, types::VarDecl};
use serde::Deserialize;
use serde_json::Value;

use crate::var_inputs::ValidatedInputs;

const MAX_BYTES: usize = 1024 * 1024;

/// Typed literal-input admission failure; no payload values appear in diagnostics.
#[derive(Debug)]
#[non_exhaustive]
pub struct Refusal {
    code: String,
    message: String,
}

impl Refusal {
    /// Stable machine code, including the runtime's required-input code.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }
    /// Human diagnostic; never used to derive the machine code.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

/// Read one literal JSON object, consuming at most 1 MiB plus one byte.
/// Duplicate keys at any depth refuse; no operator syntax is interpreted.
/// # Errors
/// Refuses unreadable, oversized, non-UTF-8, malformed or non-object input.
pub fn read(reader: impl Read) -> Result<BTreeMap<String, Value>, Refusal> {
    let mut bytes = Vec::with_capacity(MAX_BYTES + 1);
    reader
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            Refusal::new(
                "input_read_failed",
                format!("cannot read input stdin: {error}"),
            )
        })?;
    if bytes.len() > MAX_BYTES {
        return Err(Refusal::new(
            "inputs_too_large",
            "literal inputs exceed the 1 MiB byte limit",
        ));
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| Refusal::new("invalid_inputs_utf8", "literal inputs must be valid UTF-8"))?;
    let value: UniqueValue = serde_json::from_str(text).map_err(|error| {
        Refusal::new(
            "invalid_inputs_json",
            format!("invalid literal input JSON: {error}"),
        )
    })?;
    match value.0 {
        Value::Object(map) => Ok(map.into_iter().collect()),
        _ => Err(Refusal::new(
            "invalid_inputs_root",
            "literal inputs must be a JSON object (not null, an array or a scalar)",
        )),
    }
}

/// Bind only declared keys using the canonical type and required-input laws.
/// Provided values carry `api-caller`; declared defaults retain `file`.
/// # Errors
/// Refuses unknown keys, type mismatches or missing required inputs.
pub fn validate(
    values: &BTreeMap<String, Value>,
    wf: &RawWorkflow,
) -> Result<ValidatedInputs, Refusal> {
    for (name, value) in values {
        let Some((_, decl)) = wf.inputs.iter().find(|(key, _)| key.value == *name) else {
            return Err(Refusal::new(
                "unknown_input",
                "input key is not declared by this workflow",
            ));
        };
        if let VarDecl::Typed { r#type, .. } = decl {
            let ty = nika_types::types::parse_type(&r#type.value, &BTreeSet::new(), "inputs")
                .map_err(|_| {
                    Refusal::new(
                        "invalid_input_type",
                        "declared input type could not be resolved",
                    )
                })?;
            if !nika_types::types::fits(value, &ty, &BTreeMap::new()) {
                return Err(Refusal::new(
                    "input_type_mismatch",
                    "input JSON value does not conform to its declared type",
                ));
            }
        }
    }
    if let Some(error) = nika_runtime::required_inputs_refusal(wf, values) {
        return Err(Refusal::new(&error.spec_code(), error.to_string()));
    }
    let mut origins = nika_runtime::input_origins(wf, &BTreeMap::new(), &BTreeSet::new(), false);
    origins.extend(
        values
            .keys()
            .map(|name| (name.clone(), nika_runtime::InputOrigin::ApiCaller)),
    );
    Ok(ValidatedInputs {
        values: values.clone(),
        origins,
    })
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Refusal {}

/// Serde's JSON reader owns grammar, numbers, depth and Unicode. This visitor
/// changes only map insertion: duplicate keys refuse at every nesting level.
struct UniqueValue(Value);

impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(UniqueVisitor)
    }
}

struct UniqueVisitor;
impl<'de> serde::de::Visitor<'de> for UniqueVisitor {
    type Value = UniqueValue;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a JSON value with unique object keys")
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::Null))
    }
    fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
        Ok(UniqueValue(v.into()))
    }
    fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
        Ok(UniqueValue(v.into()))
    }
    fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
        Ok(UniqueValue(v.into()))
    }
    fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
        serde_json::Number::from_f64(v)
            .map(|n| UniqueValue(Value::Number(n)))
            .ok_or_else(|| E::custom("non-finite JSON number"))
    }
    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(UniqueValue(v.into()))
    }
    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut values = Vec::new();
        while let Some(UniqueValue(value)) = seq.next_element()? {
            values.push(value);
        }
        Ok(UniqueValue(Value::Array(values)))
    }
    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut values = serde_json::Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(serde::de::Error::custom("duplicate object key"));
            }
            let UniqueValue(value) = map.next_value()?;
            values.insert(key, value);
        }
        Ok(UniqueValue(Value::Object(values)))
    }
}

#[cfg(test)]
mod tests;
