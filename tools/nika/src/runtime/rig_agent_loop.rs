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
use rig::completion::{Chat, CompletionModel as _, GetTokenUsage, Prompt};
use rig::message::{Message, ReasoningContent};
use rig::providers::{anthropic, openai};
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
///
/// ## Chat History (v0.6)
///
/// The agent loop now supports conversation history for multi-turn interactions:
///
/// ```rust,ignore
/// let mut agent = RigAgentLoop::new(...)?;
///
/// // First turn
/// let result = agent.run_claude().await?;
///
/// // Continue conversation with history
/// agent.add_to_history("What's the capital of France?", &result.final_output.to_string());
/// let result2 = agent.chat_continue("And what about Germany?").await?;
/// ```
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
    /// Conversation history for multi-turn chat (v0.6)
    history: Vec<Message>,
}

impl std::fmt::Debug for RigAgentLoop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RigAgentLoop")
            .field("task_id", &self.task_id)
            .field("params", &self.params)
            .field("tool_count", &self.tools.len())
            .field("history_len", &self.history.len())
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
            history: Vec::new(),
        })
    }

    // =========================================================================
    // v0.6: Chat History Management
    // =========================================================================

    /// Add a user/assistant turn to the conversation history
    ///
    /// Call this after each completed turn to maintain context for `chat_continue()`.
    pub fn add_to_history(&mut self, user_prompt: &str, assistant_response: &str) {
        self.history.push(Message::user(user_prompt));
        self.history.push(Message::assistant(assistant_response));
    }

    /// Add a single message to the history
    pub fn push_message(&mut self, message: Message) {
        self.history.push(message);
    }

    /// Clear all conversation history
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Get the current history length (number of messages)
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// Get a reference to the conversation history
    pub fn history(&self) -> &[Message] {
        &self.history
    }

    /// Create with pre-existing history (v0.6)
    ///
    /// Useful for resuming conversations or injecting context.
    pub fn with_history(mut self, history: Vec<Message>) -> Self {
        self.history = history;
        self
    }

    /// Continue a conversation using the accumulated history (v0.6)
    ///
    /// Uses rig-core's `Chat` trait for multi-turn conversations.
    /// The history is automatically updated with the user prompt and response.
    ///
    /// # Example
    /// ```rust,ignore
    /// // First turn
    /// let result1 = agent.run_claude().await?;
    /// agent.add_to_history("Initial prompt", &extract_text(&result1));
    ///
    /// // Continue conversation
    /// let result2 = agent.chat_continue("Follow-up question").await?;
    /// // History now contains both turns
    /// ```
    pub async fn chat_continue(&mut self, prompt: &str) -> Result<RigAgentLoopResult, NikaError> {
        // Auto-detect provider and use chat with history
        // Helper: check env var exists and is non-empty
        let has_key = |key: &str| std::env::var(key).is_ok_and(|v| !v.is_empty());

        if has_key("ANTHROPIC_API_KEY") {
            return self.chat_continue_claude(prompt).await;
        }
        if has_key("OPENAI_API_KEY") {
            return self.chat_continue_openai(prompt).await;
        }
        if has_key("MISTRAL_API_KEY") {
            return self.chat_continue_mistral(prompt).await;
        }
        if has_key("GROQ_API_KEY") {
            return self.chat_continue_groq(prompt).await;
        }
        if has_key("DEEPSEEK_API_KEY") {
            return self.chat_continue_deepseek(prompt).await;
        }
        if has_key("OLLAMA_API_BASE_URL") {
            return self.chat_continue_ollama(prompt).await;
        }

        Err(NikaError::AgentValidationError {
            reason: "chat_continue requires one of: ANTHROPIC_API_KEY, OPENAI_API_KEY, MISTRAL_API_KEY, GROQ_API_KEY, DEEPSEEK_API_KEY, or OLLAMA_API_BASE_URL".to_string(),
        })
    }

    /// Continue conversation with Claude (v0.6)
    async fn chat_continue_claude(
        &mut self,
        prompt: &str,
    ) -> Result<RigAgentLoopResult, NikaError> {
        let client = anthropic::Client::from_env();
        let model_name = self
            .params
            .model
            .as_deref()
            .unwrap_or("claude-sonnet-4-20250514");
        let model = client.completion_model(model_name);

        let turn_index = (self.history.len() / 2 + 1) as u32;

        // Emit start event
        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index,
            kind: "started".to_string(),
            metadata: None,
        });

        // Build agent and chat with history
        let agent = AgentBuilder::new(model)
            .preamble(self.params.system.as_deref().unwrap_or(""))
            .build();

        let response = agent
            .chat(prompt, self.history.clone())
            .await
            .map_err(|e| NikaError::AgentExecutionError {
                task_id: self.task_id.clone(),
                reason: e.to_string(),
            })?;

        // Update history with this turn
        self.history.push(Message::user(prompt));
        self.history.push(Message::assistant(&response));

        // Determine status
        let status = if self.check_stop_conditions(&response) {
            RigAgentStatus::StopConditionMet
        } else {
            RigAgentStatus::NaturalCompletion
        };

        // Emit completion
        let stop_reason = status.as_canonical_str();
        let metadata = AgentTurnMetadata::text_only(&response, stop_reason);

        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index,
            kind: stop_reason.to_string(),
            metadata: Some(metadata),
        });

        Ok(RigAgentLoopResult {
            status,
            turns: turn_index as usize,
            final_output: serde_json::json!({ "response": response }),
            total_tokens: 0,
        })
    }

    /// Continue conversation with OpenAI (v0.6)
    async fn chat_continue_openai(
        &mut self,
        prompt: &str,
    ) -> Result<RigAgentLoopResult, NikaError> {
        let client = openai::Client::from_env();
        let model_name = self.params.model.as_deref().unwrap_or("gpt-4o");
        let model = client.completion_model(model_name);

        let turn_index = (self.history.len() / 2 + 1) as u32;

        // Emit start event
        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index,
            kind: "started".to_string(),
            metadata: None,
        });

        // Build agent and chat with history
        let agent = AgentBuilder::new(model)
            .preamble(self.params.system.as_deref().unwrap_or(""))
            .build();

        let response = agent
            .chat(prompt, self.history.clone())
            .await
            .map_err(|e| NikaError::AgentExecutionError {
                task_id: self.task_id.clone(),
                reason: e.to_string(),
            })?;

        // Update history with this turn
        self.history.push(Message::user(prompt));
        self.history.push(Message::assistant(&response));

        // Determine status
        let status = if self.check_stop_conditions(&response) {
            RigAgentStatus::StopConditionMet
        } else {
            RigAgentStatus::NaturalCompletion
        };

        // Emit completion
        let stop_reason = status.as_canonical_str();
        let metadata = AgentTurnMetadata::text_only(&response, stop_reason);

        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index,
            kind: stop_reason.to_string(),
            metadata: Some(metadata),
        });

        Ok(RigAgentLoopResult {
            status,
            turns: turn_index as usize,
            final_output: serde_json::json!({ "response": response }),
            total_tokens: 0,
        })
    }

    /// Continue conversation with Mistral (v0.6)
    async fn chat_continue_mistral(
        &mut self,
        prompt: &str,
    ) -> Result<RigAgentLoopResult, NikaError> {
        use rig::completion::Chat;

        let client = rig::providers::mistral::Client::from_env();
        let model_name = self
            .params
            .model
            .as_deref()
            .unwrap_or(rig::providers::mistral::MISTRAL_LARGE);
        let agent = client.agent(model_name).build();

        let turn_index = (self.history.len() / 2 + 1) as u32;

        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index,
            kind: "chat_continue_mistral".to_string(),
            metadata: None,
        });

        let response = agent
            .chat(prompt, self.history.clone())
            .await
            .map_err(|e| NikaError::AgentExecutionError {
                task_id: self.task_id.clone(),
                reason: format!("mistral chat error: {}", e),
            })?;

        self.history.push(Message::user(prompt));
        self.history.push(Message::assistant(&response));

        let status = RigAgentStatus::NaturalCompletion;
        let metadata = AgentTurnMetadata::text_only(&response, "end_turn");

        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index,
            kind: "chat_continue_mistral".to_string(),
            metadata: Some(metadata),
        });

        Ok(RigAgentLoopResult {
            status,
            turns: turn_index as usize,
            final_output: serde_json::json!({ "response": response }),
            total_tokens: 0,
        })
    }

    /// Continue conversation with Groq (v0.6)
    async fn chat_continue_groq(&mut self, prompt: &str) -> Result<RigAgentLoopResult, NikaError> {
        use rig::completion::Chat;

        let client = rig::providers::groq::Client::from_env();
        let model_name = self
            .params
            .model
            .as_deref()
            .unwrap_or("llama-3.3-70b-versatile");
        let agent = client.agent(model_name).build();

        let turn_index = (self.history.len() / 2 + 1) as u32;

        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index,
            kind: "chat_continue_groq".to_string(),
            metadata: None,
        });

        let response = agent
            .chat(prompt, self.history.clone())
            .await
            .map_err(|e| NikaError::AgentExecutionError {
                task_id: self.task_id.clone(),
                reason: format!("groq chat error: {}", e),
            })?;

        self.history.push(Message::user(prompt));
        self.history.push(Message::assistant(&response));

        let status = RigAgentStatus::NaturalCompletion;
        let metadata = AgentTurnMetadata::text_only(&response, "end_turn");

        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index,
            kind: "chat_continue_groq".to_string(),
            metadata: Some(metadata),
        });

        Ok(RigAgentLoopResult {
            status,
            turns: turn_index as usize,
            final_output: serde_json::json!({ "response": response }),
            total_tokens: 0,
        })
    }

    /// Continue conversation with DeepSeek (v0.6)
    async fn chat_continue_deepseek(
        &mut self,
        prompt: &str,
    ) -> Result<RigAgentLoopResult, NikaError> {
        use rig::completion::Chat;

        let client = rig::providers::deepseek::Client::from_env();
        let model_name = self
            .params
            .model
            .as_deref()
            .unwrap_or(rig::providers::deepseek::DEEPSEEK_CHAT);
        let agent = client.agent(model_name).build();

        let turn_index = (self.history.len() / 2 + 1) as u32;

        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index,
            kind: "chat_continue_deepseek".to_string(),
            metadata: None,
        });

        let response = agent
            .chat(prompt, self.history.clone())
            .await
            .map_err(|e| NikaError::AgentExecutionError {
                task_id: self.task_id.clone(),
                reason: format!("deepseek chat error: {}", e),
            })?;

        self.history.push(Message::user(prompt));
        self.history.push(Message::assistant(&response));

        let status = RigAgentStatus::NaturalCompletion;
        let metadata = AgentTurnMetadata::text_only(&response, "end_turn");

        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index,
            kind: "chat_continue_deepseek".to_string(),
            metadata: Some(metadata),
        });

        Ok(RigAgentLoopResult {
            status,
            turns: turn_index as usize,
            final_output: serde_json::json!({ "response": response }),
            total_tokens: 0,
        })
    }

    /// Continue conversation with Ollama (v0.6)
    async fn chat_continue_ollama(
        &mut self,
        prompt: &str,
    ) -> Result<RigAgentLoopResult, NikaError> {
        use rig::completion::Chat;

        let client = rig::providers::ollama::Client::from_env();
        let model_name = self.params.model.as_deref().unwrap_or("llama3.2");
        let agent = client.agent(model_name).build();

        let turn_index = (self.history.len() / 2 + 1) as u32;

        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index,
            kind: "chat_continue_ollama".to_string(),
            metadata: None,
        });

        let response = agent
            .chat(prompt, self.history.clone())
            .await
            .map_err(|e| NikaError::AgentExecutionError {
                task_id: self.task_id.clone(),
                reason: format!("ollama chat error: {}", e),
            })?;

        self.history.push(Message::user(prompt));
        self.history.push(Message::assistant(&response));

        let status = RigAgentStatus::NaturalCompletion;
        let metadata = AgentTurnMetadata::text_only(&response, "end_turn");

        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index,
            kind: "chat_continue_ollama".to_string(),
            metadata: Some(metadata),
        });

        Ok(RigAgentLoopResult {
            status,
            turns: turn_index as usize,
            final_output: serde_json::json!({ "response": response }),
            total_tokens: 0,
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

        // Get model name (default to claude-sonnet-4-20250514)
        let model_name = self
            .params
            .model
            .as_deref()
            .unwrap_or("claude-sonnet-4-20250514");
        let model = client.completion_model(model_name);

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

        // Get model name (default to claude-sonnet-4-20250514)
        let model_name = self
            .params
            .model
            .as_deref()
            .unwrap_or("claude-sonnet-4-20250514");
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

    /// Run the agent loop with the OpenAI provider
    ///
    /// This method uses rig-core's OpenAI client for actual execution.
    /// Requires OPENAI_API_KEY environment variable to be set.
    ///
    /// # Note
    /// This method takes `&mut self` because tools are consumed (moved to rig's AgentBuilder).
    /// The agent loop is designed for single-use execution.
    pub async fn run_openai(&mut self) -> Result<RigAgentLoopResult, NikaError> {
        // Create OpenAI client from environment
        let client = openai::Client::from_env();

        // Get model name (default to gpt-4o)
        let model_name = self.params.model.as_deref().unwrap_or("gpt-4o");
        let model = client.completion_model(model_name);

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

        // Emit completion event
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
            turns: 1,
            final_output: serde_json::json!({ "response": response }),
            total_tokens: 0, // Token tracking requires response metadata
        })
    }

    /// Run the agent loop with the best available provider (v0.6: expanded)
    ///
    /// Provider selection order:
    /// 1. Check AgentParams.provider field
    /// 2. Check ANTHROPIC_API_KEY env var → use Claude
    /// 3. Check OPENAI_API_KEY env var → use OpenAI
    /// 4. Check MISTRAL_API_KEY env var → use Mistral
    /// 5. Check GROQ_API_KEY env var → use Groq
    /// 6. Check DEEPSEEK_API_KEY env var → use DeepSeek
    /// 7. Check OLLAMA_API_BASE_URL env var → use Ollama
    /// 8. Error if no provider available
    ///
    /// # Note
    /// This is the recommended method for production use.
    pub async fn run_auto(&mut self) -> Result<RigAgentLoopResult, NikaError> {
        // Check explicit provider from params
        if let Some(ref provider) = self.params.provider {
            match provider.to_lowercase().as_str() {
                "claude" | "anthropic" => return self.run_claude().await,
                "openai" | "gpt" => return self.run_openai().await,
                "mistral" => return self.run_mistral().await,
                "ollama" | "local" => return self.run_ollama().await,
                "groq" => return self.run_groq().await,
                "deepseek" => return self.run_deepseek().await,
                other => {
                    return Err(NikaError::AgentValidationError {
                        reason: format!(
                            "Unknown provider: '{}'. Use 'claude', 'openai', 'mistral', 'ollama', 'groq', or 'deepseek'.",
                            other
                        ),
                    });
                }
            }
        }

        // Auto-detect based on available API keys (v0.6: expanded detection)
        // Helper: check env var exists and is non-empty
        let has_key = |key: &str| std::env::var(key).is_ok_and(|v| !v.is_empty());

        if has_key("ANTHROPIC_API_KEY") {
            return self.run_claude().await;
        }

        if has_key("OPENAI_API_KEY") {
            return self.run_openai().await;
        }

        if has_key("MISTRAL_API_KEY") {
            return self.run_mistral().await;
        }

        if has_key("GROQ_API_KEY") {
            return self.run_groq().await;
        }

        if has_key("DEEPSEEK_API_KEY") {
            return self.run_deepseek().await;
        }

        if has_key("OLLAMA_API_BASE_URL") {
            return self.run_ollama().await;
        }

        Err(NikaError::AgentValidationError {
            reason: "No API key found. Set one of: ANTHROPIC_API_KEY, OPENAI_API_KEY, MISTRAL_API_KEY, GROQ_API_KEY, DEEPSEEK_API_KEY, or OLLAMA_API_BASE_URL.".to_string(),
        })
    }

    // =========================================================================
    // v0.6: Additional Provider Methods
    // =========================================================================

    /// Run with Mistral provider (requires MISTRAL_API_KEY)
    pub async fn run_mistral(&mut self) -> Result<RigAgentLoopResult, NikaError> {
        let model_name = self
            .params
            .model
            .clone()
            .unwrap_or_else(|| rig::providers::mistral::MISTRAL_LARGE.to_string());
        let client = rig::providers::mistral::Client::from_env();
        self.run_generic_provider_impl(client, &model_name).await
    }

    /// Run with Ollama local provider (requires OLLAMA_API_BASE_URL or uses localhost:11434)
    pub async fn run_ollama(&mut self) -> Result<RigAgentLoopResult, NikaError> {
        let model_name = self
            .params
            .model
            .clone()
            .unwrap_or_else(|| "llama3.2".to_string());
        // Ollama uses from_env() which reads OLLAMA_API_BASE_URL (default: http://localhost:11434)
        let client = rig::providers::ollama::Client::from_env();
        self.run_generic_provider_impl(client, &model_name).await
    }

    /// Run with Groq provider (requires GROQ_API_KEY)
    pub async fn run_groq(&mut self) -> Result<RigAgentLoopResult, NikaError> {
        let model_name = self
            .params
            .model
            .clone()
            .unwrap_or_else(|| "llama-3.1-70b-versatile".to_string());
        let client = rig::providers::groq::Client::from_env();
        self.run_generic_provider_impl(client, &model_name).await
    }

    /// Run with DeepSeek provider (requires DEEPSEEK_API_KEY)
    pub async fn run_deepseek(&mut self) -> Result<RigAgentLoopResult, NikaError> {
        let model_name = self
            .params
            .model
            .clone()
            .unwrap_or_else(|| "deepseek-chat".to_string());
        let client = rig::providers::deepseek::Client::from_env();
        self.run_generic_provider_impl(client, &model_name).await
    }

    /// Generic provider runner implementation (v0.6)
    ///
    /// Uses rig-core's unified ProviderClient + CompletionClient interface.
    async fn run_generic_provider_impl<C>(
        &mut self,
        client: C,
        model_name: &str,
    ) -> Result<RigAgentLoopResult, NikaError>
    where
        C: CompletionClient,
    {
        let model = client.completion_model(model_name);

        // Take ownership of tools
        let tools = std::mem::take(&mut self.tools);
        let max_turns = self.params.max_turns.unwrap_or(10) as usize;
        let prompt = self.params.prompt.clone();

        // Emit start event
        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index: 1,
            kind: "started".to_string(),
            metadata: None,
        });

        // Build and run agent
        let response: String = if tools.is_empty() {
            let agent = AgentBuilder::new(model).preamble(&prompt).build();

            agent
                .prompt(&prompt)
                .max_turns(max_turns)
                .await
                .map_err(|e| NikaError::AgentExecutionError {
                    task_id: self.task_id.clone(),
                    reason: e.to_string(),
                })?
        } else {
            let agent = AgentBuilder::new(model)
                .preamble(&prompt)
                .tools(tools)
                .build();

            agent
                .prompt(&prompt)
                .max_turns(max_turns)
                .await
                .map_err(|e| NikaError::AgentExecutionError {
                    task_id: self.task_id.clone(),
                    reason: e.to_string(),
                })?
        };

        // Determine status
        let status = if self.check_stop_conditions(&response) {
            RigAgentStatus::StopConditionMet
        } else {
            RigAgentStatus::NaturalCompletion
        };

        // Emit completion event
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
            turns: 1,
            final_output: serde_json::json!({ "response": response }),
            total_tokens: 0,
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
