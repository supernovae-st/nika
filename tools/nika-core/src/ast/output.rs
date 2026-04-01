//! Output Policy - format and validation configuration
//!
//! Defines how task output should be formatted and validated:
//! - `OutputFormat`: Text (default) or JSON
//! - `SchemaRef`: Inline JSON Schema object or file path
//! - `OutputPolicy`: Format + optional schema validation + retry config

use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::fmt;

/// Reference to a JSON Schema - either inline or file path
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum SchemaRef {
    /// Inline JSON Schema object
    Inline(JsonValue),
    /// Path to JSON Schema file
    File(String),
}

impl<'de> Deserialize<'de> for SchemaRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SchemaRefVisitor;

        impl<'de> Visitor<'de> for SchemaRefVisitor {
            type Value = SchemaRef;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a JSON Schema object or a file path string")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(SchemaRef::File(v.to_string()))
            }

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(SchemaRef::File(v))
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let value = JsonValue::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(SchemaRef::Inline(value))
            }
        }

        deserializer.deserialize_any(SchemaRefVisitor)
    }
}

/// Output policy configuration
#[derive(Debug, Clone, Deserialize, Default)]
pub struct OutputPolicy {
    /// Output format (text or json)
    #[serde(default)]
    pub format: OutputFormat,

    /// JSON Schema for output validation (inline object or file path)
    #[serde(default)]
    pub schema: Option<SchemaRef>,

    /// JSON example — Nika auto-derives the JSON Schema from this at runtime.
    /// Alternative to `schema`. When set, takes precedence over `schema`.
    #[serde(default)]
    pub from_example: Option<SchemaRef>,

    /// Maximum retry attempts on validation failure (default: 2)
    #[serde(default)]
    pub max_retries: Option<u8>,

    /// Original StructuredOutputSpec when this policy was bridged from `structured:`.
    /// Preserves user's layer toggle config (enable_tool_injection, enable_repair, etc.)
    /// that would otherwise be lost in the OutputPolicy→StructuredOutputSpec roundtrip.
    #[serde(skip)]
    pub source_structured_spec: Option<super::structured::StructuredOutputSpec>,
}

impl OutputPolicy {
    /// Check if this policy requires structured output validation.
    ///
    /// Returns true when format is JSON and a schema is provided.
    /// This is used by the executor to decide whether to use StructuredOutputEngine.
    pub fn is_structured(&self) -> bool {
        self.format == OutputFormat::Json && (self.schema.is_some() || self.from_example.is_some())
    }

    /// Convert to StructuredOutputSpec for use with StructuredOutputEngine.
    ///
    /// Returns None if this policy doesn't require structured output.
    /// If this policy was bridged from a `structured:` config (via `to_output_policy()`),
    /// returns the original spec with all user-configured layer toggles preserved.
    /// Otherwise, constructs a spec with permissive defaults (all layers enabled).
    pub fn to_structured_spec(&self) -> Option<super::structured::StructuredOutputSpec> {
        if !self.is_structured() {
            return None;
        }

        // Return original spec if available (preserves user's layer toggles)
        if let Some(ref spec) = self.source_structured_spec {
            return Some(spec.clone());
        }

        // Fallback: construct spec from OutputPolicy fields (output: block path)
        Some(super::structured::StructuredOutputSpec {
            schema: self.schema.clone(),
            from_example: self.from_example.clone(),
            enable_tool_injection: None,
            enable_retry: Some(true),
            enable_repair: Some(true),
            max_retries: self.max_retries,
            repair_model: None,
            strict: None,
        })
    }
}

/// Output format enum
#[derive(Debug, Clone, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// Raw text output (default)
    #[default]
    Text,

    /// JSON parsed output
    Json,

    /// YAML formatted output
    Yaml,

    /// Markdown formatted output
    Markdown,

    /// Binary (raw bytes, not text content)
    Binary,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde_yaml;

    #[test]
    fn parse_text_format() {
        let yaml = "format: text";
        let policy: OutputPolicy = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(policy.format, OutputFormat::Text);
        assert!(policy.schema.is_none());
    }

    #[test]
    fn parse_json_with_schema_file() {
        let yaml = r#"
            format: json
            schema: .nika/schemas/result.json
        "#;
        let policy: OutputPolicy = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(policy.format, OutputFormat::Json);
        assert!(
            matches!(policy.schema, Some(SchemaRef::File(ref p)) if p == ".nika/schemas/result.json")
        );
    }

    #[test]
    fn parse_json_with_inline_schema() {
        let yaml = r#"
format: json
schema:
  type: object
  properties:
    name:
      type: string
  required:
    - name
"#;
        let policy: OutputPolicy = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(policy.format, OutputFormat::Json);
        assert!(matches!(policy.schema, Some(SchemaRef::Inline(_))));

        // Verify schema content
        if let Some(SchemaRef::Inline(schema)) = &policy.schema {
            assert_eq!(schema["type"], "object");
            assert!(schema["properties"]["name"].is_object());
        }
    }

    #[test]
    fn parse_max_retries() {
        let yaml = r#"
format: json
max_retries: 3
"#;
        let policy: OutputPolicy = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(policy.max_retries, Some(3));
    }

    #[test]
    fn default_is_text() {
        let policy = OutputPolicy::default();
        assert_eq!(policy.format, OutputFormat::Text);
        assert!(policy.schema.is_none());
        assert!(policy.max_retries.is_none());
    }

    // ========== is_structured() tests ==========

    #[test]
    fn is_structured_true_when_json_with_schema() {
        let yaml = r#"
format: json
schema:
  type: object
"#;
        let policy: OutputPolicy = serde_yaml::from_str(yaml).unwrap();
        assert!(policy.is_structured());
    }

    #[test]
    fn is_structured_false_when_text() {
        let policy = OutputPolicy::default();
        assert!(!policy.is_structured());
    }

    #[test]
    fn is_structured_false_when_json_without_schema() {
        let yaml = "format: json";
        let policy: OutputPolicy = serde_yaml::from_str(yaml).unwrap();
        assert!(!policy.is_structured());
    }

    #[test]
    fn is_structured_true_when_json_with_from_example() {
        let yaml = r#"
format: json
from_example:
  name: "Alice"
  age: 30
"#;
        let policy: OutputPolicy = serde_yaml::from_str(yaml).unwrap();
        assert!(policy.is_structured());
    }

    #[test]
    fn is_structured_false_when_text_with_schema() {
        // Edge case: text format with schema should NOT be structured
        let yaml = r#"
format: text
schema:
  type: object
"#;
        let policy: OutputPolicy = serde_yaml::from_str(yaml).unwrap();
        assert!(!policy.is_structured());
    }

    // ========== to_structured_spec() tests ==========

    #[test]
    fn to_structured_spec_returns_spec_when_structured() {
        let yaml = r#"
format: json
schema:
  type: object
  properties:
    name:
      type: string
max_retries: 5
"#;
        let policy: OutputPolicy = serde_yaml::from_str(yaml).unwrap();
        let spec = policy.to_structured_spec();
        assert!(spec.is_some());

        let spec = spec.unwrap();
        assert!(matches!(spec.schema, Some(SchemaRef::Inline(_))));
        assert_eq!(spec.max_retries, Some(5));
        assert_eq!(spec.enable_retry, Some(true));
        assert_eq!(spec.enable_repair, Some(true));
    }

    #[test]
    fn to_structured_spec_returns_none_when_not_structured() {
        let policy = OutputPolicy::default();
        assert!(policy.to_structured_spec().is_none());
    }

    #[test]
    fn to_structured_spec_with_file_schema() {
        let yaml = r#"
format: json
schema: ./schemas/user.json
"#;
        let policy: OutputPolicy = serde_yaml::from_str(yaml).unwrap();
        let spec = policy.to_structured_spec();
        assert!(spec.is_some());

        let spec = spec.unwrap();
        assert!(matches!(spec.schema, Some(SchemaRef::File(ref p)) if p == "./schemas/user.json"));
    }
}
