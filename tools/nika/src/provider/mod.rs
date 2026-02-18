//! Provider abstraction layer

pub mod claude;
pub mod openai;
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
    /// Execute a prompt and return the response
    async fn infer(&self, prompt: &str, model: &str) -> Result<String>;

    /// Default model for this provider
    fn default_model(&self) -> &str;
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

    fn default_model(&self) -> &str {
        "mock-v1"
    }
}
