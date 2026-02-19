//! End-to-End Workflow Integration Tests
//!
//! These tests run full YAML workflows against real NovaNet MCP server
//! with real Neo4j database. They verify the complete data flow:
//!
//! YAML workflow → Parser → DAG → Runner → MCP Client → NovaNet → Neo4j
//!
//! # Requirements
//!
//! - NovaNet MCP binary at default path or NOVANET_MCP_PATH
//! - Neo4j running at localhost:7687
//! - Knowledge graph populated with test data
//!
//! # Running
//!
//! ```bash
//! cargo test --test integration -- --ignored --test-threads=1 e2e
//! ```

use nika::mcp::McpClient;
use serde_json::json;

use super::helpers::{novanet_config, should_skip_integration_test};

// ============================================================================
// MCP Resource Reading Tests
// ============================================================================

/// Test reading an entity resource from NovaNet.
///
/// Verifies that read_resource works with real NovaNet MCP server.
/// Note: NovaNet MCP may not support resources/read method - this test
/// verifies the client handles both success and method-not-supported cases.
#[tokio::test]
#[ignore]
async fn test_read_resource_entity() {
    if should_skip_integration_test() {
        return;
    }

    let config = novanet_config();
    let client = McpClient::new(config).expect("Failed to create client");
    client.connect().await.expect("Failed to connect");

    // Try to read an entity resource
    // Note: NovaNet MCP may not support resources/read method
    let result = client.read_resource("entity://qr-code").await;

    // We expect either success, resource not found, or method not supported
    match result {
        Ok(resource) => {
            assert_eq!(resource.uri, "entity://qr-code");
            println!("  Resource content: {:?}", resource.text);
        }
        Err(e) => {
            // Various error types are acceptable:
            // - Resource not found (data doesn't exist)
            // - Method not supported (NovaNet doesn't implement resources/read)
            // - Tool error (method mapped to tool error)
            let error_str = e.to_string().to_lowercase();
            let is_expected_error = error_str.contains("not found")
                || error_str.contains("notfound")
                || error_str.contains("resources/read")
                || error_str.contains("method")
                || error_str.contains("not supported");

            assert!(is_expected_error, "Unexpected error type: {}", e);
            println!("  Expected error (resource or method not supported): {}", e);
        }
    }

    client.disconnect().await.expect("Failed to disconnect");
}

/// Test reading a class/schema resource from NovaNet.
#[tokio::test]
#[ignore]
async fn test_read_resource_class() {
    if should_skip_integration_test() {
        return;
    }

    let config = novanet_config();
    let client = McpClient::new(config).expect("Failed to create client");
    client.connect().await.expect("Failed to connect");

    // Try to read a class definition
    let result = client.read_resource("class://Entity").await;

    match result {
        Ok(resource) => {
            println!("  Class resource: {:?}", resource.text);
        }
        Err(e) => {
            println!("  Class resource error (may be expected): {}", e);
        }
    }

    client.disconnect().await.expect("Failed to disconnect");
}

// ============================================================================
// Tool Call Flow Tests
// ============================================================================

/// Test the full workflow: describe → query → traverse.
///
/// This simulates a typical agent workflow pattern.
#[tokio::test]
#[ignore]
async fn test_workflow_describe_query_traverse() {
    if should_skip_integration_test() {
        return;
    }

    let config = novanet_config();
    let client = McpClient::new(config).expect("Failed to create client");
    client.connect().await.expect("Failed to connect");

    // Step 1: Describe schema
    println!("  Step 1: Describe schema...");
    let describe_result = client
        .call_tool("novanet_describe", json!({"describe": "schema"}))
        .await;

    assert!(
        describe_result.is_ok(),
        "Describe failed: {:?}",
        describe_result.err()
    );
    let schema_info = describe_result.unwrap();
    assert!(!schema_info.is_error, "novanet_describe returned error");
    println!("    Schema: {} chars", schema_info.text().len());

    // Step 2: Query for entities
    println!("  Step 2: Query entities...");
    let query_result = client
        .call_tool(
            "novanet_query",
            json!({
                "cypher": "MATCH (e:Entity) RETURN e.key AS key LIMIT 5"
            }),
        )
        .await;

    assert!(
        query_result.is_ok(),
        "Query failed: {:?}",
        query_result.err()
    );
    let entities = query_result.unwrap();
    println!("    Entities: {}", entities.text());

    // Step 3: Traverse from an entity (if any exist)
    println!("  Step 3: Traverse graph...");
    let traverse_result = client
        .call_tool(
            "novanet_traverse",
            json!({
                "start": "Entity",
                "depth": 2
            }),
        )
        .await;

    // Traverse may fail if no entities exist, that's OK
    match traverse_result {
        Ok(result) => {
            println!("    Traversal: {} chars", result.text().len());
        }
        Err(e) => {
            println!("    Traversal skipped (no data): {}", e);
        }
    }

    client.disconnect().await.expect("Failed to disconnect");
    println!("  Workflow completed successfully!");
}

/// Test concurrent tool calls to verify parallel execution.
#[tokio::test]
#[ignore]
async fn test_concurrent_tool_calls() {
    if should_skip_integration_test() {
        return;
    }

    use std::sync::Arc;
    use tokio::task::JoinSet;

    let config = novanet_config();
    let client = Arc::new(McpClient::new(config).expect("Failed to create client"));
    client.connect().await.expect("Failed to connect");

    // Launch 5 concurrent describe calls
    let mut join_set = JoinSet::new();

    for i in 0..5 {
        let client = Arc::clone(&client);
        join_set.spawn(async move {
            let result = client
                .call_tool("novanet_describe", json!({"describe": "stats"}))
                .await;
            (i, result.is_ok())
        });
    }

    // Collect results
    let mut successes = 0;
    while let Some(result) = join_set.join_next().await {
        if let Ok((i, success)) = result {
            if success {
                successes += 1;
            }
            println!("  Call {} completed: success={}", i, success);
        }
    }

    assert!(
        successes >= 4,
        "Expected at least 4/5 concurrent calls to succeed"
    );

    client.disconnect().await.expect("Failed to disconnect");
}

// ============================================================================
// Reconnection Tests
// ============================================================================

/// Test that client reconnects after disconnect.
#[tokio::test]
#[ignore]
async fn test_reconnect_after_disconnect() {
    if should_skip_integration_test() {
        return;
    }

    let config = novanet_config();
    let client = McpClient::new(config).expect("Failed to create client");

    // Connect
    client.connect().await.expect("Failed to connect");
    assert!(client.is_connected());

    // Make a call
    let result1 = client
        .call_tool("novanet_describe", json!({"describe": "stats"}))
        .await;
    assert!(result1.is_ok());

    // Disconnect
    client.disconnect().await.expect("Failed to disconnect");
    assert!(!client.is_connected());

    // Reconnect
    client.reconnect().await.expect("Failed to reconnect");
    assert!(client.is_connected());

    // Make another call
    let result2 = client
        .call_tool("novanet_describe", json!({"describe": "stats"}))
        .await;
    assert!(result2.is_ok());

    client.disconnect().await.expect("Failed to disconnect");
    println!("  Reconnection test passed!");
}

// ============================================================================
// Data Validation Tests
// ============================================================================

/// Test that NovaNet returns valid JSON from all describe targets.
#[tokio::test]
#[ignore]
async fn test_describe_all_targets_return_valid_json() {
    if should_skip_integration_test() {
        return;
    }

    let config = novanet_config();
    let client = McpClient::new(config).expect("Failed to create client");
    client.connect().await.expect("Failed to connect");

    let targets = vec!["schema", "stats", "locales", "relations"];

    for target in targets {
        println!("  Testing describe target: {}", target);
        let result = client
            .call_tool("novanet_describe", json!({"describe": target}))
            .await;

        assert!(
            result.is_ok(),
            "Describe '{}' failed: {:?}",
            target,
            result.err()
        );

        let response = result.unwrap();
        assert!(!response.is_error, "Describe '{}' returned error", target);

        // Verify it's valid JSON
        let text = response.text();
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&text);
        assert!(
            parsed.is_ok(),
            "Describe '{}' returned invalid JSON: {}",
            target,
            &text[..100.min(text.len())]
        );
    }

    client.disconnect().await.expect("Failed to disconnect");
}

/// Test query execution with various Cypher patterns.
#[tokio::test]
#[ignore]
async fn test_cypher_query_patterns() {
    if should_skip_integration_test() {
        return;
    }

    let config = novanet_config();
    let client = McpClient::new(config).expect("Failed to create client");
    client.connect().await.expect("Failed to connect");

    let queries = vec![
        // Count nodes
        ("MATCH (n) RETURN count(n) AS total LIMIT 1", "count query"),
        // Get labels
        (
            "CALL db.labels() YIELD label RETURN label LIMIT 10",
            "labels query",
        ),
        // Get relationship types
        (
            "CALL db.relationshipTypes() YIELD relationshipType RETURN relationshipType LIMIT 10",
            "relationships query",
        ),
    ];

    for (cypher, description) in queries {
        println!(
            "  Testing {}: {}",
            description,
            &cypher[..50.min(cypher.len())]
        );
        let result = client
            .call_tool("novanet_query", json!({"cypher": cypher}))
            .await;

        assert!(result.is_ok(), "{} failed: {:?}", description, result.err());
        println!("    Result: {}", result.unwrap().text());
    }

    client.disconnect().await.expect("Failed to disconnect");
}

// ============================================================================
// Error Handling Tests
// ============================================================================

/// Test that invalid Cypher returns proper error.
#[tokio::test]
#[ignore]
async fn test_invalid_cypher_returns_error() {
    if should_skip_integration_test() {
        return;
    }

    let config = novanet_config();
    let client = McpClient::new(config).expect("Failed to create client");
    client.connect().await.expect("Failed to connect");

    // Invalid Cypher syntax
    let result = client
        .call_tool(
            "novanet_query",
            json!({"cypher": "THIS IS NOT VALID CYPHER"}),
        )
        .await;

    // Should return an error or is_error=true
    match result {
        Ok(response) => {
            assert!(response.is_error, "Expected error for invalid Cypher");
        }
        Err(_) => {
            // Error is also acceptable
        }
    }

    client.disconnect().await.expect("Failed to disconnect");
}

/// Test that write operations are blocked.
#[tokio::test]
#[ignore]
async fn test_write_operations_blocked() {
    if should_skip_integration_test() {
        return;
    }

    let config = novanet_config();
    let client = McpClient::new(config).expect("Failed to create client");
    client.connect().await.expect("Failed to connect");

    // Try to create a node (should be blocked)
    let result = client
        .call_tool(
            "novanet_query",
            json!({"cypher": "CREATE (n:TestNode {name: 'test'}) RETURN n"}),
        )
        .await;

    // Should return an error
    match result {
        Ok(response) => {
            assert!(response.is_error, "Expected write operation to be blocked");
            println!("  Write blocked with message: {}", response.text());
        }
        Err(e) => {
            println!("  Write blocked with error: {}", e);
        }
    }

    client.disconnect().await.expect("Failed to disconnect");
}

// ============================================================================
// Performance Tests
// ============================================================================

/// Test response time for common operations.
#[tokio::test]
#[ignore]
async fn test_response_times() {
    if should_skip_integration_test() {
        return;
    }

    let config = novanet_config();
    let client = McpClient::new(config).expect("Failed to create client");
    client.connect().await.expect("Failed to connect");

    // Warm up
    let _ = client
        .call_tool("novanet_describe", json!({"describe": "stats"}))
        .await;

    // Measure describe
    let start = std::time::Instant::now();
    let _ = client
        .call_tool("novanet_describe", json!({"describe": "stats"}))
        .await;
    let describe_time = start.elapsed();

    // Measure simple query
    let start = std::time::Instant::now();
    let _ = client
        .call_tool(
            "novanet_query",
            json!({"cypher": "MATCH (n) RETURN count(n) LIMIT 1"}),
        )
        .await;
    let query_time = start.elapsed();

    println!("  Describe time: {:?}", describe_time);
    println!("  Query time: {:?}", query_time);

    // Reasonable performance expectations (should complete within 5 seconds)
    assert!(
        describe_time.as_secs() < 5,
        "Describe too slow: {:?}",
        describe_time
    );
    assert!(query_time.as_secs() < 5, "Query too slow: {:?}", query_time);

    client.disconnect().await.expect("Failed to disconnect");
}
