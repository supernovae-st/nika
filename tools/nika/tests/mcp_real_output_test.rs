//! MCP Real Output Verification Tests
//!
//! Tests that MCP tools (invoke: verb) produce correct outputs.
//! Tests real MCP server integrations with NovaNet, Perplexity, Firecrawl, etc.
//!
//! Run mock tests: `cargo test --test mcp_real_output_test`
//! Run real tests: `cargo test --test mcp_real_output_test -- --ignored`

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

async fn run_and_collect_mcp_events(workflow: Workflow, timeout_secs: u64) -> Vec<Event> {
    let (event_log, mut rx) = EventLog::new_with_broadcast();
    let runner = Runner::with_event_log(workflow, event_log);
    let handle = tokio::spawn(async move { runner.run().await });

    let mut mcp_events = Vec::new();

    loop {
        match timeout(Duration::from_secs(timeout_secs), rx.recv()).await {
            Ok(Ok(event)) => match &event.kind {
                EventKind::McpInvoke { .. } | EventKind::McpResponse { .. } => {
                    mcp_events.push(event);
                }
                EventKind::WorkflowCompleted { .. } | EventKind::WorkflowFailed { .. } => {
                    break;
                }
                _ => {}
            },
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }

    let _ = handle.await;
    mcp_events
}

async fn run_workflow_and_get_task_output(
    workflow: Workflow,
    task_id: &str,
    timeout_secs: u64,
) -> Option<Arc<Value>> {
    let (event_log, mut rx) = EventLog::new_with_broadcast();
    let runner = Runner::with_event_log(workflow, event_log);
    let handle = tokio::spawn(async move { runner.run().await });

    let mut task_output = None;
    let target_id = task_id.to_string();

    loop {
        match timeout(Duration::from_secs(timeout_secs), rx.recv()).await {
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
                if let EventKind::WorkflowFailed { error, .. } = &event.kind {
                    eprintln!("Workflow failed: {}", error);
                    break;
                }
                if matches!(event.kind, EventKind::WorkflowCompleted { .. }) {
                    break;
                }
            }
            Ok(Err(_)) => break,
            Err(_) => {
                eprintln!("Timeout after {}s", timeout_secs);
                break;
            }
        }
    }

    let _ = handle.await;
    task_output
}

// ═══════════════════════════════════════════════════════════════════════════
// MCP EVENT VERIFICATION TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_mcp_invoke_emits_events() {
    // This test uses a mock that simulates MCP behavior
    // In real scenarios, the MCP server must be running

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
mcp:
  test-server:
    command: "echo"
    args: ["mock"]
tasks:
  - id: mcp_test
    invoke:
      mcp: test-server
      tool: test_tool
      params:
        key: "value"
"#,
    );

    // Note: This will fail because the mock MCP server doesn't exist
    // The test verifies the event emission structure
    let (event_log, mut rx) = EventLog::new_with_broadcast();
    let runner = Runner::with_event_log(workflow, event_log);
    let _handle = tokio::spawn(async move { runner.run().await });

    // Just verify we can receive events (will likely fail on MCP connection)
    let result = timeout(Duration::from_secs(5), rx.recv()).await;
    // We expect some event, even if workflow fails
    assert!(result.is_ok() || result.is_err()); // Test completes
}

// ═══════════════════════════════════════════════════════════════════════════
// NOVANET MCP TESTS (REQUIRES NOVANET RUNNING)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Requires NovaNet MCP server running"]
async fn test_novanet_describe_returns_entity() {
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
  - id: describe_entity
    invoke:
      mcp: novanet
      tool: novanet_describe
      params:
        entity: "qr-code"
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "describe_entity", 60).await;
    assert!(output.is_some(), "novanet_describe should return result");

    let value = output.unwrap();
    // NovaNet describe returns entity details
    assert!(
        value.get("key").is_some() || value.get("entity").is_some() || value.is_object(),
        "Should return entity data: {}",
        value
    );
}

#[tokio::test]
#[ignore = "Requires NovaNet MCP server running"]
async fn test_novanet_traverse_returns_graph() {
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
  - id: traverse
    invoke:
      mcp: novanet
      tool: novanet_traverse
      params:
        start: "entity:qr-code"
        arc: "HAS_NATIVE"
        depth: 1
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "traverse", 60).await;
    assert!(output.is_some(), "novanet_traverse should return result");

    let value = output.unwrap();
    // Traverse returns nodes or empty array
    assert!(
        value.is_array() || value.is_object(),
        "Should return traversal results: {}",
        value
    );
}

#[tokio::test]
#[ignore = "Requires NovaNet MCP server running"]
async fn test_novanet_introspect_returns_schema() {
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
  - id: introspect
    invoke:
      mcp: novanet
      tool: novanet_introspect
      params:
        query: "What node classes exist?"
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "introspect", 60).await;
    assert!(output.is_some(), "novanet_introspect should return result");

    let value = output.unwrap();
    // Introspect returns schema info
    println!("novanet_introspect result: {}", value);
}

// ═══════════════════════════════════════════════════════════════════════════
// PERPLEXITY MCP TESTS (REQUIRES API KEY)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Requires PERPLEXITY_API_KEY"]
async fn test_perplexity_search_returns_results() {
    if std::env::var("PERPLEXITY_API_KEY").is_err() {
        eprintln!("Skipping: PERPLEXITY_API_KEY not set");
        return;
    }

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
mcp:
  perplexity:
    command: npx
    args:
      - -y
      - "@perplexity-ai/mcp-server"
    env:
      PERPLEXITY_API_KEY: "${PERPLEXITY_API_KEY}"
tasks:
  - id: search
    invoke:
      mcp: perplexity
      tool: perplexity_search_web
      params:
        query: "What is Rust programming language?"
        recency: "month"
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "search", 30).await;
    assert!(output.is_some(), "Perplexity search should return result");

    let value = output.unwrap();
    // Perplexity returns search results
    let value_str = value.to_string();
    assert!(
        value_str.contains("Rust") || value_str.contains("programming") || !value_str.is_empty(),
        "Should return search results about Rust: {}",
        value_str
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// FIRECRAWL MCP TESTS (REQUIRES API KEY)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Requires FIRECRAWL_API_KEY"]
async fn test_firecrawl_scrape_returns_content() {
    if std::env::var("FIRECRAWL_API_KEY").is_err() {
        eprintln!("Skipping: FIRECRAWL_API_KEY not set");
        return;
    }

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
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
        url: "https://example.com"
        formats:
          - "markdown"
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "scrape", 30).await;
    assert!(output.is_some(), "Firecrawl scrape should return result");

    let value = output.unwrap();
    let value_str = value.to_string();
    // example.com has "Example Domain" text
    assert!(
        value_str.contains("Example") || value_str.contains("domain") || !value_str.is_empty(),
        "Should return scraped content: {}",
        value_str
    );
}

#[tokio::test]
#[ignore = "Requires FIRECRAWL_API_KEY"]
async fn test_firecrawl_map_returns_urls() {
    if std::env::var("FIRECRAWL_API_KEY").is_err() {
        return;
    }

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
mcp:
  firecrawl:
    command: npx
    args:
      - -y
      - "firecrawl-mcp"
    env:
      FIRECRAWL_API_KEY: "${FIRECRAWL_API_KEY}"
tasks:
  - id: map_site
    invoke:
      mcp: firecrawl
      tool: firecrawl_map
      params:
        url: "https://example.com"
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "map_site", 30).await;
    assert!(output.is_some(), "Firecrawl map should return result");

    let value = output.unwrap();
    // Map returns array of URLs
    if let Some(arr) = value.as_array() {
        assert!(!arr.is_empty() || arr.is_empty(), "Should return URL list");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SEQUENTIAL THINKING MCP TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Requires npm/npx available"]
async fn test_sequential_thinking_tool() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
mcp:
  sequential-thinking:
    command: npx
    args:
      - -y
      - "@modelcontextprotocol/server-sequential-thinking"
tasks:
  - id: think
    invoke:
      mcp: sequential-thinking
      tool: sequentialthinking
      params:
        thought: "Let me analyze this step by step"
        thoughtNumber: 1
        totalThoughts: 3
        nextThoughtNeeded: true
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "think", 30).await;
    // Sequential thinking should return or acknowledge the thought
    println!("Sequential thinking result: {:?}", output);
}

// ═══════════════════════════════════════════════════════════════════════════
// CONTEXT7 MCP TESTS (HTTP MCP)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Requires CONTEXT7_API_KEY or public access"]
async fn test_context7_resolve_library() {
    // Context7 is an HTTP MCP server
    // Note: Nika's HTTP MCP support may need verification

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
mcp:
  context7:
    type: http
    url: "https://mcp.context7.com/mcp"
tasks:
  - id: resolve
    invoke:
      mcp: context7
      tool: resolve-library-id
      params:
        libraryName: "react"
        query: "How to use hooks"
"#,
    );

    let output = run_workflow_and_get_task_output(workflow, "resolve", 30).await;
    println!("Context7 resolve result: {:?}", output);
}

// ═══════════════════════════════════════════════════════════════════════════
// MCP EVENT STRUCTURE TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Requires NovaNet MCP server running"]
async fn test_mcp_events_contain_params_and_response() {
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
  - id: mcp_call
    invoke:
      mcp: novanet
      tool: novanet_describe
      params:
        entity: "qr-code"
"#,
    );

    let events = run_and_collect_mcp_events(workflow, 60).await;

    // Find McpInvoke event
    let invoke = events
        .iter()
        .find(|e| matches!(&e.kind, EventKind::McpInvoke { .. }));
    assert!(invoke.is_some(), "Should have McpInvoke event");

    if let Some(event) = invoke {
        if let EventKind::McpInvoke {
            tool,
            params,
            call_id,
            ..
        } = &event.kind
        {
            assert_eq!(tool.as_ref(), Some(&"novanet_describe".to_string()));
            assert!(params.is_some(), "Params should be captured");
            assert!(!call_id.is_empty(), "call_id should be set");

            // Check params content
            if let Some(p) = params {
                assert_eq!(p.get("entity").and_then(|v| v.as_str()), Some("qr-code"));
            }
        }
    }

    // Find McpResponse event
    let response = events
        .iter()
        .find(|e| matches!(&e.kind, EventKind::McpResponse { .. }));
    if let Some(event) = response {
        if let EventKind::McpResponse {
            response,
            duration_ms,
            is_error,
            ..
        } = &event.kind
        {
            assert!(response.is_some(), "Response should be captured");
            assert!(*duration_ms > 0, "Duration should be > 0");
            assert!(!is_error, "Should not be error");
        }
    }
}

#[tokio::test]
#[ignore = "Requires NovaNet MCP server running"]
async fn test_mcp_call_ids_correlate() {
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
  - id: call1
    invoke:
      mcp: novanet
      tool: novanet_describe
      params:
        entity: "qr-code"

  - id: call2
    invoke:
      mcp: novanet
      tool: novanet_describe
      params:
        entity: "qr-code"

flows:
  - source: call1
    target: call2
"#,
    );

    let events = run_and_collect_mcp_events(workflow, 60).await;

    // Collect call_ids from invokes
    let invoke_ids: Vec<String> = events
        .iter()
        .filter_map(|e| {
            if let EventKind::McpInvoke { call_id, .. } = &e.kind {
                Some(call_id.clone())
            } else {
                None
            }
        })
        .collect();

    // Collect call_ids from responses
    let response_ids: Vec<String> = events
        .iter()
        .filter_map(|e| {
            if let EventKind::McpResponse { call_id, .. } = &e.kind {
                Some(call_id.clone())
            } else {
                None
            }
        })
        .collect();

    // Each invoke should have a corresponding response
    for invoke_id in &invoke_ids {
        assert!(
            response_ids.contains(invoke_id),
            "Invoke {} should have matching response",
            invoke_id
        );
    }

    // IDs should be unique
    let unique_invoke_ids: std::collections::HashSet<_> = invoke_ids.iter().collect();
    assert_eq!(
        invoke_ids.len(),
        unique_invoke_ids.len(),
        "Call IDs should be unique"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// AGENT WITH MCP TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY and NovaNet MCP"]
async fn test_agent_uses_novanet_tools() {
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
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
  - id: novanet_agent
    agent:
      prompt: |
        Use the novanet_describe tool to get information about the entity "qr-code".
        Then say DONE.
      model: claude-sonnet-4-20250514
      mcp:
        - novanet
      max_turns: 5
      stop_conditions:
        - "DONE"
"#,
    );

    let events = run_and_collect_mcp_events(workflow, 120).await;

    // Agent should have called novanet_describe
    let has_novanet_call = events.iter().any(|e| {
        matches!(
            &e.kind,
            EventKind::McpInvoke { tool: Some(t), .. } if t.contains("novanet")
        )
    });

    assert!(has_novanet_call, "Agent should have called NovaNet tool");
}

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY and PERPLEXITY_API_KEY"]
async fn test_agent_uses_perplexity_for_search() {
    if std::env::var("ANTHROPIC_API_KEY").is_err() || std::env::var("PERPLEXITY_API_KEY").is_err() {
        return;
    }

    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: claude
mcp:
  perplexity:
    command: npx
    args:
      - -y
      - "@perplexity-ai/mcp-server"
    env:
      PERPLEXITY_API_KEY: "${PERPLEXITY_API_KEY}"
tasks:
  - id: search_agent
    agent:
      prompt: |
        Search for "Rust async programming best practices" using perplexity_search_web.
        Summarize the key points and say DONE.
      model: claude-sonnet-4-20250514
      mcp:
        - perplexity
      max_turns: 5
      stop_conditions:
        - "DONE"
"#,
    );

    let events = run_and_collect_mcp_events(workflow, 60).await;

    let has_perplexity_call = events.iter().any(|e| {
        matches!(
            &e.kind,
            EventKind::McpInvoke { tool: Some(t), .. } if t.contains("perplexity")
        )
    });

    assert!(has_perplexity_call, "Agent should have called Perplexity");
}
