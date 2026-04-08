// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MCP tool wrapper implementing rig-core's `ToolDyn` trait.
//!
//! Contains `NikaMcpToolDef`, `NikaMcpTool`, and the `ToolDyn` implementation
//! that bridges our MCP tools (rmcp 0.16) with rig-core's agent system.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use parking_lot::RwLock;
use rig::completion::ToolDefinition;
use rig::tool::{ToolDyn, ToolError};

use super::error::McpToolError;
use crate::event::EventLog;
use crate::mcp::McpClient;
use crate::runtime::policy::{PolicyDecision, PolicyEnforcer};

// =============================================================================
// NikaMcpTool - Wrapper for MCP tools implementing rig-core's ToolDyn
// =============================================================================

/// Tool definition for Nika MCP tools.
///
/// This is our own definition struct that avoids the rmcp version conflict.
/// We convert MCP tool definitions from rmcp 0.16 into this format.
#[derive(Debug, Clone)]
pub struct NikaMcpToolDef {
    /// Tool name (e.g., "novanet_context")
    pub name: String,
    /// Tool description for the LLM
    pub description: String,
    /// JSON Schema for input parameters
    pub input_schema: serde_json::Value,
}

/// Shared media staging for agent tool calls.
///
/// When agent tools return binary content (images, audio, etc.),
/// the content blocks are collected here since rig's `ToolDyn::call()`
/// can only return `String`. After the agent loop completes,
/// `run_agent()` drains this map and runs the MediaProcessor pipeline.
pub type AgentMediaStaging = Arc<dashmap::DashMap<String, Vec<crate::mcp::types::ContentBlock>>>;

/// MCP tool wrapper implementing rig-core's `ToolDyn` trait.
///
/// This allows us to use our MCP tools (rmcp 0.16) with rig-core's
/// agent system without version conflicts.
///
/// Binary content from tool results is staged via `media_staging`
/// side-channel since rig's `ToolDyn::call()` returns `String` only.
#[derive(Clone)]
pub struct NikaMcpTool {
    definition: NikaMcpToolDef,
    /// Optional MCP client for real tool calls
    client: Option<Arc<McpClient>>,
    /// Shared staging for binary content blocks (agent media side-channel)
    media_staging: Option<AgentMediaStaging>,
    /// Policy enforcer for security checks on tool calls
    policy_enforcer: Option<Arc<RwLock<PolicyEnforcer>>>,
    /// Event log for PolicyBlocked events
    event_log: Option<EventLog>,
}

impl std::fmt::Debug for NikaMcpTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NikaMcpTool")
            .field("definition", &self.definition)
            .field("has_client", &self.client.is_some())
            .field("has_policy", &self.policy_enforcer.is_some())
            .finish_non_exhaustive()
    }
}

impl NikaMcpTool {
    /// Create a new NikaMcpTool from a definition (without client)
    pub fn new(definition: NikaMcpToolDef) -> Self {
        Self {
            definition,
            client: None,
            media_staging: None,
            policy_enforcer: None,
            event_log: None,
        }
    }

    /// Create a new NikaMcpTool with an MCP client for real tool calls
    pub fn with_client(definition: NikaMcpToolDef, client: Arc<McpClient>) -> Self {
        Self {
            definition,
            client: Some(client),
            media_staging: None,
            policy_enforcer: None,
            event_log: None,
        }
    }

    /// Create a new NikaMcpTool with media staging for agent loops
    pub fn with_media_staging(
        definition: NikaMcpToolDef,
        client: Arc<McpClient>,
        staging: AgentMediaStaging,
    ) -> Self {
        Self {
            definition,
            client: Some(client),
            media_staging: Some(staging),
            policy_enforcer: None,
            event_log: None,
        }
    }

    /// Attach a policy enforcer and event log for security checks.
    ///
    /// When set, every `call()` runs the tool name through the policy enforcer
    /// before dispatching to the MCP client. Blocked calls emit a PolicyBlocked
    /// event and return a ToolError.
    pub fn with_policy(
        mut self,
        enforcer: Arc<RwLock<PolicyEnforcer>>,
        event_log: EventLog,
    ) -> Self {
        self.policy_enforcer = Some(enforcer);
        self.event_log = Some(event_log);
        self
    }

    /// Get the tool name
    pub fn tool_name(&self) -> &str {
        &self.definition.name
    }
}

/// Type alias for boxed future (required by ToolDyn)
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

impl ToolDyn for NikaMcpTool {
    fn name(&self) -> String {
        self.definition.name.clone()
    }

    fn definition(&self, _prompt: String) -> BoxFuture<'_, ToolDefinition> {
        let def = ToolDefinition {
            name: self.definition.name.clone(),
            description: self.definition.description.clone(),
            parameters: self.definition.input_schema.clone(),
        };
        Box::pin(async move { def })
    }

    fn call(&self, args: String) -> BoxFuture<'_, Result<String, ToolError>> {
        let tool_name = self.definition.name.clone();
        let client = self.client.clone();
        let policy_enforcer = self.policy_enforcer.clone();
        let event_log = self.event_log.clone();

        Box::pin(async move {
            // POLICY CHECK: agent tool call
            if let Some(ref enforcer) = policy_enforcer {
                let decision = enforcer.read().check_tool_call(&tool_name);
                if let PolicyDecision::Block(reason) = decision {
                    // Emit PolicyBlocked event if event_log is available
                    if let Some(ref log) = event_log {
                        log.emit(crate::event::EventKind::PolicyBlocked {
                            task_id: std::sync::Arc::from("agent"),
                            verb: "agent".to_string(),
                            policy_type: "tool_blocklist".to_string(),
                            reason: reason.clone(),
                        });
                    }
                    tracing::warn!(
                        tool = %tool_name,
                        reason = %reason,
                        "agent: tool call blocked by policy"
                    );
                    return Err(ToolError::ToolCallError(Box::new(
                        McpToolError::call_failed(reason),
                    )));
                }
            }

            // Parse the args as JSON
            let params: serde_json::Value = serde_json::from_str(&args).map_err(|e| {
                ToolError::ToolCallError(Box::new(McpToolError::invalid_args(format!(
                    "Invalid JSON arguments: {}",
                    e
                ))))
            })?;

            // Check if we have a client
            let client = client.ok_or_else(|| {
                ToolError::ToolCallError(Box::new(McpToolError::not_configured(
                    "No MCP client configured for this tool",
                )))
            })?;

            // Call the MCP tool
            let result = client.call_tool(&tool_name, params).await.map_err(|e| {
                ToolError::ToolCallError(Box::new(McpToolError::call_failed(format!(
                    "MCP tool call failed: {}",
                    e
                ))))
            })?;

            // Stage binary content blocks via side-channel (if media staging is enabled)
            if result.has_media() {
                if let Some(ref staging) = self.media_staging {
                    let media_blocks: Vec<_> = result.media_blocks().into_iter().cloned().collect();
                    if !media_blocks.is_empty() {
                        tracing::debug!(
                            tool = %tool_name,
                            media_count = media_blocks.len(),
                            "agent: staging binary content from tool call"
                        );
                        staging
                            .entry(tool_name.clone())
                            .or_default()
                            .extend(media_blocks);
                    }
                } else {
                    tracing::warn!(
                        tool = %tool_name,
                        media_count = result.media_blocks().len(),
                        "agent: tool returned binary content but no media staging configured — data will be lost"
                    );
                }
            }

            // Extract text content from the result
            let output = result.text();

            if output.is_empty() {
                // Return the full result as JSON if no text content
                serde_json::to_string(&result).map_err(|e| {
                    ToolError::ToolCallError(Box::new(McpToolError::serialization(format!(
                        "Failed to serialize result: {}",
                        e
                    ))))
                })
            } else {
                Ok(output)
            }
        })
    }
}
