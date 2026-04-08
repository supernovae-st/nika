// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Provider execution methods
//!
//! Contains: run_mock, run (unified entry point), run_auto (auto-detect),
//! run_claude_with_thinking, and the generic run_agent_loop with retry logic.

use std::sync::Arc;

use rig::client::{CompletionClient, ProviderClient};
use rig::providers::{anthropic, openai};
use serde_json;

use crate::error::NikaError;
use crate::event::{AgentTurnMetadata, EventKind};

use crate::ast::limits::{LimitAction, LimitType};

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
            kind: nika_event::AgentTurnKind::Started,
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
            stop_reason: nika_event::AgentStopReason::from(stop_reason.to_string()),
        };

        // Emit completion event with metadata
        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index: 1,
            kind: nika_event::AgentTurnKind::from(stop_reason),
            metadata: Some(metadata),
        });

        // Check guardrails
        let guardrail_result = self.check_guardrails(&response_text).await;
        let guardrails_passed = guardrail_result.is_passed();

        // Override status based on guardrail outcome
        let status = if guardrail_result.should_fail() {
            RigAgentStatus::Failed
        } else if guardrail_result.should_escalate() {
            RigAgentStatus::Escalated(status.confidence().unwrap_or(0.0))
        } else {
            status
        };

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

    /// Unified entry point — dispatches to the right rig-core client.
    ///
    /// Replaces run_claude/run_openai/run_mistral/run_groq/run_deepseek/run_gemini/run_xai.
    /// Extended thinking intercept stays in run_claude_with_thinking.
    pub async fn run(&mut self) -> Result<RigAgentLoopResult, NikaError> {
        // Extended thinking: Anthropic-only, completely separate path
        if self.params.extended_thinking == Some(true) {
            if let Some(ref p) = self.params.provider {
                let name = p.as_str();
                if name != "anthropic" && name != "claude" {
                    tracing::warn!(
                        declared_provider = %name,
                        "extended_thinking is Anthropic-only — ignoring provider '{}', using Claude",
                        name
                    );
                }
            }
            return self.run_claude_with_thinking().await;
        }

        let provider_ref =
            self.params
                .provider
                .as_ref()
                .ok_or_else(|| NikaError::AgentValidationError {
                    reason: "provider is required for agent: tasks".to_string(),
                })?;
        let provider_str = provider_ref.as_str();

        let resolved = crate::core::find_provider(provider_str).ok_or_else(|| {
            NikaError::AgentValidationError {
                reason: format!(
                    "Unknown provider: '{}'. Use a cloud provider for agent: tasks.",
                    provider_str
                ),
            }
        })?;

        let model_name = self
            .params
            .model
            .clone()
            .ok_or_else(|| NikaError::ValidationError {
                reason: "model field is required for LLM verbs (NIKA-034)".to_string(),
            })?;

        use crate::provider::cost::ProviderKind;
        match resolved.id {
            "anthropic" => {
                let client = anthropic::Client::from_env();
                self.run_agent_loop(client, &model_name, Some(ProviderKind::Claude))
                    .await
            }
            "openai" => {
                let client = openai::Client::from_env();
                self.run_agent_loop(client, &model_name, Some(ProviderKind::OpenAI))
                    .await
            }
            "mistral" => {
                let client = rig::providers::mistral::Client::from_env();
                self.run_agent_loop(client, &model_name, Some(ProviderKind::Mistral))
                    .await
            }
            "groq" => {
                let client = rig::providers::groq::Client::from_env();
                self.run_agent_loop(client, &model_name, Some(ProviderKind::Groq))
                    .await
            }
            "deepseek" => {
                let client = rig::providers::deepseek::Client::from_env();
                self.run_agent_loop(client, &model_name, Some(ProviderKind::DeepSeek))
                    .await
            }
            "gemini" => {
                let client = rig::providers::gemini::Client::from_env();
                self.run_agent_loop(client, &model_name, Some(ProviderKind::Gemini))
                    .await
            }
            "xai" => {
                let client = rig::providers::xai::Client::from_env();
                self.run_agent_loop(client, &model_name, Some(ProviderKind::XAi))
                    .await
            }
            "native" => Err(NikaError::AgentValidationError {
                reason:
                    "Provider 'native' is not supported for agent: tasks. Use a cloud provider."
                        .to_string(),
            }),
            _ => Err(NikaError::AgentValidationError {
                reason: format!(
                    "Provider '{}' is not supported for agent: tasks.",
                    resolved.id
                ),
            }),
        }
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
        // If provider is explicitly set, delegate directly
        if self.params.provider.is_some() {
            return self.run().await;
        }

        // Auto-detect: iterate KNOWN_PROVIDERS in priority order (LLM category only)
        use crate::core::providers::{ProviderCategory, KNOWN_PROVIDERS};
        for p in KNOWN_PROVIDERS.iter() {
            if p.category == ProviderCategory::Llm && crate::secrets::has_provider_key(p) {
                self.params.provider = Some(nika_core::ProviderName::parse(p.id));
                return self.run().await;
            }
        }

        Err(NikaError::AgentValidationError {
            reason: "No API key found. Set one of: ANTHROPIC_API_KEY, OPENAI_API_KEY, MISTRAL_API_KEY, GROQ_API_KEY, DEEPSEEK_API_KEY, GEMINI_API_KEY, or XAI_API_KEY.".to_string(),
        })
    }

    /// Generic provider runner implementation
    ///
    /// Uses rig-core's unified ProviderClient + CompletionClient interface.
    /// Includes retry logic for low confidence responses.
    async fn run_agent_loop<C>(
        &mut self,
        client: C,
        model_name: &str,
        provider_kind: Option<crate::provider::cost::ProviderKind>,
    ) -> Result<RigAgentLoopResult, NikaError>
    where
        C: CompletionClient,
        C::CompletionModel: Clone + 'static,
        <C::CompletionModel as rig::completion::CompletionModel>::Response: Send,
    {
        let model_name = Self::strip_model_prefix(model_name);
        let model = client.completion_model(model_name);

        // Ensure params.provider is set for downstream use (stop_sequences key mapping, etc.)
        // In auto-detect mode, params.provider may be None even though we know the provider.
        if self.params.provider.is_none() {
            if let Some(ref pk) = provider_kind {
                self.params.provider = Some(nika_core::ProviderName::parse(pk.to_provider_id()));
            }
        }

        // Wire token_budget shorthand into LimitTracker (fixes SF9)
        // token_budget is the top-level shorthand; limits.max_tokens takes precedence.
        if let Some(budget) = self.params.token_budget {
            self.limit_tracker.set_max_tokens_if_unset(budget as u64);
        }

        // Take ownership of tools for first attempt
        let tools = self.tools_as_boxed();
        let max_turns = self.params.max_turns.unwrap_or(10) as usize;
        let base_prompt = self.params.prompt.clone();

        // Get max retries from config (default: 2)
        let max_retries = self
            .get_low_confidence_config()
            .map(|c| c.max_retries)
            .unwrap_or(2);

        let mut retry_count: u32 = 0;
        let mut current_prompt = base_prompt.clone();
        let mut total_input_tokens: u64 = 0;
        let mut total_output_tokens: u64 = 0;
        let mut total_cached_input_tokens: u64 = 0;

        // Emit start event
        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index: 1,
            kind: nika_event::AgentTurnKind::Started,
            metadata: None,
        });

        // First attempt with tools
        let mut result = self
            .stream_with_tools(model.clone(), &current_prompt, tools, max_turns)
            .await?;

        // Capture TTFT from first streaming call (most meaningful for latency tracking)
        let first_ttft_ms = result.ttft_ms;

        total_input_tokens = total_input_tokens.saturating_add(result.input_tokens);
        total_output_tokens = total_output_tokens.saturating_add(result.output_tokens);
        total_cached_input_tokens =
            total_cached_input_tokens.saturating_add(result.cached_input_tokens);

        // Record turn in limit tracker
        let turn_cost = provider_kind
            .map(|pk| {
                crate::provider::cost::calculate_cost_with_cache(
                    pk,
                    model_name,
                    result.input_tokens,
                    result.output_tokens,
                    result.cached_input_tokens,
                )
            })
            .unwrap_or(0.0);
        self.limit_tracker
            .record_turn(result.input_tokens, result.output_tokens, turn_cost);

        // Check limits after first turn
        if let Some(exceeded) = self.limit_tracker.check_limits() {
            let status = match exceeded.limit_type {
                LimitType::Turns => RigAgentStatus::MaxTurnsReached,
                LimitType::Tokens => RigAgentStatus::TokenBudgetExceeded,
                LimitType::Cost => RigAgentStatus::CostLimitReached,
                LimitType::Duration => RigAgentStatus::DurationLimitReached,
            };
            tracing::warn!(
                task_id = %self.task_id,
                limit = %exceeded.limit_type,
                current = exceeded.current,
                maximum = exceeded.maximum,
                action = ?self.limit_tracker.on_limit_action(),
                "Agent limit exceeded after first turn"
            );
            match self.limit_tracker.on_limit_action() {
                LimitAction::Fail => {
                    self.event_log.emit(EventKind::ProviderResponded {
                        task_id: Arc::from(self.task_id.as_str()),
                        request_id: None,
                        input_tokens: total_input_tokens,
                        output_tokens: total_output_tokens,
                        cache_read_tokens: total_cached_input_tokens,
                        ttft_ms: first_ttft_ms,
                        finish_reason: nika_event::FinishReason::Other(format!(
                            "limit_exceeded:{}",
                            exceeded.limit_type
                        )),
                        cost_usd: self.limit_tracker.cost_usd(),
                    });
                    return Err(NikaError::AgentLimitExceeded {
                        limit_type: format!("{}", exceeded.limit_type),
                        current: exceeded.current,
                        maximum: exceeded.maximum,
                    });
                }
                LimitAction::Escalate => {
                    tracing::warn!(task_id = %self.task_id, "Escalation requested — returning partial result");
                }
                LimitAction::CompletePartial => {}
            }
            return Ok(RigAgentLoopResult {
                status,
                turns: 1,
                final_output: serde_json::json!({ "response": result.response }),
                total_tokens: total_input_tokens + total_output_tokens,
                confidence: None,
                retry_count: 0,
                guardrails_passed: true,
                cost_usd: self.limit_tracker.cost_usd(),
                partial_result: None,
            });
        }

        let mut status = self.determine_status(&result.response);

        // Retry loop for low confidence
        while self.should_retry(&status, retry_count) {
            retry_count += 1;

            // Check limits before starting a retry
            if let Some(exceeded) = self.limit_tracker.check_limits() {
                let limit_status = match exceeded.limit_type {
                    LimitType::Turns => RigAgentStatus::MaxTurnsReached,
                    LimitType::Tokens => RigAgentStatus::TokenBudgetExceeded,
                    LimitType::Cost => RigAgentStatus::CostLimitReached,
                    LimitType::Duration => RigAgentStatus::DurationLimitReached,
                };
                tracing::warn!(
                    task_id = %self.task_id,
                    limit = %exceeded.limit_type,
                    retry = retry_count,
                    action = ?self.limit_tracker.on_limit_action(),
                    "Agent limit exceeded during retry loop"
                );
                match self.limit_tracker.on_limit_action() {
                    LimitAction::Fail => {
                        self.event_log.emit(EventKind::ProviderResponded {
                            task_id: Arc::from(self.task_id.as_str()),
                            request_id: None,
                            input_tokens: total_input_tokens,
                            output_tokens: total_output_tokens,
                            cache_read_tokens: total_cached_input_tokens,
                            ttft_ms: first_ttft_ms,
                            finish_reason: nika_event::FinishReason::Other(format!(
                                "limit_exceeded:{}",
                                exceeded.limit_type
                            )),
                            cost_usd: self.limit_tracker.cost_usd(),
                        });
                        return Err(NikaError::AgentLimitExceeded {
                            limit_type: format!("{}", exceeded.limit_type),
                            current: exceeded.current,
                            maximum: exceeded.maximum,
                        });
                    }
                    LimitAction::Escalate => {
                        tracing::warn!(task_id = %self.task_id, "Escalation requested — returning partial result");
                    }
                    LimitAction::CompletePartial => {}
                }
                status = limit_status;
                break;
            }

            // Get confidence from status for feedback message
            let confidence = match &status {
                RigAgentStatus::LowConfidence(c) => *c,
                _ => 0.0,
            };

            // Emit retry event
            self.event_log.emit(EventKind::AgentTurn {
                task_id: Arc::from(self.task_id.as_str()),
                turn_index: retry_count + 1,
                kind: nika_event::AgentTurnKind::Continue,
                metadata: Some(AgentTurnMetadata {
                    thinking: None,
                    response_text: format!(
                        "Low confidence ({:.2}), retrying ({}/{})",
                        confidence, retry_count, max_retries
                    ),
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read_tokens: 0,
                    stop_reason: nika_event::AgentStopReason::LowConfidenceRetry,
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

            total_input_tokens = total_input_tokens.saturating_add(result.input_tokens);
            total_output_tokens = total_output_tokens.saturating_add(result.output_tokens);
            total_cached_input_tokens =
                total_cached_input_tokens.saturating_add(result.cached_input_tokens);

            // Record retry turn in limit tracker
            let retry_cost = provider_kind
                .map(|pk| {
                    crate::provider::cost::calculate_cost_with_cache(
                        pk,
                        model_name,
                        result.input_tokens,
                        result.output_tokens,
                        result.cached_input_tokens,
                    )
                })
                .unwrap_or(0.0);
            self.limit_tracker
                .record_turn(result.input_tokens, result.output_tokens, retry_cost);

            status = self.determine_status(&result.response);
        }

        // Build metadata WITH token tracking
        let stop_reason = status.as_canonical_str();
        let metadata = AgentTurnMetadata {
            thinking: result.thinking,
            response_text: result.response.clone(),
            input_tokens: total_input_tokens,
            output_tokens: total_output_tokens,
            cache_read_tokens: total_cached_input_tokens,
            stop_reason: nika_event::AgentStopReason::from(stop_reason.to_string()),
        };

        self.event_log.emit(EventKind::AgentTurn {
            task_id: Arc::from(self.task_id.as_str()),
            turn_index: retry_count + 1,
            kind: nika_event::AgentTurnKind::from(stop_reason),
            metadata: Some(metadata),
        });

        // Check guardrails with retry loop for `on_failure: retry`
        let max_guardrail_retries: u32 = 2;
        let mut guardrail_retry_count: u32 = 0;
        let mut guardrail_result = self.check_guardrails(&result.response).await;

        while guardrail_result.should_retry() && guardrail_retry_count < max_guardrail_retries {
            guardrail_retry_count += 1;

            // Check limits before starting a guardrail retry
            if let Some(exceeded) = self.limit_tracker.check_limits() {
                tracing::warn!(
                    task_id = %self.task_id,
                    limit = %exceeded.limit_type,
                    guardrail_retry = guardrail_retry_count,
                    action = ?self.limit_tracker.on_limit_action(),
                    "Agent limit exceeded during guardrail retry loop"
                );
                match self.limit_tracker.on_limit_action() {
                    LimitAction::Fail => {
                        self.event_log.emit(EventKind::ProviderResponded {
                            task_id: Arc::from(self.task_id.as_str()),
                            request_id: None,
                            input_tokens: total_input_tokens,
                            output_tokens: total_output_tokens,
                            cache_read_tokens: total_cached_input_tokens,
                            ttft_ms: first_ttft_ms,
                            finish_reason: nika_event::FinishReason::Other(format!(
                                "limit_exceeded:{}",
                                exceeded.limit_type
                            )),
                            cost_usd: self.limit_tracker.cost_usd(),
                        });
                        return Err(NikaError::AgentLimitExceeded {
                            limit_type: format!("{}", exceeded.limit_type),
                            current: exceeded.current,
                            maximum: exceeded.maximum,
                        });
                    }
                    LimitAction::Escalate => {
                        tracing::warn!(task_id = %self.task_id, "Escalation requested — returning partial result");
                    }
                    LimitAction::CompletePartial => {}
                }
                break;
            }

            // Build feedback from guardrail failure messages
            let feedback = guardrail_result.failure_messages().join("; ");
            tracing::info!(
                task_id = %self.task_id,
                guardrail_retry = guardrail_retry_count,
                max = max_guardrail_retries,
                feedback = %feedback,
                "Retrying due to guardrail failure"
            );

            // Emit guardrail retry event
            self.event_log.emit(EventKind::AgentTurn {
                task_id: Arc::from(self.task_id.as_str()),
                turn_index: retry_count + guardrail_retry_count + 1,
                kind: nika_event::AgentTurnKind::Continue,
                metadata: Some(AgentTurnMetadata {
                    thinking: None,
                    response_text: format!(
                        "Guardrail validation failed, retrying ({}/{}): {}",
                        guardrail_retry_count, max_guardrail_retries, feedback
                    ),
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read_tokens: 0,
                    stop_reason: nika_event::AgentStopReason::GuardrailRetry,
                }),
            });

            // Append guardrail feedback to prompt
            current_prompt = format!(
                "{}\n\n[GUARDRAIL RETRY {}/{}] Your previous output failed quality validation:\n{}\n\nPlease fix these issues and try again.\n\nPrevious response:\n{}",
                base_prompt,
                guardrail_retry_count,
                max_guardrail_retries,
                feedback,
                result.response
            );

            // Re-run without tools (agent already has context)
            result = self
                .stream_with_tools(model.clone(), &current_prompt, vec![], max_turns)
                .await?;

            total_input_tokens = total_input_tokens.saturating_add(result.input_tokens);
            total_output_tokens = total_output_tokens.saturating_add(result.output_tokens);
            total_cached_input_tokens =
                total_cached_input_tokens.saturating_add(result.cached_input_tokens);

            // Record guardrail retry turn in limit tracker
            let gr_cost = provider_kind
                .map(|pk| {
                    crate::provider::cost::calculate_cost_with_cache(
                        pk,
                        model_name,
                        result.input_tokens,
                        result.output_tokens,
                        result.cached_input_tokens,
                    )
                })
                .unwrap_or(0.0);
            self.limit_tracker
                .record_turn(result.input_tokens, result.output_tokens, gr_cost);

            // Re-determine status and re-check guardrails
            status = self.determine_status(&result.response);
            guardrail_result = self.check_guardrails(&result.response).await;
        }

        // After guardrail retries exhausted, if still failing with retry -> accept anyway
        // (don't block forever, the guardrails_passed flag will indicate the failure)
        if guardrail_result.should_retry() {
            tracing::warn!(
                task_id = %self.task_id,
                retries = guardrail_retry_count,
                "Guardrail retries exhausted, accepting output with guardrails_passed=false"
            );
        }

        let guardrails_passed = guardrail_result.is_passed();

        // Override status when guardrails fail with terminal actions
        let status = if guardrail_result.should_fail() {
            RigAgentStatus::Failed
        } else if guardrail_result.should_escalate() {
            RigAgentStatus::Escalated(status.confidence().unwrap_or(0.0))
        } else {
            status
        };

        let total_retries = retry_count + guardrail_retry_count;

        // Emit ProviderResponded — use final status (post-guardrail override)
        let final_reason = status.as_canonical_str();
        let total_cost = provider_kind
            .map(|pk| {
                crate::provider::cost::calculate_cost_with_cache(
                    pk,
                    model_name,
                    total_input_tokens,
                    total_output_tokens,
                    total_cached_input_tokens,
                )
            })
            .unwrap_or(0.0);
        self.event_log.emit(EventKind::ProviderResponded {
            task_id: Arc::from(self.task_id.as_str()),
            request_id: None,
            input_tokens: total_input_tokens,
            output_tokens: total_output_tokens,
            cache_read_tokens: total_cached_input_tokens,
            ttft_ms: first_ttft_ms,
            finish_reason: nika_event::FinishReason::from(final_reason.to_string()),
            cost_usd: if total_cost.is_finite() {
                total_cost
            } else {
                0.0
            },
        });

        Ok(RigAgentLoopResult {
            status: status.clone(),
            turns: (total_retries + 1) as usize,
            final_output: serde_json::json!({ "response": result.response }),
            total_tokens: total_input_tokens + total_output_tokens,
            confidence: status.confidence(),
            retry_count: total_retries,
            guardrails_passed,
            cost_usd: self.limit_tracker.cost_usd(),
            partial_result: None,
        })
    }
}
