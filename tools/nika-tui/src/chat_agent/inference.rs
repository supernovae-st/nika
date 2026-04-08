// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! LLM inference for the chat agent
//!
//! Handles sending prompts to the LLM provider and streaming responses
//! back to the TUI in real-time.

use nika_engine::error::NikaError;
use nika_engine::provider::rig::StreamChunk;

use super::types::ChatMessage;
use super::ChatAgent;

impl ChatAgent {
    /// Execute an infer command (LLM text generation)
    ///
    /// # Arguments
    ///
    /// * `prompt` - The text prompt to send to the LLM
    ///
    /// # Returns
    ///
    /// The completion text from the model.
    ///
    /// # Errors
    ///
    /// Returns `NikaError::ProviderApiError` if the API call fails.
    ///
    /// # Streaming
    ///
    /// If `stream_chunk_tx` is set, tokens are streamed in real-time via
    /// `StreamChunk::Token` events, enabling Claude Code-like UX.
    pub async fn infer(&mut self, prompt: &str) -> Result<String, NikaError> {
        // Add user message to history
        self.history.push(ChatMessage::user(prompt));

        // Start streaming state
        self.streaming_state.start();

        // Run inference — capture result without early-return so we can always clean up
        let result = self.run_inference_inner(prompt).await;

        // Always finish streaming regardless of success/failure
        self.streaming_state.finish();

        match result {
            Ok(response) => {
                // Add assistant message to history
                self.history.push(ChatMessage::assistant(&response));

                // Send completion to streaming channel
                if let Some(tx) = &self.streaming_tx {
                    let _ = tx.send(response.clone()).await;
                }

                Ok(response)
            }
            Err(e) => {
                // Roll back dangling user message — keeps history consistent
                self.history.pop();
                Err(e)
            }
        }
    }

    /// Inner inference logic — called by `infer` which handles cleanup.
    async fn run_inference_inner(&mut self, prompt: &str) -> Result<String, NikaError> {
        // Send prompt to streaming channel if available
        if let Some(tx) = &self.streaming_tx {
            let _ = tx
                .send(format!("Sending to {}...", self.provider.name()))
                .await;
        }

        // Use streaming if stream_chunk_tx is set, otherwise blocking
        if let Some(tx) = self.stream_chunk_tx.clone() {
            // Clone tx for metrics send (infer_stream takes ownership)
            let metrics_tx = tx.clone();

            // Real-time streaming - tokens appear as they arrive
            let result = self
                .provider
                .infer_stream(prompt, tx, self.model_override.as_deref(), None)
                .await
                .map_err(|e| NikaError::ProviderApiError {
                    message: e.to_string(),
                })?;

            // Accumulate token metrics for status bar display
            self.total_input_tokens += result.input_tokens;
            self.total_output_tokens += result.output_tokens;

            // Send metrics to UI for status bar update
            let _ = metrics_tx
                .send(StreamChunk::Metrics {
                    input_tokens: result.input_tokens,
                    output_tokens: result.output_tokens,
                })
                .await;

            Ok(result.text)
        } else {
            // Blocking call - full response at once
            self.provider
                .infer(prompt, None, None)
                .await
                .map_err(|e| NikaError::ProviderApiError {
                    message: e.to_string(),
                })
        }
    }
}
