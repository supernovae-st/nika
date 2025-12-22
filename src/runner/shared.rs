//! # Shared Agent Runner (v4.7.1)
//!
//! Runner for `agent:` tasks with shared context access.
//!
//! ## Design
//!
//! SharedAgentRunner handles agent: tasks which:
//! - Have FULL access to GlobalContext (read AND write)
//! - Share conversation history with other agent: tasks
//! - Can see outputs from all previous tasks
//!
//! ## Context Flow
//!
//! ```text
//! agent:task1 → agent:task2 → agent:task3
//!     |            |            |
//!     v            v            v
//! [GlobalContext with shared history]
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! let runner = SharedAgentRunner::new(provider, config);
//! let result = runner.execute("task-id", "Analyze this", &mut context).await?;
//! // result and history are now in context
//! ```

use crate::provider::{Provider, TokenUsage};
use crate::runner::context::{ContextReader, ContextWriter, GlobalContext, MessageRole};
use crate::runner::core::{AgentConfig, AgentCore, AgentError, AgentOutput};
use std::sync::Arc;

// ============================================================================
// TASK RESULT
// ============================================================================

/// Result from a shared agent task
#[derive(Debug, Clone)]
pub struct TaskResult {
    /// Task identifier
    pub task_id: String,

    /// Generated output
    pub output: String,

    /// Token usage
    pub usage: TokenUsage,

    /// Whether execution succeeded
    pub success: bool,
}

impl TaskResult {
    /// Create a successful result
    pub fn success(
        task_id: impl Into<String>,
        output: impl Into<String>,
        usage: TokenUsage,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            output: output.into(),
            usage,
            success: true,
        }
    }

    /// Create a failed result
    pub fn failure(task_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            output: error.into(),
            usage: TokenUsage::default(),
            success: false,
        }
    }
}

// ============================================================================
// SHARED AGENT RUNNER
// ============================================================================

/// Runner for `agent:` tasks with shared context access
///
/// SharedAgentRunner executes agent: tasks within the main agent context.
/// Key behaviors:
///
/// - Takes `&mut GlobalContext` - can READ and WRITE
/// - Gets history from `context.agent_history()`
/// - WRITES output: `context.set_output(task_id, output)`
/// - WRITES history: adds User prompt and Assistant response
///
/// ## Example
///
/// ```rust,ignore
/// let provider = create_provider("claude")?;
/// let config = AgentConfig::new("claude-sonnet-4-5")
///     .with_system_prompt("You are a helpful assistant.");
///
/// let runner = SharedAgentRunner::new(Arc::new(provider), config);
///
/// let mut context = GlobalContext::new();
/// let result = runner.execute("analyze", "Analyze this code", &mut context).await?;
///
/// // Output is now stored in context
/// assert!(context.get_output("analyze").is_some());
///
/// // History is updated with user prompt and assistant response
/// assert_eq!(context.agent_history().len(), 2);
/// ```
pub struct SharedAgentRunner {
    /// Core execution logic
    core: AgentCore,
}

impl SharedAgentRunner {
    /// Create a new SharedAgentRunner
    ///
    /// # Arguments
    /// * `provider` - The LLM provider to use
    /// * `config` - Agent configuration (model, system prompt, tools)
    pub fn new(provider: Arc<dyn Provider>, config: AgentConfig) -> Self {
        Self {
            core: AgentCore::new(provider, config),
        }
    }

    /// Execute an agent: task with FULL context access
    ///
    /// This method:
    /// 1. Gets conversation history from context
    /// 2. Executes the prompt via the provider
    /// 3. WRITES the output to context
    /// 4. WRITES the prompt and response to conversation history
    ///
    /// # Arguments
    /// * `task_id` - Unique identifier for this task
    /// * `prompt` - The prompt to execute (already resolved)
    /// * `context` - MUTABLE reference to GlobalContext
    ///
    /// # Returns
    /// TaskResult with output and usage stats
    ///
    /// # Errors
    /// Returns AgentError if execution fails
    pub async fn execute(
        &self,
        task_id: &str,
        prompt: &str,
        context: &mut GlobalContext,
    ) -> Result<TaskResult, AgentError> {
        // Get conversation history from context (for continuity)
        let history = context.agent_history().to_vec();

        // Execute via core (not isolated - shared context)
        let output: AgentOutput = self.core.execute(prompt, context, &history, false).await?;

        if output.success {
            // WRITE output to context
            context.set_output(task_id, output.content.clone());

            // WRITE to conversation history
            context.add_agent_message(MessageRole::User, prompt.to_string());
            context.add_agent_message(MessageRole::Assistant, output.content.clone());

            Ok(TaskResult::success(task_id, output.content, output.usage))
        } else {
            Ok(TaskResult::failure(task_id, output.content))
        }
    }

    /// Execute with explicit override config
    ///
    /// Use this when a task has its own model/tools configuration.
    pub async fn execute_with_config(
        &self,
        task_id: &str,
        prompt: &str,
        context: &mut GlobalContext,
        config_override: &AgentConfig,
    ) -> Result<TaskResult, AgentError> {
        // Create a temporary core with the override config
        let temp_core = AgentCore::new(self.core.provider().clone(), config_override.clone());

        let history = context.agent_history().to_vec();
        let output = temp_core.execute(prompt, context, &history, false).await?;

        if output.success {
            context.set_output(task_id, output.content.clone());
            context.add_agent_message(MessageRole::User, prompt.to_string());
            context.add_agent_message(MessageRole::Assistant, output.content.clone());

            Ok(TaskResult::success(task_id, output.content, output.usage))
        } else {
            Ok(TaskResult::failure(task_id, output.content))
        }
    }

    /// Get a reference to the underlying AgentCore
    pub fn core(&self) -> &AgentCore {
        &self.core
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::create_provider;
    use crate::runner::context::ContextReader;

    fn mock_provider() -> Arc<dyn Provider> {
        Arc::from(create_provider("mock").unwrap())
    }

    #[tokio::test]
    async fn test_shared_runner_execute() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-sonnet-4-5");
        let runner = SharedAgentRunner::new(provider, config);

        let mut context = GlobalContext::new();
        let result = runner.execute("task1", "Say hello", &mut context).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.success);
        assert_eq!(result.task_id, "task1");

        // Check output was stored
        assert!(context.get_output("task1").is_some());

        // Check history was updated (user + assistant = 2)
        assert_eq!(context.agent_history().len(), 2);
    }

    #[tokio::test]
    async fn test_shared_runner_history_continuity() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-sonnet-4-5");
        let runner = SharedAgentRunner::new(provider, config);

        let mut context = GlobalContext::new();

        // First task
        runner
            .execute("task1", "First question", &mut context)
            .await
            .unwrap();
        assert_eq!(context.agent_history().len(), 2);

        // Second task - should see first task's history
        runner
            .execute("task2", "Follow up question", &mut context)
            .await
            .unwrap();
        assert_eq!(context.agent_history().len(), 4);

        // Third task - should see all history
        runner
            .execute("task3", "Final question", &mut context)
            .await
            .unwrap();
        assert_eq!(context.agent_history().len(), 6);
    }

    #[tokio::test]
    async fn test_shared_runner_context_writes() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-sonnet-4-5");
        let runner = SharedAgentRunner::new(provider, config);

        let mut context = GlobalContext::new();

        runner
            .execute("analyze", "Analyze code", &mut context)
            .await
            .unwrap();
        runner
            .execute("summarize", "Summarize findings", &mut context)
            .await
            .unwrap();

        // Both outputs should be in context
        assert!(context.get_output("analyze").is_some());
        assert!(context.get_output("summarize").is_some());

        // History should contain both conversations
        let history = context.format_agent_history();
        assert!(history.contains("Analyze code"));
        assert!(history.contains("Summarize findings"));
    }

    #[tokio::test]
    async fn test_shared_runner_with_config_override() {
        let provider = mock_provider();
        let base_config = AgentConfig::new("claude-sonnet-4-5");
        let runner = SharedAgentRunner::new(provider, base_config);

        let mut context = GlobalContext::new();

        let override_config =
            AgentConfig::new("claude-opus-4").with_system_prompt("You are an expert");

        let result = runner
            .execute_with_config("special", "Complex task", &mut context, &override_config)
            .await;

        assert!(result.is_ok());
        assert!(context.get_output("special").is_some());
    }
}
