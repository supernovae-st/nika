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
    let result = runner.datastore().get(task_id).expect("task result should exist");
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
        assert!(runner.datastore().get(id).unwrap().is_success(), "{id} should succeed");
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
        assert!(runner.datastore().get(id).unwrap().is_success(), "{id} failed");
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
    assert!(output.contains("alpha") && output.contains("bravo"), "got: {output}");
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
    assert!(output.contains("line1") && output.contains("line3"), "got: {output}");
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
    assert!(output.contains("NUM_one") || output.contains("one"), "got: {output}");
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
    assert!(output.contains("alpha") && output.contains("gamma"), "got: {output}");
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
    assert!(output.contains("Thibaut") && output.contains("🦋"), "got: {output}");
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
    assert!(output.is_empty() || output.trim().is_empty(), "got: {output}");
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
    let mut runner = Runner::with_event_log(workflow, event_log.clone()).unwrap().quiet();
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
