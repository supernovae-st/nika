// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Provider construction, factory methods, and metadata.
//!
//! All methods that create, configure, or inspect `RigProvider` instances.

use super::*;

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
            // OpenAI-compatible providers — config-driven via static table
            _ => {
                if let Some(&(id, base_url, env_var)) = OPENAI_COMPAT_PROVIDERS
                    .iter()
                    .find(|(id, _, _)| *id == provider.id)
                {
                    let key = crate::secrets::store::resolve_env(env_var).ok_or_else(|| {
                        ProviderError::MissingApiKey {
                            provider: id.into(),
                        }
                    })?;
                    Self::openai_compat(id, base_url, &key, None, 300, None)
                } else {
                    Err(ProviderError::NotConfigured {
                        provider: name.to_string(),
                    }
                    .into())
                }
            }
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
                ep.hourly_rate,
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
            // OpenAI-compatible providers — config-driven via static table
            _ => {
                if let Some(&(id, base_url, _)) = OPENAI_COMPAT_PROVIDERS
                    .iter()
                    .find(|(id, _, _)| *id == provider.id)
                {
                    Self::openai_compat(id, base_url, api_key, None, 300, None)
                } else {
                    Err(ProviderError::NotConfigured {
                        provider: name.to_string(),
                    }
                    .into())
                }
            }
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
        hourly_rate: Option<f64>,
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
            hourly_rate,
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
        RigProvider::Native(super::super::native::NativeRuntime::new())
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
        config: Option<super::super::native::LoadConfig>,
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
        config: Option<super::super::native::LoadConfig>,
        event_log: Option<&crate::event::EventLog>,
    ) -> Result<(), RigInferError> {
        let path = model_path.into();
        let resolved_config = config.unwrap_or_default();

        // Determine kind + identifier before move
        let (model_id, kind) = match &resolved_config.model_kind {
            super::super::native::NativeModelKind::TextGguf => (
                path.file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string()),
                "gguf".to_string(),
            ),
            super::super::native::NativeModelKind::VisionHf { model_id, .. } => {
                (model_id.clone(), "huggingface".to_string())
            }
        };

        let load_start = Instant::now();

        match self {
            RigProvider::Native(runtime) => {
                runtime.load(path.clone(), resolved_config).await.map_err(
                    |e: super::super::native::NativeError| {
                        RigInferError::PromptError(e.to_string())
                    },
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

    /// Get the configured hourly rate for custom self-hosted endpoints.
    ///
    /// Returns `Some(rate)` only for `OpenAiCompat` variants constructed with
    /// an hourly_rate (from `[endpoints.NAME] hourly_rate = ...` in
    /// `nika.toml`). When present, cost is computed as
    /// `(duration_secs / 3600) × hourly_rate` instead of token pricing.
    ///
    /// Introduced in DX-3 to fix the P0 regression where hourly endpoints
    /// emitted `cost_usd = 0.0` via the delegation path because the model
    /// string (e.g. `Qwen/Qwen3-8B`) doesn't match the static pricing catalog.
    pub fn hourly_rate(&self) -> Option<f64> {
        match self {
            RigProvider::OpenAiCompat { hourly_rate, .. } => *hourly_rate,
            _ => None,
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
