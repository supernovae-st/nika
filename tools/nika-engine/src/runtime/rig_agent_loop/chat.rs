//! Chat history management and multi-turn conversation support
//!
//! Provides methods for managing conversation history and continuing
//! multi-turn conversations with different LLM providers.

use std::sync::Arc;

use rig::agent::AgentBuilder;
use rig::client::{CompletionClient, ProviderClient};
use rig::completion::{Chat, CompletionModel};
use rig::message::Message;
use rig::providers::{anthropic, openai};
use serde_json;

use crate::error::NikaError;
use crate::event::{AgentTurnMetadata, EventKind};

use super::types::{RigAgentLoopResult, RigAgentStatus};
use super::RigAgentLoop;

impl RigAgentLoop {
    // =========================================================================
    // Chat History Management
    // =========================================================================

    /// Add a user/assistant turn to the conversation history
    ///
    /// Call this after each completed turn to maintain context for `chat_continue()`.
    pub fn add_to_history(&mut self, user_prompt: &str, assistant_response: &str) {
        self.history.push(Message::user(user_prompt));
        self.history.push(Message::assistant(assistant_response));
        self.turn_count += 1;
    }

    /// Add a single message to the history
    pub fn push_message(&mut self, message: Message) {
        self.history.push(message);
    }

    /// Clear all conversation history and reset turn count
    pub fn clear_history(&mut self) {
        self.history.clear();
        self.turn_count = 0;
    }

    /// Get the current history length (number of messages)
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// Get the number of completed turns (user + assistant exchanges).
    pub fn turn_count(&self) -> u32 {
        self.turn_count
    }

    /// Get a reference to the conversation history
    pub fn history(&self) -> &[Message] {
        &self.history
    }

    /// Create with pre-existing history
    ///
    /// Useful for resuming conversations or injecting context.
    pub fn with_history(mut self, history: Vec<Message>) -> Self {
        // Each turn = 1 user + 1 assistant message. Odd-length histories
        // (e.g., resumed mid-turn) round up to count the partial turn.
        self.turn_count = history.len().div_ceil(2) as u32;
        self.history = history;
        self
    }

    /// Continue a conversation using the accumulated history
    ///
    /// Uses rig-core's `Chat` trait for multi-turn conversations.
    /// The history is automatically updated with the user prompt and response.
    ///
    /// # Example
    /// ```rust,ignore
    /// // First turn
    /// let result1 = agent.run().await?;
    /// agent.add_to_history("Initial prompt", &extract_text(&result1));
    ///
    /// // Continue conversation
    /// let result2 = agent.chat_continue("Follow-up question").await?;
    /// // History now contains both turns
    /// ```
    pub async fn chat_continue(&mut self, prompt: &str) -> Result<RigAgentLoopResult, NikaError> {
        // Resolve provider — explicit or auto-detected
        let provider_id = if let Some(ref p) = self.params.provider {
            let resolved = crate::core::find_provider(p.as_str()).ok_or_else(|| {
                NikaError::AgentValidationError {
                    reason: format!(
                        "Unknown provider: '{}'. Use a cloud provider for chat_continue.",
                        p.as_str()
                    ),
                }
            })?;
            resolved.id
        } else {
            // Auto-detect from env vars via catalog priority order
            use crate::core::providers::{ProviderCategory, KNOWN_PROVIDERS};
            let mut found = None;
            for p in KNOWN_PROVIDERS.iter() {
                if p.category == ProviderCategory::Llm && crate::secrets::has_provider_key(p) {
                    found = Some(p.id);
                    break;
                }
            }
            found.ok_or_else(|| NikaError::AgentValidationError {
                reason: "chat_continue requires a configured provider or an API key in the environment".to_string(),
            })?
        };

        let model_name = self.resolve_model_name()?;

        // Dispatch to the right client — each arm creates a model and calls chat_continue_with_model
        macro_rules! dispatch_chat {
            ($client:expr) => {{
                let model = $client.completion_model(&model_name);
                self.chat_continue_with_model(prompt, model, &model_name).await
            }};
        }

        match provider_id {
            "anthropic" => dispatch_chat!(anthropic::Client::from_env()),
            "openai" => dispatch_chat!(openai::Client::from_env()),
            "mistral" => dispatch_chat!(rig::providers::mistral::Client::from_env()),
            "groq" => dispatch_chat!(rig::providers::groq::Client::from_env()),
            "deepseek" => dispatch_chat!(rig::providers::deepseek::Client::from_env()),
            "gemini" => dispatch_chat!(rig::providers::gemini::Client::from_env()),
            "xai" => dispatch_chat!(rig::providers::xai::Client::from_env()),
            other => Err(NikaError::AgentValidationError {
                reason: format!("Provider '{}' is not supported for chat_continue.", other),
            }),
        }
    }

    // =========================================================================
    // Shared Implementation
    // =========================================================================

    /// Extract and validate the model name from params.
    ///
    /// Returns an owned String to avoid holding an immutable borrow on `self`
    /// across the `&mut self` call to `chat_continue_with_model`.
    fn resolve_model_name(&self) -> Result<String, NikaError> {
        let raw = self
            .params
            .model
            .as_deref()
            .ok_or_else(|| NikaError::ValidationError {
                reason: "model field is required for LLM verbs (NIKA-034)".to_string(),
            })?;
        Ok(Self::strip_model_prefix(raw).to_string())
    }

    /// Generic chat continuation — all provider wrappers delegate here.
    ///
    /// Builds a rig Agent from the given CompletionModel, runs the chat with
    /// accumulated history, updates history, emits telemetry events, estimates
    /// token costs, and checks guardrails.
    ///
    /// **Note:** Token tracking uses char-based estimation (Chat trait returns
    /// only String, no usage metadata). Use `run()` for single-turn requests
    /// for single-turn requests with full streaming token tracking.
    async fn chat_continue_with_model<M: CompletionModel>(
        &mut self,
        prompt: &str,
        model: M,
        model_name: &str,
    ) -> Result<RigAgentLoopResult, NikaError> {
        let turn_index = self.turn_count + 1;

        // Inject skills into system prompt if configured
        let preamble = self.inject_skills_into_prompt().await?;

        // Emit start event
        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index,
            kind: nika_event::AgentTurnKind::Started,
            metadata: None,
        });

        // Build agent with full config
        let effective_max_tokens = self.params.effective_max_tokens().unwrap_or(8192) as u64;
        let mut builder = AgentBuilder::new(model)
            .preamble(&preamble)
            .max_tokens(effective_max_tokens);

        if let Some(temp) = self.params.effective_temperature() {
            builder = builder.temperature(f64::from(temp));
        }

        if self.params.has_explicit_tool_choice() {
            let tool_choice = self.params.effective_tool_choice();
            builder = builder.tool_choice(tool_choice.into());
        }

        if let Some(stop_params) = Self::stop_sequences_params(
            self.params
                .provider
                .as_ref()
                .map(|p| p.as_str())
                .unwrap_or(""),
            &self.params.stop_sequences,
        ) {
            builder = builder.additional_params(stop_params);
        }

        let tools = self.tools_as_boxed();
        let agent = builder.tools(tools).build();

        let raw_response = agent
            .chat(prompt, self.history.clone())
            .await
            .map_err(|e| NikaError::AgentExecutionError {
                task_id: self.task_id.clone(),
                reason: e.to_string(),
            })?;

        // Strip <think> blocks from reasoning models (Qwen, DeepSeek-R1)
        let response = crate::runtime::executor::verbs::strip_think_tags(&raw_response);

        // Update history and increment turn count.
        // Cap history size to prevent unbounded memory growth (max_turns * 2 messages).
        let max_history = self.params.max_turns.unwrap_or(10) as usize * 2;
        if self.history.len() >= max_history {
            // Trim oldest messages, keeping system-level context at the start
            let trim_count = self.history.len() + 2 - max_history;
            self.history.drain(..trim_count);
        }
        self.history.push(Message::user(prompt));
        self.history.push(Message::assistant(&response));
        self.turn_count += 1;

        // Determine status
        let status = self.determine_status(&response);

        // Emit completion
        let stop_reason = status.as_canonical_str();
        let metadata = AgentTurnMetadata::text_only(&response, stop_reason);

        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index,
            kind: nika_event::AgentTurnKind::from(stop_reason),
            metadata: Some(metadata),
        });

        // Check guardrails and override status on terminal actions
        let guardrail_result = self.check_guardrails(&response).await;
        let guardrails_passed = guardrail_result.is_passed();
        let status = if guardrail_result.should_fail() {
            RigAgentStatus::Failed
        } else if guardrail_result.should_escalate() {
            RigAgentStatus::Escalated(status.confidence().unwrap_or(0.0))
        } else {
            status
        };

        // Estimate tokens for cost tracking (Chat trait returns only String, no metadata)
        let est_input = prompt.chars().count().div_ceil(4) as u64;
        let est_output = response.chars().count().div_ceil(4) as u64;
        let provider_kind = crate::provider::cost::ProviderKind::parse(
            self.params
                .provider
                .as_ref()
                .map(|p| p.as_str())
                .unwrap_or(""),
        );
        let cost = provider_kind
            .map(|pk| crate::provider::cost::calculate_cost(pk, model_name, est_input, est_output))
            .unwrap_or(0.0);

        Ok(RigAgentLoopResult {
            status: status.clone(),
            turns: turn_index as usize,
            final_output: serde_json::json!({ "response": response }),
            total_tokens: est_input + est_output,
            confidence: status.confidence(),
            retry_count: 0,
            guardrails_passed,
            cost_usd: cost,
            partial_result: None,
        })
    }
}
