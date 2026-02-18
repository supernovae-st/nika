//! Integration tests for MCP Client
//!
//! Tests the MCP client implementation including:
//! - Client creation with valid/invalid config
//! - Mock client behavior for testing
//! - Tool calls and resource reads

use nika::mcp::{McpClient, McpConfig};
use nika::NikaError;
use pretty_assertions::assert_eq;
use serde_json::json;

// ═══════════════════════════════════════════════════════════════════════════
// SYNCHRONOUS TESTS - Client Creation and Validation
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_mcp_client_creation_with_valid_config() {
    // Arrange
    let config = McpConfig::new("novanet", "npx")
        .with_args(["-y", "@novanet/mcp-server"])
        .with_env("NEO4J_URI", "bolt://localhost:7687");

    // Act
    let result = McpClient::new(config);

    // Assert
    assert!(result.is_ok(), "Should create client with valid config");
    let client = result.unwrap();
    assert_eq!(client.name(), "novanet");
    assert!(
        !client.is_connected(),
        "Client should not be connected initially"
    );
}

#[test]
fn test_mcp_client_config_validation_empty_name() {
    // Arrange
    let config = McpConfig::new("", "npx");

    // Act
    let result = McpClient::new(config);

    // Assert
    assert!(result.is_err(), "Should reject config with empty name");
    let err = result.unwrap_err();
    match err {
        NikaError::ValidationError { reason } => {
            assert!(
                reason.contains("name"),
                "Error should mention 'name': {reason}"
            );
        }
        _ => panic!("Expected ValidationError, got: {err:?}"),
    }
}

#[test]
fn test_mcp_client_config_validation_empty_command() {
    // Arrange
    let config = McpConfig::new("novanet", "");

    // Act
    let result = McpClient::new(config);

    // Assert
    assert!(result.is_err(), "Should reject config with empty command");
    let err = result.unwrap_err();
    match err {
        NikaError::ValidationError { reason } => {
            assert!(
                reason.contains("command"),
                "Error should mention 'command': {reason}"
            );
        }
        _ => panic!("Expected ValidationError, got: {err:?}"),
    }
}

#[test]
fn test_mcp_client_mock_creation() {
    // Act
    let client = McpClient::mock("test-server");

    // Assert
    assert_eq!(client.name(), "test-server");
    assert!(client.is_connected(), "Mock client should be pre-connected");
}

// ═══════════════════════════════════════════════════════════════════════════
// ASYNC TESTS - Tool Calls and Resource Operations
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_mcp_client_mock_tool_call_novanet_describe() {
    // Arrange
    let client = McpClient::mock("novanet");

    // Act
    let result = client.call_tool("novanet_describe", json!({})).await;

    // Assert
    assert!(result.is_ok(), "Mock call_tool should succeed");
    let tool_result = result.unwrap();
    assert!(!tool_result.is_error, "Tool call should not be an error");

    // Verify the mock response contains expected data
    let text = tool_result.text();
    assert!(
        text.contains("nodes") || text.contains("62"),
        "Should contain node count: {text}"
    );
    assert!(
        text.contains("arcs") || text.contains("182"),
        "Should contain arc count: {text}"
    );
}

#[tokio::test]
async fn test_mcp_client_mock_tool_call_novanet_generate() {
    // Arrange
    let client = McpClient::mock("novanet");
    let params = json!({
        "entity": "qr-code",
        "locale": "fr-FR",
        "forms": ["text", "title"]
    });

    // Act
    let result = client.call_tool("novanet_generate", params.clone()).await;

    // Assert
    assert!(result.is_ok(), "Mock call_tool should succeed");
    let tool_result = result.unwrap();
    assert!(!tool_result.is_error, "Tool call should not be an error");

    // Verify response contains entity context
    let text = tool_result.text();
    assert!(
        text.contains("entity") || text.contains("qr-code"),
        "Should contain entity info: {text}"
    );
}

#[tokio::test]
async fn test_mcp_client_call_tool_when_not_connected() {
    // Arrange
    let config = McpConfig::new("test", "echo");
    let client = McpClient::new(config).unwrap();

    // Verify not connected
    assert!(!client.is_connected());

    // Act
    let result = client.call_tool("some_tool", json!({})).await;

    // Assert
    assert!(result.is_err(), "Should fail when not connected");
    let err = result.unwrap_err();
    match err {
        NikaError::McpNotConnected { name } => {
            assert_eq!(name, "test", "Error should contain client name");
        }
        _ => panic!("Expected McpNotConnected, got: {err:?}"),
    }
}

#[tokio::test]
async fn test_mcp_client_mock_list_tools() {
    // Arrange
    let client = McpClient::mock("novanet");

    // Act
    let result = client.list_tools().await;

    // Assert
    assert!(result.is_ok(), "Mock list_tools should succeed");
    let tools = result.unwrap();
    assert!(!tools.is_empty(), "Should return at least one tool");

    // Verify tool definitions have names
    for tool in &tools {
        assert!(!tool.name.is_empty(), "Tool should have a name");
    }
}

#[tokio::test]
async fn test_mcp_client_mock_read_resource() {
    // Arrange
    let client = McpClient::mock("novanet");

    // Act
    let result = client.read_resource("neo4j://entity/qr-code").await;

    // Assert
    assert!(result.is_ok(), "Mock read_resource should succeed");
    let resource = result.unwrap();
    assert_eq!(resource.uri, "neo4j://entity/qr-code");
    assert!(resource.text.is_some(), "Resource should have text content");
}

#[tokio::test]
async fn test_mcp_client_read_resource_when_not_connected() {
    // Arrange
    let config = McpConfig::new("test", "echo");
    let client = McpClient::new(config).unwrap();

    // Act
    let result = client.read_resource("some://resource").await;

    // Assert
    assert!(result.is_err(), "Should fail when not connected");
    match result.unwrap_err() {
        NikaError::McpNotConnected { name } => {
            assert_eq!(name, "test");
        }
        err => panic!("Expected McpNotConnected, got: {err:?}"),
    }
}

#[tokio::test]
async fn test_mcp_client_list_tools_when_not_connected() {
    // Arrange
    let config = McpConfig::new("test", "echo");
    let client = McpClient::new(config).unwrap();

    // Act
    let result = client.list_tools().await;

    // Assert
    assert!(result.is_err(), "Should fail when not connected");
    match result.unwrap_err() {
        NikaError::McpNotConnected { name } => {
            assert_eq!(name, "test");
        }
        err => panic!("Expected McpNotConnected, got: {err:?}"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ASYNC TESTS - Connect/Disconnect (Mock behavior)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_mcp_client_mock_connect_disconnect() {
    // Arrange
    let client = McpClient::mock("test");

    // Mock is already connected
    assert!(client.is_connected());

    // Act - disconnect
    let result = client.disconnect().await;

    // Assert
    assert!(result.is_ok(), "Disconnect should succeed");
    assert!(
        !client.is_connected(),
        "Should be disconnected after disconnect"
    );

    // Act - reconnect
    let result = client.connect().await;

    // Assert
    assert!(result.is_ok(), "Connect should succeed");
    assert!(client.is_connected(), "Should be connected after connect");
}

#[tokio::test]
async fn test_mcp_client_connect_idempotent() {
    // Arrange
    let client = McpClient::mock("test");
    assert!(client.is_connected());

    // Act - connect again (should be idempotent)
    let result = client.connect().await;

    // Assert
    assert!(
        result.is_ok(),
        "Connect when already connected should succeed"
    );
    assert!(client.is_connected());
}

#[tokio::test]
async fn test_mcp_client_disconnect_idempotent() {
    // Arrange
    let config = McpConfig::new("test", "echo");
    let client = McpClient::new(config).unwrap();
    assert!(!client.is_connected());

    // Act - disconnect when not connected
    let result = client.disconnect().await;

    // Assert
    assert!(
        result.is_ok(),
        "Disconnect when not connected should succeed"
    );
    assert!(!client.is_connected());
}
