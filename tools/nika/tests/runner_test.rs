//! Tests for the DAG Runner module (v0.4.1)
//!
//! Coverage targets:
//! - Runner initialization
//! - get_ready_tasks logic
//! - for_each parallelism and concurrency
//! - Event emission

use nika::ast::Workflow;
use nika::event::EventLog;
use nika::runtime::Runner;

/// Helper to create a minimal workflow YAML and parse it
fn parse_workflow(yaml: &str) -> Workflow {
    serde_yaml::from_str(yaml).expect("Failed to parse workflow YAML")
}

// =============================================================================
// TEST 1: Runner Initialization
// =============================================================================

mod runner_initialization {
    use super::*;

    #[test]
    fn test_runner_new_creates_valid_runner() {
        let yaml = r#"
schema: nika/workflow@0.3
workflow: test-init
tasks:
  - id: step1
    infer:
      prompt: "Test prompt"
"#;
        let workflow = parse_workflow(yaml);
        let runner = Runner::new(workflow);

        // Runner should be created without panic
        let event_log = runner.event_log();
        assert!(event_log.is_empty(), "Event log should start empty");
    }

    #[test]
    fn test_runner_with_event_log() {
        let yaml = r#"
schema: nika/workflow@0.3
workflow: test-event-log
tasks:
  - id: step1
    exec:
      command: "echo hello"
"#;
        let workflow = parse_workflow(yaml);
        let event_log = EventLog::new();
        let runner = Runner::with_event_log(workflow, event_log);

        assert!(runner.event_log().is_empty());
    }

    #[test]
    fn test_runner_with_multiple_tasks() {
        let yaml = r#"
schema: nika/workflow@0.3
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

        let runner = Runner::new(workflow);
        assert!(runner.event_log().is_empty());
    }
}

// =============================================================================
// TEST 2: Workflow Parsing for Runner
// =============================================================================

mod workflow_parsing {
    use super::*;

    #[test]
    fn test_workflow_with_flows() {
        let yaml = r#"
schema: nika/workflow@0.3
workflow: with-flows
tasks:
  - id: step1
    exec:
      command: "echo start"
  - id: step2
    exec:
      command: "echo end"
flows:
  - source: step1
    target: step2
"#;
        let workflow = parse_workflow(yaml);
        assert_eq!(workflow.flows.len(), 1);

        let flow = &workflow.flows[0];
        assert_eq!(flow.source.as_vec(), vec!["step1"]);
        assert_eq!(flow.target.as_vec(), vec!["step2"]);
    }

    #[test]
    fn test_workflow_with_for_each() {
        let yaml = r#"
schema: nika/workflow@0.3
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
        assert_eq!(task.for_each_var(), "item");
    }

    #[test]
    fn test_workflow_with_concurrency_settings() {
        let yaml = r#"
schema: nika/workflow@0.3
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

        assert_eq!(task.for_each_concurrency(), 3);
        assert!(!task.for_each_fail_fast());
    }

    #[test]
    fn test_workflow_concurrency_defaults() {
        let yaml = r#"
schema: nika/workflow@0.3
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

        // Default concurrency is 1 (sequential)
        assert_eq!(task.for_each_concurrency(), 1);
        // Default fail_fast is true
        assert!(task.for_each_fail_fast());
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
schema: nika/workflow@0.3
workflow: infer-test
tasks:
  - id: infer_task
    infer:
      prompt: "Generate a response"
"#;
        let workflow = parse_workflow(yaml);
        let task = &workflow.tasks[0];
        assert!(matches!(task.action, nika::ast::TaskAction::Infer { .. }));
    }

    #[test]
    fn test_exec_task() {
        let yaml = r#"
schema: nika/workflow@0.3
workflow: exec-test
tasks:
  - id: exec_task
    exec:
      command: "echo hello"
"#;
        let workflow = parse_workflow(yaml);
        let task = &workflow.tasks[0];
        assert!(matches!(task.action, nika::ast::TaskAction::Exec { .. }));
    }

    #[test]
    fn test_fetch_task() {
        let yaml = r#"
schema: nika/workflow@0.3
workflow: fetch-test
tasks:
  - id: fetch_task
    fetch:
      url: "https://example.com/api"
      method: GET
"#;
        let workflow = parse_workflow(yaml);
        let task = &workflow.tasks[0];
        assert!(matches!(task.action, nika::ast::TaskAction::Fetch { .. }));
    }

    #[test]
    fn test_invoke_task() {
        let yaml = r#"
schema: nika/workflow@0.3
workflow: invoke-test
tasks:
  - id: invoke_task
    invoke:
      mcp: novanet
      tool: novanet_describe
"#;
        let workflow = parse_workflow(yaml);
        let task = &workflow.tasks[0];
        assert!(matches!(task.action, nika::ast::TaskAction::Invoke { .. }));
    }
}

// =============================================================================
// TEST 4: Flow Graph Construction
// =============================================================================

mod flow_graph {
    use super::*;
    use nika::dag::FlowGraph;

    #[test]
    fn test_flow_graph_empty_flows() {
        let yaml = r#"
schema: nika/workflow@0.3
workflow: no-flows
tasks:
  - id: task1
    exec:
      command: "echo 1"
  - id: task2
    exec:
      command: "echo 2"
"#;
        let workflow = parse_workflow(yaml);
        let graph = FlowGraph::from_workflow(&workflow);

        // No dependencies - both tasks should have empty deps
        let deps1 = graph.get_dependencies("task1");
        let deps2 = graph.get_dependencies("task2");
        assert!(deps1.is_empty());
        assert!(deps2.is_empty());
    }

    #[test]
    fn test_flow_graph_linear_chain() {
        let yaml = r#"
schema: nika/workflow@0.3
workflow: linear-chain
tasks:
  - id: task1
    exec:
      command: "echo 1"
  - id: task2
    exec:
      command: "echo 2"
  - id: task3
    exec:
      command: "echo 3"
flows:
  - source: task1
    target: task2
  - source: task2
    target: task3
"#;
        let workflow = parse_workflow(yaml);
        let graph = FlowGraph::from_workflow(&workflow);

        assert!(graph.get_dependencies("task1").is_empty());
        assert_eq!(graph.get_dependencies("task2").len(), 1);
        assert_eq!(graph.get_dependencies("task3").len(), 1);
    }

    #[test]
    fn test_flow_graph_fan_out() {
        let yaml = r#"
schema: nika/workflow@0.3
workflow: fan-out
tasks:
  - id: root
    exec:
      command: "echo root"
  - id: branch1
    exec:
      command: "echo branch1"
  - id: branch2
    exec:
      command: "echo branch2"
  - id: branch3
    exec:
      command: "echo branch3"
flows:
  - source: root
    target: branch1
  - source: root
    target: branch2
  - source: root
    target: branch3
"#;
        let workflow = parse_workflow(yaml);
        let graph = FlowGraph::from_workflow(&workflow);

        // Root has no dependencies
        assert!(graph.get_dependencies("root").is_empty());

        // All branches depend on root
        for branch in ["branch1", "branch2", "branch3"] {
            let deps = graph.get_dependencies(branch);
            assert_eq!(deps.len(), 1);
            assert_eq!(&*deps[0], "root");
        }
    }

    #[test]
    fn test_flow_graph_fan_in() {
        let yaml = r#"
schema: nika/workflow@0.3
workflow: fan-in
tasks:
  - id: source1
    exec:
      command: "echo 1"
  - id: source2
    exec:
      command: "echo 2"
  - id: sink
    exec:
      command: "echo sink"
flows:
  - source: source1
    target: sink
  - source: source2
    target: sink
"#;
        let workflow = parse_workflow(yaml);
        let graph = FlowGraph::from_workflow(&workflow);

        // Sink depends on both sources
        let deps = graph.get_dependencies("sink");
        assert_eq!(deps.len(), 2);
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
schema: nika/workflow@0.3
workflow: event-test
tasks:
  - id: task1
    exec:
      command: "echo test"
"#;
        let workflow = parse_workflow(yaml);
        let runner = Runner::new(workflow);

        assert!(runner.event_log().is_empty());
    }

    #[test]
    fn test_event_log_with_broadcast() {
        let yaml = r#"
schema: nika/workflow@0.3
workflow: broadcast-test
tasks:
  - id: task1
    exec:
      command: "echo test"
"#;
        let workflow = parse_workflow(yaml);
        let (event_log, _rx) = EventLog::new_with_broadcast();
        let runner = Runner::with_event_log(workflow, event_log);

        assert!(runner.event_log().is_empty());
    }
}

// =============================================================================
// TEST 6: Data Binding Setup
// =============================================================================

mod data_binding_setup {
    use super::*;

    #[test]
    fn test_workflow_with_use_binding() {
        let yaml = r#"
schema: nika/workflow@0.3
workflow: binding-test
tasks:
  - id: step1
    exec:
      command: "echo hello"
    use:
      ctx: result1

  - id: step2
    exec:
      command: "echo {{use.result1}}"
flows:
  - source: step1
    target: step2
"#;
        let workflow = parse_workflow(yaml);
        assert_eq!(workflow.tasks.len(), 2);

        // First task has output binding
        let task1 = &workflow.tasks[0];
        assert!(task1.use_wiring.is_some());
    }

    #[test]
    fn test_workflow_with_multiple_bindings() {
        let yaml = r#"
schema: nika/workflow@0.3
workflow: multi-binding
tasks:
  - id: fetch_data
    exec:
      command: "echo data"
    use:
      ctx: data

  - id: fetch_config
    exec:
      command: "echo config"
    use:
      ctx: config

  - id: combine
    exec:
      command: "echo {{use.data}} {{use.config}}"
flows:
  - source: fetch_data
    target: combine
  - source: fetch_config
    target: combine
"#;
        let workflow = parse_workflow(yaml);
        assert_eq!(workflow.tasks.len(), 3);
        assert_eq!(workflow.flows.len(), 2);
    }
}
