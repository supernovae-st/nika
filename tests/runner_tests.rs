//! # Runner Module Tests (v4.7.1)
//!
//! Comprehensive tests for the runner module:
//! - SharedAgentRunner: writes to GlobalContext
//! - IsolatedAgentRunner: returns LocalContext (read-only)
//! - GlobalContext: implements ContextReader + ContextWriter
//! - LocalContext: implements ContextReader only
//! - WorkflowRunner: dispatches to correct runner based on task type
//!
//! ## Test Categories
//!
//! 1. SharedAgentRunner tests - verify writes to GlobalContext
//! 2. IsolatedAgentRunner tests - verify read-only behavior
//! 3. Context trait tests - verify Reader/Writer implementations
//! 4. WorkflowRunner dispatch tests - verify correct routing
//! 5. Bridge pattern tests - verify subagent → function → agent flow

use nika::{
    AgentConfig, ContextReader, ContextWriter, GlobalContext, IsolatedAgentRunner, MessageRole,
    Runner, SharedAgentRunner, Workflow,
};
use std::collections::HashMap;
use std::sync::Arc;

// ============================================================================
// TEST HELPERS
// ============================================================================

fn mock_provider() -> Arc<dyn nika::provider::Provider> {
    Arc::from(nika::provider::create_provider("mock").unwrap())
}

fn make_basic_workflow() -> Workflow {
    let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test workflow"

tasks:
  - id: task1
    agent:
      prompt: "First task"

flows: []
"#;
    serde_yaml::from_str(yaml).unwrap()
}

fn make_agent_subagent_workflow() -> Workflow {
    let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test workflow"

tasks:
  - id: main-agent
    agent:
      prompt: "Main agent task"
  - id: sub-agent
    subagent:
      prompt: "Isolated subagent task"
  - id: bridge
    function:
      reference: "tools::collect"
  - id: final-agent
    agent:
      prompt: "Final agent task"

flows:
  - source: main-agent
    target: sub-agent
  - source: sub-agent
    target: bridge
  - source: bridge
    target: final-agent
"#;
    serde_yaml::from_str(yaml).unwrap()
}

fn make_all_keywords_workflow() -> Workflow {
    let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test all 7 keywords"

tasks:
  - id: t1
    agent:
      prompt: "agent task"
  - id: t2
    subagent:
      prompt: "subagent task"
  - id: t3
    shell:
      command: "echo test"
  - id: t4
    http:
      url: "https://example.com"
  - id: t5
    mcp:
      reference: "fs::read"
  - id: t6
    function:
      reference: "tools::fn"
  - id: t7
    llm:
      prompt: "classify"

flows: []
"#;
    serde_yaml::from_str(yaml).unwrap()
}

// ============================================================================
// SHARED AGENT RUNNER TESTS - Writes to GlobalContext
// ============================================================================

mod shared_runner_tests {
    use super::*;

    #[tokio::test]
    async fn test_shared_runner_writes_output_to_context() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-sonnet-4-5");
        let runner = SharedAgentRunner::new(provider, config);

        let mut context = GlobalContext::new();

        // Execute task
        let result = runner
            .execute("test-task", "Say hello", &mut context)
            .await
            .unwrap();

        // Verify success
        assert!(result.success);
        assert_eq!(result.task_id, "test-task");

        // Verify output is written to context
        assert!(
            context.get_output("test-task").is_some(),
            "SharedAgentRunner should write output to GlobalContext"
        );
    }

    #[tokio::test]
    async fn test_shared_runner_writes_history_to_context() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-sonnet-4-5");
        let runner = SharedAgentRunner::new(provider, config);

        let mut context = GlobalContext::new();

        // Verify empty history initially
        assert!(context.agent_history().is_empty());

        // Execute task
        runner
            .execute("test-task", "Say hello", &mut context)
            .await
            .unwrap();

        // Verify history is updated (user + assistant = 2 messages)
        assert_eq!(
            context.agent_history().len(),
            2,
            "SharedAgentRunner should write history to GlobalContext"
        );

        // Verify roles
        assert_eq!(context.agent_history()[0].role, MessageRole::User);
        assert_eq!(context.agent_history()[1].role, MessageRole::Assistant);
    }

    #[tokio::test]
    async fn test_shared_runner_accumulates_history() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-sonnet-4-5");
        let runner = SharedAgentRunner::new(provider, config);

        let mut context = GlobalContext::new();

        // Execute multiple tasks
        runner
            .execute("task1", "First question", &mut context)
            .await
            .unwrap();
        runner
            .execute("task2", "Second question", &mut context)
            .await
            .unwrap();
        runner
            .execute("task3", "Third question", &mut context)
            .await
            .unwrap();

        // Verify history accumulates (3 tasks * 2 messages each = 6)
        assert_eq!(
            context.agent_history().len(),
            6,
            "SharedAgentRunner should accumulate history across tasks"
        );
    }

    #[tokio::test]
    async fn test_shared_runner_multiple_outputs_accessible() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-sonnet-4-5");
        let runner = SharedAgentRunner::new(provider, config);

        let mut context = GlobalContext::new();

        // Execute multiple tasks
        runner
            .execute("analyze", "Analyze code", &mut context)
            .await
            .unwrap();
        runner
            .execute("summarize", "Summarize findings", &mut context)
            .await
            .unwrap();

        // Verify all outputs are accessible
        assert!(context.get_output("analyze").is_some());
        assert!(context.get_output("summarize").is_some());
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
            .execute_with_config(
                "special-task",
                "Complex task",
                &mut context,
                &override_config,
            )
            .await
            .unwrap();

        assert!(result.success);
        assert!(context.get_output("special-task").is_some());
    }
}

// ============================================================================
// ISOLATED AGENT RUNNER TESTS - Read-only, returns LocalContext
// ============================================================================

mod isolated_runner_tests {
    use super::*;

    #[tokio::test]
    async fn test_isolated_runner_does_not_write_to_global_context() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-opus-4");
        let runner = IsolatedAgentRunner::new(provider, config);

        let context = GlobalContext::new();

        // Execute subagent task
        let _result = runner
            .execute("isolated-task", "Do something", &context)
            .await
            .unwrap();

        // Verify output is NOT in global context
        assert!(
            context.get_output("isolated-task").is_none(),
            "IsolatedAgentRunner should NOT write to GlobalContext"
        );
    }

    #[tokio::test]
    async fn test_isolated_runner_returns_local_context_with_output() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-opus-4");
        let runner = IsolatedAgentRunner::new(provider, config);

        let context = GlobalContext::new();

        // Execute subagent task
        let result = runner
            .execute("isolated-task", "Do something", &context)
            .await
            .unwrap();

        // Verify output IS in local context
        assert!(
            result.local_context.get_output("isolated-task").is_some(),
            "IsolatedAgentRunner should write to LocalContext"
        );
    }

    #[tokio::test]
    async fn test_isolated_runner_has_empty_history() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-opus-4");
        let runner = IsolatedAgentRunner::new(provider, config);

        // Set up global context with existing history
        let mut global_context = GlobalContext::new();
        global_context.add_agent_message(MessageRole::User, "Previous question".to_string());
        global_context.add_agent_message(MessageRole::Assistant, "Previous answer".to_string());

        // Execute subagent - should NOT see global history
        let result = runner
            .execute("sub-task", "New question", &global_context)
            .await
            .unwrap();

        // Verify local context has only its own history (2 messages, not 4)
        assert_eq!(
            result.local_context.local_history().len(),
            2,
            "IsolatedAgentRunner should have fresh history (isolation)"
        );

        // Verify global history is unchanged
        assert_eq!(
            global_context.agent_history().len(),
            2,
            "Global history should not be modified by IsolatedAgentRunner"
        );
    }

    #[tokio::test]
    async fn test_isolated_runner_can_read_global_outputs() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-opus-4");
        let runner = IsolatedAgentRunner::new(provider, config);

        // Set up global context with existing output
        let mut global_context = GlobalContext::new();
        global_context.set_output("previous-task", "Previous output".to_string());

        // Execute subagent
        let result = runner
            .execute("sub-task", "Process previous", &global_context)
            .await
            .unwrap();

        // Verify subagent can READ the previous output
        assert_eq!(
            result.local_context.get_output("previous-task"),
            Some("Previous output"),
            "IsolatedAgentRunner should be able to read global outputs"
        );
    }

    #[tokio::test]
    async fn test_isolated_runner_has_local_outputs() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-opus-4");
        let runner = IsolatedAgentRunner::new(provider, config);

        let context = GlobalContext::new();

        let result = runner
            .execute("analysis", "Analyze deeply", &context)
            .await
            .unwrap();

        // Verify has_local_outputs works
        assert!(
            result.has_local_outputs(),
            "SubagentResult should indicate local outputs exist"
        );
    }

    #[tokio::test]
    async fn test_isolated_runner_result_can_be_bridged() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-opus-4");
        let runner = IsolatedAgentRunner::new(provider, config);

        let mut context = GlobalContext::new();

        // Execute subagent
        let result = runner
            .execute("deep-analysis", "Analyze deeply", &context)
            .await
            .unwrap();

        // Simulate bridge pattern - copy output to global context
        context.set_output("deep-analysis", result.output.clone());

        // Verify bridge worked
        assert!(
            context.get_output("deep-analysis").is_some(),
            "Bridging should copy output to GlobalContext"
        );
    }
}

// ============================================================================
// CONTEXT TRAIT TESTS - Reader/Writer implementations
// ============================================================================

mod context_trait_tests {
    use super::*;

    #[test]
    fn test_global_context_implements_reader() {
        let mut ctx = GlobalContext::new();
        ctx.set_output("task1", "Hello World".to_string());

        // Test ContextReader methods
        assert_eq!(ctx.get_output("task1"), Some("Hello World"));
        assert_eq!(ctx.get_output("nonexistent"), None);
    }

    #[test]
    fn test_global_context_implements_writer() {
        let mut ctx = GlobalContext::new();

        // Test ContextWriter methods
        ctx.set_output("task1", "Output 1".to_string());
        ctx.set_output("task2", "Output 2".to_string());

        assert_eq!(ctx.get_output("task1"), Some("Output 1"));
        assert_eq!(ctx.get_output("task2"), Some("Output 2"));
    }

    #[test]
    fn test_global_context_structured_output() {
        let mut ctx = GlobalContext::new();

        ctx.set_structured_output(
            "user",
            serde_json::json!({
                "name": "Alice",
                "age": 30
            }),
        );

        assert_eq!(ctx.get_field("user", "name"), Some("Alice".to_string()));
        assert_eq!(ctx.get_field("user", "age"), Some("30".to_string()));
        assert_eq!(ctx.get_field("user", "missing"), None);
    }

    #[test]
    fn test_global_context_history() {
        let mut ctx = GlobalContext::new();

        ctx.add_agent_message(MessageRole::User, "Question?".to_string());
        ctx.add_agent_message(MessageRole::Assistant, "Answer.".to_string());

        assert!(ctx.has_history());
        assert_eq!(ctx.agent_history().len(), 2);
        assert_eq!(ctx.agent_history()[0].role, MessageRole::User);
        assert_eq!(ctx.agent_history()[1].role, MessageRole::Assistant);
    }

    #[test]
    fn test_global_context_format_history() {
        let mut ctx = GlobalContext::new();

        ctx.add_agent_message(MessageRole::User, "What is 2+2?".to_string());
        ctx.add_agent_message(MessageRole::Assistant, "2+2 equals 4.".to_string());

        let formatted = ctx.format_agent_history();
        assert!(formatted.contains("User: What is 2+2?"));
        assert!(formatted.contains("Assistant: 2+2 equals 4."));
    }

    #[test]
    fn test_global_context_inputs() {
        let mut inputs = HashMap::new();
        inputs.insert("file".to_string(), "src/main.rs".to_string());
        inputs.insert("mode".to_string(), "debug".to_string());

        let ctx = GlobalContext::with_inputs(inputs);

        assert_eq!(ctx.get_input("file"), Some("src/main.rs"));
        assert_eq!(ctx.get_input("mode"), Some("debug"));
        assert_eq!(ctx.get_input("missing"), None);
    }

    #[test]
    fn test_local_context_implements_reader_only() {
        let mut global = GlobalContext::new();
        global.set_output("global-task", "global output".to_string());
        global.add_agent_message(MessageRole::User, "Hello".to_string());

        let local = global.snapshot();

        // Test ContextReader - can read global outputs
        assert_eq!(local.get_output("global-task"), Some("global output"));

        // Test that history is NOT copied (isolation)
        assert!(!local.has_history());
        assert_eq!(local.agent_history().len(), 0);
    }

    #[test]
    fn test_local_context_prioritizes_local_output() {
        let mut global = GlobalContext::new();
        global.set_output("task", "global value".to_string());

        let mut local = global.snapshot();
        local.set_local_output("task", "local value".to_string());

        // Local value should take precedence
        assert_eq!(local.get_output("task"), Some("local value"));

        // Global should be unchanged
        assert_eq!(global.get_output("task"), Some("global value"));
    }

    #[test]
    fn test_local_context_isolation() {
        let mut global = GlobalContext::new();
        global.set_output("global-task", "global output".to_string());

        let mut local = global.snapshot();

        // Write to local
        local.set_local_output("local-task", "local output".to_string());
        local.add_local_message(MessageRole::User, "Local question".to_string());

        // Local sees both global and local outputs
        assert_eq!(local.get_output("global-task"), Some("global output"));
        assert_eq!(local.get_output("local-task"), Some("local output"));

        // Global does NOT see local output
        assert_eq!(global.get_output("local-task"), None);

        // Local has its own history
        assert_eq!(local.local_history().len(), 1);
        assert_eq!(global.agent_history().len(), 0);
    }

    #[test]
    fn test_snapshot_is_readonly_copy() {
        let mut global = GlobalContext::new();
        global.set_output("original", "original value".to_string());

        let local = global.snapshot();

        // Modify global after snapshot
        global.set_output("original", "modified value".to_string());

        // Local should have the original value (snapshot at point in time)
        assert_eq!(local.get_output("original"), Some("original value"));

        // Global should have the new value
        assert_eq!(global.get_output("original"), Some("modified value"));
    }

    #[test]
    fn test_context_batch_operations() {
        let mut ctx = GlobalContext::new();

        ctx.set_outputs_batch([
            ("task1", "output1"),
            ("task2", "output2"),
            ("task3", "output3"),
        ]);

        assert_eq!(ctx.get_output("task1"), Some("output1"));
        assert_eq!(ctx.get_output("task2"), Some("output2"));
        assert_eq!(ctx.get_output("task3"), Some("output3"));

        ctx.add_agent_messages_batch([
            (MessageRole::User, "Q1"),
            (MessageRole::Assistant, "A1"),
            (MessageRole::User, "Q2"),
            (MessageRole::Assistant, "A2"),
        ]);

        assert_eq!(ctx.agent_history().len(), 4);
    }

    #[test]
    fn test_context_clear() {
        let mut ctx = GlobalContext::new();
        ctx.set_output("task1", "output1".to_string());
        ctx.add_agent_message(MessageRole::User, "Question".to_string());

        ctx.clear();

        assert!(ctx.get_output("task1").is_none());
        assert!(!ctx.has_history());
    }
}

// ============================================================================
// WORKFLOW RUNNER DISPATCH TESTS - Routes to correct runner
// ============================================================================

mod workflow_runner_tests {
    use super::*;

    #[tokio::test]
    async fn test_workflow_runner_dispatches_agent_to_shared() {
        let workflow = make_basic_workflow();
        let runner = Runner::new("mock").unwrap();

        let result = runner.run(&workflow).await.unwrap();

        // agent: task should complete successfully
        assert_eq!(result.tasks_completed, 1);
        assert_eq!(result.tasks_failed, 0);

        // Output should be in context (written by SharedAgentRunner)
        assert!(result.context.get_output("task1").is_some());
    }

    #[tokio::test]
    async fn test_workflow_runner_dispatches_subagent_to_isolated() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: sub-task
    subagent:
      prompt: "Isolated work"

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock").unwrap();

        let result = runner.run(&workflow).await.unwrap();

        // subagent: task should complete successfully
        assert_eq!(result.tasks_completed, 1);
        assert_eq!(result.tasks_failed, 0);

        // v4.7.1: WorkflowRunner auto-writes subagent output to GlobalContext
        assert!(
            result.context.get_output("sub-task").is_some(),
            "WorkflowRunner should auto-bridge subagent output"
        );
    }

    #[tokio::test]
    async fn test_workflow_runner_handles_all_7_keywords() {
        let workflow = make_all_keywords_workflow();
        let runner = Runner::new("mock").unwrap();

        let result = runner.run(&workflow).await.unwrap();

        // All 7 tasks should complete
        assert_eq!(
            result.tasks_completed, 7,
            "All 7 keyword tasks should complete"
        );
        assert_eq!(result.tasks_failed, 0);

        // All outputs should be accessible
        for i in 1..=7 {
            let task_id = format!("t{}", i);
            assert!(
                result.context.get_output(&task_id).is_some(),
                "Task {} should have output",
                task_id
            );
        }
    }

    #[tokio::test]
    async fn test_workflow_runner_topological_order() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: step1
    agent:
      prompt: "Step 1"
  - id: step2
    agent:
      prompt: "Step 2"
  - id: step3
    agent:
      prompt: "Step 3"

flows:
  - source: step1
    target: step2
  - source: step2
    target: step3
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock").unwrap();

        let result = runner.run(&workflow).await.unwrap();

        // All tasks should complete in order
        assert_eq!(result.results.len(), 3);
        assert_eq!(result.results[0].task_id, "step1");
        assert_eq!(result.results[1].task_id, "step2");
        assert_eq!(result.results[2].task_id, "step3");
    }

    #[tokio::test]
    async fn test_workflow_runner_with_inputs() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: read-input
    agent:
      prompt: "Read the file"

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock").unwrap();

        let mut inputs = HashMap::new();
        inputs.insert("file".to_string(), "src/main.rs".to_string());

        let result = runner.run_with_inputs(&workflow, inputs).await.unwrap();

        assert_eq!(result.tasks_completed, 1);
    }

    #[tokio::test]
    async fn test_workflow_runner_bridge_pattern() {
        // Test the bridge pattern: subagent → function → agent
        let workflow = make_agent_subagent_workflow();
        let runner = Runner::new("mock").unwrap();

        let result = runner.run(&workflow).await.unwrap();

        // All tasks should complete
        assert_eq!(result.tasks_completed, 4);
        assert_eq!(result.tasks_failed, 0);

        // All outputs should be in context
        assert!(result.context.get_output("main-agent").is_some());
        assert!(result.context.get_output("sub-agent").is_some()); // Auto-bridged in v4.7.1
        assert!(result.context.get_output("bridge").is_some());
        assert!(result.context.get_output("final-agent").is_some());
    }

    #[tokio::test]
    async fn test_workflow_runner_context_sharing() {
        // Test that agent: tasks share context
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: agent1
    agent:
      prompt: "First agent work"
  - id: agent2
    agent:
      prompt: "Second agent work, can see {{agent1}}"

flows:
  - source: agent1
    target: agent2
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock").unwrap();

        let result = runner.run(&workflow).await.unwrap();

        // Both tasks should complete
        assert_eq!(result.tasks_completed, 2);

        // History should accumulate (4 messages: 2 tasks * 2 messages each)
        assert_eq!(
            result.context.agent_history().len(),
            4,
            "agent: tasks should share and accumulate history"
        );
    }
}

// ============================================================================
// INTEGRATION TESTS - End-to-end scenarios
// ============================================================================

mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_shared_vs_isolated_context_access() {
        let provider = mock_provider();
        let config = AgentConfig::new("claude-sonnet-4-5");

        let shared_runner = SharedAgentRunner::new(provider.clone(), config.clone());
        let isolated_runner = IsolatedAgentRunner::new(provider, config);

        let mut global_context = GlobalContext::new();

        // Shared runner writes to global context
        shared_runner
            .execute("agent-task", "Do something", &mut global_context)
            .await
            .unwrap();

        assert!(global_context.get_output("agent-task").is_some());
        assert_eq!(global_context.agent_history().len(), 2);

        // Isolated runner cannot write to global context
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
        assert_eq!(result.local_context.local_history().len(), 2);
    }

    #[tokio::test]
    async fn test_bridge_pattern_manual() {
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
        global_context.set_output("deep-audit", subagent_result.output.clone());

        // Step 4: agent: task can now see subagent output
        assert!(global_context.get_output("deep-audit").is_some());

        // Final agent can reference it
        shared_runner
            .execute("report", "Generate report", &mut global_context)
            .await
            .unwrap();

        assert!(global_context.get_output("report").is_some());
    }

    #[tokio::test]
    async fn test_full_workflow_with_all_task_types() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test full workflow"

tasks:
  - id: init
    agent:
      prompt: "Initialize workflow"
  - id: research
    subagent:
      prompt: "Deep research"
  - id: collect
    function:
      reference: "tools::aggregate"
  - id: run-tests
    shell:
      command: "echo 'tests passed'"
  - id: report
    agent:
      prompt: "Generate final report"

flows:
  - source: init
    target: research
  - source: research
    target: collect
  - source: collect
    target: run-tests
  - source: run-tests
    target: report
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock").unwrap();

        let result = runner.run(&workflow).await.unwrap();

        assert_eq!(result.tasks_completed, 5);
        assert_eq!(result.tasks_failed, 0);

        // All outputs should be accessible
        assert!(result.context.get_output("init").is_some());
        assert!(result.context.get_output("research").is_some());
        assert!(result.context.get_output("collect").is_some());
        assert!(result.context.get_output("run-tests").is_some());
        assert!(result.context.get_output("report").is_some());
    }
}

// ============================================================================
// SECURITY TESTS - Environment variable blocking
// ============================================================================

mod security_tests {
    use super::*;

    #[test]
    fn test_context_blocks_sensitive_env_vars() {
        let ctx = GlobalContext::new();

        // Should return None for blocked vars
        assert!(ctx.get_env("OPENAI_API_KEY").is_none());
        assert!(ctx.get_env("AWS_SECRET_ACCESS_KEY").is_none());
        assert!(ctx.get_env("ANTHROPIC_API_KEY").is_none());
        assert!(ctx.get_env("GITHUB_TOKEN").is_none());
    }

    #[test]
    fn test_context_allows_safe_env_vars() {
        let ctx = GlobalContext::new();

        // Should allow safe vars (might be Some or None depending on env)
        // Just verify it doesn't panic
        let _ = ctx.get_env("HOME");
        let _ = ctx.get_env("PATH");
        let _ = ctx.get_env("USER");
    }

    #[test]
    fn test_local_context_blocks_sensitive_env_vars() {
        let global = GlobalContext::new();
        let local = global.snapshot();

        // Should return None for blocked vars
        assert!(local.get_env("OPENAI_API_KEY").is_none());
        assert!(local.get_env("DATABASE_PASSWORD").is_none());
    }
}
