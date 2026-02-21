//! Integration tests for decompose: modifier (v0.5)
//!
//! Tests runtime DAG expansion via MCP traversal.
//! Phase 4 of MVP 8: RLM Enhancements.

use nika::ast::{DecomposeStrategy, Workflow};

// ============================================================================
// PARSING TESTS (GREEN - AST is implemented)
// ============================================================================

#[test]
fn test_decompose_spec_parses_in_workflow() {
    let yaml = r#"
schema: nika/workflow@0.4
provider: claude
tasks:
  - id: expand_children
    decompose:
      strategy: semantic
      traverse: HAS_CHILD
      source: $entity
    infer:
      prompt: "Generate for {{use.item}}"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(workflow.tasks.len(), 1);

    let task = &workflow.tasks[0];
    assert!(task.has_decompose());

    let spec = task.decompose_spec().expect("decompose should be present");
    assert_eq!(spec.strategy, DecomposeStrategy::Semantic);
    assert_eq!(spec.traverse, "HAS_CHILD");
    assert_eq!(spec.source, "$entity");
    assert_eq!(spec.mcp_server(), "novanet");
}

#[test]
fn test_decompose_spec_parses_with_custom_mcp() {
    let yaml = r#"
schema: nika/workflow@0.4
provider: claude
tasks:
  - id: expand_custom
    decompose:
      strategy: nested
      traverse: HAS_NATIVE
      source: "{{use.entity_key}}"
      mcp_server: custom_server
      max_items: 5
    infer:
      prompt: "Process {{use.item}}"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let task = &workflow.tasks[0];
    let spec = task.decompose_spec().unwrap();

    assert_eq!(spec.strategy, DecomposeStrategy::Nested);
    assert_eq!(spec.traverse, "HAS_NATIVE");
    assert_eq!(spec.mcp_server(), "custom_server");
    assert_eq!(spec.max_items, Some(5));
}

#[test]
fn test_decompose_coexists_with_for_each_settings() {
    // decompose can use for_each settings like concurrency and fail_fast
    let yaml = r#"
schema: nika/workflow@0.4
provider: claude
tasks:
  - id: parallel_decompose
    decompose:
      traverse: HAS_CHILD
      source: $parent
    concurrency: 5
    fail_fast: false
    infer:
      prompt: "Generate {{use.item}}"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let task = &workflow.tasks[0];

    assert!(task.has_decompose());
    assert_eq!(task.for_each_concurrency(), 5);
    assert!(!task.for_each_fail_fast());
}

#[test]
fn test_task_without_decompose() {
    let yaml = r#"
schema: nika/workflow@0.4
provider: claude
tasks:
  - id: simple_task
    infer:
      prompt: "Hello world"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let task = &workflow.tasks[0];

    assert!(!task.has_decompose());
    assert!(task.decompose_spec().is_none());
}

// ============================================================================
// RUNTIME UNIT TESTS (executor methods)
// ============================================================================

#[cfg(test)]
mod executor_unit_tests {
    use nika::ast::decompose::{DecomposeSpec, DecomposeStrategy};
    use nika::binding::ResolvedBindings;
    use nika::error::NikaError;
    use nika::event::EventLog;
    use nika::runtime::TaskExecutor;
    use nika::store::DataStore;
    use serde_json::{json, Value};

    fn create_test_executor() -> TaskExecutor {
        TaskExecutor::new("mock", None, Default::default(), EventLog::new())
    }

    #[tokio::test]
    async fn test_expand_decompose_static_with_array() {
        let executor = create_test_executor();
        let datastore = DataStore::new();

        // Pre-populate bindings with locales array
        // Note: $alias (without .) resolves from bindings, not datastore
        let mut bindings = ResolvedBindings::default();
        bindings.set("locales".to_string(), json!(["en-US", "fr-FR", "de-DE"]));

        let spec = DecomposeSpec {
            strategy: DecomposeStrategy::Static,
            traverse: "IGNORED".to_string(),
            source: "$locales".to_string(),
            mcp_server: None,
            max_items: None,
            max_depth: None,
        };

        let result: Result<Vec<Value>, NikaError> = executor
            .expand_decompose(&spec, &bindings, &datastore)
            .await;

        assert!(result.is_ok());
        let items = result.unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], json!("en-US"));
        assert_eq!(items[1], json!("fr-FR"));
        assert_eq!(items[2], json!("de-DE"));
    }

    #[tokio::test]
    async fn test_expand_decompose_static_with_max_items() {
        let executor = create_test_executor();
        let datastore = DataStore::new();

        // Pre-populate bindings with 5 items
        let mut bindings = ResolvedBindings::default();
        bindings.set("items".to_string(), json!([1, 2, 3, 4, 5]));

        let spec = DecomposeSpec {
            strategy: DecomposeStrategy::Static,
            traverse: "IGNORED".to_string(),
            source: "$items".to_string(),
            mcp_server: None,
            max_items: Some(2), // Limit to 2
            max_depth: None,
        };

        let result: Result<Vec<Value>, NikaError> = executor
            .expand_decompose(&spec, &bindings, &datastore)
            .await;

        assert!(result.is_ok());
        let items = result.unwrap();
        assert_eq!(items.len(), 2); // Truncated to max_items
        assert_eq!(items[0], json!(1));
        assert_eq!(items[1], json!(2));
    }

    #[tokio::test]
    async fn test_expand_decompose_static_non_array_fails() {
        let executor = create_test_executor();
        let datastore = DataStore::new();

        // Pre-populate bindings with a string (not an array)
        let mut bindings = ResolvedBindings::default();
        bindings.set("scalar".to_string(), json!("not an array"));

        let spec = DecomposeSpec {
            strategy: DecomposeStrategy::Static,
            traverse: "IGNORED".to_string(),
            source: "$scalar".to_string(),
            mcp_server: None,
            max_items: None,
            max_depth: None,
        };

        let result: Result<Vec<Value>, NikaError> = executor
            .expand_decompose(&spec, &bindings, &datastore)
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("array"));
    }

    #[tokio::test]
    async fn test_expand_decompose_nested_requires_mcp() {
        let executor = create_test_executor();
        let datastore = DataStore::new();

        let spec = DecomposeSpec {
            strategy: DecomposeStrategy::Nested,
            traverse: "HAS_CHILD".to_string(),
            source: "$entity".to_string(),
            mcp_server: None,
            max_items: None,
            max_depth: Some(2),
        };

        let mut bindings = ResolvedBindings::default();
        bindings.set("entity".to_string(), json!({"key": "root-entity"}));

        let result: Result<Vec<Value>, NikaError> = executor
            .expand_decompose(&spec, &bindings, &datastore)
            .await;

        // Should fail because MCP server isn't configured (test executor has no MCP configs)
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not configured") || err.contains("McpNotConfigured"),
            "Expected McpNotConfigured error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_expand_decompose_semantic_missing_mcp_fails() {
        let executor = create_test_executor();
        let datastore = DataStore::new();

        // Pre-populate bindings with entity
        let mut bindings = ResolvedBindings::default();
        bindings.set("entity".to_string(), json!({"key": "qr-code"}));

        let spec = DecomposeSpec {
            strategy: DecomposeStrategy::Semantic,
            traverse: "HAS_CHILD".to_string(),
            source: "$entity".to_string(),
            mcp_server: None, // Uses default "novanet" which isn't connected
            max_items: None,
            max_depth: None,
        };

        let result: Result<Vec<Value>, NikaError> = executor
            .expand_decompose(&spec, &bindings, &datastore)
            .await;

        // Should fail because MCP server isn't connected
        assert!(result.is_err());
    }
}

// ============================================================================
// INTEGRATION TESTS (require MCP server)
// ============================================================================

/// Test decomposer with real NovaNet MCP server
#[tokio::test]
#[ignore = "Requires running NovaNet MCP server with Neo4j"]
async fn test_decompose_semantic_with_real_mcp() {
    use nika::mcp::{McpClient, McpConfig};
    use serde_json::json;
    use std::sync::Arc;

    // Path to NovaNet MCP binary
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = std::path::Path::new(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("Should find workspace root");

    let mcp_bin = workspace_root.join("novanet-dev/tools/novanet-mcp/target/release/novanet-mcp");
    if !mcp_bin.exists() {
        eprintln!("NovaNet MCP binary not found. Build with:");
        eprintln!("  cd novanet-dev/tools/novanet-mcp && cargo build --release");
        return;
    }

    // Setup MCP client
    let password = std::env::var("NOVANET_MCP_NEO4J_PASSWORD")
        .unwrap_or_else(|_| "novanetpassword".to_string());

    let config = McpConfig::new("novanet", mcp_bin.to_string_lossy())
        .with_env("NOVANET_MCP_NEO4J_URI", "bolt://localhost:7687")
        .with_env("NOVANET_MCP_NEO4J_USER", "neo4j")
        .with_env("NOVANET_MCP_NEO4J_PASSWORD", &password);

    let client = Arc::new(McpClient::new(config).expect("Should create client"));
    client.connect().await.expect("Should connect to MCP");

    // Verify connection by listing tools
    let tools = client.list_tools().await.expect("Should list tools");
    assert!(
        tools.iter().any(|t| t.name == "novanet_traverse"),
        "novanet_traverse tool should be available"
    );

    // Test: Call novanet_traverse directly to verify it works
    let traverse_result = client
        .call_tool(
            "novanet_traverse",
            json!({
                "start_key": "qr-code",
                "arc_kinds": ["HAS_NATIVE"],
                "direction": "outgoing"
            }),
        )
        .await;

    assert!(
        traverse_result.is_ok(),
        "novanet_traverse should succeed: {:?}",
        traverse_result.err()
    );

    let result = traverse_result.unwrap();
    assert!(!result.is_error, "Result should not be an error");

    // Parse the result and verify we got nodes
    let result_json: serde_json::Value =
        serde_json::from_str(&result.text()).expect("Should parse JSON");

    // The result should have nodes (EntityNative instances for qr-code)
    let nodes = result_json
        .get("nodes")
        .or_else(|| result_json.get("items"))
        .or_else(|| result_json.as_array().map(|_| &result_json));

    assert!(
        nodes.is_some(),
        "Result should contain nodes: {}",
        result.text()
    );

    eprintln!(
        "✓ novanet_traverse returned {} nodes",
        nodes
            .and_then(|n| n.as_array())
            .map(|a| a.len())
            .unwrap_or(0)
    );

    // Cleanup
    let _ = client.disconnect().await;
}
