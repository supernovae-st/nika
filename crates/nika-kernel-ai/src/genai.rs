// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `GenAiAttrs` — typed bridge to `OpenTelemetry` `GenAI` semantic conventions.
//!
//! Maps the stable `gen_ai.*` attributes to a typed Rust struct embedded on
//! `InferRequest` and `InferResponse`. Enforces cross-provider parity
//! (Pre-launch Gate 2) — no provider can silently drop an attribute the
//! kernel exports.
//!
//! Spec: <https://opentelemetry.io/docs/specs/semconv/gen-ai/> (Development
//! status as of 2026-04 — fields stay `non_exhaustive` until Stable).
//!
//! Forward-compat: every public type is `non_exhaustive` + `Default` +
//! `new()` per INV-019 + ADR-007.

use serde::{Deserialize, Serialize};

/// `gen_ai.system` — the `GenAI` provider identifier.
///
/// Wire format uses the `OTel` semconv `snake_case` string convention.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum GenAiSystem {
    /// Unknown / not yet populated by the provider.
    #[default]
    Unknown,
    /// Anthropic (Claude family).
    Anthropic,
    /// `OpenAI`.
    OpenAi,
    /// Google (Gemini).
    Google,
    /// Mistral.
    Mistral,
    /// Meta (Llama).
    Meta,
    /// Cohere.
    Cohere,
    /// `DeepSeek`.
    DeepSeek,
    /// xAI (Grok).
    Xai,
    /// `OpenAI`-compatible third party (`LiteLLM`, `OpenRouter`, Ollama, etc.).
    OpenAiCompatible,
    /// Local model (mistral.rs GGUF, llama.cpp).
    LocalNative,
    /// Custom / user-extended. Prefer adding a first-class variant.
    Custom,
}

/// `gen_ai.operation.name` — the operation kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum GenAiOperation {
    /// Chat completion (default for `InferRequest`).
    #[default]
    Chat,
    /// Text completion (legacy).
    TextCompletion,
    /// Embedding generation.
    Embedding,
    /// Image generation.
    ImageGeneration,
    /// Audio transcription.
    AudioTranscription,
    /// Audio synthesis (TTS).
    AudioSynthesis,
}

/// Typed bridge to `OTel` `GenAI` semconv attributes.
///
/// Embedded on `InferRequest` and `InferResponse`. Default = unknown +
/// chat operation. Populated by the provider trait impl.
///
/// All fields are `Option<T>` or have `Default`; struct grows
/// non-breakingly thanks to `#[non_exhaustive]` + `pub fn new()` (INV-019).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GenAiAttrs {
    /// `gen_ai.system` — which provider.
    pub system: GenAiSystem,
    /// `gen_ai.operation.name` — operation kind.
    pub operation: GenAiOperation,
    /// `gen_ai.response.id` — provider's response identifier.
    pub response_id: Option<String>,
    /// `gen_ai.response.model` — actual model that responded (may differ
    /// from `InferRequest::model` if the provider routed).
    pub response_model: Option<String>,
    /// `gen_ai.request.encoding_formats` — vector encoding formats
    /// (e.g. `["float", "base64"]` for embeddings).
    pub encoding_formats: Vec<String>,
    /// `gen_ai.conversation.id` — multi-turn conversation correlation.
    pub conversation_id: Option<String>,
    /// `gen_ai.agent.id` — agent identifier when the request originates
    /// from a verb-agent step.
    pub agent_id: Option<String>,
    /// `gen_ai.agent.name` — human-readable agent name.
    pub agent_name: Option<String>,
}

impl GenAiAttrs {
    /// Construct an empty/default attrs object.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn types_are_send_sync() {
        _assert_send_sync::<GenAiAttrs>();
        _assert_send_sync::<GenAiSystem>();
        _assert_send_sync::<GenAiOperation>();
    }

    #[test]
    fn default_attrs_is_unknown_chat() {
        let a = GenAiAttrs::new();
        assert_eq!(a.system, GenAiSystem::Unknown);
        assert_eq!(a.operation, GenAiOperation::Chat);
        assert!(a.response_id.is_none());
        assert!(a.response_model.is_none());
    }

    #[test]
    fn system_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&GenAiSystem::Anthropic).expect("serialize"),
            "\"anthropic\""
        );
        assert_eq!(
            serde_json::to_string(&GenAiSystem::OpenAi).expect("serialize"),
            "\"open_ai\""
        );
        assert_eq!(
            serde_json::to_string(&GenAiSystem::DeepSeek).expect("serialize"),
            "\"deep_seek\""
        );
    }

    #[test]
    fn operation_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&GenAiOperation::TextCompletion).expect("serialize"),
            "\"text_completion\""
        );
        assert_eq!(
            serde_json::to_string(&GenAiOperation::ImageGeneration).expect("serialize"),
            "\"image_generation\""
        );
    }
}
