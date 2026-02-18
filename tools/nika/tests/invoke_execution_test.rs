//! Integration tests for Invoke Verb Execution
//!
//! Tests the execution of invoke verbs (MCP tool calls and resource reads)
//! using mock McpClient. This validates the runtime execution path for
//! the invoke action type.

use nika::ast::InvokeParams;
use nika::error::NikaError;
use nika::mcp::McpClient;
use serde_json::json;

// ═══════════════════════════════════════════════════════════════════════════
// HELPER FUNCTION - Executes invoke params using McpClient
// ═══════════════════════════════════════════════════════════════════════════

/// Execute an invoke action using the provided MCP client.
///
/// This helper replicates the logic that should be in the executor,
/// allowing us to test the behavior in isolation.
async fn execute_invoke(
    invoke: &InvokeParams,
    client: &McpClient,
) -> Result<serde_json::Value, NikaError> {
    // Validate the invoke params first
    invoke
        .validate()
        .map_err(|e| NikaError::ValidationError { reason: e })?;

    if let Some(tool) = &invoke.tool {
        // Tool call path
        let params = invoke.params.clone().unwrap_or(serde_json::Value::Null);
        let result = client.call_tool(tool, params).await?;
        let text = result.text();
        // Try to parse as JSON, fall back to string
        match serde_json::from_str(&text) {
            Ok(v) => Ok(v),
            Err(_) => Ok(serde_json::Value::String(text)),
        }
    } else if let Some(resource) = &invoke.resource {
        // Resource read path
        let content = client.read_resource(resource).await?;
        match content.text {
            Some(text) => match serde_json::from_str(&text) {
                Ok(v) => Ok(v),
                Err(_) => Ok(serde_json::Value::String(text)),
            },
            None => Ok(serde_json::Value::Null),
        }
    } else {
        // This should never happen if validate() passed
        unreachable!("validate() ensures tool or resource is set")
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TOOL CALL TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_invoke_execution_tool_call() {
    // Arrange
    let client = McpClient::mock("novanet");

    let invoke = InvokeParams {
        mcp: "novanet".to_string(),
        tool: Some("novanet_generate".to_string()),
        params: Some(json!({"mode": "block", "entity": "qr-code"})),
        resource: None,
    };

    // Act
    let result = execute_invoke(&invoke, &client).await;

    // Assert
    assert!(
        result.is_ok(),
        "Tool call should succeed: {:?}",
        result.err()
    );
    let value = result.unwrap();
    assert!(value.is_object(), "Result should be a JSON object: {value}");
}

#[tokio::test]
async fn test_invoke_execution_tool_call_minimal() {
    // Tool call without params (params defaults to null)
    let client = McpClient::mock("novanet");

    let invoke = InvokeParams {
        mcp: "novanet".to_string(),
        tool: Some("novanet_describe".to_string()),
        params: None,
        resource: None,
    };

    let result = execute_invoke(&invoke, &client).await;

    assert!(result.is_ok(), "Tool call without params should succeed");
    let value = result.unwrap();
    // novanet_describe mock returns {"nodes": 62, "arcs": 182, ...}
    assert!(
        value.get("nodes").is_some() || value.get("arcs").is_some(),
        "Should contain graph stats: {value}"
    );
}

#[tokio::test]
async fn test_invoke_execution_tool_call_with_params() {
    // Tool call with specific params that affect mock response
    let client = McpClient::mock("novanet");

    let invoke = InvokeParams {
        mcp: "novanet".to_string(),
        tool: Some("novanet_generate".to_string()),
        params: Some(json!({
            "entity": "qr-code",
            "locale": "fr-FR",
            "forms": ["text", "title"]
        })),
        resource: None,
    };

    let result = execute_invoke(&invoke, &client).await;

    assert!(result.is_ok());
    let value = result.unwrap();
    // Mock should include entity and locale in response
    assert_eq!(value.get("entity"), Some(&json!("qr-code")));
    assert_eq!(value.get("locale"), Some(&json!("fr-FR")));
}

// ═══════════════════════════════════════════════════════════════════════════
// RESOURCE READ TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_invoke_execution_resource_read() {
    // Arrange
    let client = McpClient::mock("novanet");

    let invoke = InvokeParams {
        mcp: "novanet".to_string(),
        tool: None,
        params: None,
        resource: Some("entity://qr-code".to_string()),
    };

    // Act
    let result = execute_invoke(&invoke, &client).await;

    // Assert
    assert!(
        result.is_ok(),
        "Resource read should succeed: {:?}",
        result.err()
    );
    let value = result.unwrap();
    assert!(value.is_object(), "Result should be a JSON object: {value}");
}

#[tokio::test]
async fn test_invoke_execution_resource_read_neo4j_uri() {
    // Mock client generates specific response for neo4j:// URIs
    let client = McpClient::mock("novanet");

    let invoke = InvokeParams {
        mcp: "novanet".to_string(),
        tool: None,
        params: None,
        resource: Some("neo4j://entity/qr-code".to_string()),
    };

    let result = execute_invoke(&invoke, &client).await;

    assert!(result.is_ok());
    let value = result.unwrap();
    // Mock response for neo4j://entity/* includes id and type
    assert_eq!(value.get("id"), Some(&json!("qr-code")));
    assert_eq!(value.get("type"), Some(&json!("Entity")));
}

// ═══════════════════════════════════════════════════════════════════════════
// VALIDATION ERROR TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_invoke_execution_fails_with_both_tool_and_resource() {
    let client = McpClient::mock("novanet");

    let invoke = InvokeParams {
        mcp: "novanet".to_string(),
        tool: Some("novanet_generate".to_string()),
        params: None,
        resource: Some("entity://qr-code".to_string()),
    };

    let result = execute_invoke(&invoke, &client).await;

    assert!(
        result.is_err(),
        "Should fail when both tool and resource are set"
    );
    match result.unwrap_err() {
        NikaError::ValidationError { reason } => {
            assert!(
                reason.contains("mutually exclusive"),
                "Error should mention mutual exclusivity: {reason}"
            );
        }
        err => panic!("Expected ValidationError, got: {err:?}"),
    }
}

#[tokio::test]
async fn test_invoke_execution_fails_with_neither_tool_nor_resource() {
    let client = McpClient::mock("novanet");

    let invoke = InvokeParams {
        mcp: "novanet".to_string(),
        tool: None,
        params: None,
        resource: None,
    };

    let result = execute_invoke(&invoke, &client).await;

    assert!(
        result.is_err(),
        "Should fail when neither tool nor resource is set"
    );
    match result.unwrap_err() {
        NikaError::ValidationError { reason } => {
            assert!(
                reason.contains("must be specified"),
                "Error should mention requirement: {reason}"
            );
        }
        err => panic!("Expected ValidationError, got: {err:?}"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CONNECTION ERROR TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_invoke_execution_fails_when_not_connected() {
    // Create a real client (not mock) that is not connected
    let config = nika::mcp::McpConfig::new("novanet", "echo");
    let client = McpClient::new(config).unwrap();

    // Not connected
    assert!(!client.is_connected());

    let invoke = InvokeParams {
        mcp: "novanet".to_string(),
        tool: Some("novanet_generate".to_string()),
        params: None,
        resource: None,
    };

    let result = execute_invoke(&invoke, &client).await;

    assert!(result.is_err(), "Should fail when client not connected");
    match result.unwrap_err() {
        NikaError::McpNotConnected { name } => {
            assert_eq!(name, "novanet");
        }
        err => panic!("Expected McpNotConnected, got: {err:?}"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// EDGE CASES
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_invoke_execution_unknown_tool_returns_generic_response() {
    // Mock client returns a generic success for unknown tools
    let client = McpClient::mock("novanet");

    let invoke = InvokeParams {
        mcp: "novanet".to_string(),
        tool: Some("unknown_tool".to_string()),
        params: Some(json!({"key": "value"})),
        resource: None,
    };

    let result = execute_invoke(&invoke, &client).await;

    assert!(result.is_ok(), "Unknown tool should return generic success");
    let value = result.unwrap();
    // Mock returns {"tool": "unknown_tool", "status": "success", ...}
    assert_eq!(value.get("tool"), Some(&json!("unknown_tool")));
    assert_eq!(value.get("status"), Some(&json!("success")));
}

#[tokio::test]
async fn test_invoke_execution_with_empty_params() {
    let client = McpClient::mock("novanet");

    let invoke = InvokeParams {
        mcp: "novanet".to_string(),
        tool: Some("novanet_describe".to_string()),
        params: Some(json!({})),
        resource: None,
    };

    let result = execute_invoke(&invoke, &client).await;

    assert!(result.is_ok(), "Empty params should be valid");
}
