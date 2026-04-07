//! Integration tests for the AST analyzer (Phase 2).
//!
//! Tests the full pipeline: YAML → parse → analyze → AnalyzedWorkflow.

use nika::ast::analyzer::{analyze, AnalyzeErrorKind};
use nika::ast::raw::{parse, RawWorkflow};
use nika::source::{ByteOffset, FileId};
use nika_core::ast::Templatable;

/// Helper to parse YAML and return RawWorkflow
fn parse_yaml(yaml: &str) -> Result<RawWorkflow, String> {
    let file_id = FileId(0);
    parse(yaml, file_id).map_err(|e| format!("{:?}", e))
}

/// Helper to analyze a RawWorkflow (takes ownership)
fn analyze_workflow(
    raw: RawWorkflow,
) -> nika::ast::analyzer::AnalyzeResult<nika::ast::analyzed::AnalyzedWorkflow> {
    analyze(raw)
}

// ═══════════════════════════════════════════════════════════════════════════
// FULL PIPELINE TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_full_pipeline_minimal_workflow() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: hello
    infer:
      prompt: "Say hello"
"#;

    let raw = parse_yaml(yaml).expect("parse should succeed");
    let result = analyze_workflow(raw);

    assert!(
        result.is_ok(),
        "analysis should succeed: {:?}",
        result.errors
    );
    let workflow = result.value.unwrap();

    assert_eq!(workflow.task_count(), 1);
    assert!(workflow.has_task("hello"));
    assert!(workflow.provider.is_some());
    assert_eq!(
        workflow.provider.as_ref().map(|p| p.as_str()),
        Some("anthropic")
    );
}

#[test]
fn test_full_pipeline_multi_task_workflow() {
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: "Multi-task test"
provider: openai

tasks:
  - id: step1
    infer:
      prompt: "Generate something"

  - id: step2
    with:
      data: "$step1"
    infer:
      prompt: "Process: {{with.data}}"

  - id: step3
    with:
      prev: "$step2"
    exec:
      command: "echo done"
"#;

    let raw = parse_yaml(yaml).expect("parse should succeed");
    let result = analyze_workflow(raw);

    assert!(
        result.is_ok(),
        "analysis should succeed: {:?}",
        result.errors
    );
    let workflow = result.value.unwrap();

    assert_eq!(workflow.task_count(), 3);
    assert!(workflow.has_task("step1"));
    assert!(workflow.has_task("step2"));
    assert!(workflow.has_task("step3"));
    assert_eq!(workflow.name.as_deref(), Some("Multi-task test"));

    // Verify with: bindings are resolved
    let step2 = workflow.get_task_by_name("step2").unwrap();
    assert_eq!(step2.with_spec.len(), 1);
    assert!(step2.with_spec.contains_key("data"));
}

#[test]
fn test_full_pipeline_all_verbs() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

mcp:
  test_server:
    command: "echo"
    args: ["mock"]

tasks:
  - id: task_infer
    infer:
      prompt: "Generate text"

  - id: task_exec
    exec:
      command: "echo hello"

  - id: task_fetch
    fetch:
      url: "https://api.example.com/data"
      method: GET

  - id: task_invoke
    invoke:
      mcp: test_server
      tool: some_tool

  - id: task_agent
    agent:
      prompt: "Do something complex"
      mcp:
        - test_server
"#;

    let raw = parse_yaml(yaml).expect("parse should succeed");
    let result = analyze_workflow(raw);

    assert!(
        result.is_ok(),
        "analysis should succeed: {:?}",
        result.errors
    );
    let workflow = result.value.unwrap();

    assert_eq!(workflow.task_count(), 5);

    // Verify verb types
    let infer_task = workflow.get_task_by_name("task_infer").unwrap();
    assert_eq!(infer_task.action.verb_name(), "infer");

    let exec_task = workflow.get_task_by_name("task_exec").unwrap();
    assert_eq!(exec_task.action.verb_name(), "exec");

    let fetch_task = workflow.get_task_by_name("task_fetch").unwrap();
    assert_eq!(fetch_task.action.verb_name(), "fetch");

    let invoke_task = workflow.get_task_by_name("task_invoke").unwrap();
    assert_eq!(invoke_task.action.verb_name(), "invoke");

    let agent_task = workflow.get_task_by_name("task_agent").unwrap();
    assert_eq!(agent_task.action.verb_name(), "agent");
}

// ═══════════════════════════════════════════════════════════════════════════
// ERROR COLLECTION TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_error_collection_multiple_errors() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model

tasks:
  - id: task1
    with:
      missing: "$unknown_task"
    infer:
      prompt: "Test"

  - id: task1
    infer:
      prompt: "Duplicate ID"

  - id: task2
    with:
      also_missing: "$another_unknown"
    exec:
      command: "echo"
"#;

    let raw = parse_yaml(yaml).expect("parse should succeed");
    let result = analyze_workflow(raw);

    assert!(result.is_err(), "should have errors");
    let errors = result.errors;

    // Should collect multiple errors (at least duplicate + unknown refs)
    assert!(
        errors.len() >= 2,
        "expected at least 2 errors, got {}",
        errors.len()
    );

    // Check error kinds are present
    let has_duplicate = errors
        .iter()
        .any(|e| e.kind == AnalyzeErrorKind::DuplicateTask);
    let has_unknown = errors
        .iter()
        .any(|e| e.kind == AnalyzeErrorKind::UnknownTask);

    assert!(has_duplicate, "should have duplicate task error");
    assert!(has_unknown, "should have unknown task error");
}

#[test]
fn test_error_collection_invalid_schema() {
    let yaml = r#"
schema: "nika/workflow@0.999"
model: test-model

tasks:
  - id: test
    infer:
      prompt: "Test"
"#;

    let raw = parse_yaml(yaml).expect("parse should succeed");
    let result = analyze_workflow(raw);

    assert!(result.is_err(), "should fail with invalid schema");
    let errors = result.errors;

    assert!(errors
        .iter()
        .any(|e| e.kind == AnalyzeErrorKind::InvalidSchema));
}

#[test]
fn test_error_message_includes_code() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model

tasks:
  - id: test
    with:
      missing: "$nonexistent"
    infer:
      prompt: "Test"
"#;

    let raw = parse_yaml(yaml).expect("parse should succeed");
    let result = analyze_workflow(raw);

    assert!(result.is_err());
    let error = &result.errors[0];

    // Error code should be in NIKA-XXX format
    let code = error.kind.code();
    assert!(
        code.starts_with("NIKA-"),
        "code should be NIKA-XXX format, got {}",
        code
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// SPAN TRACKING TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_span_tracking_preserved() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model

tasks:
  - id: test_task
    infer:
      prompt: "Hello"
"#;

    let raw = parse_yaml(yaml).expect("parse should succeed");
    let result = analyze_workflow(raw);

    assert!(result.is_ok());
    let workflow = result.value.unwrap();

    // Workflow should have a span
    assert!(
        !workflow.span.is_dummy(),
        "workflow span should not be dummy"
    );

    // Task should have a span
    let task = workflow.get_task_by_name("test_task").unwrap();
    assert!(!task.span.is_dummy(), "task span should not be dummy");
}

#[test]
fn test_error_span_points_to_correct_location() {
    let yaml = r#"
schema: "nika/workflow@0.12"

tasks:
  - id: good_task
    infer:
      prompt: "OK"

  - id: bad_task
    with:
      ref: "$nonexistent_task"
    infer:
      prompt: "Error here"
"#;

    let raw = parse_yaml(yaml).expect("parse should succeed");
    let result = analyze_workflow(raw);

    assert!(result.is_err());
    let error = &result.errors[0];

    // Error span should not be dummy
    assert!(
        !error.span.is_dummy(),
        "error span should point to source location"
    );

    // Span should be in a reasonable range (not at the start of file)
    assert!(
        error.span.start > ByteOffset(0),
        "error span should not be at file start"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// FEATURE GATING END-TO-END TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_feature_gate_for_each_v01_rejected() {
    let yaml = r#"
schema: "nika/workflow@0.1"
model: test-model

tasks:
  - id: parallel
    for_each: ["a", "b", "c"]
    as: item
    infer:
      prompt: "Process {{with.item}}"
"#;

    let raw = parse_yaml(yaml).expect("parse should succeed");
    let result = analyze_workflow(raw);

    assert!(result.is_err(), "for_each should be rejected in v0.1");
    let has_feature_error = result
        .errors
        .iter()
        .any(|e| e.kind == AnalyzeErrorKind::UnsupportedFeature);
    assert!(has_feature_error, "should have UnsupportedFeature error");
}

#[test]
fn test_feature_gate_for_each_v03_accepted() {
    let yaml = r#"
schema: "nika/workflow@0.3"
model: test-model

tasks:
  - id: parallel
    for_each: ["a", "b", "c"]
    as: item
    infer:
      prompt: "Process"
"#;

    let raw = parse_yaml(yaml).expect("parse should succeed");
    let result = analyze_workflow(raw);

    assert!(
        result.is_ok(),
        "for_each should work in v0.3: {:?}",
        result.errors
    );
}

#[test]
fn test_feature_gate_invoke_v01_rejected() {
    let yaml = r#"
schema: "nika/workflow@0.1"

mcp:
  server:
    command: "echo"

tasks:
  - id: call_tool
    invoke:
      mcp: server
      tool: some_tool
"#;

    let raw = parse_yaml(yaml).expect("parse should succeed");
    let result = analyze_workflow(raw);

    assert!(result.is_err(), "invoke should be rejected in v0.1");
}

#[test]
fn test_feature_gate_agent_v01_rejected() {
    let yaml = r#"
schema: "nika/workflow@0.1"
model: test-model

tasks:
  - id: ai_agent
    agent:
      prompt: "Do something"
"#;

    let raw = parse_yaml(yaml).expect("parse should succeed");
    let result = analyze_workflow(raw);

    assert!(result.is_err(), "agent should be rejected in v0.1");
}

#[test]
fn test_feature_gate_upgrade_suggestion() {
    let yaml = r#"
schema: "nika/workflow@0.1"
model: test-model

tasks:
  - id: parallel
    for_each: ["a", "b"]
    infer:
      prompt: "Test"
"#;

    let raw = parse_yaml(yaml).expect("parse should succeed");
    let result = analyze_workflow(raw);

    assert!(result.is_err());
    let error = result
        .errors
        .iter()
        .find(|e| e.kind == AnalyzeErrorKind::UnsupportedFeature)
        .expect("should have unsupported feature error");

    // Should have upgrade suggestion
    assert!(error.suggestion.is_some(), "should have upgrade suggestion");
    let suggestion = error.suggestion.as_ref().unwrap();
    assert!(
        suggestion.contains("0.3") || suggestion.contains("upgrade"),
        "suggestion should mention upgrade: {}",
        suggestion
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// SCHEMA VERSION TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_schema_version_parsed_correctly() {
    let versions = [
        ("nika/workflow@0.1", 1),
        ("nika/workflow@0.2", 2),
        ("nika/workflow@0.3", 3),
        ("nika/workflow@0.5", 5),
        ("nika/workflow@0.10", 10),
    ];

    for (schema_str, expected_num) in versions {
        let yaml = format!(
            r#"
schema: "{}"
model: test-model

tasks:
  - id: test
    infer:
      prompt: "Test"
"#,
            schema_str
        );

        let raw = parse_yaml(&yaml).expect("parse should succeed");
        let result = analyze_workflow(raw);

        assert!(
            result.is_ok(),
            "schema {} should be valid: {:?}",
            schema_str,
            result.errors
        );
        let workflow = result.value.unwrap();
        assert_eq!(
            workflow.schema_version.version_number(),
            expected_num,
            "schema {} should have version number {}",
            schema_str,
            expected_num
        );
    }
}

#[test]
fn test_schema_version_suggestion_on_typo() {
    let yaml = r#"
schema: "nika/workflow@0.01"
model: test-model

tasks:
  - id: test
    infer:
      prompt: "Test"
"#;

    let raw = parse_yaml(yaml).expect("parse should succeed");
    let result = analyze_workflow(raw);

    assert!(result.is_err());
    let error = &result.errors[0];
    assert_eq!(error.kind, AnalyzeErrorKind::InvalidSchema);

    // Should suggest correct version
    assert!(
        error.suggestion.is_some(),
        "should have suggestion for typo"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// TASK ID INTERNING TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_task_ids_are_interned() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model

tasks:
  - id: task_a
    infer:
      prompt: "A"

  - id: task_b
    with:
      ref: "$task_a"
    infer:
      prompt: "B"
"#;

    let raw = parse_yaml(yaml).expect("parse should succeed");
    let result = analyze_workflow(raw);

    assert!(
        result.is_ok(),
        "analysis should succeed: {:?}",
        result.errors
    );
    let workflow = result.value.unwrap();

    let task_a = workflow.get_task_by_name("task_a").unwrap();
    let task_b = workflow.get_task_by_name("task_b").unwrap();

    // Task IDs should be valid indices
    assert!((task_a.id.index() as usize) < workflow.task_count());
    assert!((task_b.id.index() as usize) < workflow.task_count());

    // Different tasks should have different IDs
    assert_ne!(task_a.id, task_b.id);

    // with: binding should have created an implicit dep on task_a
    assert!(
        task_b.implicit_deps.contains(&task_a.id),
        "task_b should have implicit dep on task_a from with: binding"
    );
    assert!(
        task_b.with_spec.contains_key("ref"),
        "task_b should have 'ref' in with_spec"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// MCP CONFIG TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_mcp_servers_analyzed() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model

mcp:
  servers:
    server1:
      command: "/path/to/server"
      args: ["--port", "8080"]
      env:
        API_KEY: "secret"

    server2:
      url: "http://localhost:3000/sse"

tasks:
  - id: test
    infer:
      prompt: "Test"
"#;

    let raw = parse_yaml(yaml).expect("parse should succeed");
    let result = analyze_workflow(raw);

    assert!(
        result.is_ok(),
        "analysis should succeed: {:?}",
        result.errors
    );
    let workflow = result.value.unwrap();

    assert_eq!(workflow.mcp_servers.len(), 2);
    assert!(workflow.mcp_servers.contains_key("server1"));
    assert!(workflow.mcp_servers.contains_key("server2"));

    let server1 = workflow.mcp_servers.get("server1").unwrap();
    assert_eq!(server1.command.as_deref(), Some("/path/to/server"));
    assert_eq!(server1.args, vec!["--port", "8080"]);
}

// ═══════════════════════════════════════════════════════════════════════════
// FOR_EACH AND RETRY ANALYSIS TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_for_each_analyzed() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model

tasks:
  - id: parallel_task
    for_each: ["item1", "item2", "item3"]
    as: current_item
    parallel: 3
    infer:
      prompt: "Process"
"#;

    let raw = parse_yaml(yaml).expect("parse should succeed");
    let result = analyze_workflow(raw);

    assert!(
        result.is_ok(),
        "analysis should succeed: {:?}",
        result.errors
    );
    let workflow = result.value.unwrap();

    let task = workflow.get_task_by_name("parallel_task").unwrap();
    assert!(task.for_each.is_some(), "task should have for_each config");

    let for_each = task.for_each.as_ref().unwrap();
    assert_eq!(for_each.as_var, "current_item");
    assert_eq!(for_each.concurrency, Some(Templatable::Value(3)));
}

#[test]
fn test_retry_analyzed() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model

tasks:
  - id: retryable_task
    retry:
      max_attempts: 5
      delay_ms: 2000
      backoff: 1.5
    infer:
      prompt: "May fail"
"#;

    let raw = parse_yaml(yaml).expect("parse should succeed");
    let result = analyze_workflow(raw);

    assert!(
        result.is_ok(),
        "analysis should succeed: {:?}",
        result.errors
    );
    let workflow = result.value.unwrap();

    let task = workflow.get_task_by_name("retryable_task").unwrap();
    assert!(task.retry.is_some(), "task should have retry config");

    let retry = task.retry.as_ref().unwrap();
    assert_eq!(retry.max_attempts, Templatable::Value(5));
    assert_eq!(retry.delay_ms, Templatable::Value(2000));
    assert_eq!(retry.backoff, Some(Templatable::Value(1.5)));
}
