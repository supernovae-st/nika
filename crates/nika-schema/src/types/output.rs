// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Output format, schema references, and output policy types.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Output format for task results.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// Plain text output.
    #[default]
    Text,
    /// JSON output.
    Json,
    /// YAML output.
    Yaml,
    /// Markdown output.
    Markdown,
    /// Binary output (base64-encoded in events).
    Binary,
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text => write!(f, "text"),
            Self::Json => write!(f, "json"),
            Self::Yaml => write!(f, "yaml"),
            Self::Markdown => write!(f, "markdown"),
            Self::Binary => write!(f, "binary"),
        }
    }
}

/// A reference to a JSON Schema — either inline or from a file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(untagged)]
pub enum SchemaRef {
    /// An inline JSON Schema definition.
    Inline(serde_json::Value),
    /// A file path to a JSON Schema file.
    File(String),
}

impl SchemaRef {
    /// Create an inline schema reference.
    #[must_use]
    pub fn inline(schema: serde_json::Value) -> Self {
        Self::Inline(schema)
    }

    /// Create a file schema reference.
    #[must_use]
    pub fn file(path: impl Into<String>) -> Self {
        Self::File(path.into())
    }

    /// Returns `true` if this is an inline schema.
    #[must_use]
    pub fn is_inline(&self) -> bool {
        matches!(self, Self::Inline(_))
    }

    /// Returns `true` if this is a file reference.
    #[must_use]
    pub fn is_file(&self) -> bool {
        matches!(self, Self::File(_))
    }
}

/// Output policy controlling how task output is formatted and validated.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OutputPolicy {
    /// Output format.
    #[serde(default)]
    pub format: OutputFormat,
    /// Optional JSON Schema for output validation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<SchemaRef>,
    /// Optional example-driven schema derivation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_example: Option<SchemaRef>,
    /// Maximum number of retries for schema validation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u8>,
}

impl OutputPolicy {
    /// Create a new output policy with text format.
    #[must_use]
    pub fn new() -> Self {
        Self {
            format: OutputFormat::default(),
            schema: None,
            from_example: None,
            max_retries: None,
        }
    }
}

impl Default for OutputPolicy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_format_default_is_text() {
        assert_eq!(OutputFormat::default(), OutputFormat::Text);
    }

    #[test]
    fn output_format_serde_roundtrip() {
        for format in [
            OutputFormat::Text,
            OutputFormat::Json,
            OutputFormat::Yaml,
            OutputFormat::Markdown,
            OutputFormat::Binary,
        ] {
            let json = serde_json::to_string(&format).expect("serialize");
            let back: OutputFormat = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, format);
        }
    }

    #[test]
    fn schema_ref_inline() {
        let schema = SchemaRef::inline(serde_json::json!({"type": "object"}));
        assert!(schema.is_inline());
        assert!(!schema.is_file());
    }

    #[test]
    fn schema_ref_file() {
        let schema = SchemaRef::file("schema.json");
        assert!(schema.is_file());
        assert!(!schema.is_inline());
    }

    #[test]
    fn schema_ref_serde_inline() {
        let schema = SchemaRef::inline(serde_json::json!({"type": "string"}));
        let json = serde_json::to_string(&schema).expect("serialize");
        let back: SchemaRef = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, schema);
    }

    #[test]
    fn schema_ref_file_serializes_as_string() {
        // Note: SchemaRef::File roundtrips as Inline(Value::String(...))
        // because serde(untagged) tries Inline first. The parser constructs
        // File variants manually — serde is only used for serialization output.
        let schema = SchemaRef::file("./output.schema.json");
        let json = serde_json::to_string(&schema).expect("serialize");
        assert_eq!(json, r#""./output.schema.json""#);
    }

    #[test]
    fn output_policy_serde_roundtrip() {
        let policy = OutputPolicy {
            format: OutputFormat::Json,
            schema: Some(SchemaRef::inline(serde_json::json!({"type": "object"}))),
            from_example: None,
            max_retries: Some(3),
        };
        let json = serde_json::to_string(&policy).expect("serialize");
        let back: OutputPolicy = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.format, OutputFormat::Json);
        assert!(back.schema.is_some());
        assert_eq!(back.max_retries, Some(3));
    }
}
