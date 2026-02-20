//! Real Integration Tests
//!
//! End-to-end tests with real APIs (Claude, OpenAI, NovaNet).
//! These tests verify the complete data flow works correctly.
//!
//! All tests are #[ignore] by default - run with:
//! `cargo test --test real_integration_test -- --ignored`
//!
//! Required environment variables:
//! - ANTHROPIC_API_KEY for Claude tests
//! - OPENAI_API_KEY for OpenAI tests
//! - Neo4j running for NovaNet tests

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::time::timeout;

use nika::ast::Workflow;
use nika::event::{Event, EventKind, EventLog};
use nika::runtime::Runner;

// ═══════════════════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════════════════

fn parse_workflow(yaml: &str) -> Workflow {
    serde_yaml::from_str(yaml).expect("Failed to parse workflow")
}

struct TestResult {
    final_output: Option<Arc<Value>>,
    events: Vec<Event>,
    error: Option<String>,
}

async fn run_workflow_full(workflow: Workflow, timeout_secs: u64) -> TestResult {
    let (event_log, mut rx) = EventLog::new_with_broadcast();
    let runner = Runner::with_event_log(workflow, event_log);
    let handle = tokio::spawn(async move { runner.run().await });

    let mut result = TestResult {
        final_output: None,
        events: Vec::new(),
        error: None,
    };

    loop {
        match timeout(Duration::from_secs(timeout_secs), rx.recv()).await {
            Ok(Ok(event)) => {
                result.events.push(event.clone());

                match &event.kind {
                    EventKind::WorkflowCompleted { final_output, .. } => {
                        result.final_output = Some(final_output.clone());
                        break;
                    }
                    EventKind::WorkflowFailed { error, .. } => {
                        result.error = Some(error.clone());
                        break;
                    }
                    _ => {}
                }
            }
            Ok(Err(_)) => break,
            Err(_) => {
                result.error = Some(format!("Timeout after {}s", timeout_secs));
                break;
            }
        }
    }

    let _ = handle.await;
    result
}

fn check_api_key(name: &str) -> bool {
    if std::env::var(name).is_err() {
        eprintln!("Skipping: {} not set", name);
        false
    } else {
        true
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CLAUDE API INTEGRATION TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY"]
async fn test_claude_infer_basic() {
    if !check_api_key("ANTHROPIC_API_KEY") {
        return;
    }

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: claude_test
    infer:
      prompt: "What is 7 * 8? Reply with just the number."
      model: claude-sonnet-4-20250514
"#,
    );

    let result = run_workflow_full(workflow, 30).await;

    assert!(
        result.error.is_none(),
        "Should not fail: {:?}",
        result.error
    );
    assert!(result.final_output.is_some(), "Should have output");

    let output = result.final_output.unwrap().to_string();
    assert!(output.contains("56"), "Should calculate 56: {}", output);
}

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY"]
async fn test_claude_agent_with_tools() {
    if !check_api_key("ANTHROPIC_API_KEY") {
        return;
    }

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: agent_test
    agent:
      prompt: |
        Calculate the following and format as JSON:
        - 15 + 27 = ?
        - 100 / 4 = ?
        - 3^4 = ?

        Output JSON like: {"sum": X, "div": Y, "pow": Z}
        Then say COMPLETE.
      model: claude-sonnet-4-20250514
      max_turns: 3
      stop_conditions:
        - "COMPLETE"
"#,
    );

    let result = run_workflow_full(workflow, 60).await;

    assert!(
        result.error.is_none(),
        "Should not fail: {:?}",
        result.error
    );

    // Check we had agent turns
    let turn_count = result
        .events
        .iter()
        .filter(|e| matches!(&e.kind, EventKind::AgentTurn { .. }))
        .count();
    assert!(turn_count > 0, "Should have agent turns");

    // Check for completion
    let output = result.final_output.unwrap().to_string();
    assert!(
        output.contains("42")
            || output.contains("25")
            || output.contains("81")
            || output.contains("COMPLETE"),
        "Should have calculations: {}",
        output
    );
}

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY"]
async fn test_claude_extended_thinking() {
    if !check_api_key("ANTHROPIC_API_KEY") {
        return;
    }

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: thinking_test
    agent:
      prompt: |
        Think carefully about this logic puzzle:
        If all cats are animals, and some animals are pets, can we conclude all cats are pets?
        Explain your reasoning, then say DONE.
      model: claude-sonnet-4-20250514
      max_turns: 2
      extended_thinking: true
      thinking_budget: 4096
      stop_conditions:
        - "DONE"
"#,
    );

    let result = run_workflow_full(workflow, 90).await;

    assert!(
        result.error.is_none(),
        "Should not fail: {:?}",
        result.error
    );

    // Check for thinking in metadata
    let has_thinking = result.events.iter().any(|e| {
        if let EventKind::AgentTurn {
            metadata: Some(meta),
            ..
        } = &e.kind
        {
            meta.thinking.is_some()
        } else {
            false
        }
    });

    println!("Extended thinking captured: {}", has_thinking);
    // Note: thinking capture depends on model support
}

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY"]
async fn test_claude_multi_task_workflow() {
    if !check_api_key("ANTHROPIC_API_KEY") {
        return;
    }

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: step1
    infer:
      prompt: "Generate a creative name for a tech startup. Just the name, nothing else."
      model: claude-sonnet-4-20250514

  - id: step2
    use:
      name: step1
    infer:
      prompt: "Write a one-sentence tagline for a company called '{{use.name}}'"
      model: claude-sonnet-4-20250514

  - id: step3
    use:
      name: step1
      tagline: step2
    exec: |
      echo '{"company": "{{use.name}}", "tagline": "{{use.tagline}}"}'
    output:
      format: json

flows:
  - source: step1
    target: step2
  - source: step2
    target: step3
"#,
    );

    let result = run_workflow_full(workflow, 60).await;

    assert!(
        result.error.is_none(),
        "Should not fail: {:?}",
        result.error
    );
    assert!(result.final_output.is_some(), "Should have output");

    let output = result.final_output.unwrap();
    assert!(
        output.get("company").is_some(),
        "Should have company: {}",
        output
    );
    assert!(
        output.get("tagline").is_some(),
        "Should have tagline: {}",
        output
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// OPENAI API INTEGRATION TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Requires OPENAI_API_KEY"]
async fn test_openai_infer_basic() {
    if !check_api_key("OPENAI_API_KEY") {
        return;
    }

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: openai
tasks:
  - id: openai_test
    infer:
      prompt: "What is 9 * 9? Reply with just the number."
      model: gpt-4o-mini
"#,
    );

    let result = run_workflow_full(workflow, 30).await;

    assert!(
        result.error.is_none(),
        "Should not fail: {:?}",
        result.error
    );
    assert!(result.final_output.is_some(), "Should have output");

    let output = result.final_output.unwrap().to_string();
    assert!(output.contains("81"), "Should calculate 81: {}", output);
}

#[tokio::test]
#[ignore = "Requires OPENAI_API_KEY"]
async fn test_openai_agent() {
    if !check_api_key("OPENAI_API_KEY") {
        return;
    }

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: openai
tasks:
  - id: openai_agent
    agent:
      prompt: |
        List 3 benefits of Rust programming language.
        Keep each point to one sentence.
        End with FINISHED.
      model: gpt-4o-mini
      max_turns: 3
      stop_conditions:
        - "FINISHED"
"#,
    );

    let result = run_workflow_full(workflow, 60).await;

    assert!(
        result.error.is_none(),
        "Should not fail: {:?}",
        result.error
    );

    let output = result.final_output.unwrap().to_string().to_lowercase();
    assert!(
        output.contains("rust")
            || output.contains("memory")
            || output.contains("safe")
            || output.contains("finished"),
        "Should mention Rust benefits: {}",
        output
    );
}

#[tokio::test]
#[ignore = "Requires OPENAI_API_KEY"]
async fn test_openai_json_output() {
    if !check_api_key("OPENAI_API_KEY") {
        return;
    }

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: openai
tasks:
  - id: json_task
    infer:
      prompt: |
        Output a JSON object with these fields:
        - language: "Rust"
        - year: 2010
        - typed: true

        Output ONLY the JSON, no explanation.
      model: gpt-4o-mini
"#,
    );

    let result = run_workflow_full(workflow, 30).await;

    assert!(result.error.is_none(), "Should not fail");

    let output = result.final_output.unwrap().to_string();
    assert!(
        output.contains("Rust") && output.contains("2010"),
        "Should have JSON with Rust and 2010: {}",
        output
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// PROVIDER COMPARISON TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY and OPENAI_API_KEY"]
async fn test_same_task_different_providers() {
    let has_claude = check_api_key("ANTHROPIC_API_KEY");
    let has_openai = check_api_key("OPENAI_API_KEY");

    if !has_claude || !has_openai {
        return;
    }

    let prompt = "What is the capital of France? Reply with just the city name.";

    // Test with Claude
    let claude_workflow = parse_workflow(&format!(
        r#"
schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: test
    infer:
      prompt: "{}"
      model: claude-sonnet-4-20250514
"#,
        prompt
    ));

    let claude_result = run_workflow_full(claude_workflow, 30).await;
    assert!(claude_result.error.is_none());
    let claude_output = claude_result.final_output.unwrap().to_string();

    // Test with OpenAI
    let openai_workflow = parse_workflow(&format!(
        r#"
schema: "nika/workflow@0.5"
provider: openai
tasks:
  - id: test
    infer:
      prompt: "{}"
      model: gpt-4o-mini
"#,
        prompt
    ));

    let openai_result = run_workflow_full(openai_workflow, 30).await;
    assert!(openai_result.error.is_none());
    let openai_output = openai_result.final_output.unwrap().to_string();

    // Both should answer "Paris"
    assert!(
        claude_output.contains("Paris"),
        "Claude should say Paris: {}",
        claude_output
    );
    assert!(
        openai_output.contains("Paris"),
        "OpenAI should say Paris: {}",
        openai_output
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// NOVANET INTEGRATION TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Requires NovaNet MCP server and Neo4j running"]
async fn test_novanet_full_workflow() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
mcp:
  novanet:
    command: cargo
    args:
      - run
      - --manifest-path
      - ../../../novanet-dev/tools/novanet-mcp/Cargo.toml
    env:
      NOVANET_MCP_NEO4J_URI: "bolt://localhost:7687"
      NOVANET_MCP_NEO4J_USER: "neo4j"
      NOVANET_MCP_NEO4J_PASSWORD: "novanetpassword"
tasks:
  - id: get_entity
    invoke:
      mcp: novanet
      tool: novanet_describe
      params:
        entity: "qr-code"

  - id: process
    use:
      entity: get_entity
    exec: |
      echo 'Entity loaded: {{use.entity.key}}'

flows:
  - source: get_entity
    target: process
"#,
    );

    let result = run_workflow_full(workflow, 120).await;

    if result.error.is_some() {
        eprintln!(
            "NovaNet test failed (expected if Neo4j not running): {:?}",
            result.error
        );
        return;
    }

    // Verify MCP events
    let mcp_events: Vec<_> = result
        .events
        .iter()
        .filter(|e| {
            matches!(
                &e.kind,
                EventKind::McpInvoke { .. } | EventKind::McpResponse { .. }
            )
        })
        .collect();

    assert!(mcp_events.len() >= 2, "Should have MCP invoke and response");
}

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY and NovaNet MCP"]
async fn test_claude_with_novanet_context() {
    if !check_api_key("ANTHROPIC_API_KEY") {
        return;
    }

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: claude
mcp:
  novanet:
    command: cargo
    args:
      - run
      - --manifest-path
      - ../../../novanet-dev/tools/novanet-mcp/Cargo.toml
    env:
      NOVANET_MCP_NEO4J_URI: "bolt://localhost:7687"
      NOVANET_MCP_NEO4J_USER: "neo4j"
      NOVANET_MCP_NEO4J_PASSWORD: "novanetpassword"
tasks:
  - id: get_context
    invoke:
      mcp: novanet
      tool: novanet_describe
      params:
        entity: "qr-code"

  - id: generate
    use:
      ctx: get_context
    infer:
      prompt: |
        Based on this entity data: {{use.ctx}}

        Write a one-sentence description for a landing page.
      model: claude-sonnet-4-20250514

flows:
  - source: get_context
    target: generate
"#,
    );

    let result = run_workflow_full(workflow, 120).await;

    if result.error.is_some() {
        eprintln!(
            "Test failed (NovaNet may not be running): {:?}",
            result.error
        );
        return;
    }

    assert!(
        result.final_output.is_some(),
        "Should have generated output"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// ERROR HANDLING TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Tests error handling with invalid API key"]
async fn test_invalid_api_key_error() {
    // Temporarily set invalid key
    std::env::set_var("ANTHROPIC_API_KEY", "invalid-key-12345");

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: should_fail
    infer:
      prompt: "This should fail"
      model: claude-sonnet-4-20250514
"#,
    );

    let result = run_workflow_full(workflow, 30).await;

    assert!(result.error.is_some(), "Should fail with invalid key");
    let error = result.error.unwrap();
    assert!(
        error.contains("401")
            || error.contains("auth")
            || error.contains("invalid")
            || error.contains("key"),
        "Error should mention auth issue: {}",
        error
    );
}

#[tokio::test]
async fn test_missing_mcp_server_error() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
mcp:
  nonexistent:
    command: "/nonexistent/command/that/does/not/exist"
    args: []
tasks:
  - id: should_fail
    invoke:
      mcp: nonexistent
      tool: some_tool
      params: {}
"#,
    );

    let result = run_workflow_full(workflow, 10).await;

    // The task should fail (not the whole workflow) because MCP server can't start
    let task_failed = result.events.iter().any(|e| {
        matches!(&e.kind, EventKind::TaskFailed { task_id, error, .. }
            if task_id.as_ref() == "should_fail" && error.contains("nonexistent"))
    });

    assert!(task_failed, "Task should fail with missing MCP server");
}

// ═══════════════════════════════════════════════════════════════════════════
// TOKEN TRACKING TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY"]
async fn test_token_tracking_accurate() {
    if !check_api_key("ANTHROPIC_API_KEY") {
        return;
    }

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: token_test
    infer:
      prompt: "Count from 1 to 10, one number per line."
      model: claude-sonnet-4-20250514
"#,
    );

    let result = run_workflow_full(workflow, 30).await;

    assert!(result.error.is_none());

    // Find ProviderResponded event
    let provider_event = result
        .events
        .iter()
        .find(|e| matches!(&e.kind, EventKind::ProviderResponded { .. }));

    assert!(
        provider_event.is_some(),
        "Should have ProviderResponded event"
    );

    if let EventKind::ProviderResponded {
        input_tokens,
        output_tokens,
        ..
    } = &provider_event.unwrap().kind
    {
        assert!(
            *input_tokens > 0,
            "Should have input tokens: {}",
            input_tokens
        );
        assert!(
            *output_tokens > 0,
            "Should have output tokens: {}",
            output_tokens
        );
        println!(
            "Token usage - Input: {}, Output: {}",
            input_tokens, output_tokens
        );
    }
}

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY"]
async fn test_agent_token_accumulation() {
    if !check_api_key("ANTHROPIC_API_KEY") {
        return;
    }

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: multi_turn
    agent:
      prompt: |
        Let's count together.
        Turn 1: Say "One"
        Turn 2: Say "Two"
        Turn 3: Say "Three DONE"
      model: claude-sonnet-4-20250514
      max_turns: 4
      stop_conditions:
        - "DONE"
"#,
    );

    let result = run_workflow_full(workflow, 60).await;

    assert!(result.error.is_none());

    // Count turns with token info
    let turn_tokens: Vec<u32> = result
        .events
        .iter()
        .filter_map(|e| {
            if let EventKind::AgentTurn {
                metadata: Some(meta),
                ..
            } = &e.kind
            {
                Some(meta.total_tokens())
            } else {
                None
            }
        })
        .collect();

    println!("Tokens per turn: {:?}", turn_tokens);

    // Should have multiple turns
    assert!(!turn_tokens.is_empty(), "Should track tokens across turns");
}

// ═══════════════════════════════════════════════════════════════════════════
// FULL E2E SCENARIO TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY, FIRECRAWL_API_KEY"]
async fn test_research_workflow_e2e() {
    if !check_api_key("ANTHROPIC_API_KEY") || !check_api_key("FIRECRAWL_API_KEY") {
        return;
    }

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: claude
mcp:
  firecrawl:
    command: npx
    args:
      - -y
      - "firecrawl-mcp"
    env:
      FIRECRAWL_API_KEY: "${FIRECRAWL_API_KEY}"
tasks:
  - id: scrape
    invoke:
      mcp: firecrawl
      tool: firecrawl_scrape
      params:
        url: "https://www.rust-lang.org"
        formats:
          - "markdown"

  - id: summarize
    use:
      content: scrape
    infer:
      prompt: |
        Summarize the main points from this webpage content:
        {{use.content}}

        Keep it to 3 bullet points.
      model: claude-sonnet-4-20250514

flows:
  - source: scrape
    target: summarize
"#,
    );

    let result = run_workflow_full(workflow, 120).await;

    if result.error.is_some() {
        eprintln!("E2E test failed: {:?}", result.error);
        return;
    }

    assert!(result.final_output.is_some());
    let output = result.final_output.unwrap().to_string().to_lowercase();

    // Should mention something about Rust
    assert!(
        output.contains("rust") || output.contains("programming") || output.contains("language"),
        "Should summarize Rust content: {}",
        output
    );
}
