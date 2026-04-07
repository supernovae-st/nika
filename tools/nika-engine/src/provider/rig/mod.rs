//! Rig-core provider wrapper
//!
//! Wraps rig-core providers (Anthropic, OpenAI) with a unified interface
//! that integrates with Nika's workflow system.
//!
//! ## Architecture
//!
//! This module provides two main components:
//!
//! 1. **RigProvider** - Enum wrapping Claude/OpenAI provider clients
//! 2. **NikaMcpTool** - Wrapper implementing rig-core's `ToolDyn` for MCP tools
//!
//! ## MCP Integration
//!
//! We use rig-core's `ToolDyn` trait to wrap our MCP tools, avoiding the rmcp
//! version conflict (rig-core uses rmcp 0.13, we use rmcp 0.16).
//!
//! ```text
//! NikaMcpToolDef (our definition)
//!        ↓
//! NikaMcpTool (implements ToolDyn)
//!        ↓
//! rig-core AgentBuilder.tool()
//! ```

// Imports used by this file AND submodules (via `use super::*`).
// Items only consumed in submodules still need to be imported here
// because the submodules use `use super::*;`.
#[allow(unused_imports)]
use crate::error_domains::ProviderError;
#[allow(unused_imports)]
use std::time::Instant;

#[cfg(feature = "native-inference")]
#[allow(unused_imports)]
use crate::provider::native::InferenceBackend;
#[allow(unused_imports)]
use rig::client::{CompletionClient, ProviderClient};
#[allow(unused_imports)]
use rig::completion::{CompletionModel as _, Prompt, PromptError};
use rig::providers::{anthropic, deepseek, gemini, groq, mistral, openai, xai};
#[allow(unused_imports)]
use rig::tool::ToolDyn;
#[allow(unused_imports)]
use tokio::sync::mpsc;
#[allow(unused_imports)]
use tokio::time::timeout;

pub mod error;
pub mod stream;
pub mod tool;
pub use error::{
    McpToolError, McpToolErrorKind, ProviderVerifyError, ProviderVerifyResult, RigInferError,
};
#[allow(unused_imports)]
use stream::consume_rig_stream;
pub use stream::{StreamChunk, StreamResult};
pub use tool::{AgentMediaStaging, NikaMcpTool, NikaMcpToolDef};

/// Options for LLM inference
///
/// Provides fine-grained control over inference behavior.
#[derive(Debug, Clone, Default)]
pub struct InferOptions {
    /// Model identifier (uses provider default if None)
    pub model: Option<String>,
    /// Temperature for sampling (0.0-2.0, lower = more deterministic)
    pub temperature: Option<f64>,
    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,
    /// System prompt to prepend
    pub system: Option<String>,
    /// Additional parameters to pass to the provider API (e.g. response_format for OpenAI)
    pub additional_params: Option<serde_json::Value>,
}

/// Check if a provider name supports native structured output (string-based check).
/// Prefer `RigProvider::supports_native_structured_output()` when you have the resolved provider.
pub fn supports_native_structured_output(provider_name: &str) -> bool {
    matches!(provider_name, "openai" | "groq" | "deepseek" | "xai")
}

/// Build the `response_format: json_schema` payload for OpenAI-compatible providers.
///
/// Returns a `serde_json::Value` suitable for `InferOptions::additional_params`.
pub fn build_response_format_params(schema: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": "structured_output",
                "strict": true,
                "schema": schema
            }
        }
    })
}

/// Check if a model ID is a reasoning model that does not support `temperature`.
///
/// **Deprecated:** Prefer [`nika_core::catalogs::model_capabilities`] directly
/// for richer, provider-aware capability checks. This function assumes OpenAI
/// provider context and only checks temperature support.
///
/// Kept for backward compatibility with callers that don't have provider context.
pub fn is_reasoning_model(model_id: &str) -> bool {
    // Try to infer provider from model name for accurate capability lookup.
    let provider = if model_id.to_lowercase().starts_with("deepseek") {
        "deepseek"
    } else {
        "openai"
    };
    !nika_core::catalogs::model_capabilities(provider, model_id).supports_temperature
}

/// Resolve the correct token-limit parameter for a provider + model combination.
///
/// Returns `(rig_max_tokens, additional_params)`:
/// - Standard models: `(Some(N), None)` → call `.max_tokens(N)` on the rig builder
/// - OpenAI reasoning models: `(None, Some(json))` → skip `.max_tokens()`, inject
///   `max_completion_tokens` via `.additional_params()`
///
/// rig-core's `max_tokens` field is `Option<u64>` with `skip_serializing_if`,
/// so not calling `.max_tokens()` omits it entirely from the JSON body.
pub(crate) fn token_limit_for_model(
    provider: &str,
    model: &str,
    max_tok: u64,
) -> (Option<u64>, Option<serde_json::Value>) {
    use nika_core::catalogs::capabilities::{model_capabilities, TokenLimitParam};
    match model_capabilities(provider, model).token_limit_param {
        TokenLimitParam::MaxCompletionTokens => (
            None,
            Some(serde_json::json!({"max_completion_tokens": max_tok})),
        ),
        TokenLimitParam::MaxTokens => (Some(max_tok), None),
    }
}

/// Compute effective temperature, stripping it for models that reject it.
///
/// Assumes "openai" provider context. For provider-aware checks, use
/// `model_capabilities(provider, model).supports_temperature` directly.
fn effective_temperature_for_model(model_id: &str, requested: Option<f64>) -> Option<f64> {
    if requested.is_some() && is_reasoning_model(model_id) {
        tracing::warn!(
            model = %model_id,
            "temperature stripped for model that does not support it"
        );
        None
    } else {
        requested
    }
}

/// Provider type enum for rig-core providers
///
/// Nika leverages rig-core's native multi-provider support.
/// Each variant wraps the corresponding rig-core client.
#[derive(Clone)]
pub enum RigProvider {
    /// Claude (Anthropic) provider - ANTHROPIC_API_KEY
    Claude(anthropic::Client),
    /// OpenAI provider - OPENAI_API_KEY
    OpenAI(openai::Client),
    /// Mistral provider - MISTRAL_API_KEY
    Mistral(mistral::Client),
    /// Groq provider - GROQ_API_KEY
    Groq(groq::Client),
    /// DeepSeek provider - DEEPSEEK_API_KEY
    DeepSeek(deepseek::Client),
    /// Gemini (Google) provider - GEMINI_API_KEY
    Gemini(gemini::Client),
    /// xAI (Grok) provider - XAI_API_KEY
    XAi(xai::Client),
    /// OpenAI-compatible endpoint (vLLM, TGI, Ollama, LiteLLM, SGLang).
    /// Uses openai::Client pointed at a custom base URL.
    OpenAiCompat {
        client: openai::Client,
        /// Display name for events/errors (e.g., "h100", "ollama")
        endpoint_name: String,
        /// Default model for this endpoint
        default_model: Option<String>,
        /// Pre-computed name for `name()` — avoids Box::leak on every call
        cached_name: String,
        /// Request timeout in seconds (from config.toml endpoint or default 300)
        timeout_secs: u64,
        /// Raw base URL for direct HTTP calls (bypasses rig-core deserialization)
        raw_base_url: String,
        /// Raw API key for direct HTTP calls
        raw_api_key: String,
        /// M7: Shared HTTP client for raw calls (connection reuse)
        http_client: reqwest::Client,
    },
    /// Deterministic mock provider — no API keys, no network calls.
    /// Mock responses are generated in the executor (infer.rs, agent.rs),
    /// not through RigProvider completion methods.
    Mock,
    /// Native local provider - GGUF models via mistral.rs
    /// Requires `native-inference` feature and explicit model loading.
    /// Now uses NativeRuntime directly with full streaming support.
    #[cfg(feature = "native-inference")]
    Native(super::native::NativeRuntime),
}

impl std::fmt::Debug for RigProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Claude(_) => f.debug_tuple("Claude").field(&"...").finish(),
            Self::OpenAI(_) => f.debug_tuple("OpenAI").field(&"...").finish(),
            Self::Mistral(_) => f.debug_tuple("Mistral").field(&"...").finish(),
            Self::Groq(_) => f.debug_tuple("Groq").field(&"...").finish(),
            Self::DeepSeek(_) => f.debug_tuple("DeepSeek").field(&"...").finish(),
            Self::Gemini(_) => f.debug_tuple("Gemini").field(&"...").finish(),
            Self::XAi(_) => f.debug_tuple("XAi").field(&"...").finish(),
            Self::OpenAiCompat {
                endpoint_name,
                default_model,
                cached_name,
                timeout_secs,
                raw_base_url,
                ..
            } => f
                .debug_struct("OpenAiCompat")
                .field("endpoint_name", endpoint_name)
                .field("default_model", default_model)
                .field("cached_name", cached_name)
                .field("timeout_secs", timeout_secs)
                .field("raw_base_url", raw_base_url)
                .field("raw_api_key", &"***")
                .finish(),
            Self::Mock => write!(f, "Mock"),
            #[cfg(feature = "native-inference")]
            Self::Native(_) => f.debug_tuple("Native").field(&"...").finish(),
        }
    }
}

/// Static registry of OpenAI-compatible cloud providers.
///
/// Adding a new provider = 1 line here + 1 catalog entry in nika-core.
/// Zero Rust code needed for the provider itself.
///
/// (provider_id, base_url, env_var_key)
static OPENAI_COMPAT_PROVIDERS: &[(&str, &str, &str)] = &[
    (
        "openrouter",
        "https://openrouter.ai/api/v1",
        "OPENROUTER_API_KEY",
    ),
    (
        "together",
        "https://api.together.xyz/v1",
        "TOGETHER_API_KEY",
    ),
    (
        "fireworks",
        "https://api.fireworks.ai/inference/v1",
        "FIREWORKS_API_KEY",
    ),
    ("cerebras", "https://api.cerebras.ai/v1", "CEREBRAS_API_KEY"),
    (
        "sambanova",
        "https://api.sambanova.ai/v1",
        "SAMBANOVA_API_KEY",
    ),
    (
        "cohere",
        "https://api.cohere.com/compatibility/v1",
        "COHERE_API_KEY",
    ),
    ("ai21", "https://api.ai21.com/studio/v1", "AI21_API_KEY"),
];

/// Dispatch to the rig-core client for all standard providers.
///
/// Reduces 7+ identical match arms to 1. The body expression gets the
/// extracted client binding `$client`. Mock and Native are NOT dispatched
/// through this macro — they have custom paths.
///
/// IMPORTANT: the `$body` expression may be `.await`-ed inside the macro.
/// Rust macros expand before type checking, so each arm gets its own
/// monomorphized async block. Streams must be consumed INSIDE `$body`
/// because the stream type is not `Send` across arms.
macro_rules! dispatch_rig {
    ($self:expr, |$client:ident| $body:expr) => {
        match $self {
            RigProvider::Claude($client) => $body,
            RigProvider::OpenAI($client) => $body,
            RigProvider::Mistral($client) => $body,
            RigProvider::Groq($client) => $body,
            RigProvider::DeepSeek($client) => $body,
            RigProvider::Gemini($client) => $body,
            RigProvider::XAi($client) => $body,
            RigProvider::OpenAiCompat {
                client: $client, ..
            } => $body,
            RigProvider::Mock => {
                unreachable!("mock provider generates responses in executor, not via RigProvider")
            }
            #[cfg(feature = "native-inference")]
            RigProvider::Native(_) => {
                unreachable!("native uses a dedicated non-rig-core path")
            }
        }
    };
}

// ═══════════════════════════════════════════════════════════════════════════
// Submodules — split from the original 2000-line god file
// ═══════════════════════════════════════════════════════════════════════════

/// Construction, factory methods, metadata (name, default_model, auto, verify)
mod construction;
/// Non-streaming inference (infer, vision, tools, options) + capability checks
mod inference;
/// Streaming inference (text, vision, with options)
mod provider_streaming;

#[cfg(test)]
mod tests;

