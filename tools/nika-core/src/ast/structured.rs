//! Structured Output Configuration
//!
//! Defines task-level configuration for JSON Schema validation:
//! - `StructuredOutputSpec`: Schema + layer enables + retry config
//!
//! Works with the `StructuredOutputEngine` for ~99.99% compliance.

use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use std::fmt;

use super::output::SchemaRef;

/// Structured output configuration for a task
///
/// Controls how the StructuredOutputEngine validates and repairs output.
/// Supports shorthand (just schema path) or full configuration.
///
/// # Examples
///
/// Shorthand (schema file path):
/// ```yaml
/// structured: ./schemas/user.json
/// ```
///
/// Full configuration:
/// ```yaml
/// structured:
///   schema: ./schemas/user.json
///   max_retries: 3
///   enable_repair: true
///   repair_model: claude-sonnet-4-6
/// ```
///
/// Inline schema:
/// ```yaml
/// structured:
///   schema:
///     type: object
///     properties:
///       name:
///         type: string
///     required: [name]
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct StructuredOutputSpec {
    /// JSON Schema reference (inline or file path).
    ///
    /// `None` when `from_example` is set — schema is derived at runtime.
    /// Use `StructuredOutputEngine::load_schema()` which handles derivation correctly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<SchemaRef>,

    /// JSON example — Nika auto-derives the JSON Schema from this at runtime.
    ///
    /// Mutually exclusive with `schema`. When set, `schema` is a placeholder `{}`.
    /// The engine calls `json_to_schema()` on the example before any validation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_example: Option<SchemaRef>,

    /// Enable Layer 0: Tool injection (DynamicSubmitTool)
    /// When true, injects a synthetic submit_result tool for provider-side
    /// schema enforcement before falling through to post-processing layers.
    /// Default: true
    #[serde(default)]
    pub enable_tool_injection: Option<bool>,

    /// Enable Layer 3: Retry with feedback
    /// Default: true
    #[serde(default)]
    pub enable_retry: Option<bool>,

    /// Enable Layer 4: LLM repair
    /// Default: true
    #[serde(default)]
    pub enable_repair: Option<bool>,

    /// Maximum retry attempts (Layer 3)
    /// Default: 2
    #[serde(default)]
    pub max_retries: Option<u8>,

    /// Model to use for repair (Layer 4)
    /// Default: same as task model
    #[serde(default)]
    pub repair_model: Option<String>,

    /// When true, derived `from_example` schemas add `additionalProperties: false`
    /// to all object schemas recursively. Prevents the LLM from injecting extra keys.
    /// Default: false. Only meaningful when `from_example` is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

impl StructuredOutputSpec {
    /// Create with a schema reference
    pub fn with_schema(schema: SchemaRef) -> Self {
        Self {
            schema: Some(schema),
            from_example: None,
            enable_tool_injection: None,
            enable_retry: None,
            enable_repair: None,
            max_retries: None,
            repair_model: None,
            strict: None,
        }
    }

    /// Create with an example file (schema derived at runtime).
    ///
    /// The file is read at execution time and its JSON structure is used to derive
    /// the JSON Schema for output validation.
    ///
    /// NOTE: The LLM prompt will NOT include the example structure — only a generic
    /// "output valid JSON" instruction. Use `with_example_inline()` when you want
    /// the example shown in the prompt.
    pub fn with_example_file(path: impl Into<String>) -> Self {
        Self {
            schema: None,
            from_example: Some(SchemaRef::File(path.into())),
            enable_tool_injection: None,
            enable_retry: None,
            enable_repair: None,
            max_retries: None,
            repair_model: None,
            strict: None,
        }
    }

    /// Create with an inline JSON example (schema derived at runtime).
    ///
    /// The example structure is injected into the LLM prompt AND used for validation.
    /// This gives the LLM full context about the expected output shape.
    pub fn with_example_inline(example: serde_json::Value) -> Self {
        Self {
            schema: None,
            from_example: Some(SchemaRef::Inline(example)),
            enable_tool_injection: None,
            enable_retry: None,
            enable_repair: None,
            max_retries: None,
            repair_model: None,
            strict: None,
        }
    }

    /// Create with an inline JSON schema
    pub fn with_inline_schema(schema: serde_json::Value) -> Self {
        Self::with_schema(SchemaRef::Inline(schema))
    }

    /// Create with a file path
    pub fn with_file_schema(path: impl Into<String>) -> Self {
        Self::with_schema(SchemaRef::File(path.into()))
    }

    /// Get max_retries with default
    pub fn max_retries_or_default(&self) -> u8 {
        self.max_retries.unwrap_or(2)
    }

    /// Check if Layer 2 (tool_use) is enabled
    pub fn enable_tool_injection_or_default(&self) -> bool {
        self.enable_tool_injection.unwrap_or(true)
    }

    /// Check if Layer 3 (retry) is enabled
    pub fn enable_retry_or_default(&self) -> bool {
        self.enable_retry.unwrap_or(true)
    }

    /// Check if Layer 4 (repair) is enabled
    pub fn enable_repair_or_default(&self) -> bool {
        self.enable_repair.unwrap_or(true)
    }

    /// Convert to OutputPolicy for executor Layer 0 dispatch.
    ///
    /// The executor's `run_infer()` uses `OutputPolicy` to trigger Layer 0 tool injection
    /// and prompt schema instructions. This bridges `structured:` config to that path.
    /// The original spec is preserved in `source_structured_spec` so that
    /// `to_structured_spec()` can roundtrip without losing layer toggle config.
    pub fn to_output_policy(&self) -> super::output::OutputPolicy {
        super::output::OutputPolicy {
            format: super::output::OutputFormat::Json,
            schema: self.schema.clone(),
            from_example: self.from_example.clone(),
            max_retries: self.max_retries,
            source_structured_spec: Some(self.clone()),
        }
    }
}

impl<'de> Deserialize<'de> for StructuredOutputSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StructuredOutputSpecVisitor;

        impl<'de> Visitor<'de> for StructuredOutputSpecVisitor {
            type Value = StructuredOutputSpec;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter
                    .write_str("a schema path string or structured output configuration object")
            }

            // Shorthand: `structured: ./schema.json`
            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(StructuredOutputSpec::with_file_schema(v))
            }

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(StructuredOutputSpec::with_file_schema(v))
            }

            // Full form: `structured: { schema: ..., max_retries: ... }`
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut schema: Option<SchemaRef> = None;
                let mut from_example: Option<SchemaRef> = None;
                let mut enable_tool_injection: Option<bool> = None;
                let mut enable_retry: Option<bool> = None;
                let mut enable_repair: Option<bool> = None;
                let mut max_retries: Option<u8> = None;
                let mut repair_model: Option<String> = None;
                let mut strict: Option<bool> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "schema" => {
                            schema = Some(map.next_value()?);
                        }
                        "from_example" => {
                            from_example = Some(map.next_value()?);
                        }
                        "enable_tool_injection" => {
                            enable_tool_injection = Some(map.next_value()?);
                        }
                        "enable_retry" => {
                            enable_retry = Some(map.next_value()?);
                        }
                        "enable_repair" => {
                            enable_repair = Some(map.next_value()?);
                        }
                        "max_retries" => {
                            max_retries = Some(map.next_value()?);
                        }
                        "repair_model" => {
                            repair_model = Some(map.next_value()?);
                        }
                        "strict" => {
                            strict = Some(map.next_value()?);
                        }
                        _ => {
                            // Ignore unknown fields
                            let _: serde_json::Value = map.next_value()?;
                        }
                    }
                }

                // from_example and schema are mutually exclusive.
                // When from_example is set, schema is None (derived at runtime by the engine).
                if schema.is_none() && from_example.is_none() {
                    return Err(de::Error::missing_field("schema or from_example"));
                }

                Ok(StructuredOutputSpec {
                    schema,
                    from_example,
                    enable_tool_injection,
                    enable_retry,
                    enable_repair,
                    max_retries,
                    repair_model,
                    strict,
                })
            }
        }

        deserializer.deserialize_any(StructuredOutputSpecVisitor)
    }
}

// Re-export from the dedicated schema module.
// json_to_schema lives in nika_core::schema but is re-exported here
// so existing callers (nika-engine via `crate::ast::structured::json_to_schema`) keep working.
pub use crate::schema::{json_to_schema, json_to_schema_strict};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde_yaml;

    #[test]
    fn parse_shorthand_file_path() {
        let yaml = "structured: ./schemas/user.json";
        let spec: StructuredOutputSpec =
            serde_yaml::from_str(&yaml.replace("structured: ", "")).unwrap();
        assert!(matches!(spec.schema, Some(SchemaRef::File(ref p)) if p == "./schemas/user.json"));
    }

    #[test]
    fn parse_full_form_with_file() {
        let yaml = r#"
schema: ./schemas/user.json
max_retries: 3
enable_repair: false
"#;
        let spec: StructuredOutputSpec = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(spec.schema, Some(SchemaRef::File(ref p)) if p == "./schemas/user.json"));
        assert_eq!(spec.max_retries, Some(3));
        assert_eq!(spec.enable_repair, Some(false));
    }

    #[test]
    fn parse_full_form_with_inline_schema() {
        let yaml = r#"
schema:
  type: object
  properties:
    name:
      type: string
  required:
    - name
max_retries: 2
"#;
        let spec: StructuredOutputSpec = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(spec.schema, Some(SchemaRef::Inline(_))));
        assert_eq!(spec.max_retries, Some(2));
    }

    #[test]
    fn defaults_are_applied() {
        let spec = StructuredOutputSpec::with_file_schema("./test.json");
        assert_eq!(spec.max_retries_or_default(), 2);
        assert!(spec.enable_tool_injection_or_default());
        assert!(spec.enable_retry_or_default());
        assert!(spec.enable_repair_or_default());
    }

    #[test]
    fn constructors_work() {
        let file_spec = StructuredOutputSpec::with_file_schema("./test.json");
        assert!(matches!(file_spec.schema, Some(SchemaRef::File(_))));

        let inline_spec = StructuredOutputSpec::with_inline_schema(serde_json::json!({
            "type": "object"
        }));
        assert!(matches!(inline_spec.schema, Some(SchemaRef::Inline(_))));
    }

    #[test]
    fn parse_with_repair_model() {
        let yaml = r#"
schema: ./test.json
repair_model: claude-sonnet-4-6
"#;
        let spec: StructuredOutputSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.repair_model, Some("claude-sonnet-4-6".to_string()));
    }

    #[test]
    fn parse_all_layer_toggles() {
        let yaml = r#"
schema: ./test.json
enable_tool_injection: false
enable_retry: true
enable_repair: false
"#;
        let spec: StructuredOutputSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.enable_tool_injection, Some(false));
        assert_eq!(spec.enable_retry, Some(true));
        assert_eq!(spec.enable_repair, Some(false));
    }

    #[test]
    fn legacy_enable_tool_use_is_ignored() {
        let yaml = r#"
schema: ./test.json
enable_tool_use: false
"#;
        let spec: StructuredOutputSpec = serde_yaml::from_str(yaml).unwrap();
        // enable_tool_use is no longer recognized; only enable_tool_injection works
        assert_eq!(spec.enable_tool_injection, None);
    }

    #[test]
    fn serialize_to_json() {
        let spec = StructuredOutputSpec::with_file_schema("./test.json");
        let json = serde_json::to_string(&spec).unwrap();
        assert!(json.contains("./test.json"));
    }

    #[test]
    fn parse_from_example_file() {
        let yaml = r#"
from_example: ./structure.json
enable_repair: true
"#;
        let spec: StructuredOutputSpec = serde_yaml::from_str(yaml).unwrap();
        assert!(
            spec.schema.is_none(),
            "schema should be None when from_example is set"
        );
        assert!(spec.from_example.is_some());
        assert!(
            matches!(spec.from_example.as_ref().unwrap(), SchemaRef::File(ref p) if p == "./structure.json")
        );
        assert_eq!(spec.enable_repair, Some(true));
    }

    #[test]
    fn parse_from_example_inline() {
        let yaml = r#"
from_example:
  title: "hello"
  count: 42
"#;
        let spec: StructuredOutputSpec = serde_yaml::from_str(yaml).unwrap();
        assert!(spec.from_example.is_some());
        assert!(matches!(
            spec.from_example.as_ref().unwrap(),
            SchemaRef::Inline(_)
        ));
    }

    #[test]
    fn parse_both_schema_and_from_example_are_preserved() {
        // When both are set, both fields survive deserialization.
        // load_schema() in nika-engine always checks from_example first,
        // so schema acts as a fallback — but we don't test that here (engine tests cover it).
        let yaml = r#"
schema:
  type: object
from_example: ./structure.json
"#;
        let spec: StructuredOutputSpec = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(spec.schema, Some(SchemaRef::Inline(_))));
        assert!(spec.from_example.is_some());
    }

    // json_to_schema tests are in nika_core::schema::tests (dedicated module).
    // This test verifies the re-export works.
    #[test]
    fn json_to_schema_reexport_works() {
        let schema = json_to_schema(&serde_json::json!({"x": 1}));
        assert_eq!(schema["type"], "object");
    }
}
