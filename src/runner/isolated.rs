//! # Isolated Agent Runner (v4.6)
//!
//! Runner for `subagent:` tasks with isolated context.
//!
//! ## Design
//!
//! IsolatedAgentRunner handles subagent: tasks which:
//! - Have READ-ONLY access to GlobalContext (via snapshot)
//! - Get their own isolated 200K context window
//! - Do NOT share conversation history with main agent
//! - Must bridge back via tools to affect main context
//!
//! ## Context Flow
//!
//! ```text
//! GlobalContext
//!     |
//!     v (snapshot - READ ONLY)
//! LocalContext ← subagent writes here
//!     |
//!     v (bridge pattern)
//! function: → GlobalContext (explicit bridge)
//! ```
//!
//! ## Compile-Time Safety
//!
//! The key safety guarantee is in the function signature:
//! ```rust,ignore
//! pub async fn execute(
//!     &self,
//!     task_id: &str,
//!     prompt: &str,
//!     context: &GlobalContext,  // IMMUTABLE! Cannot write!
//! ) -> Result<SubagentResult, AgentError>
//! ```
//!
//! This is enforced at compile-time by Rust's borrow checker.

use crate::provider::{Provider, TokenUsage};
use crate::runner::context::{GlobalContext, LocalContext, MessageRole};
use crate::runner::core::{AgentConfig, AgentCore, AgentError};
use std::sync::Arc;

// ============================================================================
// SUBAGENT RESULT
// ============================================================================

/// Result from a subagent execution
///
/// Unlike TaskResult, SubagentResult includes the LocalContext
/// which contains all the subagent's local outputs and history.
///
/// The caller (workflow executor) decides whether to bridge
/// this back to GlobalContext.
#[derive(Debug)]
pub struct SubagentResult {
    /// Task identifier
    pub task_id: String,

    /// Generated output
    pub output: String,

    /// Token usage
    pub usage: TokenUsage,

    /// Whether execution succeeded
    pub success: bool,

    /// The isolated local context (for inspection or bridging)
    pub local_context: LocalContext,
}

impl SubagentResult {
    /// Create a successful result
    pub fn success(
        task_id: impl Into<String>,
        output: impl Into<String>,
        usage: TokenUsage,
        local_context: LocalContext,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            output: output.into(),
            usage,
            success: true,
            local_context,
        }
    }

    /// Create a failed result
    pub fn failure(
        task_id: impl Into<String>,
        error: impl Into<String>,
        local_context: LocalContext,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            output: error.into(),
            usage: TokenUsage::default(),
            success: false,
            local_context,
        }
    }

    /// Get the final output (convenience method)
    pub fn final_output(&self) -> &str {
        &self.output
    }

    /// Check if the subagent has local outputs that need bridging
    pub fn has_local_outputs(&self) -> bool {
        !self.local_context.local_outputs().is_empty()
    }
}

// ============================================================================
// ISOLATED AGENT RUNNER
// ============================================================================

/// Runner for `subagent:` tasks with isolated context
///
/// IsolatedAgentRunner executes subagent: tasks in isolation:
///
/// - Takes `&GlobalContext` - IMMUTABLE, cannot write!
/// - Creates a snapshot (LocalContext) for the subagent
/// - Executes with EMPTY history (isolated)
/// - Returns SubagentResult with local context
///
/// ## Compile-Time Safety
///
/// The signature `context: &GlobalContext` (not `&mut`) ensures
/// the subagent cannot modify the global context at compile-time.
///
/// ## Example
///
/// ```rust,ignore
/// let provider = create_provider("claude")?;
/// let config = AgentConfig::new("claude-opus-4")
///     .with_max_turns(20);
///
/// let runner = IsolatedAgentRunner::new(Arc::new(provider), config);
///
/// let context = GlobalContext::new();
/// let result = runner.execute("security-audit", "Deep audit", &context).await?;
///
/// // Result contains isolated local context
/// println!("Output: {}", result.output);
///
/// // Caller must explicitly bridge if needed
/// if result.success {
///     // Bridge via function: or manual copy
/// }
/// ```
pub struct IsolatedAgentRunner {
    /// Core execution logic
    core: AgentCore,
}

impl IsolatedAgentRunner {
    /// Create a new IsolatedAgentRunner
    ///
    /// # Arguments
    /// * `provider` - The LLM provider to use
    /// * `config` - Agent configuration (model, system prompt, tools)
    pub fn new(provider: Arc<dyn Provider>, config: AgentConfig) -> Self {
        Self {
            core: AgentCore::new(provider, config),
        }
    }

    /// Execute a subagent: task with READ-ONLY context
    ///
    /// This method:
    /// 1. Creates a snapshot of GlobalContext (LocalContext)
    /// 2. Executes with EMPTY history (isolation)
    /// 3. Stores output in LocalContext only
    /// 4. Returns SubagentResult (caller must bridge to GlobalContext)
    ///
    /// # Arguments
    /// * `task_id` - Unique identifier for this task
    /// * `prompt` - The prompt to execute (already resolved)
    /// * `context` - IMMUTABLE reference to GlobalContext
    ///
    /// # Returns
    /// SubagentResult with output, usage, and local context
    ///
    /// # Errors
    /// Returns AgentError if execution fails
    ///
    /// # Compile-Time Safety
    ///
    /// Note the signature: `context: &GlobalContext` (not `&mut`).
    /// This is the compile-time guarantee that subagents cannot
    /// modify the global context directly.
    pub async fn execute(
        &self,
        task_id: &str,
        prompt: &str,
        context: &GlobalContext,
    ) -> Result<SubagentResult, AgentError> {
        // Create isolated snapshot
        let mut local_ctx = context.snapshot();

        // Execute with EMPTY history (isolation)
        // The subagent gets a fresh context, not the main agent's history
        let empty_history = vec![];
        let output = self
            .core
            .execute(prompt, &local_ctx, &empty_history, true)
            .await?;

        if output.success {
            // Store in local context only
            local_ctx.set_local_output(task_id, output.content.clone());

            // Add to local history for debugging
            local_ctx.add_local_message(MessageRole::User, prompt.to_string());
            local_ctx.add_local_message(MessageRole::Assistant, output.content.clone());

            Ok(SubagentResult::success(
                task_id,
                output.content,
                output.usage,
                local_ctx,
            ))
        } else {
            Ok(SubagentResult::failure(task_id, output.content, local_ctx))
        }
    }

    /// Execute with explicit override config
    ///
    /// Use this when a subagent task has its own model/tools configuration.
    pub async fn execute_with_config(
        &self,
        task_id: &str,
        prompt: &str,
        context: &GlobalContext,
        config_override: &AgentConfig,
    ) -> Result<SubagentResult, AgentError> {
        let mut local_ctx = context.snapshot();

        // Create temporary core with override config
        let temp_core = AgentCore::new(self.core.provider().clone(), config_override.clone());

        let empty_history = vec![];
        let output = temp_core
            .execute(prompt, &local_ctx, &empty_history, true)
            .await?;

        if output.success {
            local_ctx.set_local_output(task_id, output.content.clone());
            local_ctx.add_local_message(MessageRole::User, prompt.to_string());
            local_ctx.add_local_message(MessageRole::Assistant, output.content.clone());

            Ok(SubagentResult::success(
                task_id,
                output.content,
                output.usage,
                local_ctx,
            ))
        } else {
            Ok(SubagentResult::failure(task_id, output.content, local_ctx))
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
    use crate::runner::context::{ContextReader, ContextWriter};

    fn mock_provider() -> Arc<dyn Provider> {
        Arc::from(create_provider("mock").unwrap())
    }

    #[tokio::test]
    async fn test_isolated_runner_execute() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-opus-4");
        let runner = IsolatedAgentRunner::new(provider, config);

        let context = GlobalContext::new();
        let result = runner
            .execute("security-audit", "Audit this code", &context)
            .await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.success);
        assert_eq!(result.task_id, "security-audit");
    }

    #[tokio::test]
    async fn test_isolated_runner_does_not_modify_global() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-opus-4");
        let runner = IsolatedAgentRunner::new(provider, config);

        let context = GlobalContext::new();

        // Execute subagent
        let result = runner
            .execute("isolated-task", "Do something", &context)
            .await
            .unwrap();

        // Global context should NOT have the output
        assert!(context.get_output("isolated-task").is_none());

        // Local context in result SHOULD have it
        assert!(result.local_context.get_output("isolated-task").is_some());
    }

    #[tokio::test]
    async fn test_isolated_runner_empty_history() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-opus-4");
        let runner = IsolatedAgentRunner::new(provider, config);

        // Set up global context with history
        let mut global = GlobalContext::new();
        global.add_agent_message(MessageRole::User, "Previous question".to_string());
        global.add_agent_message(MessageRole::Assistant, "Previous answer".to_string());

        // Execute subagent - should NOT see global history
        let result = runner
            .execute("sub-task", "New question", &global)
            .await
            .unwrap();

        // Local context should only have 2 messages (from this execution)
        // Not 4 (which would include the global history)
        assert_eq!(result.local_context.local_history().len(), 2);
    }

    #[tokio::test]
    async fn test_isolated_runner_reads_global_outputs() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-opus-4");
        let runner = IsolatedAgentRunner::new(provider, config);

        // Set up global context with an output
        let mut global = GlobalContext::new();
        global.set_output("previous-task", "Previous output".to_string());

        // Execute subagent
        let result = runner
            .execute("sub-task", "Process previous", &global)
            .await
            .unwrap();

        // Subagent should be able to READ the previous output
        assert_eq!(
            result.local_context.get_output("previous-task"),
            Some("Previous output")
        );
    }

    #[tokio::test]
    async fn test_subagent_result_bridging() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-opus-4");
        let runner = IsolatedAgentRunner::new(provider, config);

        let context = GlobalContext::new();
        let result = runner
            .execute("analysis", "Analyze deeply", &context)
            .await
            .unwrap();

        // Result should indicate local outputs exist
        assert!(result.has_local_outputs());

        // In a real workflow, the caller would bridge:
        // context.set_output("analysis", result.output.clone());
    }

    #[tokio::test]
    async fn test_isolated_runner_with_config_override() {
        let provider = mock_provider();
        let base_config = AgentConfig::new("claude-sonnet-4-5");
        let runner = IsolatedAgentRunner::new(provider, base_config);

        let context = GlobalContext::new();

        let override_config = AgentConfig::new("claude-opus-4")
            .with_system_prompt("You are a security expert")
            .with_max_turns(50);

        let result = runner
            .execute_with_config("deep-audit", "Thorough audit", &context, &override_config)
            .await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.success);
    }
}
