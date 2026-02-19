//! Agent Loop - Agentic Execution Engine
//!
//! Executes multi-turn conversations with tool calling via MCP.
//!
//! The agent loop:
//! 1. Sends a prompt to the LLM with available tools
//! 2. If the LLM requests tool calls, executes them via MCP
//! 3. Feeds results back to the LLM
//! 4. Repeats until:
//!    - No more tool calls (natural completion)
//!    - Stop condition matched in output
//!    - Max turns reached
//!
//! # Example
//!
//! ```rust,ignore
//! use nika::runtime::{AgentLoop, AgentStatus};
//! use nika::ast::AgentParams;
//! use nika::event::EventLog;
//! use nika::mcp::McpClient;
//! use nika::provider::MockProvider;
//! use rustc_hash::FxHashMap;
//! use std::sync::Arc;
//!
//! let params = AgentParams {
//!     prompt: "Generate content for QR code entity".to_string(),
//!     mcp: vec!["novanet".to_string()],
//!     max_turns: Some(10),
//!     ..Default::default()
//! };
//!
//! let event_log = EventLog::new();
//! let mut mcp_clients = FxHashMap::default();
//! mcp_clients.insert("novanet".to_string(), Arc::new(McpClient::mock("novanet")));
//!
//! let agent_loop = AgentLoop::new("task1".to_string(), params, event_log, mcp_clients)?;
//! let result = agent_loop.run(Arc::new(MockProvider::default())).await?;
//!
//! match result.status {
//!     AgentStatus::NaturalCompletion => println!("Completed naturally"),
//!     AgentStatus::StopConditionMet => println!("Stop condition matched"),
//!     AgentStatus::MaxTurnsReached => println!("Reached max turns"),
//!     AgentStatus::Failed => println!("Agent failed"),
//! }
//! ```

use futures::future::join_all;
use rustc_hash::FxHashMap;
use std::sync::Arc;

use crate::ast::AgentParams;
use crate::error::{NikaError, Result};
use crate::event::{EventKind, EventLog};
use crate::mcp::McpClient;
use crate::provider::{Message, MessageRole, Provider, ToolCall, ToolDefinition};

// ============================================================================
// AgentStatus - Completion reason
// ============================================================================

/// Agent completion status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    /// Completed naturally (LLM returned no tool calls)
    NaturalCompletion,
    /// Stopped due to stop condition match in output
    StopConditionMet,
    /// Reached max_turns limit
    MaxTurnsReached,
    /// Token budget exceeded - stopped gracefully
    TokenBudgetExceeded,
    /// Error during execution
    Failed,
}

// ============================================================================
// AgentLoopResult - Execution result
// ============================================================================

/// Result of an agent loop execution
#[derive(Debug)]
pub struct AgentLoopResult {
    /// How the agent loop completed
    pub status: AgentStatus,
    /// Number of turns executed
    pub turns: u32,
    /// Final output (parsed as JSON if possible)
    pub final_output: serde_json::Value,
    /// Total tokens used across all turns
    pub total_tokens: u32,
}

// ============================================================================
// AgentLoop - Main agent implementation
// ============================================================================

/// Agent loop for agentic execution
///
/// Executes a multi-turn conversation with tool calling support via MCP.
#[derive(Debug)]
pub struct AgentLoop {
    /// Task ID for event correlation
    task_id: String,
    /// Agent parameters from YAML
    params: AgentParams,
    /// Event log for observability
    event_log: EventLog,
    /// Connected MCP clients by name
    mcp_clients: FxHashMap<String, Arc<McpClient>>,
}

impl AgentLoop {
    /// Create a new agent loop
    ///
    /// # Arguments
    ///
    /// * `task_id` - Unique task identifier for events
    /// * `params` - Agent parameters from YAML
    /// * `event_log` - Event log for observability
    /// * `mcp_clients` - Map of MCP server name to connected client
    ///
    /// # Errors
    ///
    /// Returns `NikaError::AgentValidationError` if params are invalid.
    pub fn new(
        task_id: String,
        params: AgentParams,
        event_log: EventLog,
        mcp_clients: FxHashMap<String, Arc<McpClient>>,
    ) -> Result<Self> {
        // Validate params before creating the loop
        params
            .validate()
            .map_err(|e| NikaError::AgentValidationError { reason: e })?;

        Ok(Self {
            task_id,
            params,
            event_log,
            mcp_clients,
        })
    }

    /// Run the agent loop
    ///
    /// Executes multi-turn conversation until completion, stop condition, or max turns.
    ///
    /// # Arguments
    ///
    /// * `provider` - LLM provider to use for inference
    ///
    /// # Returns
    ///
    /// `AgentLoopResult` with status, turns, output, and token usage.
    pub async fn run(&self, provider: Arc<dyn Provider>) -> Result<AgentLoopResult> {
        let max_turns = self.params.effective_max_turns();
        let token_budget = self.params.effective_token_budget();

        // Build initial conversation with optional system prompt
        let mut conversation: Vec<Message> = Vec::new();
        if let Some(system) = &self.params.system {
            conversation.push(Message::system(system));
        }
        conversation.push(Message::user(&self.params.prompt));

        let mut turn = 0u32;
        let mut total_tokens = 0u32;
        let model = self
            .params
            .model
            .as_deref()
            .unwrap_or(provider.default_model());

        // Build tool definitions from MCP clients
        let tools: Vec<ToolDefinition> = self.build_tool_definitions().await?;

        loop {
            // Check max turns before calling LLM (before emitting started event)
            if turn >= max_turns {
                return Ok(AgentLoopResult {
                    status: AgentStatus::MaxTurnsReached,
                    turns: turn,
                    final_output: self.extract_final_output(&conversation),
                    total_tokens,
                });
            }

            // Check token budget before calling LLM
            if total_tokens >= token_budget {
                self.event_log.emit(EventKind::AgentTurn {
                    task_id: self.task_id.clone().into(),
                    turn_index: turn,
                    kind: "token_budget_exceeded".to_string(),
                    tokens: Some(total_tokens),
                });

                return Ok(AgentLoopResult {
                    status: AgentStatus::TokenBudgetExceeded,
                    turns: turn,
                    final_output: self.extract_final_output(&conversation),
                    total_tokens,
                });
            }

            // Emit turn started event (only after confirming turn will execute)
            self.event_log.emit(EventKind::AgentTurn {
                task_id: self.task_id.clone().into(),
                turn_index: turn,
                kind: "started".to_string(),
                tokens: None,
            });

            // Call LLM with retry on transient failures
            let tools_ref = if tools.is_empty() {
                None
            } else {
                Some(tools.as_slice())
            };

            let max_llm_retries = 3;
            let mut llm_last_error: Option<NikaError> = None;
            let mut response = None;

            for llm_attempt in 0..=max_llm_retries {
                match provider.chat(&conversation, tools_ref, model).await {
                    Ok(resp) => {
                        response = Some(resp);
                        break;
                    }
                    Err(e) => {
                        let error = NikaError::ProviderApiError {
                            message: e.to_string(),
                        };

                        // Check if this is a retryable error
                        let is_retryable = Self::is_retryable_provider_error(&error);

                        if is_retryable && llm_attempt < max_llm_retries {
                            tracing::warn!(
                                task_id = %self.task_id,
                                attempt = llm_attempt + 1,
                                error = %error,
                                "LLM call failed, retrying"
                            );

                            // Exponential backoff: 100ms, 200ms, 400ms
                            let delay_ms = 100 * (1 << llm_attempt);
                            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                            llm_last_error = Some(error);
                            continue;
                        }

                        // Non-retryable or exhausted retries
                        return Err(error);
                    }
                }
            }

            let response = response.ok_or_else(|| {
                llm_last_error.unwrap_or_else(|| NikaError::ProviderApiError {
                    message: "LLM call failed after retries".to_string(),
                })
            })?;

            // Track tokens
            total_tokens += response.usage.total_tokens();

            // Add assistant response to conversation
            conversation.push(Message {
                role: MessageRole::Assistant,
                content: response.content.clone(),
                tool_call_id: None,
            });

            // Check stop conditions
            let content_text = response.content.as_text().unwrap_or_default();
            if self.params.should_stop(&content_text) {
                self.event_log.emit(EventKind::AgentTurn {
                    task_id: self.task_id.clone().into(),
                    turn_index: turn,
                    kind: "stop_condition_met".to_string(),
                    tokens: Some(total_tokens),
                });

                return Ok(AgentLoopResult {
                    status: AgentStatus::StopConditionMet,
                    turns: turn + 1,
                    final_output: self.parse_output(&content_text),
                    total_tokens,
                });
            }

            // Process tool calls
            if response.tool_calls.is_empty() {
                // No tool calls = natural completion
                self.event_log.emit(EventKind::AgentTurn {
                    task_id: self.task_id.clone().into(),
                    turn_index: turn,
                    kind: "natural_completion".to_string(),
                    tokens: Some(total_tokens),
                });

                return Ok(AgentLoopResult {
                    status: AgentStatus::NaturalCompletion,
                    turns: turn + 1,
                    final_output: self.parse_output(&content_text),
                    total_tokens,
                });
            }

            // Execute tool calls in parallel - errors are returned to LLM for recovery
            let tool_futures: Vec<_> = response
                .tool_calls
                .iter()
                .map(|tool_call| async {
                    let result = match self.execute_tool_call(tool_call).await {
                        Ok(result) => result,
                        Err(e) => {
                            // Return error to LLM so it can try an alternative approach
                            tracing::warn!(
                                task_id = %self.task_id,
                                tool = %tool_call.name,
                                error = %e,
                                "Tool call failed, returning error to LLM"
                            );
                            format!("ERROR: Tool '{}' failed: {}", tool_call.name, e)
                        }
                    };
                    (tool_call.id.clone(), result)
                })
                .collect();

            let tool_results = join_all(tool_futures).await;
            for (tool_call_id, result) in tool_results {
                conversation.push(Message::tool_result(&tool_call_id, &result));
            }

            self.event_log.emit(EventKind::AgentTurn {
                task_id: self.task_id.clone().into(),
                turn_index: turn,
                kind: "continue".to_string(),
                tokens: None,
            });

            turn += 1;
        }
    }

    /// Build tool definitions from MCP clients
    ///
    /// Queries each MCP server for available tools and builds unified definitions.
    /// Tool names are prefixed with MCP server name: `mcpname_toolname`, unless
    /// the tool name already includes that prefix (to avoid double-prefixing).
    async fn build_tool_definitions(&self) -> Result<Vec<ToolDefinition>> {
        let mut tools = Vec::new();

        for mcp_name in &self.params.mcp {
            let client =
                self.mcp_clients
                    .get(mcp_name)
                    .ok_or_else(|| NikaError::McpNotConnected {
                        name: mcp_name.clone(),
                    })?;

            let mcp_tools = client.list_tools().await?;
            let prefix = format!("{}_", mcp_name);

            for tool in mcp_tools {
                // Don't double-prefix if MCP server already prefixes tool names
                // (e.g., NovaNet returns "novanet_describe", not "describe")
                let tool_name = if tool.name.starts_with(&prefix) {
                    tool.name.clone()
                } else {
                    format!("{}{}", prefix, tool.name)
                };

                tools.push(ToolDefinition {
                    name: tool_name,
                    description: tool.description.unwrap_or_default(),
                    input_schema: tool.input_schema.unwrap_or(serde_json::json!({})),
                });
            }
        }

        Ok(tools)
    }

    /// Execute a tool call via MCP
    ///
    /// Parses the tool name to extract MCP server and tool, then invokes.
    /// Format: "mcpname_toolname" where mcpname is one of the configured MCP servers.
    ///
    /// Note: If the MCP server returns tools already prefixed (e.g., "novanet_describe"),
    /// we pass the full prefixed name to MCP (not strip it down to just "describe").
    async fn execute_tool_call(&self, tool_call: &ToolCall) -> Result<String> {
        // Find which MCP server this tool belongs to by checking prefixes
        let (mcp_name, tool_name) = self
            .mcp_clients
            .keys()
            .find_map(|name| {
                let prefix = format!("{}_", name);
                if tool_call.name.starts_with(&prefix) {
                    // MCP servers like NovaNet prefix their tools (e.g., "novanet_describe")
                    // We pass the FULL tool name to MCP, not the stripped version
                    Some((name.as_str(), tool_call.name.as_str()))
                } else {
                    None
                }
            })
            .ok_or_else(|| NikaError::InvalidToolName {
                name: tool_call.name.clone(),
            })?;

        // Generate unique call_id for correlation
        let call_id = uuid::Uuid::new_v4().to_string();
        let start = std::time::Instant::now();

        // Emit tool call event
        self.event_log.emit(EventKind::McpInvoke {
            task_id: self.task_id.clone().into(),
            call_id: call_id.clone(),
            mcp_server: mcp_name.to_string(),
            tool: Some(tool_name.to_string()),
            resource: None,
        });

        let client = self
            .mcp_clients
            .get(mcp_name)
            .ok_or_else(|| NikaError::McpNotConnected {
                name: mcp_name.to_string(),
            })?;

        let result = client
            .call_tool(tool_name, tool_call.arguments.clone())
            .await?;
        let duration_ms = start.elapsed().as_millis() as u64;

        // Emit response event
        self.event_log.emit(EventKind::McpResponse {
            task_id: self.task_id.clone().into(),
            call_id,
            output_len: result.text().len(),
            duration_ms,
            cached: false,
            is_error: result.is_error,
        });

        Ok(result.text())
    }

    /// Try to parse output as JSON, fallback to string
    fn parse_output(&self, content: &str) -> serde_json::Value {
        // Try to find JSON object in the content
        if let Some(start) = content.find('{') {
            if let Some(end) = content.rfind('}') {
                if let Ok(json) = serde_json::from_str(&content[start..=end]) {
                    return json;
                }
            }
        }
        // Fallback to string value
        serde_json::Value::String(content.to_string())
    }

    /// Extract final output from conversation
    fn extract_final_output(&self, conversation: &[Message]) -> serde_json::Value {
        conversation
            .last()
            .and_then(|m| m.content.as_text())
            .map(|text| self.parse_output(&text))
            .unwrap_or(serde_json::Value::Null)
    }

    /// Check if a provider error is retryable (transient).
    ///
    /// Retryable errors include:
    /// - Rate limits (429)
    /// - Server errors (5xx)
    /// - Network timeouts
    /// - Connection issues
    pub fn is_retryable_provider_error(error: &NikaError) -> bool {
        let error_str = error.to_string().to_lowercase();

        // Rate limits
        error_str.contains("rate limit")
            || error_str.contains("429")
            || error_str.contains("too many requests")
            // Server errors
            || error_str.contains("500")
            || error_str.contains("502")
            || error_str.contains("503")
            || error_str.contains("504")
            || error_str.contains("internal server error")
            || error_str.contains("bad gateway")
            || error_str.contains("service unavailable")
            || error_str.contains("gateway timeout")
            // Network issues
            || error_str.contains("timeout")
            || error_str.contains("timed out")
            || error_str.contains("connection reset")
            || error_str.contains("connection refused")
            || error_str.contains("network")
            // Overloaded
            || error_str.contains("overloaded")
            || error_str.contains("capacity")
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_status_equality() {
        assert_eq!(
            AgentStatus::NaturalCompletion,
            AgentStatus::NaturalCompletion
        );
        assert_ne!(AgentStatus::NaturalCompletion, AgentStatus::Failed);
    }

    #[test]
    fn test_agent_status_copy() {
        let status = AgentStatus::MaxTurnsReached;
        let copied = status;
        assert_eq!(status, copied);
    }

    #[test]
    fn test_parse_output_json() {
        let params = AgentParams {
            prompt: "test".to_string(),
            ..Default::default()
        };
        let agent_loop = AgentLoop {
            task_id: "test".to_string(),
            params,
            event_log: EventLog::new(),
            mcp_clients: FxHashMap::default(),
        };

        let content = r#"Here is the result: {"key": "value"} done"#;
        let output = agent_loop.parse_output(content);
        assert!(output.is_object());
        assert_eq!(output["key"], "value");
    }

    #[test]
    fn test_parse_output_string_fallback() {
        let params = AgentParams {
            prompt: "test".to_string(),
            ..Default::default()
        };
        let agent_loop = AgentLoop {
            task_id: "test".to_string(),
            params,
            event_log: EventLog::new(),
            mcp_clients: FxHashMap::default(),
        };

        let content = "Just plain text response";
        let output = agent_loop.parse_output(content);
        assert!(output.is_string());
        assert_eq!(output.as_str().unwrap(), "Just plain text response");
    }

    #[test]
    fn test_agent_loop_creation_validates_params() {
        let params = AgentParams::default(); // Empty prompt
        let result = AgentLoop::new("test".to_string(), params, EventLog::new(), FxHashMap::default());
        assert!(result.is_err());
    }

    // ========================================================================
    // Tool Name Prefixing Tests
    // ========================================================================

    #[test]
    fn test_tool_name_already_prefixed_not_doubled() {
        // When MCP server returns tool names already prefixed (e.g., novanet_describe),
        // we should NOT add another prefix (avoiding novanet_novanet_describe).
        let mcp_name = "novanet";
        let tool_name = "novanet_describe"; // Already prefixed by MCP server

        // Simulate the prefixing logic
        let prefixed = if tool_name.starts_with(&format!("{}_", mcp_name)) {
            tool_name.to_string()
        } else {
            format!("{}_{}", mcp_name, tool_name)
        };

        assert_eq!(prefixed, "novanet_describe", "Should not double-prefix");
    }

    #[test]
    fn test_tool_name_not_prefixed_gets_prefix() {
        // When MCP server returns tool names without prefix (e.g., describe),
        // we should add the prefix.
        let mcp_name = "novanet";
        let tool_name = "describe"; // Not prefixed

        // Simulate the prefixing logic
        let prefixed = if tool_name.starts_with(&format!("{}_", mcp_name)) {
            tool_name.to_string()
        } else {
            format!("{}_{}", mcp_name, tool_name)
        };

        assert_eq!(prefixed, "novanet_describe", "Should add prefix");
    }

    #[test]
    fn test_execute_tool_extracts_correct_name() {
        // When Claude calls "novanet_describe", we should pass "novanet_describe"
        // to MCP (the full tool name as registered), not just "describe".
        let tool_call_name = "novanet_describe";
        let mcp_name = "novanet";
        let prefix = format!("{}_", mcp_name);

        // New behavior: if tool name starts with MCP prefix, pass full name to MCP
        let tool_name_for_mcp = if tool_call_name.starts_with(&prefix) {
            tool_call_name.to_string() // Keep the full name for MCP
        } else {
            tool_call_name.to_string()
        };

        assert_eq!(tool_name_for_mcp, "novanet_describe", "Should pass full tool name to MCP");
    }

    #[test]
    fn test_tool_name_with_custom_mcp_prefix() {
        // Test with a different MCP server name
        let mcp_name = "custom_server";
        let tool_name = "custom_server_mytool"; // Already prefixed

        let prefixed = if tool_name.starts_with(&format!("{}_", mcp_name)) {
            tool_name.to_string()
        } else {
            format!("{}_{}", mcp_name, tool_name)
        };

        assert_eq!(prefixed, "custom_server_mytool", "Should not double-prefix");
    }

    // ========================================================================
    // Token Budget Tests
    // ========================================================================

    #[test]
    fn test_token_budget_exceeded_status() {
        assert_ne!(
            AgentStatus::TokenBudgetExceeded,
            AgentStatus::MaxTurnsReached
        );
        assert_eq!(
            AgentStatus::TokenBudgetExceeded,
            AgentStatus::TokenBudgetExceeded
        );
    }

    // ========================================================================
    // System Prompt Tests
    // ========================================================================

    #[test]
    fn test_agent_params_with_system_prompt() {
        let params = AgentParams {
            prompt: "user message".to_string(),
            system: Some("You are a helpful assistant.".to_string()),
            ..Default::default()
        };
        assert!(params.validate().is_ok());
        assert!(params.system.is_some());
    }

    // ========================================================================
    // Retry Logic Tests
    // ========================================================================

    #[test]
    fn test_is_retryable_provider_error_rate_limit() {
        let error = NikaError::ProviderApiError {
            message: "Rate limit exceeded (429)".to_string(),
        };
        assert!(AgentLoop::is_retryable_provider_error(&error));
    }

    #[test]
    fn test_is_retryable_provider_error_server_error() {
        let error = NikaError::ProviderApiError {
            message: "Internal server error (500)".to_string(),
        };
        assert!(AgentLoop::is_retryable_provider_error(&error));
    }

    #[test]
    fn test_is_retryable_provider_error_timeout() {
        let error = NikaError::ProviderApiError {
            message: "Request timed out".to_string(),
        };
        assert!(AgentLoop::is_retryable_provider_error(&error));
    }

    #[test]
    fn test_is_retryable_provider_error_not_retryable() {
        let error = NikaError::ProviderApiError {
            message: "Invalid API key".to_string(),
        };
        assert!(!AgentLoop::is_retryable_provider_error(&error));
    }
}
