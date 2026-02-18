//! MCP Client Implementation (v0.2)
//!
//! Provides a client for connecting to MCP (Model Context Protocol) servers.
//! Supports both real server connections and mock mode for testing.
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
use crate::mcp::types::{
    ContentBlock, McpConfig, ResourceContent, ToolCallResult, ToolDefinition,
};

/// MCP Client for connecting to and interacting with MCP servers.
///
/// The client can operate in two modes:
/// - **Real mode**: Spawns an MCP server process and communicates via stdio
/// - **Mock mode**: Returns canned responses for testing
#[derive(Debug)]
pub struct McpClient {
    /// Server name (from config or mock)
    name: String,

    /// Server configuration (None for mock clients)
    /// Will be used when real MCP connection is implemented.
    #[allow(dead_code)]
    config: Option<McpConfig>,

    /// Connection state (atomic for interior mutability)
    connected: AtomicBool,

    /// Whether this is a mock client
    is_mock: bool,
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

        Ok(Self {
            name: config.name.clone(),
            config: Some(config),
            connected: AtomicBool::new(false),
            is_mock: false,
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
            config: None,
            connected: AtomicBool::new(true), // Mock is pre-connected
            is_mock: true,
        }
    }

    /// Get the server name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Check if the client is connected to the server.
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// Connect to the MCP server.
    ///
    /// For mock clients, this is a no-op that always succeeds.
    /// For real clients, this spawns the server process and establishes communication.
    ///
    /// This method is idempotent - calling it when already connected succeeds.
    ///
    /// # Errors
    ///
    /// Returns `NikaError::McpStartError` if the server process fails to start.
    pub async fn connect(&self) -> Result<()> {
        if self.is_connected() {
            return Ok(());
        }

        if self.is_mock {
            self.connected.store(true, Ordering::SeqCst);
            return Ok(());
        }

        // TODO: Real connection implementation
        // For now, just mark as connected for testing
        // In a real implementation, this would:
        // 1. Spawn the server process using config.command and config.args
        // 2. Set up stdin/stdout communication
        // 3. Perform MCP handshake
        self.connected.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Disconnect from the MCP server.
    ///
    /// For mock clients, this just updates the connection state.
    /// For real clients, this terminates the server process.
    ///
    /// This method is idempotent - calling it when already disconnected succeeds.
    pub async fn disconnect(&self) -> Result<()> {
        if !self.is_connected() {
            return Ok(());
        }

        // TODO: Real disconnection implementation
        // For now, just mark as disconnected
        // In a real implementation, this would:
        // 1. Send shutdown notification
        // 2. Wait for graceful termination
        // 3. Kill process if necessary
        self.connected.store(false, Ordering::SeqCst);
        Ok(())
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
        if !self.is_connected() {
            return Err(NikaError::McpNotConnected {
                name: self.name.clone(),
            });
        }

        if self.is_mock {
            return Ok(self.mock_tool_call(name, params));
        }

        // TODO: Real tool call implementation
        // For now, return a placeholder
        Err(NikaError::McpToolError {
            tool: name.to_string(),
            reason: "Real MCP connection not implemented yet".to_string(),
        })
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
        if !self.is_connected() {
            return Err(NikaError::McpNotConnected {
                name: self.name.clone(),
            });
        }

        if self.is_mock {
            return Ok(self.mock_read_resource(uri));
        }

        // TODO: Real resource read implementation
        Err(NikaError::McpResourceNotFound {
            uri: uri.to_string(),
        })
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
        if !self.is_connected() {
            return Err(NikaError::McpNotConnected {
                name: self.name.clone(),
            });
        }

        if self.is_mock {
            return Ok(self.mock_list_tools());
        }

        // TODO: Real list tools implementation
        Ok(Vec::new())
    }

    // ═══════════════════════════════════════════════════════════════
    // MOCK IMPLEMENTATIONS
    // ═══════════════════════════════════════════════════════════════

    /// Generate mock response for tool calls.
    fn mock_tool_call(&self, name: &str, params: Value) -> ToolCallResult {
        match name {
            "novanet_describe" => {
                let response = serde_json::json!({
                    "nodes": 62,
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let result = client.call_tool("unknown_tool", serde_json::json!({})).await;
        assert!(result.is_ok());
        assert!(!result.unwrap().is_error);
    }
}
