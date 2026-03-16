//! Chat history management and multi-turn conversation support
//!
//! Provides methods for managing conversation history and continuing
//! multi-turn conversations with different LLM providers.

use std::sync::Arc;

use rig::agent::AgentBuilder;
use rig::client::{CompletionClient, ProviderClient};
use rig::completion::Chat;
use rig::message::Message;
use rig::providers::{anthropic, openai};
use serde_json;

use crate::error::NikaError;
use crate::event::{AgentTurnMetadata, EventKind};

use super::types::RigAgentLoopResult;
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
        let has_key = |key: &str| std::env::var(key).is_ok_and(|v| !v.trim().is_empty());

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
        if has_key("GEMINI_API_KEY") {
            return self.chat_continue_gemini(prompt).await;
        }
        Err(NikaError::AgentValidationError {
            reason: "chat_continue requires one of: ANTHROPIC_API_KEY, OPENAI_API_KEY, MISTRAL_API_KEY, GROQ_API_KEY, DEEPSEEK_API_KEY, or GEMINI_API_KEY".to_string(),
        })
    }

    /// Continue conversation with Claude
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

        let turn_index = self.turn_count + 1;

        // Emit start event
        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index,
            kind: "started".to_string(),
            metadata: None,
        });

        // Build agent and chat with history
        // Anthropic requires max_tokens to be set explicitly
        // Inject skills into system prompt if configured
        let preamble = self.inject_skills_into_prompt().await?;
        let effective_max_tokens = self.params.effective_max_tokens().unwrap_or(8192) as u64;
        let mut builder = AgentBuilder::new(model)
            .preamble(&preamble)
            .max_tokens(effective_max_tokens);

        // Apply temperature using native rig-core method
        if let Some(temp) = self.params.effective_temperature() {
            builder = builder.temperature(f64::from(temp));
        }

        // Apply tool_choice only if explicitly set
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

        // Determine status
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

        // Check guardrails
        let guardrail_result = self.check_guardrails(&response);
        let guardrails_passed = guardrail_result.is_passed();

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

    /// Continue conversation with OpenAI
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

        let turn_index = self.turn_count + 1;

        // Emit start event
        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index,
            kind: "started".to_string(),
            metadata: None,
        });

        // Build agent and chat with history
        // Inject skills into system prompt if configured
        let preamble = self.inject_skills_into_prompt().await?;
        let effective_max_tokens = self.params.effective_max_tokens().unwrap_or(8192) as u64;
        let mut builder = AgentBuilder::new(model)
            .preamble(&preamble)
            .max_tokens(effective_max_tokens);

        // Apply temperature using native rig-core method
        if let Some(temp) = self.params.effective_temperature() {
            builder = builder.temperature(f64::from(temp));
        }

        // Apply tool_choice only if explicitly set
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

        // Determine status
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

        // Check guardrails
        let guardrail_result = self.check_guardrails(&response);
        let guardrails_passed = guardrail_result.is_passed();

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

    /// Continue conversation with Mistral
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
        let effective_max_tokens = self.params.effective_max_tokens().unwrap_or(8192) as u64;
        let agent = client
            .agent(model_name)
            .max_tokens(effective_max_tokens)
            .build();

        let turn_index = self.turn_count + 1;

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

        let status = self.determine_status(&response);
        let metadata = AgentTurnMetadata::text_only(&response, "end_turn");

        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index,
            kind: "chat_continue_mistral".to_string(),
            metadata: Some(metadata),
        });

        // Check guardrails
        let guardrail_result = self.check_guardrails(&response);
        let guardrails_passed = guardrail_result.is_passed();

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

    /// Continue conversation with Groq
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
        let effective_max_tokens = self.params.effective_max_tokens().unwrap_or(8192) as u64;
        let agent = client
            .agent(model_name)
            .max_tokens(effective_max_tokens)
            .build();

        let turn_index = self.turn_count + 1;

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

        let status = self.determine_status(&response);
        let metadata = AgentTurnMetadata::text_only(&response, "end_turn");

        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index,
            kind: "chat_continue_groq".to_string(),
            metadata: Some(metadata),
        });

        // Check guardrails
        let guardrail_result = self.check_guardrails(&response);
        let guardrails_passed = guardrail_result.is_passed();

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

    /// Continue conversation with DeepSeek
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
        let effective_max_tokens = self.params.effective_max_tokens().unwrap_or(8192) as u64;
        let agent = client
            .agent(model_name)
            .max_tokens(effective_max_tokens)
            .build();

        let turn_index = self.turn_count + 1;

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

        let status = self.determine_status(&response);
        let metadata = AgentTurnMetadata::text_only(&response, "end_turn");

        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index,
            kind: "chat_continue_deepseek".to_string(),
            metadata: Some(metadata),
        });

        // Check guardrails
        let guardrail_result = self.check_guardrails(&response);
        let guardrails_passed = guardrail_result.is_passed();

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

    /// Continue conversation with Gemini
    async fn chat_continue_gemini(
        &mut self,
        prompt: &str,
    ) -> Result<RigAgentLoopResult, NikaError> {
        use rig::completion::Chat;

        let client = rig::providers::gemini::Client::from_env();
        let model_name = self.params.model.as_deref().unwrap_or("gemini-2.0-flash");
        let effective_max_tokens = self.params.effective_max_tokens().unwrap_or(8192) as u64;
        let agent = client
            .agent(model_name)
            .max_tokens(effective_max_tokens)
            .build();

        let turn_index = self.turn_count + 1;

        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index,
            kind: "chat_continue_gemini".to_string(),
            metadata: None,
        });

        let response = agent
            .chat(prompt, self.history.clone())
            .await
            .map_err(|e| NikaError::AgentExecutionError {
                task_id: self.task_id.clone(),
                reason: format!("gemini chat error: {}", e),
            })?;

        self.history.push(Message::user(prompt));
        self.history.push(Message::assistant(&response));

        let status = self.determine_status(&response);
        let metadata = AgentTurnMetadata::text_only(&response, "end_turn");

        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index,
            kind: "chat_continue_gemini".to_string(),
            metadata: Some(metadata),
        });

        // Check guardrails
        let guardrail_result = self.check_guardrails(&response);
        let guardrails_passed = guardrail_result.is_passed();

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
}
