//! # Provider Abstraction Layer
//!
//! Trait and implementations for LLM providers.
//!
//! ## Overview
//!
//! The provider module defines how Nika communicates with LLM backends:
//!
//! - [`Provider`] - Core trait for executing prompts
//! - [`ClaudeProvider`] - Production provider using Claude CLI
//! - [`MockProvider`] - Test provider with configurable responses
//!
//! ## Provider Trait
//!
//! All providers implement the `Provider` trait:
//!
//! ```rust,ignore
//! pub trait Provider: Send + Sync {
//!     fn name(&self) -> &str;
//!     fn execute(&self, request: PromptRequest) -> Result<PromptResponse>;
//!     fn supports_tools(&self) -> bool;
//!     fn is_available(&self) -> bool;
//! }
//! ```
//!
//! ## Available Providers
//!
//! | Provider | Use Case | Features |
//! |----------|----------|----------|
//! | `claude` | Production | Real API, tool support |
//! | `mock` | Testing | Configurable responses, failure simulation |
//!
//! ## Creating Providers
//!
//! Use [`create_provider`] to instantiate a provider by name:
//!
//! ```rust
//! use nika::provider::create_provider;
//!
//! let claude = create_provider("claude");
//! assert!(claude.is_ok());
//!
//! let mock = create_provider("mock");
//! assert!(mock.is_ok());
//!
//! let unknown = create_provider("invalid");
//! assert!(unknown.is_err());
//! ```
//!
//! ## Token Estimation
//!
//! Providers estimate token usage for cost tracking:
//!
//! ```rust
//! use nika::provider::TokenUsage;
//!
//! let usage = TokenUsage::estimate(1000, 500); // 1000 char prompt, 500 char response
//! println!("Estimated {} total tokens", usage.total_tokens);
//! ```

mod claude;
mod mock;

// New providers (Phase 3)
mod mistral;
mod ollama;
mod openai;

// ============================================================================
// TOKEN ESTIMATION CONSTANTS
// ============================================================================

/// Average characters per token for mixed content (prose + code)
/// More accurate than the naive 4 chars/token estimate.
const CHARS_PER_TOKEN_MIXED: f32 = 3.0;

/// Characters per token for primarily English prose
const CHARS_PER_TOKEN_PROSE: f32 = 3.5;

/// Characters per token for primarily code content
const CHARS_PER_TOKEN_CODE: f32 = 2.5;

pub use claude::ClaudeProvider;
pub use mistral::MistralProvider;
pub use mock::MockProvider;
pub use ollama::OllamaProvider;
pub use openai::OpenAIProvider;

use crate::runner::AgentMessage;
use anyhow::Result;
use async_trait::async_trait;

// ============================================================================
// CAPABILITIES
// ============================================================================

/// Capabilities that a provider may support
#[derive(Debug, Clone, Default)]
pub struct Capabilities {
    /// Supports tool/function calling
    pub tool_use: bool,
    /// Supports image/vision input
    pub vision: bool,
    /// Supports streaming responses
    pub streaming: bool,
    /// Supports extended thinking (Claude)
    pub extended_thinking: bool,
    /// Supports JSON mode output
    pub json_mode: bool,
    /// Maximum context window size
    pub max_context: usize,
}

impl Capabilities {
    /// Claude capabilities
    pub fn claude() -> Self {
        Self {
            tool_use: true,
            vision: true,
            streaming: true,
            extended_thinking: true,
            json_mode: true,
            max_context: 200_000,
        }
    }

    /// OpenAI GPT-4o capabilities
    pub fn openai() -> Self {
        Self {
            tool_use: true,
            vision: true,
            streaming: true,
            extended_thinking: false,
            json_mode: true,
            max_context: 128_000,
        }
    }

    /// Ollama local model capabilities
    pub fn ollama() -> Self {
        Self {
            tool_use: true,
            vision: false,
            streaming: true,
            extended_thinking: false,
            json_mode: true,
            max_context: 8_192,
        }
    }

    /// Mistral capabilities
    pub fn mistral() -> Self {
        Self {
            tool_use: true,
            vision: true,
            streaming: true,
            extended_thinking: false,
            json_mode: true,
            max_context: 128_000,
        }
    }

    /// Mock provider capabilities (everything enabled)
    pub fn mock() -> Self {
        Self {
            tool_use: true,
            vision: true,
            streaming: true,
            extended_thinking: true,
            json_mode: true,
            max_context: 200_000,
        }
    }
}

// ============================================================================
// PROVIDER TRAIT (ASYNC)
// ============================================================================

/// Core trait that all LLM providers must implement
///
/// The Provider trait abstracts away the differences between various LLM APIs,
/// allowing the Runner to execute tasks without knowing which provider is being used.
///
/// All methods are async to support HTTP-based API providers.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Returns the provider name (e.g., "claude", "openai", "ollama")
    fn name(&self) -> &str;

    /// Returns the provider's capabilities
    fn capabilities(&self) -> Capabilities;

    /// Execute a prompt and return the response
    ///
    /// This is the main entry point for all LLM interactions.
    /// The provider is responsible for:
    /// - Formatting the request according to its API
    /// - Handling the conversation history for context
    /// - Managing tool execution loops (if applicable)
    /// - Returning a structured response
    async fn execute(&self, request: PromptRequest) -> Result<PromptResponse>;

    /// Check if this provider supports tool execution
    fn supports_tools(&self) -> bool {
        self.capabilities().tool_use
    }

    /// Check if this provider is available (e.g., CLI installed, API key set)
    fn is_available(&self) -> bool {
        true
    }
}

// ============================================================================
// REQUEST/RESPONSE TYPES
// ============================================================================

/// Request to execute a prompt
#[derive(Debug, Clone)]
pub struct PromptRequest {
    /// The main prompt/instruction to execute
    pub prompt: String,

    /// Optional system prompt to set context
    pub system_prompt: Option<String>,

    /// Model to use (e.g., "claude-sonnet-4-5", "gpt-4")
    pub model: String,

    /// Conversation history for context (agent: tasks share this)
    pub history: Vec<AgentMessage>,

    /// Tools the agent is allowed to use
    pub allowed_tools: Vec<String>,

    /// Whether this is an isolated execution (subagent: = true, agent: = false)
    pub is_isolated: bool,

    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,

    /// Temperature for generation (0.0 - 1.0)
    pub temperature: Option<f32>,
}

impl PromptRequest {
    /// Create a new request with minimal required fields
    pub fn new(prompt: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            model: model.into(),
            system_prompt: None,
            history: vec![],
            allowed_tools: vec![],
            is_isolated: false,
            max_tokens: None,
            temperature: None,
        }
    }

    /// Set the system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set the conversation history
    pub fn with_history(mut self, history: Vec<AgentMessage>) -> Self {
        self.history = history;
        self
    }

    /// Set allowed tools
    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.allowed_tools = tools;
        self
    }

    /// Mark as isolated (subagent mode)
    pub fn isolated(mut self) -> Self {
        self.is_isolated = true;
        self
    }
}

/// Response from a prompt execution
#[derive(Debug, Clone)]
pub struct PromptResponse {
    /// The generated content
    pub content: String,

    /// Whether the execution was successful
    pub success: bool,

    /// Token usage statistics
    pub usage: TokenUsage,

    /// Stop reason (e.g., "end_turn", "max_tokens", "tool_use")
    pub stop_reason: Option<String>,
}

impl PromptResponse {
    /// Create a successful response
    pub fn success(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            success: true,
            usage: TokenUsage::default(),
            stop_reason: Some("end_turn".to_string()),
        }
    }

    /// Create a failed response
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            content: error.into(),
            success: false,
            usage: TokenUsage::default(),
            stop_reason: Some("error".to_string()),
        }
    }

    /// Set token usage
    pub fn with_usage(mut self, usage: TokenUsage) -> Self {
        self.usage = usage;
        self
    }
}

/// Token usage statistics
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    /// Tokens in the prompt (input)
    pub prompt_tokens: u32,

    /// Tokens in the response (output)
    pub completion_tokens: u32,

    /// Total tokens used
    pub total_tokens: u32,
}

impl TokenUsage {
    pub fn new(prompt: u32, completion: u32) -> Self {
        Self {
            prompt_tokens: prompt,
            completion_tokens: completion,
            total_tokens: prompt + completion,
        }
    }

    /// Estimate usage for mixed content (when actual counts aren't available)
    ///
    /// Uses CHARS_PER_TOKEN_MIXED which is more accurate than the naive 4 chars/token.
    /// This is a reasonable default for English prose mixed with code.
    pub fn estimate(prompt_len: usize, response_len: usize) -> Self {
        Self::estimate_with_ratio(prompt_len, response_len, CHARS_PER_TOKEN_MIXED)
    }

    /// Estimate usage for primarily English prose
    pub fn estimate_prose(prompt_len: usize, response_len: usize) -> Self {
        Self::estimate_with_ratio(prompt_len, response_len, CHARS_PER_TOKEN_PROSE)
    }

    /// Estimate usage for primarily code content
    pub fn estimate_code(prompt_len: usize, response_len: usize) -> Self {
        Self::estimate_with_ratio(prompt_len, response_len, CHARS_PER_TOKEN_CODE)
    }

    /// Estimate with a custom chars-per-token ratio
    fn estimate_with_ratio(prompt_len: usize, response_len: usize, chars_per_token: f32) -> Self {
        let prompt_tokens = (prompt_len as f32 / chars_per_token).ceil() as u32;
        let completion_tokens = (response_len as f32 / chars_per_token).ceil() as u32;
        Self::new(prompt_tokens, completion_tokens)
    }
}

// ============================================================================
// PROVIDER FACTORY
// ============================================================================

/// Create a provider instance by name
///
/// # Supported Providers
///
/// | Name | Description | Requires |
/// |------|-------------|----------|
/// | `claude` | Claude CLI | `claude` CLI installed |
/// | `openai` | OpenAI API | `OPENAI_API_KEY` env var |
/// | `ollama` | Local Ollama | Ollama running locally |
/// | `mistral` | Mistral AI | `MISTRAL_API_KEY` env var |
/// | `mock` | Testing | Nothing |
pub fn create_provider(name: &str) -> Result<Box<dyn Provider>> {
    match name.to_lowercase().as_str() {
        "claude" => Ok(Box::new(ClaudeProvider::new())),
        "openai" => Ok(Box::new(OpenAIProvider::new()?)),
        "ollama" => Ok(Box::new(OllamaProvider::new())),
        "mistral" => Ok(Box::new(MistralProvider::new()?)),
        "mock" => Ok(Box::new(MockProvider::new())),
        _ => anyhow::bail!(
            "Unknown provider: '{}'. Available: claude, openai, ollama, mistral, mock",
            name
        ),
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_request_builder() {
        let req = PromptRequest::new("Hello", "claude-sonnet-4-5")
            .with_system_prompt("You are helpful")
            .with_tools(vec!["Read".to_string()])
            .isolated();

        assert_eq!(req.prompt, "Hello");
        assert_eq!(req.model, "claude-sonnet-4-5");
        assert_eq!(req.system_prompt, Some("You are helpful".to_string()));
        assert_eq!(req.allowed_tools, vec!["Read"]);
        assert!(req.is_isolated);
    }

    #[test]
    fn test_prompt_response_success() {
        let resp = PromptResponse::success("Generated text");

        assert!(resp.success);
        assert_eq!(resp.content, "Generated text");
        assert_eq!(resp.stop_reason, Some("end_turn".to_string()));
    }

    #[test]
    fn test_token_usage_estimate() {
        // With ratio 3.0: 300/3 = 100, 150/3 = 50
        let usage = TokenUsage::estimate(300, 150);

        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
    }

    #[test]
    fn test_token_usage_estimate_prose() {
        // With ratio 3.5: 350/3.5 = 100, 175/3.5 = 50
        let usage = TokenUsage::estimate_prose(350, 175);

        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
    }

    #[test]
    fn test_token_usage_estimate_code() {
        // With ratio 2.5: 250/2.5 = 100, 125/2.5 = 50
        let usage = TokenUsage::estimate_code(250, 125);

        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
    }

    #[test]
    fn test_create_provider_mock() {
        let provider = create_provider("mock").unwrap();
        assert_eq!(provider.name(), "mock");
    }

    #[test]
    fn test_create_provider_claude() {
        let provider = create_provider("claude").unwrap();
        assert_eq!(provider.name(), "claude");
    }

    #[test]
    fn test_create_provider_unknown() {
        let result = create_provider("unknown");
        assert!(result.is_err());
    }
}
