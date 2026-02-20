//! rmcp Adapter Layer
//!
//! This module wraps Anthropic's official rmcp SDK to provide Nika's MCP client interface.
//! It handles the translation between Nika's API and rmcp's Service/Transport abstractions.
//!
//! ## Architecture
//!
//! ```text
//! McpClient (Nika API)
//!     │
//!     ├── Mock Mode ──► Direct mock responses (testing)
//!     │
//!     └── Real Mode ──► RmcpClientAdapter
//!                           │
//!                           └── rmcp::Service<ClientHandler>
//!                                   │
//!                                   └── TokioChildProcess transport
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika::mcp::{McpConfig, RmcpClientAdapter};
//!
//! let config = McpConfig::new("novanet", "cargo")
//!     .with_args(["run", "--manifest-path", "path/to/Cargo.toml"]);
//!
//! let adapter = RmcpClientAdapter::new(config);
//! adapter.connect().await?;
//!
//! let result = adapter.call_tool("novanet_describe", json!({})).await?;
//! ```

use std::process::Stdio;

use parking_lot::Mutex;
use rmcp::model::{CallToolRequestParams, ListToolsResult};
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::TokioChildProcess;
use rmcp::ServiceExt;
use serde_json::Value;
use tokio::process::Command;
use tokio::sync::Mutex as AsyncMutex;

use crate::error::{NikaError, Result};
use crate::mcp::types::{ContentBlock, McpConfig, ResourceContent, ToolCallResult, ToolDefinition};

/// Running rmcp service type alias
/// RunningService<Role, Handler> where Handler implements Service<Role>
type RmcpService = RunningService<RoleClient, ()>;

/// rmcp Client Adapter
///
/// Wraps rmcp's Service to provide Nika's MCP client interface.
/// Handles connection lifecycle, tool calls, and resource reads.
pub struct RmcpClientAdapter {
    /// Server name (from config)
    name: String,

    /// Server configuration
    config: McpConfig,

    /// Running rmcp service (None when disconnected)
    service: AsyncMutex<Option<RmcpService>>,

    /// Protocol version reported by server
    server_version: Mutex<Option<String>>,
}

impl std::fmt::Debug for RmcpClientAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RmcpClientAdapter")
            .field("name", &self.name)
            .field("config", &self.config)
            .field("connected", &self.is_connected_sync())
            .finish()
    }
}

impl RmcpClientAdapter {
    /// Create a new rmcp client adapter from configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - MCP server configuration
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let config = McpConfig::new("novanet", "cargo")
    ///     .with_args(["run", "--manifest-path", "path/to/Cargo.toml"]);
    /// let adapter = RmcpClientAdapter::new(config);
    /// ```
    pub fn new(config: McpConfig) -> Self {
        Self {
            name: config.name.clone(),
            config,
            service: AsyncMutex::new(None),
            server_version: Mutex::new(None),
        }
    }

    /// Get the server name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Check if connected (sync version for Debug impl)
    fn is_connected_sync(&self) -> bool {
        // Try to check without blocking - return false if lock is held
        self.service
            .try_lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }

    /// Check if the client is connected to the server.
    pub async fn is_connected(&self) -> bool {
        self.service.lock().await.is_some()
    }

    /// Connect to the MCP server.
    ///
    /// Spawns the server process and establishes MCP communication.
    /// The rmcp SDK handles the initialize/initialized handshake.
    ///
    /// # Errors
    ///
    /// Returns `NikaError::McpStartError` if the server fails to start.
    pub async fn connect(&self) -> Result<()> {
        let mut guard = self.service.lock().await;

        if guard.is_some() {
            return Ok(()); // Already connected
        }

        // Build command from config
        let mut cmd = Command::new(&self.config.command);
        cmd.args(&self.config.args);

        // Suppress stderr to avoid polluting TUI output
        // MCP communication happens over stdin/stdout, stderr is only for logging
        cmd.stderr(Stdio::null());

        // Add environment variables
        for (key, value) in &self.config.env {
            cmd.env(key, value);
        }

        // Create transport
        let transport = TokioChildProcess::new(cmd).map_err(|e| NikaError::McpStartError {
            name: self.name.clone(),
            reason: format!("Failed to create transport: {}", e),
        })?;

        // Connect to server using rmcp's serve pattern
        // The () implements ClientHandler with default behavior
        let service =
            ().serve(transport)
                .await
                .map_err(|e| NikaError::McpStartError {
                    name: self.name.clone(),
                    reason: format!("Failed to connect: {}", e),
                })?;

        // Store server info
        if let Some(info) = service.peer_info() {
            *self.server_version.lock() = Some(info.protocol_version.to_string());
        }

        *guard = Some(service);
        Ok(())
    }

    /// Disconnect from the MCP server.
    ///
    /// Gracefully closes the connection.
    pub async fn disconnect(&self) -> Result<()> {
        let mut guard = self.service.lock().await;

        if let Some(service) = guard.take() {
            // Graceful shutdown
            let _ = service.cancel().await;
        }

        *self.server_version.lock() = None;
        Ok(())
    }

    /// Reconnect to the MCP server.
    ///
    /// Disconnects if connected, then establishes a new connection.
    pub async fn reconnect(&self) -> Result<()> {
        tracing::info!(
            mcp_server = %self.name,
            "Attempting MCP server reconnection"
        );

        self.disconnect().await?;
        self.connect().await
    }

    /// Call an MCP tool with the given parameters.
    ///
    /// # Arguments
    ///
    /// * `name` - Tool name (e.g., "novanet_generate")
    /// * `params` - Tool parameters as JSON value
    ///
    /// # Errors
    ///
    /// Returns `NikaError::McpNotConnected` if not connected.
    /// Returns `NikaError::McpToolError` if the tool call fails.
    pub async fn call_tool(&self, name: &str, params: Value) -> Result<ToolCallResult> {
        let guard = self.service.lock().await;
        let service = guard.as_ref().ok_or_else(|| NikaError::McpNotConnected {
            name: self.name.clone(),
        })?;

        // Convert params to object format expected by rmcp
        let arguments = params.as_object().cloned();

        let request = CallToolRequestParams {
            meta: None,
            name: name.to_string().into(),
            arguments,
            task: None,
        };

        let result = service
            .call_tool(request)
            .await
            .map_err(|e| NikaError::McpToolError {
                tool: name.to_string(),
                reason: e.to_string(),
            })?;

        // Convert rmcp result to Nika's ToolCallResult
        let content: Vec<ContentBlock> = result
            .content
            .iter()
            .filter_map(|c| {
                // Extract text content
                c.as_text().map(|t| ContentBlock::text(t.text.clone()))
            })
            .collect();

        Ok(ToolCallResult {
            content,
            is_error: result.is_error.unwrap_or(false),
        })
    }

    /// Read a resource from the MCP server.
    ///
    /// # Arguments
    ///
    /// * `uri` - Resource URI (e.g., "neo4j://entity/qr-code")
    ///
    /// # Errors
    ///
    /// Returns `NikaError::McpNotConnected` if not connected.
    /// Returns `NikaError::McpResourceNotFound` if the resource doesn't exist.
    pub async fn read_resource(&self, uri: &str) -> Result<ResourceContent> {
        let guard = self.service.lock().await;
        let service = guard.as_ref().ok_or_else(|| NikaError::McpNotConnected {
            name: self.name.clone(),
        })?;

        let request = rmcp::model::ReadResourceRequestParams {
            meta: None,
            uri: uri.into(),
        };

        let result = service.read_resource(request).await.map_err(|e| {
            // Check for not found error
            let error_str = e.to_string().to_lowercase();
            if error_str.contains("not found") {
                NikaError::McpResourceNotFound {
                    uri: uri.to_string(),
                }
            } else {
                NikaError::McpToolError {
                    tool: "resources/read".to_string(),
                    reason: e.to_string(),
                }
            }
        })?;

        // Convert first resource content
        let resource = result
            .contents
            .first()
            .ok_or_else(|| NikaError::McpResourceNotFound {
                uri: uri.to_string(),
            })?;

        // Build ResourceContent from rmcp response
        // Serialize the resource content as JSON for simplicity
        let text = serde_json::to_string(resource).map_err(|e| NikaError::McpToolError {
            tool: "resources/read".to_string(),
            reason: format!("Failed to serialize resource: {}", e),
        })?;

        let content = ResourceContent::new(uri)
            .with_text(&text)
            .with_mime_type("application/json");

        Ok(content)
    }

    /// List all available tools from the MCP server.
    ///
    /// # Errors
    ///
    /// Returns `NikaError::McpNotConnected` if not connected.
    pub async fn list_tools(&self) -> Result<Vec<ToolDefinition>> {
        let guard = self.service.lock().await;
        let service = guard.as_ref().ok_or_else(|| NikaError::McpNotConnected {
            name: self.name.clone(),
        })?;

        let result: ListToolsResult =
            service
                .list_tools(Default::default())
                .await
                .map_err(|e| NikaError::McpToolError {
                    tool: "tools/list".to_string(),
                    reason: e.to_string(),
                })?;

        // Convert rmcp tools to Nika's ToolDefinition
        let tools = result
            .tools
            .into_iter()
            .map(|t| {
                let mut tool = ToolDefinition::new(t.name.as_ref());
                if let Some(desc) = &t.description {
                    tool = tool.with_description(desc.as_ref());
                }
                if let Some(schema) = t.input_schema.get("properties") {
                    tool = tool.with_input_schema(schema.clone());
                }
                tool
            })
            .collect();

        Ok(tools)
    }

    /// Get the server protocol version (if connected)
    pub fn server_version(&self) -> Option<String> {
        self.server_version.lock().clone()
    }
}

impl Drop for RmcpClientAdapter {
    fn drop(&mut self) {
        // Best-effort cleanup - rmcp handles process termination
        // The service will be dropped and cleaned up automatically
        tracing::debug!(
            mcp_server = %self.name,
            "RmcpClientAdapter dropped"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_new() {
        let config = McpConfig::new("test-server", "echo");
        let adapter = RmcpClientAdapter::new(config);
        assert_eq!(adapter.name(), "test-server");
    }

    #[tokio::test]
    async fn test_adapter_not_connected_by_default() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);
        assert!(!adapter.is_connected().await);
    }

    #[tokio::test]
    async fn test_call_tool_when_not_connected_returns_error() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);

        let result = adapter.call_tool("test_tool", serde_json::json!({})).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            NikaError::McpNotConnected { name } => assert_eq!(name, "test"),
            e => panic!("Expected McpNotConnected, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_read_resource_when_not_connected_returns_error() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);

        let result = adapter.read_resource("neo4j://entity/test").await;

        assert!(result.is_err());
        match result.unwrap_err() {
            NikaError::McpNotConnected { name } => assert_eq!(name, "test"),
            e => panic!("Expected McpNotConnected, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_list_tools_when_not_connected_returns_error() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);

        let result = adapter.list_tools().await;

        assert!(result.is_err());
        match result.unwrap_err() {
            NikaError::McpNotConnected { name } => assert_eq!(name, "test"),
            e => panic!("Expected McpNotConnected, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_disconnect_when_not_connected_is_ok() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);

        // Should not error
        let result = adapter.disconnect().await;
        assert!(result.is_ok());
    }
}
