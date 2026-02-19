//! MCP Client Implementation (v0.3)
//!
//! Provides a client for connecting to MCP (Model Context Protocol) servers.
//! Uses rmcp SDK for real connections, with mock mode for testing.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika::mcp::{McpClient, McpConfig};
//! use serde_json::json;
//!
//! // Create client from config
//! let config = McpConfig::new("novanet", "npx")
//!     .with_args(["-y", "@novanet/mcp-server"]);
//! let client = McpClient::new(config)?;
//!
//! // Connect and call tool
//! client.connect().await?;
//! let result = client.call_tool("novanet_describe", json!({})).await?;
//! ```
//!
//! ## Mock Mode
//!
//! For testing, use `McpClient::mock()` to create a pre-connected client
//! that returns canned responses:
//!
//! ```rust,ignore
//! let client = McpClient::mock("novanet");
//! assert!(client.is_connected());
//! ```

use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;

use crate::error::{NikaError, Result};
use crate::mcp::rmcp_adapter::RmcpClientAdapter;
use crate::mcp::types::{ContentBlock, McpConfig, ResourceContent, ToolCallResult, ToolDefinition};

/// MCP Client for connecting to and interacting with MCP servers.
///
/// The client can operate in two modes:
/// - **Real mode**: Uses rmcp SDK via RmcpClientAdapter
/// - **Mock mode**: Returns canned responses for testing
pub struct McpClient {
    /// Server name (from config or mock)
    name: String,

    /// Connection state (atomic for interior mutability)
    /// For mock clients, this tracks mock state.
    /// For real clients, rmcp adapter tracks actual connection.
    connected: AtomicBool,

    /// Whether this is a mock client
    is_mock: bool,

    /// rmcp adapter for real connections (None for mock clients)
    adapter: Option<RmcpClientAdapter>,
}

impl std::fmt::Debug for McpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClient")
            .field("name", &self.name)
            .field("connected", &self.connected)
            .field("is_mock", &self.is_mock)
            .field("has_adapter", &self.adapter.is_some())
            .finish()
    }
}

impl McpClient {
    /// Create a new MCP client from configuration.
    ///
    /// Validates the configuration and returns an error if invalid.
    /// The client is created in disconnected state.
    ///
    /// # Errors
    ///
    /// Returns `NikaError::ValidationError` if:
    /// - `config.name` is empty
    /// - `config.command` is empty
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let config = McpConfig::new("novanet", "npx")
    ///     .with_args(["-y", "@novanet/mcp-server"]);
    /// let client = McpClient::new(config)?;
    /// assert!(!client.is_connected());
    /// ```
    pub fn new(config: McpConfig) -> Result<Self> {
        // Validate configuration
        if config.name.is_empty() {
            return Err(NikaError::ValidationError {
                reason: "MCP server name cannot be empty".to_string(),
            });
        }

        if config.command.is_empty() {
            return Err(NikaError::ValidationError {
                reason: "MCP server command cannot be empty".to_string(),
            });
        }

        let name = config.name.clone();
        let adapter = RmcpClientAdapter::new(config);

        Ok(Self {
            name,
            connected: AtomicBool::new(false),
            is_mock: false,
            adapter: Some(adapter),
        })
    }

    /// Create a mock MCP client for testing.
    ///
    /// The mock client is pre-connected and returns canned responses:
    /// - `novanet_describe`: Returns `{"nodes": 62, "arcs": 182}`
    /// - `novanet_generate`: Returns entity context JSON
    /// - Other tools: Returns a generic success response
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let client = McpClient::mock("novanet");
    /// assert!(client.is_connected());
    /// ```
    pub fn mock(name: &str) -> Self {
        Self {
            name: name.to_string(),
            connected: AtomicBool::new(true), // Mock is pre-connected
            is_mock: true,
            adapter: None,
        }
    }

    /// Get the server name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Check if the client is connected to the server.
    pub fn is_connected(&self) -> bool {
        if self.is_mock {
            return self.connected.load(Ordering::SeqCst);
        }
        // For real clients, check adapter state synchronously
        // This is a best-effort check - use is_connected_async for accurate state
        self.connected.load(Ordering::SeqCst)
    }

    /// Check connection state asynchronously (accurate for real clients).
    pub async fn is_connected_async(&self) -> bool {
        if self.is_mock {
            return self.connected.load(Ordering::SeqCst);
        }
        if let Some(adapter) = &self.adapter {
            adapter.is_connected().await
        } else {
            false
        }
    }

    /// Connect to the MCP server.
    ///
    /// For mock clients, this is a no-op that always succeeds.
    /// For real clients, this uses rmcp SDK to connect.
    ///
    /// This method is idempotent - calling it when already connected succeeds.
    ///
    /// # Errors
    ///
    /// Returns `NikaError::McpStartError` if the server process fails to start.
    pub async fn connect(&self) -> Result<()> {
        if self.is_mock {
            self.connected.store(true, Ordering::SeqCst);
            return Ok(());
        }

        let adapter = self.adapter.as_ref().ok_or_else(|| NikaError::McpNotConnected {
            name: self.name.clone(),
        })?;

        adapter.connect().await?;
        self.connected.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Disconnect from the MCP server.
    ///
    /// For mock clients, this just updates the connection state.
    /// For real clients, this terminates the server process via rmcp.
    ///
    /// This method is idempotent - calling it when already disconnected succeeds.
    pub async fn disconnect(&self) -> Result<()> {
        if self.is_mock {
            self.connected.store(false, Ordering::SeqCst);
            return Ok(());
        }

        if let Some(adapter) = &self.adapter {
            adapter.disconnect().await?;
        }
        self.connected.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Reconnect to the MCP server.
    ///
    /// Useful when the connection is broken (e.g., broken pipe, server crashed).
    /// This terminates any existing connection and establishes a new one.
    ///
    /// # Errors
    ///
    /// Returns `NikaError::McpStartError` if reconnection fails.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // After detecting a broken connection
    /// client.reconnect().await?;
    /// // Retry the failed operation
    /// ```
    pub async fn reconnect(&self) -> Result<()> {
        if self.is_mock {
            self.connected.store(true, Ordering::SeqCst);
            return Ok(());
        }

        let adapter = self.adapter.as_ref().ok_or_else(|| NikaError::McpNotConnected {
            name: self.name.clone(),
        })?;

        adapter.reconnect().await?;
        self.connected.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Check if an error indicates a broken connection.
    ///
    /// Used to determine if a reconnection attempt should be made.
    pub fn is_connection_error(error: &NikaError) -> bool {
        let error_str = error.to_string().to_lowercase();
        error_str.contains("broken pipe")
            || error_str.contains("connection reset")
            || error_str.contains("connection refused")
            || error_str.contains("eof")
            || error_str.contains("stdin not available")
            || error_str.contains("stdout not available")
    }

    /// Call an MCP tool with the given parameters.
    ///
    /// # Arguments
    ///
    /// * `name` - Tool name (e.g., "novanet_generate", "read_file")
    /// * `params` - Tool parameters as JSON value
    ///
    /// # Errors
    ///
    /// Returns `NikaError::McpNotConnected` if the client is not connected.
    /// Returns `NikaError::McpToolError` if the tool call fails.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let result = client.call_tool("novanet_generate", json!({
    ///     "entity": "qr-code",
    ///     "locale": "fr-FR"
    /// })).await?;
    /// ```
    pub async fn call_tool(&self, name: &str, params: Value) -> Result<ToolCallResult> {
        if self.is_mock {
            if !self.connected.load(Ordering::SeqCst) {
                return Err(NikaError::McpNotConnected {
                    name: self.name.clone(),
                });
            }
            return Ok(self.mock_tool_call(name, params));
        }

        // Real mode: use rmcp adapter with retry logic
        let adapter = self.adapter.as_ref().ok_or_else(|| NikaError::McpNotConnected {
            name: self.name.clone(),
        })?;

        let max_retries = 3;
        let mut last_error: Option<NikaError> = None;

        for attempt in 0..=max_retries {
            match adapter.call_tool(name, params.clone()).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if Self::is_connection_error(&e) && attempt < max_retries {
                        tracing::warn!(
                            mcp_server = %self.name,
                            tool = %name,
                            attempt = attempt + 1,
                            error = %e,
                            "Connection error, attempting reconnect"
                        );

                        if let Err(reconnect_err) = adapter.reconnect().await {
                            tracing::error!(
                                mcp_server = %self.name,
                                error = %reconnect_err,
                                "Failed to reconnect"
                            );
                            last_error = Some(e);
                            break;
                        }

                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        last_error = Some(e);
                        continue;
                    }

                    return Err(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| NikaError::McpToolError {
            tool: name.to_string(),
            reason: "Connection failed after reconnection attempts".to_string(),
        }))
    }

    /// Read a resource from the MCP server.
    ///
    /// # Arguments
    ///
    /// * `uri` - Resource URI (e.g., "file:///path", "neo4j://entity/qr-code")
    ///
    /// # Errors
    ///
    /// Returns `NikaError::McpNotConnected` if the client is not connected.
    /// Returns `NikaError::McpResourceNotFound` if the resource doesn't exist.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let resource = client.read_resource("neo4j://entity/qr-code").await?;
    /// ```
    pub async fn read_resource(&self, uri: &str) -> Result<ResourceContent> {
        if self.is_mock {
            if !self.connected.load(Ordering::SeqCst) {
                return Err(NikaError::McpNotConnected {
                    name: self.name.clone(),
                });
            }
            return Ok(self.mock_read_resource(uri));
        }

        // Real mode: use rmcp adapter with retry logic
        let adapter = self.adapter.as_ref().ok_or_else(|| NikaError::McpNotConnected {
            name: self.name.clone(),
        })?;

        let max_retries = 3;
        let mut last_error: Option<NikaError> = None;

        for attempt in 0..=max_retries {
            match adapter.read_resource(uri).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    // Preserve McpResourceNotFound errors - no retry needed
                    if matches!(&e, NikaError::McpResourceNotFound { .. }) {
                        return Err(e);
                    }

                    if Self::is_connection_error(&e) && attempt < max_retries {
                        tracing::warn!(
                            mcp_server = %self.name,
                            uri = %uri,
                            attempt = attempt + 1,
                            error = %e,
                            "Connection error, attempting reconnect"
                        );

                        if let Err(reconnect_err) = adapter.reconnect().await {
                            tracing::error!(
                                mcp_server = %self.name,
                                error = %reconnect_err,
                                "Failed to reconnect"
                            );
                            last_error = Some(e);
                            break;
                        }

                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        last_error = Some(e);
                        continue;
                    }

                    return Err(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| NikaError::McpToolError {
            tool: "resources/read".to_string(),
            reason: "Connection failed after reconnection attempts".to_string(),
        }))
    }

    /// List all available tools from the MCP server.
    ///
    /// # Errors
    ///
    /// Returns `NikaError::McpNotConnected` if the client is not connected.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let tools = client.list_tools().await?;
    /// for tool in tools {
    ///     println!("Tool: {}", tool.name);
    /// }
    /// ```
    pub async fn list_tools(&self) -> Result<Vec<ToolDefinition>> {
        if self.is_mock {
            if !self.connected.load(Ordering::SeqCst) {
                return Err(NikaError::McpNotConnected {
                    name: self.name.clone(),
                });
            }
            return Ok(self.mock_list_tools());
        }

        // Real mode: use rmcp adapter
        let adapter = self.adapter.as_ref().ok_or_else(|| NikaError::McpNotConnected {
            name: self.name.clone(),
        })?;

        adapter.list_tools().await
    }

    // ═══════════════════════════════════════════════════════════════
    // MOCK IMPLEMENTATIONS
    // ═══════════════════════════════════════════════════════════════

    /// Generate mock response for tool calls.
    fn mock_tool_call(&self, name: &str, params: Value) -> ToolCallResult {
        match name {
            "novanet_describe" => {
                let response = serde_json::json!({
                    "nodes": 61,
                    "arcs": 182,
                    "labels": ["Entity", "EntityNative", "Page", "Block"],
                    "relationships": ["HAS_NATIVE", "CONTAINS", "FLOWS_TO"]
                });
                ToolCallResult::success(vec![ContentBlock::text(response.to_string())])
            }

            "novanet_generate" => {
                // Extract entity from params for a realistic response
                let entity = params
                    .get("entity")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let locale = params
                    .get("locale")
                    .and_then(|v| v.as_str())
                    .unwrap_or("en-US");

                let response = serde_json::json!({
                    "entity": entity,
                    "locale": locale,
                    "context": {
                        "title": format!("{} - Generated Title", entity),
                        "description": format!("Auto-generated content for {} in {}", entity, locale),
                        "keywords": ["generated", "mock", entity]
                    }
                });
                ToolCallResult::success(vec![ContentBlock::text(response.to_string())])
            }

            "novanet_traverse" => {
                let response = serde_json::json!({
                    "path": [
                        {"type": "Entity", "id": "qr-code"},
                        {"type": "EntityNative", "id": "qr-code:fr-FR"}
                    ],
                    "total": 2
                });
                ToolCallResult::success(vec![ContentBlock::text(response.to_string())])
            }

            _ => {
                // Generic success response for unknown tools
                let response = serde_json::json!({
                    "tool": name,
                    "status": "success",
                    "message": "Mock tool call completed"
                });
                ToolCallResult::success(vec![ContentBlock::text(response.to_string())])
            }
        }
    }

    /// Generate mock response for resource reads.
    fn mock_read_resource(&self, uri: &str) -> ResourceContent {
        // Generate a mock resource based on URI pattern
        let text = if uri.starts_with("neo4j://entity/") {
            let entity = uri.strip_prefix("neo4j://entity/").unwrap_or("unknown");
            serde_json::json!({
                "id": entity,
                "type": "Entity",
                "properties": {
                    "name": entity,
                    "created": "2024-01-01T00:00:00Z"
                }
            })
            .to_string()
        } else if uri.starts_with("file://") {
            "Mock file content".to_string()
        } else {
            serde_json::json!({
                "uri": uri,
                "content": "Mock resource content"
            })
            .to_string()
        };

        ResourceContent::new(uri)
            .with_mime_type("application/json")
            .with_text(text)
    }

    /// Get tool definitions synchronously.
    ///
    /// For mock clients, returns mock tool definitions.
    /// For real clients, returns an empty vector (use list_tools() async method instead).
    ///
    /// This method is primarily used for building rig agents where we need
    /// tool definitions during construction.
    pub fn get_tool_definitions(&self) -> Vec<ToolDefinition> {
        if self.is_mock {
            self.mock_list_tools()
        } else {
            // For real clients, tools should be listed via async list_tools()
            // Return empty for now - caller should use list_tools() for real clients
            Vec::new()
        }
    }

    /// Generate mock tool definitions.
    fn mock_list_tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition::new("novanet_describe")
                .with_description("Describe the NovaNet knowledge graph schema"),
            ToolDefinition::new("novanet_generate")
                .with_description("Generate native content for an entity")
                .with_input_schema(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "entity": {"type": "string", "description": "Entity ID"},
                        "locale": {"type": "string", "description": "Target locale (e.g., fr-FR)"},
                        "forms": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Content forms to generate"
                        }
                    },
                    "required": ["entity"]
                })),
            ToolDefinition::new("novanet_traverse")
                .with_description("Traverse the knowledge graph from a starting node")
                .with_input_schema(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "start": {"type": "string", "description": "Starting node (e.g., entity:qr-code)"},
                        "arc": {"type": "string", "description": "Arc type to follow"}
                    },
                    "required": ["start"]
                })),
        ]
    }
}

// Drop is handled by RmcpClientAdapter which cleans up the child process

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════
    // CONCURRENT CALL TESTS
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_multiple_sequential_calls() {
        // Verify multiple sequential calls work
        let client = McpClient::mock("test");

        for i in 0..10 {
            let result = client
                .call_tool("test_tool", serde_json::json!({"iteration": i}))
                .await;
            assert!(
                result.is_ok(),
                "Call {} should succeed: {:?}",
                i,
                result.err()
            );
        }
    }

    #[tokio::test]
    async fn test_concurrent_calls() {
        // Verify concurrent calls work
        let client = std::sync::Arc::new(McpClient::mock("test"));

        let handles: Vec<_> = (0..20)
            .map(|i| {
                let client = std::sync::Arc::clone(&client);
                tokio::spawn(async move {
                    client
                        .call_tool("test_tool", serde_json::json!({"iteration": i}))
                        .await
                })
            })
            .collect();

        for (i, handle) in handles.into_iter().enumerate() {
            let result = handle.await.expect("Task should not panic");
            assert!(result.is_ok(), "Concurrent call {} should succeed", i);
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // BASIC TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_client_name_accessor() {
        let config = McpConfig::new("test-server", "echo");
        let client = McpClient::new(config).unwrap();
        assert_eq!(client.name(), "test-server");
    }

    #[test]
    fn test_mock_client_is_pre_connected() {
        let client = McpClient::mock("test");
        assert!(client.is_connected());
        assert!(client.is_mock);
    }

    #[test]
    fn test_real_client_starts_disconnected() {
        let config = McpConfig::new("test", "echo");
        let client = McpClient::new(config).unwrap();
        assert!(!client.is_connected());
        assert!(!client.is_mock);
    }

    #[tokio::test]
    async fn test_mock_tool_call_returns_success() {
        let client = McpClient::mock("test");
        let result = client
            .call_tool("unknown_tool", serde_json::json!({}))
            .await;
        assert!(result.is_ok());
        assert!(!result.unwrap().is_error);
    }

    // ═══════════════════════════════════════════════════════════════
    // RESOURCE READ TESTS
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_mock_read_resource_entity() {
        let client = McpClient::mock("test");
        let result = client.read_resource("neo4j://entity/qr-code").await;
        assert!(result.is_ok());

        let resource = result.unwrap();
        assert_eq!(resource.uri, "neo4j://entity/qr-code");
        assert!(resource.text.is_some());
    }

    #[tokio::test]
    async fn test_mock_read_resource_file() {
        let client = McpClient::mock("test");
        let result = client.read_resource("file:///tmp/test.txt").await;
        assert!(result.is_ok());

        let resource = result.unwrap();
        assert_eq!(resource.uri, "file:///tmp/test.txt");
    }

    // ═══════════════════════════════════════════════════════════════
    // DROP TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_mock_client_drop_is_noop() {
        // Mock clients should not try to kill any process
        let client = McpClient::mock("test");
        assert!(client.is_mock);
        // Dropping should not panic
        drop(client);
    }

    #[test]
    fn test_real_client_drop_without_process() {
        // Real client that was never connected should drop safely
        let config = McpConfig::new("test", "echo");
        let client = McpClient::new(config).unwrap();
        assert!(!client.is_mock);
        // No process was spawned, drop should be safe
        drop(client);
    }
}
