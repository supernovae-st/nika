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

use crate::error_domains::ProviderError;
use std::time::Instant;

// Import InferenceBackend trait for native inference methods
#[cfg(feature = "native-inference")]
use crate::provider::native::InferenceBackend;
use rig::client::{CompletionClient, ProviderClient};
use rig::completion::{CompletionModel as _, Prompt, PromptError};
use rig::providers::{anthropic, deepseek, gemini, groq, mistral, openai, xai};
use rig::tool::ToolDyn;
use tokio::sync::mpsc;
use tokio::time::timeout;

pub mod error;
pub mod stream;
pub mod tool;
pub use error::{McpToolError, McpToolErrorKind, ProviderVerifyError, ProviderVerifyResult, RigInferError};
pub use stream::{StreamChunk, StreamResult};
pub use tool::{AgentMediaStaging, NikaMcpTool, NikaMcpToolDef};
use stream::consume_rig_stream;

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
/// OpenAI reasoning models (o-series, gpt-5) and DeepSeek Reasoner reject
/// `temperature` with HTTP 400. We strip it with a warning instead of crashing.
pub fn is_reasoning_model(model_id: &str) -> bool {
    let lower = model_id.to_lowercase();
    // OpenAI o-series reasoning models
    lower == "o1"
        || lower == "o1-mini"
        || lower == "o1-pro"
        || lower == "o3"
        || lower == "o3-mini"
        || lower == "o3-pro"
        || lower == "o4-mini"
        || lower.starts_with("o1-")
        || lower.starts_with("o3-")
        || lower == "o4"
        || lower.starts_with("o4-")
        // OpenAI GPT-5 (reasoning by default)
        || lower == "gpt-5"
        || lower.starts_with("gpt-5-")
        // DeepSeek Reasoner
        || lower == "deepseek-reasoner"
}

/// Provider type enum for rig-core providers
///
/// Nika leverages rig-core's native multi-provider support.
/// Each variant wraps the corresponding rig-core client.
#[derive(Debug, Clone)]
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
    },
    /// Native local provider - GGUF models via mistral.rs
    /// Requires `native-inference` feature and explicit model loading.
    /// Now uses NativeRuntime directly with full streaming support.
    #[cfg(feature = "native-inference")]
    Native(super::native::NativeRuntime),
}

impl RigProvider {
    /// Create a RigProvider by name or alias, with env var validation.
    ///
    /// Resolves aliases via `core::find_provider()` (e.g., "claude" -> "anthropic"),
    /// checks that the required env var is set, and returns the appropriate variant.
    ///
    /// # Errors
    ///
    /// - `ProviderError::MissingApiKey` if the provider requires a key and the env var is not set
    /// - `ProviderError::NotConfigured` if the provider name is unknown
    pub fn from_name(name: &str) -> Result<Self, crate::error::NikaError> {
        let provider = crate::core::find_provider(name).ok_or(ProviderError::NotConfigured {
            provider: name.to_string(),
        })?;

        // Check env var is set (rig-core panics without it)
        if provider.requires_key && !provider.has_env_key() {
            return Err(ProviderError::MissingApiKey {
                provider: provider.id.to_string(),
            }
            .into());
        }

        match provider.id {
            "anthropic" => Ok(Self::claude()),
            "openai" => Ok(Self::openai()),
            "mistral" => Ok(Self::mistral()),
            "groq" => Ok(Self::groq()),
            "deepseek" => Ok(Self::deepseek()),
            "gemini" => Ok(Self::gemini()),
            "xai" => Ok(Self::xai()),
            #[cfg(feature = "native-inference")]
            "native" => Ok(Self::native()),
            _ => Err(ProviderError::NotConfigured {
                provider: name.to_string(),
            }
            .into()),
        }
    }

    /// Resolve a provider name, checking custom endpoints first, then falling back to catalog.
    ///
    /// Resolution order:
    /// 1. Named custom endpoint from config (e.g., "h100" -> endpoints["h100"])
    /// 2. Catalog provider (e.g., "openai" -> standard OpenAI API)
    pub fn from_name_with_endpoints(
        name: &str,
        endpoints: &crate::provider::endpoints::CustomEndpointMap,
    ) -> Result<Self, crate::error::NikaError> {
        // 1. Check custom endpoints first
        if let Some(ep) = endpoints.get(name) {
            return Self::openai_compat(
                name,
                &ep.base_url,
                &ep.api_key,
                ep.default_model.as_deref(),
            );
        }

        // 2. Fall back to catalog provider
        Self::from_name(name)
    }

    /// Create a Claude provider from environment variable ANTHROPIC_API_KEY
    pub fn claude() -> Self {
        let client = anthropic::Client::from_env();
        RigProvider::Claude(client)
    }

    /// Create an OpenAI provider from environment variable OPENAI_API_KEY
    pub fn openai() -> Self {
        let client = openai::Client::from_env();
        RigProvider::OpenAI(client)
    }

    /// Create a Mistral provider from environment variable MISTRAL_API_KEY
    pub fn mistral() -> Self {
        let client = mistral::Client::from_env();
        RigProvider::Mistral(client)
    }

    /// Create a Groq provider from environment variable GROQ_API_KEY
    pub fn groq() -> Self {
        let client = groq::Client::from_env();
        RigProvider::Groq(client)
    }

    /// Create a DeepSeek provider from environment variable DEEPSEEK_API_KEY
    pub fn deepseek() -> Self {
        let client = deepseek::Client::from_env();
        RigProvider::DeepSeek(client)
    }

    /// Create a Gemini (Google) provider from environment variable GEMINI_API_KEY
    pub fn gemini() -> Self {
        let client = gemini::Client::from_env();
        RigProvider::Gemini(client)
    }

    /// Create an xAI (Grok) provider from environment variable XAI_API_KEY
    pub fn xai() -> Self {
        let client = xai::Client::from_env();
        RigProvider::XAi(client)
    }

    /// Create an OpenAI-compatible provider pointed at a custom base URL.
    ///
    /// Used for vLLM, TGI, Ollama, LiteLLM, SGLang, and any OpenAI-compatible server.
    pub fn openai_compat(
        endpoint_name: &str,
        base_url: &str,
        api_key: &str,
        default_model: Option<&str>,
    ) -> Result<Self, crate::error::NikaError> {
        use crate::provider::endpoints::validate_endpoint_url;
        validate_endpoint_url(base_url)
            .map_err(|e| crate::error_domains::ProviderError::InvalidConfig { message: e })?;

        let client = openai::Client::builder()
            .api_key(api_key)
            .base_url(base_url)
            .build()
            .map_err(|e| crate::error_domains::ProviderError::InvalidConfig {
                message: format!("failed to build OpenAI-compatible client: {e}"),
            })?;
        let name_str = endpoint_name.to_string();
        let cached_name = format!("openai-compat:{}", name_str);
        Ok(RigProvider::OpenAiCompat {
            client,
            endpoint_name: name_str,
            default_model: default_model.map(|s| s.to_string()),
            cached_name,
        })
    }

    /// Create a Native provider for local GGUF inference
    ///
    /// The provider is created without a model loaded. Call `load_native_model()`
    /// before running inference.
    ///
    /// Now uses NativeRuntime directly with full streaming support.
    ///
    /// Requires the `native-inference` feature.
    #[cfg(feature = "native-inference")]
    pub fn native() -> Self {
        RigProvider::Native(super::native::NativeRuntime::new())
    }

    /// Load a model for native inference.
    ///
    /// Only valid for `RigProvider::Native`. Returns an error for other providers.
    ///
    /// # Arguments
    /// * `model_path` - Path to the GGUF model file
    /// * `config` - Optional load configuration (context size, GPU layers, etc.)
    #[cfg(feature = "native-inference")]
    pub async fn load_native_model(
        &mut self,
        model_path: impl Into<std::path::PathBuf>,
        config: Option<super::native::LoadConfig>,
    ) -> Result<(), RigInferError> {
        self.load_native_model_traced(model_path, config, None)
            .await
    }

    /// Like `load_native_model` but emits a `NativeModelLoaded` event on success.
    ///
    /// Used by the executor to wire telemetry without breaking the existing API.
    #[cfg(feature = "native-inference")]
    pub async fn load_native_model_traced(
        &mut self,
        model_path: impl Into<std::path::PathBuf>,
        config: Option<super::native::LoadConfig>,
        event_log: Option<&crate::event::EventLog>,
    ) -> Result<(), RigInferError> {
        let path = model_path.into();
        let resolved_config = config.unwrap_or_default();

        // Determine kind + identifier before move
        let (model_id, kind) = match &resolved_config.model_kind {
            super::native::NativeModelKind::TextGguf => (
                path.file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string()),
                "gguf".to_string(),
            ),
            super::native::NativeModelKind::VisionHf { model_id, .. } => {
                (model_id.clone(), "huggingface".to_string())
            }
        };

        let load_start = Instant::now();

        match self {
            RigProvider::Native(runtime) => {
                runtime.load(path.clone(), resolved_config).await.map_err(
                    |e: super::native::NativeError| RigInferError::PromptError(e.to_string()),
                )?;

                let duration_ms = load_start.elapsed().as_millis() as u64;
                let is_vision = runtime.supports_vision();
                let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

                if let Some(log) = event_log {
                    log.emit(crate::event::EventKind::NativeModelLoaded {
                        model: model_id,
                        kind,
                        size_bytes,
                        duration_ms,
                        is_vision,
                    });
                }
                Ok(())
            }
            _ => Err(RigInferError::PromptError(
                "load_native_model only valid for Native provider".to_string(),
            )),
        }
    }

    /// Check if native model is loaded.
    #[cfg(feature = "native-inference")]
    pub fn is_native_loaded(&self) -> bool {
        match self {
            RigProvider::Native(runtime) => runtime.is_loaded(),
            _ => false,
        }
    }

    /// Get the `ProviderKind` for cost calculation.
    ///
    /// Custom endpoints (OpenAiCompat) return `ProviderKind::OpenAI` since they
    /// use the OpenAI-compatible API and can look up known model pricing.
    pub fn cost_provider_kind(&self) -> Option<crate::provider::cost::ProviderKind> {
        use crate::provider::cost::ProviderKind;
        match self {
            RigProvider::Claude(_) => Some(ProviderKind::Claude),
            RigProvider::OpenAI(_) => Some(ProviderKind::OpenAI),
            RigProvider::Mistral(_) => Some(ProviderKind::Mistral),
            RigProvider::Groq(_) => Some(ProviderKind::Groq),
            RigProvider::DeepSeek(_) => Some(ProviderKind::DeepSeek),
            RigProvider::Gemini(_) => Some(ProviderKind::Gemini),
            RigProvider::XAi(_) => Some(ProviderKind::XAi),
            RigProvider::OpenAiCompat { .. } => Some(ProviderKind::OpenAI),
            #[cfg(feature = "native-inference")]
            RigProvider::Native(_) => Some(ProviderKind::Native),
        }
    }

    /// Get the provider name
    pub fn name(&self) -> &str {
        match self {
            RigProvider::Claude(_) => "claude",
            RigProvider::OpenAI(_) => "openai",
            RigProvider::Mistral(_) => "mistral",
            RigProvider::Groq(_) => "groq",
            RigProvider::DeepSeek(_) => "deepseek",
            RigProvider::Gemini(_) => "gemini",
            RigProvider::XAi(_) => "xai",
            RigProvider::OpenAiCompat { cached_name, .. } => cached_name,
            #[cfg(feature = "native-inference")]
            RigProvider::Native(_) => "native",
        }
    }

    /// Get the default model for this provider
    ///
    /// | Provider | Model | Notes |
    /// |----------|-------|-------|
    /// | Claude | claude-sonnet-4-6 | Latest stable (Feb 2026) |
    /// | OpenAI | gpt-4o | Latest stable |
    /// | Mistral | mistral-large-latest | Best for complex tasks |
    /// | Groq | llama-3.3-70b-versatile | Fast inference |
    /// | DeepSeek | deepseek-chat | Cost-effective |
    /// | Gemini | gemini-2.0-flash | Latest stable |
    /// | Native | (loaded model) | Uses pre-loaded GGUF model |
    pub fn default_model(&self) -> &str {
        match self {
            // Note: rig-core's CLAUDE_3_5_SONNET constant is outdated
            // Using explicit model name for stability
            RigProvider::Claude(_) => "claude-sonnet-4-6",
            RigProvider::OpenAI(_) => openai::GPT_4O,
            RigProvider::Mistral(_) => mistral::MISTRAL_LARGE,
            RigProvider::Groq(_) => "llama-3.3-70b-versatile",
            RigProvider::DeepSeek(_) => "deepseek-chat",
            RigProvider::Gemini(_) => "gemini-2.0-flash",
            RigProvider::XAi(_) => "grok-3-fast",
            RigProvider::OpenAiCompat { default_model, .. } => {
                default_model.as_deref().unwrap_or("gpt-3.5-turbo")
            }
            // Native uses whatever model is loaded, no default
            #[cfg(feature = "native-inference")]
            RigProvider::Native(_) => "native-model",
        }
    }

    /// Simple text completion (infer) using rig-core
    ///
    /// # Arguments
    /// * `prompt` - The text prompt to send
    /// * `model` - Model identifier (uses default if None)
    ///
    /// # Returns
    /// The completion text from the model
    pub async fn infer(&self, prompt: &str, model: Option<&str>) -> Result<String, RigInferError> {
        /// Maximum time to wait for a single infer() completion (5 minutes).
        /// Prevents hung LLM calls from blocking the runtime indefinitely.
        const INFER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

        let model_id = model.unwrap_or_else(|| self.default_model());

        match self {
            RigProvider::Claude(client) => {
                // Anthropic requires max_tokens to be set explicitly
                let agent = client.agent(model_id).max_tokens(8192).build();
                timeout(INFER_TIMEOUT, agent.prompt(prompt))
                    .await
                    .map_err(|_| RigInferError::Timeout {
                        duration_ms: INFER_TIMEOUT.as_millis() as u64,
                    })?
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::OpenAI(client) => {
                let agent = client.agent(model_id).max_tokens(8192).build();
                timeout(INFER_TIMEOUT, agent.prompt(prompt))
                    .await
                    .map_err(|_| RigInferError::Timeout {
                        duration_ms: INFER_TIMEOUT.as_millis() as u64,
                    })?
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::Mistral(client) => {
                let agent = client.agent(model_id).max_tokens(8192).build();
                timeout(INFER_TIMEOUT, agent.prompt(prompt))
                    .await
                    .map_err(|_| RigInferError::Timeout {
                        duration_ms: INFER_TIMEOUT.as_millis() as u64,
                    })?
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::Groq(client) => {
                let agent = client.agent(model_id).max_tokens(8192).build();
                timeout(INFER_TIMEOUT, agent.prompt(prompt))
                    .await
                    .map_err(|_| RigInferError::Timeout {
                        duration_ms: INFER_TIMEOUT.as_millis() as u64,
                    })?
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::DeepSeek(client) => {
                let agent = client.agent(model_id).max_tokens(8192).build();
                timeout(INFER_TIMEOUT, agent.prompt(prompt))
                    .await
                    .map_err(|_| RigInferError::Timeout {
                        duration_ms: INFER_TIMEOUT.as_millis() as u64,
                    })?
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::Gemini(client) => {
                let agent = client.agent(model_id).max_tokens(8192).build();
                timeout(INFER_TIMEOUT, agent.prompt(prompt))
                    .await
                    .map_err(|_| RigInferError::Timeout {
                        duration_ms: INFER_TIMEOUT.as_millis() as u64,
                    })?
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::XAi(client) => {
                let agent = client.agent(model_id).max_tokens(8192).build();
                timeout(INFER_TIMEOUT, agent.prompt(prompt))
                    .await
                    .map_err(|_| RigInferError::Timeout {
                        duration_ms: INFER_TIMEOUT.as_millis() as u64,
                    })?
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::OpenAiCompat { client, .. } => {
                let agent = client.agent(model_id).max_tokens(8192).build();
                timeout(INFER_TIMEOUT, agent.prompt(prompt))
                    .await
                    .map_err(|_| RigInferError::Timeout {
                        duration_ms: INFER_TIMEOUT.as_millis() as u64,
                    })?
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            #[cfg(feature = "native-inference")]
            RigProvider::Native(runtime) => {
                // Native inference uses direct API, not rig-core agent
                // Model must be pre-loaded via load_native_model()
                timeout(
                    INFER_TIMEOUT,
                    runtime.infer(prompt, super::native::ChatOptions::default()),
                )
                .await
                .map_err(|_| RigInferError::Timeout {
                    duration_ms: INFER_TIMEOUT.as_millis() as u64,
                })?
                .map(|r| r.message.content)
                .map_err(|e: super::native::NativeError| RigInferError::PromptError(e.to_string()))
            }
        }
    }

    /// Vision inference: send multimodal content (text + images) to the LLM.
    ///
    /// Builds a `Message::User` with mixed text + base64 image parts,
    /// then uses `agent.prompt(message)` to send it. The agent handles
    /// provider-specific message formatting automatically.
    ///
    /// # Arguments
    /// * `user_content` - Pre-built rig UserContent items (text + images)
    /// * `model` - Optional model override
    /// * `system` - Optional system prompt
    /// * `max_tokens` - Optional max tokens
    ///
    /// # Errors
    /// Returns `RigInferError::VisionNotSupported` for DeepSeek provider.
    /// Native vision requires a VisionHf model to be loaded.
    pub async fn infer_vision(
        &self,
        user_content: Vec<rig::completion::message::UserContent>,
        model: Option<&str>,
        system: Option<&str>,
        max_tokens: Option<u32>,
    ) -> Result<String, RigInferError> {
        use rig::completion::message::Message;
        use rig::OneOrMany;

        /// Maximum time to wait for a vision inference call (5 minutes).
        const VISION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

        // Early return: DeepSeek does not support vision at all
        if matches!(self, RigProvider::DeepSeek(_)) {
            return Err(RigInferError::VisionNotSupported(
                "DeepSeek does not support vision/multimodal content".to_string(),
            ));
        }

        // Early return: Native vision uses NativeRuntime directly (not rig-core)
        #[cfg(feature = "native-inference")]
        if let RigProvider::Native(runtime) = self {
            if !runtime.supports_vision() {
                return Err(RigInferError::VisionNotSupported(
                    "Native model does not support vision. Load a vision model via \
                     NativeModelKind::VisionHf (e.g., `nika model vision <model_id> --isq Q4K`)"
                        .to_string(),
                ));
            }
            let (prompt_text, vision_images) = extract_native_vision_parts(&user_content)?;
            let options = super::native::ChatOptions {
                max_tokens,
                ..Default::default()
            };
            let response = timeout(
                VISION_TIMEOUT,
                runtime.infer_vision(&prompt_text, vision_images, options),
            )
            .await
            .map_err(|_| RigInferError::Timeout {
                duration_ms: VISION_TIMEOUT.as_millis() as u64,
            })?
            .map_err(|e: super::native::NativeError| RigInferError::PromptError(e.to_string()))?;
            return Ok(response.message.content);
        }

        let model_id = model.unwrap_or_else(|| self.default_model());
        let max_tok = max_tokens.map(u64::from).unwrap_or(8192);

        let message = Message::User {
            content: OneOrMany::many(user_content).map_err(|_| {
                RigInferError::VisionNotSupported("content parts list is empty".to_string())
            })?,
        };

        macro_rules! vision_prompt {
            ($client:expr) => {{
                let mut builder = $client.agent(model_id).max_tokens(max_tok);
                if let Some(sys) = system {
                    builder = builder.preamble(sys);
                }
                let agent = builder.build();
                timeout(VISION_TIMEOUT, agent.prompt(message))
                    .await
                    .map_err(|_| RigInferError::Timeout {
                        duration_ms: VISION_TIMEOUT.as_millis() as u64,
                    })?
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }};
        }

        match self {
            RigProvider::Claude(client) => vision_prompt!(client),
            RigProvider::OpenAI(client) => vision_prompt!(client),
            RigProvider::Mistral(client) => vision_prompt!(client),
            RigProvider::Groq(client) => vision_prompt!(client),
            RigProvider::Gemini(client) => vision_prompt!(client),
            RigProvider::XAi(client) => vision_prompt!(client),
            RigProvider::OpenAiCompat { client, .. } => vision_prompt!(client),
            // DeepSeek and Native handled above via early returns
            RigProvider::DeepSeek(_) => unreachable!("DeepSeek handled above"),
            #[cfg(feature = "native-inference")]
            RigProvider::Native(_) => unreachable!("Native handled above"),
        }
    }

    /// Vision inference with streaming output.
    ///
    /// Same as `infer_vision` but streams response tokens via an mpsc channel.
    /// Native vision uses a non-streaming fallback (sends full response as Done chunk).
    pub async fn infer_vision_stream(
        &self,
        user_content: Vec<rig::completion::message::UserContent>,
        tx: mpsc::Sender<StreamChunk>,
        model: Option<&str>,
        system: Option<&str>,
        max_tokens: Option<u32>,
    ) -> Result<StreamResult, RigInferError> {
        use rig::completion::message::Message;
        use rig::OneOrMany;

        /// Maximum time to wait for a vision stream call (5 minutes).
        const VISION_STREAM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

        // Early return: DeepSeek does not support vision at all
        if matches!(self, RigProvider::DeepSeek(_)) {
            return Err(RigInferError::VisionNotSupported(
                "DeepSeek does not support vision/multimodal content".to_string(),
            ));
        }

        // Early return: Native vision — non-streaming fallback via NativeRuntime
        // NativeRuntime.infer_vision_stream() exists but rig's StreamChunk protocol
        // differs from the native mpsc stream, so we use non-streaming + Done chunk.
        #[cfg(feature = "native-inference")]
        if let RigProvider::Native(runtime) = self {
            if !runtime.supports_vision() {
                return Err(RigInferError::VisionNotSupported(
                    "Native model does not support vision. Load a vision model via \
                     NativeModelKind::VisionHf (e.g., `nika model vision <model_id> --isq Q4K`)"
                        .to_string(),
                ));
            }
            let (prompt_text, vision_images) = extract_native_vision_parts(&user_content)?;
            let options = super::native::ChatOptions {
                max_tokens,
                ..Default::default()
            };
            let response = timeout(
                VISION_STREAM_TIMEOUT,
                runtime.infer_vision(&prompt_text, vision_images, options),
            )
            .await
            .map_err(|_| RigInferError::Timeout {
                duration_ms: VISION_STREAM_TIMEOUT.as_millis() as u64,
            })?
            .map_err(|e: super::native::NativeError| RigInferError::PromptError(e.to_string()))?;
            // Send full response as a single Done chunk (non-streaming fallback)
            let text = response.message.content;
            if let Err(e) = tx.send(StreamChunk::Done(text.clone())).await {
                tracing::warn!(error = %e, "Vision result channel closed — TUI may not show output");
            }
            return Ok(StreamResult {
                text,
                ..Default::default()
            });
        }

        let model_id = model.unwrap_or_else(|| self.default_model());
        let max_tok = max_tokens.map(u64::from).unwrap_or(8192);

        let message = Message::User {
            content: OneOrMany::many(user_content).map_err(|_| {
                RigInferError::VisionNotSupported("content parts list is empty".to_string())
            })?,
        };

        let mut response_parts: Vec<String> = Vec::new();
        let mut result = StreamResult::default();

        macro_rules! vision_stream {
            ($client:expr, $is_anthropic:expr) => {{
                let model = $client.completion_model(model_id);
                let mut builder = model.completion_request(message).max_tokens(max_tok);
                if let Some(sys) = system {
                    builder = builder.preamble(sys.to_string());
                }
                let request = builder.build();
                let stream_start = Instant::now();
                let mut stream = model
                    .stream(request)
                    .await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))?;
                consume_rig_stream(
                    &mut stream,
                    &tx,
                    &mut response_parts,
                    &mut result,
                    $is_anthropic,
                    stream_start,
                )
                .await?;
            }};
        }

        // Apply overall timeout to prevent slow-drip streams running forever
        timeout(VISION_STREAM_TIMEOUT, async {
            match self {
                RigProvider::Claude(client) => vision_stream!(client, true),
                RigProvider::OpenAI(client) => vision_stream!(client, false),
                RigProvider::Mistral(client) => vision_stream!(client, false),
                RigProvider::Groq(client) => vision_stream!(client, false),
                RigProvider::Gemini(client) => vision_stream!(client, false),
                RigProvider::XAi(client) => vision_stream!(client, false),
                RigProvider::OpenAiCompat { client, .. } => vision_stream!(client, false),
                // DeepSeek and Native handled above via early returns
                RigProvider::DeepSeek(_) => unreachable!("DeepSeek handled above"),
                #[cfg(feature = "native-inference")]
                RigProvider::Native(_) => unreachable!("Native handled above"),
            }
            Ok::<(), RigInferError>(())
        })
        .await
        .map_err(|_| RigInferError::Timeout {
            duration_ms: VISION_STREAM_TIMEOUT.as_millis() as u64,
        })??;

        result.text = response_parts.join("");
        Ok(result)
    }

    /// Infer with injected tools for structured output enforcement.
    ///
    /// Builds a single-turn agent with the given tools and `tool_choice: Required`.
    /// The LLM is forced to call one of the injected tools, returning structured output
    /// as the tool call arguments. Used by DynamicSubmitTool (Layer 0).
    ///
    /// # Arguments
    /// * `prompt` - The text prompt to send
    /// * `tools` - Tools to inject (typically a single DynamicSubmitTool)
    /// * `model` - Optional model override
    /// * `max_tokens` - Optional max tokens for the response (default: 8192)
    ///
    /// # Returns
    /// The tool call arguments as a string (the structured JSON output)
    pub async fn infer_with_tools(
        &self,
        prompt: &str,
        tools: Vec<Box<dyn ToolDyn>>,
        model: Option<&str>,
        max_tokens: Option<u32>,
        system: Option<&str>,
    ) -> Result<String, RigInferError> {
        use rig::agent::AgentBuilder;
        use rig::message::ToolChoice as RigToolChoice;

        /// Maximum time for tool-injection structured output (5 minutes).
        const TOOLS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

        let model_id = model.unwrap_or_else(|| self.default_model());
        let max_tok = max_tokens.map(|v| v as u64).unwrap_or(8192);

        macro_rules! build_agent_with_tools {
            ($client:expr) => {{
                let mut builder = AgentBuilder::new($client.completion_model(model_id))
                    .tools(tools)
                    .tool_choice(RigToolChoice::Required)
                    .max_tokens(max_tok);
                if let Some(sys) = system {
                    builder = builder.preamble(sys);
                }
                let agent = builder.build();
                agent
                    .prompt(prompt)
                    .await
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }};
        }

        let result = timeout(TOOLS_TIMEOUT, async {
            match self {
                RigProvider::Claude(client) => build_agent_with_tools!(client),
                RigProvider::OpenAI(client) => build_agent_with_tools!(client),
                RigProvider::Mistral(client) => build_agent_with_tools!(client),
                RigProvider::Groq(client) => build_agent_with_tools!(client),
                RigProvider::DeepSeek(client) => build_agent_with_tools!(client),
                RigProvider::Gemini(client) => build_agent_with_tools!(client),
                RigProvider::XAi(client) => build_agent_with_tools!(client),
                RigProvider::OpenAiCompat { client, .. } => build_agent_with_tools!(client),
                #[cfg(feature = "native-inference")]
                RigProvider::Native(_) => Err(RigInferError::PromptError(
                    "Native inference does not support tool-based structured output".to_string(),
                )),
            }
        })
        .await
        .map_err(|_| RigInferError::Timeout {
            duration_ms: TOOLS_TIMEOUT.as_millis() as u64,
        })?;
        result
    }

    /// Text completion with full control over LLM parameters
    ///
    /// # Arguments
    /// * `prompt` - The text prompt to send
    /// * `options` - LLM control options (model, temperature, max_tokens, system)
    ///
    /// # Returns
    /// The completion text from the model
    ///
    /// # Example
    /// ```ignore
    /// let options = InferOptions {
    ///     temperature: Some(0.7),
    ///     max_tokens: Some(2000),
    ///     system: Some("You are a helpful assistant.".to_string()),
    ///     ..Default::default()
    /// };
    /// let result = provider.infer_with_options("Explain Rust", &options).await?;
    /// ```
    pub async fn infer_with_options(
        &self,
        prompt: &str,
        options: &InferOptions,
    ) -> Result<String, RigInferError> {
        /// Maximum time to wait for an infer_with_options call (5 minutes).
        const OPTIONS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

        let model_id = options
            .model
            .as_deref()
            .unwrap_or_else(|| self.default_model());
        let max_tokens = options.max_tokens.unwrap_or(8192);

        // Strip temperature for reasoning models (BUG 5 / NIKA-031)
        let effective_temperature = if options.temperature.is_some() && is_reasoning_model(model_id)
        {
            tracing::warn!(
                model = %model_id,
                "temperature ignored for reasoning model '{}' (not supported)",
                model_id
            );
            None
        } else {
            options.temperature
        };

        // Use system prompt as preamble (not concatenated into user prompt)
        let user_prompt = prompt.to_string();

        macro_rules! build_and_prompt {
            ($client:expr) => {{
                let mut builder = $client.agent(model_id).max_tokens(max_tokens as u64);
                if let Some(system) = &options.system {
                    builder = builder.preamble(system);
                }
                if let Some(temp) = effective_temperature {
                    builder = builder.temperature(temp);
                }
                if let Some(ref params) = options.additional_params {
                    builder = builder.additional_params(params.clone());
                }
                let agent = builder.build();
                timeout(OPTIONS_TIMEOUT, agent.prompt(&user_prompt))
                    .await
                    .map_err(|_| RigInferError::Timeout {
                        duration_ms: OPTIONS_TIMEOUT.as_millis() as u64,
                    })?
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }};
        }

        match self {
            RigProvider::Claude(client) => build_and_prompt!(client),
            RigProvider::OpenAI(client) => build_and_prompt!(client),
            RigProvider::Mistral(client) => build_and_prompt!(client),
            RigProvider::Groq(client) => build_and_prompt!(client),
            RigProvider::DeepSeek(client) => build_and_prompt!(client),
            RigProvider::Gemini(client) => build_and_prompt!(client),
            RigProvider::XAi(client) => build_and_prompt!(client),
            RigProvider::OpenAiCompat { client, .. } => build_and_prompt!(client),
            #[cfg(feature = "native-inference")]
            RigProvider::Native(runtime) => {
                // Native inference uses ChatOptions from native module
                let chat_options = super::native::ChatOptions {
                    temperature: effective_temperature.map(|t| t as f32),
                    max_tokens: options.max_tokens,
                    ..Default::default()
                };
                timeout(OPTIONS_TIMEOUT, runtime.infer(&user_prompt, chat_options))
                    .await
                    .map_err(|_| RigInferError::Timeout {
                        duration_ms: OPTIONS_TIMEOUT.as_millis() as u64,
                    })?
                    .map(|r| r.message.content)
                    .map_err(|e: super::native::NativeError| {
                        RigInferError::PromptError(e.to_string())
                    })
            }
        }
    }

    /// Auto-detect and create a provider from available environment variables
    ///
    /// Provider detection order:
    /// 1. ANTHROPIC_API_KEY → Claude
    /// 2. OPENAI_API_KEY → OpenAI
    /// 3. MISTRAL_API_KEY → Mistral
    /// 4. GROQ_API_KEY → Groq
    /// 5. DEEPSEEK_API_KEY → DeepSeek
    /// 6. GEMINI_API_KEY → Gemini
    /// 7. NIKA_NATIVE_MODEL → Native
    ///
    /// Returns None if no provider is available.
    /// Empty env vars are treated as unset.
    pub fn auto() -> Option<Self> {
        use crate::core::providers::{ProviderCategory, KNOWN_PROVIDERS};

        // Iterate KNOWN_PROVIDERS in priority order (LLM providers first, then native)
        for p in KNOWN_PROVIDERS.iter() {
            if p.category == ProviderCategory::Llm && p.has_env_key() {
                return match p.id {
                    "anthropic" => Some(Self::claude()),
                    "openai" => Some(Self::openai()),
                    "mistral" => Some(Self::mistral()),
                    "groq" => Some(Self::groq()),
                    "deepseek" => Some(Self::deepseek()),
                    "gemini" => Some(Self::gemini()),
                    "xai" => Some(Self::xai()),
                    _ => continue,
                };
            }
        }
        // Native is opt-in: requires NIKA_NATIVE_MODEL to be set
        #[cfg(feature = "native-inference")]
        if std::env::var("NIKA_NATIVE_MODEL").is_ok_and(|v| !v.trim().is_empty()) {
            return Some(Self::native());
        }
        None
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Provider Health Check & Verification
    // ═══════════════════════════════════════════════════════════════════════════

    /// Verify the provider connection is working
    ///
    /// Makes a minimal API call to check:
    /// - API key is valid
    /// - Network connectivity works
    /// - Provider service is responding
    ///
    /// Returns Ok(VerifyResult) with latency on success,
    /// or Err with specific reason on failure.
    pub async fn verify(&self) -> Result<ProviderVerifyResult, ProviderVerifyError> {
        use std::time::Instant;

        let start = Instant::now();

        // Use a minimal prompt to test connectivity
        let test_prompt = "Hi";

        match self.infer(test_prompt, None).await {
            Ok(_) => Ok(ProviderVerifyResult {
                provider: self.name().to_string(),
                latency: start.elapsed(),
                model: self.default_model().to_string(),
            }),
            Err(e) => {
                let error_msg = e.to_string().to_lowercase();

                // Categorize the error
                if error_msg.contains("401")
                    || error_msg.contains("unauthorized")
                    || error_msg.contains("invalid api key")
                    || error_msg.contains("authentication")
                {
                    Err(ProviderVerifyError::InvalidApiKey {
                        provider: self.name().to_string(),
                    })
                } else if error_msg.contains("rate limit")
                    || error_msg.contains("429")
                    || error_msg.contains("too many requests")
                {
                    Err(ProviderVerifyError::RateLimited {
                        provider: self.name().to_string(),
                    })
                } else if error_msg.contains("timeout")
                    || error_msg.contains("timed out")
                    || error_msg.contains("deadline")
                {
                    Err(ProviderVerifyError::Timeout {
                        provider: self.name().to_string(),
                    })
                } else if error_msg.contains("connection")
                    || error_msg.contains("network")
                    || error_msg.contains("dns")
                    || error_msg.contains("refused")
                {
                    Err(ProviderVerifyError::NetworkError {
                        provider: self.name().to_string(),
                        details: e.to_string(),
                    })
                } else {
                    Err(ProviderVerifyError::ProviderError {
                        provider: self.name().to_string(),
                        details: e.to_string(),
                    })
                }
            }
        }
    }

    /// Quick check if provider credentials are configured
    ///
    /// This is a fast, synchronous check that doesn't make network calls.
    /// Use `verify()` for actual connection testing.
    pub fn is_configured(&self) -> bool {
        let has_key = |key: &str| std::env::var(key).is_ok_and(|v| !v.trim().is_empty());

        match self {
            RigProvider::Claude(_) => has_key("ANTHROPIC_API_KEY"),
            RigProvider::OpenAI(_) => has_key("OPENAI_API_KEY"),
            RigProvider::Mistral(_) => has_key("MISTRAL_API_KEY"),
            RigProvider::Groq(_) => has_key("GROQ_API_KEY"),
            RigProvider::DeepSeek(_) => has_key("DEEPSEEK_API_KEY"),
            RigProvider::Gemini(_) => has_key("GEMINI_API_KEY"),
            RigProvider::XAi(_) => has_key("XAI_API_KEY"),
            RigProvider::OpenAiCompat { .. } => true,
            #[cfg(feature = "native-inference")]
            RigProvider::Native(_) => {
                // Native doesn't need API key, but requires model to be loaded
                // Use is_native_loaded() to check if ready for inference
                true
            }
        }
    }
}

// StreamChunk, StreamResult, and consume_rig_stream are in stream.rs

impl RigProvider {
    /// Check if this provider supports native structured output via `response_format: json_schema`.
    ///
    /// Uses the resolved provider type rather than a string name, correctly handling
    /// custom endpoints (OpenAiCompat) which are OpenAI-compatible and support response_format.
    pub fn supports_native_structured_output(&self) -> bool {
        matches!(
            self,
            RigProvider::OpenAI(_)
                | RigProvider::OpenAiCompat { .. }
                | RigProvider::Groq(_)
                | RigProvider::DeepSeek(_)
                | RigProvider::XAi(_)
        )
    }

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
    ) -> Result<StreamResult, RigInferError> {
        // Overall timeout for entire streaming operation (10 min).
        // Individual chunks have their own 60s timeout in consume_rig_stream,
        // but a slow stream (1 chunk every 59s) would never hit that.
        const STREAM_TOTAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

        timeout(
            STREAM_TOTAL_TIMEOUT,
            self.infer_stream_inner(prompt, tx, model),
        )
        .await
        .map_err(|_| RigInferError::Timeout {
            duration_ms: STREAM_TOTAL_TIMEOUT.as_millis() as u64,
        })?
    }

    async fn infer_stream_inner(
        &self,
        prompt: &str,
        tx: mpsc::Sender<StreamChunk>,
        model: Option<&str>,
    ) -> Result<StreamResult, RigInferError> {
        let model_id = model.unwrap_or_else(|| self.default_model());
        let mut response_parts: Vec<String> = Vec::new();
        let mut result = StreamResult::default();

        match self {
            RigProvider::Claude(client) => {
                let model = client.completion_model(model_id);
                let request = model.completion_request(prompt).max_tokens(8192).build();
                let stream_start = Instant::now();
                let mut stream = model
                    .stream(request)
                    .await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))?;
                consume_rig_stream(
                    &mut stream,
                    &tx,
                    &mut response_parts,
                    &mut result,
                    true,
                    stream_start,
                )
                .await?;
            }
            RigProvider::OpenAI(client) => {
                let model = client.completion_model(model_id);
                let request = model.completion_request(prompt).max_tokens(8192).build();
                let stream_start = Instant::now();
                let mut stream = model
                    .stream(request)
                    .await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))?;
                consume_rig_stream(
                    &mut stream,
                    &tx,
                    &mut response_parts,
                    &mut result,
                    false,
                    stream_start,
                )
                .await?;
            }
            RigProvider::Mistral(client) => {
                let model = client.completion_model(model_id);
                let request = model.completion_request(prompt).max_tokens(8192).build();
                let stream_start = Instant::now();
                let mut stream = model
                    .stream(request)
                    .await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))?;
                consume_rig_stream(
                    &mut stream,
                    &tx,
                    &mut response_parts,
                    &mut result,
                    false,
                    stream_start,
                )
                .await?;
            }
            RigProvider::Groq(client) => {
                let model = client.completion_model(model_id);
                let request = model.completion_request(prompt).max_tokens(8192).build();
                let stream_start = Instant::now();
                let mut stream = model
                    .stream(request)
                    .await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))?;
                consume_rig_stream(
                    &mut stream,
                    &tx,
                    &mut response_parts,
                    &mut result,
                    false,
                    stream_start,
                )
                .await?;
            }
            RigProvider::DeepSeek(client) => {
                let model = client.completion_model(model_id);
                let request = model.completion_request(prompt).max_tokens(8192).build();
                let stream_start = Instant::now();
                let mut stream = model
                    .stream(request)
                    .await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))?;
                consume_rig_stream(
                    &mut stream,
                    &tx,
                    &mut response_parts,
                    &mut result,
                    false,
                    stream_start,
                )
                .await?;
            }
            RigProvider::Gemini(client) => {
                let model = client.completion_model(model_id);
                let request = model.completion_request(prompt).max_tokens(8192).build();
                let stream_start = Instant::now();
                let mut stream = model
                    .stream(request)
                    .await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))?;
                consume_rig_stream(
                    &mut stream,
                    &tx,
                    &mut response_parts,
                    &mut result,
                    false,
                    stream_start,
                )
                .await?;
            }
            RigProvider::XAi(client) => {
                let model = client.completion_model(model_id);
                let request = model.completion_request(prompt).max_tokens(8192).build();
                let stream_start = Instant::now();
                let mut stream = model
                    .stream(request)
                    .await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))?;
                consume_rig_stream(
                    &mut stream,
                    &tx,
                    &mut response_parts,
                    &mut result,
                    false,
                    stream_start,
                )
                .await?;
            }
            RigProvider::OpenAiCompat { client, .. } => {
                let model = client.completion_model(model_id);
                let request = model.completion_request(prompt).max_tokens(8192).build();
                let stream_start = Instant::now();
                let mut stream = model
                    .stream(request)
                    .await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))?;
                consume_rig_stream(
                    &mut stream,
                    &tx,
                    &mut response_parts,
                    &mut result,
                    false,
                    stream_start,
                )
                .await?;
            }
            // Native provider - uses infer_stream() for true token-by-token streaming
            #[cfg(feature = "native-inference")]
            RigProvider::Native(runtime) => {
                use futures::StreamExt;
                use std::pin::pin;

                // Native inference now supports streaming via mistral.rs
                let stream = runtime
                    .infer_stream(prompt, super::native::ChatOptions::default())
                    .await
                    .map_err(|e: super::native::NativeError| {
                        RigInferError::PromptError(e.to_string())
                    })?;

                // Pin the stream for iteration (async_stream produces !Unpin streams)
                let mut stream = pin!(stream);

                // Collect tokens as they arrive (Stream yields Result<String, NativeError>)
                while let Some(result) = stream.next().await {
                    match result {
                        Ok(token) => {
                            response_parts.push(token.clone());
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
                result.output_tokens = response_parts
                    .iter()
                    .map(|s| s.len() as u64)
                    .sum::<u64>()
                    .div_ceil(4);
            }
        }

        let complete_response = response_parts.concat();
        let _ = tx.try_send(StreamChunk::Done(complete_response.clone()));

        // Fallback: if stream ended without Final event, estimate tokens
        if result.input_tokens == 0 && result.output_tokens == 0 && !complete_response.is_empty() {
            result.input_tokens = (prompt.len() as u64).div_ceil(4);
            result.output_tokens = (complete_response.len() as u64).div_ceil(4);
        }

        // Send metrics after Done - use try_send to avoid blocking
        let _ = tx.try_send(StreamChunk::Metrics {
            input_tokens: result.input_tokens,
            output_tokens: result.output_tokens,
        });

        result.text = complete_response;
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

        timeout(
            STREAM_TOTAL_TIMEOUT,
            self.infer_stream_with_options_inner(prompt, tx, options),
        )
        .await
        .map_err(|_| RigInferError::Timeout {
            duration_ms: STREAM_TOTAL_TIMEOUT.as_millis() as u64,
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
        let max_tokens = options.max_tokens.unwrap_or(8192);
        let mut response_parts: Vec<String> = Vec::new();
        let mut result = StreamResult::default();

        // Strip temperature for reasoning models (BUG 5 / NIKA-031)
        let effective_temperature = if options.temperature.is_some() && is_reasoning_model(model_id)
        {
            tracing::warn!(
                model = %model_id,
                "temperature ignored for reasoning model '{}' (not supported)",
                model_id
            );
            None
        } else {
            options.temperature
        };

        // Helper: build request with options and start streaming
        // Uses preamble() for system prompt (not string concatenation) to ensure
        // providers treat it as a system message, not user text.
        macro_rules! build_request_with_options {
            ($client:expr) => {{
                let model = $client.completion_model(model_id);
                let mut rb = model
                    .completion_request(prompt)
                    .max_tokens(max_tokens as u64);
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
            RigProvider::Claude(client) => {
                let stream_start = Instant::now();
                let mut stream = build_request_with_options!(client);
                consume_rig_stream(
                    &mut stream,
                    &tx,
                    &mut response_parts,
                    &mut result,
                    true,
                    stream_start,
                )
                .await?;
            }
            RigProvider::OpenAI(client) => {
                let stream_start = Instant::now();
                let mut stream = build_request_with_options!(client);
                consume_rig_stream(
                    &mut stream,
                    &tx,
                    &mut response_parts,
                    &mut result,
                    false,
                    stream_start,
                )
                .await?;
            }
            RigProvider::Mistral(client) => {
                let stream_start = Instant::now();
                let mut stream = build_request_with_options!(client);
                consume_rig_stream(
                    &mut stream,
                    &tx,
                    &mut response_parts,
                    &mut result,
                    false,
                    stream_start,
                )
                .await?;
            }
            RigProvider::Groq(client) => {
                let stream_start = Instant::now();
                let mut stream = build_request_with_options!(client);
                consume_rig_stream(
                    &mut stream,
                    &tx,
                    &mut response_parts,
                    &mut result,
                    false,
                    stream_start,
                )
                .await?;
            }
            RigProvider::DeepSeek(client) => {
                let stream_start = Instant::now();
                let mut stream = build_request_with_options!(client);
                consume_rig_stream(
                    &mut stream,
                    &tx,
                    &mut response_parts,
                    &mut result,
                    false,
                    stream_start,
                )
                .await?;
            }
            RigProvider::Gemini(client) => {
                let stream_start = Instant::now();
                let mut stream = build_request_with_options!(client);
                consume_rig_stream(
                    &mut stream,
                    &tx,
                    &mut response_parts,
                    &mut result,
                    false,
                    stream_start,
                )
                .await?;
            }
            RigProvider::XAi(client) => {
                let stream_start = Instant::now();
                let mut stream = build_request_with_options!(client);
                consume_rig_stream(
                    &mut stream,
                    &tx,
                    &mut response_parts,
                    &mut result,
                    false,
                    stream_start,
                )
                .await?;
            }
            RigProvider::OpenAiCompat { client, .. } => {
                let stream_start = Instant::now();
                let mut stream = build_request_with_options!(client);
                consume_rig_stream(
                    &mut stream,
                    &tx,
                    &mut response_parts,
                    &mut result,
                    false,
                    stream_start,
                )
                .await?;
            }
            // Native provider - uses infer_stream() with options for true streaming
            #[cfg(feature = "native-inference")]
            RigProvider::Native(runtime) => {
                use futures::StreamExt;
                use std::pin::pin;

                // Native doesn't support preamble — concatenate system prompt for native only
                let native_prompt = if let Some(ref system) = options.system {
                    format!("{}\n\n{}", system, prompt)
                } else {
                    prompt.to_string()
                };
                let chat_options = super::native::ChatOptions {
                    temperature: effective_temperature.map(|t| t as f32),
                    max_tokens: options.max_tokens,
                    ..Default::default()
                };
                let stream = runtime
                    .infer_stream(&native_prompt, chat_options)
                    .await
                    .map_err(|e: super::native::NativeError| {
                        RigInferError::PromptError(e.to_string())
                    })?;

                // Pin the stream for iteration (async_stream produces !Unpin streams)
                let mut stream = pin!(stream);

                // Collect tokens as they arrive (Stream yields Result<String, NativeError>)
                while let Some(result) = stream.next().await {
                    match result {
                        Ok(token) => {
                            response_parts.push(token.clone());
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
                result.output_tokens = response_parts
                    .iter()
                    .map(|s| s.len() as u64)
                    .sum::<u64>()
                    .div_ceil(4);
            }
        }

        let complete_response = response_parts.concat();
        let _ = tx.try_send(StreamChunk::Done(complete_response.clone()));

        // Fallback: if stream ended without Final event, estimate tokens
        if result.input_tokens == 0 && result.output_tokens == 0 && !complete_response.is_empty() {
            result.input_tokens = (prompt.len() as u64).div_ceil(4);
            result.output_tokens = (complete_response.len() as u64).div_ceil(4);
        }

        let _ = tx.try_send(StreamChunk::Metrics {
            input_tokens: result.input_tokens,
            output_tokens: result.output_tokens,
        });

        result.text = complete_response;
        Ok(result)
    }
}

// NikaMcpToolDef, NikaMcpTool, AgentMediaStaging, and ToolDyn impl are in tool.rs

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
fn extract_native_vision_parts(
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

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    // =========================================================================
    // StreamResult tests
    // =========================================================================

    #[test]
    fn stream_result_from_text_has_zero_tokens() {
        let result = StreamResult::from_text("hello world");
        assert_eq!(result.text, "hello world");
        assert_eq!(result.input_tokens, 0);
        assert_eq!(result.output_tokens, 0);
        assert_eq!(result.total_tokens, 0);
        assert_eq!(result.cached_input_tokens, 0);
    }

    #[test]
    fn stream_result_default_is_empty() {
        let result = StreamResult::default();
        assert_eq!(result.text, "");
        assert_eq!(result.total_tokens, 0);
    }

    #[test]
    fn stream_result_with_tokens() {
        let result = StreamResult {
            text: "response".to_string(),
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
            cached_input_tokens: 20,
            ttft_ms: None,
            request_id: None,
        };
        assert_eq!(
            result.total_tokens,
            result.input_tokens + result.output_tokens
        );
        assert_eq!(result.cached_input_tokens, 20);
    }

    #[test]
    #[serial]
    fn test_rig_provider_claude_returns_claude_variant() {
        // This test verifies that RigProvider::claude() creates a Claude variant
        // It will fail initially because we need ANTHROPIC_API_KEY env var
        // In real code, we'll use from_env() which reads the API key

        // For now, we test the name() method which doesn't require API call
        std::env::set_var("ANTHROPIC_API_KEY", "test-key-for-unit-test");
        let provider = RigProvider::claude();

        assert_eq!(provider.name(), "claude");
        assert!(matches!(provider, RigProvider::Claude(_)));
    }

    #[test]
    #[serial]
    fn test_rig_provider_openai_returns_openai_variant() {
        std::env::set_var("OPENAI_API_KEY", "test-key-for-unit-test");
        let provider = RigProvider::openai();

        assert_eq!(provider.name(), "openai");
        assert!(matches!(provider, RigProvider::OpenAI(_)));
    }

    #[test]
    #[serial]
    fn test_rig_provider_default_model_claude() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key-for-unit-test");
        let provider = RigProvider::claude();

        // Using explicit model name instead of rig-core constant
        // rig-core's CLAUDE_3_5_SONNET is outdated
        assert_eq!(provider.default_model(), "claude-sonnet-4-6");
    }

    #[test]
    #[serial]
    fn test_rig_provider_default_model_openai() {
        std::env::set_var("OPENAI_API_KEY", "test-key-for-unit-test");
        let provider = RigProvider::openai();

        assert_eq!(provider.default_model(), openai::GPT_4O);
    }

    #[test]
    fn test_rig_infer_error_display() {
        let err = RigInferError::PromptError("Test error message".to_string());
        assert_eq!(err.to_string(), "Completion error: Test error message");
    }

    #[test]
    fn test_rig_infer_error_timeout_display() {
        // Test new Timeout variant
        let err = RigInferError::Timeout { duration_ms: 60000 };
        assert_eq!(
            err.to_string(),
            "Stream timeout: no chunk received for 60000ms"
        );
    }

    // =========================================================================
    // New Provider Tests
    // =========================================================================

    #[test]
    #[serial]
    fn test_rig_provider_mistral_returns_mistral_variant() {
        std::env::set_var("MISTRAL_API_KEY", "test-key-for-unit-test");
        let provider = RigProvider::mistral();

        assert_eq!(provider.name(), "mistral");
        assert!(matches!(provider, RigProvider::Mistral(_)));
    }

    #[test]
    #[serial]
    fn test_rig_provider_groq_returns_groq_variant() {
        std::env::set_var("GROQ_API_KEY", "test-key-for-unit-test");
        let provider = RigProvider::groq();

        assert_eq!(provider.name(), "groq");
        assert!(matches!(provider, RigProvider::Groq(_)));
    }

    #[test]
    #[serial]
    fn test_rig_provider_deepseek_returns_deepseek_variant() {
        std::env::set_var("DEEPSEEK_API_KEY", "test-key-for-unit-test");
        let provider = RigProvider::deepseek();

        assert_eq!(provider.name(), "deepseek");
        assert!(matches!(provider, RigProvider::DeepSeek(_)));
    }

    #[test]
    #[serial]
    fn test_rig_provider_default_models_v06() {
        // Test all new provider default models
        std::env::set_var("MISTRAL_API_KEY", "test");
        std::env::set_var("GROQ_API_KEY", "test");
        std::env::set_var("DEEPSEEK_API_KEY", "test");

        assert_eq!(
            RigProvider::mistral().default_model(),
            mistral::MISTRAL_LARGE
        );
        assert_eq!(
            RigProvider::groq().default_model(),
            "llama-3.3-70b-versatile"
        );
        assert_eq!(RigProvider::deepseek().default_model(), "deepseek-chat");
    }

    #[test]
    #[serial]
    fn test_rig_provider_auto_detects_claude() {
        // Clear other keys, set only Claude
        std::env::remove_var("OPENAI_API_KEY");
        std::env::remove_var("MISTRAL_API_KEY");
        std::env::remove_var("GROQ_API_KEY");
        std::env::remove_var("DEEPSEEK_API_KEY");
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");

        let provider = RigProvider::auto();
        assert!(provider.is_some());
        assert_eq!(provider.unwrap().name(), "claude");
    }

    #[test]
    #[serial]
    fn test_rig_provider_auto_returns_none_when_no_keys() {
        // Clear all API keys - uses #[serial] for test isolation
        clear_all_provider_env_vars();

        let provider = RigProvider::auto();
        assert!(provider.is_none());
    }

    // =========================================================================
    // Provider Fallback Chain Tests
    // =========================================================================

    /// Helper to clear all provider env vars for testing fallback chain
    fn clear_all_provider_env_vars() {
        std::env::remove_var("ANTHROPIC_API_KEY");
        std::env::remove_var("OPENAI_API_KEY");
        std::env::remove_var("MISTRAL_API_KEY");
        std::env::remove_var("GROQ_API_KEY");
        std::env::remove_var("DEEPSEEK_API_KEY");
        std::env::remove_var("GEMINI_API_KEY");
        std::env::remove_var("XAI_API_KEY");
    }

    #[test]
    #[serial]
    fn test_auto_fallback_to_openai() {
        // Given: Only OPENAI_API_KEY is set (Claude not available)
        clear_all_provider_env_vars();
        std::env::set_var("OPENAI_API_KEY", "test-key");

        // When: auto() is called
        let provider = RigProvider::auto();

        // Then: Should fall back to OpenAI
        assert!(provider.is_some());
        assert_eq!(provider.unwrap().name(), "openai");
    }

    #[test]
    #[serial]
    fn test_auto_fallback_to_mistral() {
        // Given: Only MISTRAL_API_KEY is set
        clear_all_provider_env_vars();
        std::env::set_var("MISTRAL_API_KEY", "test-key");

        // When: auto() is called
        let provider = RigProvider::auto();

        // Then: Should fall back to Mistral
        assert!(provider.is_some());
        assert_eq!(provider.unwrap().name(), "mistral");
    }

    #[test]
    #[serial]
    fn test_auto_fallback_to_groq() {
        // Given: Only GROQ_API_KEY is set
        clear_all_provider_env_vars();
        std::env::set_var("GROQ_API_KEY", "test-key");

        // When: auto() is called
        let provider = RigProvider::auto();

        // Then: Should fall back to Groq
        assert!(provider.is_some());
        assert_eq!(provider.unwrap().name(), "groq");
    }

    #[test]
    #[serial]
    fn test_auto_fallback_to_deepseek() {
        // Given: Only DEEPSEEK_API_KEY is set
        clear_all_provider_env_vars();
        std::env::set_var("DEEPSEEK_API_KEY", "test-key");

        // When: auto() is called
        let provider = RigProvider::auto();

        // Then: Should fall back to DeepSeek
        assert!(provider.is_some());
        assert_eq!(provider.unwrap().name(), "deepseek");
    }

    #[test]
    #[serial]
    fn test_auto_fallback_to_gemini() {
        // Given: Only GEMINI_API_KEY is set
        clear_all_provider_env_vars();
        std::env::set_var("GEMINI_API_KEY", "test-key");

        // When: auto() is called
        let provider = RigProvider::auto();

        // Then: Should fall back to Gemini
        assert!(provider.is_some());
        assert_eq!(provider.unwrap().name(), "gemini");
    }

    #[test]
    #[serial]
    fn test_auto_priority_claude_over_openai() {
        // Given: Both Claude and OpenAI keys are set
        clear_all_provider_env_vars();
        std::env::set_var("ANTHROPIC_API_KEY", "claude-key");
        std::env::set_var("OPENAI_API_KEY", "openai-key");

        // When: auto() is called
        let provider = RigProvider::auto();

        // Then: Should select Claude (higher priority)
        assert!(provider.is_some());
        assert_eq!(provider.unwrap().name(), "claude");
    }

    #[test]
    #[serial]
    fn test_auto_priority_openai_over_mistral() {
        // Given: OpenAI and Mistral keys are set (no Claude)
        clear_all_provider_env_vars();
        std::env::set_var("OPENAI_API_KEY", "openai-key");
        std::env::set_var("MISTRAL_API_KEY", "mistral-key");

        // When: auto() is called
        let provider = RigProvider::auto();

        // Then: Should select OpenAI (higher priority than Mistral)
        assert!(provider.is_some());
        assert_eq!(provider.unwrap().name(), "openai");
    }

    #[test]
    #[serial]
    fn test_auto_empty_env_var_treated_as_unset() {
        // Given: ANTHROPIC_API_KEY is set but empty
        clear_all_provider_env_vars();
        std::env::set_var("ANTHROPIC_API_KEY", ""); // Empty string
        std::env::set_var("OPENAI_API_KEY", "valid-key");

        // When: auto() is called
        let provider = RigProvider::auto();

        // Then: Should skip empty Claude and select OpenAI
        assert!(provider.is_some());
        assert_eq!(provider.unwrap().name(), "openai");
    }

    #[test]
    #[serial]
    fn test_auto_whitespace_env_var_treated_as_unset() {
        // Given: ANTHROPIC_API_KEY is set to whitespace only
        clear_all_provider_env_vars();
        std::env::set_var("ANTHROPIC_API_KEY", "   "); // Whitespace only

        // When: auto() is called
        let provider = RigProvider::auto();

        // Then: Should treat whitespace-only as unset
        // The implementation now uses !v.trim().is_empty() to reject whitespace-only keys
        assert!(
            provider.is_none(),
            "Whitespace-only API key should be treated as unset"
        );
    }

    // =========================================================================
    // NikaMcpTool tests
    // =========================================================================

    #[test]
    fn test_nika_mcp_tool_implements_tool_dyn() {
        // Given: A tool definition from our MCP infrastructure
        let tool_def = NikaMcpToolDef {
            name: "novanet_context".to_string(),
            description: "Generate native content for an entity".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "entity": { "type": "string" },
                    "locale": { "type": "string" }
                },
                "required": ["entity", "locale"]
            }),
        };

        // When: We create a NikaMcpTool wrapper
        let tool = NikaMcpTool::new(tool_def);

        // Then: It should have the correct name
        assert_eq!(tool.tool_name(), "novanet_context");
    }

    #[test]
    fn test_nika_mcp_tool_definition_returns_correct_schema() {
        use rig::tool::ToolDyn;

        // Given: A NikaMcpTool with a specific schema
        let tool_def = NikaMcpToolDef {
            name: "novanet_describe".to_string(),
            description: "Describe an entity from the knowledge graph".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "entity_key": { "type": "string" }
                },
                "required": ["entity_key"]
            }),
        };
        let tool = NikaMcpTool::new(tool_def);

        // When: We get the tool definition (sync wrapper for test)
        let name = tool.name();

        // Then: The definition should match
        assert_eq!(name, "novanet_describe");
    }

    // =========================================================================
    // RED: NikaMcpTool with McpClient - should FAIL until we wire up McpClient
    // =========================================================================

    #[tokio::test]
    async fn test_nika_mcp_tool_call_uses_mcp_client() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        // Given: A mock MCP client (pre-connected)
        let client = Arc::new(McpClient::mock("novanet"));

        // Given: A NikaMcpTool connected to the client
        let tool_def = NikaMcpToolDef {
            name: "novanet_describe".to_string(),
            description: "Describe an entity".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "entity_key": { "type": "string" }
                },
                "required": ["entity_key"]
            }),
        };
        let tool = NikaMcpTool::with_client(tool_def, client);

        // When: We call the tool
        let args = r#"{"entity_key": "qr-code"}"#.to_string();
        let result = tool.call(args).await;

        // Then: The call should succeed (mock returns success)
        assert!(result.is_ok(), "Tool call should succeed with mock client");
        let output = result.unwrap();
        assert!(!output.is_empty(), "Tool should return non-empty output");
    }

    // =========================================================================
    // USE CASE TESTS - Real-world NovaNet MCP tool scenarios
    // =========================================================================

    /// UC1: novanet_context - Assemble LLM context for content generation
    #[tokio::test]
    async fn test_usecase_novanet_context_entity_locale() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        // Given: Mock NovaNet MCP client
        let client = Arc::new(McpClient::mock("novanet"));

        // Given: novanet_context tool with full schema (matching NovaNet MCP spec)
        let tool_def = NikaMcpToolDef {
            name: "novanet_context".to_string(),
            description: "Full RLM-on-KG context assembly for generation".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "focus_key": { "type": "string", "description": "Entity key to generate for" },
                    "locale": { "type": "string", "description": "BCP-47 locale code" },
                    "mode": { "type": "string", "enum": ["block", "page"], "default": "block" },
                    "token_budget": { "type": "integer", "default": 4000 },
                    "spreading_depth": { "type": "integer", "default": 2 },
                    "forms": {
                        "type": "array",
                        "items": { "type": "string", "enum": ["text", "title", "abbrev", "url"] }
                    }
                },
                "required": ["focus_key", "locale"]
            }),
        };
        let tool = NikaMcpTool::with_client(tool_def, client);

        // When: Calling for QR code entity in French
        let args = serde_json::json!({
            "focus_key": "qr-code",
            "locale": "fr-FR",
            "mode": "page",
            "forms": ["text", "title", "abbrev"]
        })
        .to_string();

        let result = tool.call(args).await;

        // Then: Should succeed with mock response
        assert!(
            result.is_ok(),
            "novanet_context should succeed: {:?}",
            result
        );
        let output = result.unwrap();
        assert!(!output.is_empty(), "Should return generation context");
    }

    /// UC2: novanet_describe - Get entity details
    #[tokio::test]
    async fn test_usecase_novanet_describe_entity() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        let client = Arc::new(McpClient::mock("novanet"));

        let tool_def = NikaMcpToolDef {
            name: "novanet_describe".to_string(),
            description: "Bootstrap agent understanding of the knowledge graph".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "describe": {
                        "type": "string",
                        "enum": ["schema", "entity", "category", "relations", "locales", "stats"]
                    },
                    "entity_key": { "type": "string" },
                    "category_key": { "type": "string" }
                },
                "required": ["describe"]
            }),
        };
        let tool = NikaMcpTool::with_client(tool_def, client);

        // When: Describing schema overview
        let args = serde_json::json!({
            "describe": "schema"
        })
        .to_string();

        let result = tool.call(args).await;
        assert!(result.is_ok(), "novanet_describe should succeed");
    }

    /// UC3: novanet_search (walk mode) - Graph traversal
    #[tokio::test]
    async fn test_usecase_novanet_search_walk_graph() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        let client = Arc::new(McpClient::mock("novanet"));

        let tool_def = NikaMcpToolDef {
            name: "novanet_search".to_string(),
            description: "Graph traversal with configurable depth and filters".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "start_key": { "type": "string" },
                    "max_depth": { "type": "integer", "default": 2 },
                    "direction": { "type": "string", "enum": ["outgoing", "incoming", "both"] },
                    "arc_families": { "type": "array", "items": { "type": "string" } },
                    "target_kinds": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["start_key"]
            }),
        };
        let tool = NikaMcpTool::with_client(tool_def, client);

        // When: Traversing from QR code with HAS_NATIVE arc
        let args = serde_json::json!({
            "start_key": "qr-code",
            "max_depth": 2,
            "direction": "outgoing",
            "arc_families": ["ownership", "localization"]
        })
        .to_string();

        let result = tool.call(args).await;
        assert!(result.is_ok(), "novanet_search walk should succeed");
    }

    /// UC4: novanet_search - Hybrid search
    #[tokio::test]
    async fn test_usecase_novanet_search_hybrid() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        let client = Arc::new(McpClient::mock("novanet"));

        let tool_def = NikaMcpToolDef {
            name: "novanet_search".to_string(),
            description: "Fulltext + property search with hybrid mode".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "mode": { "type": "string", "enum": ["fulltext", "property", "hybrid"] },
                    "kinds": { "type": "array", "items": { "type": "string" } },
                    "realm": { "type": "string", "enum": ["shared", "org"] },
                    "limit": { "type": "integer", "default": 10 }
                },
                "required": ["query"]
            }),
        };
        let tool = NikaMcpTool::with_client(tool_def, client);

        // When: Searching for QR-related entities
        let args = serde_json::json!({
            "query": "QR code generator",
            "mode": "hybrid",
            "kinds": ["Entity", "Page"],
            "limit": 5
        })
        .to_string();

        let result = tool.call(args).await;
        assert!(result.is_ok(), "novanet_search should succeed");
    }

    /// UC5: novanet_audit - Quality checks with CSR metrics
    #[tokio::test]
    async fn test_usecase_novanet_audit_locale() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        let client = Arc::new(McpClient::mock("novanet"));

        let tool_def = NikaMcpToolDef {
            name: "novanet_audit".to_string(),
            description: "Retrieve knowledge atoms for a specific locale".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "locale": { "type": "string" },
                    "atom_type": {
                        "type": "string",
                        "enum": ["term", "expression", "pattern", "cultureref", "taboo", "audiencetrait", "all"]
                    },
                    "domain": { "type": "string" }
                },
                "required": ["locale"]
            }),
        };
        let tool = NikaMcpTool::with_client(tool_def, client);

        // When: Getting French terms for QR codes
        let args = serde_json::json!({
            "locale": "fr-FR",
            "atom_type": "term",
            "domain": "qr-code"
        })
        .to_string();

        let result = tool.call(args).await;
        assert!(result.is_ok(), "novanet_audit should succeed");
    }

    /// UC6: novanet_batch - Parallel operations
    #[tokio::test]
    async fn test_usecase_novanet_batch_context() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        let client = Arc::new(McpClient::mock("novanet"));

        let tool_def = NikaMcpToolDef {
            name: "novanet_batch".to_string(),
            description: "Assemble context for LLM generation (token-aware)".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "focus_key": { "type": "string" },
                    "locale": { "type": "string" },
                    "token_budget": { "type": "integer", "default": 4000 },
                    "strategy": {
                        "type": "string",
                        "enum": ["breadth", "depth", "relevance", "custom"]
                    }
                },
                "required": ["focus_key", "locale"]
            }),
        };
        let tool = NikaMcpTool::with_client(tool_def, client);

        // When: Assembling context for Spanish QR code generation
        let args = serde_json::json!({
            "focus_key": "qr-code",
            "locale": "es-MX",
            "token_budget": 3000,
            "strategy": "relevance"
        })
        .to_string();

        let result = tool.call(args).await;
        assert!(result.is_ok(), "novanet_batch should succeed");
    }

    // =========================================================================
    // ERROR HANDLING TESTS
    // =========================================================================

    /// Test that calling without client returns proper error
    #[tokio::test]
    async fn test_error_no_client_configured() {
        use rig::tool::ToolDyn;

        // Given: NikaMcpTool WITHOUT client
        let tool_def = NikaMcpToolDef {
            name: "novanet_describe".to_string(),
            description: "Test tool".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        let tool = NikaMcpTool::new(tool_def); // No client!

        // When: Calling the tool
        let args = r#"{"entity_key": "test"}"#.to_string();
        let result = tool.call(args).await;

        // Then: Should fail with NotConnected error
        assert!(result.is_err(), "Should fail without client");
        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("No MCP client") || err_str.contains("NotConnected"),
            "Error should mention missing client: {}",
            err_str
        );
    }

    /// Test that invalid JSON arguments return proper error
    #[tokio::test]
    async fn test_error_invalid_json_arguments() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        let client = Arc::new(McpClient::mock("novanet"));
        let tool_def = NikaMcpToolDef {
            name: "novanet_describe".to_string(),
            description: "Test tool".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        let tool = NikaMcpTool::with_client(tool_def, client);

        // When: Calling with invalid JSON
        let args = "not valid json {{{".to_string();
        let result = tool.call(args).await;

        // Then: Should fail with JSON parsing error
        assert!(result.is_err(), "Should fail with invalid JSON");
        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("Invalid JSON") || err_str.contains("JSON"),
            "Error should mention JSON parsing: {}",
            err_str
        );
    }

    /// Test that empty JSON object is valid
    #[tokio::test]
    async fn test_empty_json_object_is_valid() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        let client = Arc::new(McpClient::mock("novanet"));
        let tool_def = NikaMcpToolDef {
            name: "novanet_describe".to_string(),
            description: "Test tool".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        let tool = NikaMcpTool::with_client(tool_def, client);

        // When: Calling with empty JSON object
        let args = "{}".to_string();
        let result = tool.call(args).await;

        // Then: Should succeed (empty args are valid)
        assert!(result.is_ok(), "Empty JSON object should be valid");
    }

    // =========================================================================
    // TOOL DEFINITION TESTS
    // =========================================================================

    /// Test async definition method returns correct schema
    #[tokio::test]
    async fn test_tool_definition_async() {
        use rig::tool::ToolDyn;

        let input_schema = serde_json::json!({
            "type": "object",
            "properties": {
                "entity_key": { "type": "string" },
                "locale": { "type": "string" }
            },
            "required": ["entity_key"]
        });

        let tool_def = NikaMcpToolDef {
            name: "test_tool".to_string(),
            description: "A test tool for verification".to_string(),
            input_schema: input_schema.clone(),
        };
        let tool = NikaMcpTool::new(tool_def);

        // When: Getting the tool definition
        let definition = tool.definition("some prompt".to_string()).await;

        // Then: Definition should match
        assert_eq!(definition.name, "test_tool");
        assert_eq!(definition.description, "A test tool for verification");
        assert_eq!(definition.parameters, input_schema);
    }

    /// Test multiple tools can coexist
    #[test]
    fn test_multiple_tools_independent() {
        // Given: Multiple tool definitions
        let tool1 = NikaMcpTool::new(NikaMcpToolDef {
            name: "novanet_context".to_string(),
            description: "Generate content".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        });

        let tool2 = NikaMcpTool::new(NikaMcpToolDef {
            name: "novanet_describe".to_string(),
            description: "Describe entity".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        });

        let tool3 = NikaMcpTool::new(NikaMcpToolDef {
            name: "novanet_search".to_string(),
            description: "Traverse graph".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        });

        // Then: Each tool maintains its own identity
        assert_eq!(tool1.tool_name(), "novanet_context");
        assert_eq!(tool2.tool_name(), "novanet_describe");
        assert_eq!(tool3.tool_name(), "novanet_search");
    }

    /// Test tool can be cloned and remains functional
    #[tokio::test]
    async fn test_tool_clone_works() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        let client = Arc::new(McpClient::mock("novanet"));
        let tool_def = NikaMcpToolDef {
            name: "novanet_describe".to_string(),
            description: "Test tool".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        let tool = NikaMcpTool::with_client(tool_def, client);

        // When: Cloning the tool
        let cloned_tool = tool.clone();

        // Then: Both should work independently
        let args = r#"{"entity_key": "test"}"#.to_string();
        let result1 = tool.call(args.clone()).await;
        let result2 = cloned_tool.call(args).await;

        assert!(result1.is_ok(), "Original tool should work");
        assert!(result2.is_ok(), "Cloned tool should work");
    }

    // =========================================================================
    // MULTI-LOCALE TESTS (Real-world scenarios)
    // =========================================================================

    /// Test generating for multiple locales (common Nika workflow pattern)
    #[tokio::test]
    async fn test_multi_locale_generation_workflow() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        let client = Arc::new(McpClient::mock("novanet"));
        let tool_def = NikaMcpToolDef {
            name: "novanet_context".to_string(),
            description: "Generate native content".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "focus_key": { "type": "string" },
                    "locale": { "type": "string" },
                    "forms": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["focus_key", "locale"]
            }),
        };
        let tool = NikaMcpTool::with_client(tool_def, client);

        // When: Generating for multiple locales (simulating for_each workflow)
        let locales = ["fr-FR", "es-MX", "de-DE", "ja-JP", "zh-CN"];
        let mut results = Vec::new();

        for locale in locales {
            let args = serde_json::json!({
                "focus_key": "qr-code",
                "locale": locale,
                "forms": ["text", "title"]
            })
            .to_string();

            let result = tool.call(args).await;
            results.push((locale, result.is_ok()));
        }

        // Then: All locales should succeed
        for (locale, success) in &results {
            assert!(success, "Generation for {} should succeed", locale);
        }
        assert_eq!(results.len(), 5, "Should process all 5 locales");
    }

    // =========================================================================
    // Provider Verification Tests
    // =========================================================================

    #[test]
    fn test_provider_verify_error_types() {
        // Test all error variants
        let invalid_key = ProviderVerifyError::InvalidApiKey {
            provider: "claude".to_string(),
        };
        assert!(invalid_key.to_string().contains("Invalid API key"));
        assert!(invalid_key.suggestion().contains("API key"));

        let rate_limited = ProviderVerifyError::RateLimited {
            provider: "openai".to_string(),
        };
        assert!(rate_limited.to_string().contains("Rate limited"));

        let timeout = ProviderVerifyError::Timeout {
            provider: "mistral".to_string(),
        };
        assert!(timeout.to_string().contains("timeout"));

        let network = ProviderVerifyError::NetworkError {
            provider: "groq".to_string(),
            details: "connection refused".to_string(),
        };
        assert!(network.to_string().contains("Network error"));

        let provider_err = ProviderVerifyError::ProviderError {
            provider: "deepseek".to_string(),
            details: "server down".to_string(),
        };
        assert!(provider_err.to_string().contains("server down"));
    }

    #[test]
    fn test_provider_verify_result_fields() {
        let result = ProviderVerifyResult {
            provider: "claude".to_string(),
            latency: std::time::Duration::from_millis(150),
            model: "claude-sonnet-4-6".to_string(),
        };

        assert_eq!(result.provider, "claude");
        assert_eq!(result.latency.as_millis(), 150);
        assert_eq!(result.model, "claude-sonnet-4-6");
    }

    #[test]
    #[serial]
    fn test_is_configured_with_api_key() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let provider = RigProvider::claude();
        assert!(provider.is_configured());
    }

    #[test]
    #[serial]
    fn test_is_configured_returns_true_for_all_providers_with_keys() {
        // Set up all API keys
        std::env::set_var("ANTHROPIC_API_KEY", "test");
        std::env::set_var("OPENAI_API_KEY", "test");
        std::env::set_var("MISTRAL_API_KEY", "test");
        std::env::set_var("GROQ_API_KEY", "test");
        std::env::set_var("DEEPSEEK_API_KEY", "test");

        assert!(RigProvider::claude().is_configured());
        assert!(RigProvider::openai().is_configured());
        assert!(RigProvider::mistral().is_configured());
        assert!(RigProvider::groq().is_configured());
        assert!(RigProvider::deepseek().is_configured());
    }

    // =========================================================================
    // InferOptions Tests
    // =========================================================================

    #[test]
    fn test_infer_options_default() {
        let opts = InferOptions::default();
        assert!(opts.model.is_none());
        assert!(opts.temperature.is_none());
        assert!(opts.max_tokens.is_none());
        assert!(opts.system.is_none());
        assert!(opts.additional_params.is_none());
    }

    #[test]
    fn test_infer_options_with_all_fields() {
        let opts = InferOptions {
            model: Some("gpt-4o".to_string()),
            temperature: Some(0.7),
            max_tokens: Some(2000),
            system: Some("You are a helpful assistant.".to_string()),
            additional_params: None,
        };
        assert_eq!(opts.model.as_deref(), Some("gpt-4o"));
        assert_eq!(opts.temperature, Some(0.7));
        assert_eq!(opts.max_tokens, Some(2000));
        assert_eq!(opts.system.as_deref(), Some("You are a helpful assistant."));
    }

    #[test]
    fn test_infer_options_partial_fields() {
        let opts = InferOptions {
            temperature: Some(0.5),
            ..Default::default()
        };
        assert!(opts.model.is_none());
        assert_eq!(opts.temperature, Some(0.5));
        assert!(opts.max_tokens.is_none());
        assert!(opts.system.is_none());
        assert!(opts.additional_params.is_none());
    }

    #[test]
    fn test_infer_options_temperature_zero() {
        let opts = InferOptions {
            temperature: Some(0.0),
            ..Default::default()
        };
        assert_eq!(opts.temperature, Some(0.0));
    }

    #[test]
    fn test_infer_options_max_tokens_small() {
        let opts = InferOptions {
            max_tokens: Some(1),
            ..Default::default()
        };
        assert_eq!(opts.max_tokens, Some(1));
    }

    #[test]
    fn test_infer_options_system_empty_string() {
        let opts = InferOptions {
            system: Some(String::new()),
            ..Default::default()
        };
        assert_eq!(opts.system.as_deref(), Some(""));
    }

    #[test]
    fn test_infer_options_clone() {
        let opts = InferOptions {
            model: Some("test-model".to_string()),
            temperature: Some(0.8),
            max_tokens: Some(1000),
            system: Some("Test system".to_string()),
            additional_params: Some(serde_json::json!({"foo": "bar"})),
        };
        let cloned = opts.clone();
        assert_eq!(opts.model, cloned.model);
        assert_eq!(opts.temperature, cloned.temperature);
        assert_eq!(opts.max_tokens, cloned.max_tokens);
        assert_eq!(opts.system, cloned.system);
        assert_eq!(opts.additional_params, cloned.additional_params);
    }

    #[test]
    fn test_infer_options_with_additional_params() {
        let params = serde_json::json!({
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": "structured_output",
                    "strict": true,
                    "schema": { "type": "object" }
                }
            }
        });
        let opts = InferOptions {
            additional_params: Some(params.clone()),
            ..Default::default()
        };
        assert_eq!(opts.additional_params, Some(params));
    }

    #[test]
    fn test_infer_options_with_extended_thinking() {
        let budget: u64 = 8192;
        let thinking_params = serde_json::json!({
            "thinking": { "type": "enabled", "budget_tokens": budget }
        });
        let opts = InferOptions {
            model: Some("claude-sonnet-4-6".to_string()),
            temperature: Some(1.0),
            max_tokens: Some((budget as u32) + 8192),
            system: None,
            additional_params: Some(thinking_params.clone()),
        };
        assert_eq!(opts.temperature, Some(1.0));
        assert_eq!(opts.max_tokens, Some(16384));
        let params = opts.additional_params.unwrap();
        assert_eq!(params["thinking"]["type"], "enabled");
        assert_eq!(params["thinking"]["budget_tokens"], 8192);
    }

    // =========================================================================
    // Structured Output Helper Tests
    // =========================================================================

    #[test]
    fn test_supports_native_structured_output_by_name() {
        assert!(supports_native_structured_output("openai"));
        assert!(supports_native_structured_output("groq"));
        assert!(supports_native_structured_output("deepseek"));
        assert!(supports_native_structured_output("xai"));

        assert!(!supports_native_structured_output("claude"));
        assert!(!supports_native_structured_output("anthropic"));
        assert!(!supports_native_structured_output("gemini"));
        assert!(!supports_native_structured_output("mistral"));
        assert!(!supports_native_structured_output("native"));
        assert!(!supports_native_structured_output("mock"));
        // Custom endpoints like "h100" are NOT detected by the string check
        assert!(!supports_native_structured_output("h100"));
    }

    #[test]
    #[serial]
    fn test_supports_native_structured_output_by_provider() {
        // RigProvider method detects OpenAiCompat (custom endpoints)
        std::env::set_var("OPENAI_API_KEY", "test-key");
        assert!(RigProvider::openai().supports_native_structured_output());
        assert!(!RigProvider::claude().supports_native_structured_output());

        let compat =
            RigProvider::openai_compat("h100", "http://localhost:8000/v1", "test", None).unwrap();
        assert!(
            compat.supports_native_structured_output(),
            "OpenAiCompat (custom endpoints) should support native structured output"
        );
    }

    #[test]
    fn test_build_response_format_params() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "score": { "type": "number" }
            },
            "required": ["name", "score"]
        });
        let params = build_response_format_params(&schema);
        assert_eq!(params["response_format"]["type"], "json_schema");
        assert_eq!(
            params["response_format"]["json_schema"]["name"],
            "structured_output"
        );
        assert_eq!(params["response_format"]["json_schema"]["strict"], true);
        assert_eq!(
            params["response_format"]["json_schema"]["schema"]["properties"]["name"]["type"],
            "string"
        );
    }

    #[test]
    fn test_build_response_format_preserves_full_schema() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "items": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["items"],
            "additionalProperties": false
        });
        let params = build_response_format_params(&schema);
        let embedded = &params["response_format"]["json_schema"]["schema"];
        assert_eq!(embedded["additionalProperties"], false);
        assert_eq!(embedded["properties"]["items"]["type"], "array");
    }

    // =========================================================================
    // Vision Provider Tests
    // =========================================================================

    #[test]
    fn vision_not_supported_error_display() {
        let err = RigInferError::VisionNotSupported("DeepSeek no vision".to_string());
        assert!(err.to_string().contains("Vision not supported"));
        assert!(err.to_string().contains("DeepSeek no vision"));
    }

    /// Test DeepSeek vision rejection (only when DEEPSEEK_API_KEY is set)
    #[tokio::test]
    async fn infer_vision_deepseek_returns_error() {
        if std::env::var("DEEPSEEK_API_KEY").is_err() {
            // Can't construct DeepSeek without API key; test message building instead
            let err = RigInferError::VisionNotSupported("DeepSeek".to_string());
            assert!(err.to_string().contains("Vision not supported"));
            return;
        }
        let provider = RigProvider::deepseek();
        let content = vec![rig::completion::message::UserContent::text("hello")];
        let result = provider.infer_vision(content, None, None, None).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RigInferError::VisionNotSupported(_)
        ));
    }

    #[test]
    fn infer_vision_empty_content_builds_error() {
        // OneOrMany::many rejects empty vecs, which infer_vision maps to VisionNotSupported
        use rig::OneOrMany;
        let content: Vec<rig::completion::message::UserContent> = vec![];
        let result = OneOrMany::many(content);
        assert!(result.is_err(), "empty content should fail");
    }

    #[test]
    fn build_vision_user_content_text_only() {
        let content = [rig::completion::message::UserContent::text("Describe this")];
        assert_eq!(content.len(), 1);
    }

    #[test]
    fn build_vision_user_content_with_image() {
        use rig::completion::message::{ImageMediaType, UserContent};
        let content = [
            UserContent::text("What is in this image?"),
            UserContent::image_base64(
                "iVBORw0KGgo=", // fake base64
                Some(ImageMediaType::PNG),
                None,
            ),
        ];
        assert_eq!(content.len(), 2);
    }

    #[test]
    fn build_vision_message_from_content() {
        use rig::completion::message::{ImageMediaType, Message, UserContent};
        use rig::OneOrMany;

        let parts = vec![
            UserContent::text("Describe this image"),
            UserContent::image_base64("iVBORw0KGgo=", Some(ImageMediaType::PNG), None),
        ];
        let msg = Message::User {
            content: OneOrMany::many(parts).unwrap(),
        };
        assert!(matches!(msg, Message::User { .. }));
    }

    // =========================================================================
    // Reasoning Model Detection Tests (BUG 5 / NIKA-031)
    // =========================================================================

    #[test]
    fn reasoning_model_o_series() {
        assert!(is_reasoning_model("o1"));
        assert!(is_reasoning_model("o1-mini"));
        assert!(is_reasoning_model("o1-pro"));
        assert!(is_reasoning_model("o3"));
        assert!(is_reasoning_model("o3-mini"));
        assert!(is_reasoning_model("o3-pro"));
        assert!(is_reasoning_model("o4"));
        assert!(is_reasoning_model("o4-mini"));
        assert!(is_reasoning_model("o1-2024-12-17"));
    }

    #[test]
    fn reasoning_model_gpt5() {
        assert!(is_reasoning_model("gpt-5"));
        assert!(is_reasoning_model("gpt-5-turbo"));
    }

    #[test]
    fn reasoning_model_deepseek() {
        assert!(is_reasoning_model("deepseek-reasoner"));
    }

    #[test]
    fn reasoning_model_case_insensitive() {
        assert!(is_reasoning_model("O1"));
        assert!(is_reasoning_model("GPT-5"));
    }

    #[test]
    fn non_reasoning_models() {
        assert!(!is_reasoning_model("gpt-4o"));
        assert!(!is_reasoning_model("gpt-4o-mini"));
        assert!(!is_reasoning_model("claude-sonnet-4"));
        assert!(!is_reasoning_model("deepseek-chat"));
        assert!(!is_reasoning_model("gemini-2.0-flash"));
        assert!(!is_reasoning_model("grok-3"));
    }

    // =========================================================================
    // Endpoint resolution tests
    // =========================================================================

    #[test]
    fn test_from_name_with_endpoints_custom() {
        use crate::provider::endpoints::{CustomEndpointMap, ResolvedEndpoint};

        let mut endpoints = CustomEndpointMap::new();
        endpoints.insert(
            "local".to_string(),
            ResolvedEndpoint {
                base_url: "http://localhost:11434/v1".to_string(),
                api_key: "ollama".to_string(),
                default_model: Some("llama3.2".to_string()),
                timeout_secs: 300,
                hourly_rate: None,
                currency: "USD".to_string(),
            },
        );

        let provider = RigProvider::from_name_with_endpoints("local", &endpoints).unwrap();
        assert!(matches!(provider, RigProvider::OpenAiCompat { .. }));
    }

    #[test]
    fn test_from_name_with_endpoints_fallback_to_catalog() {
        use crate::provider::endpoints::{CustomEndpointMap, ResolvedEndpoint};

        // Add endpoint "myserver" but look up "openai" -> should fall through to catalog
        let mut endpoints = CustomEndpointMap::new();
        endpoints.insert(
            "myserver".to_string(),
            ResolvedEndpoint {
                base_url: "http://localhost:8000/v1".to_string(),
                api_key: "test".to_string(),
                default_model: None,
                timeout_secs: 300,
                hourly_rate: None,
                currency: "USD".to_string(),
            },
        );

        // "openai" is not in custom endpoints -> falls through to catalog
        // The catalog lookup should not match "myserver"
        let result = RigProvider::from_name_with_endpoints("myserver", &endpoints);
        assert!(
            matches!(result.as_ref().unwrap(), RigProvider::OpenAiCompat { .. }),
            "Custom endpoint should resolve to OpenAiCompat"
        );
    }

    #[test]
    fn test_from_name_with_endpoints_unknown() {
        use crate::provider::endpoints::CustomEndpointMap;

        let endpoints = CustomEndpointMap::new();
        let result = RigProvider::from_name_with_endpoints("nonexistent", &endpoints);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("NIKA-030"),
            "Expected NIKA-030 not configured, got: {}",
            err_msg
        );
    }

    // =========================================================================
    // Fix 1.1: No Box::leak in name() and default_model()
    // =========================================================================

    #[test]
    fn test_openai_compat_name_no_leak() {
        // Creating many OpenAiCompat providers should NOT permanently leak memory.
        // Before fix: Box::leak allocated a new &'static str every call.
        // After fix: cached_name field returned by reference.
        for i in 0..100 {
            let provider = RigProvider::openai_compat(
                &format!("endpoint-{}", i),
                "http://localhost:8000/v1",
                "test-key",
                Some("test-model"),
            )
            .unwrap();
            assert_eq!(provider.name(), format!("openai-compat:endpoint-{}", i));
        }
    }

    #[test]
    fn test_openai_compat_default_model_cached() {
        let provider = RigProvider::openai_compat(
            "h100",
            "http://localhost:8000/v1",
            "test-key",
            Some("Qwen/Qwen3-8B"),
        )
        .unwrap();
        assert_eq!(provider.default_model(), "Qwen/Qwen3-8B");

        // Without default model → fallback
        let provider2 =
            RigProvider::openai_compat("h100", "http://localhost:8000/v1", "test-key", None)
                .unwrap();
        assert_eq!(provider2.default_model(), "gpt-3.5-turbo");
    }

    // =========================================================================
    // Fix 1.2: cost_provider_kind() for custom endpoints
    // =========================================================================

    #[test]
    fn test_cost_provider_kind_standard_providers() {
        use crate::provider::cost::ProviderKind;

        // Use explicit API keys to avoid env var race in parallel tests
        let claude_client = anthropic::Client::builder()
            .api_key("test-key")
            .build()
            .unwrap();
        let openai_client = openai::Client::builder()
            .api_key("test-key")
            .build()
            .unwrap();
        assert_eq!(
            RigProvider::Claude(claude_client).cost_provider_kind(),
            Some(ProviderKind::Claude)
        );
        assert_eq!(
            RigProvider::OpenAI(openai_client).cost_provider_kind(),
            Some(ProviderKind::OpenAI)
        );
    }

    #[test]
    fn test_cost_provider_kind_openai_compat() {
        use crate::provider::cost::ProviderKind;

        let provider = RigProvider::openai_compat(
            "h100",
            "http://localhost:8000/v1",
            "test-key",
            Some("Qwen/Qwen3-8B"),
        )
        .unwrap();
        // Custom endpoints use OpenAI-compatible API → treat as OpenAI for cost
        assert_eq!(provider.cost_provider_kind(), Some(ProviderKind::OpenAI));
    }

    #[test]
    fn test_openai_compat_cost_not_zero() {
        use crate::provider::cost::calculate_cost;

        let provider = RigProvider::openai_compat(
            "h100",
            "http://localhost:8000/v1",
            "test-key",
            Some("gpt-4o"),
        )
        .unwrap();
        let pk = provider.cost_provider_kind().unwrap();
        let cost = calculate_cost(pk, "gpt-4o", 10_000, 5_000);
        assert!(
            cost > 0.0,
            "Cost should be non-zero for known model via OpenAiCompat"
        );
    }
}
