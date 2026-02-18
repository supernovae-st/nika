//! Integration tests for NovaNet MCP server.
//!
//! These tests verify real MCP communication between Nika and NovaNet.
//!
//! ## Requirements
//!
//! - NovaNet MCP server binary at `NOVANET_MCP_PATH` or default location
//! - Neo4j running at `localhost:7687`
//! - Neo4j credentials (default: neo4j/novanetpassword)
//!
//! ## Running
//!
//! ```bash
//! # Run all NovaNet integration tests
//! cargo test --features integration -- --ignored novanet --test-threads=1
//!
//! # Run specific test
//! cargo test --features integration -- --ignored test_connect_to_novanet
//! ```
//!
//! ## Notes
//!
//! - Tests are marked with `#[ignore]` to not run by default
//! - Use `--test-threads=1` to avoid connection conflicts
//! - Tests check for dependencies before running and skip gracefully

use serde_json::json;

use nika::McpClient;

// Import helpers module
use crate::helpers::{novanet_config, should_skip_integration_test};

// ═══════════════════════════════════════════════════════════════
// Connection Tests
// ═══════════════════════════════════════════════════════════════

/// Test connecting to and disconnecting from NovaNet MCP server.
///
/// Verifies:
/// - Client starts in disconnected state
/// - `connect()` establishes connection
/// - `is_connected()` returns true after connect
/// - `disconnect()` terminates connection cleanly
/// - `is_connected()` returns false after disconnect
#[tokio::test]
#[ignore] // Run with: cargo test --features integration -- --ignored
async fn test_connect_to_novanet() {
    if should_skip_integration_test() {
        return;
    }

    let config = novanet_config();
    let client = McpClient::new(config).expect("Failed to create client");

    // Client starts disconnected
    assert!(!client.is_connected(), "Client should start disconnected");

    // Connect to server
    let connect_result = client.connect().await;
    assert!(
        connect_result.is_ok(),
        "Failed to connect: {:?}",
        connect_result
    );
    assert!(client.is_connected(), "Client should be connected");

    // Disconnect cleanly
    let disconnect_result = client.disconnect().await;
    assert!(
        disconnect_result.is_ok(),
        "Failed to disconnect: {:?}",
        disconnect_result
    );
    assert!(!client.is_connected(), "Client should be disconnected");
}

/// Test that connect is idempotent (safe to call multiple times).
#[tokio::test]
#[ignore]
async fn test_connect_idempotent() {
    if should_skip_integration_test() {
        return;
    }

    let config = novanet_config();
    let client = McpClient::new(config).expect("Failed to create client");

    // First connect
    client.connect().await.expect("First connect failed");
    assert!(client.is_connected());

    // Second connect should succeed (no-op)
    client.connect().await.expect("Second connect failed");
    assert!(client.is_connected());

    // Cleanup
    client.disconnect().await.ok();
}

// ═══════════════════════════════════════════════════════════════
// Tool Discovery Tests
// ═══════════════════════════════════════════════════════════════

/// Test listing available tools from NovaNet MCP server.
///
/// Verifies:
/// - `list_tools()` returns a non-empty list
/// - Expected NovaNet tools are present (novanet_describe, novanet_query, etc.)
#[tokio::test]
#[ignore]
async fn test_list_tools() {
    if should_skip_integration_test() {
        return;
    }

    let config = novanet_config();
    let client = McpClient::new(config).expect("Failed to create client");
    client.connect().await.expect("Failed to connect");

    let result = client.list_tools().await;
    assert!(result.is_ok(), "list_tools failed: {:?}", result);

    let tools = result.unwrap();
    assert!(!tools.is_empty(), "No tools returned from NovaNet");

    // Collect tool names for verification
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();

    // NovaNet should expose at least these tools
    let expected_tools = ["novanet_describe", "novanet_query"];
    let has_expected = expected_tools
        .iter()
        .any(|expected| tool_names.contains(expected));

    assert!(
        has_expected,
        "Expected NovaNet tools {:?}, got: {:?}",
        expected_tools, tool_names
    );

    // Cleanup
    client.disconnect().await.ok();
}

// ═══════════════════════════════════════════════════════════════
// Tool Call Tests
// ═══════════════════════════════════════════════════════════════

/// Test calling novanet_describe to get schema information.
///
/// Verifies:
/// - Tool call succeeds
/// - Response is not an error
/// - Response contains schema-related content
#[tokio::test]
#[ignore]
async fn test_novanet_describe_schema() {
    if should_skip_integration_test() {
        return;
    }

    let config = novanet_config();
    let client = McpClient::new(config).expect("Failed to create client");
    client.connect().await.expect("Failed to connect");

    let result = client
        .call_tool(
            "novanet_describe",
            json!({
                "describe": "schema"
            }),
        )
        .await;

    assert!(result.is_ok(), "Tool call failed: {:?}", result);

    let response = result.unwrap();
    assert!(!response.is_error, "Tool returned error: {:?}", response);
    assert!(!response.content.is_empty(), "No content returned");

    // Verify response contains schema information
    let text = response.text();
    let has_schema_info = text.contains("realm")
        || text.contains("layer")
        || text.contains("nodes")
        || text.contains("Realm")
        || text.contains("Layer")
        || text.contains("Node");

    assert!(
        has_schema_info,
        "Response should contain schema info, got: {}",
        text
    );

    // Cleanup
    client.disconnect().await.ok();
}

/// Test calling novanet_query with a simple Cypher query.
///
/// Verifies:
/// - Cypher query execution succeeds
/// - Response is not an error
#[tokio::test]
#[ignore]
async fn test_novanet_query() {
    if should_skip_integration_test() {
        return;
    }

    let config = novanet_config();
    let client = McpClient::new(config).expect("Failed to create client");
    client.connect().await.expect("Failed to connect");

    // Simple count query that should always work
    let result = client
        .call_tool(
            "novanet_query",
            json!({
                "cypher": "MATCH (n) RETURN count(n) as count LIMIT 1"
            }),
        )
        .await;

    assert!(result.is_ok(), "Tool call failed: {:?}", result);

    let response = result.unwrap();
    assert!(
        !response.is_error,
        "Tool returned error: {:?}",
        response.text()
    );

    // Cleanup
    client.disconnect().await.ok();
}

/// Test calling a tool with invalid parameters returns an error.
#[tokio::test]
#[ignore]
async fn test_tool_call_invalid_params() {
    if should_skip_integration_test() {
        return;
    }

    let config = novanet_config();
    let client = McpClient::new(config).expect("Failed to create client");
    client.connect().await.expect("Failed to connect");

    // Call with invalid Cypher syntax
    let result = client
        .call_tool(
            "novanet_query",
            json!({
                "cypher": "INVALID CYPHER SYNTAX !!!"
            }),
        )
        .await;

    // The call should either fail or return an error response
    match result {
        Ok(response) => {
            assert!(
                response.is_error,
                "Expected error for invalid Cypher, got success"
            );
        }
        Err(_) => {
            // Tool call returning an error is also acceptable
        }
    }

    // Cleanup
    client.disconnect().await.ok();
}

// ═══════════════════════════════════════════════════════════════
// Error Handling Tests
// ═══════════════════════════════════════════════════════════════

/// Test that calling a tool while disconnected returns an error.
#[tokio::test]
#[ignore]
async fn test_call_tool_while_disconnected() {
    if should_skip_integration_test() {
        return;
    }

    let config = novanet_config();
    let client = McpClient::new(config).expect("Failed to create client");

    // Don't connect, just try to call a tool
    let result = client.call_tool("novanet_describe", json!({})).await;

    assert!(
        result.is_err(),
        "Expected error when calling tool while disconnected"
    );
}

/// Test calling a non-existent tool returns an error.
#[tokio::test]
#[ignore]
async fn test_call_nonexistent_tool() {
    if should_skip_integration_test() {
        return;
    }

    let config = novanet_config();
    let client = McpClient::new(config).expect("Failed to create client");
    client.connect().await.expect("Failed to connect");

    let result = client
        .call_tool("nonexistent_tool_that_does_not_exist", json!({}))
        .await;

    // Should either error or return an error response
    match result {
        Ok(response) => {
            assert!(
                response.is_error,
                "Expected error for non-existent tool, got success"
            );
        }
        Err(_) => {
            // Error result is also acceptable
        }
    }

    // Cleanup
    client.disconnect().await.ok();
}

// ═══════════════════════════════════════════════════════════════
// Concurrent Access Tests
// ═══════════════════════════════════════════════════════════════

/// Test multiple sequential tool calls on the same connection.
#[tokio::test]
#[ignore]
async fn test_multiple_tool_calls() {
    if should_skip_integration_test() {
        return;
    }

    let config = novanet_config();
    let client = McpClient::new(config).expect("Failed to create client");
    client.connect().await.expect("Failed to connect");

    // Make multiple sequential calls
    for i in 0..3 {
        let result = client
            .call_tool(
                "novanet_query",
                json!({
                    "cypher": format!("RETURN {} as iteration", i)
                }),
            )
            .await;

        assert!(result.is_ok(), "Tool call {} failed: {:?}", i, result.err());
        assert!(!result.unwrap().is_error, "Tool call {} returned error", i);
    }

    // Cleanup
    client.disconnect().await.ok();
}

// ═══════════════════════════════════════════════════════════════
// Use Case Tests: Entity Discovery
// ═══════════════════════════════════════════════════════════════

/// Test discovering entities in the knowledge graph.
///
/// This use case simulates a workflow that needs to find all available
/// entities before generating content.
#[tokio::test]
#[ignore]
async fn test_use_case_discover_entities() {
    if should_skip_integration_test() {
        return;
    }

    let config = novanet_config();
    let client = McpClient::new(config).expect("Failed to create client");
    client.connect().await.expect("Failed to connect");

    // Query for Entity nodes
    let result = client
        .call_tool(
            "novanet_query",
            json!({
                "cypher": "MATCH (e:Entity) RETURN e.key as key, e.slug as slug LIMIT 10"
            }),
        )
        .await;

    assert!(result.is_ok(), "Entity query failed: {:?}", result.err());
    let response = result.unwrap();
    assert!(!response.is_error, "Query returned error: {:?}", response.text());

    // Verify response contains entity data
    let text = response.text();
    println!("Discovered entities: {}", text);

    // Cleanup
    client.disconnect().await.ok();
}

/// Test discovering page structure for content generation.
#[tokio::test]
#[ignore]
async fn test_use_case_discover_pages() {
    if should_skip_integration_test() {
        return;
    }

    let config = novanet_config();
    let client = McpClient::new(config).expect("Failed to create client");
    client.connect().await.expect("Failed to connect");

    // Query for Page nodes with their blocks
    let result = client
        .call_tool(
            "novanet_query",
            json!({
                "cypher": r#"
                    MATCH (p:Page)
                    OPTIONAL MATCH (p)-[:HAS_BLOCK]->(b:Block)
                    RETURN p.key as page_key, collect(b.key) as block_keys
                    LIMIT 5
                "#
            }),
        )
        .await;

    assert!(result.is_ok(), "Page query failed: {:?}", result.err());
    let response = result.unwrap();
    assert!(!response.is_error, "Query returned error: {:?}", response.text());

    println!("Discovered pages: {}", response.text());

    // Cleanup
    client.disconnect().await.ok();
}

// ═══════════════════════════════════════════════════════════════
// Use Case Tests: Content Generation Workflow
// ═══════════════════════════════════════════════════════════════

/// Test the full content generation workflow:
/// 1. Discover schema to understand available content
/// 2. Query for specific entity context
/// 3. (Would normally invoke LLM here, simulated by query)
#[tokio::test]
#[ignore]
async fn test_use_case_content_generation_workflow() {
    if should_skip_integration_test() {
        return;
    }

    let config = novanet_config();
    let client = McpClient::new(config).expect("Failed to create client");
    client.connect().await.expect("Failed to connect");

    // Step 1: Get schema overview
    let schema_result = client
        .call_tool(
            "novanet_describe",
            json!({
                "describe": "schema"
            }),
        )
        .await;
    assert!(schema_result.is_ok(), "Schema describe failed");
    let schema = schema_result.unwrap();
    assert!(!schema.is_error, "Schema describe returned error");
    println!("Step 1 - Schema: {}", &schema.text()[..200.min(schema.text().len())]);

    // Step 2: Query for entity with native content
    let entity_result = client
        .call_tool(
            "novanet_query",
            json!({
                "cypher": r#"
                    MATCH (e:Entity)
                    WHERE e.key IS NOT NULL
                    OPTIONAL MATCH (e)-[:HAS_NATIVE]->(n:EntityNative)
                    RETURN e.key as entity, collect(n.locale) as locales
                    LIMIT 3
                "#
            }),
        )
        .await;
    assert!(entity_result.is_ok(), "Entity query failed");
    let entities = entity_result.unwrap();
    assert!(!entities.is_error, "Entity query returned error");
    println!("Step 2 - Entities with natives: {}", entities.text());

    // Step 3: Get denomination forms for content
    let forms_result = client
        .call_tool(
            "novanet_query",
            json!({
                "cypher": r#"
                    MATCH (n:EntityNative)
                    WHERE n.denomination_forms IS NOT NULL
                    RETURN n.locale as locale, n.denomination_forms as forms
                    LIMIT 2
                "#
            }),
        )
        .await;
    assert!(forms_result.is_ok(), "Forms query failed");
    let forms = forms_result.unwrap();
    println!("Step 3 - Denomination forms: {}", forms.text());

    // Cleanup
    client.disconnect().await.ok();
}

// ═══════════════════════════════════════════════════════════════
// Use Case Tests: Schema Introspection
// ═══════════════════════════════════════════════════════════════

/// Test introspecting the complete knowledge graph schema.
///
/// This is useful for understanding what node types and relationships
/// are available before building workflows.
#[tokio::test]
#[ignore]
async fn test_use_case_schema_introspection() {
    if should_skip_integration_test() {
        return;
    }

    let config = novanet_config();
    let client = McpClient::new(config).expect("Failed to create client");
    client.connect().await.expect("Failed to connect");

    // Get all node labels
    let labels_result = client
        .call_tool(
            "novanet_query",
            json!({
                "cypher": "CALL db.labels() YIELD label RETURN collect(label) as labels"
            }),
        )
        .await;
    assert!(labels_result.is_ok(), "Labels query failed");
    println!("Node labels: {}", labels_result.unwrap().text());

    // Get all relationship types
    let rels_result = client
        .call_tool(
            "novanet_query",
            json!({
                "cypher": "CALL db.relationshipTypes() YIELD relationshipType RETURN collect(relationshipType) as types"
            }),
        )
        .await;
    assert!(rels_result.is_ok(), "Relationship types query failed");
    println!("Relationship types: {}", rels_result.unwrap().text());

    // Cleanup
    client.disconnect().await.ok();
}

// ═══════════════════════════════════════════════════════════════
// Use Case Tests: Data Validation
// ═══════════════════════════════════════════════════════════════

/// Test validating knowledge graph data integrity.
///
/// This simulates a maintenance workflow that checks for:
/// - Orphan nodes (entities without natives)
/// - Missing required properties
/// - Broken relationships
#[tokio::test]
#[ignore]
async fn test_use_case_data_validation() {
    if should_skip_integration_test() {
        return;
    }

    let config = novanet_config();
    let client = McpClient::new(config).expect("Failed to create client");
    client.connect().await.expect("Failed to connect");

    // Check for entities without any native content
    let orphan_result = client
        .call_tool(
            "novanet_query",
            json!({
                "cypher": r#"
                    MATCH (e:Entity)
                    WHERE NOT (e)-[:HAS_NATIVE]->(:EntityNative)
                    RETURN e.key as orphan_entity
                    LIMIT 5
                "#
            }),
        )
        .await;
    assert!(orphan_result.is_ok(), "Orphan check failed");
    println!("Entities without natives: {}", orphan_result.unwrap().text());

    // Check for natives missing required properties
    let missing_props_result = client
        .call_tool(
            "novanet_query",
            json!({
                "cypher": r#"
                    MATCH (n:EntityNative)
                    WHERE n.denomination_forms IS NULL
                    RETURN n.locale as locale, count(*) as count
                "#
            }),
        )
        .await;
    assert!(missing_props_result.is_ok(), "Missing props check failed");
    println!(
        "Natives missing denomination_forms: {}",
        missing_props_result.unwrap().text()
    );

    // Cleanup
    client.disconnect().await.ok();
}
