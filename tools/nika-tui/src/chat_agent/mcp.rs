// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MCP tool invocation and agentic loop
//!
//! Handles the `invoke:` and `agent:` TUI commands for calling MCP tools
//! and running multi-turn agentic loops.

use nika_engine::ast::AgentParams;
use nika_engine::core::mcp_config::{load_merged_config, McpServer};
use nika_engine::error::NikaError;
use nika_engine::event::EventLog;
use nika_engine::mcp::types::McpConfig as McpClientConfig;
use nika_engine::mcp::McpClient;
use nika_engine::runtime::RigAgentLoop;
use rustc_hash::FxHashMap;
use serde_json::Value;
use std::sync::Arc;

use super::ChatAgent;

impl ChatAgent {
    /// Invoke an MCP tool
    ///
    /// # Arguments
    ///
    /// * `tool_name` - The name of the tool to invoke
    /// * `server_name` - Optional server name (uses first available if None)
    /// * `params` - JSON parameters to pass to the tool
    ///
    /// # Returns
    ///
    /// The tool result as a JSON value.
    ///
    /// # Errors
    ///
    /// Returns `NikaError::McpNotConnected` if the server is not available.
    /// Returns `NikaError::McpToolError` if the tool call fails.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use serde_json::json;
    ///
    /// let result = agent.invoke(
    ///     "novanet_describe",
    ///     Some("novanet"),
    ///     json!({ "describe": "schema" })
    /// ).await?;
    /// ```
    pub async fn invoke(
        &self,
        tool_name: &str,
        server_name: Option<&str>,
        params: Value,
    ) -> Result<String, NikaError> {
        // Load merged MCP config (global + project)
        let config = load_merged_config().map_err(|e| NikaError::InvalidConfig {
            message: format!("Failed to load MCP config: {}", e),
        })?;

        // Resolve server - use provided name or first available
        let (resolved_server_name, server): (String, &McpServer) = if let Some(name) = server_name {
            let server = config
                .servers
                .get(name)
                .ok_or_else(|| NikaError::InvalidConfig {
                    message: format!(
                        "MCP server '{}' not found. Available: {:?}",
                        name,
                        config.servers.keys().collect::<Vec<_>>()
                    ),
                })?;
            (name.to_string(), server)
        } else {
            // Use first enabled server
            let (name, server) =
                config
                    .servers
                    .iter()
                    .find(|(_, s)| s.enabled)
                    .ok_or_else(|| NikaError::InvalidConfig {
                        message: "No MCP servers configured. Use 'nika mcp add' to add one."
                            .to_string(),
                    })?;
            (name.clone(), server)
        };

        // Check if server is enabled
        if !server.enabled {
            return Err(NikaError::InvalidConfig {
                message: format!("MCP server '{}' is disabled", resolved_server_name),
            });
        }

        // Convert core::mcp_config::McpServer to mcp::types::McpConfig
        let client_config = McpClientConfig {
            name: resolved_server_name.clone(),
            command: server.command.clone(),
            args: server.args.clone(),
            env: server
                .env
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            cwd: None,
        };

        // Create and connect MCP client
        let client = McpClient::new(client_config)?;
        client.connect().await?;

        // Call the tool
        let result = client.call_tool(tool_name, params).await?;

        // Format result for display
        let format_block = |c: nika_engine::mcp::types::ContentBlock| -> String {
            use nika_engine::mcp::types::ContentBlock;
            match c {
                ContentBlock::Text { text } => text,
                ContentBlock::Image { data, mime_type } => {
                    format!("[Image: {} bytes, {}]", data.len(), mime_type)
                }
                ContentBlock::Audio { data, mime_type } => {
                    format!("[Audio: {} bytes, {}]", data.len(), mime_type)
                }
                ContentBlock::Resource(res) => res
                    .text
                    .unwrap_or_else(|| format!("[Resource: {}]", res.uri)),
                ContentBlock::ResourceLink { uri, name, .. } => {
                    if let Some(n) = name {
                        format!("[ResourceLink: {} ({})]", uri, n)
                    } else {
                        format!("[ResourceLink: {}]", uri)
                    }
                }
            }
        };

        if result.is_error {
            let error_text = result
                .content
                .into_iter()
                .map(format_block)
                .collect::<Vec<_>>()
                .join("\n");
            Err(NikaError::McpToolError {
                tool: tool_name.to_string(),
                reason: format!(
                    "MCP server '{}' returned error: {}",
                    resolved_server_name, error_text
                ),
                error_code: None,
            })
        } else {
            let text = result
                .content
                .into_iter()
                .map(format_block)
                .collect::<Vec<_>>()
                .join("\n");

            Ok(text)
        }
    }

    /// Run an agentic loop with MCP tools
    ///
    /// # Arguments
    ///
    /// * `goal` - The goal/prompt for the agent
    /// * `max_turns` - Maximum number of agentic turns (default: 10)
    /// * `extended_thinking` - Enable extended thinking mode (Claude only)
    /// * `servers` - List of MCP server names to use
    ///
    /// # Returns
    ///
    /// The final response from the agent.
    ///
    /// # Errors
    ///
    /// Returns `NikaError::InvalidConfig` if MCP servers cannot be loaded.
    /// Returns `NikaError::AgentValidationError` if agent parameters are invalid.
    /// Returns `NikaError::McpNotConnected` if MCP servers cannot connect.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let result = agent.run_agent(
    ///     "Analyze the QR Code entity using NovaNet tools".to_string(),
    ///     Some(5),
    ///     false,
    ///     vec!["novanet".to_string()]
    /// ).await?;
    /// ```
    pub async fn run_agent(
        &self,
        goal: String,
        max_turns: Option<u32>,
        extended_thinking: bool,
        servers: Vec<String>,
    ) -> Result<String, NikaError> {
        // Load merged MCP config (global + project)
        let config = load_merged_config().map_err(|e| NikaError::InvalidConfig {
            message: format!("Failed to load MCP config: {}", e),
        })?;

        // Build MCP clients for requested servers
        let mut mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

        // Determine which servers to use
        let servers_to_use = if servers.is_empty() {
            // Use all enabled servers
            config
                .servers
                .iter()
                .filter(|(_, s)| s.enabled)
                .map(|(name, _)| name.clone())
                .collect::<Vec<_>>()
        } else {
            servers
        };

        if servers_to_use.is_empty() {
            return Err(NikaError::InvalidConfig {
                message: "No MCP servers configured. Use 'nika mcp add' to add one.".to_string(),
            });
        }

        // Connect to each MCP server
        for server_name in &servers_to_use {
            let server =
                config
                    .servers
                    .get(server_name)
                    .ok_or_else(|| NikaError::InvalidConfig {
                        message: format!(
                            "MCP server '{}' not found. Available: {:?}",
                            server_name,
                            config.servers.keys().collect::<Vec<_>>()
                        ),
                    })?;

            if !server.enabled {
                tracing::warn!("Skipping disabled MCP server: {}", server_name);
                continue;
            }

            // Convert to McpClientConfig
            let client_config = McpClientConfig {
                name: server_name.clone(),
                command: server.command.clone(),
                args: server.args.clone(),
                env: server
                    .env
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
                cwd: None,
            };

            // Create and connect client
            let client = McpClient::new(client_config)?;
            client.connect().await?;
            mcp_clients.insert(server_name.clone(), Arc::new(client));
        }

        // Build AgentParams
        let params = AgentParams {
            prompt: goal,
            system: None,
            provider: None, // Let run_auto() detect from env
            model: None,    // Use default model
            mcp: servers_to_use.clone(),
            tools: vec![], // No explicit builtin tools filter
            max_turns: Some(max_turns.unwrap_or(10)),
            token_budget: None,
            stop_sequences: vec![],
            scope: None,
            extended_thinking: if extended_thinking { Some(true) } else { None },
            thinking_budget: None,
            depth_limit: Some(3), // Default depth limit for subagent spawning
            ..Default::default()
        };

        // Create EventLog for observability
        let event_log = EventLog::new();

        // Create and run the agent loop
        let task_id = format!("chat-agent-{}", uuid::Uuid::new_v4());
        let mut agent_loop =
            RigAgentLoop::new(task_id, params, event_log, mcp_clients, None, None)?;

        let result = agent_loop.run_auto().await?;

        // Extract final response from the result
        let final_response = if let Some(response) = result.final_output.get("response") {
            response.as_str().unwrap_or_default().to_string()
        } else if let Some(output) = result.final_output.get("output") {
            output.as_str().unwrap_or_default().to_string()
        } else {
            // Try to serialize the whole output
            serde_json::to_string_pretty(&result.final_output).unwrap_or_else(|_| {
                format!(
                    "[Agent completed in {} turns, {} tokens used]",
                    result.turns, result.total_tokens
                )
            })
        };

        Ok(final_response)
    }
}
