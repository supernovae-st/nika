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

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use parking_lot::Mutex;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Child;
use tokio::sync::Mutex as AsyncMutex;

use crate::error::{NikaError, Result};
use crate::mcp::protocol::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use crate::mcp::transport::McpTransport;
use crate::mcp::types::{ContentBlock, McpConfig, ResourceContent, ToolCallResult, ToolDefinition};

/// MCP Client for connecting to and interacting with MCP servers.
///
/// The client can operate in two modes:
/// - **Real mode**: Spawns an MCP server process and communicates via stdio
/// - **Mock mode**: Returns canned responses for testing
pub struct McpClient {
    /// Server name (from config or mock)
    name: String,

    /// Server configuration (None for mock clients)
    config: Option<McpConfig>,

    /// Connection state (atomic for interior mutability)
    connected: AtomicBool,

    /// Whether this is a mock client
    is_mock: bool,

    /// Child process for real MCP connection
    process: Mutex<Option<Child>>,

    /// Request ID counter for JSON-RPC
    request_id: AtomicU64,

    /// Async mutex to serialize request-response cycles
    /// Required because stdio is shared and concurrent access races
    io_lock: AsyncMutex<()>,
}

// Manual Debug impl since Child doesn't implement Debug well
impl std::fmt::Debug for McpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClient")
            .field("name", &self.name)
            .field("config", &self.config)
            .field("connected", &self.connected)
            .field("is_mock", &self.is_mock)
            .field("process", &self.process.lock().is_some())
            .field("request_id", &self.request_id)
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

        Ok(Self {
            name: config.name.clone(),
            config: Some(config),
            connected: AtomicBool::new(false),
            is_mock: false,
            process: Mutex::new(None),
            request_id: AtomicU64::new(1),
            io_lock: AsyncMutex::new(()),
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
            process: Mutex::new(None),
            request_id: AtomicU64::new(1),
            io_lock: AsyncMutex::new(()),
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

        let config = self
            .config
            .as_ref()
            .ok_or_else(|| NikaError::McpNotConnected {
                name: self.name.clone(),
            })?;

        // Create transport from config
        let args: Vec<&str> = config.args.iter().map(|s| s.as_str()).collect();
        let mut transport = McpTransport::new(&config.command, &args);

        // Add env vars
        for (k, v) in &config.env {
            transport = transport.with_env(k, v);
        }

        // Spawn process
        let child = transport.spawn().await?;
        *self.process.lock() = Some(child);

        // Initialize MCP connection
        self.initialize().await?;

        self.connected.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Initialize MCP connection with handshake.
    ///
    /// MCP protocol requires:
    /// 1. Client sends `initialize` request
    /// 2. Server responds with capabilities
    /// 3. Client sends `notifications/initialized` notification
    /// 4. Now tool calls can be made
    async fn initialize(&self) -> Result<()> {
        // Step 1: Send initialize request
        let req = JsonRpcRequest::new(
            self.next_id(),
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "nika", "version": env!("CARGO_PKG_VERSION") }
            }),
        );

        let response = self.send_request(&req).await?;
        if !response.is_success() {
            return Err(NikaError::McpStartError {
                name: self.name.clone(),
                reason: response
                    .error
                    .map(|e| e.message)
                    .unwrap_or_else(|| "Unknown error".to_string()),
            });
        }

        // Step 2: Send initialized notification (required by MCP protocol)
        self.send_notification(&JsonRpcNotification::new("notifications/initialized"))
            .await?;

        Ok(())
    }

    /// Send a JSON-RPC notification (no response expected).
    async fn send_notification(&self, notification: &JsonRpcNotification) -> Result<()> {
        // Serialize notification
        let json =
            serde_json::to_string(notification).map_err(|e| NikaError::McpToolError {
                tool: notification.method.clone(),
                reason: format!("Failed to serialize notification: {}", e),
            })?;

        // Take stdin out of the process to avoid holding lock across await
        let mut stdin = {
            let mut guard = self.process.lock();
            let process = guard.as_mut().ok_or_else(|| NikaError::McpNotConnected {
                name: self.name.clone(),
            })?;

            process
                .stdin
                .take()
                .ok_or_else(|| NikaError::McpToolError {
                    tool: notification.method.clone(),
                    reason: "stdin not available".to_string(),
                })?
        };

        // Write notification (without holding the lock)
        stdin
            .write_all(json.as_bytes())
            .await
            .map_err(|e| NikaError::McpToolError {
                tool: notification.method.clone(),
                reason: format!("Failed to write: {}", e),
            })?;
        stdin
            .write_all(b"\n")
            .await
            .map_err(|e| NikaError::McpToolError {
                tool: notification.method.clone(),
                reason: format!("Failed to write newline: {}", e),
            })?;
        stdin.flush().await.map_err(|e| NikaError::McpToolError {
            tool: notification.method.clone(),
            reason: format!("Failed to flush: {}", e),
        })?;

        // Put stdin back
        {
            let mut guard = self.process.lock();
            if let Some(process) = guard.as_mut() {
                process.stdin = Some(stdin);
            }
        }

        Ok(())
    }

    /// Get next request ID.
    fn next_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Send a JSON-RPC request and read the response.
    ///
    /// Uses io_lock to serialize concurrent requests, preventing race conditions
    /// when multiple for_each iterations access the same MCP client.
    async fn send_request(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse> {
        // Serialize concurrent requests - only one request-response cycle at a time
        let _io_guard = self.io_lock.lock().await;

        // Serialize request
        let json = serde_json::to_string(request).map_err(|e| NikaError::McpToolError {
            tool: request.method.clone(),
            reason: format!("Failed to serialize request: {}", e),
        })?;

        // Take stdin out of the process to avoid holding lock across await
        let mut stdin = {
            let mut guard = self.process.lock();
            let process = guard.as_mut().ok_or_else(|| NikaError::McpNotConnected {
                name: self.name.clone(),
            })?;

            process
                .stdin
                .take()
                .ok_or_else(|| NikaError::McpToolError {
                    tool: request.method.clone(),
                    reason: "stdin not available".to_string(),
                })?
        };

        // Write request (without holding the lock)
        stdin
            .write_all(json.as_bytes())
            .await
            .map_err(|e| NikaError::McpToolError {
                tool: request.method.clone(),
                reason: format!("Failed to write: {}", e),
            })?;
        stdin
            .write_all(b"\n")
            .await
            .map_err(|e| NikaError::McpToolError {
                tool: request.method.clone(),
                reason: format!("Failed to write newline: {}", e),
            })?;
        stdin.flush().await.map_err(|e| NikaError::McpToolError {
            tool: request.method.clone(),
            reason: format!("Failed to flush: {}", e),
        })?;

        // Put stdin back
        {
            let mut guard = self.process.lock();
            if let Some(process) = guard.as_mut() {
                process.stdin = Some(stdin);
            }
        }

        // Read response (still under io_lock to ensure request-response pairing)
        self.read_response(&request.method).await
    }

    /// Read a JSON-RPC response from stdout.
    async fn read_response(&self, method: &str) -> Result<JsonRpcResponse> {
        // Take stdout out of the process to avoid holding lock across await
        let stdout = {
            let mut guard = self.process.lock();
            let process = guard.as_mut().ok_or_else(|| NikaError::McpNotConnected {
                name: self.name.clone(),
            })?;

            process
                .stdout
                .take()
                .ok_or_else(|| NikaError::McpToolError {
                    tool: method.to_string(),
                    reason: "stdout not available".to_string(),
                })?
        };

        // Read response (without holding the lock)
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .await
            .map_err(|e| NikaError::McpToolError {
                tool: method.to_string(),
                reason: format!("Failed to read response: {}", e),
            })?;

        // Put stdout back
        {
            let mut guard = self.process.lock();
            if let Some(process) = guard.as_mut() {
                process.stdout = Some(reader.into_inner());
            }
        }

        serde_json::from_str(&line).map_err(|e| NikaError::McpToolError {
            tool: method.to_string(),
            reason: format!("Invalid JSON response: {} (line: {})", e, line.trim()),
        })
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

        // Kill process if real mode
        if !self.is_mock {
            // Take the child out of the mutex to avoid holding the lock across await
            let child = self.process.lock().take();
            if let Some(mut child) = child {
                let _ = child.kill().await;
            }
        }

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

        // Real mode: send tools/call request
        let request = JsonRpcRequest::new(
            self.next_id(),
            "tools/call",
            serde_json::json!({
                "name": name,
                "arguments": params
            }),
        );

        let response = self.send_request(&request).await?;

        if let Some(error) = response.error {
            return Err(NikaError::McpToolError {
                tool: name.to_string(),
                reason: error.message,
            });
        }

        // Parse MCP tool result from response.result
        let result = response.result.ok_or_else(|| NikaError::McpToolError {
            tool: name.to_string(),
            reason: "Empty result".to_string(),
        })?;

        // Convert to ToolCallResult
        Self::parse_tool_result(name, result)
    }

    /// Parse an MCP tool result from JSON response.
    fn parse_tool_result(_tool_name: &str, result: Value) -> Result<ToolCallResult> {
        // MCP returns: { "content": [{ "type": "text", "text": "..." }], "isError": false }
        let content = result
            .get("content")
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let text = item.get("text")?.as_str()?;
                        Some(ContentBlock::text(text.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let is_error = result
            .get("isError")
            .and_then(|e| e.as_bool())
            .unwrap_or(false);

        Ok(ToolCallResult { content, is_error })
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

        // Send tools/list request
        let request = JsonRpcRequest::new(self.next_id(), "tools/list", serde_json::json!({}));

        let response = self.send_request(&request).await?;

        if let Some(error) = response.error {
            return Err(NikaError::McpToolError {
                tool: "tools/list".to_string(),
                reason: error.message,
            });
        }

        // Parse tools from response
        let result = response.result.ok_or_else(|| NikaError::McpToolError {
            tool: "tools/list".to_string(),
            reason: "Empty result".to_string(),
        })?;

        // MCP returns: { "tools": [{ "name": "...", "description": "...", "inputSchema": {...} }] }
        let tools = result
            .get("tools")
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let name = item.get("name")?.as_str()?;
                        let description = item.get("description").and_then(|d| d.as_str());
                        let input_schema = item.get("inputSchema").cloned();

                        let mut tool = ToolDefinition::new(name);
                        if let Some(desc) = description {
                            tool = tool.with_description(desc);
                        }
                        if let Some(schema) = input_schema {
                            tool = tool.with_input_schema(schema);
                        }
                        Some(tool)
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(tools)
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
        let result = client
            .call_tool("unknown_tool", serde_json::json!({}))
            .await;
        assert!(result.is_ok());
        assert!(!result.unwrap().is_error);
    }
}
