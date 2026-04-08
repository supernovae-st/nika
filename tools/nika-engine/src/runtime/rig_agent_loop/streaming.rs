// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Streaming execution helpers
//!
//! Contains streaming completion helpers for token tracking and real-time
//! TUI display across all providers.

use std::sync::Arc;

use futures::StreamExt;
use rig::agent::{AgentBuilder, MultiTurnStreamItem, StreamingResult as RigStreamingResult};
use rig::completion::GetTokenUsage;
use rig::message::ReasoningContent;
use rig::streaming::{StreamedAssistantContent, StreamingPrompt};
use serde_json::Value;
use tokio::time::{timeout, Instant};

use crate::error::NikaError;
use crate::event::EventKind;
use crate::util::STREAM_CHUNK_TIMEOUT;

/// Maximum total time for an agent streaming call (10 minutes).
/// Prevents slow-drip streams (one chunk every 59s) from running forever.
const AGENT_STREAM_TOTAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

use super::types::StreamingResult;
use super::RigAgentLoop;

/// Send a stream chunk to TUI, logging at debug if the channel is full/closed.
macro_rules! stream_send {
    ($tx:expr, $chunk:expr $(,)?) => {
        if let Err(e) = $tx.try_send($chunk) {
            tracing::debug!("Stream send dropped: {e}");
        }
    };
}

impl RigAgentLoop {
    // =========================================================================
    // Streaming Helpers
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

        // Apply temperature — strip for models that reject it
        if let Some(temp) = self.params.effective_temperature() {
            let model_id = self.params.model.as_deref().unwrap_or("");
            if crate::provider::rig::is_reasoning_model(model_id) {
                tracing::warn!(
                    model = model_id,
                    "temperature stripped for reasoning model in agent stream"
                );
            } else {
                request_builder = request_builder.temperature(f64::from(temp));
            }
        }

        let effective_max_tokens = self.params.effective_max_tokens().unwrap_or(8192) as u64;
        let provider_name = self
            .params
            .provider
            .as_ref()
            .map(|p| p.as_str())
            .unwrap_or("");
        let model_name = self.params.model.as_deref().unwrap_or("");
        let (rig_max, token_params) = crate::provider::rig::token_limit_for_model(
            provider_name,
            model_name,
            effective_max_tokens,
        );
        if let Some(mt) = rig_max {
            request_builder = request_builder.max_tokens(mt);
        }
        if let Some(params) = token_params {
            request_builder = request_builder.additional_params(params);
        }
        let request = request_builder.build();

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
        let mut input_tokens: u64 = 0;
        let mut output_tokens: u64 = 0;
        let mut cached_input_tokens: u64 = 0;
        // Streaming token counter for live progress display
        let mut streaming_tokens: u64 = 0;
        // PERF: Hoist Arc allocation out of the streaming loop
        let task_id_arc: Arc<str> = Arc::from(self.task_id.as_str());
        // TTFT: capture time before first token arrives
        let stream_start = Instant::now();
        let mut ttft_ms: Option<u64> = None;

        // Stream chunks with timeout protection to prevent infinite hangs
        loop {
            // Overall timeout: prevent slow-drip streams from running forever
            if stream_start.elapsed() > AGENT_STREAM_TOTAL_TIMEOUT {
                return Err(NikaError::Timeout {
                    operation: "agent stream total".to_string(),
                    duration_ms: AGENT_STREAM_TOTAL_TIMEOUT.as_millis() as u64,
                });
            }
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
                        // Capture time to first token
                        if ttft_ms.is_none() {
                            ttft_ms = Some(stream_start.elapsed().as_millis() as u64);
                        }
                        // Send token to TUI for real-time display
                        if let Some(ref tx) = self.stream_tx {
                            stream_send!(
                                tx,
                                crate::provider::rig::StreamChunk::Token(text.text.clone(),)
                            );
                        }
                        // Approximate token count: ~4 chars per token
                        let delta = (text.text.len() as u64 / 4).max(1);
                        streaming_tokens += delta;
                        // Emit StreamingDelta for live progress (every ~10 tokens to avoid flood)
                        if streaming_tokens % 10 < delta {
                            self.event_log.emit(EventKind::StreamingDelta {
                                task_id: Arc::clone(&task_id_arc),
                                delta_tokens: delta,
                                total_tokens: streaming_tokens,
                            });
                        }
                        response_parts.push(text.text);
                    }
                    StreamedAssistantContent::ReasoningDelta { reasoning, .. } => {
                        // Send thinking content to TUI
                        if let Some(ref tx) = self.stream_tx {
                            stream_send!(
                                tx,
                                crate::provider::rig::StreamChunk::Thinking(reasoning.clone(),)
                            );
                        }
                        thinking_parts.push(reasoning);
                    }
                    StreamedAssistantContent::Reasoning(reasoning) => {
                        // Final reasoning block - extract text from content blocks
                        for block in reasoning.content {
                            if let ReasoningContent::Text { text, .. } = block {
                                // Send thinking to TUI
                                if let Some(ref tx) = self.stream_tx {
                                    stream_send!(
                                        tx,
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
                            input_tokens = usage.input_tokens;
                            output_tokens = usage.output_tokens;
                            cached_input_tokens = usage.cached_input_tokens;
                            // Send final metrics to TUI
                            if let Some(ref tx) = self.stream_tx {
                                stream_send!(
                                    tx,
                                    crate::provider::rig::StreamChunk::Metrics {
                                        input_tokens: usage.input_tokens,
                                        output_tokens: usage.output_tokens,
                                    }
                                );
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

        let response_text = response_parts.concat();
        let thinking_text = if thinking_parts.is_empty() {
            None
        } else {
            Some(thinking_parts.concat())
        };

        // ── Nika Shield: per-stream canary detection on assistant text +
        //    extended-thinking trace. The end-of-execute check (in
        //    TaskExecutor::execute) catches leaks in the FINAL response,
        //    but the thinking trace is opaque to that path. Sprint 2 Item 2
        //    success criterion #4 + NIKA-388 CanaryInThinking.
        if let Some(ref shield) = self.shield {
            if shield.canary_enabled() {
                let canary = shield.canary();
                if let Some(detection) = canary.check_output(&response_text) {
                    let match_type: &'static str = match detection.match_type {
                        crate::runtime::canary::CanaryMatchType::Exact => "exact",
                        crate::runtime::canary::CanaryMatchType::Substring => "substring",
                        crate::runtime::canary::CanaryMatchType::CharSpaced => "char_spaced",
                    };
                    self.event_log.emit(EventKind::CanaryDetected {
                        task_id: Arc::clone(&task_id_arc),
                        match_type: match_type.to_string(),
                    });
                    if shield.is_strict() {
                        return Err(NikaError::CanaryLeaked {
                            task_id: self.task_id.clone(),
                            match_type,
                            token_index: detection.token_index as u8,
                        });
                    }
                }
                if let Some(ref thinking) = thinking_text {
                    if canary.check_output(thinking).is_some() {
                        // Thinking-trace leaks are stronger evidence of
                        // system-prompt exfil than output-channel leaks.
                        // We tag the match type as "thinking" so the
                        // SecuritySummary + lint can attribute the source.
                        self.event_log.emit(EventKind::CanaryDetected {
                            task_id: Arc::clone(&task_id_arc),
                            match_type: "thinking".to_string(),
                        });
                        if shield.is_strict() {
                            return Err(NikaError::CanaryInThinking {
                                task_id: self.task_id.clone(),
                            });
                        }
                    }
                }
            }
        }

        Ok(StreamingResult {
            response: response_text,
            input_tokens,
            output_tokens,
            cached_input_tokens,
            thinking: thinking_text,
            ttft_ms,
        })
    }

    /// Execute agent with tools using streaming for token tracking.
    ///
    /// This handles the case where we need both tool calling AND token tracking.
    ///
    /// **Strategy:**
    /// - No tools: Use `model.stream()` for pure streaming with full token tracking
    /// - With tools + TUI: Use `stream_prompt()` with real-time chunk delivery
    /// - With tools + CLI: Use `stream_prompt()` without TUI, still captures tokens
    ///
    /// All paths use `stream_prompt()` which provides token usage via `FinalResponse::usage()`.
    ///
    /// # Type Parameters
    /// - `M`: A rig completion model that supports streaming
    ///
    /// # Returns
    /// A `StreamingResult` with accurate token tracking in all modes.
    pub(super) async fn stream_with_tools<M>(
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
        // Inject skills into system prompt if configured
        let preamble = self.inject_skills_into_prompt().await?;
        if tools.is_empty() {
            // No tools - use pure streaming (full token tracking)
            self.stream_completion_with_tokens(&model, prompt, Some(&preamble))
                .await
        } else {
            // With tools - use REAL streaming if TUI mode (stream_tx set)
            if self.stream_tx.is_some() {
                return self
                    .stream_with_tools_streaming(model, prompt, tools, max_turns)
                    .await;
            }

            // CLI mode (no TUI): Use stream_prompt() for token tracking
            // Even without TUI, we need streaming to extract token usage from FinalResponse
            // Use preamble with injected skills
            let effective_max_tokens = self.params.effective_max_tokens().unwrap_or(8192) as u64;
            let provider_name = self
                .params
                .provider
                .as_ref()
                .map(|p| p.as_str())
                .unwrap_or("");
            let model_name = self.params.model.as_deref().unwrap_or("");
            let (rig_max, token_params) = crate::provider::rig::token_limit_for_model(
                provider_name,
                model_name,
                effective_max_tokens,
            );
            let mut builder = AgentBuilder::new(model).preamble(&preamble).tools(tools);
            if let Some(mt) = rig_max {
                builder = builder.max_tokens(mt);
            }
            if let Some(params) = token_params {
                builder = builder.additional_params(params);
            }

            // Apply temperature — strip for models that reject it
            if let Some(temp) = self.params.effective_temperature() {
                let model_id = self.params.model.as_deref().unwrap_or("");
                if crate::provider::rig::is_reasoning_model(model_id) {
                    tracing::warn!(
                        model = model_id,
                        "temperature stripped for reasoning model in agent"
                    );
                } else {
                    builder = builder.temperature(f64::from(temp));
                }
            }

            // Apply tool_choice only if explicitly set
            // Skipping redundant .tool_choice(Auto) - rig-core uses Auto by default
            if self.params.has_explicit_tool_choice() {
                let tool_choice = self.params.effective_tool_choice();
                builder = builder.tool_choice(tool_choice.into());
            }

            // Inject stop_sequences via additional_params (provider-specific key)
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

            let agent = builder.build();

            // Use stream_prompt() to get token usage via FinalResponse
            // This consumes the stream without sending to TUI, but captures token counts
            let mut stream: RigStreamingResult<_> = agent
                .stream_prompt(prompt)
                .multi_turn(max_turns.saturating_sub(1))
                .await;

            let mut response_text = String::new();
            let mut thinking_text: Option<String> = None;
            let mut input_tokens = 0u64;
            let mut output_tokens = 0u64;
            let mut cached_input_tokens = 0u64;
            let cli_stream_start = Instant::now();
            let mut cli_ttft_ms: Option<u64> = None;

            // Consume stream, extracting text and token usage
            while let Some(chunk) = {
                // Overall timeout: prevent slow-drip streams from running forever
                if cli_stream_start.elapsed() > AGENT_STREAM_TOTAL_TIMEOUT {
                    return Err(NikaError::Timeout {
                        operation: "agent stream total".to_string(),
                        duration_ms: AGENT_STREAM_TOTAL_TIMEOUT.as_millis() as u64,
                    });
                }
                // Per-chunk timeout: prevent indefinite hang on slow providers
                match timeout(STREAM_CHUNK_TIMEOUT, stream.next()).await {
                    Ok(item) => item,
                    Err(_) => {
                        return Err(NikaError::Timeout {
                            operation: "agent stream chunk".to_string(),
                            duration_ms: STREAM_CHUNK_TIMEOUT.as_millis() as u64,
                        });
                    }
                }
            } {
                match chunk {
                    Ok(item) => match item {
                        // Accumulate text chunks
                        MultiTurnStreamItem::StreamAssistantItem(
                            StreamedAssistantContent::Text(text),
                        ) => {
                            if cli_ttft_ms.is_none() {
                                cli_ttft_ms = Some(cli_stream_start.elapsed().as_millis() as u64);
                            }
                            response_text.push_str(&text.text);
                        }
                        // Capture reasoning (Claude extended thinking)
                        MultiTurnStreamItem::StreamAssistantItem(
                            StreamedAssistantContent::Reasoning(reasoning),
                        ) => {
                            let reasoning_str = reasoning
                                .content
                                .iter()
                                .filter_map(|c| match c {
                                    ReasoningContent::Text { text, .. } => Some(text.as_str()),
                                    _ => None,
                                })
                                .collect::<Vec<_>>()
                                .join("");
                            match &mut thinking_text {
                                Some(t) => t.push_str(&reasoning_str),
                                None => thinking_text = Some(reasoning_str),
                            }
                        }
                        // Final response with authoritative text and token usage
                        MultiTurnStreamItem::FinalResponse(resp) => {
                            response_text = resp.response().to_string();
                            let usage = resp.usage();
                            input_tokens = usage.input_tokens;
                            output_tokens = usage.output_tokens;
                            cached_input_tokens = usage.cached_input_tokens;
                        }
                        // Accumulate reasoning deltas (streaming thinking tokens)
                        MultiTurnStreamItem::StreamAssistantItem(
                            StreamedAssistantContent::ReasoningDelta { reasoning, .. },
                        ) => match &mut thinking_text {
                            Some(t) => t.push_str(&reasoning),
                            None => thinking_text = Some(reasoning),
                        },
                        // Ignore tool calls and other items in CLI mode
                        _ => {}
                    },
                    Err(e) => {
                        return Err(NikaError::AgentExecutionError {
                            task_id: self.task_id.clone(),
                            reason: format!("Stream error: {}", e),
                        });
                    }
                }
            }

            Ok(StreamingResult {
                response: crate::runtime::executor::verbs::strip_think_tags(&response_text),
                input_tokens, // Now tracked via FinalResponse
                output_tokens,
                cached_input_tokens,
                thinking: thinking_text,
                ttft_ms: cli_ttft_ms,
            })
        }
    }

    /// Stream agent execution with REAL-TIME token delivery
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
        // Inject skills into system prompt if configured
        let preamble = self.inject_skills_into_prompt().await?;
        let effective_max_tokens = self.params.effective_max_tokens().unwrap_or(8192) as u64;
        let provider_name = self
            .params
            .provider
            .as_ref()
            .map(|p| p.as_str())
            .unwrap_or("");
        let model_name = self.params.model.as_deref().unwrap_or("");
        let (rig_max, token_params) = crate::provider::rig::token_limit_for_model(
            provider_name,
            model_name,
            effective_max_tokens,
        );
        let mut builder = AgentBuilder::new(model).preamble(&preamble).tools(tools);
        if let Some(mt) = rig_max {
            builder = builder.max_tokens(mt);
        }
        if let Some(params) = token_params {
            builder = builder.additional_params(params);
        }

        // Apply temperature — strip for reasoning models
        if let Some(temp) = self.params.effective_temperature() {
            if crate::provider::rig::is_reasoning_model(self.params.model.as_deref().unwrap_or(""))
            {
                tracing::warn!(
                    model = self.params.model.as_deref().unwrap_or("unknown"),
                    "Stripping temperature for reasoning model in streaming agent path"
                );
            } else {
                builder = builder.temperature(f64::from(temp));
            }
        }

        if self.params.has_explicit_tool_choice() {
            let tool_choice = self.params.effective_tool_choice();
            builder = builder.tool_choice(tool_choice.into());
        }

        // Inject stop_sequences via additional_params (provider-specific key)
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

        let agent = builder.build();

        // STREAMING with stream_prompt()
        // Note: multi_turn() sets max tool call rounds (0 = single turn, >0 = multi-turn)
        // The stream is created directly, errors come from individual items
        let mut stream: RigStreamingResult<_> = agent
            .stream_prompt(prompt)
            .multi_turn(max_turns.saturating_sub(1))
            .await;

        let mut response_text = String::new();
        let mut thinking_text: Option<String> = None;
        let mut input_tokens = 0u64;
        let mut output_tokens = 0u64;
        let mut cached_input_tokens = 0u64;
        let mut tool_count = 0u32;
        let mut streaming_tokens: u64 = 0;
        let tui_stream_start = Instant::now();
        let mut tui_ttft_ms: Option<u64> = None;
        // PERF: Hoist Arc allocation out of the streaming loop
        let task_id_arc: Arc<str> = Arc::from(self.task_id.as_str());

        // Per-chunk timeout to prevent hanging streams
        loop {
            // Overall timeout: prevent slow-drip streams from running forever
            if tui_stream_start.elapsed() > AGENT_STREAM_TOTAL_TIMEOUT {
                return Err(NikaError::Timeout {
                    operation: "agent stream total".to_string(),
                    duration_ms: AGENT_STREAM_TOTAL_TIMEOUT.as_millis() as u64,
                });
            }
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
                        stream_send!(
                            tx,
                            crate::provider::rig::StreamChunk::Error(format!(
                                "Stream timeout: no chunk received for {}s",
                                STREAM_CHUNK_TIMEOUT.as_secs()
                            ))
                        );
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
                        // Capture time to first token
                        if tui_ttft_ms.is_none() {
                            tui_ttft_ms = Some(tui_stream_start.elapsed().as_millis() as u64);
                        }
                        if let Some(ref tx) = self.stream_tx {
                            stream_send!(
                                tx,
                                crate::provider::rig::StreamChunk::Token(text.text.clone(),)
                            );
                        }
                        // StreamingDelta for live progress
                        let delta = (text.text.len() as u64 / 4).max(1);
                        streaming_tokens += delta;
                        if streaming_tokens % 10 < delta {
                            self.event_log.emit(EventKind::StreamingDelta {
                                task_id: Arc::clone(&task_id_arc),
                                delta_tokens: delta,
                                total_tokens: streaming_tokens,
                            });
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

                        // Serialize args once, reuse for TUI and event log
                        let args_string = serde_json::to_string(&tool_call.function.arguments)
                            .unwrap_or_default();
                        let args_value: Option<Value> = serde_json::from_str(&args_string).ok();

                        // Send McpCallStart to TUI
                        if let Some(ref tx) = self.stream_tx {
                            stream_send!(
                                tx,
                                crate::provider::rig::StreamChunk::McpCallStart {
                                    tool: tool_name.clone(),
                                    server: "agent".to_string(),
                                    params: args_string,
                                }
                            );
                        }

                        // Log event for observability
                        self.event_log.emit(EventKind::McpInvoke {
                            task_id: Arc::clone(&task_id_arc),
                            call_id,
                            mcp_server: "agent".to_string(),
                            tool: Some(tool_name),
                            resource: None,
                            params: args_value.map(Arc::new),
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
                            stream_send!(
                                tx,
                                crate::provider::rig::StreamChunk::Thinking(reasoning_str.clone(),)
                            );
                        }
                        match &mut thinking_text {
                            Some(t) => t.push_str(&reasoning_str),
                            None => thinking_text = Some(reasoning_str),
                        }
                    }

                    // Final response with token usage
                    // NOTE: We intentionally overwrite accumulated response_text here.
                    // FinalResponse from rig-core is authoritative - it contains the
                    // complete response and accurate token counts from the LLM provider.
                    // The streaming accumulation (push_str) is only for real-time TUI display.
                    MultiTurnStreamItem::FinalResponse(resp) => {
                        response_text = resp.response().to_string();
                        let usage = resp.usage();
                        input_tokens = usage.input_tokens;
                        output_tokens = usage.output_tokens;
                        cached_input_tokens = usage.cached_input_tokens;

                        // Send metrics to TUI
                        if let Some(ref tx) = self.stream_tx {
                            stream_send!(
                                tx,
                                crate::provider::rig::StreamChunk::Metrics {
                                    input_tokens: usage.input_tokens,
                                    output_tokens: usage.output_tokens,
                                }
                            );
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
            response: crate::runtime::executor::verbs::strip_think_tags(&response_text),
            input_tokens,
            output_tokens,
            cached_input_tokens,
            thinking: thinking_text,
            ttft_ms: tui_ttft_ms,
        })
    }
}
