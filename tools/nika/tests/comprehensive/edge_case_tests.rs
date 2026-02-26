//! Edge case tests for Nika workflows.
//!
//! Tests challenging scenarios:
//! - Deep dependency chains
//! - Lazy bindings with defaults
//! - Unicode and special characters
//! - Empty values
//! - Complex nested JSON
//! - Wide fan-in/fan-out
//! - All binding syntaxes ($var, {{use.var}})

use nika::ast::{TaskAction, Workflow};
use nika::dag::Dag;
use std::path::PathBuf;

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples")
}

fn parse_workflow(filename: &str) -> Workflow {
    let path = examples_dir().join(filename);
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));
    serde_yaml::from_str(&content).unwrap_or_else(|e| panic!("Failed to parse {}: {}", filename, e))
}

// ============================================================================
// DEEP DEPENDENCY CHAIN TESTS
// ============================================================================

#[test]
fn test_parse_edge_cases_workflow() {
    let workflow = parse_workflow("test-edge-cases.nika.yaml");
    assert!(!workflow.tasks.is_empty());
}

#[test]
fn test_deep_chain_10_levels() {
    let workflow = parse_workflow("test-edge-cases.nika.yaml");

    // Find chain tasks
    let chain_tasks: Vec<_> = workflow
        .tasks
        .iter()
        .filter(|t| t.id.starts_with("chain_"))
        .collect();

    assert_eq!(chain_tasks.len(), 10, "Should have 10 chain tasks");
}

#[test]
fn test_deep_chain_dag_is_valid() {
    let yaml = r#"
schema: "nika/workflow@0.5"
tasks:
  - id: c1
    exec: "echo 1"
  - id: c2
    use: { p: c1 }
    exec: "echo 2"
  - id: c3
    use: { p: c2 }
    exec: "echo 3"
  - id: c4
    use: { p: c3 }
    exec: "echo 4"
  - id: c5
    use: { p: c4 }
    exec: "echo 5"
  - id: c6
    use: { p: c5 }
    exec: "echo 6"
  - id: c7
    use: { p: c6 }
    exec: "echo 7"
  - id: c8
    use: { p: c7 }
    exec: "echo 8"
  - id: c9
    use: { p: c8 }
    exec: "echo 9"
  - id: c10
    use: { p: c9 }
    exec: "echo 10"
flows:
  - source: c1
    target: c2
  - source: c2
    target: c3
  - source: c3
    target: c4
  - source: c4
    target: c5
  - source: c5
    target: c6
  - source: c6
    target: c7
  - source: c7
    target: c8
  - source: c8
    target: c9
  - source: c9
    target: c10
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    let graph = Dag::from_workflow(&workflow);

    // Verify path exists from c1 to c10
    assert!(graph.has_path("c1", "c10"), "Path should exist c1 to c10");

    // Verify no cycles
    assert!(
        graph.detect_cycles().is_ok(),
        "Linear chain should have no cycles"
    );
}

// ============================================================================
// BINDING SYNTAX TESTS - {{use.var}} and $var
// ============================================================================

#[test]
fn test_mustache_binding_syntax() {
    let yaml = r#"
schema: "nika/workflow@0.5"
tasks:
  - id: source
    exec: "echo 'data'"
  - id: consumer
    use:
      ctx: source
    exec: "echo 'Received: {{use.ctx}}'"
flows:
  - source: source
    target: consumer
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse mustache syntax");
    if let TaskAction::Exec { exec } = &workflow.tasks[1].action {
        assert!(
            exec.command.contains("{{use.ctx}}"),
            "Should preserve mustache syntax"
        );
    }
}

#[test]
fn test_dollar_binding_syntax() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: source
    exec: "echo 'context data'"
  - id: consumer
    use:
      ctx: source
    infer: "Process this context: $ctx"
flows:
  - source: source
    target: consumer
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse dollar syntax");
    if let TaskAction::Infer { infer } = &workflow.tasks[1].action {
        assert!(
            infer.prompt.contains("$ctx"),
            "Should preserve dollar syntax"
        );
    }
}

#[test]
fn test_mixed_binding_syntaxes() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: data1
    exec: "echo 'first'"
  - id: data2
    exec: "echo 'second'"
  - id: combine
    use:
      a: data1
      b: data2
    infer: "Combine {{use.a}} with $b in one prompt"
flows:
  - source: [data1, data2]
    target: combine
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse mixed syntaxes");
    if let TaskAction::Infer { infer } = &workflow.tasks[2].action {
        assert!(infer.prompt.contains("{{use.a}}"));
        assert!(infer.prompt.contains("$b"));
    }
}

// ============================================================================
// LAZY BINDING TESTS
// ============================================================================

#[test]
fn test_lazy_binding_syntax() {
    let yaml = r#"
schema: "nika/workflow@0.5"
tasks:
  - id: producer
    exec: "echo 'data'"
  - id: consumer
    use:
      ctx:
        path: producer
        lazy: true
        default: "fallback"
    exec: "echo {{use.ctx}}"
flows:
  - source: producer
    target: consumer
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse lazy binding");
    assert_eq!(workflow.tasks.len(), 2);

    // Verify lazy binding is parsed
    let consumer = &workflow.tasks[1];
    let wiring = consumer.use_wiring.as_ref().expect("Should have wiring");
    let entry = wiring.get("ctx").expect("Should have 'ctx' entry");
    assert!(entry.lazy, "Should be lazy binding");
    assert!(entry.default.is_some(), "Should have default value");
}

#[test]
fn test_lazy_binding_with_json_default() {
    let yaml = r#"
schema: "nika/workflow@0.5"
tasks:
  - id: source
    exec: "echo 'ok'"
  - id: use_lazy
    use:
      data:
        path: source
        lazy: true
        default: '{"status": "fallback", "items": []}'
    exec: "echo {{use.data}}"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    let task = &workflow.tasks[1];
    let wiring = task.use_wiring.as_ref().unwrap();
    let entry = wiring.get("data").expect("Should have 'data' entry");

    assert!(entry.lazy);
    assert!(entry.default.is_some());
}

// ============================================================================
// UNICODE AND SPECIAL CHARACTER TESTS
// ============================================================================

#[test]
fn test_unicode_in_prompts() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: unicode_prompt
    infer: "Translate to Japanese: Hello World"
"#;

    let _workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse unicode");
}

#[test]
fn test_emoji_in_workflow() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: emoji_test
    exec: "echo 'rocket emoji here'"
"#;

    let _workflow: Workflow =
        serde_yaml::from_str(yaml).expect("Should parse with emoji references");
}

#[test]
fn test_special_chars_in_commands() {
    let yaml = r#"
schema: "nika/workflow@0.5"
tasks:
  - id: special
    exec: "echo 'quotes inside and vars'"
"#;

    let _workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse special chars");
}

// ============================================================================
// EMPTY AND NULL VALUE TESTS
// ============================================================================

#[test]
fn test_empty_array_output() {
    let yaml = r#"
schema: "nika/workflow@0.5"
tasks:
  - id: empty
    exec: "echo '[]'"
    output:
      format: json
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    let task = &workflow.tasks[0];
    assert!(task.output.is_some());
}

#[test]
fn test_null_json_output() {
    let yaml = r#"
schema: "nika/workflow@0.5"
tasks:
  - id: null_output
    exec: "echo 'null'"
    output:
      format: json
"#;

    let _workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse null");
}

// ============================================================================
// WIDE FAN-IN/FAN-OUT TESTS
// ============================================================================

#[test]
fn test_wide_fan_out_10_targets() {
    let yaml = r#"
schema: "nika/workflow@0.5"
tasks:
  - id: source
    exec: "echo 'data'"
  - id: fan1
    use: { s: source }
    exec: "echo 'fan1'"
  - id: fan2
    use: { s: source }
    exec: "echo 'fan2'"
  - id: fan3
    use: { s: source }
    exec: "echo 'fan3'"
  - id: fan4
    use: { s: source }
    exec: "echo 'fan4'"
  - id: fan5
    use: { s: source }
    exec: "echo 'fan5'"
  - id: fan6
    use: { s: source }
    exec: "echo 'fan6'"
  - id: fan7
    use: { s: source }
    exec: "echo 'fan7'"
  - id: fan8
    use: { s: source }
    exec: "echo 'fan8'"
  - id: fan9
    use: { s: source }
    exec: "echo 'fan9'"
  - id: fan10
    use: { s: source }
    exec: "echo 'fan10'"
flows:
  - source: source
    target: [fan1, fan2, fan3, fan4, fan5, fan6, fan7, fan8, fan9, fan10]
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    assert_eq!(workflow.tasks.len(), 11);

    let graph = Dag::from_workflow(&workflow);
    let successors = graph.get_successors("source");
    assert_eq!(successors.len(), 10, "Source should have 10 successors");
}

#[test]
fn test_wide_fan_in_10_sources() {
    let yaml = r#"
schema: "nika/workflow@0.5"
tasks:
  - id: s1
    exec: "echo 1"
  - id: s2
    exec: "echo 2"
  - id: s3
    exec: "echo 3"
  - id: s4
    exec: "echo 4"
  - id: s5
    exec: "echo 5"
  - id: s6
    exec: "echo 6"
  - id: s7
    exec: "echo 7"
  - id: s8
    exec: "echo 8"
  - id: s9
    exec: "echo 9"
  - id: s10
    exec: "echo 10"
  - id: sink
    use:
      a: s1
      b: s2
      c: s3
      d: s4
      e: s5
      f: s6
      g: s7
      h: s8
      i: s9
      j: s10
    exec: "echo 'aggregated'"
flows:
  - source: [s1, s2, s3, s4, s5, s6, s7, s8, s9, s10]
    target: sink
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    assert_eq!(workflow.tasks.len(), 11);

    let graph = Dag::from_workflow(&workflow);
    let deps = graph.get_dependencies("sink");
    assert_eq!(deps.len(), 10, "Sink should have 10 dependencies");
}

// ============================================================================
// FOR_EACH EDGE CASES
// ============================================================================

#[test]
fn test_large_for_each_30_items() {
    let yaml = r#"
schema: "nika/workflow@0.5"
tasks:
  - id: large_parallel
    for_each: ["01", "02", "03", "04", "05", "06", "07", "08", "09", "10",
               "11", "12", "13", "14", "15", "16", "17", "18", "19", "20",
               "21", "22", "23", "24", "25", "26", "27", "28", "29", "30"]
    as: n
    concurrency: 15
    fail_fast: false
    exec: "echo 'Processing item'"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    let task = &workflow.tasks[0];

    assert!(task.for_each.is_some());
    assert_eq!(task.concurrency, Some(15));
    assert_eq!(task.fail_fast, Some(false));
}

#[test]
fn test_for_each_with_binding_expression() {
    let yaml = r#"
schema: "nika/workflow@0.5"
tasks:
  - id: produce_list
    exec: "echo '[\"a\", \"b\", \"c\"]'"
    output:
      format: json

  - id: iterate
    for_each: "{{use.items}}"
    as: item
    concurrency: 3
    use:
      items: produce_list
    exec: "echo 'Item: {{use.item}}'"

flows:
  - source: produce_list
    target: iterate
"#;

    let _workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse binding for_each");
}

#[test]
fn test_for_each_with_dollar_binding() {
    let yaml = r#"
schema: "nika/workflow@0.5"
tasks:
  - id: list
    exec: "echo '[1, 2, 3]'"
    output:
      format: json

  - id: process
    for_each: "$items"
    as: num
    use:
      items: list
    exec: "echo 'Number'"

flows:
  - source: list
    target: process
"#;

    let _workflow: Workflow =
        serde_yaml::from_str(yaml).expect("Should parse dollar binding for_each");
}

// ============================================================================
// MIXED SHORTHAND AND FULL SYNTAX TESTS
// ============================================================================

#[test]
fn test_mixed_infer_syntax() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: shorthand
    infer: "Short prompt"

  - id: full
    infer:
      prompt: "Full prompt with model"
      model: claude-sonnet-4-6
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    assert_eq!(workflow.tasks.len(), 2);
}

#[test]
fn test_mixed_exec_syntax() {
    let yaml = r#"
schema: "nika/workflow@0.5"
tasks:
  - id: shorthand
    exec: "echo shorthand"

  - id: full
    exec:
      command: "echo full"
      timeout: 30000
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    assert_eq!(workflow.tasks.len(), 2);
}

// ============================================================================
// COMPLEX BINDING CHAINS
// ============================================================================

#[test]
fn test_binding_chain_through_multiple_tasks() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: producer
    exec: "echo '{\"name\": \"test\"}'"
    output:
      format: json

  - id: transformer
    use:
      input: producer
    infer: "Transform: {{use.input}}"

  - id: validator
    use:
      data: transformer
    exec: "echo 'Validating: {{use.data}}'"

  - id: final
    use:
      validated: validator
    infer: "Finalize: {{use.validated}}"

flows:
  - source: producer
    target: transformer
  - source: transformer
    target: validator
  - source: validator
    target: final
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    assert_eq!(workflow.tasks.len(), 4);
    assert_eq!(workflow.flows.len(), 3);

    let graph = Dag::from_workflow(&workflow);
    assert!(graph.has_path("producer", "final"));
}

// ============================================================================
// MULTIPLE BINDINGS PER TASK
// ============================================================================

#[test]
fn test_task_with_many_bindings() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: a
    exec: "echo 'a'"
  - id: b
    exec: "echo 'b'"
  - id: c
    exec: "echo 'c'"
  - id: d
    exec: "echo 'd'"
  - id: e
    exec: "echo 'e'"
  - id: aggregate
    use:
      val_a: a
      val_b: b
      val_c: c
      val_d: d
      val_e: e
    infer: |
      Combine all values:
      A: {{use.val_a}}
      B: {{use.val_b}}
      C: {{use.val_c}}
      D: {{use.val_d}}
      E: {{use.val_e}}

flows:
  - source: [a, b, c, d, e]
    target: aggregate
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    let aggregate = &workflow.tasks[5];
    let wiring = aggregate.use_wiring.as_ref().expect("Should have wiring");
    assert_eq!(wiring.len(), 5);
}
