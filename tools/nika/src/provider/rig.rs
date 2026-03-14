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

use crate::mcp::McpClient;
use crate::util::STREAM_CHUNK_TIMEOUT;
use futures::StreamExt;

// Import InferenceBackend trait for native inference methods
#[cfg(feature = "native-inference")]
use crate::provider::native::InferenceBackend;
use rig::client::{CompletionClient, ProviderClient};
use rig::completion::{CompletionModel as _, GetTokenUsage, Prompt, PromptError, ToolDefinition};
use rig::providers::{anthropic, deepseek, gemini, groq, mistral, openai};
use rig::streaming::StreamedAssistantContent;
use rig::tool::{ToolDyn, ToolError};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::timeout;

// ═══════════════════════════════════════════════════════════════════════════
// TOOL ERROR TYPES
// ═══════════════════════════════════════════════════════════════════════════

/// MCP tool call error with semantic error kinds
///
/// Provides proper error semantics instead of wrapping in std::io::Error.
#[derive(Debug)]
pub struct McpToolError {
    kind: McpToolErrorKind,
    message: String,
}

/// Error kinds for MCP tool calls
#[derive(Debug, Clone, Copy)]
pub enum McpToolErrorKind {
    /// Invalid JSON arguments
    InvalidArguments,
    /// MCP client not configured
    NotConfigured,
    /// MCP tool call failed
    CallFailed,
    /// Failed to serialize/deserialize result
    SerializationError,
}

impl McpToolError {
    /// Create an invalid arguments error
    pub fn invalid_args(msg: impl Into<String>) -> Self {
        Self {
            kind: McpToolErrorKind::InvalidArguments,
            message: msg.into(),
        }
    }

    /// Create a not configured error
    pub fn not_configured(msg: impl Into<String>) -> Self {
        Self {
            kind: McpToolErrorKind::NotConfigured,
            message: msg.into(),
        }
    }

    /// Create a call failed error
    pub fn call_failed(msg: impl Into<String>) -> Self {
        Self {
            kind: McpToolErrorKind::CallFailed,
            message: msg.into(),
        }
    }

    /// Create a serialization error
    pub fn serialization(msg: impl Into<String>) -> Self {
        Self {
            kind: McpToolErrorKind::SerializationError,
            message: msg.into(),
        }
    }
}

impl std::fmt::Display for McpToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind_str = match self.kind {
            McpToolErrorKind::InvalidArguments => "InvalidArguments",
            McpToolErrorKind::NotConfigured => "NotConfigured",
            McpToolErrorKind::CallFailed => "CallFailed",
            McpToolErrorKind::SerializationError => "SerializationError",
        };
        write!(f, "[{}] {}", kind_str, self.message)
    }
}

impl std::error::Error for McpToolError {}

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
    /// Native local provider - GGUF models via mistral.rs
    /// Requires `native-inference` feature and explicit model loading.
    /// Now uses NativeRuntime directly with full streaming support.
    #[cfg(feature = "native-inference")]
    Native(super::native::NativeRuntime),
}

impl RigProvider {
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
        match self {
            RigProvider::Native(runtime) => runtime
                .load(model_path.into(), config.unwrap_or_default())
                .await
                .map_err(|e: super::native::NativeError| RigInferError::PromptError(e.to_string())),
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

    /// Get the provider name
    pub fn name(&self) -> &'static str {
        match self {
            RigProvider::Claude(_) => "claude",
            RigProvider::OpenAI(_) => "openai",
            RigProvider::Mistral(_) => "mistral",
            RigProvider::Groq(_) => "groq",
            RigProvider::DeepSeek(_) => "deepseek",
            RigProvider::Gemini(_) => "gemini",
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
    pub fn default_model(&self) -> &'static str {
        match self {
            // Note: rig-core's CLAUDE_3_5_SONNET constant is outdated
            // Using explicit model name for stability
            RigProvider::Claude(_) => "claude-sonnet-4-6",
            RigProvider::OpenAI(_) => openai::GPT_4O,
            RigProvider::Mistral(_) => mistral::MISTRAL_LARGE,
            RigProvider::Groq(_) => "llama-3.3-70b-versatile",
            RigProvider::DeepSeek(_) => "deepseek-chat",
            RigProvider::Gemini(_) => "gemini-2.0-flash",
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
        let model_id = model.unwrap_or_else(|| self.default_model());

        match self {
            RigProvider::Claude(client) => {
                // Anthropic requires max_tokens to be set explicitly
                let agent = client.agent(model_id).max_tokens(8192).build();
                agent
                    .prompt(prompt)
                    .await
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::OpenAI(client) => {
                let agent = client.agent(model_id).max_tokens(8192).build();
                agent
                    .prompt(prompt)
                    .await
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::Mistral(client) => {
                let agent = client.agent(model_id).max_tokens(8192).build();
                agent
                    .prompt(prompt)
                    .await
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::Groq(client) => {
                let agent = client.agent(model_id).max_tokens(8192).build();
                agent
                    .prompt(prompt)
                    .await
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::DeepSeek(client) => {
                let agent = client.agent(model_id).max_tokens(8192).build();
                agent
                    .prompt(prompt)
                    .await
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::Gemini(client) => {
                let agent = client.agent(model_id).max_tokens(8192).build();
                agent
                    .prompt(prompt)
                    .await
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            #[cfg(feature = "native-inference")]
            RigProvider::Native(runtime) => {
                // Native inference uses direct API, not rig-core agent
                // Model must be pre-loaded via load_native_model()
                runtime
                    .infer(prompt, super::native::ChatOptions::default())
                    .await
                    .map(|r| r.message.content)
                    .map_err(|e: super::native::NativeError| {
                        RigInferError::PromptError(e.to_string())
                    })
            }
        }
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
        let model_id = options
            .model
            .as_deref()
            .unwrap_or_else(|| self.default_model());
        let max_tokens = options.max_tokens.unwrap_or(8192);

        // Build prompt with system message if provided
        let full_prompt = if let Some(system) = &options.system {
            format!("{}\n\n{}", system, prompt)
        } else {
            prompt.to_string()
        };

        match self {
            RigProvider::Claude(client) => {
                let mut builder = client.agent(model_id).max_tokens(max_tokens as u64);
                if let Some(temp) = options.temperature {
                    builder = builder.temperature(temp);
                }
                let agent = builder.build();
                agent
                    .prompt(&full_prompt)
                    .await
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::OpenAI(client) => {
                let mut builder = client.agent(model_id).max_tokens(max_tokens as u64);
                if let Some(temp) = options.temperature {
                    builder = builder.temperature(temp);
                }
                let agent = builder.build();
                agent
                    .prompt(&full_prompt)
                    .await
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::Mistral(client) => {
                let mut builder = client.agent(model_id).max_tokens(max_tokens as u64);
                if let Some(temp) = options.temperature {
                    builder = builder.temperature(temp);
                }
                let agent = builder.build();
                agent
                    .prompt(&full_prompt)
                    .await
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::Groq(client) => {
                let mut builder = client.agent(model_id).max_tokens(max_tokens as u64);
                if let Some(temp) = options.temperature {
                    builder = builder.temperature(temp);
                }
                let agent = builder.build();
                agent
                    .prompt(&full_prompt)
                    .await
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::DeepSeek(client) => {
                let mut builder = client.agent(model_id).max_tokens(max_tokens as u64);
                if let Some(temp) = options.temperature {
                    builder = builder.temperature(temp);
                }
                let agent = builder.build();
                agent
                    .prompt(&full_prompt)
                    .await
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::Gemini(client) => {
                let mut builder = client.agent(model_id).max_tokens(max_tokens as u64);
                if let Some(temp) = options.temperature {
                    builder = builder.temperature(temp);
                }
                let agent = builder.build();
                agent
                    .prompt(&full_prompt)
                    .await
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            #[cfg(feature = "native-inference")]
            RigProvider::Native(runtime) => {
                // Native inference uses ChatOptions from native module
                let chat_options = super::native::ChatOptions {
                    temperature: options.temperature.map(|t| t as f32),
                    max_tokens: options.max_tokens,
                    ..Default::default()
                };
                runtime
                    .infer(&full_prompt, chat_options)
                    .await
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
        // Helper: check env var exists and is non-empty
        let has_key = |key: &str| std::env::var(key).is_ok_and(|v| !v.trim().is_empty());

        if has_key("ANTHROPIC_API_KEY") {
            return Some(Self::claude());
        }
        if has_key("OPENAI_API_KEY") {
            return Some(Self::openai());
        }
        if has_key("MISTRAL_API_KEY") {
            return Some(Self::mistral());
        }
        if has_key("GROQ_API_KEY") {
            return Some(Self::groq());
        }
        if has_key("DEEPSEEK_API_KEY") {
            return Some(Self::deepseek());
        }
        // Gemini
        if has_key("GEMINI_API_KEY") {
            return Some(Self::gemini());
        }
        // Native is opt-in: requires NIKA_NATIVE_MODEL to be set
        // Note: Model must still be loaded via load_native_model() before inference
        #[cfg(feature = "native-inference")]
        if has_key("NIKA_NATIVE_MODEL") {
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
            #[cfg(feature = "native-inference")]
            RigProvider::Native(_) => {
                // Native doesn't need API key, but requires model to be loaded
                // Use is_native_loaded() to check if ready for inference
                true
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Provider Verification Types
// ═══════════════════════════════════════════════════════════════════════════

/// Result of a successful provider verification
#[derive(Debug, Clone)]
pub struct ProviderVerifyResult {
    /// Provider name (claude, openai, etc.)
    pub provider: String,
    /// Round-trip latency for the test call
    pub latency: std::time::Duration,
    /// Model used for verification
    pub model: String,
}

/// Error during provider verification
#[derive(Debug, Clone, thiserror::Error)]
pub enum ProviderVerifyError {
    #[error("Invalid API key for {provider}")]
    InvalidApiKey { provider: String },

    #[error("Rate limited by {provider}")]
    RateLimited { provider: String },

    #[error("Connection timeout to {provider}")]
    Timeout { provider: String },

    #[error("Network error connecting to {provider}: {details}")]
    NetworkError { provider: String, details: String },

    #[error("Provider error from {provider}: {details}")]
    ProviderError { provider: String, details: String },
}

impl ProviderVerifyError {
    /// Get a user-friendly suggestion for fixing the error
    pub fn suggestion(&self) -> &'static str {
        match self {
            ProviderVerifyError::InvalidApiKey { .. } => {
                "Check your API key in environment variables"
            }
            ProviderVerifyError::RateLimited { .. } => {
                "Wait a moment and try again, or check your plan limits"
            }
            ProviderVerifyError::Timeout { .. } => "Check your network connection or try again",
            ProviderVerifyError::NetworkError { .. } => {
                "Check your internet connection and firewall settings"
            }
            ProviderVerifyError::ProviderError { .. } => {
                "The provider service may be experiencing issues"
            }
        }
    }
}

/// Error type for RigProvider infer operations
#[derive(Debug, thiserror::Error)]
pub enum RigInferError {
    #[error("Completion error: {0}")]
    PromptError(String),

    /// Stream timeout - no chunk received within timeout period
    #[error("Stream timeout: no chunk received for {duration_ms}ms")]
    Timeout { duration_ms: u64 },
}

// =============================================================================
// StreamChunk - Communication type for streaming responses
// =============================================================================

/// Chunk of streaming response for real-time display
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// Text token from the model
    Token(String),
    /// Thinking/reasoning content (Claude extended thinking)
    Thinking(String),
    /// Stream completed successfully with final text
    Done(String),
    /// Stream failed with error
    Error(String),
    /// Token usage metrics (sent after completion)
    Metrics {
        input_tokens: u64,
        output_tokens: u64,
    },
    /// MCP server connected successfully
    McpConnected(String),
    /// MCP server connection failed
    McpError { server_name: String, error: String },
    // ═══════════════════════════════════════════════════════════════════════════
    // Chat Inline Box Events
    // ═══════════════════════════════════════════════════════════════════════════
    /// MCP tool call started (for inline visualization)
    McpCallStart {
        tool: String,
        server: String,
        params: String,
    },
    /// MCP tool call completed successfully
    McpCallComplete { result: String },
    /// MCP tool call failed
    McpCallFailed { error: String },
    /// Infer stream started (for inline visualization)
    InferStart {
        model: String,
        /// The user prompt text (for TaskBox::Infer display)
        prompt: String,
        prompt_tokens: u32,
        max_tokens: u32,
    },
    /// Infer stream token count update
    InferTokens { output_tokens: u32 },
    /// Infer stream completed
    InferComplete,
    // ═══════════════════════════════════════════════════════════════════════════
    // Activity Events for /exec, /fetch, /agent
    // ═══════════════════════════════════════════════════════════════════════════
    /// Shell command started (for activity stack)
    ExecStart { command: String },
    /// Shell command completed
    ExecComplete,
    /// HTTP fetch started (for activity stack)
    FetchStart { url: String, method: String },
    /// HTTP fetch completed
    FetchComplete,
    /// Agent loop started (for activity stack)
    AgentStart { goal: String },
    /// Agent loop completed
    AgentComplete,
    // ═══════════════════════════════════════════════════════════════════════════
    // Connection Verification Events
    // ═══════════════════════════════════════════════════════════════════════════
    /// Provider verification started
    ProviderVerifying { provider: String, model: String },
    /// Provider verification succeeded
    ProviderVerified {
        provider: String,
        model: String,
        latency_ms: u64,
    },
    /// Provider verification failed
    ProviderVerifyFailed { provider: String, error: String },
    /// Provider not configured (no API key set)
    ProviderNotConfigured { provider: String },
    /// MCP server ping started
    McpPinging { server: String },
    /// MCP server ping succeeded
    McpPinged {
        server: String,
        latency_ms: u64,
        tool_count: usize,
    },
    /// All provider verifications timed out (no providers available)
    ProviderVerificationTimeout,
    // ═══════════════════════════════════════════════════════════════════════════
    // Native Model Events
    // ═══════════════════════════════════════════════════════════════════════════
    /// Native model pull started
    NativeModelPullStarted { model: String },
    /// Native model pull progress update
    NativeModelPullProgress {
        model: String,
        status: String,
        completed: u64,
        total: u64,
    },
    /// Native model pull completed successfully
    NativeModelPulled {
        model: String,
        path: String,
        size: u64,
    },
    /// Native model pull failed
    NativeModelPullFailed { model: String, error: String },
    /// Native model deleted
    NativeModelDeleted { model: String },
    /// Native model delete failed
    NativeModelDeleteFailed { model: String, error: String },
    /// Native models list refreshed
    NativeModelsRefreshed { count: usize },
}

// =============================================================================
// StreamResult - Complete streaming response with token usage
// =============================================================================

/// Complete streaming response with text and token usage metrics
#[derive(Debug, Clone, Default)]
pub struct StreamResult {
    /// The complete response text
    pub text: String,
    /// Number of input tokens used
    pub input_tokens: u64,
    /// Number of output tokens generated
    pub output_tokens: u64,
    /// Total tokens (input + output)
    pub total_tokens: u64,
    /// Cached input tokens (from prompt caching)
    pub cached_input_tokens: u64,
}

impl StreamResult {
    /// Create a new StreamResult with just text (zero tokens)
    pub fn from_text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Default::default()
        }
    }
}

/// Consume a rig-core streaming response, forwarding chunks to the channel.
///
/// Shared streaming loop for all rig-core providers. Handles:
/// - Per-chunk timeout via `STREAM_CHUNK_TIMEOUT`
/// - Token text forwarding via `StreamChunk::Token`
/// - Optional thinking/reasoning capture (Claude only)
/// - Token usage extraction from `Final` response
///
/// Returns early on timeout or stream error.
async fn consume_rig_stream<R>(
    stream: &mut rig::streaming::StreamingCompletionResponse<R>,
    tx: &mpsc::Sender<StreamChunk>,
    response_parts: &mut Vec<String>,
    result: &mut StreamResult,
    capture_thinking: bool,
) -> Result<(), RigInferError>
where
    R: Clone + Unpin + GetTokenUsage + serde::Serialize + serde::de::DeserializeOwned,
{
    loop {
        let chunk_result = match timeout(STREAM_CHUNK_TIMEOUT, stream.next()).await {
            Ok(Some(result)) => result,
            Ok(None) => break,
            Err(_elapsed) => {
                let _ = tx.try_send(StreamChunk::Error(format!(
                    "Stream timeout: no chunk received for {}s",
                    STREAM_CHUNK_TIMEOUT.as_secs()
                )));
                return Err(RigInferError::Timeout {
                    duration_ms: STREAM_CHUNK_TIMEOUT.as_millis() as u64,
                });
            }
        };

        match chunk_result {
            Ok(content) => match content {
                StreamedAssistantContent::Text(text) => {
                    response_parts.push(text.text.clone());
                    let _ = tx.try_send(StreamChunk::Token(text.text));
                }
                StreamedAssistantContent::ReasoningDelta { reasoning, .. } if capture_thinking => {
                    let _ = tx.try_send(StreamChunk::Thinking(reasoning));
                }
                StreamedAssistantContent::Final(response) => {
                    if let Some(usage) = response.token_usage() {
                        result.input_tokens = usage.input_tokens;
                        result.output_tokens = usage.output_tokens;
                        result.total_tokens = usage.total_tokens;
                        result.cached_input_tokens = usage.cached_input_tokens;
                    }
                }
                _ => {}
            },
            Err(e) => {
                let _ = tx.try_send(StreamChunk::Error(e.to_string()));
                return Err(RigInferError::PromptError(e.to_string()));
            }
        }
    }
    Ok(())
}

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
    ) -> Result<StreamResult, RigInferError> {
        let model_id = model.unwrap_or_else(|| self.default_model());
        let mut response_parts: Vec<String> = Vec::new();
        let mut result = StreamResult::default();

        match self {
            RigProvider::Claude(client) => {
                let model = client.completion_model(model_id);
                let request = model.completion_request(prompt).max_tokens(8192).build();
                let mut stream = model
                    .stream(request)
                    .await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))?;
                consume_rig_stream(&mut stream, &tx, &mut response_parts, &mut result, true)
                    .await?;
            }
            RigProvider::OpenAI(client) => {
                let model = client.completion_model(model_id);
                let request = model.completion_request(prompt).max_tokens(8192).build();
                let mut stream = model
                    .stream(request)
                    .await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))?;
                consume_rig_stream(&mut stream, &tx, &mut response_parts, &mut result, false)
                    .await?;
            }
            RigProvider::Mistral(client) => {
                let model = client.completion_model(model_id);
                let request = model.completion_request(prompt).max_tokens(8192).build();
                let mut stream = model
                    .stream(request)
                    .await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))?;
                consume_rig_stream(&mut stream, &tx, &mut response_parts, &mut result, false)
                    .await?;
            }
            RigProvider::Groq(client) => {
                let model = client.completion_model(model_id);
                let request = model.completion_request(prompt).max_tokens(8192).build();
                let mut stream = model
                    .stream(request)
                    .await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))?;
                consume_rig_stream(&mut stream, &tx, &mut response_parts, &mut result, false)
                    .await?;
            }
            RigProvider::DeepSeek(client) => {
                let model = client.completion_model(model_id);
                let request = model.completion_request(prompt).max_tokens(8192).build();
                let mut stream = model
                    .stream(request)
                    .await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))?;
                consume_rig_stream(&mut stream, &tx, &mut response_parts, &mut result, false)
                    .await?;
            }
            RigProvider::Gemini(client) => {
                let model = client.completion_model(model_id);
                let request = model.completion_request(prompt).max_tokens(8192).build();
                let mut stream = model
                    .stream(request)
                    .await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))?;
                consume_rig_stream(&mut stream, &tx, &mut response_parts, &mut result, false)
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

                // Note: Token counts not available in streaming mode
                // They would require post-hoc tokenization
            }
        }

        let complete_response = response_parts.concat();
        let _ = tx.try_send(StreamChunk::Done(complete_response.clone()));

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
        let model_id = options
            .model
            .as_deref()
            .unwrap_or_else(|| self.default_model());
        let max_tokens = options.max_tokens.unwrap_or(8192);
        let mut response_parts: Vec<String> = Vec::new();
        let mut result = StreamResult::default();

        // Build prompt with system message if provided
        let full_prompt = if let Some(system) = &options.system {
            format!("{}\n\n{}", system, prompt)
        } else {
            prompt.to_string()
        };

        // Helper: build request with options and start streaming
        macro_rules! build_request_with_options {
            ($client:expr) => {{
                let model = $client.completion_model(model_id);
                let mut rb = model
                    .completion_request(&full_prompt)
                    .max_tokens(max_tokens as u64);
                if let Some(temp) = options.temperature {
                    rb = rb.temperature(temp);
                }
                model
                    .stream(rb.build())
                    .await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))?
            }};
        }

        match self {
            RigProvider::Claude(client) => {
                let mut stream = build_request_with_options!(client);
                consume_rig_stream(&mut stream, &tx, &mut response_parts, &mut result, true)
                    .await?;
            }
            RigProvider::OpenAI(client) => {
                let mut stream = build_request_with_options!(client);
                consume_rig_stream(&mut stream, &tx, &mut response_parts, &mut result, false)
                    .await?;
            }
            RigProvider::Mistral(client) => {
                let mut stream = build_request_with_options!(client);
                consume_rig_stream(&mut stream, &tx, &mut response_parts, &mut result, false)
                    .await?;
            }
            RigProvider::Groq(client) => {
                let mut stream = build_request_with_options!(client);
                consume_rig_stream(&mut stream, &tx, &mut response_parts, &mut result, false)
                    .await?;
            }
            RigProvider::DeepSeek(client) => {
                let mut stream = build_request_with_options!(client);
                consume_rig_stream(&mut stream, &tx, &mut response_parts, &mut result, false)
                    .await?;
            }
            RigProvider::Gemini(client) => {
                let mut stream = build_request_with_options!(client);
                consume_rig_stream(&mut stream, &tx, &mut response_parts, &mut result, false)
                    .await?;
            }
            // Native provider - uses infer_stream() with options for true streaming
            #[cfg(feature = "native-inference")]
            RigProvider::Native(runtime) => {
                use futures::StreamExt;
                use std::pin::pin;

                // Native inference now supports streaming via mistral.rs
                let chat_options = super::native::ChatOptions {
                    temperature: options.temperature.map(|t| t as f32),
                    max_tokens: options.max_tokens,
                    ..Default::default()
                };
                let stream = runtime
                    .infer_stream(&full_prompt, chat_options)
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

                // Note: Token counts not available in streaming mode
                // They would require post-hoc tokenization
            }
        }

        let complete_response = response_parts.concat();
        let _ = tx.try_send(StreamChunk::Done(complete_response.clone()));

        let _ = tx.try_send(StreamChunk::Metrics {
            input_tokens: result.input_tokens,
            output_tokens: result.output_tokens,
        });

        result.text = complete_response;
        Ok(result)
    }
}

// =============================================================================
// NikaMcpTool - Wrapper for MCP tools implementing rig-core's ToolDyn
// =============================================================================

/// Tool definition for Nika MCP tools.
///
/// This is our own definition struct that avoids the rmcp version conflict.
/// We convert MCP tool definitions from rmcp 0.16 into this format.
#[derive(Debug, Clone)]
pub struct NikaMcpToolDef {
    /// Tool name (e.g., "novanet_generate")
    pub name: String,
    /// Tool description for the LLM
    pub description: String,
    /// JSON Schema for input parameters
    pub input_schema: serde_json::Value,
}

/// MCP tool wrapper implementing rig-core's `ToolDyn` trait.
///
/// This allows us to use our MCP tools (rmcp 0.16) with rig-core's
/// agent system without version conflicts.
#[derive(Debug, Clone)]
pub struct NikaMcpTool {
    definition: NikaMcpToolDef,
    /// Optional MCP client for real tool calls
    client: Option<Arc<McpClient>>,
}

impl NikaMcpTool {
    /// Create a new NikaMcpTool from a definition (without client)
    pub fn new(definition: NikaMcpToolDef) -> Self {
        Self {
            definition,
            client: None,
        }
    }

    /// Create a new NikaMcpTool with an MCP client for real tool calls
    pub fn with_client(definition: NikaMcpToolDef, client: Arc<McpClient>) -> Self {
        Self {
            definition,
            client: Some(client),
        }
    }

    /// Get the tool name
    pub fn tool_name(&self) -> &str {
        &self.definition.name
    }
}

/// Type alias for boxed future (required by ToolDyn)
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

impl ToolDyn for NikaMcpTool {
    fn name(&self) -> String {
        self.definition.name.clone()
    }

    fn definition(&self, _prompt: String) -> BoxFuture<'_, ToolDefinition> {
        let def = ToolDefinition {
            name: self.definition.name.clone(),
            description: self.definition.description.clone(),
            parameters: self.definition.input_schema.clone(),
        };
        Box::pin(async move { def })
    }

    fn call(&self, args: String) -> BoxFuture<'_, Result<String, ToolError>> {
        let tool_name = self.definition.name.clone();
        let client = self.client.clone();

        Box::pin(async move {
            // Parse the args as JSON
            let params: serde_json::Value = serde_json::from_str(&args).map_err(|e| {
                ToolError::ToolCallError(Box::new(McpToolError::invalid_args(format!(
                    "Invalid JSON arguments: {}",
                    e
                ))))
            })?;

            // Check if we have a client
            let client = client.ok_or_else(|| {
                ToolError::ToolCallError(Box::new(McpToolError::not_configured(
                    "No MCP client configured for this tool",
                )))
            })?;

            // Call the MCP tool
            let result = client.call_tool(&tool_name, params).await.map_err(|e| {
                ToolError::ToolCallError(Box::new(McpToolError::call_failed(format!(
                    "MCP tool call failed: {}",
                    e
                ))))
            })?;

            // Extract text content from the result
            let output = result.text();

            if output.is_empty() {
                // Return the full result as JSON if no text content
                serde_json::to_string(&result).map_err(|e| {
                    ToolError::ToolCallError(Box::new(McpToolError::serialization(format!(
                        "Failed to serialize result: {}",
                        e
                    ))))
                })
            } else {
                Ok(output)
            }
        })
    }
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
            name: "novanet_generate".to_string(),
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
        assert_eq!(tool.tool_name(), "novanet_generate");
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

    /// UC1: novanet_generate - Generate native content for an entity
    #[tokio::test]
    async fn test_usecase_novanet_generate_entity_locale() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        // Given: Mock NovaNet MCP client
        let client = Arc::new(McpClient::mock("novanet"));

        // Given: novanet_generate tool with full schema (matching NovaNet MCP spec)
        let tool_def = NikaMcpToolDef {
            name: "novanet_generate".to_string(),
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
            "novanet_generate should succeed: {:?}",
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

    /// UC3: novanet_traverse - Graph traversal
    #[tokio::test]
    async fn test_usecase_novanet_traverse_graph() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        let client = Arc::new(McpClient::mock("novanet"));

        let tool_def = NikaMcpToolDef {
            name: "novanet_traverse".to_string(),
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
        assert!(result.is_ok(), "novanet_traverse should succeed");
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

    /// UC5: novanet_atoms - Knowledge atoms retrieval
    #[tokio::test]
    async fn test_usecase_novanet_atoms_locale() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        let client = Arc::new(McpClient::mock("novanet"));

        let tool_def = NikaMcpToolDef {
            name: "novanet_atoms".to_string(),
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
        assert!(result.is_ok(), "novanet_atoms should succeed");
    }

    /// UC6: novanet_assemble - Context assembly
    #[tokio::test]
    async fn test_usecase_novanet_assemble_context() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        let client = Arc::new(McpClient::mock("novanet"));

        let tool_def = NikaMcpToolDef {
            name: "novanet_assemble".to_string(),
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
        assert!(result.is_ok(), "novanet_assemble should succeed");
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
            name: "novanet_generate".to_string(),
            description: "Generate content".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        });

        let tool2 = NikaMcpTool::new(NikaMcpToolDef {
            name: "novanet_describe".to_string(),
            description: "Describe entity".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        });

        let tool3 = NikaMcpTool::new(NikaMcpToolDef {
            name: "novanet_traverse".to_string(),
            description: "Traverse graph".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        });

        // Then: Each tool maintains its own identity
        assert_eq!(tool1.tool_name(), "novanet_generate");
        assert_eq!(tool2.tool_name(), "novanet_describe");
        assert_eq!(tool3.tool_name(), "novanet_traverse");
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
            name: "novanet_generate".to_string(),
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
    }

    #[test]
    fn test_infer_options_with_all_fields() {
        let opts = InferOptions {
            model: Some("gpt-4o".to_string()),
            temperature: Some(0.7),
            max_tokens: Some(2000),
            system: Some("You are a helpful assistant.".to_string()),
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
        };
        let cloned = opts.clone();
        assert_eq!(opts.model, cloned.model);
        assert_eq!(opts.temperature, cloned.temperature);
        assert_eq!(opts.max_tokens, cloned.max_tokens);
        assert_eq!(opts.system, cloned.system);
    }
}
