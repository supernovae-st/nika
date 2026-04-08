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

/// Build an InferResponse from a plain text result.
fn text_response(text: String, start: Instant) -> InferResponse {
    InferResponse {
        content: vec![ContentBlock::Text { text }],
        usage: TokenUsage::default(),
        stop_reason: StopReason::EndTurn,
        ttft_ms: Some(start.elapsed().as_millis() as u64),
        cached_tokens: None,
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
                            Some(Ok(InferEvent::Done(StopReason::EndTurn)))
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
        let start = Instant::now();
        let system = extract_system(&request.messages);
        let has_vision = has_vision_content(&request.messages);

        if has_vision {
            // ── Vision path ──────────────────────────────────────────
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
            Ok(text_response(text, start))
        } else {
            // ── Text path (with full options) ────────────────────────
            let prompt = extract_user_prompt(&request.messages);
            let opts = InferOptions {
                model: Some(request.model.clone()),
                temperature: request.temperature.map(f64::from),
                max_tokens: request.max_tokens,
                system,
                additional_params: None,
            };
            let text = self
                .infer_with_options(&prompt, &opts)
                .await
                .map_err(convert_error)?;
            Ok(text_response(text, start))
        }
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
        assert!(matches!(&events[1], InferEvent::Done(StopReason::EndTurn)));
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
        assert!(matches!(&events[1], InferEvent::Done(_)));
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
}
