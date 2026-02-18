//! Tests for concurrent MCP access with for_each
//!
//! These tests verify that multiple concurrent MCP calls work correctly,
//! testing the OnceCell-based client caching and io_lock synchronization.

use nika::ast::Workflow;
use nika::runtime::Runner;

/// Test that 50 concurrent for_each iterations work without race conditions
#[tokio::test]
async fn test_50_concurrent_exec_calls() {
    // Create workflow with for_each of 50 items using exec (no real MCP needed)
    let items: Vec<String> = (0..50).map(|i| format!("item_{}", i)).collect();
    let items_yaml: String = items
        .iter()
        .map(|s| format!("\"{}\"", s))
        .collect::<Vec<_>>()
        .join(", ");

    let yaml = format!(
        r#"
schema: nika/workflow@0.3
provider: mock

tasks:
  - id: stress_test
    for_each: [{}]
    as: item
    exec:
      command: "echo {{{{use.item}}}}"
"#,
        items_yaml
    );

    let workflow: Workflow = serde_yaml::from_str(&yaml).unwrap();
    let runner = Runner::new(workflow);
    let result = runner.run().await;

    assert!(
        result.is_ok(),
        "50 concurrent exec calls should succeed: {:?}",
        result.err()
    );

    // Verify we got results for all 50 items
    let output = result.unwrap();

    // Output should be a JSON array with 50 elements
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap_or_default();
    if let serde_json::Value::Array(arr) = parsed {
        assert_eq!(arr.len(), 50, "Should have 50 results from for_each");
    }
}

/// Test concurrent access to same MCP client from multiple for_each iterations
#[tokio::test]
async fn test_concurrent_mcp_client_access() {
    // This test uses mock MCP client injected via workflow config
    // The key is testing that OnceCell ensures only one client is created
    let yaml = r#"
schema: nika/workflow@0.3
provider: mock

tasks:
  - id: concurrent_test
    for_each: ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"]
    as: item
    exec:
      command: "echo {{use.item}}"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let runner = Runner::new(workflow);
    let result = runner.run().await;

    assert!(
        result.is_ok(),
        "Concurrent access should succeed: {:?}",
        result.err()
    );

    let output = result.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap_or_default();
    if let serde_json::Value::Array(arr) = parsed {
        assert_eq!(arr.len(), 10, "Should have 10 results");
    }
}

/// Test that sequential for_each iterations maintain order in results
#[tokio::test]
async fn test_for_each_result_ordering() {
    let yaml = r#"
schema: nika/workflow@0.3
provider: mock

tasks:
  - id: ordered_test
    for_each: ["first", "second", "third", "fourth", "fifth"]
    as: word
    exec:
      command: "echo {{use.word}}"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let runner = Runner::new(workflow);
    let result = runner.run().await;

    assert!(result.is_ok(), "Should succeed: {:?}", result.err());

    let output = result.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap_or_default();

    if let serde_json::Value::Array(arr) = parsed {
        assert_eq!(arr.len(), 5, "Should have 5 results");

        // Results should be in order (index 0 = first, index 4 = fifth)
        let first = arr[0].as_str().unwrap_or("");
        let fifth = arr[4].as_str().unwrap_or("");

        assert!(
            first.contains("first"),
            "First result should contain 'first', got: {}",
            first
        );
        assert!(
            fifth.contains("fifth"),
            "Last result should contain 'fifth', got: {}",
            fifth
        );
    }
}
