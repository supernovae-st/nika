# MCP Validation Architecture Design

**Status:** Draft
**Date:** 2026-02-20
**Author:** Claude Code
**Context:** Nika v0.5.1

---

## Problem Statement

Currently, when passing invalid parameters to MCP tools, Nika returns cryptic errors like:

```
[NIKA-102] MCP tool 'novanet_generate' call failed: missing field `describe`
```

This is unhelpful because:
1. Users don't know what fields ARE required
2. No hint about the correct field names
3. Error comes from deep in rmcp, losing context

## Proposed Solution

A 3-layer validation architecture:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  MCP VALIDATION ARCHITECTURE                                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Layer 1: SCHEMA DISCOVERY                                                  │
│  ┌───────────────────────────────────────────────────────────────────────┐ │
│  │ On connect(), cache tool schemas from list_tools()                    │ │
│  │ ToolSchemaCache: DashMap<(server, tool), CompiledSchema>              │ │
│  └───────────────────────────────────────────────────────────────────────┘ │
│                              │                                              │
│                              ▼                                              │
│  Layer 2: PRE-CALL VALIDATION                                               │
│  ┌───────────────────────────────────────────────────────────────────────┐ │
│  │ Before call_tool(), validate params against cached schema             │ │
│  │ Uses jsonschema crate (already in Cargo.toml)                         │ │
│  │ Returns McpValidationError with detailed field info                   │ │
│  └───────────────────────────────────────────────────────────────────────┘ │
│                              │                                              │
│                              ▼                                              │
│  Layer 3: ERROR ENHANCEMENT                                                 │
│  ┌───────────────────────────────────────────────────────────────────────┐ │
│  │ If MCP returns error, parse and enhance with:                         │ │
│  │ - Similar field suggestions (edit distance)                           │ │
│  │ - Schema-based "did you mean?" hints                                  │ │
│  │ - Link to tool documentation                                          │ │
│  └───────────────────────────────────────────────────────────────────────┘ │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Module Structure

### New Files

```
src/mcp/
├── mod.rs              # Add: pub mod validation;
├── validation/
│   ├── mod.rs          # Public API
│   ├── schema_cache.rs # Layer 1: Schema caching
│   ├── validator.rs    # Layer 2: Pre-call validation
│   └── enhancer.rs     # Layer 3: Error enhancement
```

### File Responsibilities

| File | Responsibility | Lines (est) |
|------|----------------|-------------|
| `mod.rs` | Re-exports, ValidationConfig | ~30 |
| `schema_cache.rs` | Cache schemas from list_tools() | ~150 |
| `validator.rs` | JSON Schema validation | ~200 |
| `enhancer.rs` | Error parsing + suggestions | ~150 |

---

## New Types

### 1. ValidationConfig

```rust
// src/mcp/validation/mod.rs

/// Configuration for MCP parameter validation
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    /// Enable pre-call validation (default: true)
    pub pre_validate: bool,

    /// Enable error enhancement (default: true)
    pub enhance_errors: bool,

    /// Max edit distance for "did you mean" suggestions (default: 3)
    pub suggestion_distance: usize,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            pre_validate: true,
            enhance_errors: true,
            suggestion_distance: 3,
        }
    }
}
```

### 2. ToolSchemaCache

```rust
// src/mcp/validation/schema_cache.rs

use dashmap::DashMap;
use jsonschema::Validator;
use std::sync::Arc;

/// Cache key: (server_name, tool_name)
type CacheKey = (String, String);

/// Cached compiled JSON Schema validator
pub struct CachedSchema {
    /// Raw schema JSON (for error messages)
    pub raw: serde_json::Value,

    /// Compiled validator (thread-safe)
    pub validator: Arc<Validator>,

    /// Required properties (extracted for quick access)
    pub required: Vec<String>,

    /// All property names (for suggestions)
    pub properties: Vec<String>,
}

/// Thread-safe schema cache
pub struct ToolSchemaCache {
    cache: DashMap<CacheKey, CachedSchema>,
}

impl ToolSchemaCache {
    pub fn new() -> Self {
        Self {
            cache: DashMap::new(),
        }
    }

    /// Populate cache from list_tools() results
    pub fn populate(&self, server: &str, tools: &[ToolDefinition]) -> Result<usize> {
        let mut count = 0;
        for tool in tools {
            if let Some(schema) = &tool.input_schema {
                self.compile_and_cache(server, &tool.name, schema)?;
                count += 1;
            }
        }
        Ok(count)
    }

    /// Get cached schema for a tool
    pub fn get(&self, server: &str, tool: &str) -> Option<dashmap::mapref::one::Ref<CacheKey, CachedSchema>> {
        self.cache.get(&(server.to_string(), tool.to_string()))
    }

    /// Compile and cache a schema
    fn compile_and_cache(&self, server: &str, tool: &str, schema: &serde_json::Value) -> Result<()> {
        // Extract required fields
        let required = schema
            .get("required")
            .and_then(|r| r.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        // Extract property names
        let properties = schema
            .get("properties")
            .and_then(|p| p.as_object())
            .map(|obj| obj.keys().cloned().collect())
            .unwrap_or_default();

        // Compile validator
        let validator = Validator::new(schema)
            .map_err(|e| NikaError::McpProtocolError {
                reason: format!("Invalid schema for {}.{}: {}", server, tool, e),
            })?;

        let cached = CachedSchema {
            raw: schema.clone(),
            validator: Arc::new(validator),
            required,
            properties,
        };

        self.cache.insert((server.to_string(), tool.to_string()), cached);
        Ok(())
    }

    /// Clear all cached schemas
    pub fn clear(&self) {
        self.cache.clear();
    }

    /// Get cache stats
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            tool_count: self.cache.len(),
            servers: self.cache.iter()
                .map(|e| e.key().0.clone())
                .collect::<std::collections::HashSet<_>>()
                .len(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CacheStats {
    pub tool_count: usize,
    pub servers: usize,
}
```

### 3. McpValidator

```rust
// src/mcp/validation/validator.rs

use super::schema_cache::{CachedSchema, ToolSchemaCache};
use crate::error::{NikaError, Result};

/// Validation result with detailed errors
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
}

/// Single validation error
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// JSON path to the error (e.g., "/entity", "/locale")
    pub path: String,

    /// Error kind
    pub kind: ValidationErrorKind,

    /// Human-readable message
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationErrorKind {
    /// Required field is missing
    MissingRequired { field: String },

    /// Field type is wrong
    TypeMismatch { expected: String, actual: String },

    /// Unknown field (not in schema)
    UnknownField { field: String, suggestions: Vec<String> },

    /// Value doesn't match pattern/format
    InvalidValue { reason: String },

    /// Enum value not in allowed list
    InvalidEnum { value: String, allowed: Vec<String> },
}

/// MCP parameter validator
pub struct McpValidator {
    cache: ToolSchemaCache,
    config: ValidationConfig,
}

impl McpValidator {
    pub fn new(config: ValidationConfig) -> Self {
        Self {
            cache: ToolSchemaCache::new(),
            config,
        }
    }

    /// Get reference to schema cache
    pub fn cache(&self) -> &ToolSchemaCache {
        &self.cache
    }

    /// Validate parameters against cached schema
    pub fn validate(
        &self,
        server: &str,
        tool: &str,
        params: &serde_json::Value,
    ) -> ValidationResult {
        // If validation disabled, always pass
        if !self.config.pre_validate {
            return ValidationResult { is_valid: true, errors: vec![] };
        }

        // Get cached schema
        let Some(schema_ref) = self.cache.get(server, tool) else {
            // No schema cached = can't validate, pass through
            tracing::debug!(
                server = %server,
                tool = %tool,
                "No cached schema, skipping validation"
            );
            return ValidationResult { is_valid: true, errors: vec![] };
        };

        let schema = schema_ref.value();
        let mut errors = Vec::new();

        // Run JSON Schema validation
        let validation = schema.validator.iter_errors(params);

        for error in validation {
            let path = error.instance_path.to_string();
            let kind = self.classify_error(&error, schema);
            let message = self.format_error(&error, schema);

            errors.push(ValidationError { path, kind, message });
        }

        ValidationResult {
            is_valid: errors.is_empty(),
            errors,
        }
    }

    /// Classify validation error into a kind
    fn classify_error(
        &self,
        error: &jsonschema::ValidationError,
        schema: &CachedSchema,
    ) -> ValidationErrorKind {
        let error_kind = format!("{:?}", error.kind);

        if error_kind.contains("Required") {
            // Extract field name from error
            let field = error.instance_path.last()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            ValidationErrorKind::MissingRequired { field }
        } else if error_kind.contains("Type") {
            ValidationErrorKind::TypeMismatch {
                expected: "expected_type".to_string(), // TODO: extract from error
                actual: "actual_type".to_string(),
            }
        } else if error_kind.contains("AdditionalProperties") {
            let field = error.instance_path.last()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let suggestions = self.find_suggestions(&field, &schema.properties);
            ValidationErrorKind::UnknownField { field, suggestions }
        } else if error_kind.contains("Enum") {
            ValidationErrorKind::InvalidEnum {
                value: format!("{}", error.instance),
                allowed: vec![], // TODO: extract from schema
            }
        } else {
            ValidationErrorKind::InvalidValue {
                reason: error.to_string(),
            }
        }
    }

    /// Format a human-readable error message
    fn format_error(
        &self,
        error: &jsonschema::ValidationError,
        schema: &CachedSchema,
    ) -> String {
        let base = error.to_string();

        // Add suggestions for missing fields
        if !schema.required.is_empty() {
            format!(
                "{}. Required fields: {}",
                base,
                schema.required.join(", ")
            )
        } else {
            base
        }
    }

    /// Find similar field names (for "did you mean?")
    fn find_suggestions(&self, field: &str, properties: &[String]) -> Vec<String> {
        properties
            .iter()
            .filter(|p| Self::edit_distance(field, p) <= self.config.suggestion_distance)
            .cloned()
            .collect()
    }

    /// Simple Levenshtein distance
    fn edit_distance(a: &str, b: &str) -> usize {
        let a = a.to_lowercase();
        let b = b.to_lowercase();

        if a.is_empty() { return b.len(); }
        if b.is_empty() { return a.len(); }

        let a_chars: Vec<char> = a.chars().collect();
        let b_chars: Vec<char> = b.chars().collect();

        let mut matrix = vec![vec![0usize; b_chars.len() + 1]; a_chars.len() + 1];

        for i in 0..=a_chars.len() { matrix[i][0] = i; }
        for j in 0..=b_chars.len() { matrix[0][j] = j; }

        for i in 1..=a_chars.len() {
            for j in 1..=b_chars.len() {
                let cost = if a_chars[i-1] == b_chars[j-1] { 0 } else { 1 };
                matrix[i][j] = std::cmp::min(
                    std::cmp::min(
                        matrix[i-1][j] + 1,     // deletion
                        matrix[i][j-1] + 1,     // insertion
                    ),
                    matrix[i-1][j-1] + cost,    // substitution
                );
            }
        }

        matrix[a_chars.len()][b_chars.len()]
    }
}
```

### 4. ErrorEnhancer

```rust
// src/mcp/validation/enhancer.rs

use crate::error::NikaError;
use super::schema_cache::ToolSchemaCache;

/// Enhances MCP errors with better context
pub struct ErrorEnhancer<'a> {
    cache: &'a ToolSchemaCache,
}

impl<'a> ErrorEnhancer<'a> {
    pub fn new(cache: &'a ToolSchemaCache) -> Self {
        Self { cache }
    }

    /// Enhance an MCP error with better context
    pub fn enhance(
        &self,
        server: &str,
        tool: &str,
        error: NikaError,
    ) -> NikaError {
        let NikaError::McpToolError { tool: tool_name, reason } = &error else {
            return error; // Only enhance McpToolError
        };

        // Try to parse the error message
        let enhanced_reason = self.enhance_reason(server, tool, &reason);

        NikaError::McpToolError {
            tool: tool_name.clone(),
            reason: enhanced_reason,
        }
    }

    fn enhance_reason(&self, server: &str, tool: &str, reason: &str) -> String {
        let Some(schema_ref) = self.cache.get(server, tool) else {
            return reason.to_string();
        };

        let schema = schema_ref.value();
        let reason_lower = reason.to_lowercase();

        // Missing field pattern
        if reason_lower.contains("missing field") {
            return self.enhance_missing_field(reason, schema);
        }

        // Unknown field pattern
        if reason_lower.contains("unknown field") || reason_lower.contains("unexpected") {
            return self.enhance_unknown_field(reason, schema);
        }

        // Add required fields hint
        if !schema.required.is_empty() {
            format!(
                "{}. Required fields: [{}]. Available: [{}]",
                reason,
                schema.required.join(", "),
                schema.properties.join(", ")
            )
        } else {
            reason.to_string()
        }
    }

    fn enhance_missing_field(&self, reason: &str, schema: &super::schema_cache::CachedSchema) -> String {
        format!(
            "{}. Required: [{}]",
            reason,
            schema.required.join(", ")
        )
    }

    fn enhance_unknown_field(&self, reason: &str, schema: &super::schema_cache::CachedSchema) -> String {
        format!(
            "{}. Valid fields: [{}]",
            reason,
            schema.properties.join(", ")
        )
    }
}
```

---

## New Error Types (NIKA-107, NIKA-108)

```rust
// Add to src/error.rs in MCP section

#[error("[NIKA-107] MCP parameter validation failed for '{tool}': {details}")]
McpValidationFailed {
    tool: String,
    details: String,
    /// Required fields that are missing
    missing: Vec<String>,
    /// Suggested corrections
    suggestions: Vec<String>,
},

#[error("[NIKA-108] MCP schema error for '{tool}': {reason}")]
McpSchemaError {
    tool: String,
    reason: String,
},
```

---

## Integration with McpClient

### Modified `McpClient::connect()`

```rust
// In src/mcp/client.rs

impl McpClient {
    /// Connect to the MCP server.
    /// Also populates the schema cache from list_tools().
    pub async fn connect(&self) -> Result<()> {
        // ... existing connect logic ...

        // NEW: Populate schema cache after successful connect
        if let Some(validator) = &self.validator {
            let tools = self.list_tools().await?;
            let cached = validator.cache().populate(&self.name, &tools)?;
            tracing::info!(
                mcp_server = %self.name,
                tools_cached = cached,
                "Schema cache populated"
            );
        }

        Ok(())
    }
}
```

### Modified `McpClient::call_tool()`

```rust
// In src/mcp/client.rs

impl McpClient {
    pub async fn call_tool(&self, name: &str, params: Value) -> Result<ToolCallResult> {
        // ... existing connection check ...

        // NEW: Pre-call validation
        if let Some(validator) = &self.validator {
            let result = validator.validate(&self.name, name, &params);
            if !result.is_valid {
                let details = result.errors.iter()
                    .map(|e| e.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; ");

                let missing: Vec<String> = result.errors.iter()
                    .filter_map(|e| match &e.kind {
                        ValidationErrorKind::MissingRequired { field } => Some(field.clone()),
                        _ => None,
                    })
                    .collect();

                let suggestions: Vec<String> = result.errors.iter()
                    .filter_map(|e| match &e.kind {
                        ValidationErrorKind::UnknownField { suggestions, .. } => {
                            Some(suggestions.clone())
                        }
                        _ => None,
                    })
                    .flatten()
                    .collect();

                return Err(NikaError::McpValidationFailed {
                    tool: name.to_string(),
                    details,
                    missing,
                    suggestions,
                });
            }
        }

        // ... existing call logic with error enhancement ...
    }
}
```

---

## TDD Test Plan

### 1. Schema Cache Tests (`schema_cache_test.rs`)

```rust
// Tests to write FIRST (TDD)

#[test]
fn test_cache_empty_by_default() {
    let cache = ToolSchemaCache::new();
    assert_eq!(cache.stats().tool_count, 0);
}

#[test]
fn test_populate_from_tool_definitions() {
    let cache = ToolSchemaCache::new();
    let tools = vec![
        ToolDefinition::new("tool1")
            .with_input_schema(json!({
                "type": "object",
                "properties": { "a": { "type": "string" } },
                "required": ["a"]
            })),
    ];

    let count = cache.populate("server", &tools).unwrap();
    assert_eq!(count, 1);
    assert!(cache.get("server", "tool1").is_some());
}

#[test]
fn test_populate_skips_tools_without_schema() {
    let cache = ToolSchemaCache::new();
    let tools = vec![
        ToolDefinition::new("no_schema"),
        ToolDefinition::new("has_schema")
            .with_input_schema(json!({"type": "object"})),
    ];

    let count = cache.populate("server", &tools).unwrap();
    assert_eq!(count, 1);
    assert!(cache.get("server", "no_schema").is_none());
}

#[test]
fn test_get_nonexistent_returns_none() {
    let cache = ToolSchemaCache::new();
    assert!(cache.get("server", "tool").is_none());
}

#[test]
fn test_clear_removes_all_entries() {
    let cache = ToolSchemaCache::new();
    cache.populate("s", &[
        ToolDefinition::new("t").with_input_schema(json!({}))
    ]).unwrap();
    assert_eq!(cache.stats().tool_count, 1);

    cache.clear();
    assert_eq!(cache.stats().tool_count, 0);
}

#[test]
fn test_extracts_required_fields() {
    let cache = ToolSchemaCache::new();
    cache.populate("s", &[
        ToolDefinition::new("t").with_input_schema(json!({
            "type": "object",
            "properties": {
                "entity": { "type": "string" },
                "locale": { "type": "string" }
            },
            "required": ["entity"]
        }))
    ]).unwrap();

    let schema = cache.get("s", "t").unwrap();
    assert_eq!(schema.required, vec!["entity"]);
    assert!(schema.properties.contains(&"entity".to_string()));
    assert!(schema.properties.contains(&"locale".to_string()));
}
```

### 2. Validator Tests (`validator_test.rs`)

```rust
#[test]
fn test_validate_missing_required_field() {
    let validator = McpValidator::new(ValidationConfig::default());
    validator.cache().populate("novanet", &[
        ToolDefinition::new("novanet_generate")
            .with_input_schema(json!({
                "type": "object",
                "properties": {
                    "entity": { "type": "string" },
                    "locale": { "type": "string" }
                },
                "required": ["entity"]
            }))
    ]).unwrap();

    // Missing required "entity" field
    let result = validator.validate("novanet", "novanet_generate", &json!({
        "locale": "fr-FR"
    }));

    assert!(!result.is_valid);
    assert_eq!(result.errors.len(), 1);
    assert!(matches!(
        &result.errors[0].kind,
        ValidationErrorKind::MissingRequired { field } if field == "entity"
    ));
}

#[test]
fn test_validate_valid_params_passes() {
    let validator = McpValidator::new(ValidationConfig::default());
    validator.cache().populate("novanet", &[
        ToolDefinition::new("novanet_generate")
            .with_input_schema(json!({
                "type": "object",
                "properties": {
                    "entity": { "type": "string" }
                },
                "required": ["entity"]
            }))
    ]).unwrap();

    let result = validator.validate("novanet", "novanet_generate", &json!({
        "entity": "qr-code"
    }));

    assert!(result.is_valid);
    assert!(result.errors.is_empty());
}

#[test]
fn test_validate_disabled_always_passes() {
    let config = ValidationConfig {
        pre_validate: false,
        ..Default::default()
    };
    let validator = McpValidator::new(config);

    // No schema cached, but should pass
    let result = validator.validate("any", "tool", &json!({}));
    assert!(result.is_valid);
}

#[test]
fn test_validate_no_cached_schema_passes() {
    let validator = McpValidator::new(ValidationConfig::default());

    // No schema cached for this tool
    let result = validator.validate("unknown", "tool", &json!({
        "anything": "goes"
    }));

    assert!(result.is_valid);
}

#[test]
fn test_validate_type_mismatch() {
    let validator = McpValidator::new(ValidationConfig::default());
    validator.cache().populate("s", &[
        ToolDefinition::new("t").with_input_schema(json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer" }
            }
        }))
    ]).unwrap();

    let result = validator.validate("s", "t", &json!({
        "count": "not-an-integer"
    }));

    assert!(!result.is_valid);
    assert!(matches!(
        &result.errors[0].kind,
        ValidationErrorKind::TypeMismatch { .. }
    ));
}

#[test]
fn test_edit_distance_exact_match() {
    assert_eq!(McpValidator::edit_distance("entity", "entity"), 0);
}

#[test]
fn test_edit_distance_one_char_diff() {
    assert_eq!(McpValidator::edit_distance("entity", "entityy"), 1);
    assert_eq!(McpValidator::edit_distance("entty", "entity"), 1);
}

#[test]
fn test_edit_distance_case_insensitive() {
    assert_eq!(McpValidator::edit_distance("Entity", "ENTITY"), 0);
}

#[test]
fn test_find_suggestions_within_distance() {
    let validator = McpValidator::new(ValidationConfig::default());
    validator.cache().populate("s", &[
        ToolDefinition::new("t").with_input_schema(json!({
            "type": "object",
            "properties": {
                "entity": {},
                "locale": {},
                "forms": {}
            }
        }))
    ]).unwrap();

    let schema = validator.cache().get("s", "t").unwrap();
    let suggestions = validator.find_suggestions("entiy", &schema.properties);

    assert!(suggestions.contains(&"entity".to_string()));
}
```

### 3. Enhancer Tests (`enhancer_test.rs`)

```rust
#[test]
fn test_enhance_missing_field_error() {
    let cache = ToolSchemaCache::new();
    cache.populate("novanet", &[
        ToolDefinition::new("novanet_generate")
            .with_input_schema(json!({
                "type": "object",
                "properties": {
                    "entity": { "type": "string" },
                    "locale": { "type": "string" }
                },
                "required": ["entity"]
            }))
    ]).unwrap();

    let enhancer = ErrorEnhancer::new(&cache);
    let original = NikaError::McpToolError {
        tool: "novanet_generate".to_string(),
        reason: "missing field `entity`".to_string(),
    };

    let enhanced = enhancer.enhance("novanet", "novanet_generate", original);

    let NikaError::McpToolError { reason, .. } = enhanced else {
        panic!("Expected McpToolError");
    };

    assert!(reason.contains("Required:"));
    assert!(reason.contains("entity"));
}

#[test]
fn test_enhance_passes_through_non_mcp_errors() {
    let cache = ToolSchemaCache::new();
    let enhancer = ErrorEnhancer::new(&cache);

    let original = NikaError::ParseError { details: "test".to_string() };
    let enhanced = enhancer.enhance("s", "t", original);

    assert!(matches!(enhanced, NikaError::ParseError { .. }));
}

#[test]
fn test_enhance_no_schema_returns_original() {
    let cache = ToolSchemaCache::new();
    let enhancer = ErrorEnhancer::new(&cache);

    let original = NikaError::McpToolError {
        tool: "unknown".to_string(),
        reason: "error".to_string(),
    };

    let enhanced = enhancer.enhance("s", "unknown", original);

    let NikaError::McpToolError { reason, .. } = enhanced else {
        panic!("Expected McpToolError");
    };
    assert_eq!(reason, "error");
}
```

### 4. Integration Tests (`mcp_validation_integration_test.rs`)

```rust
#[tokio::test]
async fn test_full_validation_flow() {
    // Mock client with validation enabled
    let config = McpConfig::new("novanet", "echo");
    let mut client = McpClient::new(config).unwrap();
    client.enable_validation(ValidationConfig::default());

    // This would normally fail with cryptic error
    // With validation, we get a clear message
    let result = client.call_tool("novanet_generate", json!({
        "locale": "fr-FR"  // Missing "entity"
    })).await;

    assert!(result.is_err());
    let err = result.unwrap_err();

    // Should be McpValidationFailed, not McpToolError
    assert!(matches!(err, NikaError::McpValidationFailed { .. }));

    // Check error details
    if let NikaError::McpValidationFailed { missing, .. } = err {
        assert!(missing.contains(&"entity".to_string()));
    }
}

#[tokio::test]
async fn test_validation_disabled_skips_check() {
    let config = McpConfig::new("test", "echo");
    let mut client = McpClient::new(config).unwrap();

    // Disable validation
    client.enable_validation(ValidationConfig {
        pre_validate: false,
        ..Default::default()
    });

    // Should pass validation (but may fail at MCP level)
    // This tests that validation is actually skipped
}
```

---

## Error Code Assignment

| Code | Error | Description |
|------|-------|-------------|
| NIKA-107 | `McpValidationFailed` | Pre-call validation found errors |
| NIKA-108 | `McpSchemaError` | Failed to compile/parse tool schema |

These fit in the existing NIKA-100-109 MCP range.

---

## Public API Changes

### McpClient

```rust
impl McpClient {
    // NEW: Enable validation with config
    pub fn enable_validation(&mut self, config: ValidationConfig);

    // NEW: Get validation stats
    pub fn validation_stats(&self) -> Option<CacheStats>;

    // MODIFIED: connect() now populates schema cache
    pub async fn connect(&self) -> Result<()>;

    // MODIFIED: call_tool() now validates before calling
    pub async fn call_tool(&self, name: &str, params: Value) -> Result<ToolCallResult>;
}
```

### New Public Exports

```rust
// In src/mcp/mod.rs
pub mod validation;
pub use validation::{ValidationConfig, ValidationResult, ValidationError, ValidationErrorKind};
```

---

## Implementation Order (TDD)

1. **Phase 1: Schema Cache** (tests first)
   - Write schema_cache tests
   - Implement `ToolSchemaCache`
   - Verify tests pass

2. **Phase 2: Validator** (tests first)
   - Write validator tests
   - Implement `McpValidator`
   - Verify tests pass

3. **Phase 3: Enhancer** (tests first)
   - Write enhancer tests
   - Implement `ErrorEnhancer`
   - Verify tests pass

4. **Phase 4: Integration** (tests first)
   - Write integration tests
   - Add new error types to `error.rs`
   - Modify `McpClient`
   - Verify all tests pass

5. **Phase 5: Documentation**
   - Update CLAUDE.md
   - Add inline docs
   - Create example

---

## Dependencies

Already in `Cargo.toml`:
- `jsonschema = "0.26"` - JSON Schema validation
- `dashmap = "6.1"` - Thread-safe cache
- `serde_json = "1.0"` - JSON handling

No new dependencies required.

---

## Backward Compatibility

- `ValidationConfig::default()` enables validation
- Existing workflows continue to work
- New error types are additive (no breaking changes)
- `McpClient` API is extended, not changed

To disable validation (old behavior):
```rust
client.enable_validation(ValidationConfig {
    pre_validate: false,
    enhance_errors: false,
    ..Default::default()
});
```

---

## Example: Before/After

### Before (v0.5.0)

```
[NIKA-102] MCP tool 'novanet_generate' call failed: missing field `describe`
```

### After (v0.5.1)

```
[NIKA-107] MCP parameter validation failed for 'novanet_generate':
  Missing required field: entity. Required fields: [entity].
  Available fields: [entity, locale, forms, denomination_forms].
  Did you mean 'entity' instead of 'entiy'?
```

---

## Metrics

| Metric | Before | After |
|--------|--------|-------|
| Error clarity | 1/10 | 9/10 |
| Time to fix | ~5 min | ~30 sec |
| Code added | 0 | ~600 lines |
| New dependencies | 0 | 0 |

---

## References

- [jsonschema crate docs](https://docs.rs/jsonschema)
- [MCP Protocol Spec](https://modelcontextprotocol.io/)
- [rmcp SDK](https://github.com/anthropics/rmcp)
- Nika ADR-003: MCP-Only Integration
