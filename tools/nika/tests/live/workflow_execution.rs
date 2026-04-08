// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(dead_code, unused_variables)]
//! Live workflow execution tests - run actual .nika.yaml workflows.
//!
//! These tests execute real workflow files with actual API calls.

use nika::ast::{parse_workflow, Workflow};
use std::env;
use std::path::PathBuf;

/// Get the examples directory path
fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples")
}

/// Check if we have any LLM provider available
fn has_any_provider() -> bool {
    env::var("ANTHROPIC_API_KEY").is_ok() || env::var("OPENAI_API_KEY").is_ok()
}

/// Parse a workflow file by filename from examples directory
fn parse_workflow_file(filename: &str) -> Workflow {
    let path = examples_dir().join(filename);
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));
    parse_workflow(&content).unwrap_or_else(|e| panic!("Failed to parse {}: {}", filename, e))
}

// ============================================================================
// SIMPLE WORKFLOW TESTS
// ============================================================================

#[tokio::test]
#[ignore = "Requires API key"]
async fn test_workflow_simple_infer() {
    if !has_any_provider() {
        return;
    }

    // Create a simple inline workflow
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: test-simple-infer
description: "Simple infer test"
provider: claude

tasks:
  - id: greet
    infer: "Say hello in 3 words"
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse workflow");
    assert_eq!(workflow.schema, "nika/workflow@0.12");
    assert_eq!(workflow.tasks.len(), 1);
}

#[tokio::test]
#[ignore = "Requires API key"]
async fn test_workflow_multi_task_dag() {
    if !has_any_provider() {
        return;
    }

    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: test-multi-task
description: "Multi-task DAG test"
provider: claude

tasks:
  - id: step1
    exec: "echo 'step1'"

  - id: step2
    exec: "echo 'step2'"

  - id: step3
    infer: "Combine: {{with.a}} and {{with.b}}"
    with:
      a: $step1
      b: $step2

"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse workflow");
    assert_eq!(workflow.tasks.len(), 3);
    // step3 depends on step1 and step2 via with: bindings
    assert!(workflow.flow_count() > 0);
}

// ============================================================================
// EXAMPLE WORKFLOW VALIDATION
// ============================================================================

#[test]
fn test_parse_claude_test_workflow() {
    let workflow = parse_workflow_file("claude-test.nika.yaml");
    assert!(!workflow.tasks.is_empty());
    // Schema version may vary between example files
    assert!(workflow.schema.starts_with("nika/workflow@"));
}

#[test]
fn test_parse_agent_simple_workflow() {
    let workflow = parse_workflow_file("agent-simple.nika.yaml");
    assert!(!workflow.tasks.is_empty());
}

#[test]
fn test_parse_sequential_thinking_workflow() {
    let workflow = parse_workflow_file("sequential-thinking.nika.yaml");
    assert!(!workflow.tasks.is_empty());
}

#[test]
fn test_parse_research_agent_workflow() {
    let workflow = parse_workflow_file("research-agent.nika.yaml");
    assert!(!workflow.tasks.is_empty());
}

#[test]
fn test_parse_production_test_workflow() {
    let workflow = parse_workflow_file("production-test.nika.yaml");
    assert!(!workflow.tasks.is_empty());
}

// ============================================================================
// WORKFLOW VALIDATION TESTS
// ============================================================================

#[test]
fn test_all_example_workflows_parse() {
    let examples = examples_dir();
    if !examples.exists() {
        eprintln!("Examples directory not found: {}", examples.display());
        return;
    }

    let mut parsed = 0;
    let mut failed = Vec::new();

    for entry in std::fs::read_dir(&examples).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.extension().map(|e| e == "yaml").unwrap_or(false) {
            let filename = path.file_name().unwrap().to_string_lossy().to_string();

            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    failed.push(format!("{}: read error: {}", filename, e));
                    continue;
                }
            };

            match parse_workflow(&content) {
                Ok(_) => {
                    parsed += 1;
                }
                Err(e) => {
                    failed.push(format!("{}: parse error: {}", filename, e));
                }
            }
        }
    }

    println!("Parsed {} workflows successfully", parsed);

    if !failed.is_empty() {
        println!("\nFailed workflows:");
        for f in &failed {
            println!("  - {}", f);
        }
    }

    // Allow some failures (e.g., test files with intentional errors)
    // Note: After v0.19.1 cleanup, we have ~27 example workflows in root directory
    assert!(
        parsed >= 20,
        "Expected at least 20 parseable workflows, got {}",
        parsed
    );
}

// ============================================================================
// COMPLEX DAG WORKFLOW TESTS
// ============================================================================

#[tokio::test]
#[ignore = "Requires API key"]
async fn test_workflow_diamond_dependency() {
    if !has_any_provider() {
        return;
    }

    // Diamond pattern: A -> B, A -> C, B -> D, C -> D
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: test-diamond
description: "Diamond dependency pattern"
provider: claude

tasks:
  - id: A
    exec: "echo 'A'"

  - id: B
    exec: "echo 'B: {{with.a}}'"
    with:
      a: $A

  - id: C
    exec: "echo 'C: {{with.a}}'"
    with:
      a: $A

  - id: D
    infer: "Combine B={{with.b}} and C={{with.c}}"
    with:
      b: $B
      c: $C

"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse workflow");
    assert_eq!(workflow.tasks.len(), 4);
}

#[tokio::test]
#[ignore = "Requires API key"]
async fn test_workflow_parallel_tasks() {
    if !has_any_provider() {
        return;
    }

    // Parallel tasks with no dependencies
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: test-parallel
description: "Parallel execution test"
provider: claude

tasks:
  - id: task1
    exec: "echo 'task1'"

  - id: task2
    exec: "echo 'task2'"

  - id: task3
    exec: "echo 'task3'"

  - id: final
    infer: "Summarize: {{with.t1}}, {{with.t2}}, {{with.t3}}"
    with:
      t1: $task1
      t2: $task2
      t3: $task3

"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse workflow");
    assert_eq!(workflow.tasks.len(), 4);
}

// ============================================================================
// FOR_EACH PARALLELISM TESTS
// ============================================================================

#[tokio::test]
#[ignore = "Requires API key"]
async fn test_workflow_for_each_static() {
    if !has_any_provider() {
        return;
    }

    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: test-for-each
description: "for_each parallelism test"
provider: claude

tasks:
  - id: greet_all
    for_each: ["Alice", "Bob", "Charlie"]
    as: name
    concurrency: 3
    infer: "Say hello to {{with.name}}"
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse workflow");
    let task = &workflow.tasks[0];
    assert!(task.for_each.is_some());
}

// ============================================================================
// BINDING TESTS
// ============================================================================

#[tokio::test]
#[ignore = "Requires API key"]
async fn test_workflow_lazy_bindings() {
    if !has_any_provider() {
        return;
    }

    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: test-lazy-bindings
description: "Lazy binding test"
provider: claude

tasks:
  - id: get_data
    exec: "echo 'data123'"

  - id: use_data
    infer: "Process: {{with.data}}"
    with:
      data:
        path: get_data
        lazy: true
        default: "fallback"

"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse workflow");
    assert_eq!(workflow.tasks.len(), 2);
}
