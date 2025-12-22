//! # MCP Client Module (v4.7.1)
//!
//! Handles connections to MCP (Model Context Protocol) servers.
//!
//! ## Reference Format
//!
//! ```yaml
//! mcp: server::tool
//! ```
//!
//! Where:
//! - `server` = Server name (key in mcp config)
//! - `tool` = Tool name to call on that server
//!
//! ## Server Configuration
//!
//! MCP servers are configured in the workflow:
//!
//! ```yaml
//! mcp:
//!   filesystem:
//!     command: npx
//!     args: ["-y", "@modelcontextprotocol/server-filesystem", "/allowed/path"]
//!   git:
//!     command: uvx
//!     args: ["mcp-server-git"]
//! ```

use anyhow::{anyhow, bail, Context, Result};
use rmcp::{
    model::CallToolRequestParam,
    service::ServiceExt,
    transport::{ConfigureCommandExt, TokioChildProcess},
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::RwLock;

// Import MCP server config from workflow module
pub use crate::workflow::McpServerConfig;

/// Parsed MCP reference (server::tool)
#[derive(Debug, Clone)]
pub struct McpReference {
    pub server: String,
    pub tool: String,
}

impl McpReference {
    /// Parse a reference string in the format "server::tool"
    pub fn parse(reference: &str) -> Result<Self> {
        let parts: Vec<&str> = reference.split("::").collect();
        if parts.len() != 2 {
            bail!(
                "Invalid MCP reference '{}': expected 'server::tool' format",
                reference
            );
        }

        let server = parts[0].trim();
        let tool = parts[1].trim();

        if server.is_empty() {
            bail!(
                "Invalid MCP reference '{}': server name is empty",
                reference
            );
        }
        if tool.is_empty() {
            bail!("Invalid MCP reference '{}': tool name is empty", reference);
        }

        Ok(Self {
            server: server.to_string(),
            tool: tool.to_string(),
        })
    }
}

/// MCP Client - manages connections to MCP servers
pub struct McpClient {
    /// Server configurations (server name -> config)
    servers: HashMap<String, McpServerConfig>,
    /// Cache of running server connections
    /// Key: server name, Value: running service handle
    #[allow(clippy::type_complexity)]
    connections:
        Arc<RwLock<HashMap<String, Arc<rmcp::service::RunningService<rmcp::RoleClient, ()>>>>>,
}

impl McpClient {
    /// Create a new MCP client with the given server configurations
    pub fn new(servers: HashMap<String, McpServerConfig>) -> Self {
        Self {
            servers,
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create an MCP client with no configured servers
    pub fn empty() -> Self {
        Self::new(HashMap::new())
    }

    /// Get the list of configured server names
    pub fn server_names(&self) -> Vec<&str> {
        self.servers.keys().map(|s| s.as_str()).collect()
    }

    /// Check if a server is configured
    pub fn has_server(&self, name: &str) -> bool {
        self.servers.contains_key(name)
    }

    /// Call a tool on an MCP server
    ///
    /// # Arguments
    ///
    /// * `reference` - The server::tool reference
    /// * `args` - Optional JSON arguments for the tool
    ///
    /// # Returns
    ///
    /// The tool result as a JSON value
    pub async fn call_tool(
        &self,
        reference: &str,
        args: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let mcp_ref = McpReference::parse(reference)?;

        // Get or create connection to the server
        let service = self.get_or_create_connection(&mcp_ref.server).await?;

        // Convert args to the expected format
        let arguments = args.map(|v| {
            if let serde_json::Value::Object(map) = v {
                map
            } else {
                let mut m = serde_json::Map::new();
                m.insert("value".to_string(), v);
                m
            }
        });

        // Call the tool
        let result = service
            .call_tool(CallToolRequestParam {
                name: mcp_ref.tool.clone().into(),
                arguments,
            })
            .await
            .with_context(|| format!("Failed to call MCP tool '{}'", reference))?;

        // Serialize the result directly to JSON
        // The rmcp Content type is complex, so we serialize it as-is
        let result_json = serde_json::to_value(&result)?;

        Ok(serde_json::json!({
            "isError": result.is_error.unwrap_or(false),
            "content": result_json.get("content").cloned().unwrap_or(serde_json::Value::Array(vec![]))
        }))
    }

    /// Get or create a connection to an MCP server
    async fn get_or_create_connection(
        &self,
        server_name: &str,
    ) -> Result<Arc<rmcp::service::RunningService<rmcp::RoleClient, ()>>> {
        // Check if we already have a connection
        {
            let connections = self.connections.read().await;
            if let Some(service) = connections.get(server_name) {
                return Ok(Arc::clone(service));
            }
        }

        // Get server config
        let server_config = self
            .servers
            .get(server_name)
            .ok_or_else(|| anyhow!("MCP server '{}' not configured", server_name))?;

        // Create the transport
        let cmd = Command::new(&server_config.command);

        // Configure args and env
        let args = server_config.args.clone();
        let env = server_config.env.clone();

        let transport = TokioChildProcess::new(cmd.configure(move |c| {
            for arg in &args {
                c.arg(arg);
            }
            for (key, value) in &env {
                c.env(key, value);
            }
        }))
        .with_context(|| format!("Failed to start MCP server '{}'", server_name))?;

        // Connect to the server
        let service = ()
            .serve(transport)
            .await
            .with_context(|| format!("Failed to initialize MCP server '{}'", server_name))?;

        let service = Arc::new(service);

        // Cache the connection
        {
            let mut connections = self.connections.write().await;
            connections.insert(server_name.to_string(), Arc::clone(&service));
        }

        Ok(service)
    }

    /// Close all connections by clearing the cache
    /// Note: The child processes will be cleaned up when dropped
    pub async fn close_all(&self) -> Result<()> {
        let mut connections = self.connections.write().await;
        // Clear all connections - they'll be dropped and child processes cleaned up
        connections.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mcp_reference() {
        let ref1 = McpReference::parse("filesystem::read_file").unwrap();
        assert_eq!(ref1.server, "filesystem");
        assert_eq!(ref1.tool, "read_file");

        let ref2 = McpReference::parse("git::status").unwrap();
        assert_eq!(ref2.server, "git");
        assert_eq!(ref2.tool, "status");
    }

    #[test]
    fn test_parse_mcp_reference_invalid() {
        // Missing ::
        assert!(McpReference::parse("filesystem").is_err());

        // Empty server
        assert!(McpReference::parse("::tool").is_err());

        // Empty tool
        assert!(McpReference::parse("server::").is_err());

        // Too many parts
        assert!(McpReference::parse("a::b::c").is_err());
    }

    #[test]
    fn test_mcp_config_deserialize() {
        let yaml = r#"
filesystem:
  command: npx
  args:
    - "-y"
    - "@modelcontextprotocol/server-filesystem"
    - "/tmp"
git:
  command: uvx
  args:
    - "mcp-server-git"
"#;
        let servers: HashMap<String, McpServerConfig> = serde_yaml::from_str(yaml).unwrap();
        assert!(servers.contains_key("filesystem"));
        assert!(servers.contains_key("git"));
        assert_eq!(servers["filesystem"].command, "npx");
        assert_eq!(servers["git"].args.len(), 1);
    }
}
