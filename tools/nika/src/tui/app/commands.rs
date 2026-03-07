//! Chat Command Handlers
//!
//! Contains helper methods for chat verb commands (/infer, /exec, /fetch, /invoke, /agent).
//! The actual command execution is handled by ChatAgent.

use crate::ast::Workflow;
use crate::serde_yaml;
use crate::tui::chat_agent::ChatAgent;

use super::super::views::MessageRole;
use super::App;

impl App {
    /// Ensure chat agent exists, creating one if necessary
    ///
    /// Returns a mutable reference to the chat agent.
    #[allow(dead_code)]
    pub(crate) fn ensure_chat_agent(&mut self) -> Option<&mut ChatAgent> {
        if self.chat_agent.is_none() {
            self.chat_agent = ChatAgent::new().ok();
        }
        self.chat_agent.as_mut()
    }

    /// Build conversation context from chat view messages for LLM prompt
    ///
    /// Returns a formatted string with recent conversation history.
    /// Used to provide context for LLM inferences.
    #[allow(dead_code)]
    pub(crate) fn build_conversation_context(&self) -> String {
        // Get last N messages from chat_view for context
        let messages: Vec<_> = self.chat_view.messages.iter().rev().take(10).collect();

        if messages.is_empty() {
            return String::new();
        }

        let mut context = String::from("\n\n[Previous conversation]\n");
        for msg in messages.into_iter().rev() {
            let role = match &msg.role {
                MessageRole::User => "User",
                MessageRole::Nika => "Assistant",
                MessageRole::System => "System",
                MessageRole::Tool => "Tool",
            };
            context.push_str(&format!("{}: {}\n", role, msg.content));
        }
        context.push_str("[Current request]\n");
        context
    }

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

    /// Get available MCP server names from configuration
    #[allow(dead_code)]
    pub(crate) fn get_mcp_server_names(&self) -> Vec<String> {
        self.mcp_configs
            .as_ref()
            .map(|c| c.keys().cloned().collect())
            .unwrap_or_default()
    }
}
