// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Live verb integration tests - test all 5 verbs via workflow execution.
//!
//! Tests each verb (infer, exec, fetch, invoke, agent) by parsing and validating
//! workflow definitions. Execution tests require API keys.

use nika::ast::parse_workflow;
use std::env;

/// Check if we have any LLM provider available
fn has_any_provider() -> bool {
    env::var("ANTHROPIC_API_KEY").is_ok()
        || env::var("OPENAI_API_KEY").is_ok()
        || env::var("MISTRAL_API_KEY").is_ok()
        || env::var("GROQ_API_KEY").is_ok()
}

// ============================================================================
// EXEC VERB PARSING TESTS
// ============================================================================

#[test]
fn test_parse_exec_verb_shorthand() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: run
    exec: "echo 'hello world'"
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse");
    assert_eq!(workflow.tasks.len(), 1);
}

#[test]
fn test_parse_exec_verb_full() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: run
    exec:
      command: "echo 'hello'"
      timeout: 5000
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse");
    assert_eq!(workflow.tasks.len(), 1);
}

#[test]
fn test_parse_exec_with_working_dir() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: run
    exec:
      command: "pwd"
      working_dir: "/tmp"
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse");
    assert_eq!(workflow.tasks.len(), 1);
}

// ============================================================================
// FETCH VERB PARSING TESTS
// ============================================================================

#[test]
fn test_parse_fetch_verb_simple() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: get_data
    fetch:
      url: "https://httpbin.org/get"
      method: GET
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse");
    assert_eq!(workflow.tasks.len(), 1);
}

#[test]
fn test_parse_fetch_verb_post() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: post_data
    fetch:
      url: "https://httpbin.org/post"
      method: POST
      body: '{"name": "test"}'
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse");
    assert_eq!(workflow.tasks.len(), 1);
}

#[test]
fn test_parse_fetch_verb_with_headers() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: fetch_auth
    fetch:
      url: "https://api.example.com/data"
      method: GET
      headers:
        Authorization: "Bearer token123"
        Content-Type: "application/json"
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse");
    assert_eq!(workflow.tasks.len(), 1);
}

// ============================================================================
// INFER VERB PARSING TESTS
// ============================================================================

#[test]
fn test_parse_infer_verb_shorthand() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: generate
    infer: "Write a haiku about Rust"
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse");
    assert_eq!(workflow.tasks.len(), 1);
}

#[test]
fn test_parse_infer_verb_full() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: generate
    infer:
      prompt: "Write a haiku about Rust"
      model: "claude-sonnet-4-20250514"
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse");
    assert_eq!(workflow.tasks.len(), 1);
}

// ============================================================================
// INVOKE VERB PARSING TESTS
// ============================================================================

#[test]
fn test_parse_invoke_verb() {
    let yaml = r#"
schema: "nika/workflow@0.12"
mcp:
  servers:
    novanet:
      command: "node"
      args: ["./mcp-server.js"]
tasks:
  - id: call_tool
    invoke:
      mcp: novanet
      tool: "novanet_describe"
      params:
        entity: "qr-code"
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse");
    assert_eq!(workflow.tasks.len(), 1);
    assert!(workflow.mcp.is_some());
}

// ============================================================================
// AGENT VERB PARSING TESTS
// ============================================================================

#[test]
fn test_parse_agent_verb() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: research
    agent:
      prompt: "Research the topic and provide a summary"
      max_turns: 5
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse");
    assert_eq!(workflow.tasks.len(), 1);
}

#[test]
fn test_parse_agent_verb_with_mcp() {
    let yaml = r#"
schema: "nika/workflow@0.12"
mcp:
  tools:
    command: "node"
    args: ["./tools-server.js"]
tasks:
  - id: research
    agent:
      prompt: "Use available tools to gather information"
      mcp: ["tools"]
      max_turns: 10
      depth_limit: 3
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse");
    assert_eq!(workflow.tasks.len(), 1);
}

// ============================================================================
// ALL VERBS COMBINED PARSING TEST
// ============================================================================

#[test]
fn test_parse_all_verbs_workflow() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude
mcp:
  servers:
    tools:
      command: "node"
      args: ["./tools.js"]

tasks:
  - id: step1_exec
    exec: "echo 'Step 1: exec'"

  - id: step2_fetch
    depends_on: [step1_exec]
    fetch:
      url: "https://httpbin.org/get"
      method: GET

  - id: step3_infer
    depends_on: [step2_fetch]
    infer: "Analyze the data"
    with:
      data: $step2_fetch

  - id: step4_invoke
    depends_on: [step3_infer]
    invoke:
      mcp: tools
      tool: "format_output"
      params:
        input: "{{with.analysis}}"
    with:
      analysis: $step3_infer

  - id: step5_agent
    depends_on: [step4_invoke]
    agent:
      prompt: "Synthesize all results into a report"
      max_turns: 3
    with:
      formatted: $step4_invoke
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse workflow");
    assert_eq!(workflow.tasks.len(), 5);
    // depends_on creates edges via task.flow during lowering
    assert!(workflow.flow_count() >= 4);
    assert!(workflow.mcp.is_some());
}

// ============================================================================
// FOR_EACH MODIFIER TESTS
// ============================================================================

#[test]
fn test_parse_for_each_static_array() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: greet_all
    for_each: ["Alice", "Bob", "Charlie"]
    as: name
    concurrency: 3
    infer: "Say hello to {{with.name}}"
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse");
    let task = &workflow.tasks[0];
    assert!(task.for_each.is_some());
}

#[test]
fn test_parse_for_each_binding() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: get_items
    exec: "echo 'item1,item2,item3'"

  - id: process_items
    for_each: "{{with.items}}"
    as: item
    infer: "Process {{with.item}}"
    with:
      items: $get_items

"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse");
    assert_eq!(workflow.tasks.len(), 2);
}

// ============================================================================
// LIVE PROVIDER TESTS (Require API keys)
// ============================================================================

#[tokio::test]
#[ignore = "Requires API key"]
async fn test_infer_with_provider() {
    use nika::provider::rig::RigProvider;

    if !has_any_provider() {
        eprintln!("Skipping: No API keys available");
        return;
    }

    let provider = RigProvider::auto().expect("Should have a provider");
    let result = provider
        .infer("What is 1+1? Answer with just the number.", None, None)
        .await;

    assert!(result.is_ok());
    assert!(result.unwrap().contains('2'));
}

#[tokio::test]
#[ignore = "Requires API key"]
async fn test_infer_with_context() {
    use nika::provider::rig::RigProvider;

    if !has_any_provider() {
        return;
    }

    let provider = RigProvider::auto().unwrap();

    let result = provider
        .infer(
            "You are a poet. Write a single haiku about Rust programming.",
            None,
            None,
        )
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.len() > 20, "Haiku too short: {}", response);
}
