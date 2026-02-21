//! Decomposer Module - Runtime DAG expansion via MCP traversal (v0.5)
//!
//! Resolves `decompose:` modifiers to produce iteration items at runtime.
//! Strategies:
//! - `semantic`: Calls novanet_traverse MCP tool
//! - `static`: Resolves binding to array directly
//! - `nested`: Recursive traversal (not yet implemented)
//!
//! Note: The main decompose logic is integrated into TaskExecutor for direct
//! datastore access. This module provides standalone functions and tests.

#![allow(dead_code)]

use std::sync::Arc;

use serde_json::{json, Value};
use tracing::{debug, instrument};

use crate::ast::decompose::{DecomposeSpec, DecomposeStrategy};
use crate::binding::ResolvedBindings;
use crate::error::NikaError;
use crate::mcp::McpClient;
use crate::store::DataStore;

/// Expand a decompose spec into iteration items
///
/// Returns an array of JSON values that can be used as for_each items.
///
/// # Arguments
/// * `spec` - The decompose specification
/// * `mcp_clients` - Map of MCP client names to clients
/// * `bindings` - Resolved bindings for the current task
/// * `datastore` - Data store for resolving paths
///
/// # Errors
/// Returns error if:
/// - MCP client not found
/// - MCP traversal fails
/// - Source binding cannot be resolved
#[instrument(name = "decompose", skip(mcp_clients, bindings, datastore), fields(
    strategy = ?spec.strategy,
    traverse = %spec.traverse,
    source = %spec.source
))]
pub async fn expand(
    spec: &DecomposeSpec,
    mcp_clients: &dashmap::DashMap<String, Arc<McpClient>>,
    bindings: &ResolvedBindings,
    datastore: &DataStore,
) -> Result<Vec<Value>, NikaError> {
    match spec.strategy {
        DecomposeStrategy::Semantic => {
            expand_semantic(spec, mcp_clients, bindings, datastore).await
        }
        DecomposeStrategy::Static => expand_static(spec, bindings, datastore),
        DecomposeStrategy::Nested => expand_nested(spec, mcp_clients, bindings, datastore).await,
    }
}

/// Expand using semantic traversal via MCP
///
/// Calls `novanet_traverse` to discover items based on graph arcs.
async fn expand_semantic(
    spec: &DecomposeSpec,
    mcp_clients: &dashmap::DashMap<String, Arc<McpClient>>,
    bindings: &ResolvedBindings,
    datastore: &DataStore,
) -> Result<Vec<Value>, NikaError> {
    // Get MCP client
    let server_name = spec.mcp_server();
    let client = mcp_clients
        .get(server_name)
        .ok_or_else(|| NikaError::McpNotConnected {
            name: server_name.to_string(),
        })?;

    // Resolve source binding
    let source_value = resolve_source(&spec.source, bindings, datastore)?;
    let source_key = extract_key(&source_value)?;

    debug!(
        source_key = %source_key,
        arc = %spec.traverse,
        "Calling novanet_traverse for decompose"
    );

    // Call novanet_traverse
    let params = json!({
        "start": source_key,
        "arc": spec.traverse,
        "direction": "outgoing"
    });

    let result = client.call_tool("novanet_traverse", params).await?;

    // Parse JSON from result content
    let result_json: Value =
        serde_json::from_str(&result.text()).map_err(|e| NikaError::McpInvalidResponse {
            tool: "novanet_traverse".to_string(),
            reason: format!("failed to parse JSON response: {}", e),
        })?;

    // Extract nodes from result
    let mut items = extract_nodes(&result_json)?;

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

/// Expand using static binding resolution
///
/// Simply resolves the source binding and expects an array.
fn expand_static(
    spec: &DecomposeSpec,
    bindings: &ResolvedBindings,
    datastore: &DataStore,
) -> Result<Vec<Value>, NikaError> {
    let source_value = resolve_source(&spec.source, bindings, datastore)?;

    // Expect array
    let items = source_value
        .as_array()
        .ok_or_else(|| NikaError::BindingTypeMismatch {
            expected: "array".to_string(),
            actual: type_name(&source_value),
            path: spec.source.clone(),
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
/// Collects all discovered nodes (excluding the root) into a flat array.
async fn expand_nested(
    spec: &DecomposeSpec,
    mcp_clients: &dashmap::DashMap<String, Arc<McpClient>>,
    bindings: &ResolvedBindings,
    datastore: &DataStore,
) -> Result<Vec<Value>, NikaError> {
    use std::collections::HashSet;

    // Get MCP client
    let server_name = spec.mcp_server();
    let client = mcp_clients
        .get(server_name)
        .ok_or_else(|| NikaError::McpNotConnected {
            name: server_name.to_string(),
        })?;

    // Resolve source binding
    let source_value = resolve_source(&spec.source, bindings, datastore)?;
    let root_key = extract_key(&source_value)?;

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

        // Call novanet_traverse for current node
        let params = json!({
            "start": current_key,
            "arc": spec.traverse,
            "direction": "outgoing"
        });

        let result = match client.call_tool("novanet_traverse", params).await {
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

        // Extract child nodes
        let children = match extract_nodes(&result_json) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for child in children {
            // Get child key for tracking
            let child_key = match extract_key(&child) {
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

/// Resolve source binding expression
fn resolve_source(
    source: &str,
    bindings: &ResolvedBindings,
    datastore: &DataStore,
) -> Result<Value, NikaError> {
    // Handle different binding syntaxes
    if source.starts_with("{{use.") && source.ends_with("}}") {
        // Template syntax: {{use.alias}}
        let alias = &source[6..source.len() - 2];
        bindings
            .get(alias)
            .cloned()
            .ok_or_else(|| NikaError::BindingNotFound {
                alias: alias.to_string(),
            })
    } else if let Some(alias) = source.strip_prefix('$') {
        // Dollar syntax: $alias or $task.path
        if alias.contains('.') {
            // Path syntax: $task.field
            datastore
                .resolve_path(alias)
                .ok_or_else(|| NikaError::BindingNotFound {
                    alias: alias.to_string(),
                })
        } else {
            // Simple alias
            bindings
                .get(alias)
                .cloned()
                .ok_or_else(|| NikaError::BindingNotFound {
                    alias: alias.to_string(),
                })
        }
    } else {
        // Literal value (shouldn't happen for decompose, but handle gracefully)
        Ok(Value::String(source.to_string()))
    }
}

/// Extract key from source value
///
/// Handles both string keys and objects with `key` field.
fn extract_key(value: &Value) -> Result<String, NikaError> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Object(obj) => obj
            .get("key")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| NikaError::BindingTypeMismatch {
                expected: "string or object with 'key'".to_string(),
                actual: "object without 'key'".to_string(),
                path: "decompose.source".to_string(),
            }),
        _ => Err(NikaError::BindingTypeMismatch {
            expected: "string or object".to_string(),
            actual: type_name(value),
            path: "decompose.source".to_string(),
        }),
    }
}

/// Extract nodes array from novanet_traverse result
fn extract_nodes(result: &Value) -> Result<Vec<Value>, NikaError> {
    // Try different result formats
    if let Some(nodes) = result.get("nodes").and_then(|v| v.as_array()) {
        return Ok(nodes.clone());
    }

    if let Some(items) = result.get("items").and_then(|v| v.as_array()) {
        return Ok(items.clone());
    }

    if let Some(results) = result.get("results").and_then(|v| v.as_array()) {
        return Ok(results.clone());
    }

    // If result itself is an array
    if let Some(arr) = result.as_array() {
        return Ok(arr.clone());
    }

    Err(NikaError::McpInvalidResponse {
        tool: "novanet_traverse".to_string(),
        reason: "expected nodes/items/results array in response".to_string(),
    })
}

/// Get JSON type name for error messages
fn type_name(value: &Value) -> String {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_key_from_string() {
        let value = Value::String("entity:qr-code".to_string());
        let key = extract_key(&value).unwrap();
        assert_eq!(key, "entity:qr-code");
    }

    #[test]
    fn test_extract_key_from_object() {
        let value = json!({"key": "entity:qr-code", "name": "QR Code"});
        let key = extract_key(&value).unwrap();
        assert_eq!(key, "entity:qr-code");
    }

    #[test]
    fn test_extract_key_from_object_missing_key() {
        let value = json!({"name": "QR Code"});
        let result = extract_key(&value);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_nodes_from_nodes_field() {
        let result = json!({
            "nodes": [{"key": "a"}, {"key": "b"}]
        });
        let nodes = extract_nodes(&result).unwrap();
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn test_extract_nodes_from_items_field() {
        let result = json!({
            "items": [{"key": "a"}, {"key": "b"}]
        });
        let nodes = extract_nodes(&result).unwrap();
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn test_extract_nodes_from_array() {
        let result = json!([{"key": "a"}, {"key": "b"}]);
        let nodes = extract_nodes(&result).unwrap();
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn test_type_name() {
        assert_eq!(type_name(&Value::Null), "null");
        assert_eq!(type_name(&json!(true)), "boolean");
        assert_eq!(type_name(&json!(42)), "number");
        assert_eq!(type_name(&json!("hello")), "string");
        assert_eq!(type_name(&json!([])), "array");
        assert_eq!(type_name(&json!({})), "object");
    }
}
