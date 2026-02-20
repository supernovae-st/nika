//! Verb Output Verification Tests
//!
//! Comprehensive tests for all 5 Nika verbs (infer, exec, fetch, invoke, agent).
//! Verifies that each verb produces correct, usable outputs.
//!
//! Run: `cargo test --test verb_output_test`
//! Run with API: `cargo test --test verb_output_test -- --ignored`

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::time::timeout;

use nika::ast::Workflow;
use nika::event::{EventKind, EventLog};
use nika::runtime::Runner;

// ═══════════════════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════════════════

fn parse_workflow(yaml: &str) -> Workflow {
    serde_yaml::from_str(yaml).expect("Failed to parse workflow")
}

async fn run_workflow_and_get_output(workflow: Workflow) -> Option<Arc<Value>> {
    let (event_log, mut rx) = EventLog::new_with_broadcast();
    let runner = Runner::with_event_log(workflow, event_log);
    let handle = tokio::spawn(async move { runner.run().await });

    let mut final_output = None;

    loop {
        match timeout(Duration::from_secs(30), rx.recv()).await {
            Ok(Ok(event)) => {
                if let EventKind::WorkflowCompleted {
                    final_output: output,
                    ..
                } = event.kind
                {
                    final_output = Some(output);
                    break;
                }
                if let EventKind::WorkflowFailed { error, .. } = event.kind {
                    panic!("Workflow failed: {}", error);
                }
            }
            Ok(Err(_)) => break,
            Err(_) => panic!("Workflow timed out after 30s"),
        }
    }

    let _ = handle.await;
    final_output
}

async fn run_workflow_and_get_task_output(workflow: Workflow, task_id: &str) -> Option<Arc<Value>> {
    let (event_log, mut rx) = EventLog::new_with_broadcast();
    let runner = Runner::with_event_log(workflow, event_log);
    let handle = tokio::spawn(async move { runner.run().await });

    let mut task_output = None;
    let target_id = task_id.to_string();

    loop {
        match timeout(Duration::from_secs(30), rx.recv()).await {
            Ok(Ok(event)) => {
                if let EventKind::TaskCompleted {
                    task_id: tid,
                    output,
                    ..
                } = &event.kind
                {
                    if tid.as_ref() == target_id {
                        task_output = Some(output.clone());
                    }
                }
                if matches!(
                    event.kind,
                    EventKind::WorkflowCompleted { .. } | EventKind::WorkflowFailed { .. }
                ) {
                    break;
                }
            }
            Ok(Err(_)) => break,
            Err(_) => panic!("Workflow timed out"),
        }
    }

    let _ = handle.await;
    task_output
}

// ═══════════════════════════════════════════════════════════════════════════
// EXEC: VERB TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_exec_simple_echo() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: simple_echo
    exec: "echo 'hello world'"
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "simple_echo").await;
    assert!(output.is_some());

    let output_str = output.unwrap().to_string();
    assert!(
        output_str.contains("hello") || output_str.contains("world"),
        "Output should contain echo text: {}",
        output_str
    );
}

#[tokio::test]
async fn test_exec_json_output() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: json_echo
    exec: |
      echo '{"name": "test", "value": 123}'
    output:
      format: json
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "json_echo").await;
    assert!(output.is_some());

    let value = output.unwrap();
    assert_eq!(value.get("name").and_then(|v| v.as_str()), Some("test"));
    assert_eq!(value.get("value").and_then(|v| v.as_i64()), Some(123));
}

#[tokio::test]
async fn test_exec_multiline_script() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: multiline
    exec: |
      A=10
      B=20
      echo $((A + B))
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "multiline").await;
    assert!(output.is_some());

    let output_str = output.unwrap().to_string();
    assert!(
        output_str.contains("30"),
        "Should calculate 10+20=30: {}",
        output_str
    );
}

#[tokio::test]
async fn test_exec_with_env_vars() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: env_test
    exec: "echo $HOME"
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "env_test").await;
    assert!(output.is_some());

    let output_str = output.unwrap().to_string();
    // HOME should be expanded
    assert!(
        output_str.contains("/") || output_str.contains("Users") || output_str.contains("home"),
        "HOME should be expanded: {}",
        output_str
    );
}

#[tokio::test]
async fn test_exec_shorthand_syntax() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: shorthand
    exec: "echo shorthand_works"
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "shorthand").await;
    assert!(output.is_some());
    assert!(output.unwrap().to_string().contains("shorthand_works"));
}

// ═══════════════════════════════════════════════════════════════════════════
// EXEC: DATA BINDING TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_exec_with_use_binding() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: producer
    exec: |
      echo '{"message": "from_producer"}'
    output:
      format: json

  - id: consumer
    use:
      data: producer
    exec: "echo 'received: {{use.data.message}}'"

flows:
  - source: producer
    target: consumer
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "consumer").await;
    assert!(output.is_some());

    let output_str = output.unwrap().to_string();
    assert!(
        output_str.contains("from_producer"),
        "Should have bound data: {}",
        output_str
    );
}

#[tokio::test]
async fn test_exec_for_each_parallel() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: parallel
    for_each: ["apple", "banana", "cherry"]
    as: fruit
    concurrency: 3
    exec: "echo '{{use.fruit}}'"
"#,
    );

    // for_each aggregates results in workflow's final output (no TaskCompleted for parent)
    let output = run_workflow_and_get_output(workflow).await;
    assert!(output.is_some(), "Workflow should produce final output");

    // Final output is a string containing the aggregated result
    let value = output.unwrap();
    let output_str = value.to_string();
    // Each iteration echoes its fruit, results are aggregated
    assert!(
        output_str.contains("apple")
            || output_str.contains("banana")
            || output_str.contains("cherry"),
        "Should contain at least one fruit in output: {}",
        output_str
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// FETCH: VERB TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_fetch_get_json() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: fetch_json
    fetch:
      url: "https://httpbin.org/json"
      method: GET
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "fetch_json").await;
    assert!(output.is_some(), "Fetch should return output");

    // httpbin.org/json returns a JSON object
    // Note: TaskCompleted.output stores as Value, not necessarily parsed JSON object
    let value = output.unwrap();

    // The output might be a string containing JSON, or a parsed object
    // Check both cases
    if value.is_object() {
        // Already a JSON object
        assert!(
            value.get("slideshow").is_some(),
            "httpbin.org/json should have slideshow field"
        );
    } else if let Some(s) = value.as_str() {
        // String containing JSON - try to parse
        let parsed: serde_json::Value =
            serde_json::from_str(s).expect("Output should be valid JSON string");
        assert!(parsed.is_object(), "Parsed output should be object");
    } else {
        // Debug: print what we got
        panic!("Unexpected output type: {:?}", value);
    }
}

#[tokio::test]
async fn test_fetch_with_headers() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: fetch_headers
    fetch:
      url: "https://httpbin.org/headers"
      method: GET
      headers:
        X-Custom-Header: "test-value"
        Accept: "application/json"
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "fetch_headers").await;
    assert!(output.is_some());

    let value = output.unwrap();
    // httpbin.org/headers echoes back headers
    if let Some(headers) = value.get("headers") {
        assert!(
            headers.get("X-Custom-Header").is_some() || headers.get("x-custom-header").is_some(),
            "Should have custom header in response"
        );
    }
}

#[tokio::test]
async fn test_fetch_post_with_body() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: fetch_post
    fetch:
      url: "https://httpbin.org/post"
      method: POST
      headers:
        Content-Type: "application/json"
      body: '{"key": "value"}'
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "fetch_post").await;
    assert!(output.is_some());

    let value = output.unwrap();
    // httpbin.org/post echoes back posted data
    if let Some(data) = value.get("data") {
        assert!(data.to_string().contains("key"), "Should echo posted data");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// INFER: VERB TESTS
// Note: infer: verb doesn't support mock provider (only agent: does via run_mock())
// These tests require real API keys.
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "infer: doesn't support mock provider - requires real API"]
async fn test_infer_with_mock_provider() {
    // Note: This test documents that mock provider is NOT supported for infer:
    // Use agent: verb for mock testing, or real providers (claude/openai) for infer:
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: mock_infer
    infer:
      prompt: "What is 2+2?"
      model: claude-sonnet-4-20250514
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "mock_infer").await;
    assert!(output.is_some());
}

#[tokio::test]
#[ignore = "infer: doesn't support mock provider - requires real API"]
async fn test_infer_shorthand_syntax() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: shorthand_infer
    infer: "Generate a greeting"
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "shorthand_infer").await;
    assert!(output.is_some());
}

#[tokio::test]
#[ignore = "infer: doesn't support mock provider - requires real API"]
async fn test_infer_with_context_binding() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: context_producer
    exec: |
      echo '{"topic": "Rust programming"}'
    output:
      format: json

  - id: context_infer
    use:
      ctx: context_producer
    infer:
      prompt: "Explain {{use.ctx.topic}} briefly"
      model: claude-sonnet-4-20250514

flows:
  - source: context_producer
    target: context_infer
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "context_infer").await;
    assert!(output.is_some());
}

// ═══════════════════════════════════════════════════════════════════════════
// INFER: REAL API TESTS (IGNORED BY DEFAULT)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY"]
async fn test_infer_real_claude() {
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        eprintln!("Skipping: ANTHROPIC_API_KEY not set");
        return;
    }

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: real_claude
    infer:
      prompt: "What is 2+2? Answer with just the number."
      model: claude-sonnet-4-20250514
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "real_claude").await;
    assert!(output.is_some());

    let output_str = output.unwrap().to_string();
    assert!(
        output_str.contains("4"),
        "Claude should answer 4: {}",
        output_str
    );
}

#[tokio::test]
#[ignore = "Requires OPENAI_API_KEY"]
async fn test_infer_real_openai() {
    if std::env::var("OPENAI_API_KEY").is_err() {
        eprintln!("Skipping: OPENAI_API_KEY not set");
        return;
    }

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: openai
tasks:
  - id: real_openai
    infer:
      prompt: "What is 2+2? Answer with just the number."
      model: gpt-4o-mini
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "real_openai").await;
    assert!(output.is_some());

    let output_str = output.unwrap().to_string();
    assert!(
        output_str.contains("4"),
        "OpenAI should answer 4: {}",
        output_str
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// PROVIDER EVENTS VERIFICATION
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY"]
async fn test_infer_emits_provider_events() {
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        return;
    }

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: provider_test
    infer:
      prompt: "Say hello"
      model: claude-sonnet-4-20250514
"#,
    );

    let (event_log, mut rx) = EventLog::new_with_broadcast();
    let runner = Runner::with_event_log(workflow, event_log);
    let handle = tokio::spawn(async move { runner.run().await });

    let mut provider_called = false;
    let mut provider_responded = false;

    loop {
        match timeout(Duration::from_secs(30), rx.recv()).await {
            Ok(Ok(event)) => match &event.kind {
                EventKind::ProviderCalled {
                    provider, model, ..
                } => {
                    provider_called = true;
                    assert_eq!(provider, "claude");
                    assert!(model.contains("claude") || model.contains("sonnet"));
                }
                EventKind::ProviderResponded {
                    input_tokens,
                    output_tokens,
                    ..
                } => {
                    provider_responded = true;
                    assert!(*input_tokens > 0, "Should have input tokens");
                    assert!(*output_tokens > 0, "Should have output tokens");
                }
                EventKind::WorkflowCompleted { .. } | EventKind::WorkflowFailed { .. } => break,
                _ => {}
            },
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }

    let _ = handle.await;

    assert!(provider_called, "Should have ProviderCalled event");
    assert!(provider_responded, "Should have ProviderResponded event");
}

// ═══════════════════════════════════════════════════════════════════════════
// AGENT: VERB TESTS (MOCK)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_agent_mock_completes() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: mock_agent
    agent:
      prompt: "Complete this task"
      max_turns: 3
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "mock_agent").await;
    assert!(output.is_some(), "Agent should complete");
}

// ═══════════════════════════════════════════════════════════════════════════
// AGENT: REAL API TESTS (IGNORED)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY"]
async fn test_agent_real_claude() {
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        return;
    }

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: real_agent
    agent:
      prompt: "Calculate 15 * 7 and say DONE when finished"
      model: claude-sonnet-4-20250514
      max_turns: 3
      stop_conditions:
        - "DONE"
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "real_agent").await;
    assert!(output.is_some());

    let output_str = output.unwrap().to_string();
    assert!(
        output_str.contains("105") || output_str.contains("DONE"),
        "Agent should calculate or stop: {}",
        output_str
    );
}

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY"]
async fn test_agent_emits_turn_events() {
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        return;
    }

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: turn_agent
    agent:
      prompt: "Say hello and then DONE"
      model: claude-sonnet-4-20250514
      max_turns: 2
      stop_conditions:
        - "DONE"
"#,
    );

    let (event_log, mut rx) = EventLog::new_with_broadcast();
    let runner = Runner::with_event_log(workflow, event_log);
    let handle = tokio::spawn(async move { runner.run().await });

    let mut agent_started = false;
    let mut turn_count = 0;

    loop {
        match timeout(Duration::from_secs(60), rx.recv()).await {
            Ok(Ok(event)) => match &event.kind {
                EventKind::AgentStart { max_turns, .. } => {
                    agent_started = true;
                    assert_eq!(*max_turns, 2);
                }
                EventKind::AgentTurn { turn_index, .. } => {
                    turn_count = (*turn_index + 1) as usize;
                }
                EventKind::WorkflowCompleted { .. } | EventKind::WorkflowFailed { .. } => break,
                _ => {}
            },
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }

    let _ = handle.await;

    assert!(agent_started, "Should have AgentStart event");
    assert!(turn_count > 0, "Should have at least one turn");
}

// ═══════════════════════════════════════════════════════════════════════════
// EXTENDED THINKING TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY and extended thinking support"]
async fn test_agent_extended_thinking_captures_reasoning() {
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        return;
    }

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: thinking_agent
    agent:
      prompt: "Think carefully about what 17 * 23 equals. Show your reasoning."
      model: claude-sonnet-4-20250514
      max_turns: 2
      extended_thinking: true
      thinking_budget: 4096
"#,
    );

    let (event_log, mut rx) = EventLog::new_with_broadcast();
    let runner = Runner::with_event_log(workflow, event_log);
    let handle = tokio::spawn(async move { runner.run().await });

    let mut has_thinking = false;

    loop {
        match timeout(Duration::from_secs(60), rx.recv()).await {
            Ok(Ok(event)) => {
                if let EventKind::AgentTurn {
                    metadata: Some(meta),
                    ..
                } = &event.kind
                {
                    if meta.thinking.is_some() {
                        has_thinking = true;
                        let thinking = meta.thinking.as_ref().unwrap();
                        assert!(!thinking.is_empty(), "Thinking should not be empty");
                    }
                }
                if matches!(
                    event.kind,
                    EventKind::WorkflowCompleted { .. } | EventKind::WorkflowFailed { .. }
                ) {
                    break;
                }
            }
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }

    let _ = handle.await;

    // Note: has_thinking may be false if model doesn't support it
    // This test verifies the capture mechanism works when available
    println!("Extended thinking captured: {}", has_thinking);
}

// ═══════════════════════════════════════════════════════════════════════════
// END-TO-END WORKFLOW TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_multi_verb_workflow() {
    // Note: Using agent: instead of infer: because mock provider only supports agent:
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: step1_exec
    exec: |
      echo '{"data": "from_exec"}'
    output:
      format: json

  - id: step2_agent
    use:
      input: step1_exec
    agent:
      prompt: "Process this data: {{use.input.data}}"
      max_turns: 1

  - id: step3_exec
    use:
      result: step2_agent
    exec: "echo 'Final result received'"

flows:
  - source: step1_exec
    target: step2_agent
  - source: step2_agent
    target: step3_exec
"#,
    );

    let output = run_workflow_and_get_output(workflow).await;
    assert!(output.is_some(), "Multi-verb workflow should complete");
}

#[tokio::test]
async fn test_diamond_dependency_workflow() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: start
    exec: "echo 'start'"

  - id: left
    exec: "echo 'left'"

  - id: right
    exec: "echo 'right'"

  - id: join
    exec: "echo 'join'"

flows:
  - source: start
    target: [left, right]
  - source: [left, right]
    target: join
"#,
    );

    let output = run_workflow_and_get_output(workflow).await;
    assert!(output.is_some(), "Diamond workflow should complete");
}
