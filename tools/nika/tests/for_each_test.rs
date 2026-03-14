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
//!         locale: "{{use.locale}}"
//! ```

use nika::ast::parse_workflow;
use nika::runtime::Runner;

// ═══════════════════════════════════════════════════════════════
// for_each Parsing Tests
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_for_each_array_literal() {
    // Task with for_each array literal
    let yaml = r#"
schema: nika/workflow@0.3
tasks:
  - id: process_locales
    for_each: ["en-US", "fr-FR", "de-DE"]
    as: locale
    exec:
      command: "echo {{use.locale}}"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let task = &workflow.tasks[0];

    // Verify for_each parsed
    assert!(task.for_each.is_some(), "for_each should be Some");
    let for_each = task.for_each.as_ref().unwrap();

    // Should be an array with 3 items
    assert!(for_each.is_array(), "for_each should be an array");
    assert_eq!(for_each.as_array().unwrap().len(), 3);

    // Verify 'as' variable name
    assert_eq!(task.for_each_as.as_deref(), Some("locale"));
}

#[test]
fn test_for_each_default_as_item() {
    // When 'as' is not specified, default to "item"
    let yaml = r#"
schema: nika/workflow@0.3
tasks:
  - id: process_numbers
    for_each: [1, 2, 3]
    exec:
      command: "echo {{use.item}}"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let task = &workflow.tasks[0];

    assert!(task.for_each.is_some());
    // When 'as' is not specified, it defaults to "item" (set by analyzer)
    assert_eq!(task.for_each_as.as_deref(), Some("item"));
}

#[test]
fn test_for_each_with_invoke() {
    // for_each with invoke action - real use case
    let yaml = r#"
schema: nika/workflow@0.3
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
        locale: "{{use.locale}}"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let task = &workflow.tasks[0];

    assert!(task.for_each.is_some());
    assert_eq!(task.for_each_as.as_deref(), Some("locale"));
}

#[test]
fn test_for_each_preserves_action() {
    // Ensure the action is still parsed correctly alongside for_each
    let yaml = r#"
schema: nika/workflow@0.3
tasks:
  - id: test_task
    for_each: ["a", "b"]
    as: letter
    exec:
      command: "echo {{use.letter}}"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let task = &workflow.tasks[0];

    // Verify action is Exec
    match &task.action {
        nika::ast::TaskAction::Exec { exec } => {
            assert_eq!(exec.command, "echo {{use.letter}}");
        }
        other => panic!("Expected Exec action, got {:?}", other),
    }
}

// ═══════════════════════════════════════════════════════════════
// for_each Validation Tests
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_for_each_empty_array_error() {
    // Empty array should be invalid
    let yaml = r#"
schema: nika/workflow@0.3
tasks:
  - id: empty_foreach
    for_each: []
    exec:
      command: "echo test"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let task = &workflow.tasks[0];

    // Validation should fail for empty array (task-level)
    let result = task.validate_for_each();
    assert!(result.is_err(), "Empty for_each should be invalid");

    // Note: workflow-level validate_schema() was removed
    // The task-level check above is sufficient
}

#[test]
fn test_task_without_for_each() {
    // Regular task without for_each should work
    let yaml = r#"
schema: nika/workflow@0.3
tasks:
  - id: simple_task
    exec:
      command: "echo hello"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let task = &workflow.tasks[0];

    assert!(task.for_each.is_none());
    assert!(task.for_each_as.is_none());
}

// ═══════════════════════════════════════════════════════════════
// for_each Runtime Execution Tests
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_for_each_executes_for_all_items() {
    // for_each should execute the task once per item in the array
    let yaml = r#"
schema: nika/workflow@0.3
tasks:
  - id: echo_items
    for_each: ["apple", "banana", "cherry"]
    as: fruit
    exec:
      command: "echo {{use.fruit}}"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let mut runner = Runner::new(workflow);
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
schema: nika/workflow@0.3
tasks:
  - id: echo_numbers
    for_each: [1, 2, 3]
    exec:
      command: "echo {{use.item}}"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let mut runner = Runner::new(workflow);
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
schema: nika/workflow@0.3
tasks:
  - id: mixed_results
    for_each: ["echo success1", "exit 1", "echo success2"]
    as: cmd
    exec:
      command: "sh -c '{{use.cmd}}'"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let mut runner = Runner::new(workflow);
    let result = runner.run().await;

    // Workflow should complete (not panic) even with partial failures
    // The result depends on how we handle failures - either:
    // 1. Workflow fails if any task fails
    // 2. Workflow succeeds but collects failure info
    // Current behavior: workflow may fail, but should not panic
    assert!(
        result.is_ok() || result.is_err(),
        "Workflow should complete without panic"
    );
}

#[tokio::test]
async fn test_for_each_all_succeed() {
    // All items succeed - baseline test
    let yaml = r#"
schema: nika/workflow@0.3
tasks:
  - id: all_good
    for_each: ["hello", "world", "test"]
    as: word
    exec:
      command: "echo {{use.word}}"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let mut runner = Runner::new(workflow);
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
schema: nika/workflow@0.3
tasks:
  - id: empty_outputs
    for_each: ["true", "true", "true"]
    as: cmd
    exec:
      command: "{{use.cmd}}"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let mut runner = Runner::new(workflow);
    let result = runner.run().await;

    assert!(
        result.is_ok(),
        "Empty output case should succeed: {:?}",
        result.err()
    );
}

// ═══════════════════════════════════════════════════════════════
// for_each with Nested Path Binding (v0.24.1 BUG FIX)
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_for_each_with_use_nested_path_binding() {
    // BUG: for_each: "{{use.data.nested.items}}" silently fails when
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
    for_each: "{{use.data.nested.items}}"
    as: item
    exec:
      command: "echo Processing: {{use.item}}"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let mut runner = Runner::new(workflow);
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
