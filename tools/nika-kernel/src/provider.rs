// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Provider traits + DTOs — unified LLM inference abstraction.
//!
//! ISP decomposition:
//! - `ProviderInfer` — single-shot inference
//! - `ProviderStream` — streaming inference
//! - `ProviderMeta` — capabilities and metadata (sync)
//! - `ProviderEmbed` — embedding generation (opt-in)
//! - `ProviderVision` — vision capability check (opt-in, sync)
//!
//! Super-trait: `Provider = ProviderInfer + ProviderStream + ProviderMeta`.
//! `ProviderEmbed` and `ProviderVision` are NOT part of `Provider`.

use std::pin::Pin;

use futures_core::Stream;
use serde::{Deserialize, Serialize};

use crate::memory::{MemoryDirective, MemoryFrameRef};

// ─── Message types ───────────────────────────────────────────────────

/// A message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Message {
    /// Message role.
    pub role: Role,
    /// Content blocks.
    pub content: Vec<ContentBlock>,
}

impl Message {
    /// Create a message with a single text block.
    #[must_use]
    pub fn text(role: Role, text: impl Into<String>) -> Self {
        Self {
            role,
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }

    /// Create a new message with role and content blocks.
    #[must_use]
    pub fn new(role: Role, content: Vec<ContentBlock>) -> Self {
        Self { role, content }
    }
}

/// Message role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Role {
    /// System instructions.
    System,
    /// User input.
    User,
    /// Assistant response.
    Assistant,
    /// Tool result.
    Tool,
}

/// A block of content within a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ContentBlock {
    /// Plain text content.
    Text {
        /// The text.
        text: String,
    },
    /// Image content.
    Image {
        /// Image source (CAS hash or URL).
        source: String,
        /// Detail level (`"high"`, `"low"`, `"auto"`).
        detail: Option<String>,
    },
    /// Tool use request from the model.
    ToolUse {
        /// Tool call identifier.
        id: String,
        /// Tool name.
        name: String,
        /// Tool input parameters.
        input: serde_json::Value,
    },
    /// Tool execution result.
    ToolResult {
        /// The tool call ID this result corresponds to.
        tool_use_id: String,
        /// Tool output content.
        content: String,
        /// Whether the tool execution resulted in an error.
        is_error: bool,
    },
    /// Extended thinking block.
    Thinking {
        /// Thinking content.
        text: String,
    },
}

// ─── Tool definition ─────────────────────────────────────────────────

/// A tool definition for function calling.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolDef {
    /// Tool name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// JSON Schema for the tool parameters.
    pub parameters: serde_json::Value,
}

impl ToolDef {
    /// Create a new tool definition.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
        }
    }
}

// ─── Request configuration ───────────────────────────────────────────

/// Tool choice strategy.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ToolChoice {
    /// Let the model decide.
    #[default]
    Auto,
    /// Force tool use.
    Required,
    /// Disable tool use.
    None,
    /// Use a specific tool.
    Specific(String),
}

/// Response format.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ResponseFormat {
    /// Free-form text (default).
    #[default]
    Text,
    /// JSON output.
    Json,
    /// JSON output conforming to a schema.
    JsonSchema(serde_json::Value),
}

// Need custom PartialEq for ResponseFormat because serde_json::Value
// already implements PartialEq, but we derived it above.

/// Provider-specific extra parameters.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ProviderExtras {
    /// Additional key-value parameters passed to the provider.
    pub params: serde_json::Map<String, serde_json::Value>,
}

impl ProviderExtras {
    /// Create empty extras.
    #[must_use]
    pub fn new() -> Self {
        Self {
            params: serde_json::Map::new(),
        }
    }
}

// ─── InferRequest ────────────────────────────────────────────────────

/// An LLM inference request.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct InferRequest {
    /// Model identifier (e.g., `"claude-sonnet-4-20250514"`).
    pub model: String,
    /// Conversation messages.
    pub messages: Vec<Message>,
    /// Sampling temperature (0.0–2.0).
    pub temperature: Option<f32>,
    /// Maximum output tokens.
    pub max_tokens: Option<u32>,
    /// Available tools for function calling.
    pub tools: Vec<ToolDef>,
    /// Tool choice strategy.
    pub tool_choice: ToolChoice,
    /// Expected response format.
    pub response_format: ResponseFormat,
    /// Stop sequences.
    pub stop_sequences: Vec<String>,
    /// Extended thinking token budget.
    pub thinking_budget: Option<u32>,
    /// Provider-specific extra parameters.
    pub extra: ProviderExtras,
    /// Memory directive (Cortex hook, Phase 1).
    pub memory: Option<MemoryDirective>,
}

impl InferRequest {
    /// Create a new inference request with model and messages.
    #[must_use]
    pub fn new(model: impl Into<String>, messages: Vec<Message>) -> Self {
        Self {
            model: model.into(),
            messages,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            tool_choice: ToolChoice::default(),
            response_format: ResponseFormat::default(),
            stop_sequences: Vec::new(),
            thinking_budget: None,
            extra: ProviderExtras::new(),
            memory: None,
        }
    }
}

// ─── InferResponse ───────────────────────────────────────────────────

/// Token usage statistics.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TokenUsage {
    /// Input/prompt tokens.
    pub input_tokens: u64,
    /// Output/completion tokens.
    pub output_tokens: u64,
    /// Tokens read from cache (prompt caching).
    pub cache_read_tokens: Option<u64>,
    /// Tokens written to cache.
    pub cache_write_tokens: Option<u64>,
}

impl TokenUsage {
    /// Create a new token usage with input and output counts.
    #[must_use]
    pub fn new(input_tokens: u64, output_tokens: u64) -> Self {
        Self {
            input_tokens,
            output_tokens,
            cache_read_tokens: None,
            cache_write_tokens: None,
        }
    }
}

/// Reason the model stopped generating.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum StopReason {
    /// Natural end of response.
    EndTurn,
    /// Hit the `max_tokens` limit.
    MaxTokens,
    /// Hit a stop sequence.
    StopSequence,
    /// Model wants to use a tool.
    ToolUse,
    /// Content was filtered.
    ContentFilter,
    /// Unknown/provider-specific reason.
    Unknown(String),
}

/// An LLM inference response.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct InferResponse {
    /// Response content blocks.
    pub content: Vec<ContentBlock>,
    /// Token usage.
    pub usage: TokenUsage,
    /// Why the model stopped.
    pub stop_reason: StopReason,
    /// Time to first token in milliseconds.
    pub ttft_ms: Option<u64>,
    /// Number of cached tokens used.
    pub cached_tokens: Option<u32>,
    /// Provider-assigned request identifier.
    pub request_id: Option<String>,
    /// Estimated cost in USD.
    pub cost_usd: Option<f64>,
    /// Raw finish reason from the provider.
    pub finish_reason_raw: Option<String>,
    /// Memory frames created during inference (Cortex hook, Phase 1).
    pub memory_frames: Vec<MemoryFrameRef>,
}

impl InferResponse {
    /// Create a new inference response.
    #[must_use]
    pub fn new(
        content: Vec<ContentBlock>,
        usage: TokenUsage,
        stop_reason: StopReason,
    ) -> Self {
        Self {
            content,
            usage,
            stop_reason,
            ttft_ms: None,
            cached_tokens: None,
            request_id: None,
            cost_usd: None,
            finish_reason_raw: None,
            memory_frames: Vec::new(),
        }
    }
}

// ─── Streaming ───────────────────────────────────────────────────────

/// A streaming inference event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum InferEvent {
    /// Text delta.
    Delta {
        /// The text chunk.
        text: String,
    },
    /// Tool use start.
    ToolUseStart {
        /// Tool call ID.
        id: String,
        /// Tool name.
        name: String,
    },
    /// Tool use input delta.
    ToolUseDelta {
        /// Tool call ID.
        id: String,
        /// Partial JSON input.
        partial_json: String,
    },
    /// Extended thinking.
    Thinking {
        /// Thinking text.
        text: String,
    },
    /// Token usage update.
    Usage(TokenUsage),
    /// Stream completed.
    Done {
        /// Stop reason.
        stop_reason: StopReason,
        /// Provider request ID.
        request_id: Option<String>,
        /// Raw finish reason.
        finish_reason_raw: Option<String>,
    },
}

/// Type alias for the streaming inference event stream.
///
/// Boxed because `dyn Stream` is the only way to be object-safe
/// with async iterators. Single allocation per LLM call.
pub type InferEventStream =
    Pin<Box<dyn Stream<Item = Result<InferEvent, ProviderError>> + Send>>;

// ─── Provider errors ─────────────────────────────────────────────────

/// Provider errors.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum ProviderError {
    /// API error with status code.
    #[error("provider API error ({status}): {message}")]
    Api {
        /// HTTP status code.
        status: u16,
        /// Error message from the provider.
        message: String,
    },

    /// Requested model not found.
    #[error("model not found: {model}")]
    ModelNotFound {
        /// Model identifier.
        model: String,
    },

    /// Rate limited.
    #[error("rate limited")]
    RateLimited {
        /// Suggested retry delay in milliseconds.
        retry_after_ms: Option<u64>,
    },

    /// Authentication failed.
    #[error("authentication failed: {reason}")]
    AuthFailed {
        /// Why auth failed.
        reason: String,
    },

    /// Other provider error.
    #[error("provider error: {reason}")]
    Other {
        /// Error description.
        reason: String,
    },
}

impl ProviderError {
    /// Whether this error is transient and may succeed on retry.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::RateLimited { .. } | Self::Api { status: 500..=599, .. }
        )
    }
}

// ─── Traits ──────────────────────────────────────────────────────────

/// Single-shot LLM inference.
#[trait_variant::make(ProviderInferDyn: Send)]
pub trait ProviderInfer: Send + Sync {
    /// Run inference and return the complete response.
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError>;
}

/// Streaming LLM inference.
#[trait_variant::make(ProviderStreamDyn: Send)]
pub trait ProviderStream: Send + Sync {
    /// Run inference and return a stream of events.
    async fn infer_stream(
        &self,
        request: InferRequest,
    ) -> Result<InferEventStream, ProviderError>;
}

/// Provider metadata (sync, no `trait_variant` needed).
pub trait ProviderMeta: Send + Sync {
    /// Provider name (e.g., `"anthropic"`, `"openai"`).
    fn name(&self) -> &str;

    /// Whether the provider supports `response_format: json_schema`.
    fn supports_response_format(&self) -> bool {
        false
    }
}

/// Full provider — Infer + Stream + Meta.
pub trait Provider: ProviderInfer + ProviderStream + ProviderMeta {}
impl<T: ProviderInfer + ProviderStream + ProviderMeta> Provider for T {}

/// Embedding generation (opt-in, not part of Provider).
#[trait_variant::make(ProviderEmbedDyn: Send)]
pub trait ProviderEmbed: Send + Sync {
    /// Generate embeddings for the given texts.
    async fn embed(&self, input: &[String]) -> Result<Vec<Vec<f32>>, ProviderError>;
}

/// Vision capability check (opt-in, not part of Provider).
pub trait ProviderVision: Send + Sync {
    /// Whether this provider supports vision/image inputs.
    fn supports_vision(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_text_constructor() {
        let msg = Message::text(Role::User, "hello");
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.content.len(), 1);
    }

    #[test]
    fn role_serde_roundtrip() {
        let role = Role::Assistant;
        let json = serde_json::to_string(&role).expect("serialize");
        assert_eq!(json, "\"assistant\"");
        let back: Role = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, role);
    }

    #[test]
    fn content_block_text_serde() {
        let block = ContentBlock::Text {
            text: "hello".into(),
        };
        let json = serde_json::to_string(&block).expect("serialize");
        assert!(json.contains("\"type\":\"text\""));
        let back: ContentBlock = serde_json::from_str(&json).expect("deserialize");
        assert!(matches!(back, ContentBlock::Text { text } if text == "hello"));
    }

    #[test]
    fn tool_def_new() {
        let def = ToolDef::new("nika:read", "Read a file", serde_json::json!({}));
        assert_eq!(def.name, "nika:read");
    }

    #[test]
    fn tool_choice_default_is_auto() {
        assert_eq!(ToolChoice::default(), ToolChoice::Auto);
    }

    #[test]
    fn response_format_default_is_text() {
        assert_eq!(ResponseFormat::default(), ResponseFormat::Text);
    }

    #[test]
    fn infer_request_new_defaults() {
        let req = InferRequest::new("claude-sonnet", vec![]);
        assert_eq!(req.model, "claude-sonnet");
        assert!(req.temperature.is_none());
        assert!(req.max_tokens.is_none());
        assert!(req.tools.is_empty());
        assert_eq!(req.tool_choice, ToolChoice::Auto);
        assert_eq!(req.response_format, ResponseFormat::Text);
        assert!(req.stop_sequences.is_empty());
        assert!(req.thinking_budget.is_none());
        assert!(req.memory.is_none());
    }

    #[test]
    fn token_usage_new() {
        let usage = TokenUsage::new(100, 50);
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert!(usage.cache_read_tokens.is_none());
    }

    #[test]
    fn infer_response_new_defaults() {
        let resp = InferResponse::new(vec![], TokenUsage::new(10, 20), StopReason::EndTurn);
        assert!(resp.ttft_ms.is_none());
        assert!(resp.memory_frames.is_empty());
    }

    #[test]
    fn provider_error_is_transient() {
        assert!(ProviderError::RateLimited {
            retry_after_ms: Some(1000),
        }
        .is_transient());
        assert!(ProviderError::Api {
            status: 503,
            message: "overloaded".into(),
        }
        .is_transient());
        assert!(!ProviderError::AuthFailed {
            reason: "bad key".into(),
        }
        .is_transient());
        assert!(!ProviderError::ModelNotFound {
            model: "x".into(),
        }
        .is_transient());
    }

    #[test]
    fn provider_error_display() {
        let err = ProviderError::ModelNotFound {
            model: "gpt-5".into(),
        };
        assert_eq!(err.to_string(), "model not found: gpt-5");
    }

    #[test]
    fn infer_event_serde() {
        let event = InferEvent::Delta {
            text: "hello".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("\"type\":\"delta\""));
    }

    #[test]
    fn stop_reason_serde() {
        let reason = StopReason::ToolUse;
        let json = serde_json::to_string(&reason).expect("serialize");
        assert_eq!(json, "\"tool_use\"");
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn provider_types_send_sync() {
        _assert_send_sync::<Message>();
        _assert_send_sync::<InferRequest>();
        _assert_send_sync::<InferResponse>();
        _assert_send_sync::<TokenUsage>();
        _assert_send_sync::<ToolDef>();
        _assert_send_sync::<ProviderExtras>();
    }

    /// Verify `ProviderMeta` default: `supports_response_format` returns false.
    struct DefaultMetaProvider;
    impl ProviderMeta for DefaultMetaProvider {
        fn name(&self) -> &'static str {
            "test"
        }
    }

    #[test]
    fn provider_meta_default_supports_response_format_is_false() {
        let p = DefaultMetaProvider;
        assert!(!p.supports_response_format());
    }

    /// Verify blanket super-trait: implementing all 3 atomics gives Provider for free.
    struct DummyProvider;

    impl ProviderInfer for DummyProvider {
        async fn infer(&self, _: InferRequest) -> Result<InferResponse, ProviderError> {
            Ok(InferResponse::new(vec![], TokenUsage::new(0, 0), StopReason::EndTurn))
        }
    }

    impl ProviderStream for DummyProvider {
        async fn infer_stream(&self, _: InferRequest) -> Result<InferEventStream, ProviderError> {
            // Hand-rolled empty stream (no futures-util dep needed)
            struct EmptyStream;
            impl futures_core::Stream for EmptyStream {
                type Item = Result<InferEvent, ProviderError>;
                fn poll_next(
                    self: std::pin::Pin<&mut Self>,
                    _cx: &mut std::task::Context<'_>,
                ) -> std::task::Poll<Option<Self::Item>> {
                    std::task::Poll::Ready(None)
                }
            }
            Ok(Box::pin(EmptyStream))
        }
    }

    impl ProviderMeta for DummyProvider {
        fn name(&self) -> &str {
            "dummy"
        }
    }

    #[test]
    fn blanket_provider_impl() {
        fn _accepts_provider<T: Provider>(_: &T) {}
        let p = DummyProvider;
        _accepts_provider(&p);
    }
}
