// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Structured output specification — the `structured:` / `output:` section.
//!
//! Controls JSON Schema enforcement on LLM output. The orchestration
//! (retry loop, repair prompts) lives in `nika-verb-infer` (L2);
//! only the configuration types live here.

use serde::{Deserialize, Serialize};

use super::output::SchemaRef;

/// Structured output specification for infer tasks.
///
/// When set, the runtime extracts JSON from the LLM response, validates
/// it against the schema, and optionally retries or repairs failures.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct StructuredOutputSpec {
    /// Inline or file reference to a JSON Schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<SchemaRef>,
    /// Example JSON to derive a schema from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_example: Option<serde_json::Value>,
    /// Maximum retries on validation failure.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Whether to attempt LLM-based repair of invalid output.
    #[serde(default)]
    pub enable_repair: bool,
    /// Model to use for repair (defaults to the task's model).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repair_model: Option<String>,
    /// Whether to use provider-native structured output (tool injection).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_tool_injection: Option<bool>,
    /// Whether to enforce strict schema mode (no additional properties).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

fn default_max_retries() -> u32 {
    2
}

impl StructuredOutputSpec {
    /// Create a new structured output spec with an inline schema.
    #[must_use]
    pub fn with_schema(schema: SchemaRef) -> Self {
        Self {
            schema: Some(schema),
            from_example: None,
            max_retries: default_max_retries(),
            enable_repair: false,
            repair_model: None,
            enable_tool_injection: None,
            strict: None,
        }
    }

    /// Create a new structured output spec from an example JSON value.
    #[must_use]
    pub fn from_example(example: serde_json::Value) -> Self {
        Self {
            schema: None,
            from_example: Some(example),
            max_retries: default_max_retries(),
            enable_repair: false,
            repair_model: None,
            enable_tool_injection: None,
            strict: None,
        }
    }

    /// Create an empty structured output spec.
    #[must_use]
    pub fn new() -> Self {
        Self {
            schema: None,
            from_example: None,
            max_retries: default_max_retries(),
            enable_repair: false,
            repair_model: None,
            enable_tool_injection: None,
            strict: None,
        }
    }
}

impl Default for StructuredOutputSpec {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_schema() {
        let spec = StructuredOutputSpec::with_schema(SchemaRef::inline(
            serde_json::json!({"type": "object"}),
        ));
        assert!(spec.schema.is_some());
        assert_eq!(spec.max_retries, 2);
        assert!(!spec.enable_repair);
    }

    #[test]
    fn from_example_constructor() {
        let spec = StructuredOutputSpec::from_example(serde_json::json!({"name": "test"}));
        assert!(spec.from_example.is_some());
        assert!(spec.schema.is_none());
    }

    #[test]
    fn serde_roundtrip() {
        let spec = StructuredOutputSpec {
            schema: Some(SchemaRef::file("schema.json")),
            max_retries: 5,
            enable_repair: true,
            repair_model: Some("claude-haiku-3-5-20241022".into()),
            strict: Some(true),
            ..StructuredOutputSpec::new()
        };
        let json = serde_json::to_string(&spec).expect("serialize");
        let back: StructuredOutputSpec = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.max_retries, 5);
        assert!(back.enable_repair);
        assert_eq!(back.strict, Some(true));
    }

    #[test]
    fn default_max_retries_is_2() {
        let spec = StructuredOutputSpec::new();
        assert_eq!(spec.max_retries, 2);
    }

    #[test]
    fn serde_empty_object() {
        let spec: StructuredOutputSpec = serde_json::from_str("{}").expect("deserialize");
        assert!(spec.schema.is_none());
        assert_eq!(spec.max_retries, 2);
    }
}
