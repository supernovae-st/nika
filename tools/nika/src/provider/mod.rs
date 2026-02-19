//! Provider abstraction layer

pub mod claude;
pub mod openai;
pub mod rig;
mod types;

pub use claude::ClaudeProvider;
pub use openai::OpenAIProvider;
pub use types::*;

use anyhow::Result;
use async_trait::async_trait;

/// Default models per provider
pub const CLAUDE_DEFAULT_MODEL: &str = "claude-sonnet-4-5";
pub const OPENAI_DEFAULT_MODEL: &str = "gpt-4o";

/// LLM provider abstraction for inference operations
///
/// Implementations:
/// - [`ClaudeProvider`]: Anthropic Claude API
/// - [`OpenAIProvider`]: OpenAI API
/// - [`MockProvider`]: Testing mock (returns "Mock response")
///
/// # Example
/// ```rust,ignore
/// let provider = create_provider("claude")?;
/// let response = provider.infer("Hello", "claude-sonnet-4-5").await?;
/// ```
#[async_trait]
pub trait Provider: Send + Sync {
    /// Execute a prompt and return the response (simple, no tools)
    async fn infer(&self, prompt: &str, model: &str) -> Result<String>;

    /// Chat with tool support for multi-turn conversations
    ///
    /// # Arguments
    /// * `messages` - Conversation history
    /// * `tools` - Optional tool definitions available to the LLM
    /// * `model` - Model identifier to use
    ///
    /// # Returns
    /// A `ChatResponse` containing the assistant's reply and any tool calls
    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        model: &str,
    ) -> Result<ChatResponse>;

    /// Default model for this provider
    fn default_model(&self) -> &str;

    /// Get the provider name (e.g., "claude", "openai", "mock")
    fn name(&self) -> &str;

    /// Get the current model identifier
    fn model(&self) -> &str {
        self.default_model()
    }
}

/// Create provider by name
pub fn create_provider(name: &str) -> Result<Box<dyn Provider>> {
    if name.eq_ignore_ascii_case("claude") {
        Ok(Box::new(ClaudeProvider::new()?))
    } else if name.eq_ignore_ascii_case("openai") {
        Ok(Box::new(OpenAIProvider::new()?))
    } else if name.eq_ignore_ascii_case("mock") {
        Ok(Box::new(MockProvider))
    } else {
        anyhow::bail!(
            "Unknown provider: '{}'. Available: claude, openai, mock",
            name
        )
    }
}

/// Mock provider for testing
#[derive(Default)]
pub struct MockProvider;

#[async_trait]
impl Provider for MockProvider {
    async fn infer(&self, _prompt: &str, _model: &str) -> Result<String> {
        Ok("Mock response".to_string())
    }

    async fn chat(
        &self,
        _messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
        _model: &str,
    ) -> Result<ChatResponse> {
        Ok(ChatResponse {
            content: MessageContent::Text("Mock response".to_string()),
            tool_calls: vec![],
            stop_reason: StopReason::EndTurn,
            usage: Usage::new(10, 10),
        })
    }

    fn default_model(&self) -> &str {
        "mock-v1"
    }

    fn name(&self) -> &str {
        "mock"
    }
}
