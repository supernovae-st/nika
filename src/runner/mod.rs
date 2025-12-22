//! # Nika Workflow Runner (v4.6)
//!
//! Executes workflows using providers (Claude CLI, mock, etc.).
//!
//! ## Architecture
//!
//! The runner module is organized into:
//!
//! - [`context`] - GlobalContext and LocalContext for state management
//! - [`core`] - AgentCore with shared execution logic
//! - [`shared`] - SharedAgentRunner for agent: tasks
//! - [`isolated`] - IsolatedAgentRunner for subagent: tasks
//!
//! ## v4.6 Performance Optimizations
//!
//! - Single-pass template resolution (template.rs)
//! - SmartString for task IDs (inline <= 31 chars)
//! - Arc<str> for zero-copy context sharing
//! - Memory pool for ExecutionContext reuse
//!
//! ## Context Model
//!
//! ```text
//! +-----------------------------------------------------------+
//! |                    GLOBAL CONTEXT                          |
//! |  - Shared across all agent: tasks                         |
//! |  - Accumulates conversation history                        |
//! |  - Stores all task outputs                                 |
//! +-----------------------------------------------------------+
//!     |                                    ^
//!     | snapshot (read-only)               | bridge (explicit)
//!     v                                    |
//! +-----------------------------------------------------------+
//! |                    LOCAL CONTEXT                           |
//! |  - Created per subagent: task                             |
//! |  - Fresh history (isolated)                                |
//! |  - Local outputs (not visible to main)                     |
//! +-----------------------------------------------------------+
//! ```
//!
//! ## Runners
//!
//! | Runner | Context | History | Writes To |
//! |--------|---------|---------|-----------|
//! | `SharedAgentRunner` | `&mut GlobalContext` | Shared | GlobalContext |
//! | `IsolatedAgentRunner` | `&GlobalContext` | Empty | LocalContext |
//!
//! ## Compile-Time Safety
//!
//! The key safety guarantee is in the function signatures:
//!
//! ```rust,ignore
//! // SharedAgentRunner - can write
//! pub async fn execute(&self, ..., context: &mut GlobalContext) -> Result<TaskResult, _>
//!
//! // IsolatedAgentRunner - cannot write (compile-time enforced!)
//! pub async fn execute(&self, ..., context: &GlobalContext) -> Result<SubagentResult, _>
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika::runner::{GlobalContext, SharedAgentRunner, IsolatedAgentRunner};
//! use nika::runner::core::AgentConfig;
//! use nika::provider::create_provider;
//!
//! // Create provider
//! let provider = Arc::new(create_provider("claude")?);
//!
//! // Create runners
//! let config = AgentConfig::new("claude-sonnet-4-5")
//!     .with_system_prompt("You are helpful.");
//! let shared_runner = SharedAgentRunner::new(provider.clone(), config.clone());
//! let isolated_runner = IsolatedAgentRunner::new(provider, config);
//!
//! // Execute agent: task (shared context)
//! let mut context = GlobalContext::new();
//! let result = shared_runner.execute("analyze", "Analyze this", &mut context).await?;
//!
//! // Execute subagent: task (isolated context)
//! let result = isolated_runner.execute("audit", "Security audit", &context).await?;
//! // Note: result.output is NOT in context - must bridge explicitly
//! ```

pub mod context;
pub mod core;
pub mod isolated;
pub mod shared;
pub mod workflow;

// Re-export main types from submodules
pub use context::{
    AgentMessage, ContextReader, ContextWriter, GlobalContext, LocalContext, MessageRole,
};
pub use core::{AgentConfig, AgentCore, AgentError, AgentOutput};
pub use isolated::{IsolatedAgentRunner, SubagentResult};
pub use shared::SharedAgentRunner;

// Re-export workflow runner types
pub use workflow::{ErrorCategory, ErrorContext, RunResult, Runner, TaskResult};

// ============================================================================
// BACKWARD COMPATIBILITY
// ============================================================================

// Re-export ExecutionContext as GlobalContext for backward compatibility
// This allows existing code to continue working
pub type ExecutionContext = GlobalContext;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::create_provider;
    use std::sync::Arc;

    fn mock_provider() -> Arc<dyn crate::provider::Provider> {
        Arc::from(create_provider("mock").unwrap())
    }

    #[tokio::test]
    async fn test_shared_vs_isolated_context_access() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-sonnet-4-5");

        let shared_runner = SharedAgentRunner::new(provider.clone(), config.clone());
        let isolated_runner = IsolatedAgentRunner::new(provider, config);

        let mut global_context = GlobalContext::new();

        // Shared runner can write to global context
        shared_runner
            .execute("agent-task", "Do something", &mut global_context)
            .await
            .unwrap();

        // Output is in global context
        assert!(global_context.get_output("agent-task").is_some());

        // History is updated
        assert_eq!(global_context.agent_history().len(), 2);

        // Isolated runner cannot write to global context (by design)
        let subagent_result = isolated_runner
            .execute("subagent-task", "Isolated work", &global_context)
            .await
            .unwrap();

        // Output is NOT in global context
        assert!(global_context.get_output("subagent-task").is_none());

        // But it IS in the result's local context
        assert!(subagent_result
            .local_context
            .get_output("subagent-task")
            .is_some());

        // Global history unchanged (still 2, not 4)
        assert_eq!(global_context.agent_history().len(), 2);
    }

    #[tokio::test]
    async fn test_bridge_pattern() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-sonnet-4-5");

        let shared_runner = SharedAgentRunner::new(provider.clone(), config.clone());
        let isolated_runner = IsolatedAgentRunner::new(provider, config);

        let mut global_context = GlobalContext::new();

        // Step 1: agent: task
        shared_runner
            .execute("analyze", "Analyze code", &mut global_context)
            .await
            .unwrap();

        // Step 2: subagent: task (reads analyze output, writes to local)
        let subagent_result = isolated_runner
            .execute("deep-audit", "Deep security audit", &global_context)
            .await
            .unwrap();

        // Step 3: Bridge - function: would do this, but we simulate it
        // This is the "bridge pattern" - explicit data transfer
        global_context.set_output("deep-audit", subagent_result.output.clone());

        // Step 4: agent: task can now see subagent output
        assert!(global_context.get_output("deep-audit").is_some());
    }

    #[tokio::test]
    async fn test_isolation_guarantees() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-sonnet-4-5");

        let isolated_runner = IsolatedAgentRunner::new(provider, config);

        // Set up global context with existing data
        let mut global_context = GlobalContext::new();
        global_context.set_output("existing-task", "Existing output".to_string());
        global_context.add_agent_message(MessageRole::User, "Previous question".to_string());
        global_context.add_agent_message(MessageRole::Assistant, "Previous answer".to_string());

        // Execute isolated task
        let result = isolated_runner
            .execute("isolated", "New work", &global_context)
            .await
            .unwrap();

        // Subagent can READ existing outputs
        assert_eq!(
            result.local_context.get_output("existing-task"),
            Some("Existing output")
        );

        // But has FRESH history (doesn't see previous conversation)
        assert_eq!(result.local_context.local_history().len(), 2); // Only its own messages
    }
}
