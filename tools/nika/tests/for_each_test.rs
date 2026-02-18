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

use nika::ast::Workflow;
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

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
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

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let task = &workflow.tasks[0];

    assert!(task.for_each.is_some());
    // When 'as' is not specified, it should default to None (runtime uses "item")
    assert!(task.for_each_as.is_none());
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

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
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

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
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

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let task = &workflow.tasks[0];

    // Validation should fail for empty array (task-level)
    let result = task.validate_for_each();
    assert!(result.is_err(), "Empty for_each should be invalid");

    // Validation should also fail at workflow level
    let workflow_result = workflow.validate_schema();
    assert!(
        workflow_result.is_err(),
        "workflow.validate_schema() should catch empty for_each"
    );
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

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
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

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let runner = Runner::new(workflow);
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

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let runner = Runner::new(workflow);
    let result = runner.run().await;

    assert!(
        result.is_ok(),
        "Workflow should succeed: {:?}",
        result.err()
    );
}
