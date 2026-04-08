// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Null-guard dispatch helpers for `TransformOp::apply()`.
//!
//! These eliminate the repetitive 5-line null-guard + type-check pattern
//! used by ~25 simple transforms. Each helper wraps:
//! - Null input → `TransformError::NullInput`
//! - Wrong type → `type_mismatch(...)`
//! - Correct type → delegate to the body closure

use serde_json::Value;

use super::helpers::type_mismatch;
use super::TransformError;

/// Strict-null string transform: Null→Error, String→body(s), _→TypeMismatch.
#[inline]
pub fn strict_str(
    op: &'static str,
    value: &Value,
    body: impl FnOnce(&str) -> Result<Value, TransformError>,
) -> Result<Value, TransformError> {
    match value {
        Value::Null => Err(TransformError::NullInput { op }),
        Value::String(s) => body(s),
        _ => Err(type_mismatch(op, "string", value)),
    }
}

/// Strict-null array transform: Null→Error, Array→body(arr), _→TypeMismatch.
#[inline]
pub fn strict_arr(
    op: &'static str,
    value: &Value,
    body: impl FnOnce(&[Value]) -> Result<Value, TransformError>,
) -> Result<Value, TransformError> {
    match value {
        Value::Null => Err(TransformError::NullInput { op }),
        Value::Array(arr) => body(arr),
        _ => Err(type_mismatch(op, "array", value)),
    }
}

/// Strict-null object transform: Null→Error, Object→body(obj), _→TypeMismatch.
#[inline]
pub fn strict_obj(
    op: &'static str,
    value: &Value,
    body: impl FnOnce(&serde_json::Map<String, Value>) -> Result<Value, TransformError>,
) -> Result<Value, TransformError> {
    match value {
        Value::Null => Err(TransformError::NullInput { op }),
        Value::Object(obj) => body(obj),
        _ => Err(type_mismatch(op, "object", value)),
    }
}

/// Strict-null number transform: Null→Error, Number→body(n), _→TypeMismatch.
#[inline]
pub fn strict_num(
    op: &'static str,
    value: &Value,
    body: impl FnOnce(&serde_json::Number) -> Result<Value, TransformError>,
) -> Result<Value, TransformError> {
    match value {
        Value::Null => Err(TransformError::NullInput { op }),
        Value::Number(n) => body(n),
        _ => Err(type_mismatch(op, "number", value)),
    }
}
