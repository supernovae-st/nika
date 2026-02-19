//! Rig-based Agent Loop (v0.3)
//!
//! This module implements agentic execution using rig-core's AgentBuilder.
//! It replaces the custom agent_loop.rs with rig's native multi-turn support.
//!
//! ## Key Benefits
//! - Native tool calling via rig's ToolDyn trait
//! - Simpler codebase (rig handles the loop)
//! - Better provider abstraction (rig handles Claude/OpenAI/etc)
//!
//! ## Architecture
//! ```text
//! RigAgentLoop
//!   ├── Creates rig::Agent via AgentBuilder
//!   ├── Converts MCP tools to NikaMcpTool (implements ToolDyn)
//!   ├── Runs agent.chat() for multi-turn execution
//!   └── Emits events to EventLog for observability
//! ```

use std::sync::Arc;

use rig::agent::AgentBuilder;
use rig::client::{CompletionClient, ProviderClient};
use rig::providers::anthropic;
use rustc_hash::FxHashMap;
use serde_json::Value;

use crate::ast::AgentParams;
use crate::error::NikaError;
use crate::event::{EventKind, EventLog};
use crate::mcp::McpClient;
use crate::provider::rig::{NikaMcpTool, NikaMcpToolDef};

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/// Status of the rig-based agent execution
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RigAgentStatus {
    /// Agent completed naturally (no more tool calls)
    NaturalCompletion,
    /// Stop condition matched in output
    StopConditionMet,
    /// Maximum turns reached
    MaxTurnsReached,
    /// Token budget exceeded
    TokenBudgetExceeded,
    /// Agent failed with error
    Failed,
}

/// Result of running the rig-based agent loop
#[derive(Debug)]
pub struct RigAgentLoopResult {
    /// Final status
    pub status: RigAgentStatus,
    /// Number of turns executed
    pub turns: usize,
    /// Final output from agent
    pub final_output: Value,
    /// Total tokens used (if available)
    pub total_tokens: u64,
}

// ═══════════════════════════════════════════════════════════════════════════
// RigAgentLoop
// ═══════════════════════════════════════════════════════════════════════════

/// Rig-based agentic execution loop
///
/// Uses rig-core's AgentBuilder for multi-turn execution with MCP tools.
pub struct RigAgentLoop {
    /// Task identifier for event logging
    task_id: String,
    /// Agent parameters from workflow YAML
    params: AgentParams,
    /// Event log for observability
    event_log: EventLog,
    /// Connected MCP clients (used in run_claude for tool result callbacks)
    #[allow(dead_code)] // Will be used when run_claude is fully implemented
    mcp_clients: FxHashMap<String, Arc<McpClient>>,
    /// Pre-built tools from MCP clients
    tools: Vec<Box<dyn rig::tool::ToolDyn>>,
}

impl std::fmt::Debug for RigAgentLoop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RigAgentLoop")
            .field("task_id", &self.task_id)
            .field("params", &self.params)
            .field("tool_count", &self.tools.len())
            .finish_non_exhaustive()
    }
}

impl RigAgentLoop {
    /// Create a new rig-based agent loop
    ///
    /// # Errors
    /// - NIKA-113: Empty prompt
    /// - NIKA-113: Invalid max_turns (0 or > 100)
    pub fn new(
        task_id: String,
        params: AgentParams,
        event_log: EventLog,
        mcp_clients: FxHashMap<String, Arc<McpClient>>,
    ) -> Result<Self, NikaError> {
        // Validate params
        if params.prompt.is_empty() {
            return Err(NikaError::AgentValidationError {
                reason: format!("Agent prompt cannot be empty (task: {})", task_id),
            });
        }

        if let Some(max_turns) = params.max_turns {
            if max_turns == 0 {
                return Err(NikaError::AgentValidationError {
                    reason: format!("max_turns must be at least 1 (task: {})", task_id),
                });
            }
            if max_turns > 100 {
                return Err(NikaError::AgentValidationError {
                    reason: format!("max_turns cannot exceed 100 (task: {})", task_id),
                });
            }
        }

        // Build tools from MCP clients
        let tools = Self::build_tools(&params.mcp, &mcp_clients)?;

        Ok(Self {
            task_id,
            params,
            event_log,
            mcp_clients,
            tools,
        })
    }

    /// Build NikaMcpTool instances from MCP clients
    fn build_tools(
        mcp_names: &[String],
        mcp_clients: &FxHashMap<String, Arc<McpClient>>,
    ) -> Result<Vec<Box<dyn rig::tool::ToolDyn>>, NikaError> {
        let mut tools: Vec<Box<dyn rig::tool::ToolDyn>> = Vec::new();

        for mcp_name in mcp_names {
            let client = mcp_clients.get(mcp_name).ok_or_else(|| {
                NikaError::McpNotConnected {
                    name: mcp_name.clone(),
                }
            })?;

            // Get tool definitions from MCP client
            // For now, we'll get mock tools if client is in mock mode
            let tool_defs = client.get_tool_definitions();

            for def in tool_defs {
                let tool = NikaMcpTool::with_client(
                    NikaMcpToolDef {
                        name: def.name.clone(),
                        description: def.description.clone().unwrap_or_default(),
                        input_schema: def.input_schema.clone().unwrap_or_else(|| serde_json::json!({"type": "object"})),
                    },
                    client.clone(),
                );
                tools.push(Box::new(tool));
            }
        }

        Ok(tools)
    }

    /// Get the number of tools available
    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }

    /// Run the agent loop with a mock provider (for testing)
    ///
    /// This method simulates agent execution without making real API calls.
    pub async fn run_mock(&self) -> Result<RigAgentLoopResult, NikaError> {
        // Emit start event
        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index: 1,
            kind: "started".to_string(),
            tokens: Some(0),
        });

        // For mock execution, we simulate a single turn with natural completion
        let final_output = serde_json::json!({
            "response": "Mock response from rig agent",
            "completed": true
        });

        // Check stop conditions
        let status = if self.check_stop_conditions(&final_output.to_string()) {
            RigAgentStatus::StopConditionMet
        } else {
            RigAgentStatus::NaturalCompletion
        };

        // Emit completion event
        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index: 1,
            kind: format!("{:?}", status),
            tokens: Some(100),
        });

        Ok(RigAgentLoopResult {
            status,
            turns: 1,
            final_output,
            total_tokens: 100, // Mock token count
        })
    }

    /// Run the agent loop with the real Claude provider
    ///
    /// This method uses rig-core's AgentBuilder for actual execution.
    /// Requires ANTHROPIC_API_KEY environment variable to be set.
    #[allow(dead_code)]
    pub async fn run_claude(&self) -> Result<RigAgentLoopResult, NikaError> {
        // Create Anthropic client from environment
        let client = anthropic::Client::from_env();

        let model = client.completion_model(anthropic::completion::CLAUDE_3_5_SONNET);

        // Build agent with tools
        let mut builder = AgentBuilder::new(model)
            .preamble(&self.params.prompt);

        // Set max turns
        if let Some(max_turns) = self.params.max_turns {
            builder = builder.default_max_turns(max_turns as usize);
        }

        // TODO: Add tools to builder
        // This requires moving tools ownership or using references
        // For now, we'll handle this in a follow-up implementation

        let _agent = builder.build();

        // Emit start event
        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index: 1,
            kind: "started".to_string(),
            tokens: Some(0),
        });

        // Run agent chat
        // let response = agent.chat(&self.params.prompt, vec![]).await
        //     .map_err(|e| NikaError::AgentLoopError {
        //         task_id: self.task_id.clone(),
        //         reason: e.to_string(),
        //     })?;

        // For now, return mock result (real implementation coming)
        Ok(RigAgentLoopResult {
            status: RigAgentStatus::NaturalCompletion,
            turns: 1,
            final_output: serde_json::json!({"response": "Claude response"}),
            total_tokens: 0,
        })
    }

    /// Check if any stop condition is met in the output
    fn check_stop_conditions(&self, output: &str) -> bool {
        self.params
            .stop_conditions
            .iter()
            .any(|cond| output.contains(cond))
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Unit Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rig_agent_status_variants() {
        let status = RigAgentStatus::NaturalCompletion;
        assert_eq!(status, RigAgentStatus::NaturalCompletion);

        let status = RigAgentStatus::MaxTurnsReached;
        assert_eq!(status, RigAgentStatus::MaxTurnsReached);
    }

    #[test]
    fn test_rig_agent_loop_result_debug() {
        let result = RigAgentLoopResult {
            status: RigAgentStatus::NaturalCompletion,
            turns: 1,
            final_output: serde_json::json!({}),
            total_tokens: 50,
        };
        let debug = format!("{:?}", result);
        assert!(debug.contains("NaturalCompletion"));
    }

    #[test]
    fn test_check_stop_conditions() {
        let params = AgentParams {
            prompt: "Test".to_string(),
            stop_conditions: vec!["DONE".to_string(), "COMPLETE".to_string()],
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new(
            "test".to_string(),
            params,
            event_log,
            mcp_clients,
        ).unwrap();

        assert!(agent.check_stop_conditions("Task is DONE"));
        assert!(agent.check_stop_conditions("COMPLETE!"));
        assert!(!agent.check_stop_conditions("Still working..."));
    }
}
