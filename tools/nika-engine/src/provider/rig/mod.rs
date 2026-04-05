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
pub use error::{
    McpToolError, McpToolErrorKind, ProviderVerifyError, ProviderVerifyResult, RigInferError,
};
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

/// Dispatch to the rig-core client for all standard providers.
///
/// Reduces 7+ identical match arms to 1. The body expression gets the
/// extracted client binding `$client`. Mock and Native are NOT dispatched
/// through this macro — they have custom paths.
///
/// IMPORTANT: the `$body` expression may be `.await`-ed inside the macro.
/// Rust macros expand before type checking, so each arm gets its own
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
        if provider.requires_key && !crate::secrets::has_provider_key(provider) {
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
            "mock" => Ok(Self::Mock),
            #[cfg(feature = "native-inference")]
            "native" => Ok(Self::native()),
            // OpenAI-compatible providers — zero rig-core code, config-driven
            "openrouter" => {
                let key =
                    crate::secrets::store::resolve_env("OPENROUTER_API_KEY").ok_or_else(|| {
                        ProviderError::MissingApiKey {
                            provider: "openrouter".into(),
                        }
                    })?;
                Self::openai_compat(
                    "openrouter",
                    "https://openrouter.ai/api/v1",
                    &key,
                    None,
                    300,
                )
            }
            "together" => {
                let key =
                    crate::secrets::store::resolve_env("TOGETHER_API_KEY").ok_or_else(|| {
                        ProviderError::MissingApiKey {
                            provider: "together".into(),
                        }
                    })?;
                Self::openai_compat("together", "https://api.together.xyz/v1", &key, None, 300)
            }
            "fireworks" => {
                let key =
                    crate::secrets::store::resolve_env("FIREWORKS_API_KEY").ok_or_else(|| {
                        ProviderError::MissingApiKey {
                            provider: "fireworks".into(),
                        }
                    })?;
                Self::openai_compat(
                    "fireworks",
                    "https://api.fireworks.ai/inference/v1",
                    &key,
                    None,
                    300,
                )
            }
            "cerebras" => {
                let key =
                    crate::secrets::store::resolve_env("CEREBRAS_API_KEY").ok_or_else(|| {
                        ProviderError::MissingApiKey {
                            provider: "cerebras".into(),
                        }
                    })?;
                Self::openai_compat("cerebras", "https://api.cerebras.ai/v1", &key, None, 300)
            }
            "sambanova" => {
                let key =
                    crate::secrets::store::resolve_env("SAMBANOVA_API_KEY").ok_or_else(|| {
                        ProviderError::MissingApiKey {
                            provider: "sambanova".into(),
                        }
                    })?;
                Self::openai_compat("sambanova", "https://api.sambanova.ai/v1", &key, None, 300)
            }
            "cohere" => {
                let key =
                    crate::secrets::store::resolve_env("COHERE_API_KEY").ok_or_else(|| {
                        ProviderError::MissingApiKey {
                            provider: "cohere".into(),
                        }
                    })?;
                Self::openai_compat(
                    "cohere",
                    "https://api.cohere.com/compatibility/v1",
                    &key,
                    None,
                    300,
                )
            }
            "ai21" => {
                let key = crate::secrets::store::resolve_env("AI21_API_KEY").ok_or_else(|| {
                    ProviderError::MissingApiKey {
                        provider: "ai21".into(),
                    }
                })?;
                Self::openai_compat("ai21", "https://api.ai21.com/studio/v1", &key, None, 300)
            }
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
                ep.timeout_secs,
            );
        }

        // 2. Fall back to catalog provider
        Self::from_name(name)
    }

    /// Create a RigProvider by name with an explicit API key.
    ///
    /// Avoids `unsafe { std::env::set_var() }` — constructs the rig-core client
    /// directly with the provided key instead of reading from the environment.
    pub fn from_name_with_key(name: &str, api_key: &str) -> Result<Self, crate::error::NikaError> {
        let provider = crate::core::find_provider(name).ok_or(ProviderError::NotConfigured {
            provider: name.to_string(),
        })?;

        match provider.id {
            "anthropic" => anthropic::Client::new(api_key)
                .map(RigProvider::Claude)
                .map_err(|e| {
                    ProviderError::InvalidConfig {
                        message: format!("failed to build anthropic client: {e}"),
                    }
                    .into()
                }),
            "openai" => openai::Client::new(api_key)
                .map(RigProvider::OpenAI)
                .map_err(|e| {
                    ProviderError::InvalidConfig {
                        message: format!("failed to build openai client: {e}"),
                    }
                    .into()
                }),
            "mistral" => mistral::Client::new(api_key)
                .map(RigProvider::Mistral)
                .map_err(|e| {
                    ProviderError::InvalidConfig {
                        message: format!("failed to build mistral client: {e}"),
                    }
                    .into()
                }),
            "groq" => groq::Client::new(api_key)
                .map(RigProvider::Groq)
                .map_err(|e| {
                    ProviderError::InvalidConfig {
                        message: format!("failed to build groq client: {e}"),
                    }
                    .into()
                }),
            "deepseek" => deepseek::Client::new(api_key)
                .map(RigProvider::DeepSeek)
                .map_err(|e| {
                    ProviderError::InvalidConfig {
                        message: format!("failed to build deepseek client: {e}"),
                    }
                    .into()
                }),
            "gemini" => gemini::Client::new(api_key)
                .map(RigProvider::Gemini)
                .map_err(|e| {
                    ProviderError::InvalidConfig {
                        message: format!("failed to build gemini client: {e}"),
                    }
                    .into()
                }),
            "xai" => xai::Client::new(api_key)
                .map(RigProvider::XAi)
                .map_err(|e| {
                    ProviderError::InvalidConfig {
                        message: format!("failed to build xai client: {e}"),
                    }
                    .into()
                }),
            // OpenAI-compatible providers
            "openrouter" => Self::openai_compat(
                "openrouter",
                "https://openrouter.ai/api/v1",
                api_key,
                None,
                300,
            ),
            "together" => Self::openai_compat(
                "together",
                "https://api.together.xyz/v1",
                api_key,
                None,
                300,
            ),
            "fireworks" => Self::openai_compat(
                "fireworks",
                "https://api.fireworks.ai/inference/v1",
                api_key,
                None,
                300,
            ),
            "cerebras" => {
                Self::openai_compat("cerebras", "https://api.cerebras.ai/v1", api_key, None, 300)
            }
            "sambanova" => Self::openai_compat(
                "sambanova",
                "https://api.sambanova.ai/v1",
                api_key,
                None,
                300,
            ),
            "cohere" => Self::openai_compat(
                "cohere",
                "https://api.cohere.com/compatibility/v1",
                api_key,
                None,
                300,
            ),
            "ai21" => {
                Self::openai_compat("ai21", "https://api.ai21.com/studio/v1", api_key, None, 300)
            }
            _ => Err(ProviderError::NotConfigured {
                provider: name.to_string(),
            }
            .into()),
        }
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
        timeout_secs: u64,
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
            timeout_secs,
            raw_base_url: base_url.to_string(),
            raw_api_key: api_key.to_string(),
            http_client: reqwest::Client::new(),
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
            RigProvider::Mock => None,
            #[cfg(feature = "native-inference")]
            RigProvider::Native(_) => Some(ProviderKind::Native),
        }
    }

    /// Get the provider name
    pub fn name(&self) -> &str {
        match self {
            RigProvider::Claude(_) => "anthropic",
            RigProvider::OpenAI(_) => "openai",
            RigProvider::Mistral(_) => "mistral",
            RigProvider::Groq(_) => "groq",
            RigProvider::DeepSeek(_) => "deepseek",
            RigProvider::Gemini(_) => "gemini",
            RigProvider::XAi(_) => "xai",
            RigProvider::OpenAiCompat { cached_name, .. } => cached_name,
            RigProvider::Mock => "mock",
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
        // Custom endpoints have their own default model, not from the global catalog
        if let RigProvider::OpenAiCompat { default_model, .. } = self {
            return default_model.as_deref().unwrap_or("gpt-3.5-turbo");
        }
        // Delegate to single source of truth (nika-core ModelResolver catalog)
        nika_core::catalogs::default_model_for_provider(self.name()).unwrap_or("claude-sonnet-4-6")
    }

    /// Shared low-level POST to /chat/completions for OpenAI-compatible endpoints.
    ///
    /// Returns the parsed JSON response body + token usage. Both `raw_openai_compat_infer`
    /// and `infer_with_tools` (OpenAiCompat arm) delegate here, eliminating HTTP code
    /// duplication and ensuring token tracking works in both paths.
    async fn raw_chat_completion(
        http_client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        body: serde_json::Value,
        timeout: std::time::Duration,
    ) -> Result<(serde_json::Value, u64, u64), RigInferError> {
        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

        let mut req = http_client.post(&url).json(&body).timeout(timeout);
        if !api_key.is_empty() {
            req = req.bearer_auth(api_key);
        }

        let resp = req.send().await.map_err(|e| {
            if e.is_timeout() {
                RigInferError::Timeout {
                    duration_ms: timeout.as_millis() as u64,
                }
            } else {
                RigInferError::PromptError(format!("HTTP error: {e}"))
            }
        })?;

        let status = resp.status();
        let body_text = resp.text().await.map_err(|e| {
            RigInferError::PromptError(format!("failed to read response body: {e}"))
        })?;

        if !status.is_success() {
            // H2: Truncate error body to avoid leaking internal infra details
            let truncated = if body_text.len() > 500 {
                format!("{}...(truncated)", &body_text[..500])
            } else {
                body_text.clone()
            };
            return Err(RigInferError::PromptError(format!(
                "HTTP {status}: {truncated}"
            )));
        }

        let json: serde_json::Value = serde_json::from_str(&body_text)
            .map_err(|e| RigInferError::PromptError(format!("invalid JSON response: {e}")))?;

        // T15: Extract token usage from response
        let prompt_tokens = json
            .pointer("/usage/prompt_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let completion_tokens = json
            .pointer("/usage/completion_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        Ok((json, prompt_tokens, completion_tokens))
    }

    /// Raw HTTP completion for OpenAI-compatible endpoints.
    ///
    /// Bypasses rig-core deserialization entirely — extracts `choices[0].message.content`
    /// from the raw JSON response. This avoids deserialization failures with vLLM, Ollama,
    /// and other servers that add non-standard fields (annotations, reasoning, stop_reason).
    #[allow(clippy::too_many_arguments)]
    /// Returns `(content, prompt_tokens, completion_tokens)`.
    async fn raw_openai_compat_infer(
        http_client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        model: &str,
        messages: Vec<serde_json::Value>,
        max_tokens: u64,
        temperature: Option<f64>,
        timeout: std::time::Duration,
    ) -> Result<(String, u64, u64), RigInferError> {
        let mut body = serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": max_tokens,
        });
        if let Some(temp) = temperature {
            body["temperature"] = serde_json::json!(temp);
        }

        let (json, prompt_tokens, completion_tokens) =
            Self::raw_chat_completion(http_client, base_url, api_key, body, timeout).await?;

        let content = json["choices"]
            .get(0)
            .and_then(|c| c["message"]["content"].as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                RigInferError::PromptError(
                    "no content in response choices[0].message.content".into(),
                )
            })?;

        Ok((content, prompt_tokens, completion_tokens))
    }

    /// Simple text completion (infer) using rig-core
    ///
    /// # Arguments
    /// * `prompt` - The text prompt to send
    /// * `model` - Model identifier (uses default if None)
    ///
    /// # Returns
    /// The completion text from the model
    pub async fn infer(
        &self,
        prompt: &str,
        model: Option<&str>,
        max_tokens: Option<u64>,
    ) -> Result<String, RigInferError> {
        /// Maximum time to wait for a single infer() completion (5 minutes).
        /// Prevents hung LLM calls from blocking the runtime indefinitely.
        const INFER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

        let model_id = model.unwrap_or_else(|| self.default_model());
        let effective_max_tokens = max_tokens.unwrap_or(8192);

        match self {
            RigProvider::Claude(client) => {
                // Anthropic requires max_tokens to be set explicitly
                let agent = client
                    .agent(model_id)
                    .max_tokens(effective_max_tokens)
                    .build();
                timeout(INFER_TIMEOUT, agent.prompt(prompt))
                    .await
                    .map_err(|_| RigInferError::Timeout {
                        duration_ms: INFER_TIMEOUT.as_millis() as u64,
                    })?
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::OpenAI(client) => {
                let agent = client
                    .agent(model_id)
                    .max_tokens(effective_max_tokens)
                    .build();
                timeout(INFER_TIMEOUT, agent.prompt(prompt))
                    .await
                    .map_err(|_| RigInferError::Timeout {
                        duration_ms: INFER_TIMEOUT.as_millis() as u64,
                    })?
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::Mistral(client) => {
                let agent = client
                    .agent(model_id)
                    .max_tokens(effective_max_tokens)
                    .build();
                timeout(INFER_TIMEOUT, agent.prompt(prompt))
                    .await
                    .map_err(|_| RigInferError::Timeout {
                        duration_ms: INFER_TIMEOUT.as_millis() as u64,
                    })?
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::Groq(client) => {
                let agent = client
                    .agent(model_id)
                    .max_tokens(effective_max_tokens)
                    .build();
                timeout(INFER_TIMEOUT, agent.prompt(prompt))
                    .await
                    .map_err(|_| RigInferError::Timeout {
                        duration_ms: INFER_TIMEOUT.as_millis() as u64,
                    })?
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::DeepSeek(client) => {
                let agent = client
                    .agent(model_id)
                    .max_tokens(effective_max_tokens)
                    .build();
                timeout(INFER_TIMEOUT, agent.prompt(prompt))
                    .await
                    .map_err(|_| RigInferError::Timeout {
                        duration_ms: INFER_TIMEOUT.as_millis() as u64,
                    })?
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::Gemini(client) => {
                let agent = client
                    .agent(model_id)
                    .max_tokens(effective_max_tokens)
                    .build();
                timeout(INFER_TIMEOUT, agent.prompt(prompt))
                    .await
                    .map_err(|_| RigInferError::Timeout {
                        duration_ms: INFER_TIMEOUT.as_millis() as u64,
                    })?
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::XAi(client) => {
                let agent = client
                    .agent(model_id)
                    .max_tokens(effective_max_tokens)
                    .build();
                timeout(INFER_TIMEOUT, agent.prompt(prompt))
                    .await
                    .map_err(|_| RigInferError::Timeout {
                        duration_ms: INFER_TIMEOUT.as_millis() as u64,
                    })?
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::OpenAiCompat {
                raw_base_url,
                raw_api_key,
                timeout_secs,
                http_client,
                ..
            } => {
                let compat_timeout = std::time::Duration::from_secs(*timeout_secs);
                let messages = vec![serde_json::json!({"role": "user", "content": prompt})];
                let (content, prompt_tokens, completion_tokens) = Self::raw_openai_compat_infer(
                    http_client,
                    raw_base_url,
                    raw_api_key,
                    model_id,
                    messages,
                    effective_max_tokens,
                    None,
                    compat_timeout,
                )
                .await?;
                tracing::debug!(prompt_tokens, completion_tokens, "OpenAiCompat infer usage");
                Ok(content)
            }
            RigProvider::Mock => {
                unreachable!("mock provider generates responses in executor, not via RigProvider")
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
        let max_tok = max_tokens.map(|v| v.max(16)).map(u64::from).unwrap_or(8192);

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
            RigProvider::OpenAiCompat {
                client,
                timeout_secs,
                ..
            } => {
                let compat_timeout = std::time::Duration::from_secs(*timeout_secs);
                let mut builder = client.agent(model_id).max_tokens(max_tok);
                if let Some(sys) = system {
                    builder = builder.preamble(sys);
                }
                let agent = builder.build();
                timeout(compat_timeout, agent.prompt(message))
                    .await
                    .map_err(|_| RigInferError::Timeout {
                        duration_ms: compat_timeout.as_millis() as u64,
                    })?
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            // DeepSeek and Native are handled above via early returns.
            // These arms exist for exhaustiveness in case the early returns are refactored.
            RigProvider::DeepSeek(_) => Err(RigInferError::VisionNotSupported(
                "DeepSeek does not support vision".to_string(),
            )),
            RigProvider::Mock => {
                unreachable!("mock provider generates responses in executor, not via RigProvider")
            }
            #[cfg(feature = "native-inference")]
            RigProvider::Native(_) => Err(RigInferError::VisionNotSupported(
                "Native provider requires NativeRuntime path for vision".to_string(),
            )),
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
        let max_tok = max_tokens.map(|v| v.max(16)).map(u64::from).unwrap_or(8192);

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

        // Use endpoint-specific timeout for OpenAiCompat, default for others
        let effective_timeout = match self {
            RigProvider::OpenAiCompat { timeout_secs, .. } => {
                std::time::Duration::from_secs(*timeout_secs)
            }
            _ => VISION_STREAM_TIMEOUT,
        };

        // Apply overall timeout to prevent slow-drip streams running forever
        timeout(effective_timeout, async {
            match self {
                RigProvider::Claude(client) => vision_stream!(client, true),
                RigProvider::OpenAI(client) => vision_stream!(client, false),
                RigProvider::Mistral(client) => vision_stream!(client, false),
                RigProvider::Groq(client) => vision_stream!(client, false),
                RigProvider::Gemini(client) => vision_stream!(client, false),
                RigProvider::XAi(client) => vision_stream!(client, false),
                RigProvider::OpenAiCompat { client, .. } => vision_stream!(client, false),
                // DeepSeek and Native are handled above via early returns.
                // These arms exist for exhaustiveness in case the early returns are refactored.
                RigProvider::DeepSeek(_) => {
                    return Err(RigInferError::VisionNotSupported(
                        "DeepSeek does not support vision".to_string(),
                    ))
                }
                RigProvider::Mock => {
                    unreachable!(
                        "mock provider generates responses in executor, not via RigProvider"
                    )
                }
                #[cfg(feature = "native-inference")]
                RigProvider::Native(_) => {
                    return Err(RigInferError::VisionNotSupported(
                        "Native provider requires NativeRuntime path for vision".to_string(),
                    ))
                }
            }
            Ok::<(), RigInferError>(())
        })
        .await
        .map_err(|_| RigInferError::Timeout {
            duration_ms: effective_timeout.as_millis() as u64,
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
    /// `(content, prompt_tokens, completion_tokens)` — tokens are non-zero for
    /// OpenAiCompat (from API response), zero for rig-core providers (no access).
    pub async fn infer_with_tools(
        &self,
        prompt: &str,
        tools: Vec<Box<dyn ToolDyn>>,
        model: Option<&str>,
        max_tokens: Option<u32>,
        system: Option<&str>,
    ) -> Result<(String, u64, u64), RigInferError> {
        use rig::agent::AgentBuilder;
        use rig::message::ToolChoice as RigToolChoice;

        /// Maximum time for tool-injection structured output (5 minutes).
        const TOOLS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

        let model_id = model.unwrap_or_else(|| self.default_model());
        let max_tok = max_tokens.map(|v| v.max(16) as u64).unwrap_or(8192);

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
                    .map(|s| (s, 0u64, 0u64)) // rig-core doesn't expose token counts
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }};
        }

        let effective_timeout = match self {
            RigProvider::OpenAiCompat { timeout_secs, .. } => {
                std::time::Duration::from_secs(*timeout_secs)
            }
            _ => TOOLS_TIMEOUT,
        };

        let result = timeout(effective_timeout, async {
            match self {
                RigProvider::Claude(client) => build_agent_with_tools!(client),
                RigProvider::OpenAI(client) => build_agent_with_tools!(client),
                RigProvider::Mistral(client) => build_agent_with_tools!(client),
                RigProvider::Groq(client) => build_agent_with_tools!(client),
                RigProvider::DeepSeek(client) => build_agent_with_tools!(client),
                RigProvider::Gemini(client) => build_agent_with_tools!(client),
                RigProvider::XAi(client) => build_agent_with_tools!(client),
                RigProvider::OpenAiCompat {
                    raw_base_url,
                    raw_api_key,
                    http_client,
                    ..
                } => {
                    // Bypass rig-core agent.prompt() to avoid deserialization
                    // failures with vLLM/Ollama non-standard response fields.
                    // Convert ToolDyn definitions to OpenAI tool format, send raw
                    // HTTP via raw_chat_completion(), and extract tool_calls.
                    let mut openai_tools = Vec::new();
                    for tool in &tools {
                        let def = tool.definition(String::new()).await;
                        openai_tools.push(serde_json::json!({
                            "type": "function",
                            "function": {
                                "name": def.name,
                                "description": def.description,
                                "parameters": def.parameters,
                            }
                        }));
                    }

                    let mut messages = Vec::new();
                    if let Some(sys) = system {
                        messages.push(serde_json::json!({"role": "system", "content": sys}));
                    }
                    messages.push(serde_json::json!({"role": "user", "content": prompt}));

                    let body = serde_json::json!({
                        "model": model_id,
                        "messages": messages,
                        "max_tokens": max_tok,
                        "tools": openai_tools,
                        "tool_choice": "required",
                    });

                    let (json, prompt_tokens, completion_tokens) = Self::raw_chat_completion(
                        http_client,
                        raw_base_url,
                        raw_api_key,
                        body,
                        effective_timeout,
                    )
                    .await?;

                    // Primary: extract tool call arguments
                    let arguments = json["choices"]
                        .get(0)
                        .and_then(|c| c["message"]["tool_calls"].get(0))
                        .and_then(|tc| tc["function"]["arguments"].as_str())
                        .map(|s| s.to_string());

                    if let Some(args) = arguments {
                        Ok((args, prompt_tokens, completion_tokens))
                    } else {
                        // Fallback: content field (some vLLM models respond
                        // with JSON in content instead of tool calls)
                        json["choices"]
                            .get(0)
                            .and_then(|c| c["message"]["content"].as_str())
                            .map(|s| s.to_string())
                            .map(|s| (s, prompt_tokens, completion_tokens))
                            .ok_or_else(|| {
                                RigInferError::PromptError(
                                    "no tool_calls or content in response".into(),
                                )
                            })
                    }
                }
                RigProvider::Mock => {
                    unreachable!(
                        "mock provider generates responses in executor, not via RigProvider"
                    )
                }
                #[cfg(feature = "native-inference")]
                RigProvider::Native(_) => Err(RigInferError::PromptError(
                    "Native inference does not support tool-based structured output".to_string(),
                )),
            }
        })
        .await
        .map_err(|_| RigInferError::Timeout {
            duration_ms: effective_timeout.as_millis() as u64,
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
        // Clamp to 16 minimum — OpenAI rejects < 16, no provider benefits from < 16
        let max_tokens = options.max_tokens.unwrap_or(8192).max(16);

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
            RigProvider::OpenAiCompat {
                raw_base_url,
                raw_api_key,
                timeout_secs,
                http_client,
                ..
            } => {
                let compat_timeout = std::time::Duration::from_secs(*timeout_secs);
                let mut messages = Vec::new();
                if let Some(system) = &options.system {
                    messages.push(serde_json::json!({"role": "system", "content": system}));
                }
                messages.push(serde_json::json!({"role": "user", "content": user_prompt}));
                let (content, prompt_tokens, completion_tokens) = Self::raw_openai_compat_infer(
                    http_client,
                    raw_base_url,
                    raw_api_key,
                    model_id,
                    messages,
                    max_tokens as u64,
                    effective_temperature,
                    compat_timeout,
                )
                .await?;
                tracing::debug!(
                    prompt_tokens,
                    completion_tokens,
                    "OpenAiCompat infer_with_options usage"
                );
                Ok(content)
            }
            RigProvider::Mock => {
                unreachable!("mock provider generates responses in executor, not via RigProvider")
            }
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
            if p.category == ProviderCategory::Llm && crate::secrets::has_provider_key(p) {
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
        if crate::secrets::store::resolve_env("NIKA_NATIVE_MODEL").is_some() {
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

        match self.infer(test_prompt, None, None).await {
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
                        details: crate::util::redact_secrets(&e.to_string()),
                    })
                } else {
                    Err(ProviderVerifyError::ProviderError {
                        provider: self.name().to_string(),
                        details: crate::util::redact_secrets(&e.to_string()),
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
        let has_key = |key: &str| crate::secrets::store::resolve_env(key).is_some();

        match self {
            RigProvider::Claude(_) => has_key("ANTHROPIC_API_KEY"),
            RigProvider::OpenAI(_) => has_key("OPENAI_API_KEY"),
            RigProvider::Mistral(_) => has_key("MISTRAL_API_KEY"),
            RigProvider::Groq(_) => has_key("GROQ_API_KEY"),
            RigProvider::DeepSeek(_) => has_key("DEEPSEEK_API_KEY"),
            RigProvider::Gemini(_) => has_key("GEMINI_API_KEY"),
            RigProvider::XAi(_) => has_key("XAI_API_KEY"),
            RigProvider::OpenAiCompat { .. } => true,
            RigProvider::Mock => true,
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

    /// True only for Anthropic/Claude — controls `is_anthropic` param in
    /// `consume_rig_stream` (thinking block capture, stop_reason mapping).
    pub fn is_anthropic(&self) -> bool {
        matches!(self, RigProvider::Claude(_))
    }

    /// True if this provider supports vision/multimodal content.
    /// Used to give an early, clear error before attempting the call.
    pub fn supports_vision(&self) -> bool {
        !matches!(self, RigProvider::DeepSeek(_) | RigProvider::Mock)
    }

    /// True if extended thinking (chain-of-thought) is supported.
    pub fn supports_thinking(&self) -> bool {
        matches!(self, RigProvider::Claude(_))
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
        let mut response_parts: Vec<String> = Vec::new();
        let mut result = StreamResult::default();

        match self {
            RigProvider::Claude(client) => {
                let model = client.completion_model(model_id);
                let request = model
                    .completion_request(prompt)
                    .max_tokens(effective_max_tokens)
                    .build();
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
                let request = model
                    .completion_request(prompt)
                    .max_tokens(effective_max_tokens)
                    .build();
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
                let request = model
                    .completion_request(prompt)
                    .max_tokens(effective_max_tokens)
                    .build();
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
                let request = model
                    .completion_request(prompt)
                    .max_tokens(effective_max_tokens)
                    .build();
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
                let request = model
                    .completion_request(prompt)
                    .max_tokens(effective_max_tokens)
                    .build();
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
                let request = model
                    .completion_request(prompt)
                    .max_tokens(effective_max_tokens)
                    .build();
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
                let request = model
                    .completion_request(prompt)
                    .max_tokens(effective_max_tokens)
                    .build();
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
            RigProvider::Mock => {
                unreachable!("mock provider generates responses in executor, not via RigProvider")
            }
            RigProvider::OpenAiCompat { client, .. } => {
                let model = client.completion_model(model_id);
                let request = model
                    .completion_request(prompt)
                    .max_tokens(effective_max_tokens)
                    .build();
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
            RigProvider::Mock => {
                unreachable!("mock provider generates responses in executor, not via RigProvider")
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
mod tests;
