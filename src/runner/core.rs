//! # Agent Core (v4.6)
//!
//! Shared execution logic for agent: and subagent: tasks.
//!
//! The AgentCore handles the common aspects of LLM interaction:
//! - Building prompt requests
//! - Executing via provider
//! - Processing responses
//!
//! The difference between agent:/subagent: is handled by the runners.

use crate::provider::{PromptRequest, PromptResponse, Provider, TokenUsage};
use crate::runner::context::{AgentMessage, ContextReader};
use std::sync::Arc;
use thiserror::Error;

// ============================================================================
// AGENT ERRORS
// ============================================================================

/// Errors that can occur during agent execution
#[derive(Error, Debug)]
pub enum AgentError {
    /// Provider execution failed
    #[error("Provider error: {0}")]
    Provider(String),

    /// Template resolution failed
    #[error("Template error: {0}")]
    Template(String),

    /// Configuration error
    #[error("Config error: {0}")]
    Config(String),

    /// Timeout during execution
    #[error("Timeout: {0}")]
    Timeout(String),
}

impl AgentError {
    /// Check if this is a timeout error
    pub fn is_timeout(&self) -> bool {
        matches!(self, AgentError::Timeout(_))
    }
}

// ============================================================================
// AGENT CONFIG
// ============================================================================

/// Configuration for agent execution
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Model to use (e.g., "claude-sonnet-4-5")
    pub model: String,

    /// System prompt
    pub system_prompt: Option<String>,

    /// Allowed tools
    pub allowed_tools: Vec<String>,

    /// Maximum turns for agentic loops
    pub max_turns: Option<u32>,
}

impl AgentConfig {
    /// Create a new config with just the model
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            system_prompt: None,
            allowed_tools: Vec::new(),
            max_turns: None,
        }
    }

    /// Set the system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set allowed tools
    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.allowed_tools = tools;
        self
    }

    /// Set max turns
    pub fn with_max_turns(mut self, turns: u32) -> Self {
        self.max_turns = Some(turns);
        self
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self::new("claude-sonnet-4-5")
    }
}

// ============================================================================
// AGENT OUTPUT
// ============================================================================

/// Output from agent execution
#[derive(Debug, Clone)]
pub struct AgentOutput {
    /// The generated content
    pub content: String,

    /// Token usage
    pub usage: TokenUsage,

    /// Whether execution was successful
    pub success: bool,

    /// Stop reason (e.g., "end_turn", "max_tokens")
    pub stop_reason: Option<String>,
}

impl AgentOutput {
    /// Create a successful output
    pub fn success(content: impl Into<String>, usage: TokenUsage) -> Self {
        Self {
            content: content.into(),
            usage,
            success: true,
            stop_reason: Some("end_turn".to_string()),
        }
    }

    /// Create a failed output
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            content: error.into(),
            usage: TokenUsage::default(),
            success: false,
            stop_reason: Some("error".to_string()),
        }
    }
}

// ============================================================================
// AGENT CORE
// ============================================================================

/// Core execution logic shared between SharedAgentRunner and IsolatedAgentRunner
///
/// AgentCore handles:
/// - Building PromptRequest from config and context
/// - Executing via the provider
/// - Converting PromptResponse to AgentOutput
pub struct AgentCore {
    /// The LLM provider
    provider: Arc<dyn Provider>,

    /// Base configuration
    config: AgentConfig,
}

impl AgentCore {
    /// Create a new AgentCore with provider and config
    pub fn new(provider: Arc<dyn Provider>, config: AgentConfig) -> Self {
        Self { provider, config }
    }

    /// Execute a prompt with the given context and history
    ///
    /// # Arguments
    /// * `prompt` - The resolved prompt text
    /// * `context` - Context for template resolution (implements ContextReader)
    /// * `history` - Conversation history to include
    /// * `isolated` - Whether this is an isolated execution (subagent)
    ///
    /// # Returns
    /// AgentOutput with the generated content and usage stats
    pub async fn execute<C: ContextReader>(
        &self,
        prompt: &str,
        _context: &C,
        history: &[AgentMessage],
        isolated: bool,
    ) -> Result<AgentOutput, AgentError> {
        // Build request
        let mut request = PromptRequest::new(prompt, &self.config.model);

        // Set system prompt if configured
        if let Some(ref system) = self.config.system_prompt {
            request = request.with_system_prompt(system);
        }

        // Set history (for agent: tasks this includes shared history)
        if !history.is_empty() {
            request = request.with_history(history.to_vec());
        }

        // Set allowed tools
        if !self.config.allowed_tools.is_empty() {
            request = request.with_tools(self.config.allowed_tools.clone());
        }

        // Mark as isolated for subagent
        if isolated {
            request = request.isolated();
        }

        // Execute via provider
        let response: PromptResponse = self
            .provider
            .execute(request)
            .await
            .map_err(|e| AgentError::Provider(e.to_string()))?;

        // Convert response to output
        if response.success {
            Ok(AgentOutput {
                content: response.content,
                usage: response.usage,
                success: true,
                stop_reason: response.stop_reason,
            })
        } else {
            // Check for timeout
            if response.content.contains("timed out") {
                Err(AgentError::Timeout(response.content))
            } else {
                Ok(AgentOutput::failure(response.content))
            }
        }
    }

    /// Get the provider reference
    pub fn provider(&self) -> &Arc<dyn Provider> {
        &self.provider
    }

    /// Get the config reference
    pub fn config(&self) -> &AgentConfig {
        &self.config
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::create_provider;
    use crate::runner::context::GlobalContext;

    fn mock_provider() -> Arc<dyn Provider> {
        Arc::from(create_provider("mock").unwrap())
    }

    #[test]
    fn test_agent_config_builder() {
        let config = AgentConfig::new("claude-opus-4")
            .with_system_prompt("You are helpful")
            .with_tools(vec!["Read".to_string(), "Write".to_string()])
            .with_max_turns(10);

        assert_eq!(config.model, "claude-opus-4");
        assert_eq!(config.system_prompt, Some("You are helpful".to_string()));
        assert_eq!(config.allowed_tools, vec!["Read", "Write"]);
        assert_eq!(config.max_turns, Some(10));
    }

    #[test]
    fn test_agent_output_success() {
        let usage = TokenUsage::new(100, 50);
        let output = AgentOutput::success("Hello!", usage.clone());

        assert!(output.success);
        assert_eq!(output.content, "Hello!");
        assert_eq!(output.usage.total_tokens, 150);
    }

    #[test]
    fn test_agent_output_failure() {
        let output = AgentOutput::failure("Something went wrong");

        assert!(!output.success);
        assert_eq!(output.content, "Something went wrong");
    }

    #[tokio::test]
    async fn test_agent_core_execute() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-sonnet-4-5");
        let core = AgentCore::new(provider, config);

        let ctx = GlobalContext::new();
        let history: Vec<AgentMessage> = vec![];

        let result = core.execute("Say hello", &ctx, &history, false).await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.success);
        assert!(output.content.contains("Mock"));
    }

    #[tokio::test]
    async fn test_agent_core_execute_isolated() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-sonnet-4-5");
        let core = AgentCore::new(provider, config);

        let ctx = GlobalContext::new();
        let history: Vec<AgentMessage> = vec![];

        let result = core.execute("Analyze this", &ctx, &history, true).await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.success);
    }
}
