//! Output Policy - format and validation configuration (v0.2)
//!
//! Defines how task output should be formatted and validated:
//! - `OutputFormat`: Text (default) or JSON
//! - `SchemaRef`: Inline JSON Schema object or file path
//! - `OutputPolicy`: Format + optional schema validation + retry config

use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::fmt;

/// Reference to a JSON Schema - either inline or file path
#[derive(Debug, Clone)]
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

    /// Maximum retry attempts on validation failure (default: 2)
    #[serde(default)]
    pub max_retries: Option<u8>,

    /// Save output to file path
    #[serde(default)]
    pub save: Option<String>,
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

    /// YAML formatted output (v0.10+)
    Yaml,

    /// Markdown formatted output (v0.10+)
    Markdown,
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
    fn parse_save_path() {
        let yaml = r#"
format: json
save: output/result.json
"#;
        let policy: OutputPolicy = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(policy.save.as_deref(), Some("output/result.json"));
    }

    #[test]
    fn default_is_text() {
        let policy = OutputPolicy::default();
        assert_eq!(policy.format, OutputFormat::Text);
        assert!(policy.schema.is_none());
        assert!(policy.max_retries.is_none());
        assert!(policy.save.is_none());
    }
}
