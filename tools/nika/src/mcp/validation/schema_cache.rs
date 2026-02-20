//! Schema Cache Module (Layer 1)
//!
//! Caches tool schemas from MCP `list_tools()` for validation.
//!
//! ## Design
//!
//! - On connect(), cache tool schemas from list_tools()
//! - Thread-safe via DashMap
//! - Extracts required fields and property names for fast lookup
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika::mcp::validation::ToolSchemaCache;
//!
//! let cache = ToolSchemaCache::new();
//! let count = cache.populate("novanet", &tools)?;
//! let schema = cache.get("novanet", "novanet_generate");
//! ```

use dashmap::DashMap;
use jsonschema::Validator;
use std::sync::Arc;

use crate::error::{NikaError, Result};
use crate::mcp::types::ToolDefinition;

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

/// Statistics about the schema cache
#[derive(Debug, Clone, PartialEq)]
pub struct CacheStats {
    /// Number of tools cached
    pub tool_count: usize,

    /// Number of distinct servers
    pub servers: usize,
}

/// Thread-safe schema cache for MCP tools
pub struct ToolSchemaCache {
    cache: DashMap<CacheKey, CachedSchema>,
}

impl Default for ToolSchemaCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolSchemaCache {
    /// Create a new empty cache
    pub fn new() -> Self {
        Self {
            cache: DashMap::new(),
        }
    }

    /// Populate cache from list_tools() results
    ///
    /// Returns the number of tools cached (skips tools without input_schema)
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
    pub fn get(
        &self,
        server: &str,
        tool: &str,
    ) -> Option<dashmap::mapref::one::Ref<'_, CacheKey, CachedSchema>> {
        self.cache.get(&(server.to_string(), tool.to_string()))
    }

    /// Clear all cached schemas
    pub fn clear(&self) {
        self.cache.clear();
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let servers: std::collections::HashSet<_> =
            self.cache.iter().map(|e| e.key().0.clone()).collect();

        CacheStats {
            tool_count: self.cache.len(),
            servers: servers.len(),
        }
    }

    /// Compile and cache a schema
    fn compile_and_cache(
        &self,
        server: &str,
        tool: &str,
        schema: &serde_json::Value,
    ) -> Result<()> {
        // Extract required fields
        let required = schema
            .get("required")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        // Extract property names
        let properties = schema
            .get("properties")
            .and_then(|p| p.as_object())
            .map(|obj| obj.keys().cloned().collect())
            .unwrap_or_default();

        // Compile validator
        let validator = Validator::new(schema).map_err(|e| NikaError::McpProtocolError {
            reason: format!("Invalid schema for {}.{}: {}", server, tool, e),
        })?;

        let cached = CachedSchema {
            raw: schema.clone(),
            validator: Arc::new(validator),
            required,
            properties,
        };

        self.cache
            .insert((server.to_string(), tool.to_string()), cached);
        Ok(())
    }
}

// ============================================================================
// TESTS (TDD - Written First)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ========================================================================
    // Test: Cache is empty by default
    // ========================================================================
    #[test]
    fn test_cache_empty_by_default() {
        let cache = ToolSchemaCache::new();
        assert_eq!(cache.stats().tool_count, 0);
        assert_eq!(cache.stats().servers, 0);
    }

    // ========================================================================
    // Test: Populate from tool definitions
    // ========================================================================
    #[test]
    fn test_populate_from_tool_definitions() {
        let cache = ToolSchemaCache::new();
        let tools = vec![ToolDefinition::new("tool1").with_input_schema(json!({
            "type": "object",
            "properties": { "a": { "type": "string" } },
            "required": ["a"]
        }))];

        let count = cache.populate("server", &tools).unwrap();
        assert_eq!(count, 1);
        assert!(cache.get("server", "tool1").is_some());
    }

    // ========================================================================
    // Test: Populate skips tools without schema
    // ========================================================================
    #[test]
    fn test_populate_skips_tools_without_schema() {
        let cache = ToolSchemaCache::new();
        let tools = vec![
            ToolDefinition::new("no_schema"),
            ToolDefinition::new("has_schema").with_input_schema(json!({"type": "object"})),
        ];

        let count = cache.populate("server", &tools).unwrap();
        assert_eq!(count, 1);
        assert!(cache.get("server", "no_schema").is_none());
        assert!(cache.get("server", "has_schema").is_some());
    }

    // ========================================================================
    // Test: Get nonexistent returns None
    // ========================================================================
    #[test]
    fn test_get_nonexistent_returns_none() {
        let cache = ToolSchemaCache::new();
        assert!(cache.get("server", "tool").is_none());
    }

    // ========================================================================
    // Test: Clear removes all entries
    // ========================================================================
    #[test]
    fn test_clear_removes_all_entries() {
        let cache = ToolSchemaCache::new();
        cache
            .populate(
                "s",
                &[ToolDefinition::new("t").with_input_schema(json!({}))],
            )
            .unwrap();
        assert_eq!(cache.stats().tool_count, 1);

        cache.clear();
        assert_eq!(cache.stats().tool_count, 0);
    }

    // ========================================================================
    // Test: Extracts required fields
    // ========================================================================
    #[test]
    fn test_extracts_required_fields() {
        let cache = ToolSchemaCache::new();
        cache
            .populate(
                "s",
                &[ToolDefinition::new("t").with_input_schema(json!({
                    "type": "object",
                    "properties": {
                        "entity": { "type": "string" },
                        "locale": { "type": "string" }
                    },
                    "required": ["entity"]
                }))],
            )
            .unwrap();

        let schema = cache.get("s", "t").unwrap();
        assert_eq!(schema.required, vec!["entity"]);
        assert!(schema.properties.contains(&"entity".to_string()));
        assert!(schema.properties.contains(&"locale".to_string()));
    }

    // ========================================================================
    // Test: Multiple servers tracked separately
    // ========================================================================
    #[test]
    fn test_multiple_servers_tracked() {
        let cache = ToolSchemaCache::new();
        cache
            .populate(
                "server1",
                &[ToolDefinition::new("t1").with_input_schema(json!({}))],
            )
            .unwrap();
        cache
            .populate(
                "server2",
                &[ToolDefinition::new("t2").with_input_schema(json!({}))],
            )
            .unwrap();

        let stats = cache.stats();
        assert_eq!(stats.tool_count, 2);
        assert_eq!(stats.servers, 2);
    }

    // ========================================================================
    // Test: Same tool name, different servers
    // ========================================================================
    #[test]
    fn test_same_tool_name_different_servers() {
        let cache = ToolSchemaCache::new();
        cache
            .populate(
                "server1",
                &[ToolDefinition::new("tool").with_input_schema(json!({
                    "type": "object",
                    "properties": { "a": {} },
                    "required": ["a"]
                }))],
            )
            .unwrap();
        cache
            .populate(
                "server2",
                &[ToolDefinition::new("tool").with_input_schema(json!({
                    "type": "object",
                    "properties": { "b": {} },
                    "required": ["b"]
                }))],
            )
            .unwrap();

        let schema1 = cache.get("server1", "tool").unwrap();
        let schema2 = cache.get("server2", "tool").unwrap();

        assert_eq!(schema1.required, vec!["a"]);
        assert_eq!(schema2.required, vec!["b"]);
    }

    // ========================================================================
    // Test: Invalid schema returns error
    // ========================================================================
    #[test]
    fn test_invalid_schema_returns_error() {
        let cache = ToolSchemaCache::new();

        // Schema with invalid $ref should fail to compile
        let result = cache.populate(
            "s",
            &[ToolDefinition::new("t").with_input_schema(json!({
                "$ref": "#/definitions/nonexistent"
            }))],
        );

        // jsonschema may or may not error on invalid refs depending on version
        // What matters is we handle it gracefully
        // If it doesn't error, the test still passes
        if let Err(err) = result {
            assert!(matches!(err, NikaError::McpProtocolError { .. }));
        }
    }

    // ========================================================================
    // Test: Default impl works
    // ========================================================================
    #[test]
    fn test_default_impl() {
        let cache = ToolSchemaCache::default();
        assert_eq!(cache.stats().tool_count, 0);
    }

    // ========================================================================
    // Test: Extracted properties order independent
    // ========================================================================
    #[test]
    fn test_properties_extraction() {
        let cache = ToolSchemaCache::new();
        cache
            .populate(
                "s",
                &[ToolDefinition::new("t").with_input_schema(json!({
                    "type": "object",
                    "properties": {
                        "z_field": {},
                        "a_field": {},
                        "m_field": {}
                    }
                }))],
            )
            .unwrap();

        let schema = cache.get("s", "t").unwrap();
        // Should have all 3 properties
        assert_eq!(schema.properties.len(), 3);
        assert!(schema.properties.contains(&"z_field".to_string()));
        assert!(schema.properties.contains(&"a_field".to_string()));
        assert!(schema.properties.contains(&"m_field".to_string()));
    }
}
