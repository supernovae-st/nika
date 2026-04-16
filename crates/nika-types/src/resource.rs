// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `OpenTelemetry`-compatible resource types.
//!
//! `Resource` describes the entity producing telemetry data (service name,
//! version, environment, etc.).

use serde::{Deserialize, Serialize};

/// A resource describing the telemetry-producing entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Resource {
    /// Service name (e.g., `"nika-engine"`).
    pub service_name: String,
    /// Attribute key-value pairs.
    pub attrs: Vec<KeyValue>,
}

impl Resource {
    /// Create a resource with a service name.
    #[must_use]
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            attrs: Vec::new(),
        }
    }

    /// Add an attribute (builder pattern).
    #[must_use]
    pub fn with_attr(mut self, key: impl Into<String>, value: Value) -> Self {
        self.attrs.push(KeyValue {
            key: key.into(),
            value,
        });
        self
    }
}

/// A key-value pair for resource attributes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct KeyValue {
    /// Attribute key.
    pub key: String,
    /// Attribute value.
    pub value: Value,
}

impl KeyValue {
    /// Create a new key-value pair.
    #[must_use]
    pub fn new(key: impl Into<String>, value: Value) -> Self {
        Self {
            key: key.into(),
            value,
        }
    }
}

/// A typed attribute value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum Value {
    /// String value.
    String(String),
    /// Integer value.
    Int(i64),
    /// Float value.
    Float(f64),
    /// Boolean value.
    Bool(bool),
}

impl Value {
    /// Create a string value.
    #[must_use]
    pub fn string(v: impl Into<String>) -> Self {
        Self::String(v.into())
    }

    /// Create an integer value.
    #[must_use]
    pub fn int(v: i64) -> Self {
        Self::Int(v)
    }

    /// Create a float value.
    #[must_use]
    pub fn float(v: f64) -> Self {
        Self::Float(v)
    }

    /// Create a boolean value.
    #[must_use]
    pub fn bool(v: bool) -> Self {
        Self::Bool(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_new() {
        let r = Resource::new("nika-engine");
        assert_eq!(r.service_name, "nika-engine");
        assert!(r.attrs.is_empty());
    }

    #[test]
    fn resource_with_attrs() {
        let r = Resource::new("nika-engine")
            .with_attr("version", Value::string("0.80.0"))
            .with_attr("environment", Value::string("production"));
        assert_eq!(r.attrs.len(), 2);
        assert_eq!(r.attrs[0].key, "version");
    }

    #[test]
    fn key_value_new() {
        let kv = KeyValue::new("host", Value::string("localhost"));
        assert_eq!(kv.key, "host");
    }

    #[test]
    fn value_variants() {
        assert!(matches!(Value::string("x"), Value::String(_)));
        assert!(matches!(Value::int(42), Value::Int(42)));
        assert!(matches!(
            Value::float(std::f64::consts::PI),
            Value::Float(_)
        ));
        assert!(matches!(Value::bool(true), Value::Bool(true)));
    }

    #[test]
    fn resource_serde_roundtrip() {
        let r = Resource::new("test").with_attr("k", Value::int(42));
        let json = serde_json::to_string(&r).expect("serialize");
        let back: Resource = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(r, back);
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn resource_types_are_send_sync() {
        _assert_send_sync::<Resource>();
        _assert_send_sync::<KeyValue>();
        _assert_send_sync::<Value>();
    }
}
