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

use std::path::PathBuf;
use std::sync::Arc;

use futures::StreamExt;
use rig::agent::AgentBuilder;
use rig::agent::{MultiTurnStreamItem, StreamingResult as RigStreamingResult};
use rig::client::{CompletionClient, ProviderClient};
use rig::completion::{Chat, CompletionModel as _, GetTokenUsage, Prompt};
use rig::message::{Message, ReasoningContent, ToolChoice as RigToolChoice};
use rig::providers::{anthropic, openai};
use rig::streaming::{StreamedAssistantContent, StreamingPrompt};
use rustc_hash::FxHashMap;
use serde_json::Value;
use tokio::time::timeout;

use crate::ast::guardrails::run_guardrails;
use crate::ast::AgentParams;
use crate::ast::ToolChoice as NikaToolChoice;
use crate::error::NikaError;
use crate::event::{AgentTurnMetadata, EventKind, EventLog};
use crate::mcp::McpClient;
use crate::provider::rig::{NikaMcpTool, NikaMcpToolDef};
use crate::runtime::SkillInjector;
use crate::util::STREAM_CHUNK_TIMEOUT;

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/// Status of the rig-based agent execution
#[derive(Debug, Clone, PartialEq)]
pub enum RigAgentStatus {
    /// Agent completed naturally (no more tool calls)
    NaturalCompletion,
    /// Agent called nika:complete tool explicitly (v0.21)
    ExplicitCompletion,
    /// Agent completed with confidence above threshold (v0.22)
    HighConfidence(f64),
    /// Agent completed with confidence below threshold (v0.22)
    LowConfidence(f64),
    /// Agent completed but flagged for review via routing (v0.22)
    FlaggedForReview(f64),
    /// Agent escalated to human/other agent via routing (v0.22)
    Escalated(f64),
    /// Stop condition matched in output
    StopConditionMet,
    /// Maximum turns reached
    MaxTurnsReached,
    /// Token budget exceeded
    TokenBudgetExceeded,
    /// Cost budget exceeded (v0.24)
    CostLimitReached,
    /// Duration limit exceeded (v0.24)
    DurationLimitReached,
    /// Partial completion with checkpoint saved (v0.24)
    PartialCompletion,
    /// Agent failed with error
    Failed,
}

impl RigAgentStatus {
    /// Convert to canonical snake_case string for event logging.
    /// Aligns with Anthropic API's stop_reason values.
    pub fn as_canonical_str(&self) -> &'static str {
        match self {
            Self::NaturalCompletion => "end_turn",
            Self::ExplicitCompletion => "tool_complete", // v0.21
            Self::HighConfidence(_) => "tool_complete_high", // v0.22
            Self::LowConfidence(_) => "tool_complete_low", // v0.22
            Self::FlaggedForReview(_) => "tool_complete_flagged", // v0.22 routing
            Self::Escalated(_) => "escalated",           // v0.22 routing
            Self::StopConditionMet => "stop_sequence",
            Self::MaxTurnsReached => "max_turns",
            Self::TokenBudgetExceeded => "max_tokens",
            Self::CostLimitReached => "max_cost",        // v0.24
            Self::DurationLimitReached => "max_duration", // v0.24
            Self::PartialCompletion => "partial",        // v0.24
            Self::Failed => "error",
        }
    }

    /// Check if this status represents successful completion (v0.22)
    pub fn is_completed(&self) -> bool {
        matches!(
            self,
            Self::NaturalCompletion
                | Self::ExplicitCompletion
                | Self::HighConfidence(_)
                | Self::FlaggedForReview(_) // Accepted, just flagged
                | Self::StopConditionMet
        )
    }

    /// Check if this status requires retry (v0.22)
    pub fn requires_retry(&self) -> bool {
        matches!(self, Self::LowConfidence(_))
    }

    /// Check if this status was escalated (v0.22)
    pub fn is_escalated(&self) -> bool {
        matches!(self, Self::Escalated(_))
    }

    /// Check if this status is flagged for review (v0.22)
    pub fn is_flagged(&self) -> bool {
        matches!(self, Self::FlaggedForReview(_))
    }

    /// Get confidence value if available (v0.22)
    pub fn confidence(&self) -> Option<f64> {
        match self {
            Self::HighConfidence(c)
            | Self::LowConfidence(c)
            | Self::FlaggedForReview(c)
            | Self::Escalated(c) => Some(*c),
            _ => None,
        }
    }

    /// Check if a resource limit was reached (v0.24)
    pub fn is_limit_reached(&self) -> bool {
        matches!(
            self,
            Self::MaxTurnsReached
                | Self::TokenBudgetExceeded
                | Self::CostLimitReached
                | Self::DurationLimitReached
        )
    }

    /// Check if this is a partial completion (v0.24)
    pub fn is_partial(&self) -> bool {
        matches!(self, Self::PartialCompletion)
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
    /// Confidence level from nika:complete (v0.22)
    pub confidence: Option<f64>,
    /// Number of retries due to low confidence (v0.22)
    pub retry_count: u32,
    /// Whether all guardrails passed (v0.23)
    pub guardrails_passed: bool,
    /// Total cost in USD (v0.24)
    pub cost_usd: f64,
    /// Partial result if limit reached (v0.24)
    pub partial_result: Option<super::partial::PartialResult>,
}

/// Result from streaming execution with token tracking.
///
/// Used internally by streaming helpers to capture both the response
/// content and token usage from `StreamedAssistantContent::Final`.
#[derive(Debug, Clone)]
struct StreamingResult {
    /// The accumulated response text
    response: String,
    /// Input tokens used (from token_usage())
    input_tokens: u32,
    /// Output tokens used (from token_usage())
    output_tokens: u32,
    /// Optional thinking/reasoning content (Claude extended thinking)
    thinking: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════════════
// ToolChoice Conversion (v0.8.0)
// ═══════════════════════════════════════════════════════════════════════════

/// Convert Nika's ToolChoice to rig-core's ToolChoice.
///
/// Nika's ToolChoice maps directly to rig-core's ToolChoice variants:
/// - `Auto` → LLM decides when to use tools (default)
/// - `Required` → Must use at least one tool per turn
/// - `None` → Never use tools (text-only response)
impl From<NikaToolChoice> for RigToolChoice {
    fn from(choice: NikaToolChoice) -> Self {
        match choice {
            NikaToolChoice::Auto => RigToolChoice::Auto,
            NikaToolChoice::Required => RigToolChoice::Required,
            NikaToolChoice::None => RigToolChoice::None,
        }
    }
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
    /// Conversation history for multi-turn chat (v0.6).
    ///
    /// NOTE: This Vec is cloned on each `chat()` call because rig-core's API
    /// takes ownership. The clone is necessary to preserve history for future turns.
    /// Pre-allocated with capacity based on `max_turns` to minimize reallocations.
    history: Vec<Message>,
    /// Optional streaming channel for real-time token display (v0.8.1 TUI integration)
    stream_tx: Option<tokio::sync::mpsc::Sender<crate::provider::rig::StreamChunk>>,
    /// Skill injector for loading and caching skills (v0.15.4)
    skill_injector: Option<Arc<SkillInjector>>,
    /// Skills map from workflow definition (skill_name -> path)
    skills_map: Option<std::collections::HashMap<String, String>>,
    /// Base directory for resolving skill paths
    base_dir: Option<PathBuf>,
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

        // Add builtin nika:* tools (v0.12.0)
        // These are always available to agents for workflow control and observability.
        // LogTool and EmitTool now emit EventKind::Log and EventKind::Custom events.
        use super::builtin::{
            AssertTool, CompleteTool, EmitTool, LogTool, NikaBuiltinToolAdapter, PromptTool,
            RunTool, SleepTool,
        };

        // Create Arc wrappers for sharing with builtin tools (v0.12.0)
        // EventLog is Clone with Arc internals, so this is cheap.
        let event_log_arc = Arc::new(event_log.clone());
        let task_id_arc: Arc<str> = task_id.as_str().into();

        tools.push(Box::new(NikaBuiltinToolAdapter::new(Arc::new(SleepTool))));
        // v0.12.0: LogTool now emits EventKind::Log to EventLog
        tools.push(Box::new(
            NikaBuiltinToolAdapter::new(Arc::new(LogTool))
                .with_event_log(Arc::clone(&event_log_arc), Arc::clone(&task_id_arc)),
        ));
        // v0.12.0: EmitTool now emits EventKind::Custom to EventLog
        tools.push(Box::new(
            NikaBuiltinToolAdapter::new(Arc::new(EmitTool))
                .with_event_log(Arc::clone(&event_log_arc), Arc::clone(&task_id_arc)),
        ));
        tools.push(Box::new(NikaBuiltinToolAdapter::new(Arc::new(AssertTool))));
        tools.push(Box::new(NikaBuiltinToolAdapter::new(Arc::new(
            PromptTool::default(),
        ))));
        tools.push(Box::new(NikaBuiltinToolAdapter::new(Arc::new(RunTool))));
        // v0.21: CompleteTool for explicit completion mode
        tools.push(Box::new(NikaBuiltinToolAdapter::new(Arc::new(
            CompleteTool,
        ))));

        // PERF: Pre-allocate history capacity based on max_turns.
        // Each turn adds 2 messages (user + assistant), so capacity = max_turns * 2.
        // This reduces reallocations during conversation.
        let history_capacity = params.max_turns.unwrap_or(10) as usize * 2;

        Ok(Self {
            task_id,
            params,
            event_log,
            mcp_clients,
            tools,
            history: Vec::with_capacity(history_capacity),
            stream_tx: None,
            skill_injector: None,
            skills_map: None,
            base_dir: None,
        })
    }

    /// Set streaming channel for real-time token display (v0.8.1 TUI integration)
    ///
    /// When set, tokens will be sent to this channel as they arrive during streaming.
    /// This enables Claude Code-like real-time text display in the TUI.
    pub fn with_stream_tx(
        mut self,
        tx: tokio::sync::mpsc::Sender<crate::provider::rig::StreamChunk>,
    ) -> Self {
        self.stream_tx = Some(tx);
        self
    }

    /// Configure skill injection for this agent (v0.15.4)
    ///
    /// When set, skills defined in the workflow are loaded and prepended to
    /// the agent's system prompt before LLM calls.
    ///
    /// # Arguments
    /// * `injector` - Shared SkillInjector instance (with DashMap cache)
    /// * `skills_map` - Mapping of skill names to file paths from workflow YAML
    /// * `base_dir` - Base directory for resolving relative skill paths
    ///
    /// # Example
    /// ```ignore
    /// let agent = RigAgentLoop::new(task_id, params, log, mcp)?
    ///     .with_skills(
    ///         Arc::new(SkillInjector::new()),
    ///         skills_map,
    ///         PathBuf::from("/path/to/workflow"),
    ///     );
    /// ```
    pub fn with_skills(
        mut self,
        injector: Arc<SkillInjector>,
        skills_map: std::collections::HashMap<String, String>,
        base_dir: PathBuf,
    ) -> Self {
        self.skill_injector = Some(injector);
        self.skills_map = Some(skills_map);
        self.base_dir = Some(base_dir);
        self
    }

    // =========================================================================
    // v0.15.4: Skill Injection
    // =========================================================================

    /// Inject skills into the system prompt (v0.15.4)
    ///
    /// If skills are configured via `with_skills()` and the agent has skills
    /// defined in `AgentParams.skills`, this method loads and prepends skill
    /// content to the base system prompt.
    ///
    /// # Returns
    /// - Enhanced prompt with skill content prepended, or
    /// - Original system prompt if no skills configured
    async fn inject_skills_into_prompt(&self) -> Result<String, NikaError> {
        let base_prompt = self.params.system.as_deref();

        // Check if skill injection is configured
        let (Some(injector), Some(skills_map), Some(base_dir)) =
            (&self.skill_injector, &self.skills_map, &self.base_dir)
        else {
            // Not configured - return base prompt as-is
            return Ok(base_prompt.unwrap_or_default().to_string());
        };

        // Check if agent has skills defined
        let Some(skill_names) = &self.params.skills else {
            // No skills on this agent - return base prompt
            return Ok(base_prompt.unwrap_or_default().to_string());
        };

        if skill_names.is_empty() {
            return Ok(base_prompt.unwrap_or_default().to_string());
        }

        // Convert Vec<String> to &[&str] for the inject() API
        let skill_refs: Vec<&str> = skill_names.iter().map(|s| s.as_str()).collect();

        // Inject skills into the prompt
        injector
            .inject(base_prompt, &skill_refs, skills_map, base_dir)
            .await
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
    ///
    /// **Note:** Token tracking is not available for chat methods.
    /// The rig-core `Chat` trait returns only `String`, not token metadata.
    /// Use `run_claude()` for single-turn requests with full token tracking.
    async fn chat_continue_claude(
        &mut self,
        prompt: &str,
    ) -> Result<RigAgentLoopResult, NikaError> {
        let client = anthropic::Client::from_env();
        let model_name = self.params.model.as_deref().unwrap_or("claude-sonnet-4-6");
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
        // Anthropic requires max_tokens to be set explicitly
        // Inject skills into system prompt if configured (v0.15.4)
        let preamble = self.inject_skills_into_prompt().await?;
        let mut builder = AgentBuilder::new(model)
            .preamble(&preamble)
            .max_tokens(8192);

        // Apply temperature using native rig-core method (v0.8.0)
        if let Some(temp) = self.params.effective_temperature() {
            builder = builder.temperature(f64::from(temp));
        }

        // Apply tool_choice only if explicitly set (v0.8.0 optimization)
        // Skipping redundant .tool_choice(Auto) - rig-core uses Auto by default
        // See AgentParams::has_explicit_tool_choice() for provider compatibility notes
        if self.params.has_explicit_tool_choice() {
            let tool_choice = self.params.effective_tool_choice();
            builder = builder.tool_choice(tool_choice.into());
        }

        let agent = builder.build();

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

        // Determine status (v0.21: uses determine_status for completion detection)
        let status = self.determine_status(&response);

        // Emit completion
        let stop_reason = status.as_canonical_str();
        let metadata = AgentTurnMetadata::text_only(&response, stop_reason);

        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index,
            kind: stop_reason.to_string(),
            metadata: Some(metadata),
        });

        // Check guardrails (v0.23)
        let guardrails_passed = self.check_guardrails(&response);

        Ok(RigAgentLoopResult {
            status: status.clone(),
            turns: turn_index as usize,
            final_output: serde_json::json!({ "response": response }),
            total_tokens: 0,
            confidence: status.confidence(),
            retry_count: 0,
            guardrails_passed,
            cost_usd: 0.0,
            partial_result: None,
        })
    }

    /// Continue conversation with OpenAI (v0.6)
    ///
    /// **Note:** Token tracking is not available for chat methods.
    /// The rig-core `Chat` trait returns only `String`, not token metadata.
    /// Use `run_openai()` for single-turn requests with full token tracking.
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
        // Inject skills into system prompt if configured (v0.15.4)
        let preamble = self.inject_skills_into_prompt().await?;
        let mut builder = AgentBuilder::new(model)
            .preamble(&preamble)
            .max_tokens(8192);

        // Apply temperature using native rig-core method (v0.8.0)
        if let Some(temp) = self.params.effective_temperature() {
            builder = builder.temperature(f64::from(temp));
        }

        // Apply tool_choice only if explicitly set (v0.8.0 optimization)
        // Skipping redundant .tool_choice(Auto) - rig-core uses Auto by default
        if self.params.has_explicit_tool_choice() {
            let tool_choice = self.params.effective_tool_choice();
            builder = builder.tool_choice(tool_choice.into());
        }

        let agent = builder.build();

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

        // Determine status (v0.21: uses determine_status for completion detection)
        let status = self.determine_status(&response);

        // Emit completion
        let stop_reason = status.as_canonical_str();
        let metadata = AgentTurnMetadata::text_only(&response, stop_reason);

        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index,
            kind: stop_reason.to_string(),
            metadata: Some(metadata),
        });

        // Check guardrails (v0.23)
        let guardrails_passed = self.check_guardrails(&response);

        Ok(RigAgentLoopResult {
            status: status.clone(),
            turns: turn_index as usize,
            final_output: serde_json::json!({ "response": response }),
            total_tokens: 0,
            confidence: status.confidence(),
            retry_count: 0,
            guardrails_passed,
            cost_usd: 0.0,
            partial_result: None,
        })
    }

    /// Continue conversation with Mistral (v0.6)
    ///
    /// **Note:** Token tracking is not available for chat methods.
    /// Use `run_mistral()` for single-turn requests with full token tracking.
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
        let agent = client.agent(model_name).max_tokens(8192).build();

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

        // Check guardrails (v0.23)
        let guardrails_passed = self.check_guardrails(&response);

        Ok(RigAgentLoopResult {
            status: status.clone(),
            turns: turn_index as usize,
            final_output: serde_json::json!({ "response": response }),
            total_tokens: 0,
            confidence: status.confidence(),
            retry_count: 0,
            guardrails_passed,
            cost_usd: 0.0,
            partial_result: None,
        })
    }

    /// Continue conversation with Groq (v0.6)
    ///
    /// **Note:** Token tracking is not available for chat methods.
    /// Use `run_groq()` for single-turn requests with full token tracking.
    async fn chat_continue_groq(&mut self, prompt: &str) -> Result<RigAgentLoopResult, NikaError> {
        use rig::completion::Chat;

        let client = rig::providers::groq::Client::from_env();
        let model_name = self
            .params
            .model
            .as_deref()
            .unwrap_or("llama-3.3-70b-versatile");
        let agent = client.agent(model_name).max_tokens(8192).build();

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

        // Check guardrails (v0.23)
        let guardrails_passed = self.check_guardrails(&response);

        Ok(RigAgentLoopResult {
            status: status.clone(),
            turns: turn_index as usize,
            final_output: serde_json::json!({ "response": response }),
            total_tokens: 0,
            confidence: status.confidence(),
            retry_count: 0,
            guardrails_passed,
            cost_usd: 0.0,
            partial_result: None,
        })
    }

    /// Continue conversation with DeepSeek (v0.6)
    ///
    /// **Note:** Token tracking is not available for chat methods.
    /// Use `run_deepseek()` for single-turn requests with full token tracking.
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
        let agent = client.agent(model_name).max_tokens(8192).build();

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

        // Check guardrails (v0.23)
        let guardrails_passed = self.check_guardrails(&response);

        Ok(RigAgentLoopResult {
            status: status.clone(),
            turns: turn_index as usize,
            final_output: serde_json::json!({ "response": response }),
            total_tokens: 0,
            confidence: status.confidence(),
            retry_count: 0,
            guardrails_passed,
            cost_usd: 0.0,
            partial_result: None,
        })
    }

    /// Continue conversation with Ollama (v0.6)
    ///
    /// **Note:** Token tracking is not available for chat methods.
    /// Use `run_ollama()` for single-turn requests with full token tracking.
    async fn chat_continue_ollama(
        &mut self,
        prompt: &str,
    ) -> Result<RigAgentLoopResult, NikaError> {
        use rig::completion::Chat;

        let client = rig::providers::ollama::Client::from_env();
        let model_name = self.params.model.as_deref().unwrap_or("llama3.2");
        let agent = client.agent(model_name).max_tokens(8192).build();

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

        // Check guardrails (v0.23)
        let guardrails_passed = self.check_guardrails(&response);

        Ok(RigAgentLoopResult {
            status: status.clone(),
            turns: turn_index as usize,
            final_output: serde_json::json!({ "response": response }),
            total_tokens: 0,
            confidence: status.confidence(),
            retry_count: 0,
            guardrails_passed,
            cost_usd: 0.0,
            partial_result: None,
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

    // =========================================================================
    // Streaming Helpers (v0.7.2 - Token Tracking Migration)
    // =========================================================================

    /// Execute a completion request with streaming, capturing tokens.
    ///
    /// This is the core streaming helper used for simple completions (no tools).
    /// It handles the stream processing loop and extracts token usage from
    /// `StreamedAssistantContent::Final`.
    ///
    /// # Type Parameters
    /// - `M`: A rig completion model that supports streaming
    ///
    /// # Returns
    /// A `StreamingResult` containing the response text and token counts.
    async fn stream_completion_with_tokens<M>(
        &self,
        model: &M,
        prompt: &str,
        system: Option<&str>,
    ) -> Result<StreamingResult, NikaError>
    where
        M: rig::completion::CompletionModel,
        <M as rig::completion::CompletionModel>::Response: Send,
    {
        // Build completion request
        let mut request_builder = model.completion_request(prompt);
        if let Some(sys) = system {
            request_builder = request_builder.preamble(sys.to_string());
        }

        // Apply temperature if specified using native rig-core method (v0.8.0)
        if let Some(temp) = self.params.effective_temperature() {
            request_builder = request_builder.temperature(f64::from(temp));
        }

        let request = request_builder.max_tokens(8192).build();

        // Execute streaming request
        let mut stream =
            model
                .stream(request)
                .await
                .map_err(|e| NikaError::AgentExecutionError {
                    task_id: self.task_id.clone(),
                    reason: format!("Streaming request failed: {}", e),
                })?;

        // Accumulate response and extract tokens
        // PERF: Pre-allocate with expected capacity to avoid reallocation
        let mut response_parts: Vec<String> = Vec::with_capacity(16);
        let mut thinking_parts: Vec<String> = Vec::with_capacity(8);
        let mut input_tokens: u32 = 0;
        let mut output_tokens: u32 = 0;

        // Stream chunks with timeout protection to prevent infinite hangs
        loop {
            let chunk_result = match timeout(STREAM_CHUNK_TIMEOUT, stream.next()).await {
                Ok(Some(result)) => result,
                Ok(None) => break, // Stream ended normally
                Err(_elapsed) => {
                    return Err(NikaError::Timeout {
                        operation: "streaming chunk".to_string(),
                        duration_ms: STREAM_CHUNK_TIMEOUT.as_millis() as u64,
                    });
                }
            };

            match chunk_result {
                Ok(content) => match content {
                    StreamedAssistantContent::Text(text) => {
                        // v0.8.1: Send token to TUI for real-time display
                        if let Some(ref tx) = self.stream_tx {
                            let _ = tx.try_send(crate::provider::rig::StreamChunk::Token(
                                text.text.clone(),
                            ));
                        }
                        response_parts.push(text.text);
                    }
                    StreamedAssistantContent::ReasoningDelta { reasoning, .. } => {
                        // v0.8.1: Send thinking content to TUI
                        if let Some(ref tx) = self.stream_tx {
                            let _ = tx.try_send(crate::provider::rig::StreamChunk::Thinking(
                                reasoning.clone(),
                            ));
                        }
                        thinking_parts.push(reasoning);
                    }
                    StreamedAssistantContent::Reasoning(reasoning) => {
                        // Final reasoning block - extract text from content blocks
                        for block in reasoning.content {
                            if let ReasoningContent::Text { text, .. } = block {
                                // v0.8.1: Send thinking to TUI
                                if let Some(ref tx) = self.stream_tx {
                                    let _ = tx.try_send(
                                        crate::provider::rig::StreamChunk::Thinking(text.clone()),
                                    );
                                }
                                thinking_parts.push(text);
                            }
                        }
                    }
                    StreamedAssistantContent::Final(final_resp) => {
                        // Extract token usage from final response
                        if let Some(usage) = final_resp.token_usage() {
                            input_tokens = usage.input_tokens as u32;
                            output_tokens = usage.output_tokens as u32;
                            // v0.8.1: Send final metrics to TUI
                            if let Some(ref tx) = self.stream_tx {
                                let _ = tx.try_send(crate::provider::rig::StreamChunk::Metrics {
                                    input_tokens: usage.input_tokens,
                                    output_tokens: usage.output_tokens,
                                });
                            }
                        }
                    }
                    _ => {
                        // Tool calls and other events - handled elsewhere
                    }
                },
                Err(e) => {
                    return Err(NikaError::AgentExecutionError {
                        task_id: self.task_id.clone(),
                        reason: format!("Stream chunk failed: {}", e),
                    });
                }
            }
        }

        Ok(StreamingResult {
            response: response_parts.concat(),
            input_tokens,
            output_tokens,
            thinking: if thinking_parts.is_empty() {
                None
            } else {
                Some(thinking_parts.concat())
            },
        })
    }

    /// Execute agent with tools using streaming for token tracking (when possible).
    ///
    /// This handles the case where we need both tool calling AND token tracking.
    ///
    /// **Strategy:**
    /// - No tools: Use `model.stream()` for pure streaming with full token tracking
    /// - With tools: Fall back to `agent.prompt()` (tokens will be 0)
    ///
    /// **NOTE:** Waiting on rig-core streaming agent API (upstream limitation).
    /// Tracking: https://github.com/0xPlaygrounds/rig/issues (check for streaming agent RFC)
    /// When available, migrate from `agent.prompt()` to streaming agent API for full token tracking with tools.
    ///
    /// # Type Parameters
    /// - `M`: A rig completion model that supports streaming
    ///
    /// # Returns
    /// A `StreamingResult` - with accurate tokens when no tools, 0 tokens otherwise.
    async fn stream_with_tools<M>(
        &mut self,
        model: M,
        prompt: &str,
        tools: Vec<Box<dyn rig::tool::ToolDyn>>,
        max_turns: usize,
    ) -> Result<StreamingResult, NikaError>
    where
        M: rig::completion::CompletionModel + Clone + 'static,
        <M as rig::completion::CompletionModel>::Response: Send,
    {
        // Inject skills into system prompt if configured (v0.15.4)
        let preamble = self.inject_skills_into_prompt().await?;
        if tools.is_empty() {
            // No tools - use pure streaming (full token tracking)
            self.stream_completion_with_tokens(&model, prompt, Some(&preamble))
                .await
        } else {
            // v0.8.1: With tools - use REAL streaming if TUI mode (stream_tx set)
            if self.stream_tx.is_some() {
                return self
                    .stream_with_tools_streaming(model, prompt, tools, max_turns)
                    .await;
            }

            // Fallback: No TUI (CLI mode) - use blocking agent.prompt()
            // Use preamble with injected skills (v0.15.4)
            let mut builder = AgentBuilder::new(model)
                .preamble(&preamble)
                .tools(tools)
                .max_tokens(8192);

            // Apply temperature using native rig-core method (v0.8.0)
            if let Some(temp) = self.params.effective_temperature() {
                builder = builder.temperature(f64::from(temp));
            }

            // Apply tool_choice only if explicitly set (v0.8.0 optimization)
            // Skipping redundant .tool_choice(Auto) - rig-core uses Auto by default
            if self.params.has_explicit_tool_choice() {
                let tool_choice = self.params.effective_tool_choice();
                builder = builder.tool_choice(tool_choice.into());
            }

            let agent = builder.build();

            let response = agent
                .prompt(prompt)
                .max_turns(max_turns)
                .await
                .map_err(|e| NikaError::AgentExecutionError {
                    task_id: self.task_id.clone(),
                    reason: e.to_string(),
                })?;

            Ok(StreamingResult {
                response,
                input_tokens: 0, // Not available with agent.prompt()
                output_tokens: 0,
                thinking: None,
            })
        }
    }

    /// Stream agent execution with REAL-TIME token delivery (v0.8.1)
    ///
    /// Uses rig-core's `stream_prompt()` API which supports streaming
    /// even when tools are present. Sends tokens and tool calls to TUI
    /// via the `stream_tx` channel.
    ///
    /// # Key differences from stream_with_tools():
    /// - Uses `stream_prompt()` instead of `prompt()` - true streaming
    /// - Sends `StreamChunk::Token` for each text chunk
    /// - Sends `StreamChunk::McpCallStart` for each tool call
    /// - Sends `StreamChunk::Metrics` with final token counts
    async fn stream_with_tools_streaming<M>(
        &mut self,
        model: M,
        prompt: &str,
        tools: Vec<Box<dyn rig::tool::ToolDyn>>,
        max_turns: usize,
    ) -> Result<StreamingResult, NikaError>
    where
        M: rig::completion::CompletionModel + Clone + 'static,
        <M as rig::completion::CompletionModel>::Response: Send,
    {
        // Build agent with tools
        // Inject skills into system prompt if configured (v0.15.4)
        let preamble = self.inject_skills_into_prompt().await?;
        let mut builder = AgentBuilder::new(model)
            .preamble(&preamble)
            .tools(tools)
            .max_tokens(8192);

        if let Some(temp) = self.params.effective_temperature() {
            builder = builder.temperature(f64::from(temp));
        }

        if self.params.has_explicit_tool_choice() {
            let tool_choice = self.params.effective_tool_choice();
            builder = builder.tool_choice(tool_choice.into());
        }

        let agent = builder.build();

        // v0.8.1: STREAMING with stream_prompt()
        // Note: multi_turn() sets max tool call rounds (0 = single turn, >0 = multi-turn)
        // The stream is created directly, errors come from individual items
        let mut stream: RigStreamingResult<_> = agent
            .stream_prompt(prompt)
            .multi_turn(max_turns.saturating_sub(1))
            .await;

        let mut response_text = String::new();
        let mut thinking_text: Option<String> = None;
        let mut input_tokens = 0u32;
        let mut output_tokens = 0u32;
        let mut tool_count = 0u32;

        // v0.8.5: Per-chunk timeout to prevent hanging streams
        loop {
            let chunk = match timeout(STREAM_CHUNK_TIMEOUT, stream.next()).await {
                Ok(Some(chunk)) => chunk,
                Ok(None) => break, // Stream ended normally
                Err(_elapsed) => {
                    // Timeout - stream stalled
                    tracing::warn!(
                        task_id = %self.task_id,
                        timeout_secs = STREAM_CHUNK_TIMEOUT.as_secs(),
                        "Agent stream timed out waiting for chunk"
                    );
                    if let Some(ref tx) = self.stream_tx {
                        let _ = tx.try_send(crate::provider::rig::StreamChunk::Error(format!(
                            "Stream timeout: no chunk received for {}s",
                            STREAM_CHUNK_TIMEOUT.as_secs()
                        )));
                    }
                    return Err(NikaError::Timeout {
                        operation: format!("agent streaming (task: {})", self.task_id),
                        duration_ms: STREAM_CHUNK_TIMEOUT.as_millis() as u64,
                    });
                }
            };

            match chunk {
                Ok(item) => match item {
                    // Streaming text - send to TUI for Matrix decrypt effect
                    MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Text(
                        text,
                    )) => {
                        if let Some(ref tx) = self.stream_tx {
                            let _ = tx.try_send(crate::provider::rig::StreamChunk::Token(
                                text.text.clone(),
                            ));
                        }
                        response_text.push_str(&text.text);
                    }

                    // Tool call - notify TUI (shows in Mission Control)
                    MultiTurnStreamItem::StreamAssistantItem(
                        StreamedAssistantContent::ToolCall { tool_call, .. },
                    ) => {
                        tool_count += 1;
                        let call_id = format!("agent-{}-{}", self.task_id, tool_count);
                        let tool_name = tool_call.function.name.clone();

                        // v0.8.1: Serialize args once, reuse for TUI and event log
                        let args_string = serde_json::to_string(&tool_call.function.arguments)
                            .unwrap_or_default();
                        let args_value: Option<Value> = serde_json::from_str(&args_string).ok();

                        // Send McpCallStart to TUI
                        if let Some(ref tx) = self.stream_tx {
                            let _ = tx.try_send(crate::provider::rig::StreamChunk::McpCallStart {
                                tool: tool_name.clone(),
                                server: "agent".to_string(),
                                params: args_string,
                            });
                        }

                        // Log event for observability
                        self.event_log.emit(EventKind::McpInvoke {
                            task_id: Arc::from(self.task_id.as_str()),
                            call_id,
                            mcp_server: "agent".to_string(),
                            tool: Some(tool_name),
                            resource: None,
                            params: args_value,
                        });
                    }

                    // Reasoning/thinking content (Claude extended thinking)
                    MultiTurnStreamItem::StreamAssistantItem(
                        StreamedAssistantContent::Reasoning(reasoning),
                    ) => {
                        // Reasoning.content is a Vec<ReasoningContent>
                        let reasoning_str = reasoning
                            .content
                            .iter()
                            .filter_map(|c| match c {
                                ReasoningContent::Text { text, .. } => Some(text.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("");
                        if let Some(ref tx) = self.stream_tx {
                            let _ = tx.try_send(crate::provider::rig::StreamChunk::Thinking(
                                reasoning_str.clone(),
                            ));
                        }
                        match &mut thinking_text {
                            Some(t) => t.push_str(&reasoning_str),
                            None => thinking_text = Some(reasoning_str),
                        }
                    }

                    // Final response with token usage
                    MultiTurnStreamItem::FinalResponse(resp) => {
                        response_text = resp.response().to_string();
                        let usage = resp.usage();
                        input_tokens = usage.input_tokens as u32;
                        output_tokens = usage.output_tokens as u32;

                        // Send metrics to TUI
                        if let Some(ref tx) = self.stream_tx {
                            let _ = tx.try_send(crate::provider::rig::StreamChunk::Metrics {
                                input_tokens: usage.input_tokens,
                                output_tokens: usage.output_tokens,
                            });
                        }
                    }

                    // Tool results (from rig executing tools)
                    MultiTurnStreamItem::StreamUserItem(_) => {
                        // Tool results are handled internally by rig
                        // We just track completion for TUI
                        if let Some(ref tx) = self.stream_tx {
                            let _ =
                                tx.try_send(crate::provider::rig::StreamChunk::McpCallComplete {
                                    result: "Tool completed".to_string(),
                                });
                        }
                    }

                    // Other variants (ignore)
                    _ => {}
                },
                Err(e) => {
                    return Err(NikaError::AgentExecutionError {
                        task_id: self.task_id.clone(),
                        reason: format!("Stream chunk failed: {}", e),
                    });
                }
            }
        }

        Ok(StreamingResult {
            response: response_text,
            input_tokens,
            output_tokens,
            thinking: thinking_text,
        })
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

        // Check stop conditions (v0.21: uses determine_status for completion detection)
        let status = self.determine_status(&final_output.to_string());

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

        // Check guardrails (v0.23)
        let guardrails_passed = self.check_guardrails(&response_text);

        Ok(RigAgentLoopResult {
            status: status.clone(),
            turns: 1,
            final_output,
            total_tokens: 100, // Mock token count
            confidence: status.confidence(),
            retry_count: 0,
            guardrails_passed,
            cost_usd: 0.0,
            partial_result: None,
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
    /// ## Token Tracking (v0.7.2)
    /// - Without tools: Uses streaming API for accurate token tracking
    /// - With tools: Falls back to agent.prompt() (tokens will be 0)
    /// - With extended_thinking: Uses dedicated streaming path
    pub async fn run_claude(&mut self) -> Result<RigAgentLoopResult, NikaError> {
        // Check if extended thinking is enabled
        if self.params.extended_thinking == Some(true) {
            return self.run_claude_with_thinking().await;
        }

        // Create Anthropic client from environment
        let client = anthropic::Client::from_env();

        // Get model name (default to claude-sonnet-4-6)
        let model_name = self.params.model.as_deref().unwrap_or("claude-sonnet-4-6");
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

        // Execute with streaming helper (v0.7.2 - token tracking)
        // - No tools: Pure streaming with token tracking
        // - With tools: Falls back to agent.prompt() (0 tokens)
        let prompt = self.params.prompt.clone();
        let result = self
            .stream_with_tools(model, &prompt, tools, max_turns)
            .await?;

        // Determine status from response (v0.21: uses determine_status for completion detection)
        let status = self.determine_status(&result.response);

        // Build metadata WITH token tracking (v0.7.2)
        let stop_reason = status.as_canonical_str();
        let metadata = AgentTurnMetadata {
            thinking: result.thinking,
            response_text: result.response.clone(),
            input_tokens: result.input_tokens,
            output_tokens: result.output_tokens,
            cache_read_tokens: 0,
            stop_reason: stop_reason.to_string(),
        };

        // Emit completion event
        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index: 1,
            kind: stop_reason.to_string(),
            metadata: Some(metadata),
        });

        // Check guardrails (v0.23)
        let guardrails_passed = self.check_guardrails(&result.response);

        Ok(RigAgentLoopResult {
            status: status.clone(),
            turns: 1, // rig handles turns internally, we report completion as 1
            final_output: serde_json::json!({ "response": result.response }),
            total_tokens: (result.input_tokens + result.output_tokens) as u64,
            confidence: status.confidence(),
            retry_count: 0,
            guardrails_passed,
            cost_usd: 0.0,
            partial_result: None,
        })
    }

    /// Check if any stop condition is met in the output
    fn check_stop_conditions(&self, output: &str) -> bool {
        self.params
            .stop_conditions
            .iter()
            .any(|cond| output.contains(cond))
    }

    /// Check if output contains explicit completion signal (v0.21)
    ///
    /// This checks for the COMPLETION_MARKER in tool results, indicating the
    /// agent called nika:complete to signal task completion.
    fn check_completion_signal(&self, output: &str) -> bool {
        use crate::runtime::builtin::COMPLETION_MARKER;
        output.contains(COMPLETION_MARKER)
    }

    /// Run all configured guardrails against the output (v0.23)
    ///
    /// Emits `GuardrailPassed` or `GuardrailFailed` events for each guardrail.
    /// Returns `true` if all guardrails pass, `false` if any fail.
    pub fn check_guardrails(&self, output: &str) -> bool {
        if self.params.guardrails.is_empty() {
            return true;
        }

        let results = run_guardrails(&self.params.guardrails, output);
        let mut all_passed = true;

        for result in results {
            let task_id = Arc::from(self.task_id.as_str());
            if result.passed {
                self.event_log.emit(EventKind::GuardrailPassed {
                    task_id,
                    guardrail_type: result.guardrail_type,
                    description: result.guardrail_id,
                });
            } else {
                self.event_log.emit(EventKind::GuardrailFailed {
                    task_id,
                    guardrail_type: result.guardrail_type,
                    description: result.guardrail_id,
                    message: result
                        .message
                        .unwrap_or_else(|| "Guardrail check failed".to_string()),
                });
                all_passed = false;
            }
        }

        all_passed
    }

    /// Determine agent status based on output content (v0.21 + v0.22)
    ///
    /// Checks in order:
    /// 1. Explicit completion via nika:complete tool
    ///    - With confidence: compare against threshold → HighConfidence/LowConfidence
    ///    - Without confidence: ExplicitCompletion
    /// 2. Legacy stop conditions (StopConditionMet)
    /// 3. Natural completion (NaturalCompletion)
    pub fn determine_status(&self, output: &str) -> RigAgentStatus {
        if self.check_completion_signal(output) {
            // Parse the completion response to extract confidence (v0.22)
            use crate::runtime::builtin::parse_completion_response;

            if let Some(response) = parse_completion_response(output) {
                // Check if confidence is provided
                if let Some(confidence) = response.confidence {
                    // Use apply_routing for confidence-based status (v0.22)
                    return self.apply_routing(confidence);
                }
            }
            // No confidence provided, treat as explicit completion
            RigAgentStatus::ExplicitCompletion
        } else if self.check_stop_conditions(output) {
            RigAgentStatus::StopConditionMet
        } else {
            RigAgentStatus::NaturalCompletion
        }
    }

    /// Get confidence threshold from completion config (v0.22)
    ///
    /// Returns the configured threshold, or 0.8 as default.
    fn get_confidence_threshold(&self) -> f64 {
        self.params
            .effective_completion()
            .and_then(|c| c.confidence)
            .map(|conf| conf.threshold)
            .unwrap_or(0.8)
    }

    /// Get low confidence configuration (v0.22)
    ///
    /// Returns the OnLowConfidenceConfig if available, or None.
    fn get_low_confidence_config(&self) -> Option<crate::ast::completion::OnLowConfidenceConfig> {
        self.params
            .effective_completion()
            .and_then(|c| c.confidence)
            .map(|conf| conf.on_low.clone())
    }

    /// Check if retry should be attempted for low confidence (v0.22)
    ///
    /// Returns true if:
    /// - Status is LowConfidence
    /// - on_low.action is Retry
    /// - retry_count < max_retries
    fn should_retry(&self, status: &RigAgentStatus, retry_count: u32) -> bool {
        if !matches!(status, RigAgentStatus::LowConfidence(_)) {
            return false;
        }

        let Some(config) = self.get_low_confidence_config() else {
            return false;
        };

        config.action == crate::ast::completion::LowConfidenceAction::Retry
            && retry_count < config.max_retries
    }

    /// Get retry feedback message (v0.22)
    ///
    /// Returns the feedback message to append to prompt on retry.
    fn get_retry_feedback(&self, confidence: f64) -> String {
        let config = self.get_low_confidence_config();
        let threshold = self.get_confidence_threshold();

        // Use custom feedback if configured
        if let Some(feedback) = config.as_ref().and_then(|c| c.feedback.clone()) {
            return format!(
                "\n\n[RETRY: Your previous response had confidence {:.2}, below threshold {:.2}. {}]",
                confidence, threshold, feedback
            );
        }

        // Default feedback
        format!(
            "\n\n[RETRY: Your previous response had confidence {:.2}, which is below the required threshold of {:.2}. Please reconsider your response and provide a higher confidence answer.]",
            confidence, threshold
        )
    }

    /// Get confidence routing configuration (v0.22)
    ///
    /// Returns the ConfidenceRouting if available, or None.
    fn get_confidence_routing(&self) -> Option<crate::ast::completion::ConfidenceRouting> {
        self.params
            .effective_completion()
            .and_then(|c| c.confidence)
            .and_then(|conf| conf.routing.clone())
    }

    /// Apply confidence-based routing (v0.22)
    ///
    /// Uses routing configuration to determine the appropriate status
    /// based on confidence level. If routing is not configured, falls back
    /// to simple threshold-based High/Low confidence.
    fn apply_routing(&self, confidence: f64) -> RigAgentStatus {
        let Some(routing) = self.get_confidence_routing() else {
            // No routing configured, use simple threshold
            let threshold = self.get_confidence_threshold();
            return if confidence >= threshold {
                RigAgentStatus::HighConfidence(confidence)
            } else {
                RigAgentStatus::LowConfidence(confidence)
            };
        };

        // Determine which route applies based on confidence
        // Check high route first (highest min value)
        if let Some(high_min) = routing.high.min {
            if confidence >= high_min {
                return self.route_action_to_status(&routing.high.action, confidence);
            }
        }

        // Check medium route (typically >= threshold)
        if let Some(medium_min) = routing.medium.min {
            if confidence >= medium_min {
                return self.route_action_to_status(&routing.medium.action, confidence);
            }
        }

        // Default to low route
        self.route_action_to_status(&routing.low.action, confidence)
    }

    /// Convert a RouteAction to RigAgentStatus (v0.22)
    fn route_action_to_status(
        &self,
        action: &crate::ast::completion::RouteAction,
        confidence: f64,
    ) -> RigAgentStatus {
        use crate::ast::completion::RouteAction;

        match action {
            RouteAction::Accept => RigAgentStatus::HighConfidence(confidence),
            RouteAction::AcceptWithFlag => RigAgentStatus::FlaggedForReview(confidence),
            RouteAction::Retry => RigAgentStatus::LowConfidence(confidence),
            RouteAction::Escalate => RigAgentStatus::Escalated(confidence),
        }
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

        // Get model name (default to claude-sonnet-4-6)
        let model_name = self.params.model.as_deref().unwrap_or("claude-sonnet-4-6");
        let model = client.completion_model(model_name);

        // Build completion request with thinking enabled
        // Use configurable thinking_budget from AgentParams (default: 4096)
        let thinking_budget = self.params.effective_thinking_budget();

        // Extended thinking requires additional_params (Claude-specific API feature)
        let thinking_config = serde_json::json!({
            "thinking": {
                "type": "enabled",
                "budget_tokens": thinking_budget
            }
        });

        // Build request with native temperature method (v0.8.0)
        // Inject skills into system prompt if configured (v0.15.4)
        let preamble = self.inject_skills_into_prompt().await?;

        // v0.18.0: Use effective_max_tokens (required for extended thinking)
        // Claude requires max_tokens > thinking_budget
        let max_tokens = self
            .params
            .effective_max_tokens()
            .unwrap_or((thinking_budget as u32) + 8192);

        let mut request_builder = model
            .completion_request(&self.params.prompt)
            .preamble(preamble)
            .max_tokens(max_tokens as u64)
            .additional_params(thinking_config);

        // Apply temperature using native rig-core method (v0.8.0)
        if let Some(temp) = self.params.effective_temperature() {
            request_builder = request_builder.temperature(f64::from(temp));
        }

        let request = request_builder.build();

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

        // v0.8.5: Per-chunk timeout to prevent hanging streams
        loop {
            let chunk_result = match timeout(STREAM_CHUNK_TIMEOUT, stream.next()).await {
                Ok(Some(chunk)) => chunk,
                Ok(None) => break, // Stream ended normally
                Err(_elapsed) => {
                    // Timeout - stream stalled
                    tracing::warn!(
                        task_id = %self.task_id,
                        timeout_secs = STREAM_CHUNK_TIMEOUT.as_secs(),
                        "Thinking stream timed out waiting for chunk"
                    );
                    return Err(NikaError::Timeout {
                        operation: format!("thinking capture (task: {})", self.task_id),
                        duration_ms: STREAM_CHUNK_TIMEOUT.as_millis() as u64,
                    });
                }
            };

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

        // Determine status (v0.21: uses determine_status for completion detection)
        let status = self.determine_status(&response);

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

        // Check guardrails (v0.23)
        let guardrails_passed = self.check_guardrails(&response);

        Ok(RigAgentLoopResult {
            status: status.clone(),
            turns: 1,
            final_output: serde_json::json!({ "response": response }),
            total_tokens: (input_tokens + output_tokens) as u64,
            confidence: status.confidence(),
            retry_count: 0,
            guardrails_passed,
            cost_usd: 0.0,
            partial_result: None,
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

        // Execute with streaming helper (v0.7.2 - token tracking)
        // - No tools: Pure streaming with token tracking
        // - With tools: Falls back to agent.prompt() (0 tokens)
        let prompt = self.params.prompt.clone();
        let result = self
            .stream_with_tools(model, &prompt, tools, max_turns)
            .await?;

        // Determine status from response (v0.21: uses determine_status for completion detection)
        let status = self.determine_status(&result.response);

        // Build metadata WITH token tracking (v0.7.2)
        let stop_reason = status.as_canonical_str();
        let metadata = AgentTurnMetadata {
            thinking: result.thinking,
            response_text: result.response.clone(),
            input_tokens: result.input_tokens,
            output_tokens: result.output_tokens,
            cache_read_tokens: 0,
            stop_reason: stop_reason.to_string(),
        };

        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index: 1,
            kind: stop_reason.to_string(),
            metadata: Some(metadata),
        });

        // Check guardrails (v0.23)
        let guardrails_passed = self.check_guardrails(&result.response);

        Ok(RigAgentLoopResult {
            status: status.clone(),
            turns: 1,
            final_output: serde_json::json!({ "response": result.response }),
            total_tokens: (result.input_tokens + result.output_tokens) as u64,
            confidence: status.confidence(),
            retry_count: 0,
            guardrails_passed,
            cost_usd: 0.0,
            partial_result: None,
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
                "gemini" | "google" => return self.run_gemini().await, // v0.15.0
                other => {
                    return Err(NikaError::AgentValidationError {
                        reason: format!(
                            "Unknown provider: '{}'. Use 'claude', 'openai', 'mistral', 'ollama', 'groq', 'deepseek', or 'gemini'.",
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

        // v0.15.0: Gemini support
        if has_key("GEMINI_API_KEY") {
            return self.run_gemini().await;
        }

        if has_key("OLLAMA_API_BASE_URL") {
            return self.run_ollama().await;
        }

        Err(NikaError::AgentValidationError {
            reason: "No API key found. Set one of: ANTHROPIC_API_KEY, OPENAI_API_KEY, MISTRAL_API_KEY, GROQ_API_KEY, DEEPSEEK_API_KEY, GEMINI_API_KEY, or OLLAMA_API_BASE_URL.".to_string(),
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
            .unwrap_or_else(|| "llama-3.3-70b-versatile".to_string());
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

    /// Run with Gemini provider (requires GEMINI_API_KEY) - v0.15.0
    pub async fn run_gemini(&mut self) -> Result<RigAgentLoopResult, NikaError> {
        let model_name = self
            .params
            .model
            .clone()
            .unwrap_or_else(|| "gemini-2.0-flash".to_string());
        let client = rig::providers::gemini::Client::from_env();
        self.run_generic_provider_impl(client, &model_name).await
    }

    /// Generic provider runner implementation (v0.6 + v0.22 retry)
    ///
    /// Uses rig-core's unified ProviderClient + CompletionClient interface.
    /// Includes retry logic for low confidence responses (v0.22).
    async fn run_generic_provider_impl<C>(
        &mut self,
        client: C,
        model_name: &str,
    ) -> Result<RigAgentLoopResult, NikaError>
    where
        C: CompletionClient,
        C::CompletionModel: Clone + 'static,
        <C::CompletionModel as rig::completion::CompletionModel>::Response: Send,
    {
        let model = client.completion_model(model_name);

        // Take ownership of tools for first attempt
        let tools = std::mem::take(&mut self.tools);
        let max_turns = self.params.max_turns.unwrap_or(10) as usize;
        let base_prompt = self.params.prompt.clone();

        // Get max retries from config (default: 2)
        let max_retries = self
            .get_low_confidence_config()
            .map(|c| c.max_retries)
            .unwrap_or(2);

        let mut retry_count: u32 = 0;
        let mut current_prompt = base_prompt.clone();
        let mut total_input_tokens: u32 = 0;
        let mut total_output_tokens: u32 = 0;

        // Emit start event
        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index: 1,
            kind: "started".to_string(),
            metadata: None,
        });

        // First attempt with tools
        let mut result = self
            .stream_with_tools(model.clone(), &current_prompt, tools, max_turns)
            .await?;

        total_input_tokens += result.input_tokens;
        total_output_tokens += result.output_tokens;

        let mut status = self.determine_status(&result.response);

        // Retry loop for low confidence (v0.22)
        while self.should_retry(&status, retry_count) {
            retry_count += 1;

            // Get confidence from status for feedback message
            let confidence = match &status {
                RigAgentStatus::LowConfidence(c) => *c,
                _ => 0.0,
            };

            // Emit retry event
            self.event_log.emit(EventKind::AgentTurn {
                task_id: Arc::from(self.task_id.as_str()),
                turn_index: retry_count + 1,
                kind: format!("retry_{}", retry_count),
                metadata: Some(AgentTurnMetadata {
                    thinking: None,
                    response_text: format!(
                        "Low confidence ({:.2}), retrying ({}/{})",
                        confidence, retry_count, max_retries
                    ),
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read_tokens: 0,
                    stop_reason: "low_confidence_retry".to_string(),
                }),
            });

            // Append feedback to prompt for retry
            current_prompt = format!(
                "{}\n\n{}\n\nPrevious response:\n{}",
                base_prompt,
                self.get_retry_feedback(confidence),
                result.response
            );

            // Retry without tools (agent has already gathered context)
            // Using empty tools vec for retry attempts
            result = self
                .stream_with_tools(model.clone(), &current_prompt, vec![], max_turns)
                .await?;

            total_input_tokens += result.input_tokens;
            total_output_tokens += result.output_tokens;

            status = self.determine_status(&result.response);
        }

        // Build metadata WITH token tracking (v0.7.2)
        let stop_reason = status.as_canonical_str();
        let metadata = AgentTurnMetadata {
            thinking: result.thinking,
            response_text: result.response.clone(),
            input_tokens: total_input_tokens,
            output_tokens: total_output_tokens,
            cache_read_tokens: 0,
            stop_reason: stop_reason.to_string(),
        };

        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index: retry_count + 1,
            kind: stop_reason.to_string(),
            metadata: Some(metadata),
        });

        // Check guardrails (v0.23)
        let guardrails_passed = self.check_guardrails(&result.response);

        Ok(RigAgentLoopResult {
            status: status.clone(),
            turns: (retry_count + 1) as usize,
            final_output: serde_json::json!({ "response": result.response }),
            total_tokens: (total_input_tokens + total_output_tokens) as u64,
            confidence: status.confidence(),
            retry_count,
            guardrails_passed,
            cost_usd: 0.0,
            partial_result: None,
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
            confidence: None,
            retry_count: 0,
            guardrails_passed: true,
            cost_usd: 0.0,
            partial_result: None,
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
    // Completion Detection Tests (v0.21)
    // ========================================================================

    #[test]
    fn test_check_completion_signal() {
        use crate::runtime::builtin::COMPLETION_MARKER;

        let params = AgentParams {
            prompt: "Test".to_string(),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // Should detect completion marker
        let response_with_marker = format!(
            r#"{{"completed": true, "marker": "{}"}}"#,
            COMPLETION_MARKER
        );
        assert!(agent.check_completion_signal(&response_with_marker));

        // Should not detect without marker
        assert!(!agent.check_completion_signal("Task completed successfully"));
        assert!(!agent.check_completion_signal(r#"{"completed": true}"#));
    }

    #[test]
    fn test_determine_status_explicit_completion() {
        use crate::runtime::builtin::COMPLETION_MARKER;

        let params = AgentParams {
            prompt: "Test".to_string(),
            stop_conditions: vec!["DONE".to_string()],
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // Explicit completion has highest priority
        let response = format!(r#"Result: {{"marker": "{}"}}"#, COMPLETION_MARKER);
        assert_eq!(
            agent.determine_status(&response),
            RigAgentStatus::ExplicitCompletion
        );
    }

    #[test]
    fn test_determine_status_stop_condition() {
        let params = AgentParams {
            prompt: "Test".to_string(),
            stop_conditions: vec!["DONE".to_string()],
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // Stop condition (no completion marker)
        assert_eq!(
            agent.determine_status("Task is DONE"),
            RigAgentStatus::StopConditionMet
        );
    }

    #[test]
    fn test_determine_status_natural_completion() {
        let params = AgentParams {
            prompt: "Test".to_string(),
            stop_conditions: vec!["DONE".to_string()],
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // Natural completion (no marker, no stop condition)
        assert_eq!(
            agent.determine_status("Task completed normally"),
            RigAgentStatus::NaturalCompletion
        );
    }

    #[test]
    fn test_determine_status_explicit_over_stop_condition() {
        use crate::runtime::builtin::COMPLETION_MARKER;

        let params = AgentParams {
            prompt: "Test".to_string(),
            stop_conditions: vec!["DONE".to_string()],
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // When both marker and stop condition present, explicit wins
        let response = format!("DONE and marker: {}", COMPLETION_MARKER);
        assert_eq!(
            agent.determine_status(&response),
            RigAgentStatus::ExplicitCompletion
        );
    }

    #[test]
    fn test_explicit_completion_status_canonical_str() {
        assert_eq!(
            RigAgentStatus::ExplicitCompletion.as_canonical_str(),
            "tool_complete"
        );
    }

    // ========================================================================
    // Confidence-Based Completion Tests (v0.22)
    // ========================================================================

    #[test]
    fn test_determine_status_high_confidence() {
        use crate::ast::completion::{CompletionConfig, CompletionMode, ConfidenceConfig};
        use crate::runtime::builtin::COMPLETION_MARKER;

        let params = AgentParams {
            prompt: "Test".to_string(),
            completion: Some(CompletionConfig {
                mode: CompletionMode::Explicit,
                confidence: Some(ConfidenceConfig {
                    threshold: 0.8,
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // Completion response with confidence >= threshold
        let response = format!(
            r#"{{"completed": true, "result": "done", "confidence": 0.95, "marker": "{}"}}"#,
            COMPLETION_MARKER
        );
        let status = agent.determine_status(&response);
        assert!(
            matches!(status, RigAgentStatus::HighConfidence(c) if c == 0.95),
            "Expected HighConfidence(0.95), got {:?}",
            status
        );
    }

    #[test]
    fn test_determine_status_low_confidence() {
        use crate::ast::completion::{CompletionConfig, CompletionMode, ConfidenceConfig};
        use crate::runtime::builtin::COMPLETION_MARKER;

        let params = AgentParams {
            prompt: "Test".to_string(),
            completion: Some(CompletionConfig {
                mode: CompletionMode::Explicit,
                confidence: Some(ConfidenceConfig {
                    threshold: 0.8,
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // Completion response with confidence < threshold
        let response = format!(
            r#"{{"completed": true, "result": "done", "confidence": 0.5, "marker": "{}"}}"#,
            COMPLETION_MARKER
        );
        let status = agent.determine_status(&response);
        assert!(
            matches!(status, RigAgentStatus::LowConfidence(c) if c == 0.5),
            "Expected LowConfidence(0.5), got {:?}",
            status
        );
    }

    #[test]
    fn test_determine_status_no_confidence_defaults_to_explicit() {
        use crate::ast::completion::{CompletionConfig, CompletionMode, ConfidenceConfig};
        use crate::runtime::builtin::COMPLETION_MARKER;

        let params = AgentParams {
            prompt: "Test".to_string(),
            completion: Some(CompletionConfig {
                mode: CompletionMode::Explicit,
                confidence: Some(ConfidenceConfig {
                    threshold: 0.8,
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // Completion response without confidence
        let response = format!(
            r#"{{"completed": true, "result": "done", "marker": "{}"}}"#,
            COMPLETION_MARKER
        );
        let status = agent.determine_status(&response);
        assert_eq!(
            status,
            RigAgentStatus::ExplicitCompletion,
            "No confidence should default to ExplicitCompletion"
        );
    }

    #[test]
    fn test_confidence_status_helper_methods() {
        // HighConfidence is completed
        let high = RigAgentStatus::HighConfidence(0.95);
        assert!(high.is_completed());
        assert!(!high.requires_retry());
        assert_eq!(high.confidence(), Some(0.95));
        assert_eq!(high.as_canonical_str(), "tool_complete_high");

        // LowConfidence requires retry
        let low = RigAgentStatus::LowConfidence(0.5);
        assert!(!low.is_completed());
        assert!(low.requires_retry());
        assert_eq!(low.confidence(), Some(0.5));
        assert_eq!(low.as_canonical_str(), "tool_complete_low");

        // Other statuses have no confidence
        assert_eq!(RigAgentStatus::NaturalCompletion.confidence(), None);
        assert_eq!(RigAgentStatus::ExplicitCompletion.confidence(), None);
    }

    #[test]
    fn test_get_confidence_threshold_default() {
        let params = AgentParams {
            prompt: "Test".to_string(),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // Default threshold is 0.8
        assert_eq!(agent.get_confidence_threshold(), 0.8);
    }

    #[test]
    fn test_get_confidence_threshold_custom() {
        use crate::ast::completion::{CompletionConfig, CompletionMode, ConfidenceConfig};

        let params = AgentParams {
            prompt: "Test".to_string(),
            completion: Some(CompletionConfig {
                mode: CompletionMode::Explicit,
                confidence: Some(ConfidenceConfig {
                    threshold: 0.9,
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        assert_eq!(agent.get_confidence_threshold(), 0.9);
    }

    // ========================================================================
    // Retry Logic Tests (v0.22)
    // ========================================================================

    #[test]
    fn test_should_retry_returns_false_for_non_low_confidence() {
        let params = AgentParams {
            prompt: "Test".to_string(),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // Non-LowConfidence statuses should not retry
        assert!(!agent.should_retry(&RigAgentStatus::NaturalCompletion, 0));
        assert!(!agent.should_retry(&RigAgentStatus::ExplicitCompletion, 0));
        assert!(!agent.should_retry(&RigAgentStatus::HighConfidence(0.9), 0));
        assert!(!agent.should_retry(&RigAgentStatus::MaxTurnsReached, 0));
    }

    #[test]
    fn test_should_retry_with_retry_action() {
        use crate::ast::completion::{
            CompletionConfig, CompletionMode, ConfidenceConfig, LowConfidenceAction,
            OnLowConfidenceConfig,
        };

        let params = AgentParams {
            prompt: "Test".to_string(),
            completion: Some(CompletionConfig {
                mode: CompletionMode::Explicit,
                confidence: Some(ConfidenceConfig {
                    threshold: 0.8,
                    on_low: OnLowConfidenceConfig {
                        action: LowConfidenceAction::Retry,
                        max_retries: 3,
                        feedback: None,
                    },
                    routing: None,
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // Should retry when under max_retries
        assert!(agent.should_retry(&RigAgentStatus::LowConfidence(0.5), 0));
        assert!(agent.should_retry(&RigAgentStatus::LowConfidence(0.5), 1));
        assert!(agent.should_retry(&RigAgentStatus::LowConfidence(0.5), 2));

        // Should NOT retry when at or above max_retries
        assert!(!agent.should_retry(&RigAgentStatus::LowConfidence(0.5), 3));
        assert!(!agent.should_retry(&RigAgentStatus::LowConfidence(0.5), 4));
    }

    #[test]
    fn test_should_retry_with_accept_action() {
        use crate::ast::completion::{
            CompletionConfig, CompletionMode, ConfidenceConfig, LowConfidenceAction,
            OnLowConfidenceConfig,
        };

        let params = AgentParams {
            prompt: "Test".to_string(),
            completion: Some(CompletionConfig {
                mode: CompletionMode::Explicit,
                confidence: Some(ConfidenceConfig {
                    threshold: 0.8,
                    on_low: OnLowConfidenceConfig {
                        action: LowConfidenceAction::Accept,
                        max_retries: 3,
                        feedback: None,
                    },
                    routing: None,
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // Accept action should never retry
        assert!(!agent.should_retry(&RigAgentStatus::LowConfidence(0.5), 0));
    }

    #[test]
    fn test_should_retry_with_escalate_action() {
        use crate::ast::completion::{
            CompletionConfig, CompletionMode, ConfidenceConfig, LowConfidenceAction,
            OnLowConfidenceConfig,
        };

        let params = AgentParams {
            prompt: "Test".to_string(),
            completion: Some(CompletionConfig {
                mode: CompletionMode::Explicit,
                confidence: Some(ConfidenceConfig {
                    threshold: 0.8,
                    on_low: OnLowConfidenceConfig {
                        action: LowConfidenceAction::Escalate,
                        max_retries: 3,
                        feedback: None,
                    },
                    routing: None,
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // Escalate action should never retry
        assert!(!agent.should_retry(&RigAgentStatus::LowConfidence(0.5), 0));
    }

    #[test]
    fn test_should_retry_without_confidence_config() {
        let params = AgentParams {
            prompt: "Test".to_string(),
            ..Default::default() // No completion config
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // Without config, should not retry (no on_low config)
        assert!(!agent.should_retry(&RigAgentStatus::LowConfidence(0.5), 0));
    }

    #[test]
    fn test_get_retry_feedback_default() {
        use crate::ast::completion::{CompletionConfig, CompletionMode, ConfidenceConfig};

        let params = AgentParams {
            prompt: "Test".to_string(),
            completion: Some(CompletionConfig {
                mode: CompletionMode::Explicit,
                confidence: Some(ConfidenceConfig {
                    threshold: 0.8,
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        let feedback = agent.get_retry_feedback(0.5);
        assert!(feedback.contains("RETRY"));
        assert!(feedback.contains("0.50")); // Confidence value
        assert!(feedback.contains("0.80")); // Threshold value
    }

    #[test]
    fn test_get_retry_feedback_custom() {
        use crate::ast::completion::{
            CompletionConfig, CompletionMode, ConfidenceConfig, LowConfidenceAction,
            OnLowConfidenceConfig,
        };

        let params = AgentParams {
            prompt: "Test".to_string(),
            completion: Some(CompletionConfig {
                mode: CompletionMode::Explicit,
                confidence: Some(ConfidenceConfig {
                    threshold: 0.8,
                    on_low: OnLowConfidenceConfig {
                        action: LowConfidenceAction::Retry,
                        max_retries: 2,
                        feedback: Some(
                            "Please verify your sources and provide citations".to_string(),
                        ),
                    },
                    routing: None,
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        let feedback = agent.get_retry_feedback(0.6);
        assert!(feedback.contains("RETRY"));
        assert!(feedback.contains("0.60")); // Confidence value
        assert!(feedback.contains("verify your sources")); // Custom feedback
    }

    #[test]
    fn test_get_low_confidence_config_present() {
        use crate::ast::completion::{
            CompletionConfig, CompletionMode, ConfidenceConfig, LowConfidenceAction,
            OnLowConfidenceConfig,
        };

        let params = AgentParams {
            prompt: "Test".to_string(),
            completion: Some(CompletionConfig {
                mode: CompletionMode::Explicit,
                confidence: Some(ConfidenceConfig {
                    threshold: 0.8,
                    on_low: OnLowConfidenceConfig {
                        action: LowConfidenceAction::Retry,
                        max_retries: 5,
                        feedback: Some("Custom".to_string()),
                    },
                    routing: None,
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        let config = agent.get_low_confidence_config().unwrap();
        assert_eq!(config.action, LowConfidenceAction::Retry);
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.feedback, Some("Custom".to_string()));
    }

    #[test]
    fn test_get_low_confidence_config_absent() {
        let params = AgentParams {
            prompt: "Test".to_string(),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        assert!(agent.get_low_confidence_config().is_none());
    }

    // ========================================================================
    // Confidence Routing Tests (v0.22)
    // ========================================================================

    #[test]
    fn test_apply_routing_without_config_uses_threshold() {
        use crate::ast::completion::{CompletionConfig, CompletionMode, ConfidenceConfig};

        let params = AgentParams {
            prompt: "Test".to_string(),
            completion: Some(CompletionConfig {
                mode: CompletionMode::Explicit,
                confidence: Some(ConfidenceConfig {
                    threshold: 0.8,
                    routing: None, // No routing configured
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // High confidence (>= threshold)
        let status = agent.apply_routing(0.85);
        assert!(
            matches!(status, RigAgentStatus::HighConfidence(c) if c == 0.85),
            "Expected HighConfidence(0.85), got {:?}",
            status
        );

        // Low confidence (< threshold)
        let status = agent.apply_routing(0.5);
        assert!(
            matches!(status, RigAgentStatus::LowConfidence(c) if c == 0.5),
            "Expected LowConfidence(0.5), got {:?}",
            status
        );
    }

    #[test]
    fn test_apply_routing_with_high_medium_low_routes() {
        use crate::ast::completion::{
            CompletionConfig, CompletionMode, ConfidenceConfig, ConfidenceRoute, ConfidenceRouting,
            RouteAction,
        };

        let params = AgentParams {
            prompt: "Test".to_string(),
            completion: Some(CompletionConfig {
                mode: CompletionMode::Explicit,
                confidence: Some(ConfidenceConfig {
                    threshold: 0.7,
                    routing: Some(ConfidenceRouting {
                        high: ConfidenceRoute {
                            min: Some(0.85),
                            action: RouteAction::Accept,
                            escalate_to: None,
                        },
                        medium: ConfidenceRoute {
                            min: Some(0.7),
                            action: RouteAction::AcceptWithFlag,
                            escalate_to: None,
                        },
                        low: ConfidenceRoute {
                            min: None,
                            action: RouteAction::Escalate,
                            escalate_to: Some("human".to_string()),
                        },
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // High confidence route (>= 0.85)
        let status = agent.apply_routing(0.92);
        assert!(
            matches!(status, RigAgentStatus::HighConfidence(c) if c == 0.92),
            "Expected HighConfidence for 0.92, got {:?}",
            status
        );

        // Medium confidence route (>= 0.7, < 0.85)
        let status = agent.apply_routing(0.75);
        assert!(
            matches!(status, RigAgentStatus::FlaggedForReview(c) if c == 0.75),
            "Expected FlaggedForReview for 0.75, got {:?}",
            status
        );

        // Low confidence route (< 0.7)
        let status = agent.apply_routing(0.5);
        assert!(
            matches!(status, RigAgentStatus::Escalated(c) if c == 0.5),
            "Expected Escalated for 0.5, got {:?}",
            status
        );
    }

    #[test]
    fn test_apply_routing_retry_action() {
        use crate::ast::completion::{
            CompletionConfig, CompletionMode, ConfidenceConfig, ConfidenceRoute, ConfidenceRouting,
            RouteAction,
        };

        let params = AgentParams {
            prompt: "Test".to_string(),
            completion: Some(CompletionConfig {
                mode: CompletionMode::Explicit,
                confidence: Some(ConfidenceConfig {
                    threshold: 0.7,
                    routing: Some(ConfidenceRouting {
                        high: ConfidenceRoute {
                            min: Some(0.9),
                            action: RouteAction::Accept,
                            escalate_to: None,
                        },
                        medium: ConfidenceRoute {
                            min: Some(0.7),
                            action: RouteAction::AcceptWithFlag,
                            escalate_to: None,
                        },
                        low: ConfidenceRoute {
                            min: None,
                            action: RouteAction::Retry, // Low confidence -> retry
                            escalate_to: None,
                        },
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // Low confidence with Retry action
        let status = agent.apply_routing(0.5);
        assert!(
            matches!(status, RigAgentStatus::LowConfidence(c) if c == 0.5),
            "Expected LowConfidence for Retry action, got {:?}",
            status
        );
    }

    #[test]
    fn test_route_action_to_status_all_variants() {
        use crate::ast::completion::RouteAction;

        let params = AgentParams {
            prompt: "Test".to_string(),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        assert!(matches!(
            agent.route_action_to_status(&RouteAction::Accept, 0.9),
            RigAgentStatus::HighConfidence(c) if c == 0.9
        ));

        assert!(matches!(
            agent.route_action_to_status(&RouteAction::AcceptWithFlag, 0.75),
            RigAgentStatus::FlaggedForReview(c) if c == 0.75
        ));

        assert!(matches!(
            agent.route_action_to_status(&RouteAction::Retry, 0.5),
            RigAgentStatus::LowConfidence(c) if c == 0.5
        ));

        assert!(matches!(
            agent.route_action_to_status(&RouteAction::Escalate, 0.3),
            RigAgentStatus::Escalated(c) if c == 0.3
        ));
    }

    #[test]
    fn test_get_confidence_routing_present() {
        use crate::ast::completion::{
            CompletionConfig, CompletionMode, ConfidenceConfig, ConfidenceRoute, ConfidenceRouting,
            RouteAction,
        };

        let params = AgentParams {
            prompt: "Test".to_string(),
            completion: Some(CompletionConfig {
                mode: CompletionMode::Explicit,
                confidence: Some(ConfidenceConfig {
                    threshold: 0.7,
                    routing: Some(ConfidenceRouting {
                        high: ConfidenceRoute {
                            min: Some(0.9),
                            action: RouteAction::Accept,
                            escalate_to: None,
                        },
                        medium: ConfidenceRoute {
                            min: Some(0.7),
                            action: RouteAction::AcceptWithFlag,
                            escalate_to: None,
                        },
                        low: ConfidenceRoute {
                            min: None,
                            action: RouteAction::Escalate,
                            escalate_to: Some("human".to_string()),
                        },
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        let routing = agent.get_confidence_routing();
        assert!(routing.is_some());

        let r = routing.unwrap();
        assert_eq!(r.high.min, Some(0.9));
        assert_eq!(r.high.action, RouteAction::Accept);
        assert_eq!(r.low.escalate_to, Some("human".to_string()));
    }

    #[test]
    fn test_get_confidence_routing_absent() {
        let params = AgentParams {
            prompt: "Test".to_string(),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        assert!(agent.get_confidence_routing().is_none());
    }

    #[test]
    fn test_flagged_and_escalated_status_properties() {
        // FlaggedForReview
        let flagged = RigAgentStatus::FlaggedForReview(0.75);
        assert_eq!(flagged.as_canonical_str(), "tool_complete_flagged");
        assert_eq!(flagged.confidence(), Some(0.75));
        assert!(!flagged.requires_retry());

        // Escalated
        let escalated = RigAgentStatus::Escalated(0.4);
        assert_eq!(escalated.as_canonical_str(), "escalated");
        assert_eq!(escalated.confidence(), Some(0.4));
        assert!(!escalated.requires_retry());
    }

    #[test]
    fn test_determine_status_with_routing() {
        use crate::ast::completion::{
            CompletionConfig, CompletionMode, ConfidenceConfig, ConfidenceRoute, ConfidenceRouting,
            RouteAction,
        };
        use crate::runtime::builtin::COMPLETION_MARKER;

        let params = AgentParams {
            prompt: "Test".to_string(),
            completion: Some(CompletionConfig {
                mode: CompletionMode::Explicit,
                confidence: Some(ConfidenceConfig {
                    threshold: 0.7,
                    routing: Some(ConfidenceRouting {
                        high: ConfidenceRoute {
                            min: Some(0.85),
                            action: RouteAction::Accept,
                            escalate_to: None,
                        },
                        medium: ConfidenceRoute {
                            min: Some(0.7),
                            action: RouteAction::AcceptWithFlag,
                            escalate_to: None,
                        },
                        low: ConfidenceRoute {
                            min: None,
                            action: RouteAction::Escalate,
                            escalate_to: Some("supervisor".to_string()),
                        },
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // Medium confidence -> FlaggedForReview
        let response = format!(
            r#"{{"completed": true, "result": "done", "confidence": 0.78, "marker": "{}"}}"#,
            COMPLETION_MARKER
        );
        let status = agent.determine_status(&response);
        assert!(
            matches!(status, RigAgentStatus::FlaggedForReview(c) if (c - 0.78).abs() < 0.001),
            "Expected FlaggedForReview(0.78), got {:?}",
            status
        );

        // Low confidence -> Escalated
        let response = format!(
            r#"{{"completed": true, "result": "done", "confidence": 0.5, "marker": "{}"}}"#,
            COMPLETION_MARKER
        );
        let status = agent.determine_status(&response);
        assert!(
            matches!(status, RigAgentStatus::Escalated(c) if (c - 0.5).abs() < 0.001),
            "Expected Escalated(0.5), got {:?}",
            status
        );
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
