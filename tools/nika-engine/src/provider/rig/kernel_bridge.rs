// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Bridge: `impl nika_kernel::provider::Provider for RigProvider`
//!
//! Connects the kernel Provider trait (L0.5) to the existing RigProvider enum.
//! The `dispatch_rig!` macro lives on inside the trait impl — connect, don't delete.
//!
//! ## Design
//!
//! The kernel Provider trait unifies all inference modes into two methods:
//! - `infer(InferRequest)` — non-streaming (text, vision, tool use)
//! - `infer_stream(InferRequest)` — streaming
//!
//! InferRequest encodes mode via its content:
//! - Empty `tools` → plain inference
//! - Non-empty `tools` → tool use
//! - Image ContentBlocks → vision
//!
//! This bridge delegates to the existing RigProvider methods (infer_with_options,
//! infer_vision, infer_stream_with_options, etc.), converting between kernel DTOs
//! and rig-core types.

use std::pin::Pin;
use std::time::Instant;

use futures::stream::Stream;
use tokio::sync::mpsc;

use nika_kernel::provider::{
    ContentBlock, InferEvent, InferRequest, InferResponse, InferStream, Message, ProviderError,
    Role, StopReason, TokenUsage,
};

use super::stream::StreamChunk;
use super::{InferOptions, RigProvider};

// ─────────────────────────────────────────────────────────────────────
// Request parsing helpers
// ─────────────────────────────────────────────────────────────────────

/// Extract the system prompt from messages (first System role message).
fn extract_system(messages: &[Message]) -> Option<String> {
    messages.iter().find_map(|m| {
        if m.role == Role::System {
            let parts: Vec<&str> = m
                .content
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect();
            let text = parts.join("\n");
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        } else {
            None
        }
    })
}

/// Extract the user prompt text from messages (concatenated Text blocks from User messages).
fn extract_user_prompt(messages: &[Message]) -> String {
    messages
        .iter()
        .filter(|m| m.role == Role::User)
        .flat_map(|m| m.content.iter())
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Check if any User message contains Image content blocks.
fn has_vision_content(messages: &[Message]) -> bool {
    messages.iter().any(|m| {
        m.role == Role::User
            && m.content
                .iter()
                .any(|b| matches!(b, ContentBlock::Image { .. }))
    })
}

/// Convert kernel User messages to rig-core `UserContent` for vision inference.
///
/// Assumes `source` in Image blocks contains base64-encoded image data
/// (CAS hashes must be resolved to base64 by the caller before building InferRequest).
fn to_rig_user_content(
    messages: &[Message],
) -> Vec<rig::completion::message::UserContent> {
    use rig::completion::message::{
        DocumentSourceKind, Image, ImageDetail, ImageMediaType, Text, UserContent,
    };

    messages
        .iter()
        .filter(|m| m.role == Role::User)
        .flat_map(|m| m.content.iter())
        .filter_map(|block| match block {
            ContentBlock::Text { text } => {
                Some(UserContent::Text(Text { text: text.clone() }))
            }
            ContentBlock::Image { source, detail } => {
                // Detect media type from base64 header prefix (best-effort).
                let media_type = if source.starts_with("/9j/") {
                    Some(ImageMediaType::JPEG)
                } else if source.starts_with("R0lGOD") {
                    Some(ImageMediaType::GIF)
                } else if source.starts_with("UklGR") {
                    Some(ImageMediaType::WEBP)
                } else {
                    // Default to PNG for unknown prefixes
                    Some(ImageMediaType::PNG)
                };

                let image_detail = detail.as_deref().map(|d| match d {
                    "high" => ImageDetail::High,
                    "low" => ImageDetail::Low,
                    _ => ImageDetail::Auto,
                });

                Some(UserContent::Image(Image {
                    data: DocumentSourceKind::Base64(source.clone()),
                    media_type,
                    detail: image_detail,
                    additional_params: None,
                }))
            }
            _ => None,
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────
// Error conversion
// ─────────────────────────────────────────────────────────────────────

/// Convert engine RigInferError → kernel ProviderError.
fn convert_error(e: super::error::RigInferError) -> ProviderError {
    use super::error::RigInferError;

    match e {
        RigInferError::Timeout { duration_ms } => ProviderError::Api {
            message: format!("timeout after {duration_ms}ms"),
        },
        RigInferError::VisionNotSupported(msg) => ProviderError::Api { message: msg },
        RigInferError::PromptError(msg) => {
            let lower = msg.to_lowercase();
            if lower.contains("401")
                || lower.contains("unauthorized")
                || lower.contains("invalid api key")
            {
                ProviderError::AuthFailed {
                    provider: "unknown".into(),
                }
            } else if lower.contains("rate limit") || lower.contains("429") {
                ProviderError::RateLimited {
                    retry_after_ms: 0,
                }
            } else {
                ProviderError::Api { message: msg }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Response builders
// ─────────────────────────────────────────────────────────────────────

/// Build an InferResponse from a plain text result (vision fallback).
///
/// Used ONLY for the vision path where `RigProvider::infer_vision` returns
/// a `String` without metadata. The text path uses
/// [`stream_result_to_infer_response`] instead (S17-A0).
fn vision_text_response(text: String, start: Instant) -> InferResponse {
    let mut response = InferResponse::new(
        vec![ContentBlock::Text { text }],
        TokenUsage::default(),
        StopReason::EndTurn,
    );
    response.ttft_ms = Some(start.elapsed().as_millis() as u64);
    response
}

/// Convert a [`StreamResult`] into an [`InferResponse`] with real metadata.
///
/// S17-A0: Prior to this, `Provider::infer()` used `infer_with_options()`
/// which returns only a `String` — losing token counts, request_id, ttft_ms,
/// and finish_reason. By routing through `infer_stream_with_options()` and
/// converting the `StreamResult`, the kernel trait now returns rich metadata
/// that the verb crate's golden oracle tests assert on.
fn stream_result_to_infer_response(
    sr: super::stream::StreamResult,
    model: &str,
) -> InferResponse {
    let stop_reason = sr
        .finish_reason
        .as_ref()
        .map(finish_reason_to_stop_reason)
        .unwrap_or(StopReason::EndTurn);

    let mut response = InferResponse::new(
        vec![ContentBlock::Text { text: sr.text }],
        TokenUsage {
            input_tokens: sr.input_tokens,
            output_tokens: sr.output_tokens,
            cache_read_tokens: if sr.cached_input_tokens > 0 {
                Some(sr.cached_input_tokens)
            } else {
                None
            },
            cache_write_tokens: None,
        },
        stop_reason,
    );
    response.ttft_ms = sr.ttft_ms;
    response.request_id = sr.request_id;
    // P0-2 fix: compute cost from the static pricing catalog so the verb
    // crate emits ProviderResponded with real cost_usd instead of 0.0.
    // Prior to this fix, cost_usd was None → unwrap_or(0.0) in the verb
    // crate, making cost tracking non-functional for ~90% of calls.
    response.cost_usd = nika_core::catalogs::estimate_cost(
        model,
        sr.input_tokens,
        sr.output_tokens,
    )
    .map(|c| c.usd);
    response
}

/// Reverse mapping: event-log [`FinishReason`] → kernel [`StopReason`].
///
/// Inverse of `nika_verb_infer::stop_reason_to_finish_reason`. Used only
/// inside the kernel bridge when converting `StreamResult` (which stores
/// `FinishReason` from the streaming pipeline) back to `StopReason`
/// (which `InferResponse` requires). The round-trip is slightly lossy
/// for `ContentFilter` vs `Unknown` but acceptable — the verb crate
/// re-maps `StopReason → FinishReason` with the full logic at event
/// emission time (invariant #21).
fn finish_reason_to_stop_reason(fr: &nika_event::types::FinishReason) -> StopReason {
    use nika_event::types::FinishReason;
    match fr {
        FinishReason::EndTurn | FinishReason::Stop => StopReason::EndTurn,
        FinishReason::MaxTokens => StopReason::MaxTokens,
        FinishReason::StopSequence => StopReason::StopSequence,
        FinishReason::ToolUse => StopReason::ToolUse,
        FinishReason::Mock => StopReason::EndTurn,
        FinishReason::StructuredOutputRetry | FinishReason::StructuredOutputRepair => {
            StopReason::EndTurn
        }
        FinishReason::Other(s) => {
            if s == "content_filter" || s == "safety" || s == "policy_violation" {
                StopReason::ContentFilter
            } else {
                StopReason::Unknown(s.clone())
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Stream adapter: StreamChunk → InferEvent
// ─────────────────────────────────────────────────────────────────────

/// Convert an mpsc receiver of `StreamChunk` into a kernel `InferEvent` Stream.
///
/// Skips TUI-specific events (McpConnected, InferStart, etc.) that are
/// irrelevant to the kernel abstraction.
fn stream_from_rx(
    rx: mpsc::Receiver<StreamChunk>,
) -> Pin<Box<dyn Stream<Item = Result<InferEvent, ProviderError>> + Send>> {
    Box::pin(futures::stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Some(chunk) => {
                    let event = match chunk {
                        StreamChunk::Token(text) => {
                            Some(Ok(InferEvent::Delta(text)))
                        }
                        StreamChunk::Thinking(text) => {
                            Some(Ok(InferEvent::Thinking(text)))
                        }
                        StreamChunk::Done(_) => {
                            // rig's StreamChunk::Done carries no provider metadata —
                            // request_id and finish_reason_raw would require a deeper
                            // rig integration (response headers). When W14-B2 lands
                            // the infer bridge surgery, this will be enriched from
                            // the rig client's final response metadata.
                            Some(Ok(InferEvent::Done {
                                stop_reason: StopReason::EndTurn,
                                request_id: None,
                                finish_reason_raw: None,
                            }))
                        }
                        StreamChunk::Error(msg) => {
                            Some(Err(ProviderError::Api { message: msg }))
                        }
                        StreamChunk::Metrics {
                            input_tokens,
                            output_tokens,
                        } => Some(Ok(InferEvent::Usage(TokenUsage {
                            input_tokens,
                            output_tokens,
                            cache_read_tokens: None,
                            cache_write_tokens: None,
                        }))),
                        // Skip TUI-specific events — they carry no inference data
                        _ => continue,
                    };
                    return event.map(|e| (e, rx));
                }
                None => return None,
            }
        }
    }))
}

// ─────────────────────────────────────────────────────────────────────
// impl Provider for RigProvider
// ─────────────────────────────────────────────────────────────────────

#[async_trait::async_trait]
impl nika_kernel::provider::Provider for RigProvider {
    fn name(&self) -> &str {
        RigProvider::name(self)
    }

    fn capabilities(
        &self,
        model: &str,
    ) -> Option<nika_kernel::provider::ModelCapabilities> {
        Some(nika_core::catalogs::model_capabilities(self.name(), model))
    }

    async fn infer(
        &self,
        request: InferRequest,
    ) -> Result<InferResponse, ProviderError> {
        let system = extract_system(&request.messages);
        let has_vision = has_vision_content(&request.messages);

        if has_vision {
            // ── Vision path (metadata-poor, uses non-streaming) ─────
            // Vision streaming exists but the vision path is out of
            // scope for S17-A0 — metadata enrichment is S18+ work.
            let start = Instant::now();
            let user_content = to_rig_user_content(&request.messages);
            let text = self
                .infer_vision(
                    user_content,
                    Some(request.model.as_str()),
                    system.as_deref(),
                    request.max_tokens,
                )
                .await
                .map_err(convert_error)?;
            Ok(vision_text_response(text, start))
        } else {
            // ── Text path (S17-A0: streaming for real metadata) ─────
            // Route through `infer_stream_with_options` internally so
            // the returned `StreamResult` carries real token counts,
            // request_id, and ttft_ms. Prior to S17, this called
            // `infer_with_options` which returned only a `String`.
            let prompt = extract_user_prompt(&request.messages);
            let opts = InferOptions {
                model: Some(request.model.clone()),
                temperature: request.temperature.map(f64::from),
                max_tokens: request.max_tokens,
                system,
                additional_params: None,
            };

            // Drain channel — we only need the StreamResult metadata.
            let (tx, mut rx) = mpsc::channel::<StreamChunk>(32);
            tokio::spawn(async move { while rx.recv().await.is_some() {} });

            let stream_result = self
                .infer_stream_with_options(&prompt, tx, &opts)
                .await
                .map_err(convert_error)?;

            Ok(stream_result_to_infer_response(stream_result, &request.model))
        }
    }

    fn supports_response_format(&self) -> bool {
        RigProvider::supports_native_structured_output(self)
    }

    async fn infer_stream(
        &self,
        request: InferRequest,
    ) -> Result<InferStream, ProviderError> {
        let has_vision = has_vision_content(&request.messages);
        let system = extract_system(&request.messages);

        if has_vision {
            // ── Vision streaming ─────────────────────────────────────
            let user_content = to_rig_user_content(&request.messages);
            let (tx, rx) = mpsc::channel(64);
            let provider = self.clone();
            let model = request.model.clone();
            let max_tokens = request.max_tokens;

            tokio::spawn(async move {
                if let Err(e) = provider
                    .infer_vision_stream(
                        user_content,
                        tx.clone(),
                        Some(&model),
                        system.as_deref(),
                        max_tokens,
                    )
                    .await
                {
                    let _ = tx.send(StreamChunk::Error(e.to_string())).await;
                }
            });

            Ok(stream_from_rx(rx))
        } else {
            // ── Text streaming with options ──────────────────────────
            let prompt = extract_user_prompt(&request.messages);
            let (tx, rx) = mpsc::channel(64);
            let provider = self.clone();
            let opts = InferOptions {
                model: Some(request.model.clone()),
                temperature: request.temperature.map(f64::from),
                max_tokens: request.max_tokens,
                system,
                additional_params: None,
            };

            tokio::spawn(async move {
                if let Err(e) = provider
                    .infer_stream_with_options(&prompt, tx.clone(), &opts)
                    .await
                {
                    let _ = tx.send(StreamChunk::Error(e.to_string())).await;
                }
            });

            Ok(stream_from_rx(rx))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::provider::Provider;

    /// Build a simple text InferRequest.
    fn text_request(prompt: &str) -> InferRequest {
        InferRequest {
            model: "mock-model".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: prompt.to_string(),
                }],
            }],
            temperature: None,
            max_tokens: None,
            tools: vec![],
            tool_choice: Default::default(),
            response_format: Default::default(),
            stop_sequences: vec![],
            thinking_budget: None,
            extra: Default::default(),
        }
    }

    /// Build a request with system + user messages.
    fn text_request_with_system(system: &str, prompt: &str) -> InferRequest {
        InferRequest {
            model: "mock-model".to_string(),
            messages: vec![
                Message {
                    role: Role::System,
                    content: vec![ContentBlock::Text {
                        text: system.to_string(),
                    }],
                },
                Message {
                    role: Role::User,
                    content: vec![ContentBlock::Text {
                        text: prompt.to_string(),
                    }],
                },
            ],
            temperature: Some(0.5),
            max_tokens: Some(100),
            tools: vec![],
            tool_choice: Default::default(),
            response_format: Default::default(),
            stop_sequences: vec![],
            thinking_budget: None,
            extra: Default::default(),
        }
    }

    // ─── Helper extraction tests ─────────────────────────────────────

    #[test]
    fn test_extract_system_present() {
        let req = text_request_with_system("Be helpful", "Hello");
        assert_eq!(extract_system(&req.messages), Some("Be helpful".into()));
    }

    #[test]
    fn test_extract_system_absent() {
        let req = text_request("Hello");
        assert_eq!(extract_system(&req.messages), None);
    }

    #[test]
    fn test_extract_user_prompt() {
        let req = text_request("Hello world");
        assert_eq!(extract_user_prompt(&req.messages), "Hello world");
    }

    #[test]
    fn test_extract_user_prompt_multi_message() {
        let req = InferRequest {
            model: "test".into(),
            messages: vec![
                Message {
                    role: Role::System,
                    content: vec![ContentBlock::Text {
                        text: "sys".into(),
                    }],
                },
                Message {
                    role: Role::User,
                    content: vec![ContentBlock::Text {
                        text: "first".into(),
                    }],
                },
                Message {
                    role: Role::User,
                    content: vec![ContentBlock::Text {
                        text: "second".into(),
                    }],
                },
            ],
            temperature: None,
            max_tokens: None,
            tools: vec![],
            tool_choice: Default::default(),
            response_format: Default::default(),
            stop_sequences: vec![],
            thinking_budget: None,
            extra: Default::default(),
        };
        assert_eq!(extract_user_prompt(&req.messages), "first\nsecond");
    }

    #[test]
    fn test_has_vision_content_false() {
        let req = text_request("No images");
        assert!(!has_vision_content(&req.messages));
    }

    #[test]
    fn test_has_vision_content_true() {
        let req = InferRequest {
            model: "test".into(),
            messages: vec![Message {
                role: Role::User,
                content: vec![
                    ContentBlock::Text {
                        text: "Describe".into(),
                    },
                    ContentBlock::Image {
                        source: "iVBORw0KGgoAAAA==".into(),
                        detail: Some("high".into()),
                    },
                ],
            }],
            temperature: None,
            max_tokens: None,
            tools: vec![],
            tool_choice: Default::default(),
            response_format: Default::default(),
            stop_sequences: vec![],
            thinking_budget: None,
            extra: Default::default(),
        };
        assert!(has_vision_content(&req.messages));
    }

    #[test]
    fn test_has_vision_content_system_image_ignored() {
        // Image in a System message should NOT trigger vision path
        let req = InferRequest {
            model: "test".into(),
            messages: vec![
                Message {
                    role: Role::System,
                    content: vec![ContentBlock::Image {
                        source: "abc".into(),
                        detail: None,
                    }],
                },
                Message {
                    role: Role::User,
                    content: vec![ContentBlock::Text {
                        text: "Hello".into(),
                    }],
                },
            ],
            temperature: None,
            max_tokens: None,
            tools: vec![],
            tool_choice: Default::default(),
            response_format: Default::default(),
            stop_sequences: vec![],
            thinking_budget: None,
            extra: Default::default(),
        };
        assert!(!has_vision_content(&req.messages));
    }

    // ─── Vision content conversion ───────────────────────────────────

    #[test]
    fn test_to_rig_user_content_text_only() {
        let messages = vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "Hello".into(),
            }],
        }];
        let parts = to_rig_user_content(&messages);
        assert_eq!(parts.len(), 1);
        assert!(matches!(&parts[0], rig::completion::message::UserContent::Text(_)));
    }

    #[test]
    fn test_to_rig_user_content_mixed() {
        let messages = vec![Message {
            role: Role::User,
            content: vec![
                ContentBlock::Image {
                    source: "/9j/4AAQ".into(),
                    detail: Some("high".into()),
                },
                ContentBlock::Text {
                    text: "Describe this".into(),
                },
            ],
        }];
        let parts = to_rig_user_content(&messages);
        assert_eq!(parts.len(), 2);
        assert!(matches!(&parts[0], rig::completion::message::UserContent::Image(_)));
        assert!(matches!(&parts[1], rig::completion::message::UserContent::Text(_)));
    }

    #[test]
    fn test_to_rig_user_content_skips_system() {
        let messages = vec![
            Message {
                role: Role::System,
                content: vec![ContentBlock::Text {
                    text: "system".into(),
                }],
            },
            Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "user".into(),
                }],
            },
        ];
        let parts = to_rig_user_content(&messages);
        assert_eq!(parts.len(), 1);
    }

    // ─── Error conversion ────────────────────────────────────────────

    #[test]
    fn test_convert_error_timeout() {
        use super::super::error::RigInferError;
        let err = convert_error(RigInferError::Timeout { duration_ms: 5000 });
        assert!(matches!(err, ProviderError::Api { .. }));
        match err {
            ProviderError::Api { message } => assert!(message.contains("5000")),
            _ => panic!("expected Api error"),
        }
    }

    #[test]
    fn test_convert_error_auth() {
        use super::super::error::RigInferError;
        let err = convert_error(RigInferError::PromptError("HTTP 401 Unauthorized".into()));
        assert!(matches!(err, ProviderError::AuthFailed { .. }));
    }

    #[test]
    fn test_convert_error_rate_limit() {
        use super::super::error::RigInferError;
        let err = convert_error(RigInferError::PromptError("429 rate limit exceeded".into()));
        assert!(matches!(err, ProviderError::RateLimited { .. }));
    }

    // ─── Provider trait: name + capabilities ─────────────────────────

    #[test]
    fn test_provider_name_mock() {
        let provider = RigProvider::Mock;
        assert_eq!(Provider::name(&provider), "mock");
    }

    #[test]
    fn test_provider_capabilities() {
        let provider = RigProvider::Mock;
        let caps = Provider::capabilities(&provider, "claude-sonnet-4-6");
        assert!(caps.is_some());
    }

    // ─── Stream adapter ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_stream_from_rx_text_events() {
        use futures::StreamExt;

        let (tx, rx) = mpsc::channel(16);
        let stream = stream_from_rx(rx);

        // Send a token, done, and metrics
        tx.send(StreamChunk::Token("Hello".into())).await.unwrap();
        tx.send(StreamChunk::Done("Hello".into())).await.unwrap();
        tx.send(StreamChunk::Metrics {
            input_tokens: 10,
            output_tokens: 5,
        })
        .await
        .unwrap();
        drop(tx);

        // Collect all events
        let events: Vec<_> = stream
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(events.len(), 3);
        assert!(matches!(&events[0], InferEvent::Delta(s) if s == "Hello"));
        assert!(matches!(
            &events[1],
            InferEvent::Done {
                stop_reason: StopReason::EndTurn,
                request_id: None,
                finish_reason_raw: None,
            }
        ));
        assert!(matches!(&events[2], InferEvent::Usage(u) if u.input_tokens == 10));
    }

    #[tokio::test]
    async fn test_stream_from_rx_skips_tui_events() {
        use futures::StreamExt;

        let (tx, rx) = mpsc::channel(16);
        let stream = stream_from_rx(rx);

        // Send a TUI event followed by a real token
        tx.send(StreamChunk::McpConnected("novanet".into()))
            .await
            .unwrap();
        tx.send(StreamChunk::Token("data".into())).await.unwrap();
        tx.send(StreamChunk::InferComplete).await.unwrap();
        tx.send(StreamChunk::Done("data".into())).await.unwrap();
        drop(tx);

        let events: Vec<_> = stream
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        // Only Token + Done — TUI events skipped
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], InferEvent::Delta(s) if s == "data"));
        assert!(matches!(&events[1], InferEvent::Done { .. }));
    }

    #[tokio::test]
    async fn test_stream_from_rx_error_event() {
        use futures::StreamExt;

        let (tx, rx) = mpsc::channel(16);
        let stream = stream_from_rx(rx);

        tx.send(StreamChunk::Error("boom".into())).await.unwrap();
        drop(tx);

        let events: Vec<_> = stream.collect::<Vec<_>>().await;
        assert_eq!(events.len(), 1);
        assert!(events[0].is_err());
    }

    #[tokio::test]
    async fn test_stream_from_rx_thinking_event() {
        use futures::StreamExt;

        let (tx, rx) = mpsc::channel(16);
        let stream = stream_from_rx(rx);

        tx.send(StreamChunk::Thinking("reasoning...".into()))
            .await
            .unwrap();
        tx.send(StreamChunk::Done("answer".into())).await.unwrap();
        drop(tx);

        let events: Vec<_> = stream
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], InferEvent::Thinking(s) if s == "reasoning..."));
    }

    // ─── S17-A0: stream_result_to_infer_response tests ──────────────

    /// Golden oracle: `stream_result_to_infer_response` preserves all
    /// metadata fields from `StreamResult` into `InferResponse`. This
    /// is the TDD test for S17-A0 — it validates that `Provider::infer()`
    /// (which now routes through streaming internally) returns real token
    /// counts instead of `TokenUsage::default()`.
    #[test]
    fn stream_result_to_response_preserves_all_metadata() {
        use crate::provider::rig::StreamResult;
        use nika_event::types::FinishReason;

        let sr = StreamResult {
            text: "Hello, world!".to_string(),
            input_tokens: 42,
            output_tokens: 17,
            total_tokens: 59,
            cached_input_tokens: 8,
            ttft_ms: Some(123),
            request_id: Some("req_golden_s17".to_string()),
            finish_reason: Some(FinishReason::EndTurn),
        };

        let response = stream_result_to_infer_response(sr, "claude-sonnet-4");

        // Text content preserved
        assert_eq!(response.content.len(), 1);
        assert!(matches!(
            &response.content[0],
            ContentBlock::Text { text } if text == "Hello, world!"
        ));

        // Token usage — real values, NOT defaults
        assert_eq!(response.usage.input_tokens, 42);
        assert_eq!(response.usage.output_tokens, 17);
        assert_eq!(response.usage.cache_read_tokens, Some(8));
        assert!(response.usage.cache_write_tokens.is_none());

        // Metadata fields
        assert_eq!(response.ttft_ms, Some(123));
        assert_eq!(response.request_id.as_deref(), Some("req_golden_s17"));
        assert_eq!(response.stop_reason, StopReason::EndTurn);

        // P0-2 fix: cost_usd now computed from static pricing catalog.
        // For a known model like "claude-sonnet-4", the catalog returns
        // real pricing. The cost should be non-zero for 42 input + 17 output.
        assert!(
            response.cost_usd.is_some(),
            "cost_usd must be computed from pricing catalog for known models"
        );
        let cost = response.cost_usd.unwrap();
        assert!(cost > 0.0, "cost for 42+17 tokens must be > 0.0, got {cost}");
        assert!(cost < 1.0, "cost for 42+17 tokens should be < $1, got {cost}");
    }

    /// Zero cached_input_tokens → `cache_read_tokens: None` (not `Some(0)`).
    #[test]
    fn stream_result_to_response_zero_cached_is_none() {
        use crate::provider::rig::StreamResult;

        let sr = StreamResult {
            text: "ok".to_string(),
            input_tokens: 10,
            output_tokens: 5,
            cached_input_tokens: 0,
            ..Default::default()
        };

        let response = stream_result_to_infer_response(sr, "unknown-model");
        assert!(
            response.usage.cache_read_tokens.is_none(),
            "zero cached_input_tokens must map to None, not Some(0)"
        );
        // Unknown model → no pricing → cost_usd is None
        assert!(response.cost_usd.is_none());
    }

    /// Missing finish_reason → `StopReason::EndTurn` default.
    #[test]
    fn stream_result_to_response_none_finish_reason_defaults_to_end_turn() {
        use crate::provider::rig::StreamResult;

        let sr = StreamResult {
            text: "ok".to_string(),
            finish_reason: None,
            ..Default::default()
        };

        let response = stream_result_to_infer_response(sr, "mock-model");
        assert_eq!(response.stop_reason, StopReason::EndTurn);
    }

    // ─── S17-A0: finish_reason_to_stop_reason tests ─────────────────

    #[test]
    fn finish_to_stop_typed_variants_round_trip() {
        use nika_event::types::FinishReason;

        let cases = [
            (FinishReason::EndTurn, StopReason::EndTurn),
            (FinishReason::Stop, StopReason::EndTurn),
            (FinishReason::MaxTokens, StopReason::MaxTokens),
            (FinishReason::StopSequence, StopReason::StopSequence),
            (FinishReason::ToolUse, StopReason::ToolUse),
            (FinishReason::Mock, StopReason::EndTurn),
        ];
        for (fr, expected) in cases {
            let label = format!("{fr:?}");
            let got = finish_reason_to_stop_reason(&fr);
            assert_eq!(got, expected, "finish_reason_to_stop_reason({label})");
        }
    }

    #[test]
    fn finish_to_stop_content_filter_variants() {
        use nika_event::types::FinishReason;

        for raw in ["content_filter", "safety", "policy_violation"] {
            let fr = FinishReason::Other(raw.to_string());
            let got = finish_reason_to_stop_reason(&fr);
            assert_eq!(
                got,
                StopReason::ContentFilter,
                "Other(\"{raw}\") should map to ContentFilter"
            );
        }
    }

    #[test]
    fn finish_to_stop_unknown_other() {
        use nika_event::types::FinishReason;

        let fr = FinishReason::Other("server_overloaded".to_string());
        let got = finish_reason_to_stop_reason(&fr);
        assert_eq!(got, StopReason::Unknown("server_overloaded".to_string()));
    }
}
