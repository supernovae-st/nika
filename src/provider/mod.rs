//! Provider abstraction layer for Nika runtime
//!
//! This module defines the trait that all LLM providers must implement,
//! enabling Nika to work with Claude, OpenAI, Ollama, and others.

mod claude;
mod mock;

pub use claude::ClaudeProvider;
pub use mock::MockProvider;

use crate::runner::AgentMessage;
use anyhow::Result;

// ============================================================================
// PROVIDER TRAIT
// ============================================================================

/// Core trait that all LLM providers must implement
///
/// The Provider trait abstracts away the differences between various LLM APIs,
/// allowing the Runner to execute tasks without knowing which provider is being used.
pub trait Provider: Send + Sync {
    /// Returns the provider name (e.g., "claude", "openai", "ollama")
    fn name(&self) -> &str;

    /// Execute a prompt and return the response
    ///
    /// This is the main entry point for all LLM interactions.
    /// The provider is responsible for:
    /// - Formatting the request according to its API
    /// - Handling the conversation history for context
    /// - Managing tool execution loops (if applicable)
    /// - Returning a structured response
    fn execute(&self, request: PromptRequest) -> Result<PromptResponse>;

    /// Check if this provider supports tool execution
    fn supports_tools(&self) -> bool {
        false
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

    /// Estimate usage (when actual counts aren't available)
    pub fn estimate(prompt_len: usize, response_len: usize) -> Self {
        // Rough estimate: ~4 chars per token
        let prompt_tokens = (prompt_len / 4) as u32;
        let completion_tokens = (response_len / 4) as u32;
        Self::new(prompt_tokens, completion_tokens)
    }
}

// ============================================================================
// PROVIDER FACTORY
// ============================================================================

/// Create a provider instance by name
pub fn create_provider(name: &str) -> Result<Box<dyn Provider>> {
    match name.to_lowercase().as_str() {
        "claude" => Ok(Box::new(ClaudeProvider::new())),
        "mock" => Ok(Box::new(MockProvider::new())),
        _ => anyhow::bail!("Unknown provider: {}", name),
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
        let usage = TokenUsage::estimate(400, 200); // ~100 + ~50 tokens

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
