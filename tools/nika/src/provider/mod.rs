//! Provider abstraction layer
//!
//! ## Provider Strategy (v0.3.1)
//!
//! Nika is migrating from custom providers to [rig-core](https://github.com/0xPlaygrounds/rig).
//!
//! | Component | Old Approach | New Approach (v0.3.1+) |
//! |-----------|--------------|------------------------|
//! | `agent:` verb | `AgentLoop` + `Provider` trait | [`RigAgentLoop`](crate::runtime::RigAgentLoop) + rig-core |
//! | `infer:` verb | `executor.rs` + `Provider` trait | Pending migration to rig |
//! | Tool calling | Manual JSON construction | [`NikaMcpTool`](rig::NikaMcpTool) (rig `ToolDyn`) |
//!
//! ## Current Modules
//!
//! - [`rig`] - **Recommended**: rig-core integration (`RigProvider`, `NikaMcpTool`)
//! - [`claude`] - *Deprecated*: Legacy Claude provider (use rig for new code)
//! - [`openai`] - *Deprecated*: Legacy OpenAI provider (use rig for new code)
//! - [`types`] - *Deprecated*: Legacy types (use rig-core types)
//!
//! ## Example: Using rig-core (Recommended)
//!
//! ```rust,ignore
//! use nika::runtime::RigAgentLoop;
//! use nika::ast::AgentParams;
//! use nika::event::EventLog;
//!
//! let params = AgentParams {
//!     prompt: "Generate a landing page".to_string(),
//!     mcp: vec!["novanet".to_string()],
//!     max_turns: Some(5),
//!     ..Default::default()
//! };
//! let agent = RigAgentLoop::new("task-1".into(), params, EventLog::new(), mcp_clients)?;
//! let result = agent.run_claude().await?;
//! ```

// Allow deprecated during transition period (these are still used by executor.rs)
#[allow(deprecated)]
pub mod claude;
#[allow(deprecated)]
pub mod openai;
pub mod rig;
mod types;

#[allow(deprecated)]
pub use claude::ClaudeProvider;
#[allow(deprecated)]
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
///
/// **Note:** This function uses deprecated providers during transition.
/// For new agent code, use [`crate::runtime::RigAgentLoop`] instead.
#[allow(deprecated)]
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
