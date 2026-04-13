// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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
//! ## Internal Usage
//!
//! This adapter is internal to the MCP module and should be accessed via `McpClient`.
//!
//! ```rust,ignore
//! // Users should use McpClient, not RmcpClientAdapter directly
//! use nika_mcp::{McpClient, McpConfig};
//!
//! let config = McpConfig::new("novanet", "cargo")
//!     .with_args(["run", "--manifest-path", "path/to/Cargo.toml"]);
//!
//! let client = McpClient::new(config)?;
//! client.connect().await?;
//! ```

use std::process::Stdio;

use parking_lot::Mutex;
use rmcp::model::{CallToolRequestParams, ListToolsResult};
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::TokioChildProcess;
use rmcp::{ServiceError, ServiceExt};
use serde_json::Value;
use tokio::process::Command;
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::timeout;

use crate::error::{McpError, Result};
use crate::{CONNECT_TIMEOUT, MCP_CALL_TIMEOUT, RECONNECT_TIMEOUT};
// McpRetryConfig and retry_mcp_call are used in mcp/client.rs, not here
#[allow(unused_imports)] // Keep import visible for IDE navigation
use crate::retry::{retry_mcp_call, McpRetryConfig};
use crate::types::{
    ContentBlock, McpConfig, McpErrorCode, ResourceContent, ToolCallResult, ToolDefinition,
};

/// Extract JSON-RPC error code from rmcp ServiceError.
///
/// Uses structured error extraction from rmcp's ServiceError type.
/// Falls back to regex-based extraction.
fn extract_mcp_error_code(error: &ServiceError) -> Option<McpErrorCode> {
    match error {
        ServiceError::McpError(mcp_error) => {
            // Direct structured access to error code from ErrorData
            Some(McpErrorCode::from_code(mcp_error.code.0))
        }
        ServiceError::Timeout { .. } => {
            // Timeout is not a JSON-RPC error, but we can map it
            None
        }
        ServiceError::TransportClosed | ServiceError::TransportSend(_) => {
            // Transport errors are not JSON-RPC errors
            None
        }
        ServiceError::Cancelled { .. } => {
            // Cancellation is not a JSON-RPC error
            None
        }
        ServiceError::UnexpectedResponse => {
            // Protocol error, not JSON-RPC
            None
        }
        // ServiceError is #[non_exhaustive], handle any new variants
        _ => None,
    }
}

/// Running rmcp service type alias
/// RunningService<Role, Handler> where Handler implements Service<Role>
type RmcpService = RunningService<RoleClient, ()>;

/// rmcp Client Adapter (internal)
///
/// Wraps rmcp's Service to provide Nika's MCP client interface.
/// Handles connection lifecycle, tool calls, and resource reads.
/// Users should access MCP via `McpClient`, not this type directly.
pub(crate) struct RmcpClientAdapter {
    /// Server name (from config)
    name: String,

    /// Server configuration
    config: McpConfig,

    /// Running rmcp service (None when disconnected)
    service: AsyncMutex<Option<RmcpService>>,

    /// Protocol version reported by server
    server_version: Mutex<Option<String>>,

    /// Cached tool definitions (populated after list_tools() call)
    /// Used by get_cached_tools() for synchronous access in rig integration
    cached_tools: Mutex<Vec<ToolDefinition>>,

    /// Timestamp of the last list_tools() call that populated the cache
    /// Used for TTL-based cache invalidation. None means cache was never populated.
    tools_fetched_at: Mutex<Option<std::time::Instant>>,
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
            cached_tools: Mutex::new(Vec::new()),
            tools_fetched_at: Mutex::new(None),
        }
    }

    /// Check if connected synchronously (non-blocking).
    ///
    /// Uses try_lock to avoid blocking. Returns false if:
    /// - The lock is held by another task
    /// - No service connection exists
    ///
    /// For accurate state, prefer `is_connected()` async method.
    pub fn is_connected_sync(&self) -> bool {
        // Try to check without blocking - return false if lock is held
        self.service
            .try_lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }

    /// Check if the client is connected to the server.
    pub async fn is_connected(&self) -> bool {
        let guard = match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.service.lock(),
        )
        .await
        {
            Ok(g) => g,
            Err(_) => return false,
        };
        guard.is_some()
    }

    /// Connect to the MCP server.
    ///
    /// Spawns the server process and establishes MCP communication.
    /// The rmcp SDK handles the initialize/initialized handshake.
    ///
    /// # Errors
    ///
    /// Returns `McpError::McpStartError` if the server fails to start.
    pub async fn connect(&self) -> Result<()> {
        let mut guard = match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.service.lock(),
        )
        .await
        {
            Ok(g) => g,
            Err(_) => {
                return Err(McpError::McpToolError {
                    tool: "service".to_string(),
                    reason: "MCP service lock timeout -- server may be unresponsive".to_string(),
                    error_code: None,
                })
            }
        };

        if guard.is_some() {
            return Ok(()); // Already connected
        }

        // Build command from config
        let mut cmd = Command::new(&self.config.command);
        cmd.args(&self.config.args);

        // Suppress stderr to avoid polluting TUI output
        // MCP communication happens over stdin/stdout, stderr is only for logging
        cmd.stderr(Stdio::null());

        // Suppress logging in child process to avoid TUI pollution
        // This must be set BEFORE adding config env vars to allow override if needed
        cmd.env("RUST_LOG", "off");

        // Validate environment variables for library injection (LD_PRELOAD etc.)
        let env_pairs: Vec<(String, String)> = self
            .config
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        validate_env_vars(&env_pairs)?;

        // Add environment variables from workflow config
        for (key, value) in &self.config.env {
            cmd.env(key, value);
        }

        // Set working directory if specified
        if let Some(ref cwd) = self.config.cwd {
            let cwd_path = std::path::Path::new(cwd);
            if cwd_path.exists() {
                cmd.current_dir(cwd_path);
                tracing::debug!(name = %self.name, cwd = %cwd, "MCP server cwd set");
            } else {
                tracing::warn!(name = %self.name, cwd = %cwd, "MCP server cwd does not exist, ignoring");
            }
        }

        // Create transport
        let transport = TokioChildProcess::new(cmd).map_err(|e| McpError::McpStartError {
            name: self.name.clone(),
            reason: format!("Failed to create transport: {}", e),
        })?;

        // Connect to server using rmcp's serve pattern
        // The () implements ClientHandler with default behavior
        // Wrap with timeout to prevent hanging on unresponsive MCP servers
        let service = timeout(CONNECT_TIMEOUT, ().serve(transport))
            .await
            .map_err(|_| McpError::McpTimeout {
                name: self.name.clone(),
                operation: "connect".to_string(),
                timeout_secs: CONNECT_TIMEOUT.as_secs(),
            })?
            .map_err(|e| McpError::McpStartError {
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
        let mut guard = match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.service.lock(),
        )
        .await
        {
            Ok(g) => g,
            Err(_) => {
                return Err(McpError::McpToolError {
                    tool: "service".to_string(),
                    reason: "MCP service lock timeout -- server may be unresponsive".to_string(),
                    error_code: None,
                })
            }
        };

        if let Some(service) = guard.take() {
            // Graceful shutdown
            if let Err(e) = service.cancel().await {
                tracing::warn!(error = %e, "MCP server graceful shutdown failed");
            }
        }

        *self.server_version.lock() = None;

        // Invalidate tool cache so reconnection forces a fresh list_tools()
        self.invalidate_tool_cache();

        Ok(())
    }

    /// Reconnect to the MCP server.
    ///
    /// Disconnects if connected, then establishes a new connection.
    /// The entire reconnection operation has a timeout of RECONNECT_TIMEOUT (30 seconds)
    /// to prevent indefinite hanging on unresponsive MCP servers.
    ///
    /// # Reconnection Safety
    ///
    /// Reconnection attempts are timeout-protected to prevent hangs if the MCP server
    /// became unresponsive during reconnection. This fix wraps the entire operation
    /// with a 30-second timeout.
    pub async fn reconnect(&self) -> Result<()> {
        tracing::info!(
            mcp_server = %self.name,
            timeout_secs = RECONNECT_TIMEOUT.as_secs(),
            "Attempting MCP server reconnection with timeout"
        );

        // Wrap entire reconnection with timeout to prevent indefinite hanging
        timeout(RECONNECT_TIMEOUT, async {
            self.disconnect().await?;
            self.connect().await
        })
        .await
        .map_err(|_| McpError::McpTimeout {
            name: self.name.clone(),
            operation: "reconnect".to_string(),
            timeout_secs: RECONNECT_TIMEOUT.as_secs(),
        })?
    }

    /// Call an MCP tool with the given parameters.
    ///
    /// # Arguments
    ///
    /// * `name` - Tool name (e.g., "novanet_context")
    /// * `params` - Tool parameters as JSON value
    ///
    /// # Errors
    ///
    /// Returns `McpError::McpNotConnected` if not connected.
    /// Returns `McpError::McpToolError` if the tool call fails.
    pub async fn call_tool(&self, name: &str, params: Value) -> Result<ToolCallResult> {
        // Clone the Peer and release the lock immediately to prevent lock contention
        // during the timeout period (60s). Without this, concurrent call_tool requests
        // would block waiting for the mutex while one request times out.
        let peer = {
            let guard =
                match tokio::time::timeout(std::time::Duration::from_secs(5), self.service.lock())
                    .await
                {
                    Ok(g) => g,
                    Err(_) => {
                        return Err(McpError::McpToolError {
                            tool: "service".to_string(),
                            reason: "MCP service lock timeout -- server may be unresponsive"
                                .to_string(),
                            error_code: None,
                        })
                    }
                };
            let service = guard.as_ref().ok_or_else(|| McpError::McpNotConnected {
                name: self.name.clone(),
            })?;
            // Clone the Peer (Peer implements Clone via derive)
            // RunningService derefs to Peer, and Peer is Clone
            use std::ops::Deref;
            service.deref().clone()
        }; // Lock is released here

        // Convert params to object format expected by rmcp
        // Reject non-object, non-null params with a clear error
        if !params.is_null() && !params.is_object() {
            return Err(McpError::McpToolError {
                tool: name.to_string(),
                reason: format!(
                    "Tool params must be a JSON object, got {}",
                    match &params {
                        serde_json::Value::Array(_) => "array",
                        serde_json::Value::String(_) => "string",
                        serde_json::Value::Number(_) => "number",
                        serde_json::Value::Bool(_) => "boolean",
                        _ => "unknown",
                    }
                ),
                error_code: None,
            });
        }
        let arguments = params.as_object().cloned();

        let request = CallToolRequestParams {
            meta: None,
            name: name.to_string().into(),
            arguments,
            task: None,
        };

        let result = timeout(MCP_CALL_TIMEOUT, peer.call_tool(request))
            .await
            .map_err(|_| McpError::McpTimeout {
                name: self.name.clone(),
                operation: "call_tool".to_string(),
                timeout_secs: MCP_CALL_TIMEOUT.as_secs(),
            })?
            .map_err(|e| {
                // Use structured error code extraction from ServiceError
                let error_code = extract_mcp_error_code(&e);
                McpError::McpToolError {
                    tool: name.to_string(),
                    reason: e.to_string(),
                    error_code,
                }
            })?;

        // Convert rmcp result to Nika's ToolCallResult (exhaustive 5-variant match)
        let content: Vec<ContentBlock> = result
            .content
            .iter()
            .map(|c| {
                use rmcp::model::RawContent;
                match &**c {
                    RawContent::Text(t) => ContentBlock::text(t.text.clone()),
                    RawContent::Image(i) => {
                        ContentBlock::image(i.data.clone(), i.mime_type.clone())
                    }
                    RawContent::Audio(a) => ContentBlock::Audio {
                        data: a.data.clone(),
                        mime_type: a.mime_type.clone(),
                    },
                    RawContent::Resource(r) => {
                        use rmcp::model::ResourceContents;
                        match &r.resource {
                            ResourceContents::TextResourceContents {
                                text,
                                uri,
                                mime_type,
                                ..
                            } => {
                                let mut rc = ResourceContent::new(uri.clone());
                                if let Some(mime) = mime_type {
                                    rc = rc.with_mime_type(mime.as_str());
                                }
                                rc = rc.with_text(text.clone());
                                ContentBlock::resource(rc)
                            }
                            ResourceContents::BlobResourceContents {
                                blob,
                                uri,
                                mime_type,
                                ..
                            } => {
                                let mut rc = ResourceContent::new(uri.clone());
                                if let Some(mime) = mime_type {
                                    rc = rc.with_mime_type(mime.as_str());
                                }
                                rc = rc.with_blob(blob.clone());
                                ContentBlock::resource(rc)
                            }
                        }
                    }
                    // [H2] RawResource.name is String, not Option<String> in rmcp 0.16
                    // Convert empty string to None so skip_serializing_if works correctly
                    RawContent::ResourceLink(l) => ContentBlock::ResourceLink {
                        uri: l.uri.clone(),
                        name: if l.name.is_empty() {
                            None
                        } else {
                            Some(l.name.clone())
                        },
                        mime_type: l.mime_type.clone(),
                    },
                }
            })
            .collect();

        // Layer 1 extraction verification
        let rmcp_count = result.content.len();
        let extracted_count = content.len();
        if extracted_count != rmcp_count {
            tracing::warn!(
                rmcp_count,
                extracted_count,
                "Content block count mismatch after extraction"
            );
        }

        Ok(ToolCallResult {
            content,
            is_error: result.is_error.unwrap_or(false),
            was_cached: false,
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
    /// Returns `McpError::McpNotConnected` if not connected.
    /// Returns `McpError::McpResourceNotFound` if the resource doesn't exist.
    pub async fn read_resource(&self, uri: &str) -> Result<ResourceContent> {
        // Clone the Peer and release the lock immediately to prevent lock contention
        let peer = {
            let guard =
                match tokio::time::timeout(std::time::Duration::from_secs(5), self.service.lock())
                    .await
                {
                    Ok(g) => g,
                    Err(_) => {
                        return Err(McpError::McpToolError {
                            tool: "service".to_string(),
                            reason: "MCP service lock timeout -- server may be unresponsive"
                                .to_string(),
                            error_code: None,
                        })
                    }
                };
            let service = guard.as_ref().ok_or_else(|| McpError::McpNotConnected {
                name: self.name.clone(),
            })?;
            use std::ops::Deref;
            service.deref().clone()
        }; // Lock is released here

        let request = rmcp::model::ReadResourceRequestParams {
            meta: None,
            uri: uri.into(),
        };

        let result = timeout(MCP_CALL_TIMEOUT, peer.read_resource(request))
            .await
            .map_err(|_| McpError::McpTimeout {
                name: self.name.clone(),
                operation: "read_resource".to_string(),
                timeout_secs: MCP_CALL_TIMEOUT.as_secs(),
            })?
            .map_err(|e| {
                // Use structured error code extraction
                let error_code = extract_mcp_error_code(&e);

                // Check for resource not found (either via error code or message)
                // MCP uses -32002 (RESOURCE_NOT_FOUND) which maps to ServerError(-32002)
                let is_not_found = matches!(error_code, Some(McpErrorCode::ServerError(-32002)))
                    || e.to_string().to_lowercase().contains("not found");

                if is_not_found {
                    McpError::McpResourceNotFound {
                        uri: uri.to_string(),
                    }
                } else {
                    McpError::McpToolError {
                        tool: "resources/read".to_string(),
                        reason: e.to_string(),
                        error_code,
                    }
                }
            })?;

        // Convert first resource content
        let resource = result
            .contents
            .first()
            .ok_or_else(|| McpError::McpResourceNotFound {
                uri: uri.to_string(),
            })?;

        // Build ResourceContent from rmcp response, preserving blob data
        use rmcp::model::ResourceContents;
        let content = match resource {
            ResourceContents::TextResourceContents {
                text, mime_type, ..
            } => {
                let mut rc = ResourceContent::new(uri);
                rc = rc.with_text(text.clone());
                if let Some(mime) = mime_type {
                    rc = rc.with_mime_type(mime.as_str());
                }
                rc
            }
            ResourceContents::BlobResourceContents {
                blob, mime_type, ..
            } => {
                let mut rc = ResourceContent::new(uri);
                rc = rc.with_blob(blob.clone());
                if let Some(mime) = mime_type {
                    rc = rc.with_mime_type(mime.as_str());
                }
                rc
            }
        };

        Ok(content)
    }

    /// List all available tools from the MCP server.
    ///
    /// # Errors
    ///
    /// Returns `McpError::McpNotConnected` if not connected.
    pub async fn list_tools(&self) -> Result<Vec<ToolDefinition>> {
        // Clone the Peer and release the lock immediately to prevent lock contention
        let peer = {
            let guard =
                match tokio::time::timeout(std::time::Duration::from_secs(5), self.service.lock())
                    .await
                {
                    Ok(g) => g,
                    Err(_) => {
                        return Err(McpError::McpToolError {
                            tool: "service".to_string(),
                            reason: "MCP service lock timeout -- server may be unresponsive"
                                .to_string(),
                            error_code: None,
                        })
                    }
                };
            let service = guard.as_ref().ok_or_else(|| McpError::McpNotConnected {
                name: self.name.clone(),
            })?;
            use std::ops::Deref;
            service.deref().clone()
        }; // Lock is released here

        let result: ListToolsResult =
            timeout(MCP_CALL_TIMEOUT, peer.list_tools(Default::default()))
                .await
                .map_err(|_| McpError::McpTimeout {
                    name: self.name.clone(),
                    operation: "list_tools".to_string(),
                    timeout_secs: MCP_CALL_TIMEOUT.as_secs(),
                })?
                .map_err(|e| {
                    // Use structured error code extraction
                    let error_code = extract_mcp_error_code(&e);
                    McpError::McpToolError {
                        tool: "tools/list".to_string(),
                        reason: e.to_string(),
                        error_code,
                    }
                })?;

        // Convert rmcp tools to Nika's ToolDefinition
        let tools: Vec<ToolDefinition> = result
            .tools
            .into_iter()
            .map(|t| {
                let mut tool = ToolDefinition::new(t.name.as_ref());
                if let Some(desc) = &t.description {
                    tool = tool.with_description(desc.as_ref());
                }
                // Convert Arc<Map> to Value and ensure "type": "object" for Claude API
                // Claude requires the root schema to have a "type" field
                let mut schema_map: serde_json::Map<String, serde_json::Value> =
                    (*t.input_schema).clone();
                // Ensure "type": "object" is present (required by Claude API)
                if !schema_map.contains_key("type") {
                    schema_map.insert("type".to_string(), serde_json::json!("object"));
                }
                tool = tool.with_input_schema(serde_json::Value::Object(schema_map));
                tool
            })
            .collect();

        // Cache tools for synchronous access via get_cached_tools()
        *self.cached_tools.lock() = tools.clone();
        *self.tools_fetched_at.lock() = Some(std::time::Instant::now());

        Ok(tools)
    }

    /// Get cached tool definitions (synchronous access).
    ///
    /// Returns tools from the last `list_tools()` call, or empty vec if never called.
    /// This is used by rig integration which requires sync access to tool definitions.
    pub fn get_cached_tools(&self) -> Vec<ToolDefinition> {
        self.cached_tools.lock().clone()
    }

    /// Check if the tool cache is still fresh within the given TTL.
    ///
    /// Returns `true` if tools were fetched within `ttl` duration, `false` if
    /// the cache is stale or was never populated.
    pub fn is_tool_cache_fresh(&self, ttl: std::time::Duration) -> bool {
        self.tools_fetched_at
            .lock()
            .map(|fetched_at| fetched_at.elapsed() < ttl)
            .unwrap_or(false)
    }

    /// Invalidate the tool cache, forcing re-fetch on next `list_tools()` call.
    pub fn invalidate_tool_cache(&self) {
        self.cached_tools.lock().clear();
        *self.tools_fetched_at.lock() = None;
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

    // ═══════════════════════════════════════════════════════════════════════════
    // RmcpAdapter Construction Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_adapter_new() {
        let config = McpConfig::new("test-server", "echo");
        let adapter = RmcpClientAdapter::new(config);
        assert_eq!(adapter.name.as_str(), "test-server");
    }

    #[test]
    fn test_adapter_new_with_args_and_env() {
        let config = McpConfig::new("novanet", "cargo")
            .with_arg("run")
            .with_env("NEO4J_URI", "bolt://localhost:7687");

        let adapter = RmcpClientAdapter::new(config);
        assert_eq!(adapter.name.as_str(), "novanet");
    }

    #[test]
    fn test_adapter_debug_not_connected() {
        let config = McpConfig::new("test-server", "echo");
        let adapter = RmcpClientAdapter::new(config);

        let debug_str = format!("{:?}", adapter);
        assert!(debug_str.contains("RmcpClientAdapter"));
        assert!(debug_str.contains("test-server"));
        assert!(debug_str.contains("connected"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Connection State Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_adapter_not_connected_by_default() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);
        assert!(!adapter.is_connected().await);
    }

    #[test]
    fn test_adapter_not_connected_sync() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);
        assert!(!adapter.is_connected_sync());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Disconnect Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_disconnect_when_not_connected_is_ok() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);

        let result = adapter.disconnect().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_disconnect_clears_server_version() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);

        // Manually set server version to test clearing
        *adapter.server_version.lock() = Some("1.0".to_string());
        assert_eq!(
            adapter.server_version.lock().as_ref().map(|s| s.as_str()),
            Some("1.0")
        );

        adapter.disconnect().await.ok();
        assert_eq!(adapter.server_version.lock().as_ref(), None);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Tool Call Error Tests (when not connected)
    // ═══════════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_call_tool_when_not_connected_returns_error() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);

        let result = adapter.call_tool("test_tool", serde_json::json!({})).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            McpError::McpNotConnected { name } => assert_eq!(name, "test"),
            e => panic!("Expected McpNotConnected, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_call_tool_with_object_params() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);

        let params = serde_json::json!({
            "entity": "qr-code",
            "locale": "fr-FR"
        });

        let result = adapter.call_tool("novanet_context", params).await;

        // Should fail with McpNotConnected, not with param conversion error
        assert!(result.is_err());
        match result.unwrap_err() {
            McpError::McpNotConnected { name } => assert_eq!(name, "test"),
            e => panic!("Expected McpNotConnected, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_call_tool_with_non_object_params() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);

        // Pass a string instead of object
        let params = serde_json::json!("not-an-object");

        let result = adapter.call_tool("test_tool", params).await;

        // Should still error with McpNotConnected (params are converted to None)
        assert!(result.is_err());
        match result.unwrap_err() {
            McpError::McpNotConnected { name } => assert_eq!(name, "test"),
            e => panic!("Expected McpNotConnected, got: {:?}", e),
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Resource Read Error Tests (when not connected)
    // ═══════════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_read_resource_when_not_connected_returns_error() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);

        let result = adapter.read_resource("neo4j://entity/test").await;

        assert!(result.is_err());
        match result.unwrap_err() {
            McpError::McpNotConnected { name } => assert_eq!(name, "test"),
            e => panic!("Expected McpNotConnected, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_read_resource_with_various_uris() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);

        let uris = vec![
            "neo4j://entity/qr-code",
            "neo4j://page/landing",
            "neo4j://block/hero",
            "file:///path/to/file",
        ];

        for uri in uris {
            let result = adapter.read_resource(uri).await;
            assert!(result.is_err());
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // List Tools Error Tests (when not connected)
    // ═══════════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_list_tools_when_not_connected_returns_error() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);

        let result = adapter.list_tools().await;

        assert!(result.is_err());
        match result.unwrap_err() {
            McpError::McpNotConnected { name } => assert_eq!(name, "test"),
            e => panic!("Expected McpNotConnected, got: {:?}", e),
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Cached Tools Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_get_cached_tools_returns_empty_initially() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);

        let cached = adapter.get_cached_tools();
        assert!(cached.is_empty());
    }

    #[test]
    fn test_get_cached_tools_with_populated_cache() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);

        // Manually populate cache
        let tools = vec![
            ToolDefinition::new("novanet_context"),
            ToolDefinition::new("novanet_search"),
        ];
        *adapter.cached_tools.lock() = tools.clone();

        let cached = adapter.get_cached_tools();
        assert_eq!(cached.len(), 2);
        assert_eq!(cached[0].name, "novanet_context");
        assert_eq!(cached[1].name, "novanet_search");
    }

    #[test]
    fn test_cached_tools_independence() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);

        let tool1 = ToolDefinition::new("tool1");
        *adapter.cached_tools.lock() = vec![tool1];

        let cached1 = adapter.get_cached_tools();
        assert_eq!(cached1.len(), 1);

        // Modify cache again
        let tool2 = ToolDefinition::new("tool2");
        *adapter.cached_tools.lock() = vec![tool2];

        let cached2 = adapter.get_cached_tools();
        assert_eq!(cached2.len(), 1);
        assert_eq!(cached2[0].name, "tool2");
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Tool Cache TTL Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_tool_cache_not_fresh_initially() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);

        assert!(!adapter.is_tool_cache_fresh(std::time::Duration::from_secs(300)));
    }

    #[test]
    fn test_tool_cache_fresh_after_populate() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);

        // Simulate list_tools() populating the cache
        *adapter.cached_tools.lock() = vec![ToolDefinition::new("tool1")];
        *adapter.tools_fetched_at.lock() = Some(std::time::Instant::now());

        assert!(adapter.is_tool_cache_fresh(std::time::Duration::from_secs(300)));
    }

    #[test]
    fn test_tool_cache_stale_after_ttl() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);

        // Simulate a cache populated in the past (beyond TTL)
        *adapter.cached_tools.lock() = vec![ToolDefinition::new("tool1")];
        *adapter.tools_fetched_at.lock() =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(600));

        // Stale with 5-min TTL
        assert!(!adapter.is_tool_cache_fresh(std::time::Duration::from_secs(300)));
        // But fresh with 15-min TTL
        assert!(adapter.is_tool_cache_fresh(std::time::Duration::from_secs(900)));
    }

    #[test]
    fn test_invalidate_tool_cache() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);

        // Populate
        *adapter.cached_tools.lock() = vec![ToolDefinition::new("tool1")];
        *adapter.tools_fetched_at.lock() = Some(std::time::Instant::now());
        assert!(adapter.is_tool_cache_fresh(std::time::Duration::from_secs(300)));
        assert_eq!(adapter.get_cached_tools().len(), 1);

        // Invalidate
        adapter.invalidate_tool_cache();
        assert!(!adapter.is_tool_cache_fresh(std::time::Duration::from_secs(300)));
        assert!(adapter.get_cached_tools().is_empty());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Drop Implementation Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_adapter_drop_does_not_panic() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);

        // This should not panic
        drop(adapter);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Structured Error Code Extraction Tests
    // ═══════════════════════════════════════════════════════════════════════════

    use rmcp::model::ErrorCode;

    /// Helper to create an McpError (ErrorData) for testing
    fn make_mcp_error(code: i32, message: &str) -> ServiceError {
        ServiceError::McpError(rmcp::ErrorData::new(
            ErrorCode(code),
            message.to_string(),
            None,
        ))
    }

    #[test]
    fn test_extract_mcp_error_code_invalid_params() {
        let error = make_mcp_error(-32602, "Invalid params");
        let code = extract_mcp_error_code(&error);
        assert_eq!(code, Some(McpErrorCode::InvalidParams));
    }

    #[test]
    fn test_extract_mcp_error_code_method_not_found() {
        let error = make_mcp_error(-32601, "Method not found");
        let code = extract_mcp_error_code(&error);
        assert_eq!(code, Some(McpErrorCode::MethodNotFound));
    }

    #[test]
    fn test_extract_mcp_error_code_parse_error() {
        let error = make_mcp_error(-32700, "Parse error");
        let code = extract_mcp_error_code(&error);
        assert_eq!(code, Some(McpErrorCode::ParseError));
    }

    #[test]
    fn test_extract_mcp_error_code_invalid_request() {
        let error = make_mcp_error(-32600, "Invalid request");
        let code = extract_mcp_error_code(&error);
        assert_eq!(code, Some(McpErrorCode::InvalidRequest));
    }

    #[test]
    fn test_extract_mcp_error_code_internal_error() {
        let error = make_mcp_error(-32603, "Internal error");
        let code = extract_mcp_error_code(&error);
        assert_eq!(code, Some(McpErrorCode::InternalError));
    }

    #[test]
    fn test_extract_mcp_error_code_server_error() {
        let error = make_mcp_error(-32050, "Server error");
        let code = extract_mcp_error_code(&error);
        assert!(matches!(code, Some(McpErrorCode::ServerError(-32050))));
    }

    #[test]
    fn test_extract_mcp_error_code_resource_not_found() {
        let error = make_mcp_error(-32002, "Resource not found");
        let code = extract_mcp_error_code(&error);
        // -32002 is in the ServerError range (-32099..=-32000)
        // MCP uses this for RESOURCE_NOT_FOUND
        assert_eq!(code, Some(McpErrorCode::ServerError(-32002)));
    }

    #[test]
    fn test_extract_mcp_error_code_various_server_errors() {
        let test_cases = vec![
            (-32000, McpErrorCode::ServerError(-32000)),
            (-32050, McpErrorCode::ServerError(-32050)),
            (-32099, McpErrorCode::ServerError(-32099)),
        ];

        for (error_code, expected) in test_cases {
            let error = make_mcp_error(error_code, "Test error");
            let code = extract_mcp_error_code(&error);
            assert_eq!(code, Some(expected), "Failed for code: {}", error_code);
        }
    }

    #[test]
    fn test_extract_mcp_error_code_timeout_returns_none() {
        let error = ServiceError::Timeout {
            timeout: std::time::Duration::from_secs(30),
        };
        let code = extract_mcp_error_code(&error);
        assert_eq!(code, None);
    }

    #[test]
    fn test_extract_mcp_error_code_transport_closed_returns_none() {
        let error = ServiceError::TransportClosed;
        let code = extract_mcp_error_code(&error);
        assert_eq!(code, None);
    }

    #[test]
    fn test_extract_mcp_error_code_cancelled_returns_none() {
        let error = ServiceError::Cancelled {
            reason: Some("User cancelled".to_string()),
        };
        let code = extract_mcp_error_code(&error);
        assert_eq!(code, None);
    }

    #[test]
    fn test_extract_mcp_error_code_unexpected_response_returns_none() {
        let error = ServiceError::UnexpectedResponse;
        let code = extract_mcp_error_code(&error);
        assert_eq!(code, None);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Integration Tests (Configuration & Conversion)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_adapter_preserves_config_command() {
        let config = McpConfig::new("myserver", "python").with_args(["script.py", "--flag"]);

        let adapter = RmcpClientAdapter::new(config);
        assert_eq!(adapter.name.as_str(), "myserver");
    }

    #[test]
    fn test_adapter_with_complex_config() {
        let config = McpConfig::new("complex-server", "node")
            .with_arg("--require")
            .with_arg("dotenv/config")
            .with_arg("index.js")
            .with_env("LOG_LEVEL", "debug")
            .with_env("PORT", "3000");

        let adapter = RmcpClientAdapter::new(config);
        assert_eq!(adapter.name.as_str(), "complex-server");
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Reconnection Timeout Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_reconnect_timeout_constant_is_30_seconds() {
        // Reconnection has explicit 30-second timeout
        assert_eq!(RECONNECT_TIMEOUT.as_secs(), 30);
    }

    #[tokio::test]
    async fn test_reconnect_when_not_connected() {
        // Reconnect on disconnected adapter should attempt connect
        let config = McpConfig::new("test", "nonexistent_command_xyz");
        let adapter = RmcpClientAdapter::new(config);

        // Reconnect should fail (command doesn't exist), but with proper error
        let result = adapter.reconnect().await;
        assert!(result.is_err());

        // Should not be connected after failed reconnect
        assert!(!adapter.is_connected().await);
    }

    #[test]
    fn test_reconnect_timeout_exceeds_connect_timeout() {
        // Reconnect timeout should be >= connect timeout
        // since reconnect involves disconnect + connect
        assert!(RECONNECT_TIMEOUT >= CONNECT_TIMEOUT);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // NIKA-104: Cache Invalidation on Disconnect
    // ═══════════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_disconnect_invalidates_tool_cache() {
        let config = McpConfig::new("test", "echo");
        let adapter = RmcpClientAdapter::new(config);

        // Manually populate the tool cache
        adapter
            .cached_tools
            .lock()
            .push(ToolDefinition::new("fake_tool").with_description("test"));
        *adapter.tools_fetched_at.lock() = Some(std::time::Instant::now());

        assert!(!adapter.get_cached_tools().is_empty());
        assert!(adapter.is_tool_cache_fresh(std::time::Duration::from_secs(60)));

        // Disconnect should clear the cache
        let _ = adapter.disconnect().await;

        assert!(adapter.get_cached_tools().is_empty());
        assert!(!adapter.is_tool_cache_fresh(std::time::Duration::from_secs(60)));
    }
}

// ── Security: env var validation (inlined from runtime/security.rs) ──────

#[allow(clippy::items_after_test_module)]
const BLOCKED_ENV_VARS: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "SHELL",
    "LANG",
    "TERM",
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "DYLD_INSERT_LIBRARIES",
];

#[allow(clippy::items_after_test_module)]
fn is_valid_env_var_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[allow(clippy::items_after_test_module)]
fn validate_env_vars(vars: &[(String, String)]) -> Result<()> {
    for (key, _) in vars {
        if !is_valid_env_var_name(key) {
            return Err(McpError::ConfigError {
                reason: format!(
                    "Invalid environment variable name '{}': must match [A-Za-z_][A-Za-z0-9_]*",
                    key
                ),
            });
        }
        let upper = key.to_uppercase();
        for blocked in BLOCKED_ENV_VARS {
            if upper == *blocked {
                return Err(McpError::ConfigError {
                    reason: format!("Environment variable '{}' is blocked for security", key),
                });
            }
        }
    }
    Ok(())
}
