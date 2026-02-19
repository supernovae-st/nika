//! TDD Tests for RigAgentLoop - rig-core based agentic execution
//!
//! These tests define the expected behavior of the new rig-based agent loop.
//! Following TDD: tests are written FIRST, then implementation.

use rustc_hash::FxHashMap;
use std::sync::Arc;

use nika::ast::AgentParams;
use nika::event::EventLog;
use nika::mcp::McpClient;
use nika::runtime::{RigAgentLoop, RigAgentLoopResult, RigAgentStatus};

// ═══════════════════════════════════════════════════════════════════════════
// RigAgentLoop Creation Tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_rig_agent_loop_creation_with_valid_params() {
    // Arrange - no MCP servers specified, so no clients needed
    let params = AgentParams {
        prompt: "Test prompt for rig agent".to_string(),
        mcp: vec![], // No MCP servers
        max_turns: Some(3),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    // Act
    let result = RigAgentLoop::new("test_task".to_string(), params, event_log, mcp_clients);

    // Assert
    assert!(
        result.is_ok(),
        "Should create RigAgentLoop with valid params"
    );
}

#[test]
fn test_rig_agent_loop_creation_with_empty_prompt_fails() {
    // Arrange
    let params = AgentParams::default(); // Empty prompt

    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();

    // Act
    let result = RigAgentLoop::new("test_task".to_string(), params, event_log, mcp_clients);

    // Assert
    assert!(result.is_err(), "Should fail with empty prompt");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("NIKA-113"),
        "Should be AgentValidationError: {err}"
    );
}

#[test]
fn test_rig_agent_loop_creation_with_zero_max_turns_fails() {
    // Arrange
    let params = AgentParams {
        prompt: "Test prompt".to_string(),
        max_turns: Some(0),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();

    // Act
    let result = RigAgentLoop::new("test_task".to_string(), params, event_log, mcp_clients);

    // Assert
    assert!(result.is_err(), "Should fail with zero max_turns");
}

// ═══════════════════════════════════════════════════════════════════════════
// RigAgentLoop Execution Tests (async)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_rig_agent_loop_runs_to_natural_completion() {
    // Arrange
    let params = AgentParams {
        prompt: "Simple task that should complete immediately".to_string(),
        max_turns: Some(5),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let agent_loop = RigAgentLoop::new(
        "test_natural".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
    )
    .unwrap();

    // Act - Run with mock rig provider (no actual API calls)
    let result = agent_loop.run_mock().await;

    // Assert
    assert!(result.is_ok(), "Agent loop should complete: {:?}", result);
    let result = result.unwrap();
    assert_eq!(
        result.status,
        RigAgentStatus::NaturalCompletion,
        "Should complete naturally (mock returns no tool calls)"
    );
    assert_eq!(result.turns, 1, "Should complete in one turn");
}

#[tokio::test]
async fn test_rig_agent_loop_respects_max_turns() {
    let params = AgentParams {
        prompt: "Task that needs multiple turns".to_string(),
        max_turns: Some(1), // Force early stop
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let agent_loop = RigAgentLoop::new(
        "test_max_turns".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
    )
    .unwrap();

    // Act
    let result = agent_loop.run_mock().await;

    // Assert
    assert!(result.is_ok(), "Agent loop should complete");
    let result = result.unwrap();
    assert!(result.turns <= 1, "Should not exceed max_turns");
}

#[tokio::test]
async fn test_rig_agent_loop_emits_events() {
    // Arrange
    let params = AgentParams {
        prompt: "Task for event tracking".to_string(),
        max_turns: Some(3),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let agent_loop = RigAgentLoop::new(
        "test_events".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
    )
    .unwrap();

    // Act
    let result = agent_loop.run_mock().await;

    // Assert
    assert!(result.is_ok(), "Agent loop should complete");

    // Verify events were emitted
    let events = event_log.events();
    assert!(!events.is_empty(), "Should emit at least one event");

    // Check for agent turn events
    let agent_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(&e.kind, nika::event::EventKind::AgentTurn { .. }))
        .collect();
    assert!(
        !agent_events.is_empty(),
        "Should emit AgentTurn events: {:?}",
        events
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// RigAgentLoop with MCP Tools Tests
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_rig_agent_loop_with_mock_mcp_client() {
    // Arrange
    let params = AgentParams {
        prompt: "Task that uses MCP tools".to_string(),
        mcp: vec!["novanet".to_string()],
        max_turns: Some(5),
        ..Default::default()
    };

    let event_log = EventLog::new();

    // Create mock MCP client
    let mut mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();
    mcp_clients.insert("novanet".to_string(), Arc::new(McpClient::mock("novanet")));

    let agent_loop = RigAgentLoop::new(
        "test_with_mcp".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
    )
    .unwrap();

    // Act
    let result = agent_loop.run_mock().await;

    // Assert
    assert!(result.is_ok(), "Agent loop should complete with MCP client");
}

#[tokio::test]
async fn test_rig_agent_loop_builds_tool_definitions_from_mcp() {
    // Arrange - Create agent loop with MCP client
    let params = AgentParams {
        prompt: "Task requiring tools".to_string(),
        mcp: vec!["novanet".to_string()],
        max_turns: Some(5),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mut mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();
    mcp_clients.insert("novanet".to_string(), Arc::new(McpClient::mock("novanet")));

    let agent_loop = RigAgentLoop::new(
        "test_tools".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
    )
    .unwrap();

    // Act - Get the tool count (via a method we'll add)
    let tool_count = agent_loop.tool_count();

    // Assert - Mock client returns 3 tools
    assert!(tool_count > 0, "Should have tools from MCP client");
}

// ═══════════════════════════════════════════════════════════════════════════
// RigAgentStatus Tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_rig_agent_status_equality() {
    assert_eq!(
        RigAgentStatus::NaturalCompletion,
        RigAgentStatus::NaturalCompletion
    );
    assert_eq!(
        RigAgentStatus::StopConditionMet,
        RigAgentStatus::StopConditionMet
    );
    assert_eq!(
        RigAgentStatus::MaxTurnsReached,
        RigAgentStatus::MaxTurnsReached
    );
    assert_eq!(RigAgentStatus::Failed, RigAgentStatus::Failed);

    assert_ne!(RigAgentStatus::NaturalCompletion, RigAgentStatus::Failed);
}

// ═══════════════════════════════════════════════════════════════════════════
// RigAgentLoopResult Tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_rig_agent_loop_result_debug() {
    let result = RigAgentLoopResult {
        status: RigAgentStatus::NaturalCompletion,
        turns: 3,
        final_output: serde_json::json!({"result": "done"}),
        total_tokens: 150,
    };

    let debug = format!("{:?}", result);
    assert!(debug.contains("NaturalCompletion"));
    assert!(debug.contains("turns"));
}

// ═══════════════════════════════════════════════════════════════════════════
// YAML Workflow Integration Tests
// ═══════════════════════════════════════════════════════════════════════════

/// Test: Parse a workflow YAML with agent: verb and create RigAgentLoop
#[test]
fn test_workflow_yaml_agent_verb_parses_to_agent_params() {
    use nika::ast::{TaskAction, Workflow};

    let yaml = r#"
schema: nika/workflow@0.2

mcp:
  novanet:
    command: "cargo run --bin novanet-mcp"

tasks:
  - id: generate_content
    agent:
      prompt: |
        Generate a landing page for the QR Code entity.
        Use the novanet tools to fetch context and generate content.
      mcp:
        - novanet
      max_turns: 5
      stop_conditions:
        - "TASK_COMPLETE"
    output:
      use.ctx: generated_page
"#;

    // Act: Parse the workflow
    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse workflow YAML");

    // Assert: Workflow parsed correctly
    assert_eq!(workflow.schema, "nika/workflow@0.2");
    assert_eq!(workflow.tasks.len(), 1);

    // Assert: Task has agent verb with correct params
    let task = &workflow.tasks[0];
    assert_eq!(task.id, "generate_content");

    match &task.action {
        TaskAction::Agent { agent } => {
            assert!(agent.prompt.contains("landing page"));
            assert_eq!(agent.mcp, vec!["novanet".to_string()]);
            assert_eq!(agent.max_turns, Some(5));
            assert_eq!(agent.stop_conditions, vec!["TASK_COMPLETE".to_string()]);
        }
        _ => panic!("Expected Agent action, got {:?}", task.action),
    }
}

/// Test: Multi-locale generation workflow pattern (common Nika use case)
#[tokio::test]
async fn test_workflow_multi_locale_generation_pattern() {
    // Simulate: for_each over locales with RigAgentLoop per locale
    let locales = vec!["fr-FR", "es-MX", "de-DE", "ja-JP", "zh-CN"];
    let mut results = Vec::new();

    for locale in &locales {
        let params = AgentParams {
            prompt: format!(
                "Generate native content for QR code entity in {} locale.\n\
                 Use novanet_generate to fetch context.\n\
                 Output should include: title, description, and SEO metadata.",
                locale
            ),
            mcp: vec!["novanet".to_string()],
            max_turns: Some(3),
            stop_conditions: vec!["GENERATION_COMPLETE".to_string()],
            ..Default::default()
        };

        let event_log = EventLog::new();
        let mut mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();
        mcp_clients.insert("novanet".to_string(), Arc::new(McpClient::mock("novanet")));

        let agent_loop = RigAgentLoop::new(
            format!("generate_{}", locale),
            params,
            event_log,
            mcp_clients,
        )
        .unwrap();

        let result = agent_loop.run_mock().await;
        results.push((locale.to_string(), result));
    }

    // Assert: All locales completed successfully
    for (locale, result) in &results {
        assert!(
            result.is_ok(),
            "Generation for {} should succeed: {:?}",
            locale,
            result
        );
    }
    assert_eq!(results.len(), 5, "Should process all 5 locales");
}

/// Test: Stop conditions are checked in output
#[tokio::test]
async fn test_workflow_stop_condition_detection() {
    let params = AgentParams {
        prompt: "Generate content until DONE marker".to_string(),
        mcp: vec![],
        max_turns: Some(10),
        stop_conditions: vec!["DONE".to_string(), "COMPLETE".to_string()],
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let agent_loop = RigAgentLoop::new(
        "test_stop_conditions".to_string(),
        params,
        event_log,
        mcp_clients,
    )
    .unwrap();

    let result = agent_loop.run_mock().await;

    // Assert: Mock completes (doesn't trigger stop conditions)
    assert!(result.is_ok());
    let result = result.unwrap();
    // Mock returns NaturalCompletion (stop conditions not in mock output)
    assert_eq!(result.status, RigAgentStatus::NaturalCompletion);
}

/// Test: MCP client not found for specified server
#[test]
fn test_workflow_mcp_server_not_found_error() {
    let params = AgentParams {
        prompt: "Task requiring missing MCP server".to_string(),
        mcp: vec!["nonexistent_server".to_string()],
        max_turns: Some(3),
        ..Default::default()
    };

    let event_log = EventLog::new();
    // Empty mcp_clients - doesn't contain "nonexistent_server"
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    // Act
    let result = RigAgentLoop::new(
        "test_missing_mcp".to_string(),
        params,
        event_log,
        mcp_clients,
    );

    // Assert: Should fail with McpNotConnected error
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("NIKA-100") || err.to_string().contains("not connected"),
        "Should be McpNotConnected error: {err}"
    );
}

/// Test: Multiple MCP servers in single agent
#[tokio::test]
async fn test_workflow_multiple_mcp_servers() {
    let params = AgentParams {
        prompt: "Task using multiple MCP servers".to_string(),
        mcp: vec!["novanet".to_string(), "filesystem".to_string()],
        max_turns: Some(5),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mut mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();
    mcp_clients.insert("novanet".to_string(), Arc::new(McpClient::mock("novanet")));
    mcp_clients.insert(
        "filesystem".to_string(),
        Arc::new(McpClient::mock("filesystem")),
    );

    let agent_loop =
        RigAgentLoop::new("test_multi_mcp".to_string(), params, event_log, mcp_clients).unwrap();

    // Assert: Tools from both MCP servers are available
    // novanet mock returns 3 tools, filesystem mock returns 3 tools
    assert!(
        agent_loop.tool_count() >= 6,
        "Should have tools from both MCP servers, got {}",
        agent_loop.tool_count()
    );
}

/// Test: Agent with max_turns limit enforced
#[test]
fn test_workflow_max_turns_validation() {
    // max_turns > 100 should fail
    let params = AgentParams {
        prompt: "Test prompt".to_string(),
        mcp: vec![],
        max_turns: Some(101), // Over limit
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let result = RigAgentLoop::new("test_max".to_string(), params, event_log, mcp_clients);

    assert!(result.is_err(), "max_turns > 100 should fail");
}

/// Test: Parse complex workflow with invoke + agent combined
#[test]
fn test_workflow_invoke_then_agent_pattern() {
    use nika::ast::{TaskAction, Workflow};

    let yaml = r#"
schema: nika/workflow@0.2

mcp:
  novanet:
    command: "cargo run --bin novanet-mcp"

tasks:
  - id: fetch_context
    invoke:
      mcp: novanet
      tool: novanet_generate
      params:
        focus_key: "qr-code"
        locale: "fr-FR"
        forms: ["text", "title"]
    output:
      use.ctx: entity_context

  - id: generate_page
    agent:
      prompt: |
        Using the context from $entity_context, generate a complete
        landing page with SEO-optimized content.
      mcp:
        - novanet
      max_turns: 10
    output:
      use.ctx: final_page

flows:
  - source: fetch_context
    target: generate_page
"#;

    // Act
    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse workflow");

    // Assert: Both tasks parsed correctly
    assert_eq!(workflow.tasks.len(), 2);

    // First task is invoke
    assert_eq!(workflow.tasks[0].id, "fetch_context");
    assert!(matches!(
        workflow.tasks[0].action,
        TaskAction::Invoke { .. }
    ));

    // Second task is agent with dependency
    assert_eq!(workflow.tasks[1].id, "generate_page");
    assert!(matches!(workflow.tasks[1].action, TaskAction::Agent { .. }));

    // Check flows at workflow level (DAG edges)
    assert_eq!(workflow.flows.len(), 1, "Should have one flow edge");
}

/// Test: Event log captures all agent turns
#[tokio::test]
async fn test_workflow_event_log_captures_agent_lifecycle() {
    let params = AgentParams {
        prompt: "Generate content with event tracking".to_string(),
        mcp: vec!["novanet".to_string()],
        max_turns: Some(3),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mut mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();
    mcp_clients.insert("novanet".to_string(), Arc::new(McpClient::mock("novanet")));

    let agent_loop = RigAgentLoop::new(
        "test_lifecycle".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
    )
    .unwrap();

    // Act
    let _ = agent_loop.run_mock().await;

    // Assert: Events captured
    let events = event_log.events();

    // Should have at least: started event + completed event
    assert!(events.len() >= 2, "Should capture start and end events");

    // Verify event sequence
    let kinds: Vec<String> = events
        .iter()
        .filter_map(|e| {
            if let nika::event::EventKind::AgentTurn { kind, .. } = &e.kind {
                Some(kind.clone())
            } else {
                None
            }
        })
        .collect();

    assert!(
        kinds.contains(&"started".to_string()),
        "Should have started event: {:?}",
        kinds
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// NovaNet-Specific Use Case Tests
// ═══════════════════════════════════════════════════════════════════════════

/// UC-001: Generate EntityNative content for single locale
#[tokio::test]
async fn test_uc001_generate_entity_native_single_locale() {
    let params = AgentParams {
        prompt: r#"
You are a content generation agent for NovaNet knowledge graph.

Task: Generate native content for the "qr-code" entity in French (fr-FR) locale.

Steps:
1. Use novanet_describe to understand the entity structure
2. Use novanet_generate to create denomination_forms
3. Ensure title, description, and SEO metadata are generated
4. Mark COMPLETE when done.

Focus on high-quality, SEO-optimized French content.
"#
        .to_string(),
        mcp: vec!["novanet".to_string()],
        max_turns: Some(5),
        stop_conditions: vec!["COMPLETE".to_string()],
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mut mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();
    mcp_clients.insert("novanet".to_string(), Arc::new(McpClient::mock("novanet")));

    let agent_loop = RigAgentLoop::new(
        "uc001_entity_native".to_string(),
        params,
        event_log,
        mcp_clients,
    )
    .unwrap();

    let result = agent_loop.run_mock().await;
    assert!(result.is_ok(), "UC-001 should complete: {:?}", result);
}

/// UC-002: Multi-locale pipeline (5 locales parallel simulation)
#[tokio::test]
async fn test_uc002_multi_locale_pipeline() {
    use tokio::task::JoinSet;

    let locales = vec![
        ("fr-FR", "French France"),
        ("es-MX", "Spanish Mexico"),
        ("de-DE", "German Germany"),
        ("ja-JP", "Japanese Japan"),
        ("pt-BR", "Portuguese Brazil"),
    ];

    let mut join_set = JoinSet::new();

    for (locale_code, locale_name) in locales.clone() {
        let params = AgentParams {
            prompt: format!(
                "Generate landing page content for QR code in {} ({}).\n\
                 Use NovaNet tools. Output DONE when complete.",
                locale_name, locale_code
            ),
            mcp: vec!["novanet".to_string()],
            max_turns: Some(3),
            stop_conditions: vec!["DONE".to_string()],
            ..Default::default()
        };

        let event_log = EventLog::new();
        let mut mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();
        mcp_clients.insert("novanet".to_string(), Arc::new(McpClient::mock("novanet")));

        let agent_loop = RigAgentLoop::new(
            format!("uc002_{}", locale_code),
            params,
            event_log,
            mcp_clients,
        )
        .unwrap();

        // Spawn parallel execution using tokio JoinSet
        let locale = locale_code.to_string();
        join_set.spawn(async move {
            let result = agent_loop.run_mock().await;
            (locale, result)
        });
    }

    // Collect all results
    let mut results = Vec::new();
    while let Some(join_result) = join_set.join_next().await {
        results.push(join_result.expect("Task should not panic"));
    }

    // Verify all succeeded
    for (locale, result) in &results {
        assert!(
            result.is_ok(),
            "UC-002 locale {} should complete: {:?}",
            locale,
            result
        );
    }
    assert_eq!(results.len(), 5, "Should process all 5 locales");
}

/// UC-003: SEO content refinement agent
#[tokio::test]
async fn test_uc003_seo_refinement_agent() {
    let params = AgentParams {
        prompt: r#"
You are an SEO optimization agent for QR code landing pages.

Task: Analyze and refine SEO metadata for the fr-FR landing page.

Steps:
1. Use novanet_search to find current SEO keywords
2. Analyze keyword competition and volume
3. Suggest title tag and meta description improvements
4. Output OPTIMIZED when refinements are complete.
"#
        .to_string(),
        mcp: vec!["novanet".to_string()],
        max_turns: Some(5),
        stop_conditions: vec!["OPTIMIZED".to_string()],
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mut mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();
    mcp_clients.insert("novanet".to_string(), Arc::new(McpClient::mock("novanet")));

    let agent_loop = RigAgentLoop::new(
        "uc003_seo_refinement".to_string(),
        params,
        event_log,
        mcp_clients,
    )
    .unwrap();

    let result = agent_loop.run_mock().await;
    assert!(result.is_ok(), "UC-003 should complete");
}
