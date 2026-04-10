// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Provider trait + DTOs — unified LLM inference abstraction.
//!
//! This replaces the `RigProvider` 9-variant enum + `dispatch_rig!` macro.
//! The existing RigProvider gets wrapped in `impl Provider for RigProvider`.
//!
//! Key design: `InferRequest::tools` unifies plain inference and tool use.
//! Empty tools list = plain inference. One code path, not three.

use std::pin::Pin;

use futures_core::Stream;
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────
// Request / Response DTOs
// ─────────────────────────────────────────────────────────────────────

/// A message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

/// Message role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// A block of content within a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Image {
        source: String,
        detail: Option<String>,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
    Thinking {
        text: String,
    },
}

/// A tool definition for function calling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Tool choice strategy.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolChoice {
    #[default]
    Auto,
    Required,
    None,
    Specific(String),
}

/// Response format hint.
#[derive(Debug, Clone, Default)]
pub enum ResponseFormat {
    #[default]
    Text,
    Json,
    JsonSchema(serde_json::Value),
}

/// Provider-specific pass-through parameters.
#[derive(Debug, Clone, Default)]
pub struct ProviderExtras {
    pub params: serde_json::Map<String, serde_json::Value>,
}

/// Unified inference request.
///
/// `tools` empty = plain inference. Non-empty = tool use.
/// One struct, one code path, one trait method.
#[derive(Debug, Clone)]
pub struct InferRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub tools: Vec<ToolDef>,
    pub tool_choice: ToolChoice,
    pub response_format: ResponseFormat,
    pub stop_sequences: Vec<String>,
    pub thinking_budget: Option<u32>,
    pub extra: ProviderExtras,
}

/// Inference response.
///
/// Constructed via [`InferResponse::new`]; `#[non_exhaustive]` prevents
/// struct-literal construction so future fields can be added without
/// breaking downstream code.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct InferResponse {
    pub content: Vec<ContentBlock>,
    pub usage: TokenUsage,
    pub stop_reason: StopReason,
    pub ttft_ms: Option<u64>,
    pub cached_tokens: Option<u32>,
    /// Provider-assigned request identifier (e.g., OpenAI `response.id`).
    ///
    /// Surfaced in the `ProviderResponded` event for correlation with
    /// provider-side logs and billing dashboards.
    pub request_id: Option<String>,
    /// Computed cost in USD for the request.
    ///
    /// Populated by the bridge from `ModelCapabilities` pricing; `None`
    /// when pricing data is unavailable for the model.
    pub cost_usd: Option<f64>,
}

impl InferResponse {
    /// Minimal constructor — all optional fields default to `None`.
    pub fn new(content: Vec<ContentBlock>, usage: TokenUsage, stop_reason: StopReason) -> Self {
        Self {
            content,
            usage,
            stop_reason,
            ttft_ms: None,
            cached_tokens: None,
            request_id: None,
            cost_usd: None,
        }
    }
}

/// Token usage statistics.
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
}

/// Why the model stopped generating.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    StopSequence,
    ToolUse,
    ContentFilter,
    Unknown(String),
}

/// Streaming event from an inference call.
#[derive(Debug, Clone)]
pub enum InferEvent {
    Delta(String),
    ToolUseStart { id: String, name: String },
    ToolUseDelta { id: String, partial_json: String },
    Thinking(String),
    Usage(TokenUsage),
    Done(StopReason),
}

/// Re-export the canonical model capabilities from nika-core.
///
/// Do NOT define a separate Capabilities struct here — nika-core's
/// `ModelCapabilities` is the source of truth for the static catalog.
pub use nika_core::catalogs::ModelCapabilities;

// ─────────────────────────────────────────────────────────────────────
// Provider trait
// ─────────────────────────────────────────────────────────────────────

/// Stream of inference events for streaming responses.
pub type InferStream = Pin<Box<dyn Stream<Item = Result<InferEvent, ProviderError>> + Send>>;

/// Errors from LLM providers.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("Provider API error: {message}")]
    Api { message: String },

    #[error("Model not found: {model}")]
    ModelNotFound { model: String },

    #[error("Rate limited: retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },

    #[error("Authentication failed for provider '{provider}'")]
    AuthFailed { provider: String },

    #[error("Provider error: {reason}")]
    Other { reason: String },
}

/// Unified LLM provider trait.
///
/// Replaces `RigProvider` enum dispatch. Each provider implements this directly.
/// The existing `RigProvider` gets a blanket `impl Provider for RigProvider`
/// that delegates via `dispatch_rig!`.
#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    /// Provider name (e.g. "anthropic", "openai").
    fn name(&self) -> &str;

    /// Model capabilities for the given model ID.
    fn capabilities(&self, model: &str) -> Option<ModelCapabilities>;

    /// Run inference (non-streaming).
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError>;

    /// Run inference with streaming events.
    async fn infer_stream(&self, request: InferRequest) -> Result<InferStream, ProviderError>;

    /// True if the provider supports native structured output via
    /// `response_format: json_schema`.
    ///
    /// Used by `nika-verb-infer`'s Layer 0a structured-output strategy
    /// to decide whether to route via native response_format or fall back
    /// to prompt-based schema injection. Defaults to `false` — each
    /// provider opts in.
    fn supports_response_format(&self) -> bool {
        false
    }
}
