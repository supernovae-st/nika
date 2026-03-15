//! Provider-specific execution methods
//!
//! Contains: run_mock, run_claude, run_openai, run_auto,
//! run_mistral, run_groq, run_deepseek, run_gemini,
//! and the generic provider implementation with retry logic.

use std::sync::Arc;

use rig::client::{CompletionClient, ProviderClient};
use rig::providers::{anthropic, openai};
use serde_json;

use crate::error::NikaError;
use crate::event::{AgentTurnMetadata, EventKind};

use super::types::{RigAgentLoopResult, RigAgentStatus};
use super::RigAgentLoop;

impl RigAgentLoop {
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
        let status = self.determine_status(&final_output.to_string());

        // Build metadata for completion event
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

        // Check guardrails
        let guardrail_result = self.check_guardrails(&response_text);
        let guardrails_passed = guardrail_result.is_passed();

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
    /// ## Extended Thinking
    /// When `extended_thinking: true` is set in AgentParams, this method uses
    /// the streaming API to capture Claude's reasoning process. The thinking
    /// is stored in `AgentTurnMetadata.thinking` for observability.
    ///
    /// ## Token Tracking
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

        // Execute with streaming helper
        // - No tools: Pure streaming with token tracking
        // - With tools: Falls back to agent.prompt() (0 tokens)
        let prompt = self.params.prompt.clone();
        let result = self
            .stream_with_tools(model, &prompt, tools, max_turns)
            .await?;

        // Determine status from response
        let status = self.determine_status(&result.response);

        // Build metadata WITH token tracking
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

        // Check guardrails
        let guardrail_result = self.check_guardrails(&result.response);
        let guardrails_passed = guardrail_result.is_passed();

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

        // Execute with streaming helper
        // - No tools: Pure streaming with token tracking
        // - With tools: Falls back to agent.prompt() (0 tokens)
        let prompt = self.params.prompt.clone();
        let result = self
            .stream_with_tools(model, &prompt, tools, max_turns)
            .await?;

        // Determine status from response
        let status = self.determine_status(&result.response);

        // Build metadata WITH token tracking
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

        // Check guardrails
        let guardrail_result = self.check_guardrails(&result.response);
        let guardrails_passed = guardrail_result.is_passed();

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

    /// Run the agent loop with the best available provider
    ///
    /// Provider selection order:
    /// 1. Check AgentParams.provider field
    /// 2. Check ANTHROPIC_API_KEY env var → use Claude
    /// 3. Check OPENAI_API_KEY env var → use OpenAI
    /// 4. Check MISTRAL_API_KEY env var → use Mistral
    /// 5. Check GROQ_API_KEY env var → use Groq
    /// 6. Check DEEPSEEK_API_KEY env var → use DeepSeek
    /// 7. Error if no provider available
    ///
    /// # Note
    /// This is the recommended method for production use.
    pub async fn run_auto(&mut self) -> Result<RigAgentLoopResult, NikaError> {
        // Check explicit provider from params
        if let Some(ref provider_name) = self.params.provider {
            let resolved = crate::core::find_provider(provider_name).ok_or_else(|| {
                NikaError::AgentValidationError {
                    reason: format!(
                        "Unknown provider: '{}'. Use 'claude', 'openai', 'mistral', 'groq', 'deepseek', or 'gemini'.",
                        provider_name
                    ),
                }
            })?;
            return match resolved.id {
                "anthropic" => self.run_claude().await,
                "openai" => self.run_openai().await,
                "mistral" => self.run_mistral().await,
                "groq" => self.run_groq().await,
                "deepseek" => self.run_deepseek().await,
                "gemini" => self.run_gemini().await,
                "native" => Err(NikaError::AgentValidationError {
                    reason: "Provider 'native' is not supported for agent: tasks. Native inference (mistral.rs) is only available for infer: tasks. Use a cloud provider (claude, openai, mistral, groq, deepseek, gemini) for agent tasks.".to_string(),
                }),
                _ => Err(NikaError::AgentValidationError {
                    reason: format!("Provider '{}' is not supported for agent: tasks.", resolved.id),
                }),
            };
        }

        // Auto-detect: iterate KNOWN_PROVIDERS in priority order (LLM category only)
        use crate::core::providers::{ProviderCategory, KNOWN_PROVIDERS};
        for p in KNOWN_PROVIDERS.iter() {
            if p.category == ProviderCategory::Llm && p.has_env_key() {
                return match p.id {
                    "anthropic" => self.run_claude().await,
                    "openai" => self.run_openai().await,
                    "mistral" => self.run_mistral().await,
                    "groq" => self.run_groq().await,
                    "deepseek" => self.run_deepseek().await,
                    "gemini" => self.run_gemini().await,
                    _ => continue,
                };
            }
        }

        Err(NikaError::AgentValidationError {
            reason: "No API key found. Set one of: ANTHROPIC_API_KEY, OPENAI_API_KEY, MISTRAL_API_KEY, GROQ_API_KEY, DEEPSEEK_API_KEY, or GEMINI_API_KEY.".to_string(),
        })
    }

    // =========================================================================
    // Additional Provider Methods
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

    /// Run with Gemini provider (requires GEMINI_API_KEY)
    pub async fn run_gemini(&mut self) -> Result<RigAgentLoopResult, NikaError> {
        let model_name = self
            .params
            .model
            .clone()
            .unwrap_or_else(|| "gemini-2.0-flash".to_string());
        let client = rig::providers::gemini::Client::from_env();
        self.run_generic_provider_impl(client, &model_name).await
    }

    /// Generic provider runner implementation
    ///
    /// Uses rig-core's unified ProviderClient + CompletionClient interface.
    /// Includes retry logic for low confidence responses.
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

        // Retry loop for low confidence
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

        // Build metadata WITH token tracking
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

        // Check guardrails
        let guardrail_result = self.check_guardrails(&result.response);
        let guardrails_passed = guardrail_result.is_passed();

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
