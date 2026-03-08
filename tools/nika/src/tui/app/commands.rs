//! MCP Client Initialization
//!
//! Contains MCP server configuration loading from workflow files.

use crate::ast::Workflow;
use crate::serde_yaml;

use super::App;

impl App {
    /// Load MCP server configurations from workflow
    ///
    /// Parses the workflow YAML and extracts MCP server configs.
    /// Actual client connections are lazy-initialized on first use via `get_mcp_client()`.
    pub(crate) fn init_mcp_clients(&mut self) {
        // Read and parse workflow file
        let yaml_content = match std::fs::read_to_string(&self.workflow_path) {
            Ok(content) => content,
            Err(e) => {
                tracing::warn!("Failed to read workflow for MCP init: {}", e);
                return;
            }
        };

        let workflow: Workflow = match serde_yaml::from_str(&yaml_content) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("Failed to parse workflow for MCP init: {}", e);
                return;
            }
        };

        // Store MCP configs for lazy initialization
        if let Some(mcp_configs) = workflow.mcp {
            let server_names: Vec<_> = mcp_configs.keys().cloned().collect();
            tracing::info!(servers = ?server_names, "Loaded MCP server configurations");

            // Update ChatView's session context with actual MCP servers
            self.chat_view.set_mcp_servers(server_names.iter().cloned());

            self.mcp_configs = Some(mcp_configs);
        }
    }
}
