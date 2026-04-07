//! Tests for the DAG Runner module
//!
//! Coverage targets:
//! - Runner initialization
//! - get_ready_tasks logic
//! - for_each parallelism and concurrency
//! - Event emission

use nika::ast::analyzed::{AnalyzedTaskAction, AnalyzedWorkflow};
use nika::ast::parse_analyzed;
use nika::event::EventLog;
use nika::runtime::Runner;
use nika_core::ast::Templatable;

/// Helper to create a minimal workflow YAML and parse it
fn parse_workflow(yaml: &str) -> AnalyzedWorkflow {
    parse_analyzed(yaml).expect("Failed to parse workflow YAML")
}

// =============================================================================
// TEST 1: Runner Initialization
// =============================================================================

mod runner_initialization {
    use super::*;

    #[test]
    fn test_runner_new_creates_valid_runner() {
        let yaml = r#"
schema: nika/workflow@0.12
workflow: test-init
tasks:
  - id: step1
    infer:
      prompt: "Test prompt"
"#;
        let workflow = parse_workflow(yaml);
        let runner = Runner::new(workflow).unwrap();

        // Runner should be created without panic
        let event_log = runner.event_log();
        assert!(event_log.is_empty(), "Event log should start empty");
    }

    #[test]
    fn test_runner_with_event_log() {
        let yaml = r#"
schema: nika/workflow@0.12
workflow: test-event-log
tasks:
  - id: step1
    exec:
      command: "echo hello"
"#;
        let workflow = parse_workflow(yaml);
        let event_log = EventLog::new();
        let runner = Runner::with_event_log(workflow, event_log).unwrap();

        assert!(runner.event_log().is_empty());
    }

    #[test]
    fn test_runner_with_multiple_tasks() {
        let yaml = r#"
schema: nika/workflow@0.12
workflow: multi-task
tasks:
  - id: step1
    exec:
      command: "echo step1"
  - id: step2
    exec:
      command: "echo step2"
  - id: step3
    exec:
      command: "echo step3"
"#;
        let workflow = parse_workflow(yaml);
        assert_eq!(workflow.tasks.len(), 3);

        let runner = Runner::new(workflow).unwrap();
        assert!(runner.event_log().is_empty());
    }
}

// =============================================================================
// TEST 2: Workflow Parsing for Runner
// =============================================================================

mod workflow_parsing {
    use super::*;

    #[test]
    fn test_workflow_with_depends_on() {
        let yaml = r#"
schema: nika/workflow@0.12
workflow: with-deps
tasks:
  - id: step1
    exec:
      command: "echo start"
  - id: step2
    depends_on: [step1]
    exec:
      command: "echo end"
"#;
        let workflow = parse_workflow(yaml);
        assert_eq!(workflow.tasks.len(), 2);

        // step2 depends on step1
        let step2 = &workflow.tasks[1];
        assert_eq!(step2.depends_on.len(), 1);
    }

    #[test]
    fn test_workflow_with_for_each() {
        let yaml = r#"
schema: nika/workflow@0.12
workflow: with-for-each
tasks:
  - id: parallel_task
    for_each: ["item1", "item2", "item3"]
    as: item
    exec:
      command: "echo {{item}}"
"#;
        let workflow = parse_workflow(yaml);
        assert_eq!(workflow.tasks.len(), 1);

        let task = &workflow.tasks[0];
        assert!(task.for_each.is_some());
        let fe = task.for_each.as_ref().unwrap();
        assert_eq!(fe.as_var, "item");
    }

    #[test]
    fn test_workflow_with_concurrency_settings() {
        let yaml = r#"
schema: nika/workflow@0.12
workflow: with-concurrency
tasks:
  - id: parallel_task
    for_each: ["a", "b", "c", "d", "e"]
    as: item
    concurrency: 3
    fail_fast: false
    exec:
      command: "echo {{item}}"
"#;
        let workflow = parse_workflow(yaml);
        let task = &workflow.tasks[0];
        let fe = task.for_each.as_ref().unwrap();

        assert_eq!(fe.concurrency, Some(Templatable::Value(3)));
        assert_eq!(fe.fail_fast, Templatable::Value(false));
    }

    #[test]
    fn test_workflow_concurrency_defaults() {
        let yaml = r#"
schema: nika/workflow@0.12
workflow: concurrency-defaults
tasks:
  - id: parallel_task
    for_each: ["a", "b"]
    as: item
    exec:
      command: "echo {{item}}"
"#;
        let workflow = parse_workflow(yaml);
        let task = &workflow.tasks[0];
        let fe = task.for_each.as_ref().unwrap();

        // Default concurrency is 1 (sequential)
        assert_eq!(fe.concurrency, Some(Templatable::Value(1)));
        // Default fail_fast is true
        assert_eq!(fe.fail_fast, Templatable::Value(true));
    }
}

// =============================================================================
// TEST 3: Task Action Variants
// =============================================================================

mod task_actions {
    use super::*;

    #[test]
    fn test_infer_task() {
        let yaml = r#"
schema: nika/workflow@0.12
workflow: infer-test
tasks:
  - id: infer_task
    infer:
      prompt: "Generate a response"
"#;
        let workflow = parse_workflow(yaml);
        let task = &workflow.tasks[0];
        assert!(matches!(task.action, AnalyzedTaskAction::Infer(..)));
    }

    #[test]
    fn test_exec_task() {
        let yaml = r#"
schema: nika/workflow@0.12
workflow: exec-test
tasks:
  - id: exec_task
    exec:
      command: "echo hello"
"#;
        let workflow = parse_workflow(yaml);
        let task = &workflow.tasks[0];
        assert!(matches!(task.action, AnalyzedTaskAction::Exec(..)));
    }

    #[test]
    fn test_fetch_task() {
        let yaml = r#"
schema: nika/workflow@0.12
workflow: fetch-test
tasks:
  - id: fetch_task
    fetch:
      url: "https://example.com/api"
      method: GET
"#;
        let workflow = parse_workflow(yaml);
        let task = &workflow.tasks[0];
        assert!(matches!(task.action, AnalyzedTaskAction::Fetch(..)));
    }

    #[test]
    fn test_invoke_task() {
        let yaml = r#"
schema: nika/workflow@0.12
workflow: invoke-test
tasks:
  - id: invoke_task
    invoke:
      mcp: novanet
      tool: novanet_describe
"#;
        let workflow = parse_workflow(yaml);
        let task = &workflow.tasks[0];
        assert!(matches!(task.action, AnalyzedTaskAction::Invoke(..)));
    }
}

// =============================================================================
// TEST 4: Dependency Graph Construction
// =============================================================================

mod dependency_graph {
    use super::*;

    #[test]
    fn test_no_dependencies() {
        let yaml = r#"
schema: nika/workflow@0.12
workflow: no-deps
tasks:
  - id: task1
    exec:
      command: "echo 1"
  - id: task2
    exec:
      command: "echo 2"
"#;
        let workflow = parse_workflow(yaml);

        // No dependencies - both tasks should have empty deps
        assert!(workflow.tasks[0].depends_on.is_empty());
        assert!(workflow.tasks[1].depends_on.is_empty());
    }

    #[test]
    fn test_linear_chain() {
        let yaml = r#"
schema: nika/workflow@0.12
workflow: linear-chain
tasks:
  - id: task1
    exec:
      command: "echo 1"
  - id: task2
    depends_on: [task1]
    exec:
      command: "echo 2"
  - id: task3
    depends_on: [task2]
    exec:
      command: "echo 3"
"#;
        let workflow = parse_workflow(yaml);

        assert!(workflow.tasks[0].depends_on.is_empty());
        assert_eq!(workflow.tasks[1].depends_on.len(), 1);
        assert_eq!(workflow.tasks[2].depends_on.len(), 1);
    }

    #[test]
    fn test_fan_out() {
        let yaml = r#"
schema: nika/workflow@0.12
workflow: fan-out
tasks:
  - id: root
    exec:
      command: "echo root"
  - id: branch1
    depends_on: [root]
    exec:
      command: "echo branch1"
  - id: branch2
    depends_on: [root]
    exec:
      command: "echo branch2"
  - id: branch3
    depends_on: [root]
    exec:
      command: "echo branch3"
"#;
        let workflow = parse_workflow(yaml);

        // Root has no dependencies
        assert!(workflow.tasks[0].depends_on.is_empty());

        // All branches depend on root
        for task in &workflow.tasks[1..] {
            assert_eq!(task.depends_on.len(), 1);
        }
    }

    #[test]
    fn test_fan_in() {
        let yaml = r#"
schema: nika/workflow@0.12
workflow: fan-in
tasks:
  - id: source1
    exec:
      command: "echo 1"
  - id: source2
    exec:
      command: "echo 2"
  - id: sink
    depends_on: [source1, source2]
    exec:
      command: "echo sink"
"#;
        let workflow = parse_workflow(yaml);

        // Sink depends on both sources
        let sink = &workflow.tasks[2];
        assert_eq!(sink.depends_on.len(), 2);
    }
}

// =============================================================================
// TEST 5: Event Log Integration
// =============================================================================

mod event_log_integration {
    use super::*;

    #[test]
    fn test_event_log_starts_empty() {
        let yaml = r#"
schema: nika/workflow@0.12
workflow: event-test
tasks:
  - id: task1
    exec:
      command: "echo test"
"#;
        let workflow = parse_workflow(yaml);
        let runner = Runner::new(workflow).unwrap();

        assert!(runner.event_log().is_empty());
    }

    #[test]
    fn test_event_log_with_broadcast() {
        let yaml = r#"
schema: nika/workflow@0.12
workflow: broadcast-test
tasks:
  - id: task1
    exec:
      command: "echo test"
"#;
        let workflow = parse_workflow(yaml);
        let (event_log, _rx) = EventLog::new_with_broadcast();
        let runner = Runner::with_event_log(workflow, event_log).unwrap();

        assert!(runner.event_log().is_empty());
    }
}

// =============================================================================
// TEST 6: Data Binding Setup
// =============================================================================

mod data_binding_setup {
    use super::*;

    #[test]
    fn test_workflow_with_binding() {
        let yaml = r#"
schema: nika/workflow@0.12
workflow: binding-test
tasks:
  - id: step1
    exec:
      command: "echo hello"

  - id: step2
    depends_on: [step1]
    exec:
      command: "echo {{with.result}}"
    with:
      result: $step1
"#;
        let workflow = parse_workflow(yaml);
        assert_eq!(workflow.tasks.len(), 2);

        // Second task has with: binding
        let task2 = &workflow.tasks[1];
        assert!(!task2.with_spec.is_empty());
    }

    #[test]
    fn test_workflow_with_multiple_bindings() {
        let yaml = r#"
schema: nika/workflow@0.12
workflow: multi-binding
tasks:
  - id: fetch_data
    exec:
      command: "echo data"

  - id: fetch_config
    exec:
      command: "echo config"

  - id: combine
    depends_on: [fetch_data, fetch_config]
    exec:
      command: "echo {{with.data}} {{with.config}}"
    with:
      data: $fetch_data
      config: $fetch_config
"#;
        let workflow = parse_workflow(yaml);
        assert_eq!(workflow.tasks.len(), 3);

        // combine task has 2 with: bindings
        let combine = &workflow.tasks[2];
        assert_eq!(combine.with_spec.len(), 2);
    }
}
