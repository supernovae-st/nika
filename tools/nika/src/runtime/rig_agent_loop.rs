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

use futures::StreamExt;
use rig::agent::AgentBuilder;
use rig::client::{CompletionClient, ProviderClient};
use rig::completion::{CompletionModel as _, GetTokenUsage, Prompt};
use rig::message::ReasoningContent;
use rig::providers::anthropic;
use rig::streaming::StreamedAssistantContent;
use rustc_hash::FxHashMap;
use serde_json::Value;

use crate::ast::AgentParams;
use crate::error::NikaError;
use crate::event::{AgentTurnMetadata, EventKind, EventLog};
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

impl RigAgentStatus {
    /// Convert to canonical snake_case string for event logging.
    /// Aligns with Anthropic API's stop_reason values.
    pub fn as_canonical_str(&self) -> &'static str {
        match self {
            Self::NaturalCompletion => "end_turn",
            Self::StopConditionMet => "stop_sequence",
            Self::MaxTurnsReached => "max_turns",
            Self::TokenBudgetExceeded => "max_tokens",
            Self::Failed => "error",
        }
    }
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
        let mut tools = Self::build_tools(&params.mcp, &mcp_clients)?;

        // Add spawn_agent tool if depth_limit allows spawning (MVP 8 Phase 2)
        // Default depth is 1 (root agent). Child agents get higher depths via spawn_agent.
        let current_depth = 1_u32;
        let max_depth = params.effective_depth_limit();
        if current_depth < max_depth {
            let spawn_tool = super::spawn::SpawnAgentTool::with_mcp(
                current_depth,
                max_depth,
                Arc::from(task_id.as_str()),
                event_log.clone(),
                mcp_clients.clone(),
                params.mcp.clone(),
            );
            tools.push(Box::new(spawn_tool));
        }

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
            let client = mcp_clients
                .get(mcp_name)
                .ok_or_else(|| NikaError::McpNotConnected {
                    name: mcp_name.clone(),
                })?;

            // Get tool definitions from MCP client
            // For now, we'll get mock tools if client is in mock mode
            let tool_defs = client.get_tool_definitions();

            for def in tool_defs {
                let tool = NikaMcpTool::with_client(
                    NikaMcpToolDef {
                        name: def.name.clone(),
                        description: def.description.clone().unwrap_or_default(),
                        input_schema: def
                            .input_schema
                            .clone()
                            .unwrap_or_else(|| serde_json::json!({"type": "object"})),
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
        // Emit start event (no metadata for "started")
        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index: 1,
            kind: "started".to_string(),
            metadata: None,
        });

        // For mock execution, we simulate a single turn with natural completion
        let response_text = "Mock response from rig agent".to_string();
        let final_output = serde_json::json!({
            "response": &response_text,
            "completed": true
        });

        // Check stop conditions
        let status = if self.check_stop_conditions(&final_output.to_string()) {
            RigAgentStatus::StopConditionMet
        } else {
            RigAgentStatus::NaturalCompletion
        };

        // Build metadata for completion event (v0.4.1)
        let stop_reason = status.as_canonical_str();
        let metadata = AgentTurnMetadata {
            thinking: None, // Mock mode doesn't have thinking
            response_text: response_text.clone(),
            input_tokens: 50,
            output_tokens: 50,
            cache_read_tokens: 0,
            stop_reason: stop_reason.to_string(),
        };

        // Emit completion event with metadata
        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index: 1,
            kind: stop_reason.to_string(),
            metadata: Some(metadata),
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
    ///
    /// # Note
    /// This method takes `&mut self` because tools are consumed (moved to rig's AgentBuilder).
    /// The agent loop is designed for single-use execution.
    ///
    /// ## Extended Thinking (v0.4+)
    /// When `extended_thinking: true` is set in AgentParams, this method uses
    /// the streaming API to capture Claude's reasoning process. The thinking
    /// is stored in `AgentTurnMetadata.thinking` for observability.
    ///
    /// ## Metadata Capture
    /// - With extended_thinking: Uses streaming API, captures thinking blocks
    /// - Without extended_thinking: Uses prompt() API, no thinking captured
    pub async fn run_claude(&mut self) -> Result<RigAgentLoopResult, NikaError> {
        // Check if extended thinking is enabled
        if self.params.extended_thinking == Some(true) {
            return self.run_claude_with_thinking().await;
        }

        // Create Anthropic client from environment
        let client = anthropic::Client::from_env();

        let model = client.completion_model(anthropic::completion::CLAUDE_3_5_SONNET);

        // Take ownership of tools (they'll be consumed by the builder)
        let tools = std::mem::take(&mut self.tools);

        // Get max_turns
        let max_turns = self.params.max_turns.unwrap_or(10) as usize;

        // Emit start event (no metadata for "started")
        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index: 1,
            kind: "started".to_string(),
            metadata: None,
        });

        // Build and run agent
        // AgentBuilder type changes when tools are added, so we branch here
        let response = if tools.is_empty() {
            // No tools - simple completion
            let agent = AgentBuilder::new(model)
                .preamble(&self.params.prompt)
                .build();

            agent
                .prompt(&self.params.prompt)
                .max_turns(max_turns)
                .await
                .map_err(|e| NikaError::AgentExecutionError {
                    task_id: self.task_id.clone(),
                    reason: e.to_string(),
                })?
        } else {
            // With tools - agentic execution
            let agent = AgentBuilder::new(model)
                .preamble(&self.params.prompt)
                .tools(tools)
                .build();

            agent
                .prompt(&self.params.prompt)
                .max_turns(max_turns)
                .await
                .map_err(|e| NikaError::AgentExecutionError {
                    task_id: self.task_id.clone(),
                    reason: e.to_string(),
                })?
        };

        // Determine status from response
        let response_str = response.clone();
        let status = if self.check_stop_conditions(&response_str) {
            RigAgentStatus::StopConditionMet
        } else {
            RigAgentStatus::NaturalCompletion
        };

        // Emit completion event (v0.4.1)
        // Note: Token usage and thinking are not available from rig's Prompt trait.
        // We emit text_only metadata - tokens show as 0 indicating "unavailable".
        // Full metadata capture requires streaming API or direct completion requests.
        let stop_reason = status.as_canonical_str();
        let metadata = AgentTurnMetadata::text_only(&response, stop_reason);

        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index: 1,
            kind: stop_reason.to_string(),
            metadata: Some(metadata),
        });

        Ok(RigAgentLoopResult {
            status,
            turns: 1, // rig handles turns internally, we report completion as 1
            final_output: serde_json::json!({ "response": response }),
            total_tokens: 0, // Token tracking requires response metadata
        })
    }

    /// Check if any stop condition is met in the output
    fn check_stop_conditions(&self, output: &str) -> bool {
        self.params
            .stop_conditions
            .iter()
            .any(|cond| output.contains(cond))
    }

    /// Run the agent loop with extended thinking enabled (Claude only).
    ///
    /// Uses rig-core's streaming API to capture thinking blocks from Claude's
    /// extended thinking feature. The thinking is accumulated and stored in
    /// the AgentTurnMetadata for observability.
    ///
    /// # Errors
    /// - NIKA-113: Extended thinking failed
    /// - NIKA-110: Agent execution error
    pub async fn run_claude_with_thinking(&mut self) -> Result<RigAgentLoopResult, NikaError> {
        // Create Anthropic client from environment
        let client = anthropic::Client::from_env();

        // Get model name
        let model_name = self
            .params
            .model
            .as_deref()
            .unwrap_or(anthropic::completion::CLAUDE_3_5_SONNET);
        let model = client.completion_model(model_name);

        // Build completion request with thinking enabled
        // Use configurable thinking_budget from AgentParams (default: 4096)
        let thinking_budget = self.params.effective_thinking_budget();
        let request = model
            .completion_request(&self.params.prompt)
            .preamble(self.params.system.clone().unwrap_or_default())
            .additional_params(serde_json::json!({
                "thinking": {
                    "type": "enabled",
                    "budget_tokens": thinking_budget
                }
            }))
            .build();

        // Emit start event
        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index: 1,
            kind: "started".to_string(),
            metadata: None,
        });

        // Execute streaming request
        let mut stream =
            model
                .stream(request)
                .await
                .map_err(|e| NikaError::AgentExecutionError {
                    task_id: self.task_id.clone(),
                    reason: format!("Streaming request failed: {}", e),
                })?;

        // Accumulate thinking, response, and token usage
        let mut thinking_parts: Vec<String> = Vec::new();
        let mut response_parts: Vec<String> = Vec::new();
        let mut input_tokens: u32 = 0;
        let mut output_tokens: u32 = 0;

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(content) => match content {
                    StreamedAssistantContent::Text(text) => {
                        response_parts.push(text.text);
                    }
                    StreamedAssistantContent::ReasoningDelta { reasoning, .. } => {
                        thinking_parts.push(reasoning);
                    }
                    StreamedAssistantContent::Reasoning(reasoning) => {
                        // Final reasoning block - extract text from content blocks
                        for block in reasoning.content {
                            if let ReasoningContent::Text { text, .. } = block {
                                thinking_parts.push(text);
                            }
                        }
                    }
                    StreamedAssistantContent::Final(final_resp) => {
                        // Extract token usage from final response (v0.4.1 fix)
                        if let Some(usage) = final_resp.token_usage() {
                            input_tokens = usage.input_tokens as u32;
                            output_tokens = usage.output_tokens as u32;
                        }
                    }
                    _ => {
                        // Tool calls and other events - handled by agent loop
                        tracing::debug!("Streaming event: {:?}", content);
                    }
                },
                Err(e) => {
                    // Return error instead of silently swallowing - critical for debugging
                    return Err(NikaError::ThinkingCaptureFailed {
                        reason: format!(
                            "Streaming chunk failed for task '{}': {}",
                            self.task_id, e
                        ),
                    });
                }
            }
        }

        // Combine accumulated text
        let thinking = if thinking_parts.is_empty() {
            None
        } else {
            Some(thinking_parts.concat())
        };
        let response = response_parts.concat();

        // Determine status
        let status = if self.check_stop_conditions(&response) {
            RigAgentStatus::StopConditionMet
        } else {
            RigAgentStatus::NaturalCompletion
        };

        // Build metadata with thinking and token usage (v0.4.1 fix)
        let stop_reason = status.as_canonical_str();
        let metadata = AgentTurnMetadata {
            thinking,
            response_text: response.clone(),
            input_tokens,
            output_tokens,
            cache_read_tokens: 0, // Cache tracking requires message metadata
            stop_reason: stop_reason.to_string(),
        };

        // Emit completion event
        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index: 1,
            kind: stop_reason.to_string(),
            metadata: Some(metadata),
        });

        Ok(RigAgentLoopResult {
            status,
            turns: 1,
            final_output: serde_json::json!({ "response": response }),
            total_tokens: (input_tokens + output_tokens) as u64,
        })
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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        assert!(agent.check_stop_conditions("Task is DONE"));
        assert!(agent.check_stop_conditions("COMPLETE!"));
        assert!(!agent.check_stop_conditions("Still working..."));
    }

    // ========================================================================
    // Extended Thinking Tests (v0.4+)
    // ========================================================================

    #[test]
    fn test_agent_loop_with_extended_thinking_creates_successfully() {
        let params = AgentParams {
            prompt: "Analyze this problem step by step".to_string(),
            extended_thinking: Some(true),
            provider: Some("claude".to_string()),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("thinking-test".to_string(), params, event_log, mcp_clients);

        assert!(
            agent.is_ok(),
            "Agent with extended_thinking should be created"
        );
    }

    #[test]
    fn test_agent_loop_extended_thinking_false_creates_successfully() {
        let params = AgentParams {
            prompt: "Simple query".to_string(),
            extended_thinking: Some(false),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new(
            "no-thinking-test".to_string(),
            params,
            event_log,
            mcp_clients,
        );

        assert!(
            agent.is_ok(),
            "Agent with extended_thinking: false should be created"
        );
    }

    #[test]
    fn test_agent_loop_extended_thinking_none_creates_successfully() {
        let params = AgentParams {
            prompt: "Default behavior".to_string(),
            extended_thinking: None,
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("default-test".to_string(), params, event_log, mcp_clients);

        assert!(
            agent.is_ok(),
            "Agent with extended_thinking: None should be created"
        );
    }

    #[test]
    fn test_agent_loop_with_system_prompt_and_thinking() {
        let params = AgentParams {
            prompt: "What is 2+2?".to_string(),
            system: Some("You are a math tutor. Think step by step.".to_string()),
            extended_thinking: Some(true),
            provider: Some("claude".to_string()),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new(
            "system-thinking-test".to_string(),
            params,
            event_log,
            mcp_clients,
        );

        assert!(
            agent.is_ok(),
            "Agent with system prompt and thinking should be created"
        );
    }
}
