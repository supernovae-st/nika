# Feature: `from_example` — Derive JSON Schema from Example Files

> **For Claude on Nika repo:** Use `superpowers:executing-plans` to implement task-by-task.

**Goal:** Allow `structured:` and `output:` to accept a JSON example file (not a JSON Schema) and auto-derive the schema at runtime. Eliminates the need for external Python scripts to convert templates to schemas.

**Crate:** Changes span `nika-core` (AST + parsing) and `nika-engine` (runtime schema loading).

---

## Motivation

Users have JSON template/structure files like:
```json
{
  "head": {
    "title": "string",
    "description": "string",
    "rating": { "value": 4.7, "count": 1378 },
    "schemas": ["product"]
  },
  "sections": [
    { "type": "hero_template", "fields": { "title": "string" } }
  ]
}
```

These define the exact structure the LLM output must follow, but they are NOT valid JSON Schemas. Today, users must write a Python script to convert these to JSON Schema format before Nika can validate against them.

With `from_example`, users write:
```yaml
structured:
  from_example: ./structure.json
  enable_repair: true
  max_retries: 3
```

And Nika auto-derives the JSON Schema internally.

---

## Task 1: Add `json_to_schema` utility function

**File:** `tools/nika-core/src/ast/structured.rs` (add at bottom, before `#[cfg(test)]`)

**What:** A pure function that converts any `serde_json::Value` into its corresponding JSON Schema.

```rust
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
            let required: Vec<Value> = map.keys()
                .map(|k| Value::String(k.clone()))
                .collect();
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
```

**Tests** (add in the `#[cfg(test)] mod tests` block):

```rust
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
    assert_eq!(schema["properties"]["head"]["properties"]["rating"]["properties"]["value"]["type"], "number");
    assert_eq!(schema["properties"]["sections"]["type"], "array");
    assert_eq!(schema["properties"]["sections"]["items"]["properties"]["type"]["type"], "string");
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
    assert_eq!(json_to_schema(&serde_json::json!(3.14))["type"], "number");
    assert_eq!(json_to_schema(&serde_json::json!(42))["type"], "integer");
}
```

**Verify:** `cargo test -p nika-core json_to_schema`

---

## Task 2: Add `from_example` field to `StructuredOutputSpec`

**File:** `tools/nika-core/src/ast/structured.rs`

### 2a. Add field to struct (line ~48, after `schema`)

```rust
pub struct StructuredOutputSpec {
    /// JSON Schema reference (inline or file path)
    pub schema: SchemaRef,

    /// JSON example file — Nika auto-derives the JSON Schema from this.
    /// Mutually exclusive with `schema`. When set, `schema` is derived at runtime.
    #[serde(default)]
    pub from_example: Option<SchemaRef>,

    // ... rest unchanged
}
```

### 2b. Update constructors (add after `with_file_schema`)

```rust
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
```

### 2c. Update deserializer `visit_map` (line ~186, add arm in the match)

```rust
"from_example" => {
    // Parse as SchemaRef (supports string path or inline object)
    let val: SchemaRef = map.next_value()?;
    from_example = Some(val);
}
```

Declare `let mut from_example: Option<SchemaRef> = None;` alongside the other `let mut` declarations.

### 2d. Update the final construction in `visit_map` (line ~216)

Replace:
```rust
let schema = schema.ok_or_else(|| de::Error::missing_field("schema"))?;
```

With:
```rust
// from_example and schema are mutually exclusive
// from_example uses a placeholder schema (derived at runtime by the engine)
let schema = if from_example.is_some() {
    schema.unwrap_or(SchemaRef::Inline(serde_json::json!({})))
} else {
    schema.ok_or_else(|| de::Error::missing_field("schema or from_example"))?
};
```

And add `from_example` to the constructed `StructuredOutputSpec`.

### 2e. Tests

```rust
#[test]
fn parse_from_example_file() {
    let yaml = r#"
from_example: ./structure.json
enable_repair: true
"#;
    let spec: StructuredOutputSpec = serde_yaml::from_str(yaml).unwrap();
    assert!(spec.from_example.is_some());
    assert!(matches!(spec.from_example.unwrap(), SchemaRef::File(ref p) if p == "./structure.json"));
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
    assert!(matches!(spec.from_example.unwrap(), SchemaRef::Inline(_)));
}

#[test]
fn schema_and_from_example_schema_wins() {
    let yaml = r#"
schema:
  type: object
from_example: ./structure.json
"#;
    let spec: StructuredOutputSpec = serde_yaml::from_str(yaml).unwrap();
    // Both are set — schema takes priority (from_example is a convenience)
    assert!(matches!(spec.schema, SchemaRef::Inline(_)));
    assert!(spec.from_example.is_some());
}
```

**Verify:** `cargo test -p nika-core structured`

---

## Task 3: Handle `from_example` in `StructuredOutputEngine::load_schema`

**File:** `tools/nika-engine/src/runtime/structured_output.rs`, method `load_schema` (line ~191)

Replace the current `load_schema` with:

```rust
pub async fn load_schema(&mut self) -> Result<Arc<Value>, NikaError> {
    if self.compiled_schema.is_none() {
        let schema = if let Some(ref example_ref) = self.spec.from_example {
            // from_example: load example, derive schema
            let example_value = match example_ref {
                SchemaRef::Inline(v) => v.clone(),
                SchemaRef::File(path) => {
                    let content = tokio::fs::read_to_string(path).await.map_err(|e| {
                        NikaError::SchemaFailed {
                            details: format!("Failed to read example '{}': {}", path, e),
                        }
                    })?;
                    serde_json::from_str(&content).map_err(|e| NikaError::SchemaFailed {
                        details: format!("Invalid JSON in example '{}': {}", path, e),
                    })?
                }
            };
            // Derive JSON Schema from the example
            crate::ast::structured::json_to_schema(&example_value)
        } else {
            // Standard: load schema directly
            match &self.spec.schema {
                SchemaRef::Inline(v) => v.clone(),
                SchemaRef::File(path) => {
                    let content = tokio::fs::read_to_string(path).await.map_err(|e| {
                        NikaError::SchemaFailed {
                            details: format!("Failed to read schema '{}': {}", path, e),
                        }
                    })?;
                    serde_json::from_str(&content).map_err(|e| NikaError::SchemaFailed {
                        details: format!("Invalid JSON in schema '{}': {}", path, e),
                    })?
                }
            }
        };
        self.compiled_schema = Some(Arc::new(schema));
    }
    self.compiled_schema
        .clone()
        .ok_or_else(|| NikaError::SchemaFailed {
            details: "Schema compilation produced None (internal error)".to_string(),
        })
}
```

**Note:** `json_to_schema` needs to be `pub` in `nika-core` and importable from `nika-engine`. It already is if placed in `structured.rs` and the module is `pub use`'d.

**Verify:** `cargo test -p nika-engine structured`

---

## Task 4: Handle `from_example` in `build_json_schema_instruction`

**File:** `tools/nika-engine/src/runtime/executor/mod.rs`, method `build_json_schema_instruction` (line ~248)

Currently, when `SchemaRef::File` is used, the prompt injection only says "must be valid JSON" (no schema details). For `from_example`, we want to inject the example itself as the target structure.

Add a check for `from_example` before the existing logic:

```rust
pub(super) fn build_json_schema_instruction(
    output_policy: Option<&OutputPolicy>,
) -> Option<String> {
    let policy = output_policy?;
    if policy.format != OutputFormat::Json {
        return None;
    }

    // If the policy was bridged from a structured spec with from_example,
    // inject the example directly as the schema instruction
    if let Some(ref spec) = policy.source_structured_spec {
        if let Some(ref example_ref) = spec.from_example {
            if let SchemaRef::Inline(ref example) = example_ref {
                let example_str = serde_json::to_string_pretty(example).unwrap_or_default();
                return Some(format!(
                    "\n\n---\n\
                     CRITICAL OUTPUT REQUIREMENT:\n\
                     Your response MUST be valid JSON matching this exact structure:\n\n\
                     ```json\n{}\n```\n\n\
                     Rules:\n\
                     - Output ONLY the JSON object, no additional text\n\
                     - Do NOT wrap in markdown code blocks (no ```json)\n\
                     - All keys shown above must be present\n\
                     - Value types must match (strings, numbers, arrays, objects)",
                    example_str
                ));
            }
            // File-based from_example: can't inline at prompt-build time
            // (file is loaded async in load_schema), fall through to generic instruction
        }
    }

    // ... rest of existing logic unchanged
```

**Verify:** `cargo test -p nika-engine build_json_schema`

---

## Task 5: Update `OutputPolicy` bridge

**File:** `tools/nika-core/src/ast/output.rs`

The `OutputPolicy.from_example` doesn't exist — `from_example` lives on `StructuredOutputSpec`. The bridge `to_output_policy()` already preserves `source_structured_spec`, so the `from_example` field survives the roundtrip. No changes needed here unless we want `output: { from_example: ... }` syntax too.

**Optional enhancement** (if desired): Add `from_example` to `OutputPolicy` directly:

```yaml
# This would also work:
output:
  format: json
  from_example: ./structure.json
```

This is nice but not required for V1. Skip for now.

---

## Task 6: Gate test

**File:** `tools/nika/examples/gates/feature/structured-from-example.nika.yaml`

```yaml
# FEATURE: structured output from example file (auto-derived schema)
schema: nika/workflow@0.12
model: gpt-4.1-mini
workflow: feat-structured-from-example

tasks:
  - id: gen_example
    description: "Write example file for the test"
    exec: |
      echo '{"name": "test", "age": 25, "active": true}' > /tmp/nika-test-example.json
      echo "OK"

  - id: extract
    depends_on: [gen_example]
    infer: "Return a JSON object about a person named Alice who is 30 and active"
    provider: openai
    max_tokens: 100
    structured:
      from_example:
        name: "example"
        age: 25
        active: true

  - id: extract_file
    depends_on: [gen_example]
    infer: "Return a JSON object about a person named Bob who is 40 and not active"
    provider: openai
    max_tokens: 100
    structured:
      from_example: /tmp/nika-test-example.json
      enable_repair: true
```

**Verify:** `nika check examples/gates/feature/structured-from-example.nika.yaml && nika run examples/gates/feature/structured-from-example.nika.yaml`

---

## Task 7: Update JSON Schema validation file

**File:** `tools/nika/schemas/nika-workflow.schema.json`

The `StructuredOutputSpec` schema currently has `"required": ["schema"]` and `"additionalProperties": false`. Changes needed:

1. Remove `"schema"` from `required` (it's optional when `from_example` is set)
2. Add `from_example` property to the object form
3. Add a `oneOf` or conditional to require either `schema` OR `from_example`

Current object form in `$defs.StructuredOutputSpec.oneOf[1]`:
```json
{
  "type": "object",
  "required": ["schema"],
  "additionalProperties": false,
  "properties": {
    "schema": { ... },
    "enable_extractor": { ... },
    ...
  }
}
```

Change to:
```json
{
  "type": "object",
  "additionalProperties": false,
  "anyOf": [
    { "required": ["schema"] },
    { "required": ["from_example"] }
  ],
  "properties": {
    "schema": {
      "oneOf": [
        { "type": "object", "description": "Inline JSON Schema object" },
        { "type": "string", "description": "Path to JSON Schema file" }
      ],
      "description": "JSON Schema for output validation (inline object or file path)"
    },
    "from_example": {
      "oneOf": [
        { "type": "object", "description": "Inline JSON example object (schema derived at runtime)" },
        { "type": "string", "description": "Path to JSON example file (schema derived at runtime)" }
      ],
      "description": "JSON example file — Nika auto-derives the JSON Schema from this structure"
    },
    "enable_extractor": { "type": "boolean", "default": true },
    "enable_tool_injection": { "type": "boolean", "default": true },
    "enable_tool_use": { "type": "boolean", "default": true },
    "enable_retry": { "type": "boolean", "default": true },
    "enable_repair": { "type": "boolean", "default": true },
    "max_retries": { "type": "integer", "minimum": 0, "maximum": 10, "default": 2 },
    "repair_model": { "type": "string" }
  }
}
```

**Verify:** `nika check examples/gates/feature/structured-from-example.nika.yaml` passes schema validation.

---

## Summary

| Task | File | Change | Effort |
|------|------|--------|--------|
| 1 | `nika-core/ast/structured.rs` | `json_to_schema()` function | Small |
| 2 | `nika-core/ast/structured.rs` | `from_example` field + deser | Medium |
| 3 | `nika-engine/runtime/structured_output.rs` | `load_schema()` branching | Small |
| 4 | `nika-engine/runtime/executor/mod.rs` | Prompt injection for example | Small |
| 5 | `nika-core/ast/output.rs` | Optional: skip for V1 | Skip |
| 6 | `examples/gates/feature/` | Gate test | Small |
| 7 | `schemas/nika-workflow.schema.json` | Schema validation update | Small |

**Total: ~150 lines of Rust + 30 lines YAML test**
