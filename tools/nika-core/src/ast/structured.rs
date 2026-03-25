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
    /// JSON Schema reference (inline or file path)
    pub schema: SchemaRef,

    /// JSON example — Nika auto-derives the JSON Schema from this.
    /// Mutually exclusive with `schema`. When set, `schema` is derived at runtime.
    #[serde(default)]
    pub from_example: Option<SchemaRef>,

    /// Enable Layer 1: rig Extractor (Rust type extraction)
    /// Default: true
    #[serde(default)]
    pub enable_extractor: Option<bool>,

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
}

impl StructuredOutputSpec {
    /// Create with a schema reference
    pub fn with_schema(schema: SchemaRef) -> Self {
        Self {
            schema,
            from_example: None,
            enable_extractor: None,
            enable_tool_injection: None,
            enable_retry: None,
            enable_repair: None,
            max_retries: None,
            repair_model: None,
        }
    }

    /// Create with an example file (schema derived at runtime)
    pub fn with_example_file(path: impl Into<String>) -> Self {
        Self {
            schema: SchemaRef::Inline(serde_json::json!({})), // placeholder, replaced at load time
            from_example: Some(SchemaRef::File(path.into())),
            enable_extractor: None,
            enable_tool_injection: None,
            enable_retry: None,
            enable_repair: None,
            max_retries: None,
            repair_model: None,
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
            schema: Some(self.schema.clone()),
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
                let mut enable_extractor: Option<bool> = None;
                let mut enable_tool_injection: Option<bool> = None;
                let mut enable_retry: Option<bool> = None;
                let mut enable_repair: Option<bool> = None;
                let mut max_retries: Option<u8> = None;
                let mut repair_model: Option<String> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "schema" => {
                            schema = Some(map.next_value()?);
                        }
                        "from_example" => {
                            from_example = Some(map.next_value()?);
                        }
                        "enable_extractor" => {
                            enable_extractor = Some(map.next_value()?);
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
                        _ => {
                            // Ignore unknown fields
                            let _: serde_json::Value = map.next_value()?;
                        }
                    }
                }

                // from_example and schema are mutually exclusive.
                // When from_example is set, schema is a placeholder (derived at runtime by the engine).
                let schema = if from_example.is_some() {
                    schema.unwrap_or(SchemaRef::Inline(serde_json::json!({})))
                } else {
                    schema.ok_or_else(|| de::Error::missing_field("schema or from_example"))?
                };

                Ok(StructuredOutputSpec {
                    schema,
                    from_example,
                    enable_extractor,
                    enable_tool_injection,
                    enable_retry,
                    enable_repair,
                    max_retries,
                    repair_model,
                })
            }
        }

        deserializer.deserialize_any(StructuredOutputSpecVisitor)
    }
}

/// Derive a JSON Schema from a JSON example value.
///
/// Recursively inspects the example and produces a schema that would validate
/// any JSON with the same structure (same keys, same types, same nesting).
///
/// - Objects → `{ "type": "object", "properties": {...}, "required": [...] }`
/// - Arrays  → `{ "type": "array", "items": <schema of first element> }`
/// - Strings → `{ "type": "string" }`
/// - Numbers → `{ "type": "number" }` (integers get `"integer"`)
/// - Bools   → `{ "type": "boolean" }`
/// - Null    → `{ "type": "null" }`
pub fn json_to_schema(value: &serde_json::Value) -> serde_json::Value {
    use serde_json::{json, Value};
    match value {
        Value::Object(map) => {
            let mut properties = serde_json::Map::new();
            let required: Vec<Value> =
                map.keys().map(|k| Value::String(k.clone())).collect();
            for (key, val) in map {
                properties.insert(key.clone(), json_to_schema(val));
            }
            json!({
                "type": "object",
                "properties": Value::Object(properties),
                "required": required
            })
        }
        Value::Array(items) => {
            if let Some(first) = items.first() {
                json!({ "type": "array", "items": json_to_schema(first) })
            } else {
                json!({ "type": "array" })
            }
        }
        Value::String(_) => json!({ "type": "string" }),
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                json!({ "type": "integer" })
            } else {
                json!({ "type": "number" })
            }
        }
        Value::Bool(_) => json!({ "type": "boolean" }),
        Value::Null => json!({ "type": "null" }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde_yaml;

    #[test]
    fn parse_shorthand_file_path() {
        let yaml = "structured: ./schemas/user.json";
        let spec: StructuredOutputSpec =
            serde_yaml::from_str(&yaml.replace("structured: ", "")).unwrap();
        assert!(matches!(spec.schema, SchemaRef::File(ref p) if p == "./schemas/user.json"));
    }

    #[test]
    fn parse_full_form_with_file() {
        let yaml = r#"
schema: ./schemas/user.json
max_retries: 3
enable_repair: false
"#;
        let spec: StructuredOutputSpec = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(spec.schema, SchemaRef::File(ref p) if p == "./schemas/user.json"));
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
        assert!(matches!(spec.schema, SchemaRef::Inline(_)));
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
        assert!(matches!(file_spec.schema, SchemaRef::File(_)));

        let inline_spec = StructuredOutputSpec::with_inline_schema(serde_json::json!({
            "type": "object"
        }));
        assert!(matches!(inline_spec.schema, SchemaRef::Inline(_)));
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
enable_extractor: false
enable_tool_injection: false
enable_retry: true
enable_repair: false
"#;
        let spec: StructuredOutputSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.enable_extractor, Some(false));
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
        assert!(matches!(spec.from_example.as_ref().unwrap(), SchemaRef::Inline(_)));
    }

    #[test]
    fn schema_and_from_example_schema_wins() {
        let yaml = r#"
schema:
  type: object
from_example: ./structure.json
"#;
        let spec: StructuredOutputSpec = serde_yaml::from_str(yaml).unwrap();
        // Both are set — schema is preserved, from_example is also preserved
        assert!(matches!(spec.schema, SchemaRef::Inline(_)));
        assert!(spec.from_example.is_some());
    }

    #[test]
    fn json_to_schema_flat_object() {
        let example = serde_json::json!({
            "title": "hello",
            "count": 42
        });
        let schema = json_to_schema(&example);
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["title"]["type"], "string");
        assert_eq!(schema["properties"]["count"]["type"], "integer");
    }

    #[test]
    fn json_to_schema_nested() {
        let example = serde_json::json!({
            "head": { "title": "x", "rating": { "value": 4.7 } },
            "sections": [{ "type": "hero", "fields": { "title": "y" } }]
        });
        let schema = json_to_schema(&example);
        assert_eq!(schema["properties"]["head"]["type"], "object");
        assert_eq!(
            schema["properties"]["head"]["properties"]["rating"]["properties"]["value"]["type"],
            "number"
        );
        assert_eq!(schema["properties"]["sections"]["type"], "array");
        assert_eq!(
            schema["properties"]["sections"]["items"]["properties"]["type"]["type"],
            "string"
        );
    }

    #[test]
    fn json_to_schema_empty_array() {
        let schema = json_to_schema(&serde_json::json!([]));
        assert_eq!(schema["type"], "array");
        assert!(schema.get("items").is_none());
    }

    #[test]
    fn json_to_schema_primitives() {
        assert_eq!(json_to_schema(&serde_json::json!("hello"))["type"], "string");
        assert_eq!(json_to_schema(&serde_json::json!(true))["type"], "boolean");
        assert_eq!(json_to_schema(&serde_json::json!(null))["type"], "null");
        assert_eq!(json_to_schema(&serde_json::json!(1.5))["type"], "number");
        assert_eq!(json_to_schema(&serde_json::json!(42))["type"], "integer");
    }
}
