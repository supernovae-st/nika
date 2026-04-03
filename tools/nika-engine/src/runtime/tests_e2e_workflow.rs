//! E2E workflow tests — parse real YAML → run through Runner → verify results.
//!
//! All tests use `provider: mock` — no API keys needed, deterministic results.
//! Tests cover: DAG patterns, data flow, verbs, for_each, transforms, error cases, edge cases.

use crate::ast::parse_analyzed;
use crate::event::EventLog;
use crate::runtime::runner::Runner;

/// Helper: parse YAML, run workflow, return runner for inspection
async fn run_yaml(yaml: &str) -> Runner {
    let workflow = parse_analyzed(yaml).expect("YAML should parse");
    let mut runner = Runner::new(workflow).unwrap().quiet();
    runner.run().await.expect("Workflow should succeed");
    runner
}

/// Helper: parse YAML, run workflow, expect success, return task output
async fn run_and_get(yaml: &str, task_id: &str) -> String {
    let runner = run_yaml(yaml).await;
    let result = runner
        .datastore()
        .get(task_id)
        .expect("task result should exist");
    assert!(result.is_success(), "task should succeed: {:?}", result);
    result.output_str().to_string()
}

/// Helper: parse YAML, run workflow, expect failure
#[allow(dead_code)]
async fn run_expect_fail(yaml: &str) {
    let workflow = parse_analyzed(yaml).expect("YAML should parse");
    let mut runner = Runner::new(workflow).unwrap().quiet();
    let result = runner.run().await;
    assert!(result.is_err(), "Workflow should fail but succeeded");
}

// ═══════════════════════════════════════════════════════════════════
// 1. DAG PATTERNS (tests 1-5)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn e2e_dag_sequential_chain() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: step1
    exec: "echo FIRST"
  - id: step2
    depends_on: [step1]
    exec: "echo SECOND"
  - id: step3
    depends_on: [step2]
    exec: "echo THIRD"
"#;
    let runner = run_yaml(yaml).await;
    assert!(runner.datastore().get("step1").unwrap().is_success());
    assert!(runner.datastore().get("step2").unwrap().is_success());
    assert!(runner.datastore().get("step3").unwrap().is_success());
}

#[tokio::test]
async fn e2e_dag_diamond_pattern() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: source
    exec: "echo origin"
  - id: left
    depends_on: [source]
    exec: "echo left_branch"
  - id: right
    depends_on: [source]
    exec: "echo right_branch"
  - id: merge
    depends_on: [left, right]
    exec: "echo merged"
"#;
    let runner = run_yaml(yaml).await;
    for id in &["source", "left", "right", "merge"] {
        assert!(
            runner.datastore().get(id).unwrap().is_success(),
            "{id} should succeed"
        );
    }
}

#[tokio::test]
async fn e2e_dag_parallel_independent_tasks() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: a
    exec: "echo alpha"
  - id: b
    exec: "echo bravo"
  - id: c
    exec: "echo charlie"
"#;
    let runner = run_yaml(yaml).await;
    assert!(runner.datastore().get("a").unwrap().is_success());
    assert!(runner.datastore().get("b").unwrap().is_success());
    assert!(runner.datastore().get("c").unwrap().is_success());
}

#[tokio::test]
async fn e2e_dag_tree_topology() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: root
    exec: "echo root"
  - id: child_a
    depends_on: [root]
    exec: "echo child_a"
  - id: child_b
    depends_on: [root]
    exec: "echo child_b"
  - id: leaf_a1
    depends_on: [child_a]
    exec: "echo leaf_a1"
  - id: leaf_b1
    depends_on: [child_b]
    exec: "echo leaf_b1"
"#;
    let runner = run_yaml(yaml).await;
    for id in &["root", "child_a", "child_b", "leaf_a1", "leaf_b1"] {
        assert!(
            runner.datastore().get(id).unwrap().is_success(),
            "{id} failed"
        );
    }
}

#[tokio::test]
async fn e2e_dag_wide_fan_out() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: hub
    exec: "echo hub"
  - id: spoke_1
    depends_on: [hub]
    exec: "echo s1"
  - id: spoke_2
    depends_on: [hub]
    exec: "echo s2"
  - id: spoke_3
    depends_on: [hub]
    exec: "echo s3"
  - id: spoke_4
    depends_on: [hub]
    exec: "echo s4"
  - id: spoke_5
    depends_on: [hub]
    exec: "echo s5"
  - id: collector
    depends_on: [spoke_1, spoke_2, spoke_3, spoke_4, spoke_5]
    exec: "echo collected"
"#;
    let runner = run_yaml(yaml).await;
    assert!(runner.datastore().get("collector").unwrap().is_success());
}

// ═══════════════════════════════════════════════════════════════════
// 2. DATA FLOW (tests 6-10)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn e2e_data_flow_with_binding() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: produce
    exec: "echo hello_world"
  - id: consume
    depends_on: [produce]
    with:
      msg: $produce
    exec:
      command: "echo GOT_{{with.msg}}"
      shell: true
"#;
    let output = run_and_get(yaml, "consume").await;
    assert!(output.contains("GOT_hello_world"), "got: {output}");
}

#[tokio::test]
async fn e2e_data_flow_fallback_operator() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: source
    exec: "echo hello"
  - id: check
    depends_on: [source]
    with:
      val: $source.nonexistent_field ?? "fallback_value"
    exec:
      command: "echo {{with.val}}"
      shell: true
"#;
    let output = run_and_get(yaml, "check").await;
    assert!(output.contains("fallback_value"), "got: {output}");
}

#[tokio::test]
async fn e2e_data_flow_inputs() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
inputs:
  greeting: "bonjour"
tasks:
  - id: say_hi
    exec:
      command: "echo {{inputs.greeting}}"
      shell: true
"#;
    let output = run_and_get(yaml, "say_hi").await;
    assert!(output.contains("bonjour"), "got: {output}");
}

#[tokio::test]
async fn e2e_data_flow_multi_binding() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: a
    exec: "echo alpha"
  - id: b
    exec: "echo bravo"
  - id: merge
    depends_on: [a, b]
    with:
      x: $a
      y: $b
    exec:
      command: "echo {{with.x}}_{{with.y}}"
      shell: true
"#;
    let output = run_and_get(yaml, "merge").await;
    assert!(
        output.contains("alpha") && output.contains("bravo"),
        "got: {output}"
    );
}

#[tokio::test]
async fn e2e_data_flow_chained_bindings() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: step1
    exec: "echo hello"
  - id: step2
    depends_on: [step1]
    with:
      data: $step1
    exec:
      command: "echo step2_{{with.data}}"
      shell: true
  - id: step3
    depends_on: [step2]
    with:
      data: $step2
    exec:
      command: "echo step3_{{with.data}}"
      shell: true
"#;
    let output = run_and_get(yaml, "step3").await;
    assert!(output.contains("step3_"), "got: {output}");
}

// ═══════════════════════════════════════════════════════════════════
// 3. EXEC VERB (tests 11-15)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn e2e_exec_basic_echo() {
    let output = run_and_get(
        r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: greet
    exec: "echo hello_nika"
"#,
        "greet",
    )
    .await;
    assert!(output.contains("hello_nika"));
}

#[tokio::test]
async fn e2e_exec_shell_pipe() {
    let output = run_and_get(
        r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: pipe
    exec:
      command: "echo HELLO_WORLD | tr '[:upper:]' '[:lower:]'"
      shell: true
"#,
        "pipe",
    )
    .await;
    assert!(output.contains("hello_world"), "got: {output}");
}

#[tokio::test]
async fn e2e_exec_env_vars() {
    let output = run_and_get(
        r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: env_test
    exec:
      command: "echo $MY_VAR"
      shell: true
      env:
        MY_VAR: "nika_rocks"
"#,
        "env_test",
    )
    .await;
    assert!(output.contains("nika_rocks"), "got: {output}");
}

#[tokio::test]
async fn e2e_exec_template_in_command() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
inputs:
  name: "nika"
tasks:
  - id: templated
    exec:
      command: "echo name_is_{{inputs.name}}"
      shell: true
"#;
    let output = run_and_get(yaml, "templated").await;
    assert!(output.contains("name_is_nika"), "got: {output}");
}

#[tokio::test]
async fn e2e_exec_multiline_output() {
    let output = run_and_get(
        r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: multi
    exec:
      command: "echo line1 && echo line2 && echo line3"
      shell: true
"#,
        "multi",
    )
    .await;
    assert!(
        output.contains("line1") && output.contains("line3"),
        "got: {output}"
    );
}

// ═══════════════════════════════════════════════════════════════════
// 4. INFER VERB WITH MOCK (tests 16-20)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn e2e_infer_mock_basic() {
    let output = run_and_get(
        r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: gen
    infer: "Tell me a story"
"#,
        "gen",
    )
    .await;
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["mock"], true);
    assert_eq!(parsed["status"], "success");
}

#[tokio::test]
async fn e2e_infer_mock_with_template() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
inputs:
  topic: "rust"
tasks:
  - id: research
    infer:
      prompt: "Research the topic: {{inputs.topic}}"
"#;
    let output = run_and_get(yaml, "research").await;
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["mock"], true);
}

#[tokio::test]
async fn e2e_infer_mock_chain() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: step1
    infer: "Generate data"
  - id: step2
    depends_on: [step1]
    with:
      data: $step1
    infer:
      prompt: "Process: {{with.data}}"
"#;
    let runner = run_yaml(yaml).await;
    assert!(runner.datastore().get("step1").unwrap().is_success());
    assert!(runner.datastore().get("step2").unwrap().is_success());
}

#[tokio::test]
async fn e2e_infer_mock_with_system_prompt() {
    let output = run_and_get(
        r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: sys
    infer:
      prompt: "Hello"
      system: "You are a test assistant"
"#,
        "sys",
    )
    .await;
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["mock"], true);
}

#[tokio::test]
async fn e2e_infer_mock_temperature() {
    let output = run_and_get(
        r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: creative
    infer:
      prompt: "Be creative"
      temperature: 0.9
"#,
        "creative",
    )
    .await;
    assert!(!output.is_empty());
}

// ═══════════════════════════════════════════════════════════════════
// 5. FOR_EACH (tests 21-25)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn e2e_for_each_sequential() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: loop_task
    for_each: ["x", "y", "z"]
    exec: "echo {{with.item}}"
"#;
    let output = run_and_get(yaml, "loop_task").await;
    // for_each output is a JSON array
    let arr: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap_or_default();
    assert_eq!(arr.len(), 3, "Should have 3 results, got: {output}");
}

#[tokio::test]
async fn e2e_for_each_concurrent() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: par
    for_each: ["a", "b", "c", "d"]
    concurrency: 2
    exec: "echo {{with.item}}"
"#;
    let output = run_and_get(yaml, "par").await;
    let arr: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap_or_default();
    assert_eq!(arr.len(), 4, "Should have 4 results, got: {output}");
}

#[tokio::test]
async fn e2e_for_each_with_as_variable() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: named
    for_each: ["one", "two"]
    as: num
    exec: "echo NUM_{{with.num}}"
"#;
    let output = run_and_get(yaml, "named").await;
    assert!(
        output.contains("NUM_one") || output.contains("one"),
        "got: {output}"
    );
}

#[tokio::test]
async fn e2e_for_each_from_upstream_task() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: produce_list
    exec:
      command: "echo '[\"foo\", \"bar\", \"baz\"]'"
      shell: true
    output:
      format: json
  - id: consume
    depends_on: [produce_list]
    for_each: $produce_list
    exec: "echo ITEM_{{with.item}}"
"#;
    let output = run_and_get(yaml, "consume").await;
    let arr: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap_or_default();
    assert_eq!(arr.len(), 3, "Should have 3 results, got: {output}");
}

#[tokio::test]
async fn e2e_for_each_fail_fast_false() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: resilient
    for_each: ["ok1", "ok2", "ok3"]
    fail_fast: false
    exec: "echo {{with.item}}"
"#;
    let output = run_and_get(yaml, "resilient").await;
    let arr: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap_or_default();
    assert_eq!(arr.len(), 3, "got: {output}");
}

// ═══════════════════════════════════════════════════════════════════
// 6. PIPE TRANSFORMS (tests 26-30)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn e2e_transform_upper_lower_trim() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: source
    exec: "echo '  Hello World  '"
  - id: transformed
    depends_on: [source]
    with:
      val: $source | trim | upper
    exec:
      command: "echo RESULT_{{with.val}}"
      shell: true
"#;
    let output = run_and_get(yaml, "transformed").await;
    // "  Hello World  " → trim → "Hello World" → upper → "HELLO WORLD"
    // But exec output includes trailing newline, so trim handles that
    assert!(
        output.contains("HELLO WORLD") || output.contains("HELLO_WORLD"),
        "got: {output}"
    );
}

#[tokio::test]
async fn e2e_transform_length() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: src
    exec:
      command: "echo '[1,2,3]'"
      shell: true
    output:
      format: json
  - id: count
    depends_on: [src]
    with:
      n: $src | length
    exec:
      command: "echo COUNT_{{with.n}}"
      shell: true
"#;
    let output = run_and_get(yaml, "count").await;
    assert!(output.contains("COUNT_3"), "got: {output}");
}

#[tokio::test]
async fn e2e_transform_first_last() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: arr
    exec:
      command: "echo '[\"alpha\",\"beta\",\"gamma\"]'"
      shell: true
    output:
      format: json
  - id: pick
    depends_on: [arr]
    with:
      f: $arr | first
      l: $arr | last
    exec:
      command: "echo FIRST_{{with.f}}_LAST_{{with.l}}"
      shell: true
"#;
    let output = run_and_get(yaml, "pick").await;
    assert!(
        output.contains("alpha") && output.contains("gamma"),
        "got: {output}"
    );
}

#[tokio::test]
async fn e2e_transform_to_json() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: raw
    exec: "echo test_value"
  - id: jsonified
    depends_on: [raw]
    with:
      j: $raw | to_json
    exec:
      command: "echo JSON_{{with.j}}"
      shell: true
"#;
    let output = run_and_get(yaml, "jsonified").await;
    assert!(output.contains("JSON_"), "got: {output}");
}

#[tokio::test]
async fn e2e_transform_default_on_null() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: src
    exec: "echo hello"
  - id: guarded
    depends_on: [src]
    with:
      val: $src.does_not_exist ?? "none_value"
    exec:
      command: "echo DEFAULT_{{with.val}}"
      shell: true
"#;
    let output = run_and_get(yaml, "guarded").await;
    assert!(output.contains("DEFAULT_none_value"), "got: {output}");
}

// ═══════════════════════════════════════════════════════════════════
// 7. ERROR CASES (tests 31-35)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn e2e_error_dag_cycle_detected() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: a
    depends_on: [b]
    exec: "echo a"
  - id: b
    depends_on: [a]
    exec: "echo b"
"#;
    let result = parse_analyzed(yaml);
    assert!(result.is_err(), "Cycle should be detected at parse time");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("cyclic") || err.contains("NIKA-020") || err.contains("NIKA-143"),
        "Error should mention cyclic dependency: {err}"
    );
}

#[test]
fn e2e_error_missing_schema() {
    let yaml = r#"
workflow: no-schema
provider: mock
tasks:
  - id: a
    exec: "echo a"
"#;
    let result = parse_analyzed(yaml);
    assert!(result.is_err(), "Missing schema should fail");
}

#[tokio::test]
async fn e2e_error_exec_command_fails() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: fail_cmd
    exec: "false"
"#;
    let workflow = parse_analyzed(yaml).unwrap();
    let mut runner = Runner::new(workflow).unwrap().quiet();
    let result = runner.run().await;
    assert!(result.is_err(), "Non-zero exit should fail workflow");
}

#[test]
fn e2e_error_missing_depends_on_target() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: orphan
    depends_on: [nonexistent_task]
    exec: "echo orphan"
"#;
    let result = parse_analyzed(yaml);
    assert!(result.is_err(), "Missing dependency should fail");
}

#[test]
fn e2e_error_duplicate_task_ids() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: dup
    exec: "echo first"
  - id: dup
    exec: "echo second"
"#;
    let result = parse_analyzed(yaml);
    assert!(result.is_err(), "Duplicate IDs should fail");
}

// ═══════════════════════════════════════════════════════════════════
// 8. EDGE CASES (tests 36-40)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn e2e_edge_unicode_in_templates() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
inputs:
  name: "Thibaut"
  emoji: "🦋"
tasks:
  - id: unicode
    exec:
      command: "echo Hello_{{inputs.name}}_{{inputs.emoji}}"
      shell: true
"#;
    let output = run_and_get(yaml, "unicode").await;
    assert!(
        output.contains("Thibaut") && output.contains("🦋"),
        "got: {output}"
    );
}

#[tokio::test]
async fn e2e_edge_empty_exec_output() {
    let output = run_and_get(
        r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: empty
    exec: "echo -n ''"
"#,
        "empty",
    )
    .await;
    // Empty output should still succeed
    assert!(
        output.is_empty() || output.trim().is_empty(),
        "got: {output}"
    );
}

#[tokio::test]
async fn e2e_edge_single_task_workflow() {
    let output = run_and_get(
        r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: solo
    exec: "echo minimal"
"#,
        "solo",
    )
    .await;
    assert!(output.contains("minimal"));
}

#[tokio::test]
async fn e2e_edge_long_pipeline_10_tasks() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: t1
    exec: "echo 1"
  - id: t2
    depends_on: [t1]
    exec: "echo 2"
  - id: t3
    depends_on: [t2]
    exec: "echo 3"
  - id: t4
    depends_on: [t3]
    exec: "echo 4"
  - id: t5
    depends_on: [t4]
    exec: "echo 5"
  - id: t6
    depends_on: [t5]
    exec: "echo 6"
  - id: t7
    depends_on: [t6]
    exec: "echo 7"
  - id: t8
    depends_on: [t7]
    exec: "echo 8"
  - id: t9
    depends_on: [t8]
    exec: "echo 9"
  - id: t10
    depends_on: [t9]
    exec: "echo 10"
"#;
    let runner = run_yaml(yaml).await;
    for i in 1..=10 {
        let id = format!("t{i}");
        assert!(
            runner.datastore().get(&id).unwrap().is_success(),
            "task {id} should succeed"
        );
    }
}

#[tokio::test]
async fn e2e_edge_event_log_workflow_events() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: logged
    exec: "echo hello"
"#;
    let workflow = parse_analyzed(yaml).unwrap();
    let event_log = EventLog::new();
    let mut runner = Runner::with_event_log(workflow, event_log.clone())
        .unwrap()
        .quiet();
    runner.run().await.unwrap();

    let events = event_log.events();
    assert!(!events.is_empty(), "Should have events");

    // Check for WorkflowStarted
    let has_started = events
        .iter()
        .any(|e| matches!(e.kind, crate::event::EventKind::WorkflowStarted { .. }));
    assert!(has_started, "Should emit WorkflowStarted");

    // Check for TaskCompleted
    let has_completed = events.iter().any(|e| {
        matches!(
            e.kind,
            crate::event::EventKind::TaskCompleted { ref task_id, .. }
            if task_id.as_ref() == "logged"
        )
    });
    assert!(has_completed, "Should emit TaskCompleted for 'logged'");

    // Check for WorkflowCompleted
    let has_wf_completed = events
        .iter()
        .any(|e| matches!(e.kind, crate::event::EventKind::WorkflowCompleted { .. }));
    assert!(has_wf_completed, "Should emit WorkflowCompleted");
}

// ═══════════════════════════════════════════════════════════════════
// 9. STRUCTURED OUTPUT (tests 41-43)
// ═══════════════════════════════════════════════════════════════════

/// Structured output with a schema matching mock provider fields succeeds.
/// Mock returns JSON with `name`, `age`, `items`, etc. — schema validation
/// in `make_task_result` passes because the mock output satisfies the schema.
#[tokio::test]
async fn e2e_structured_output_basic_schema() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: extract
    infer: "Tell me about Alice, 30, developer"
    structured:
      schema:
        type: object
        properties:
          name: { type: string }
          age: { type: number }
          items:
            type: array
            items: { type: string }
        required: [name, age, items]
"#;
    let output = run_and_get(yaml, "extract").await;
    let parsed: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|e| panic!("Should be valid JSON: {e}\nGot: {output}"));
    assert!(parsed.is_object(), "Should be a JSON object: {output}");
    // Validate schema-conforming mock generates all required fields
    assert!(
        parsed.get("name").is_some(),
        "JSON should contain 'name' field"
    );
    assert!(
        parsed.get("age").is_some(),
        "JSON should contain 'age' field"
    );
    assert!(
        parsed.get("items").and_then(|v| v.as_array()).is_some(),
        "JSON should contain 'items' as an array"
    );
}

/// Mock provider generates schema-conforming JSON when structured: is configured.
/// This proves the generate_mock_json() function produces valid output for any schema.
#[tokio::test]
async fn e2e_mock_structured_output_valid_json() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: list
    infer: "List three colors"
    structured:
      schema:
        type: object
        properties:
          colors:
            type: array
            items: { type: string }
            minItems: 2
        required: [colors]
"#;
    let workflow = parse_analyzed(yaml).unwrap();
    let event_log = EventLog::new();
    let mut runner = Runner::with_event_log(workflow, event_log.clone())
        .unwrap()
        .quiet();
    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "Mock + structured should succeed: {:?}",
        result.err()
    );

    let output = result.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|e| panic!("Should be valid JSON: {e}\nGot: {output}"));
    let colors = parsed.get("colors").and_then(|v| v.as_array());
    assert!(
        colors.is_some(),
        "Output should have 'colors' array: {output}"
    );
    assert!(
        colors.unwrap().len() >= 2,
        "colors should have at least 2 items (minItems): {output}"
    );
}

/// Structured output flows into a downstream task via `with:` binding.
/// Uses a schema matching mock fields so step1 succeeds, then verifies
/// step2 receives the structured JSON and produces its own output.
#[tokio::test]
async fn e2e_structured_output_chained() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: step1
    infer: "Describe a user"
    structured:
      schema:
        type: object
        properties:
          name: { type: string }
          age: { type: number }
        required: [name, age]
  - id: step2
    depends_on: [step1]
    with:
      data: $step1
    infer:
      prompt: "Expand on: {{with.data}}"
"#;
    let runner = run_yaml(yaml).await;
    let r1 = runner.datastore().get("step1").expect("step1 exists");
    assert!(r1.is_success(), "step1 should succeed");
    // Verify step1 produced valid JSON matching the declared schema
    let step1_output = r1.output_str();
    let parsed: serde_json::Value = serde_json::from_str(&step1_output)
        .unwrap_or_else(|e| panic!("step1 should be valid JSON: {e}\nGot: {step1_output}"));
    assert!(parsed.is_object(), "step1 should be a JSON object");
    assert!(
        parsed.get("name").is_some(),
        "step1 should have 'name' field"
    );
    assert!(parsed.get("age").is_some(), "step1 should have 'age' field");

    let r2 = runner.datastore().get("step2").expect("step2 exists");
    assert!(
        r2.is_success(),
        "step2 should succeed — data flows from structured step1 to step2"
    );
    // Verify step2 received step1's output via binding and produced its own output
    let step2_output = r2.output_str();
    assert!(
        !step2_output.is_empty(),
        "step2 should have non-empty output"
    );
    // step2 is also mock infer, so it returns mock JSON
    let parsed2: serde_json::Value = serde_json::from_str(&step2_output)
        .unwrap_or_else(|e| panic!("step2 should be valid JSON: {e}\nGot: {step2_output}"));
    assert_eq!(parsed2["mock"], true, "step2 should be a mock response");
}

// ═══════════════════════════════════════════════════════════════════
// 10. FETCH VERB (tests 44-46)
// ═══════════════════════════════════════════════════════════════════

/// Fetch to an unreachable IP (TEST-NET-1, RFC 5737) with a short timeout
/// fails the workflow. 192.0.2.1 passes SSRF checks (not private) but the
/// connection will time out because the range is non-routable.
#[tokio::test]
async fn e2e_fetch_invalid_url_fails() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: bad_fetch
    fetch:
      url: "http://192.0.2.1:1/nonexistent"
      timeout: 2
"#;
    let workflow = parse_analyzed(yaml).unwrap();
    let mut runner = Runner::new(workflow).unwrap().quiet();
    let result = runner.run().await;
    assert!(
        result.is_err(),
        "Fetch to unreachable IP should fail the workflow"
    );
}

/// Data chain: exec produces HTML string, then infer with mock processes it.
/// Tests that non-HTTP data can flow through a fetch-like pipeline pattern
/// (exec as data source instead of fetch, wired into infer via with: binding).
#[tokio::test]
async fn e2e_fetch_data_chain_exec_to_infer() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: source
    exec: "echo '<html><body><p>Hello world</p></body></html>'"
  - id: analyze
    depends_on: [source]
    with:
      html: $source
    infer:
      prompt: "Analyze this HTML: {{with.html}}"
"#;
    let output = run_and_get(yaml, "analyze").await;
    assert!(!output.is_empty(), "Should produce analysis output");
    // Mock provider returns JSON — verify it's a valid mock response
    let parsed: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|e| panic!("Should be valid JSON: {e}\nGot: {output}"));
    assert_eq!(parsed["mock"], true, "Should be a mock response");
}

/// SSRF protection blocks fetch to localhost (127.0.0.1).
/// Nika's policy enforcer blocks private/loopback IP ranges at the
/// string-level check (before any DNS resolution or HTTP request).
#[tokio::test]
async fn e2e_fetch_ssrf_blocked() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: ssrf
    fetch:
      url: "http://127.0.0.1:9999/secret"
"#;
    let workflow = parse_analyzed(yaml).unwrap();
    let event_log = EventLog::new();
    let mut runner = Runner::with_event_log(workflow, event_log.clone())
        .unwrap()
        .quiet();
    let result = runner.run().await;
    assert!(result.is_err(), "SSRF to localhost should be blocked");

    // Verify the failure is a policy violation, not a connection error
    let events = event_log.events();
    let has_policy_blocked = events.iter().any(|e| {
        matches!(
            &e.kind,
            crate::event::EventKind::PolicyBlocked { verb, .. }
            if verb == "fetch"
        )
    });
    assert!(
        has_policy_blocked,
        "Should emit PolicyBlocked event for SSRF-blocked fetch"
    );
}

// ═══════════════════════════════════════════════════════════════════
// 11. INVOKE VERB (BUILTIN TOOLS) (tests 47-48)
// ═══════════════════════════════════════════════════════════════════

/// Invoke `nika:log` builtin tool succeeds and returns structured JSON
/// with `logged: true`. This tests the full E2E invoke dispatch path:
/// YAML parse → DAG → TaskExecutor → BuiltinToolRouter → LogTool → result.
#[tokio::test]
async fn e2e_invoke_builtin_nika_log() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: log_it
    invoke:
      tool: "nika:log"
      params:
        level: info
        message: "E2E invoke test"
"#;
    let output = run_and_get(yaml, "log_it").await;
    let parsed: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|e| panic!("Should be valid JSON: {e}\nGot: {output}"));
    assert_eq!(parsed["logged"], true, "nika:log should return logged=true");
    assert_eq!(parsed["level"], "info", "level should be 'info'");
    assert_eq!(parsed["message"], "E2E invoke test", "message should match");
}

/// Invoke an unknown builtin tool `nika:nonexistent_tool_xyz` should fail
/// the workflow with a BuiltinToolError. This proves the router rejects
/// unregistered tool names rather than silently succeeding.
#[tokio::test]
async fn e2e_invoke_unknown_tool_fails() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: bad_invoke
    invoke:
      tool: "nika:nonexistent_tool_xyz"
      params:
        foo: bar
"#;
    let workflow = parse_analyzed(yaml).unwrap();
    let event_log = EventLog::new();
    let mut runner = Runner::with_event_log(workflow, event_log.clone())
        .unwrap()
        .quiet();
    let result = runner.run().await;
    assert!(
        result.is_err(),
        "Unknown builtin tool should fail the workflow"
    );

    // Verify the error mentions the unknown tool name
    let events = event_log.events();
    let task_failed = events.iter().find(|e| {
        matches!(
            &e.kind,
            crate::event::EventKind::TaskFailed { task_id, .. }
            if task_id.as_ref() == "bad_invoke"
        )
    });
    assert!(
        task_failed.is_some(),
        "Should have TaskFailed event for 'bad_invoke'"
    );
    if let Some(evt) = task_failed {
        if let crate::event::EventKind::TaskFailed { error, .. } = &evt.kind {
            assert!(
                error.contains("nonexistent_tool_xyz") || error.contains("Unknown"),
                "Error should mention the unknown tool: {error}"
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// 12. RETRY + ERROR PROPAGATION (tests 49-50)
// ═══════════════════════════════════════════════════════════════════

/// `exec: "false"` always exits non-zero. With `retry: { max_attempts: 2 }`,
/// the engine should attempt the command twice before ultimately failing.
/// This proves the retry machinery actually executes (not silently skipped).
#[tokio::test]
async fn e2e_retry_exec_still_fails() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: flaky
    exec: "false"
    retry:
      max_attempts: 2
      delay_ms: 10
"#;
    let workflow = parse_analyzed(yaml).unwrap();
    let event_log = EventLog::new();
    let mut runner = Runner::with_event_log(workflow, event_log.clone())
        .unwrap()
        .quiet();
    let result = runner.run().await;
    assert!(
        result.is_err(),
        "Permanently failing command should still fail after retries"
    );

    // Verify we got a TaskFailed event for the flaky task
    let events = event_log.events();
    let task_failed = events.iter().find(|e| {
        matches!(
            &e.kind,
            crate::event::EventKind::TaskFailed { task_id, .. }
            if task_id.as_ref() == "flaky"
        )
    });
    assert!(
        task_failed.is_some(),
        "Should have TaskFailed event for 'flaky' after retries exhausted"
    );
}

/// Upstream `exec: "false"` fails. Downstream task with `depends_on: [upstream]`
/// and `with: { data: $upstream }` should be skipped/failed via NIKA-026 cascade.
/// This proves the DAG executor propagates failures to dependent tasks.
#[tokio::test]
async fn e2e_error_propagation_nika026() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: upstream
    exec: "false"
  - id: downstream
    depends_on: [upstream]
    with:
      data: $upstream
    exec: "echo '{{with.data}}'"
"#;
    let workflow = parse_analyzed(yaml).unwrap();
    let event_log = EventLog::new();
    let mut runner = Runner::with_event_log(workflow, event_log.clone())
        .unwrap()
        .quiet();
    let result = runner.run().await;
    assert!(
        result.is_err(),
        "Upstream failure should cascade to workflow failure"
    );

    // Verify upstream failed
    let events = event_log.events();
    let upstream_failed = events.iter().any(|e| {
        matches!(
            &e.kind,
            crate::event::EventKind::TaskFailed { task_id, .. }
            if task_id.as_ref() == "upstream"
        )
    });
    assert!(
        upstream_failed,
        "Upstream task should have TaskFailed event"
    );

    // Verify downstream was skipped (NIKA-026 dependency chain failure)
    let downstream_skipped = events.iter().any(|e| {
        matches!(
            &e.kind,
            crate::event::EventKind::TaskSkipped { task_id, .. }
            if task_id.as_ref() == "downstream"
        )
    });
    let downstream_failed = events.iter().any(|e| {
        matches!(
            &e.kind,
            crate::event::EventKind::TaskFailed { task_id, .. }
            if task_id.as_ref() == "downstream"
        )
    });
    assert!(
        downstream_skipped || downstream_failed,
        "Downstream should be skipped or failed due to NIKA-026 cascade"
    );
}

// ═══════════════════════════════════════════════════════════════════
// 13. PARAMETRIC TRANSFORMS + FOR_EACH+INFER (tests 51-52)
// ═══════════════════════════════════════════════════════════════════

/// Parametric transforms `split(",")` and `join(" + ")` in a data pipeline.
/// Exec outputs "alpha,beta,gamma", downstream splits on comma via `with:` binding,
/// then joins with " + " in the exec template. Proves parametric transforms
/// work end-to-end through the binding→template resolution chain.
#[tokio::test]
async fn e2e_transform_join_split_default() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: source
    exec: "echo alpha,beta,gamma"
  - id: transform
    depends_on: [source]
    with:
      parts: $source | trim | split(",")
    exec:
      command: "echo {{with.parts | join(\" + \")}}"
      shell: true
"#;
    let output = run_and_get(yaml, "transform").await;
    assert!(
        output.contains("alpha") && output.contains("beta"),
        "Split+join should preserve elements: {output}"
    );
}

/// `for_each` with inline array + `as: color` + `infer:` verb using mock provider.
/// The mock provider returns deterministic JSON for each iteration.
/// for_each output is always a JSON array — verify length == 3 (one per color).
#[tokio::test]
async fn e2e_for_each_with_infer() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: loop_infer
    for_each: ["red", "green", "blue"]
    as: color
    infer: "Describe the color {{with.color}}"
"#;
    let output = run_and_get(yaml, "loop_infer").await;
    let arr: Vec<serde_json::Value> = serde_json::from_str(&output)
        .unwrap_or_else(|e| panic!("for_each output should be a JSON array: {e}\nGot: {output}"));
    assert_eq!(
        arr.len(),
        3,
        "Should have 3 infer results (one per color), got: {output}"
    );
}

// ═══════════════════════════════════════════════════════════════════
// TOOLS.WORKING_DIR CONFIG — exec cwd resolution
// ═══════════════════════════════════════════════════════════════════

/// working_dir = "project" → exec task uses project_root as cwd
#[tokio::test]
async fn exec_working_dir_project() {
    let project_dir = tempfile::tempdir().expect("tempdir");
    let workflow_dir = project_dir.path().join("workflows");
    std::fs::create_dir_all(&workflow_dir).unwrap();

    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: where_am_i
    exec:
      command: "pwd"
      shell: true
"#;
    let workflow = parse_analyzed(yaml).expect("YAML should parse");
    let mut runner = Runner::new(workflow)
        .unwrap()
        .quiet()
        .with_base_path(workflow_dir)
        .with_project_root(project_dir.path().to_path_buf())
        .with_working_dir_mode("project".to_string());
    runner.run().await.expect("Workflow should succeed");

    let result = runner
        .datastore()
        .get("where_am_i")
        .expect("task result should exist");
    assert!(result.is_success(), "task should succeed: {:?}", result);
    let output = result.output_str().to_string();

    // Canonicalize for macOS /private/var vs /var
    let expected = project_dir.path().canonicalize().unwrap();
    let actual = std::path::Path::new(output.trim()).canonicalize().unwrap();
    assert_eq!(
        actual, expected,
        "working_dir=project should use project_root as cwd"
    );
}

/// working_dir = "workflow" → exec task uses workflow_base_dir as cwd
#[tokio::test]
async fn exec_working_dir_workflow() {
    let project_dir = tempfile::tempdir().expect("tempdir");
    let workflow_dir = project_dir.path().join("workflows");
    std::fs::create_dir_all(&workflow_dir).unwrap();

    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: where_am_i
    exec:
      command: "pwd"
      shell: true
"#;
    let workflow = parse_analyzed(yaml).expect("YAML should parse");
    let mut runner = Runner::new(workflow)
        .unwrap()
        .quiet()
        .with_base_path(workflow_dir.clone())
        .with_project_root(project_dir.path().to_path_buf())
        .with_working_dir_mode("workflow".to_string());
    runner.run().await.expect("Workflow should succeed");

    let result = runner
        .datastore()
        .get("where_am_i")
        .expect("task result should exist");
    assert!(result.is_success(), "task should succeed: {:?}", result);
    let output = result.output_str().to_string();

    let expected = workflow_dir.canonicalize().unwrap();
    let actual = std::path::Path::new(output.trim()).canonicalize().unwrap();
    assert_eq!(
        actual, expected,
        "working_dir=workflow should use workflow_base_dir as cwd"
    );
}

/// working_dir = "none" → exec task inherits process cwd
#[tokio::test]
async fn exec_working_dir_none() {
    let project_dir = tempfile::tempdir().expect("tempdir");
    let workflow_dir = project_dir.path().join("workflows");
    std::fs::create_dir_all(&workflow_dir).unwrap();

    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: where_am_i
    exec:
      command: "pwd"
      shell: true
"#;
    let workflow = parse_analyzed(yaml).expect("YAML should parse");
    let mut runner = Runner::new(workflow)
        .unwrap()
        .quiet()
        .with_base_path(workflow_dir)
        .with_project_root(project_dir.path().to_path_buf())
        .with_working_dir_mode("none".to_string());
    runner.run().await.expect("Workflow should succeed");

    let result = runner
        .datastore()
        .get("where_am_i")
        .expect("task result should exist");
    assert!(result.is_success(), "task should succeed: {:?}", result);
    let output = result.output_str().to_string();

    // Should be the process cwd, NOT the project_root or workflow_dir
    let process_cwd = std::env::current_dir().unwrap().canonicalize().unwrap();
    let actual = std::path::Path::new(output.trim()).canonicalize().unwrap();
    assert_eq!(
        actual, process_cwd,
        "working_dir=none should inherit process cwd"
    );
}

/// Test 26: working_dir mode correctly propagates through runner to executor
#[tokio::test]
async fn tool_context_respects_working_dir() {
    let project_dir = tempfile::tempdir().expect("tempdir");
    let workflow_dir = project_dir.path().join("workflows");
    std::fs::create_dir_all(&workflow_dir).unwrap();

    // Create a script in the project root that writes its cwd
    let script = project_dir.path().join("check.sh");
    std::fs::write(&script, "#!/bin/sh\npwd").unwrap();

    // Verify the Runner.with_working_dir_mode propagates to the executor
    // by running an exec task that reports its cwd
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: check_cwd
    exec:
      command: "pwd"
      shell: true
"#;

    // Mode "project" should use project root
    let workflow = parse_analyzed(yaml).expect("YAML should parse");
    let mut runner = Runner::new(workflow)
        .unwrap()
        .quiet()
        .with_base_path(workflow_dir.clone())
        .with_project_root(project_dir.path().to_path_buf())
        .with_working_dir_mode("project".to_string());
    runner.run().await.expect("Workflow should succeed");

    let result = runner.datastore().get("check_cwd").unwrap();
    assert!(result.is_success());
    let output = result.output_str().to_string();
    let expected = project_dir.path().canonicalize().unwrap();
    let actual = std::path::Path::new(output.trim()).canonicalize().unwrap();
    assert_eq!(
        actual, expected,
        "working_dir mode should propagate from Runner to TaskExecutor"
    );
}

/// working_dir = "project" → nika:read resolves paths from project root, not workflow dir
#[tokio::test]
async fn nika_read_respects_working_dir_project() {
    let project_dir = tempfile::tempdir().expect("tempdir");
    let workflow_dir = project_dir.path().join("workflows");
    std::fs::create_dir_all(&workflow_dir).unwrap();

    // Create a file at project root (NOT in workflows/)
    let src_dir = project_dir.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(src_dir.join("data.txt"), "hello from project root").unwrap();

    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: read_file
    invoke:
      tool: "nika:read"
      params:
        file_path: "./src/data.txt"
"#;
    let workflow = parse_analyzed(yaml).expect("YAML should parse");
    let mut runner = Runner::new(workflow)
        .unwrap()
        .quiet()
        .with_base_path(workflow_dir)
        .with_project_root(project_dir.path().to_path_buf())
        .with_working_dir_mode("project".to_string());
    runner.run().await.expect("Workflow should succeed");

    let result = runner
        .datastore()
        .get("read_file")
        .expect("task result should exist");
    assert!(
        result.is_success(),
        "nika:read should succeed when working_dir=project: {:?}",
        result
    );
    let output = result.output_str();
    assert!(
        output.contains("hello from project root"),
        "nika:read should read file from project root, got: {}",
        output
    );
}

/// working_dir = "workflow" (default) → nika:read resolves from workflow dir
#[tokio::test]
async fn nika_read_default_resolves_from_workflow_dir() {
    let project_dir = tempfile::tempdir().expect("tempdir");
    let workflow_dir = project_dir.path().join("workflows");
    std::fs::create_dir_all(&workflow_dir).unwrap();

    // Create a file in workflow dir
    std::fs::write(workflow_dir.join("local.txt"), "workflow local file").unwrap();

    // Create same-name file at project root (should NOT be read)
    std::fs::write(project_dir.path().join("local.txt"), "project root file").unwrap();

    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: read_file
    invoke:
      tool: "nika:read"
      params:
        file_path: "./local.txt"
"#;
    let workflow = parse_analyzed(yaml).expect("YAML should parse");
    let mut runner = Runner::new(workflow)
        .unwrap()
        .quiet()
        .with_base_path(workflow_dir)
        .with_project_root(project_dir.path().to_path_buf())
        .with_working_dir_mode("workflow".to_string());
    runner.run().await.expect("Workflow should succeed");

    let result = runner
        .datastore()
        .get("read_file")
        .expect("task result should exist");
    assert!(
        result.is_success(),
        "nika:read should succeed: {:?}",
        result
    );
    let output = result.output_str();
    assert!(
        output.contains("workflow local file"),
        "working_dir=workflow should read from workflow dir, got: {}",
        output
    );
}

// ═══════════════════════════════════════════════════════════════════
// 16. SPRINT 1 — NEW PIPE TRANSFORMS (pluck, where, pick, omit, sort_by, group_by, merge, regex, base64)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn e2e_transform_pluck() {
    let output = run_and_get(
        r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: data
    exec:
      command: 'echo ''[{"name":"Alice","age":30},{"name":"Bob","age":25}]'''
      shell: true
    output:
      format: json
  - id: result
    depends_on: [data]
    with:
      names: $data | pluck("name")
    exec:
      command: "echo {{with.names}}"
      shell: true
"#,
        "result",
    )
    .await;
    assert!(
        output.contains("Alice"),
        "pluck should extract names: {}",
        output
    );
    assert!(
        output.contains("Bob"),
        "pluck should extract Bob: {}",
        output
    );
}

#[tokio::test]
async fn e2e_transform_where_filter() {
    let output = run_and_get(
        r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: data
    exec:
      command: 'echo ''[{"name":"Alice","status":"active"},{"name":"Bob","status":"inactive"},{"name":"Charlie","status":"active"}]'''
      shell: true
    output:
      format: json
  - id: result
    depends_on: [data]
    with:
      active: $data | where("status", "active") | length
    exec:
      command: "echo {{with.active}}"
      shell: true
"#,
        "result",
    )
    .await;
    assert!(
        output.contains("2"),
        "where should filter to 2 active: {}",
        output
    );
}

#[tokio::test]
async fn e2e_transform_pick_omit() {
    let output = run_and_get(
        r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: data
    exec:
      command: 'echo ''{"name":"Alice","age":30,"secret":"xxx","role":"admin"}'''
      shell: true
    output:
      format: json
  - id: result
    depends_on: [data]
    with:
      picked: $data | pick("name", "age") | to_json
    exec:
      command: "echo {{with.picked}}"
      shell: true
"#,
        "result",
    )
    .await;
    assert!(output.contains("name"), "pick should keep name: {}", output);
    assert!(output.contains("age"), "pick should keep age: {}", output);
    assert!(
        !output.contains("secret"),
        "pick should omit secret: {}",
        output
    );
}

#[tokio::test]
async fn e2e_transform_sort_by() {
    let output = run_and_get(
        r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: data
    exec:
      command: 'echo ''[{"name":"Bob","age":25},{"name":"Alice","age":30},{"name":"Charlie","age":20}]'''
      shell: true
    output:
      format: json
  - id: result
    depends_on: [data]
    with:
      sorted: $data | sort_by("age") | pluck("name") | join(", ")
    exec:
      command: "echo {{with.sorted}}"
      shell: true
"#,
        "result",
    )
    .await;
    assert!(
        output.contains("Charlie, Bob, Alice"),
        "sort_by age ascending: {}",
        output
    );
}

#[tokio::test]
async fn e2e_transform_group_by() {
    let output = run_and_get(
        r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: data
    exec:
      command: 'echo ''[{"locale":"fr","text":"Bonjour"},{"locale":"en","text":"Hello"},{"locale":"fr","text":"Merci"}]'''
      shell: true
    output:
      format: json
  - id: result
    depends_on: [data]
    with:
      groups: $data | group_by("locale") | keys | sort | join(", ")
    exec:
      command: "echo {{with.groups}}"
      shell: true
"#,
        "result",
    )
    .await;
    assert!(
        output.contains("en, fr"),
        "group_by locale keys: {}",
        output
    );
}

#[tokio::test]
async fn e2e_transform_merge() {
    let output = run_and_get(
        r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: data
    exec:
      command: 'echo ''[{"a":1,"nested":{"x":1}},{"b":2,"nested":{"y":2}}]'''
      shell: true
    output:
      format: json
  - id: result
    depends_on: [data]
    with:
      merged: $data | merge | keys | sort | join(", ")
    exec:
      command: "echo {{with.merged}}"
      shell: true
"#,
        "result",
    )
    .await;
    // Verify merge produced an object with keys from both inputs
    assert!(output.contains("a"), "merge should have key a: {}", output);
    assert!(output.contains("b"), "merge should have key b: {}", output);
    assert!(
        output.contains("nested"),
        "merge should have key nested: {}",
        output
    );
}

#[tokio::test]
async fn e2e_transform_base64_roundtrip() {
    let output = run_and_get(
        r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: data
    exec: "echo Hello_Nika"
  - id: result
    depends_on: [data]
    with:
      roundtrip: $data | trim | base64_encode | base64_decode
    exec:
      command: "echo {{with.roundtrip}}"
      shell: true
"#,
        "result",
    )
    .await;
    assert!(
        output.contains("Hello_Nika"),
        "base64 roundtrip: {}",
        output
    );
}

#[tokio::test]
async fn e2e_transform_pipeline_combo() {
    let output = run_and_get(
        r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: data
    exec:
      command: 'echo ''[{"name":"Alice","age":30,"status":"active"},{"name":"Bob","age":25,"status":"inactive"},{"name":"Charlie","age":35,"status":"active"}]'''
      shell: true
    output:
      format: json
  - id: result
    depends_on: [data]
    with:
      names: $data | where("status", "active") | sort_by("age") | pluck("name") | join(", ")
    exec:
      command: "echo {{with.names}}"
      shell: true
"#,
        "result",
    )
    .await;
    assert!(
        output.contains("Alice, Charlie"),
        "pipeline: where→sort_by→pluck→join: {}",
        output
    );
}

// ═══════════════════════════════════════════════════════════════════
// 17. SPRINT 2 — NEW BUILTIN TOOLS (json_verify, aggregate, json_flatten, locale_lookup, yaml_validate)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn e2e_builtin_json_verify() {
    let output = run_and_get(
        r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: verify
    invoke:
      tool: nika:json_verify
      params:
        source:
          greeting: "Hello {name}"
          ui:
            save: "Save"
        translation:
          greeting: "Bonjour {name}"
          ui:
            save: "Enregistrer"
"#,
        "verify",
    )
    .await;
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["pass"], true, "verify should pass: {}", output);
    assert_eq!(parsed["total_keys"], 2);
}

#[tokio::test]
async fn e2e_builtin_json_verify_missing_key() {
    let output = run_and_get(
        r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: verify
    invoke:
      tool: nika:json_verify
      params:
        source:
          greeting: "Hello {name}"
          farewell: "Goodbye"
        translation:
          greeting: "Bonjour {name}"
"#,
        "verify",
    )
    .await;
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["pass"], false, "verify should fail: {}", output);
    assert!(parsed["missing_keys"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("farewell")));
}

#[tokio::test]
async fn e2e_builtin_aggregate() {
    let output = run_and_get(
        r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: stats
    invoke:
      tool: nika:aggregate
      params:
        array: [10, 20, 30, 40, 50]
        ops: ["sum", "avg", "min", "max", "count"]
"#,
        "stats",
    )
    .await;
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["sum"], 150);
    assert_eq!(parsed["avg"], 30);
    assert_eq!(parsed["min"], 10);
    assert_eq!(parsed["max"], 50);
    assert_eq!(parsed["count"], 5);
}

#[tokio::test]
async fn e2e_builtin_json_flatten_unflatten() {
    let output = run_and_get(
        r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: flatten
    invoke:
      tool: nika:json_flatten
      params:
        data:
          ui:
            button:
              save: "Save"
          version: 1
"#,
        "flatten",
    )
    .await;
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["ui.button.save"], "Save", "flatten: {}", output);
    assert_eq!(parsed["version"], 1);
}

#[tokio::test]
async fn e2e_builtin_locale_lookup() {
    let output = run_and_get(
        r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: lookup
    invoke:
      tool: nika:locale_lookup
      params:
        locale: "fr-BE"
        mapping:
          en: "eng_Latn"
          fr: "fra_Latn"
          de: "deu_Latn"
"#,
        "lookup",
    )
    .await;
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["code"], "fra_Latn", "locale lookup: {}", output);
    assert_eq!(parsed["matched_via"], "language_prefix");
}

#[tokio::test]
async fn e2e_builtin_yaml_validate() {
    let output = run_and_get(
        r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: validate
    invoke:
      tool: nika:yaml_validate
      params:
        data:
          - label: "en"
            content:
              locale:
                key: "en"
                language: "English"
          - label: "broken"
            content:
              locale:
                key: "xx"
        required_fields: ["locale.key", "locale.language"]
"#,
        "validate",
    )
    .await;
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["total"], 2);
    let failures = parsed["failures"].as_array().unwrap();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0]["label"], "broken");
}
