// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Streaming inference methods for `RigProvider`.
//!
//! Covers text streaming, vision streaming, and streaming with options.

use super::*;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::time::timeout;

impl RigProvider {
    /// Stream text completion with real-time token updates
    ///
    /// Sends tokens to the provided channel as they arrive from the model.
    /// This enables real-time display in the TUI like Claude Code / Gemini.
    ///
    /// # Arguments
    /// * `prompt` - The text prompt to send
    /// * `tx` - Channel sender for streaming chunks
    ///
    /// # Returns
    /// `StreamResult` containing complete response text and token usage metrics
    pub async fn infer_stream(
        &self,
        prompt: &str,
        tx: mpsc::Sender<StreamChunk>,
        model: Option<&str>,
        max_tokens: Option<u64>,
    ) -> Result<StreamResult, RigInferError> {
        // Overall timeout for entire streaming operation (10 min default).
        // Individual chunks have their own 60s timeout in consume_rig_stream,
        // but a slow stream (1 chunk every 59s) would never hit that.
        const STREAM_TOTAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

        // Use endpoint-specific timeout for OpenAiCompat (doubled for streaming headroom)
        let effective_timeout = match self {
            RigProvider::OpenAiCompat { timeout_secs, .. } => {
                std::time::Duration::from_secs((*timeout_secs).max(60) * 2)
            }
            _ => STREAM_TOTAL_TIMEOUT,
        };

        timeout(
            effective_timeout,
            self.infer_stream_inner(prompt, tx, model, max_tokens),
        )
        .await
        .map_err(|_| RigInferError::Timeout {
            duration_ms: effective_timeout.as_millis() as u64,
        })?
    }

    async fn infer_stream_inner(
        &self,
        prompt: &str,
        tx: mpsc::Sender<StreamChunk>,
        model: Option<&str>,
        max_tokens: Option<u64>,
    ) -> Result<StreamResult, RigInferError> {
        let model_id = model.unwrap_or_else(|| self.default_model());
        let effective_max_tokens = max_tokens.unwrap_or(8192);
        let mut response_buf = String::with_capacity(4096);
        let mut result = StreamResult::default();

        match self {
            // Native provider — dedicated non-rig-core streaming path
            #[cfg(feature = "native-inference")]
            RigProvider::Native(runtime) => {
                use futures::StreamExt;
                use std::pin::pin;

                // B-1 fix · auto-load on first call (see also the
                // `_with_options` sibling above).
                runtime
                    .ensure_loaded(model_id)
                    .await
                    .map_err(|e: super::super::native::NativeError| {
                        RigInferError::PromptError(e.to_string())
                    })?;

                // Native inference now supports streaming via mistral.rs
                let stream = runtime
                    .infer_stream(prompt, super::super::native::ChatOptions::default())
                    .await
                    .map_err(|e: super::super::native::NativeError| {
                        RigInferError::PromptError(e.to_string())
                    })?;

                // Pin the stream for iteration (async_stream produces !Unpin streams)
                let mut stream = pin!(stream);

                // Collect tokens as they arrive (Stream yields Result<String, NativeError>)
                while let Some(result) = stream.next().await {
                    match result {
                        Ok(token) => {
                            response_buf.push_str(&token);
                            let _ = tx.try_send(StreamChunk::Token(token));
                        }
                        Err(e) => {
                            let _ = tx.try_send(StreamChunk::Error(e.to_string()));
                            return Err(RigInferError::PromptError(e.to_string()));
                        }
                    }
                }

                // Post-hoc token estimation (chars/4 heuristic — native
                // streaming doesn't return usage metadata).
                result.input_tokens = (prompt.len() as u64).div_ceil(4);
                result.output_tokens = (response_buf.len() as u64).div_ceil(4);
            }
            // All rig-core providers (Claude, OpenAI, Mistral, Groq, DeepSeek, Gemini, XAi, OpenAiCompat)
            _ => {
                let is_anthropic = self.is_anthropic();
                let (rig_max, token_params) =
                    token_limit_for_model(self.name(), model_id, effective_max_tokens);
                dispatch_rig!(self, |client| {
                    let model = client.completion_model(model_id);
                    let mut rb = model.completion_request(prompt);
                    if let Some(mt) = rig_max {
                        rb = rb.max_tokens(mt);
                    }
                    if let Some(params) = token_params.clone() {
                        rb = rb.additional_params(params);
                    }
                    let request = rb.build();
                    let stream_start = Instant::now();
                    let mut stream = model
                        .stream(request)
                        .await
                        .map_err(|e| RigInferError::PromptError(e.to_string()))?;
                    consume_rig_stream(
                        &mut stream,
                        &tx,
                        &mut response_buf,
                        &mut result,
                        is_anthropic,
                        stream_start,
                    )
                    .await?;
                });
            }
        }

        let _ = tx.try_send(StreamChunk::Done(response_buf.clone()));

        // Fallback: if stream ended without Final event, estimate tokens
        if result.input_tokens == 0 && result.output_tokens == 0 && !response_buf.is_empty() {
            result.input_tokens = (prompt.len() as u64).div_ceil(4);
            result.output_tokens = (response_buf.len() as u64).div_ceil(4);
        }

        // Send metrics after Done - use try_send to avoid blocking
        let _ = tx.try_send(StreamChunk::Metrics {
            input_tokens: result.input_tokens,
            output_tokens: result.output_tokens,
        });

        result.text = response_buf;
        Ok(result)
    }

    /// Stream inference with LLM control options
    ///
    /// Similar to `infer_stream` but accepts `InferOptions` for temperature,
    /// max_tokens, and system prompt control.
    ///
    /// # Arguments
    /// * `prompt` - The user prompt text
    /// * `tx` - Channel sender for streaming chunks
    /// * `options` - LLM control options (temperature, max_tokens, system)
    ///
    /// # Returns
    /// `StreamResult` containing complete response text and token usage metrics
    pub async fn infer_stream_with_options(
        &self,
        prompt: &str,
        tx: mpsc::Sender<StreamChunk>,
        options: &InferOptions,
    ) -> Result<StreamResult, RigInferError> {
        const STREAM_TOTAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

        // T16: Use endpoint-specific timeout for OpenAiCompat (doubled for streaming headroom)
        let effective_timeout = match self {
            RigProvider::OpenAiCompat { timeout_secs, .. } => {
                std::time::Duration::from_secs((*timeout_secs).max(60) * 2)
            }
            _ => STREAM_TOTAL_TIMEOUT,
        };

        timeout(
            effective_timeout,
            self.infer_stream_with_options_inner(prompt, tx, options),
        )
        .await
        .map_err(|_| RigInferError::Timeout {
            duration_ms: effective_timeout.as_millis() as u64,
        })?
    }

    async fn infer_stream_with_options_inner(
        &self,
        prompt: &str,
        tx: mpsc::Sender<StreamChunk>,
        options: &InferOptions,
    ) -> Result<StreamResult, RigInferError> {
        let model_id = options
            .model
            .as_deref()
            .unwrap_or_else(|| self.default_model());
        let max_tokens = options.max_tokens.unwrap_or(8192).max(16);
        let mut response_buf = String::with_capacity(4096);
        let mut result = StreamResult::default();

        let effective_temperature = effective_temperature_for_model(model_id, options.temperature);
        let (rig_max, token_params) =
            token_limit_for_model(self.name(), model_id, max_tokens as u64);

        // Helper: build request with options and start streaming
        // Uses preamble() for system prompt (not string concatenation) to ensure
        // providers treat it as a system message, not user text.
        macro_rules! build_request_with_options {
            ($client:expr) => {{
                let model = $client.completion_model(model_id);
                let mut rb = model.completion_request(prompt);
                if let Some(mt) = rig_max {
                    rb = rb.max_tokens(mt);
                }
                if let Some(ref params) = token_params {
                    rb = rb.additional_params(params.clone());
                }
                if let Some(ref system) = options.system {
                    rb = rb.preamble(system.clone());
                }
                if let Some(temp) = effective_temperature {
                    rb = rb.temperature(temp);
                }
                if let Some(ref params) = options.additional_params {
                    rb = rb.additional_params(params.clone());
                }
                model
                    .stream(rb.build())
                    .await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))?
            }};
        }

        match self {
            // Native provider — dedicated non-rig-core streaming path with options
            #[cfg(feature = "native-inference")]
            RigProvider::Native(runtime) => {
                use futures::StreamExt;
                use std::pin::pin;

                // B-1 fix · auto-load on first call (kernel-bridge text path
                // routes through here from `Provider::infer` so the CLI infer
                // verb hits this site BEFORE the simpler `RigProvider::infer`
                // arm — both paths now call `ensure_loaded` to share the
                // interior-mutable LoadedState introduced in b47595f5f).
                runtime
                    .ensure_loaded(model_id)
                    .await
                    .map_err(|e: super::super::native::NativeError| {
                        RigInferError::PromptError(e.to_string())
                    })?;

                // Native doesn't support preamble — concatenate system prompt for native only
                let native_prompt = if let Some(ref system) = options.system {
                    format!("{}\n\n{}", system, prompt)
                } else {
                    prompt.to_string()
                };
                let chat_options = super::super::native::ChatOptions {
                    temperature: effective_temperature.map(|t| t as f32),
                    max_tokens: options.max_tokens,
                    ..Default::default()
                };
                let stream = runtime
                    .infer_stream(&native_prompt, chat_options)
                    .await
                    .map_err(|e: super::super::native::NativeError| {
                        RigInferError::PromptError(e.to_string())
                    })?;

                // Pin the stream for iteration (async_stream produces !Unpin streams)
                let mut stream = pin!(stream);

                // Collect tokens as they arrive (Stream yields Result<String, NativeError>)
                while let Some(result) = stream.next().await {
                    match result {
                        Ok(token) => {
                            response_buf.push_str(&token);
                            let _ = tx.try_send(StreamChunk::Token(token));
                        }
                        Err(e) => {
                            let _ = tx.try_send(StreamChunk::Error(e.to_string()));
                            return Err(RigInferError::PromptError(e.to_string()));
                        }
                    }
                }

                // Post-hoc token estimation (chars/4 heuristic — native
                // streaming doesn't return usage metadata).
                result.input_tokens = (native_prompt.len() as u64).div_ceil(4);
                result.output_tokens = (response_buf.len() as u64).div_ceil(4);
            }
            // All rig-core providers
            _ => {
                let is_anthropic = self.is_anthropic();
                dispatch_rig!(self, |client| {
                    let stream_start = Instant::now();
                    let mut stream = build_request_with_options!(client);
                    consume_rig_stream(
                        &mut stream,
                        &tx,
                        &mut response_buf,
                        &mut result,
                        is_anthropic,
                        stream_start,
                    )
                    .await?;
                });
            }
        }

        let _ = tx.try_send(StreamChunk::Done(response_buf.clone()));

        // Fallback: if stream ended without Final event, estimate tokens
        if result.input_tokens == 0 && result.output_tokens == 0 && !response_buf.is_empty() {
            result.input_tokens = (prompt.len() as u64).div_ceil(4);
            result.output_tokens = (response_buf.len() as u64).div_ceil(4);
        }

        let _ = tx.try_send(StreamChunk::Metrics {
            input_tokens: result.input_tokens,
            output_tokens: result.output_tokens,
        });

        result.text = response_buf;
        Ok(result)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// NATIVE VISION HELPER
// ═══════════════════════════════════════════════════════════════════════════

/// Extract text prompt and `VisionImage` instances from rig `UserContent` parts.
///
/// The executor builds `Vec<UserContent>` with base64-encoded images for cloud providers.
/// For native inference, we need to decode the base64 back into raw bytes and produce
/// `VisionImage` instances that `NativeRuntime::infer_vision()` can consume.
///
/// # Returns
/// `(prompt_text, vision_images)` where prompt_text is all text parts joined by newlines.
#[cfg(feature = "native-inference")]
pub(super) fn extract_native_vision_parts(
    user_content: &[rig::completion::message::UserContent],
) -> Result<(String, Vec<crate::core::backend::VisionImage>), RigInferError> {
    use base64::Engine as _;
    use rig::completion::message::{DocumentSourceKind, Image, UserContent};

    let mut text_parts: Vec<String> = Vec::new();
    let mut images: Vec<crate::core::backend::VisionImage> = Vec::new();

    for part in user_content {
        match part {
            UserContent::Text(text) => {
                text_parts.push(text.text.clone());
            }
            UserContent::Image(Image {
                data, media_type, ..
            }) => {
                let bytes = match data {
                    DocumentSourceKind::Base64(b64) => base64::engine::general_purpose::STANDARD
                        .decode(b64)
                        .map_err(|e| {
                            RigInferError::PromptError(format!(
                                "Failed to decode base64 image for native vision: {}",
                                e
                            ))
                        })?,
                    DocumentSourceKind::Raw(raw) => raw.clone(),
                    DocumentSourceKind::Url(url) => {
                        return Err(RigInferError::VisionNotSupported(format!(
                            "Native vision does not support URL images. Pre-fetch the image: {}",
                            url
                        )));
                    }
                    _ => {
                        return Err(RigInferError::PromptError(
                            "Unsupported image source kind for native vision".to_string(),
                        ));
                    }
                };

                // Map rig's ImageMediaType to MIME string
                let mime = media_type
                    .as_ref()
                    .map(|mt| match mt {
                        rig::completion::message::ImageMediaType::JPEG => "image/jpeg",
                        rig::completion::message::ImageMediaType::PNG => "image/png",
                        rig::completion::message::ImageMediaType::GIF => "image/gif",
                        rig::completion::message::ImageMediaType::WEBP => "image/webp",
                        _ => "image/png", // Default fallback
                    })
                    .unwrap_or("image/png");

                images.push(crate::core::backend::VisionImage::new(bytes, mime));
            }
            // Skip non-image/text content (tool results, audio, etc.)
            _ => {}
        }
    }

    Ok((text_parts.join("\n"), images))
}
