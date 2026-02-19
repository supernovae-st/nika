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
        };

        let result: Result<Vec<Value>, NikaError> = executor
            .expand_decompose(&spec, &bindings, &datastore)
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("array"));
    }

    #[tokio::test]
    async fn test_expand_decompose_nested_not_implemented() {
        let executor = create_test_executor();
        let datastore = DataStore::new();

        let spec = DecomposeSpec {
            strategy: DecomposeStrategy::Nested,
            traverse: "HAS_CHILD".to_string(),
            source: "$entity".to_string(),
            mcp_server: None,
            max_items: None,
        };

        let bindings = ResolvedBindings::default();
        let result: Result<Vec<Value>, NikaError> = executor
            .expand_decompose(&spec, &bindings, &datastore)
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not implemented") || err.contains("NotImplemented"));
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

/// Test decomposer with mock MCP that returns traverse results
#[tokio::test]
#[ignore = "Requires running NovaNet MCP server with Neo4j"]
async fn test_decompose_semantic_with_real_mcp() {
    // This test requires:
    // 1. NovaNet MCP server running
    // 2. Neo4j with test data
    // See tests/rig_integration_test.rs for similar setup
}
