// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Decompose expansion strategies
//!
//! Expands decompose specs into iteration items via:
//! - Semantic: MCP walk (novanet_search walk mode)
//! - Static: Binding resolution (no MCP call)
//! - Nested: Recursive BFS traversal via MCP

use tracing::{debug, instrument};

use crate::ast::decompose::{DecomposeSpec, DecomposeStrategy};
use crate::binding::ResolvedBindings;
use crate::error::NikaError;
use crate::error_domains::BindingError;
use crate::store::RunContext;

use super::TaskExecutor;

impl TaskExecutor {
    /// Expand a decompose spec into iteration items
    ///
    /// Returns an array of JSON values that can be used as for_each items.
    /// Supports semantic (MCP traverse), static (binding resolution), and nested strategies.
    #[instrument(name = "expand_decompose", skip(self, bindings, datastore), fields(
        strategy = ?spec.strategy,
        traverse = %spec.traverse,
        source = %spec.source
    ))]
    pub async fn expand_decompose(
        &self,
        spec: &DecomposeSpec,
        bindings: &ResolvedBindings,
        datastore: &RunContext,
    ) -> Result<Vec<serde_json::Value>, NikaError> {
        match spec.strategy {
            DecomposeStrategy::Semantic => {
                self.expand_decompose_semantic(spec, bindings, datastore)
                    .await
            }
            DecomposeStrategy::Static => self.expand_decompose_static(spec, bindings, datastore),
            DecomposeStrategy::Nested => {
                self.expand_decompose_nested(spec, bindings, datastore)
                    .await
            }
        }
    }

    /// Expand using semantic traversal via MCP (calls novanet_search walk mode)
    async fn expand_decompose_semantic(
        &self,
        spec: &DecomposeSpec,
        bindings: &ResolvedBindings,
        datastore: &RunContext,
    ) -> Result<Vec<serde_json::Value>, NikaError> {
        use serde_json::{json, Value};

        // Get MCP client
        let server_name = spec.mcp_server();
        let client = self.get_mcp_client(server_name).await?;

        // Resolve source binding
        let source_value = self.resolve_decompose_source(&spec.source, bindings, datastore)?;
        let source_key = self.extract_decompose_key(&source_value)?;

        debug!(
            source_key = %source_key,
            arc = %spec.traverse,
            "Calling novanet_search (walk) for decompose"
        );

        // Call novanet_search with walk mode (replaces novanet_traverse)
        let params = json!({
            "mode": "walk",
            "start_key": source_key,
            "arc_kinds": [spec.traverse],
            "direction": "outgoing"
        });

        let result = client.call_tool("novanet_search", params).await?;

        // Parse JSON from result content
        let result_json: Value =
            serde_json::from_str(&result.text()).map_err(|e| NikaError::McpInvalidResponse {
                tool: "novanet_search".to_string(),
                reason: format!("failed to parse JSON response: {}", e),
            })?;

        // Extract nodes from result (pass ownership to avoid clone)
        let mut items = self.extract_decompose_nodes(result_json)?;

        // Apply max_items limit
        if let Some(max) = spec.max_items {
            items.truncate(max);
        }

        debug!(
            count = items.len(),
            max_items = ?spec.max_items,
            "Decompose expanded to items"
        );

        Ok(items)
    }

    /// Expand using static binding resolution (no MCP call)
    fn expand_decompose_static(
        &self,
        spec: &DecomposeSpec,
        bindings: &ResolvedBindings,
        datastore: &RunContext,
    ) -> Result<Vec<serde_json::Value>, NikaError> {
        let source_value = self.resolve_decompose_source(&spec.source, bindings, datastore)?;

        // Expect array
        let items = source_value
            .as_array()
            .ok_or_else(|| -> NikaError {
                BindingError::TypeMismatch {
                    expected: "array".to_string(),
                    actual: self.json_type_name(&source_value),
                    path: spec.source.clone(),
                }
                .into()
            })?
            .clone();

        // Apply max_items limit
        let mut items = items;
        if let Some(max) = spec.max_items {
            items.truncate(max);
        }

        Ok(items)
    }

    /// Expand using nested recursive traversal via MCP
    ///
    /// Recursively follows arcs until max_depth or no more children.
    /// Uses BFS to collect all descendant nodes (excluding root) into a flat array.
    async fn expand_decompose_nested(
        &self,
        spec: &DecomposeSpec,
        bindings: &ResolvedBindings,
        datastore: &RunContext,
    ) -> Result<Vec<serde_json::Value>, NikaError> {
        use serde_json::{json, Value};
        use std::collections::HashSet;

        // Get MCP client
        let server_name = spec.mcp_server();
        let client = self.get_mcp_client(server_name).await?;

        // Resolve source binding
        let source_value = self.resolve_decompose_source(&spec.source, bindings, datastore)?;
        let root_key = self.extract_decompose_key(&source_value)?;

        // Defaults for nested traversal
        let max_depth = spec.max_depth.unwrap_or(3);
        let max_items = spec.max_items.unwrap_or(100); // Safety limit

        debug!(
            root_key = %root_key,
            arc = %spec.traverse,
            max_depth = max_depth,
            max_items = max_items,
            "Starting nested decompose traversal"
        );

        // BFS traversal to collect all descendant nodes
        let mut items: Vec<Value> = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: Vec<(String, usize)> = vec![(root_key.clone(), 0)];

        visited.insert(root_key.clone());

        while let Some((current_key, depth)) = queue.pop() {
            // Stop if we've reached max depth
            if depth >= max_depth {
                continue;
            }

            // Stop if we have enough items
            if items.len() >= max_items {
                break;
            }

            // Call novanet_search (walk mode) for current node
            let params = json!({
                "mode": "walk",
                "start_key": current_key,
                "arc_kinds": [spec.traverse],
                "direction": "outgoing"
            });

            let result = match client.call_tool("novanet_search", params).await {
                Ok(r) => r,
                Err(e) => {
                    debug!(key = %current_key, error = %e, "Traverse failed, skipping node");
                    continue;
                }
            };

            // Parse result
            let result_json: Value = match serde_json::from_str(&result.text()) {
                Ok(v) => v,
                Err(e) => {
                    debug!(key = %current_key, error = %e, "Failed to parse traverse result");
                    continue;
                }
            };

            // Extract child nodes (pass ownership to avoid clone)
            let children = match self.extract_decompose_nodes(result_json) {
                Ok(c) => c,
                Err(_) => continue,
            };

            for child in children {
                // Get child key for tracking
                let child_key = match self.extract_decompose_key(&child) {
                    Ok(k) => k,
                    Err(_) => continue,
                };

                // Skip if already visited (avoid cycles)
                if visited.contains(&child_key) {
                    continue;
                }

                visited.insert(child_key.clone());
                items.push(child);

                // Add to queue for further traversal
                queue.push((child_key, depth + 1));

                // Early exit if we have enough items
                if items.len() >= max_items {
                    break;
                }
            }
        }

        debug!(
            count = items.len(),
            visited = visited.len(),
            "Nested decompose completed"
        );

        Ok(items)
    }

    /// Resolve source binding expression for decompose
    pub(super) fn resolve_decompose_source(
        &self,
        source: &str,
        bindings: &ResolvedBindings,
        datastore: &RunContext,
    ) -> Result<serde_json::Value, NikaError> {
        if source.starts_with("{{with.") && source.ends_with("}}") {
            // Template syntax: {{with.alias}} - supports lazy bindings
            let alias = &source[7..source.len() - 2];
            bindings.get_resolved(alias, datastore)
        } else if let Some(alias) = source.strip_prefix('$') {
            if alias.contains('.') {
                // Path syntax: $task.field
                datastore.resolve_path(alias).ok_or_else(|| {
                    BindingError::NotFound {
                        alias: alias.to_string(),
                    }
                    .into()
                })
            } else {
                // Simple alias - supports lazy bindings
                bindings.get_resolved(alias, datastore)
            }
        } else {
            // Literal value
            Ok(serde_json::Value::String(source.to_string()))
        }
    }

    /// Extract key from source value (string or object with 'key' field)
    pub(super) fn extract_decompose_key(
        &self,
        value: &serde_json::Value,
    ) -> Result<String, NikaError> {
        match value {
            serde_json::Value::String(s) => Ok(s.clone()),
            serde_json::Value::Object(obj) => obj
                .get("key")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| {
                    BindingError::TypeMismatch {
                        expected: "string or object with 'key'".to_string(),
                        actual: "object without 'key'".to_string(),
                        path: "decompose.source".to_string(),
                    }
                    .into()
                }),
            _ => Err(BindingError::TypeMismatch {
                expected: "string or object".to_string(),
                actual: self.json_type_name(value),
                path: "decompose.source".to_string(),
            }
            .into()),
        }
    }

    /// Extract nodes array from novanet_search walk result
    /// Extract nodes from decompose result, taking ownership to avoid cloning
    /// PERF: Takes ownership of Value to avoid cloning arrays
    pub(super) fn extract_decompose_nodes(
        &self,
        result: serde_json::Value,
    ) -> Result<Vec<serde_json::Value>, NikaError> {
        // Try to extract from object fields first (no clone needed)
        if let serde_json::Value::Object(mut map) = result {
            if let Some(serde_json::Value::Array(nodes)) = map.remove("nodes") {
                return Ok(nodes);
            }
            if let Some(serde_json::Value::Array(items)) = map.remove("items") {
                return Ok(items);
            }
            if let Some(serde_json::Value::Array(results)) = map.remove("results") {
                return Ok(results);
            }
        // Handle direct array case
        } else if let serde_json::Value::Array(arr) = result {
            return Ok(arr);
        }
        Err(NikaError::McpInvalidResponse {
            tool: "novanet_search".to_string(),
            reason: "expected nodes/items/results array in response".to_string(),
        })
    }

    /// Get JSON type name for error messages
    pub(super) fn json_type_name(&self, value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::Null => "null",
            serde_json::Value::Bool(_) => "boolean",
            serde_json::Value::Number(_) => "number",
            serde_json::Value::String(_) => "string",
            serde_json::Value::Array(_) => "array",
            serde_json::Value::Object(_) => "object",
        }
        .to_string()
    }
}
