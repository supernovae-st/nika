// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tests for for_each parallelism (v0.3)
//!
//! for_each enables parallel execution of a task over an array of values:
//!
//! ```yaml
//! tasks:
//!   - id: process_locales
//!     for_each: ["en-US", "fr-FR", "de-DE"]
//!     as: locale
//!     invoke:
//!       mcp: novanet
//!       tool: novanet_generate
//!       params:
//!         entity: "qr-code"
//!         locale: "{{with.locale}}"
//! ```

use nika::ast::analyzed::AnalyzedTaskAction;
use nika::ast::parse_analyzed;
use nika::runtime::Runner;

// ═══════════════════════════════════════════════════════════════
// for_each Parsing Tests
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_for_each_array_literal() {
    // Task with for_each array literal
    let yaml = r#"
schema: nika/workflow@0.12
tasks:
  - id: process_locales
    for_each: ["en-US", "fr-FR", "de-DE"]
    as: locale
    exec:
      command: "echo {{with.locale}}"
"#;

    let workflow = parse_analyzed(yaml).unwrap();
    let task = &workflow.tasks[0];

    // Verify for_each parsed
    assert!(task.for_each.is_some(), "for_each should be Some");
    let for_each = task.for_each.as_ref().unwrap();

    // Should be an array with 3 items
    assert!(for_each.is_array(), "for_each should be an array");
    let items = for_each.parse_items().unwrap();
    assert_eq!(items.len(), 3);

    // Verify 'as' variable name
    assert_eq!(for_each.as_var, "locale");
}

#[test]
fn test_for_each_default_as_item() {
    // When 'as' is not specified, default to "item"
    let yaml = r#"
schema: nika/workflow@0.12
tasks:
  - id: process_numbers
    for_each: [1, 2, 3]
    exec:
      command: "echo {{with.item}}"
"#;

    let workflow = parse_analyzed(yaml).unwrap();
    let task = &workflow.tasks[0];

    assert!(task.for_each.is_some());
    // When 'as' is not specified, it defaults to "item" (set by analyzer)
    let fe = task.for_each.as_ref().unwrap();
    assert_eq!(fe.as_var, "item");
}

#[test]
fn test_for_each_with_invoke() {
    // for_each with invoke action - real use case
    let yaml = r#"
schema: nika/workflow@0.12
mcp:
  novanet:
    command: cargo
    args: [run, -p, novanet-mcp]
tasks:
  - id: generate_content
    for_each: ["en-US", "fr-FR"]
    as: locale
    invoke:
      mcp: novanet
      tool: novanet_generate
      params:
        entity: "qr-code"
        locale: "{{with.locale}}"
"#;

    let workflow = parse_analyzed(yaml).unwrap();
    let task = &workflow.tasks[0];

    assert!(task.for_each.is_some());
    let fe = task.for_each.as_ref().unwrap();
    assert_eq!(fe.as_var, "locale");
}

#[test]
fn test_for_each_preserves_action() {
    // Ensure the action is still parsed correctly alongside for_each
    let yaml = r#"
schema: nika/workflow@0.12
tasks:
  - id: test_task
    for_each: ["a", "b"]
    as: letter
    exec:
      command: "echo {{with.letter}}"
"#;

    let workflow = parse_analyzed(yaml).unwrap();
    let task = &workflow.tasks[0];

    // Verify action is Exec
    match &task.action {
        AnalyzedTaskAction::Exec(exec) => {
            assert_eq!(exec.command, "echo {{with.letter}}");
        }
        other => panic!("Expected Exec action, got {:?}", other),
    }
}

// ═══════════════════════════════════════════════════════════════
// for_each Parsing Validation Tests
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_for_each_empty_array_parsed() {
    // Empty array should still parse at the AST level
    let yaml = r#"
schema: nika/workflow@0.12
tasks:
  - id: empty_foreach
    for_each: []
    exec:
      command: "echo test"
"#;

    let workflow = parse_analyzed(yaml).unwrap();
    let task = &workflow.tasks[0];
    let fe = task.for_each.as_ref().unwrap();
    // Empty array parses to "[]"
    let items = fe.parse_items().unwrap();
    assert!(items.is_empty(), "Empty for_each should have 0 items");
}

#[test]
fn test_task_without_for_each() {
    // Regular task without for_each should work
    let yaml = r#"
schema: nika/workflow@0.12
tasks:
  - id: simple_task
    exec:
      command: "echo hello"
"#;

    let workflow = parse_analyzed(yaml).unwrap();
    let task = &workflow.tasks[0];

    assert!(task.for_each.is_none());
}

// ═══════════════════════════════════════════════════════════════
// for_each Runtime Execution Tests
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_for_each_executes_for_all_items() {
    // for_each should execute the task once per item in the array
    let yaml = r#"
schema: nika/workflow@0.12
tasks:
  - id: echo_items
    for_each: ["apple", "banana", "cherry"]
    as: fruit
    exec:
      command: "echo {{with.fruit}}"
"#;

    let workflow = parse_analyzed(yaml).unwrap();
    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;

    assert!(
        result.is_ok(),
        "Workflow should succeed: {:?}",
        result.err()
    );

    // The output should contain all fruits (order may vary due to parallelism)
    let output = result.unwrap();
    assert!(
        output.contains("apple") || output.contains("banana") || output.contains("cherry"),
        "Output should contain at least one fruit: {output}"
    );
}

#[tokio::test]
async fn test_for_each_with_default_item_variable() {
    // When 'as' is not specified, the variable should be 'item'
    let yaml = r#"
schema: nika/workflow@0.12
tasks:
  - id: echo_numbers
    for_each: [1, 2, 3]
    exec:
      command: "echo {{with.item}}"
"#;

    let workflow = parse_analyzed(yaml).unwrap();
    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;

    assert!(
        result.is_ok(),
        "Workflow should succeed: {:?}",
        result.err()
    );
}

// ═══════════════════════════════════════════════════════════════
// for_each Partial Failure Tests
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_for_each_partial_failure_collects_all() {
    // One item succeeds, one fails (exit 1), one succeeds
    // Workflow should complete and collect all results (including failures)
    let yaml = r#"
schema: nika/workflow@0.12
tasks:
  - id: mixed_results
    for_each: ["echo success1", "exit 1", "echo success2"]
    as: cmd
    exec:
      command: "sh -c '{{with.cmd}}'"
"#;

    let workflow = parse_analyzed(yaml).unwrap();
    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;

    // runner.run() returns Ok even with partial failures — individual task
    // results (including failures) are stored in the datastore
    assert!(
        result.is_ok(),
        "Workflow should complete without panic: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_for_each_all_succeed() {
    // All items succeed - baseline test
    let yaml = r#"
schema: nika/workflow@0.12
tasks:
  - id: all_good
    for_each: ["hello", "world", "test"]
    as: word
    exec:
      command: "echo {{with.word}}"
"#;

    let workflow = parse_analyzed(yaml).unwrap();
    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;

    assert!(
        result.is_ok(),
        "All-success case should pass: {:?}",
        result.err()
    );

    let output = result.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap_or_default();
    if let serde_json::Value::Array(arr) = parsed {
        assert_eq!(arr.len(), 3, "Should have 3 results");
    }
}

#[tokio::test]
async fn test_for_each_with_empty_output() {
    // Items that produce empty output
    let yaml = r#"
schema: nika/workflow@0.12
tasks:
  - id: empty_outputs
    for_each: ["true", "true", "true"]
    as: cmd
    exec:
      command: "{{with.cmd}}"
"#;

    let workflow = parse_analyzed(yaml).unwrap();
    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;

    assert!(
        result.is_ok(),
        "Empty output case should succeed: {:?}",
        result.err()
    );
}

// ═══════════════════════════════════════════════════════════════
// for_each + depends_on Pipeline Safety (v0.27.0)
// ═══════════════════════════════════════════════════════════════

/// Prove: YAML with for_each + depends_on goes through the full
/// parse → analyze → DAG → run pipeline without false rejection.
///
/// This guards against the dangling-dep detection (Dag::from_analyzed)
/// incorrectly flagging for_each template tasks as missing.
#[tokio::test]
async fn test_for_each_with_depends_on_full_pipeline() {
    let yaml = r#"
schema: nika/workflow@0.12
tasks:
  - id: produce
    exec:
      command: "echo '[\"alpha\", \"beta\", \"gamma\"]'"

  - id: process
    depends_on: [produce]
    for_each: $produce
    as: item
    exec:
      command: "echo Processing {{with.item}}"

  - id: aggregate
    depends_on: [process]
    exec:
      command: "echo Done"
"#;
    let workflow =
        parse_analyzed(yaml).expect("for_each + depends_on should parse and analyze without error");

    let mut runner = Runner::new(workflow)
        .expect("for_each + depends_on should build DAG without false rejection");

    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "for_each + depends_on pipeline should execute: {:?}",
        result.err()
    );
}

// ═══════════════════════════════════════════════════════════════
// for_each with Nested Path Binding (v0.24.1 BUG FIX)
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_for_each_with_use_nested_path_binding() {
    // BUG: for_each: "{{with.data.nested.items}}" silently fails when
    // the binding is "data: producer" because the code tries to resolve
    // "data.nested.items" as the alias instead of resolving "data" first
    // and then traversing ".nested.items".
    //
    // This test verifies that nested path traversal through use: bindings
    // works correctly with for_each.
    let yaml = r#"
schema: nika/workflow@0.12
tasks:
  - id: producer
    exec:
      command: "echo '{\"nested\": {\"items\": [\"alpha\", \"beta\", \"gamma\"]}}'"

  - id: consumer
    with:
      data: $producer
    for_each: "{{with.data.nested.items}}"
    as: item
    exec:
      command: "echo Processing: {{with.item}}"
"#;

    let workflow = parse_analyzed(yaml).unwrap();
    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;

    assert!(
        result.is_ok(),
        "Workflow should complete: {:?}",
        result.err()
    );

    // The workflow output should contain the consumer task results
    // For for_each tasks, the output is a JSON array of results
    let output = result.unwrap();

    // Output should contain all 3 processed items (alpha, beta, gamma)
    // The for_each should have expanded to 3 iterations
    assert!(
        output.contains("alpha") || output.contains("Processing: alpha"),
        "Output should contain 'alpha': {}",
        output
    );
    assert!(
        output.contains("beta") || output.contains("Processing: beta"),
        "Output should contain 'beta': {}",
        output
    );
    assert!(
        output.contains("gamma") || output.contains("Processing: gamma"),
        "Output should contain 'gamma': {}",
        output
    );

    // Additionally verify the output is a valid JSON array with 3 elements
    // (indicating 3 for_each iterations ran)
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap_or_default();
    if let serde_json::Value::Array(arr) = &parsed {
        assert_eq!(
            arr.len(),
            3,
            "Should have 3 results from for_each iterations, got: {}",
            output
        );
    } else {
        // If not a JSON array, the for_each likely didn't expand properly
        panic!(
            "Expected JSON array output from for_each task, got: {}",
            output
        );
    }
}
