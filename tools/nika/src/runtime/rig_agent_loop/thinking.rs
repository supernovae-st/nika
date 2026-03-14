//! Extended thinking, guardrails, and confidence routing
//!
//! Contains: check_stop_conditions, check_completion_signal, check_guardrails,
//! determine_status, confidence routing, and run_claude_with_thinking.

use std::sync::Arc;

use futures::StreamExt;
use rig::client::{CompletionClient, ProviderClient};
use rig::completion::CompletionModel as _;
use rig::completion::GetTokenUsage;
use rig::message::ReasoningContent;
use rig::providers::anthropic;
use rig::streaming::StreamedAssistantContent;
use serde_json;
use tokio::time::timeout;

use crate::ast::guardrails::{escalation_required, immediate_failures, run_sync_guardrails};
use crate::error::NikaError;
use crate::event::{AgentTurnMetadata, EventKind};
use crate::util::STREAM_CHUNK_TIMEOUT;

use super::types::{GuardrailCheckResult, RigAgentLoopResult, RigAgentStatus};
use super::RigAgentLoop;

impl RigAgentLoop {
    /// Check if any stop condition is met in the output
    pub(crate) fn check_stop_conditions(&self, output: &str) -> bool {
        self.params
            .stop_conditions
            .iter()
            .any(|cond| output.contains(cond))
    }

    /// Check if output contains explicit completion signal
    ///
    /// This checks for the COMPLETION_MARKER in tool results, indicating the
    /// agent called nika:complete to signal task completion.
    pub(crate) fn check_completion_signal(&self, output: &str) -> bool {
        use crate::runtime::builtin::COMPLETION_MARKER;
        output.contains(COMPLETION_MARKER)
    }

    /// Run all configured guardrails against the output
    ///
    /// Emits events for each guardrail result:
    /// - `GuardrailPassed`: Guardrail check succeeded
    /// - `GuardrailFailed`: Guardrail check failed
    /// - `GuardrailEscalation`: Guardrail failed with `on_failure: escalate`
    ///
    /// Returns `GuardrailCheckResult` indicating the appropriate action:
    /// - `AllPassed`: All guardrails passed
    /// - `FailedRetry`: Some failed with `on_failure: retry` (default)
    /// - `FailedEscalate`: Some failed with `on_failure: escalate`
    /// - `FailedImmediate`: Some failed with `on_failure: fail`
    ///
    /// Priority: Immediate > Escalate > Retry
    pub fn check_guardrails(&self, output: &str) -> GuardrailCheckResult {
        if self.params.guardrails.is_empty() {
            return GuardrailCheckResult::AllPassed;
        }

        let results = run_sync_guardrails(&self.params.guardrails, output);
        let mut all_passed = true;

        // PERF: Hoist Arc allocation outside loops to avoid
        // per-iteration allocation overhead. Arc::clone is cheap (atomic inc).
        let task_id: Arc<str> = Arc::from(self.task_id.as_str());

        // Emit events for each result
        for result in &results {
            if result.passed {
                self.event_log.emit(EventKind::GuardrailPassed {
                    task_id: Arc::clone(&task_id),
                    guardrail_type: result.guardrail_type.clone(),
                    description: result.guardrail_id.clone(),
                });
            } else {
                self.event_log.emit(EventKind::GuardrailFailed {
                    task_id: Arc::clone(&task_id),
                    guardrail_type: result.guardrail_type.clone(),
                    description: result.guardrail_id.clone(),
                    message: result
                        .message
                        .clone()
                        .unwrap_or_else(|| "Guardrail check failed".to_string()),
                });
                all_passed = false;
            }
        }

        if all_passed {
            return GuardrailCheckResult::AllPassed;
        }

        // Check for immediate failures first (highest priority)
        let immediate = immediate_failures(&results);
        if !immediate.is_empty() {
            return GuardrailCheckResult::FailedImmediate;
        }

        // Check for escalation requirements
        let escalations = escalation_required(&results);
        if !escalations.is_empty() {
            // Emit escalation events for each guardrail requiring escalation
            for result in escalations {
                self.event_log.emit(EventKind::GuardrailEscalation {
                    task_id: Arc::clone(&task_id),
                    guardrail_type: result.guardrail_type.clone(),
                    guardrail_id: result.guardrail_id.clone(),
                    message: result
                        .message
                        .clone()
                        .unwrap_or_else(|| "Guardrail requires escalation".to_string()),
                    severity: "high".to_string(),
                    suggested_action: Some("Review agent output and provide guidance".to_string()),
                });
            }
            return GuardrailCheckResult::FailedEscalate;
        }

        // Default: retry (on_failure: retry)
        GuardrailCheckResult::FailedRetry
    }

    /// Determine agent status based on output content
    ///
    /// Checks in order:
    /// 1. Explicit completion via nika:complete tool
    ///    - With confidence: compare against threshold → HighConfidence/LowConfidence
    ///    - Without confidence: ExplicitCompletion
    /// 2. Stop conditions (StopConditionMet)
    /// 3. Natural completion (NaturalCompletion)
    pub fn determine_status(&self, output: &str) -> RigAgentStatus {
        if self.check_completion_signal(output) {
            // Parse the completion response to extract confidence
            use crate::runtime::builtin::parse_completion_response;

            if let Some(response) = parse_completion_response(output) {
                // Check if confidence is provided
                if let Some(confidence) = response.confidence {
                    // Use apply_routing for confidence-based status
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

    /// Get confidence threshold from completion config
    ///
    /// Returns the configured threshold, or 0.8 as default.
    pub(crate) fn get_confidence_threshold(&self) -> f64 {
        self.params
            .effective_completion()
            .and_then(|c| c.confidence)
            .map(|conf| conf.threshold)
            .unwrap_or(0.8)
    }

    /// Get low confidence configuration
    ///
    /// Returns the OnLowConfidenceConfig if available, or None.
    pub(super) fn get_low_confidence_config(
        &self,
    ) -> Option<crate::ast::completion::OnLowConfidenceConfig> {
        self.params
            .effective_completion()
            .and_then(|c| c.confidence)
            .map(|conf| conf.on_low.clone())
    }

    /// Check if retry should be attempted for low confidence
    ///
    /// Returns true if:
    /// - Status is LowConfidence
    /// - on_low.action is Retry
    /// - retry_count < max_retries
    pub(super) fn should_retry(&self, status: &RigAgentStatus, retry_count: u32) -> bool {
        if !matches!(status, RigAgentStatus::LowConfidence(_)) {
            return false;
        }

        let Some(config) = self.get_low_confidence_config() else {
            return false;
        };

        config.action == crate::ast::completion::LowConfidenceAction::Retry
            && retry_count < config.max_retries
    }

    /// Get retry feedback message
    ///
    /// Returns the feedback message to append to prompt on retry.
    pub(super) fn get_retry_feedback(&self, confidence: f64) -> String {
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

    /// Get confidence routing configuration
    ///
    /// Returns the ConfidenceRouting if available, or None.
    pub(crate) fn get_confidence_routing(
        &self,
    ) -> Option<crate::ast::completion::ConfidenceRouting> {
        self.params
            .effective_completion()
            .and_then(|c| c.confidence)
            .and_then(|conf| conf.routing.clone())
    }

    /// Apply confidence-based routing
    ///
    /// Uses routing configuration to determine the appropriate status
    /// based on confidence level. If routing is not configured, falls back
    /// to simple threshold-based High/Low confidence.
    pub(crate) fn apply_routing(&self, confidence: f64) -> RigAgentStatus {
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

    /// Convert a RouteAction to RigAgentStatus
    pub(crate) fn route_action_to_status(
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

        // Build request with native temperature method
        // Inject skills into system prompt if configured
        let preamble = self.inject_skills_into_prompt().await?;

        // Use effective_max_tokens (required for extended thinking)
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

        // Apply temperature using native rig-core method
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

        // Per-chunk timeout to prevent hanging streams
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
                        // Extract token usage from final response
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
        let status = self.determine_status(&response);

        // Build metadata with thinking and token usage
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

        // Check guardrails
        let guardrail_result = self.check_guardrails(&response);
        let guardrails_passed = guardrail_result.is_passed();

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
}
