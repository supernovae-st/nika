// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Pure JSON value classification shared by schema diagnostics.

use serde_json::Value;

/// Whether a literal JSON value matches a JSON-Schema `type` name. `integer`
/// is the number subtype (an integral value matches both `integer` and
/// `number`; a fractional number matches only `number`).
#[must_use]
pub fn json_matches_type(value: &Value, type_name: &str) -> bool {
    match type_name {
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => {
            value.is_i64() || value.is_u64() || value.as_f64().is_some_and(|n| n.fract() == 0.0)
        }
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        "null" => value.is_null(),
        _ => false,
    }
}

/// A lower bound strictly above its paired upper bound is unsatisfiable — no
/// value can be both. Covers the four JSON-Schema min/max pairs (numeric
/// range · string length · array length · object property count).
/// A short JSON kind name for diagnostics.
#[must_use]
pub fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}
