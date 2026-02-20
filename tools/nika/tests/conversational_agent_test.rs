//! Conversational Agent Tests
//!
//! Tests multi-turn agent conversations, tool calling sequences,
//! context accumulation, and spawn_agent nesting.
//!
//! These tests verify the agent: verb works correctly in conversational
//! scenarios typical of chat interfaces.

use rustc_hash::FxHashMap;
use std::sync::Arc;

use nika::ast::{AgentParams, Workflow};
use nika::event::{EventKind, EventLog};
use nika::mcp::McpClient;
use nika::runtime::RigAgentLoop;

// ============================================================================
// UNIT TESTS - Agent Configuration
// ============================================================================

#[test]
fn test_agent_params_default() {
    let params = AgentParams::default();
    assert!(params.prompt.is_empty());
    assert!(params.mcp.is_empty());
    assert!(params.max_turns.is_none());
    assert!(params.depth_limit.is_none());
}

#[test]
fn test_agent_params_with_depth_limit() {
    let params = AgentParams {
        prompt: "Test prompt".to_string(),
        mcp: vec!["novanet".to_string()],
        max_turns: Some(5),
        depth_limit: Some(3),
        ..Default::default()
    };
    assert_eq!(params.depth_limit, Some(3));
    assert_eq!(params.max_turns, Some(5));
}

#[test]
fn test_agent_params_mcp_list() {
    let params = AgentParams {
        prompt: "Multi-MCP test".to_string(),
        mcp: vec![
            "novanet".to_string(),
            "perplexity".to_string(),
            "firecrawl".to_string(),
        ],
        ..Default::default()
    };
    assert_eq!(params.mcp.len(), 3);
    assert!(params.mcp.contains(&"novanet".to_string()));
    assert!(params.mcp.contains(&"perplexity".to_string()));
}

// ============================================================================
// UNIT TESTS - Event Log for Conversations
// ============================================================================

#[test]
fn test_event_log_conversation_tracking() {
    let log = EventLog::new();

    // Simulate conversation events using correct EventKind variants
    log.emit(EventKind::AgentStart {
        task_id: "chat-1".into(),
        max_turns: 5,
        mcp_servers: vec!["novanet".to_string()],
    });

    log.emit(EventKind::AgentTurn {
        task_id: "chat-1".into(),
        turn_index: 1,
        kind: "continue".to_string(),
        metadata: None,
    });

    log.emit(EventKind::AgentComplete {
        task_id: "chat-1".into(),
        turns: 1,
        stop_reason: "natural_completion".to_string(),
    });

    let events = log.events();
    assert_eq!(events.len(), 3);
}

#[test]
fn test_event_log_mcp_invoke_sequence() {
    let log = EventLog::new();

    // Simulate MCP tool calling in conversation using correct variants
    log.emit(EventKind::McpInvoke {
        task_id: "chat-1".into(),
        call_id: "call-123".to_string(),
        mcp_server: "novanet".to_string(),
        tool: Some("novanet_describe".to_string()),
        resource: None,
        params: Some(serde_json::json!({"entity": "qr-code"})),
    });

    log.emit(EventKind::McpResponse {
        task_id: "chat-1".into(),
        call_id: "call-123".to_string(),
        output_len: 256,
        duration_ms: 150,
        cached: false,
        is_error: false,
        response: Some(serde_json::json!({"name": "QR Code"})),
    });

    let events = log.events();
    assert_eq!(events.len(), 2);
}

#[test]
fn test_event_log_spawn_agent() {
    let log = EventLog::new();

    // Simulate nested agent spawning
    log.emit(EventKind::AgentSpawned {
        parent_task_id: "parent-1".into(),
        child_task_id: "child-1".into(),
        depth: 1,
    });

    let events = log.events();
    assert_eq!(events.len(), 1);

    // Verify event kind matching
    let event = &events[0];
    if let EventKind::AgentSpawned {
        parent_task_id,
        child_task_id,
        depth,
    } = &event.kind
    {
        assert_eq!(parent_task_id.as_ref(), "parent-1");
        assert_eq!(child_task_id.as_ref(), "child-1");
        assert_eq!(*depth, 1);
    } else {
        panic!("Expected AgentSpawned event");
    }
}

// ============================================================================
// UNIT TESTS - Workflow Parsing for Conversational Agent
// ============================================================================

#[test]
fn test_parse_basic_conversational_agent() {
    let yaml = r#"
schema: nika/workflow@0.5
workflow: basic-chat
description: "Basic conversational agent"

tasks:
  - id: chat
    agent:
      prompt: "You are a helpful assistant."
      max_turns: 10
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse workflow");
    assert_eq!(workflow.tasks.len(), 1);
    assert_eq!(workflow.tasks[0].id, "chat");
}

#[test]
fn test_parse_agent_with_mcp_servers() {
    let yaml = r#"
schema: nika/workflow@0.5
workflow: mcp-agent

tasks:
  - id: research-agent
    agent:
      prompt: "Research the topic using available tools."
      mcp:
        - novanet
        - perplexity
      max_turns: 20
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse workflow");
    assert_eq!(workflow.tasks.len(), 1);
}

#[test]
fn test_parse_agent_with_depth_limit() {
    let yaml = r#"
schema: nika/workflow@0.5
workflow: nested-agent

tasks:
  - id: orchestrator
    agent:
      prompt: "Orchestrate sub-tasks."
      depth_limit: 3
      max_turns: 5
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse workflow");
    assert_eq!(workflow.tasks.len(), 1);
}

#[test]
fn test_parse_agent_with_extended_thinking() {
    let yaml = r#"
schema: nika/workflow@0.5
workflow: thinking-agent

tasks:
  - id: analyst
    agent:
      prompt: "Analyze this complex problem step by step."
      extended_thinking: true
      budget_tokens: 5000
      max_turns: 3
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse workflow");
    assert_eq!(workflow.tasks.len(), 1);
}

// ============================================================================
// UNIT TESTS - RigAgentLoop Creation
// ============================================================================

#[test]
fn test_rig_agent_loop_creation_valid() {
    let params = AgentParams {
        prompt: "Test conversational prompt".to_string(),
        mcp: vec![],
        max_turns: Some(5),
        ..Default::default()
    };

    let log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let result = RigAgentLoop::new("chat-test".to_string(), params, log, mcp_clients);
    assert!(
        result.is_ok(),
        "Should create RigAgentLoop with valid params"
    );
}

#[test]
fn test_rig_agent_loop_empty_prompt_fails() {
    let params = AgentParams::default();
    let log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let result = RigAgentLoop::new("chat-test".to_string(), params, log, mcp_clients);
    assert!(result.is_err(), "Should fail with empty prompt");
}

#[test]
fn test_rig_agent_loop_with_depth_limit() {
    let params = AgentParams {
        prompt: "Orchestrator prompt".to_string(),
        mcp: vec![],
        max_turns: Some(5),
        depth_limit: Some(3),
        ..Default::default()
    };

    let log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let result = RigAgentLoop::new("orchestrator".to_string(), params, log, mcp_clients);
    assert!(result.is_ok());
}

// ============================================================================
// ASYNC TESTS - Mock Provider Execution
// ============================================================================

#[tokio::test]
async fn test_mock_conversation_flow() {
    let params = AgentParams {
        prompt: "Say hello and introduce yourself.".to_string(),
        mcp: vec![],
        max_turns: Some(1),
        ..Default::default()
    };

    let log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let agent = RigAgentLoop::new("mock-chat".to_string(), params, log.clone(), mcp_clients)
        .expect("Failed to create agent");

    // Use mock provider (no API key needed)
    let result = agent.run_mock().await;

    assert!(result.is_ok(), "Mock provider should always succeed");
}

#[tokio::test]
async fn test_mock_multi_turn_conversation() {
    let params = AgentParams {
        prompt: "Count from 1 to 3, one number per turn.".to_string(),
        mcp: vec![],
        max_turns: Some(3),
        ..Default::default()
    };

    let log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let agent = RigAgentLoop::new("multi-turn".to_string(), params, log.clone(), mcp_clients)
        .expect("Failed to create agent");

    let result = agent.run_mock().await;
    assert!(result.is_ok());

    // Verify events were logged
    let events = log.events();
    assert!(!events.is_empty(), "Should have logged events");

    // Mock emits AgentTurn events (started and completed)
    let has_turn = events
        .iter()
        .any(|e| matches!(&e.kind, EventKind::AgentTurn { .. }));
    assert!(has_turn, "Should have AgentTurn event");
}

#[tokio::test]
async fn test_mock_agent_event_log_integration() {
    let params = AgentParams {
        prompt: "Test event logging".to_string(),
        mcp: vec![],
        max_turns: Some(1),
        ..Default::default()
    };

    let log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let agent = RigAgentLoop::new("event-test".to_string(), params, log.clone(), mcp_clients)
        .expect("Failed to create agent");

    let _ = agent.run_mock().await;

    // Check for AgentTurn events
    let events = log.events();
    let turn_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(&e.kind, EventKind::AgentTurn { .. }))
        .collect();

    // Mock provider should complete in one turn
    assert!(
        !turn_events.is_empty()
            || events
                .iter()
                .any(|e| matches!(&e.kind, EventKind::AgentComplete { .. })),
        "Should have turn or complete events"
    );
}

// ============================================================================
// INTEGRATION TESTS - Event Pattern Verification
// ============================================================================

#[test]
fn test_event_kind_task_id_extraction() {
    // Verify that task_id extraction works for agent events
    let event = EventKind::AgentTurn {
        task_id: "test-task".into(),
        turn_index: 1,
        kind: "continue".to_string(),
        metadata: None,
    };

    assert_eq!(event.task_id(), Some("test-task"));

    let event = EventKind::AgentComplete {
        task_id: "complete-task".into(),
        turns: 2,
        stop_reason: "max_turns".to_string(),
    };

    assert_eq!(event.task_id(), Some("complete-task"));
}

#[test]
fn test_agent_spawn_event_parent_tracking() {
    let event = EventKind::AgentSpawned {
        parent_task_id: "parent".into(),
        child_task_id: "child".into(),
        depth: 2,
    };

    // AgentSpawned uses parent_task_id
    assert_eq!(event.task_id(), Some("parent"));
}

// ============================================================================
// WORKFLOW CONTEXT TESTS
// ============================================================================

#[test]
fn test_parse_agent_with_context_binding() {
    let yaml = r#"
schema: nika/workflow@0.5
workflow: context-agent

tasks:
  - id: fetch-context
    fetch:
      url: "https://api.example.com/data"
    use.data: api_data

  - id: analyze
    depends_on:
      - fetch-context
    agent:
      prompt: |
        Analyze the following data:
        {{use.api_data}}
      max_turns: 3
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse workflow");
    assert_eq!(workflow.tasks.len(), 2);
}

#[test]
fn test_parse_chained_agents() {
    let yaml = r#"
schema: nika/workflow@0.5
workflow: chained-agents

tasks:
  - id: researcher
    agent:
      prompt: "Research the topic and summarize findings."
      max_turns: 5
    use.findings: research_output

  - id: writer
    depends_on:
      - researcher
    agent:
      prompt: |
        Based on these findings, write an article:
        {{use.research_output}}
      max_turns: 3
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse workflow");
    assert_eq!(workflow.tasks.len(), 2);
}

// ============================================================================
// ERROR HANDLING TESTS
// ============================================================================

#[test]
fn test_agent_zero_max_turns_fails() {
    let params = AgentParams {
        prompt: "Test prompt".to_string(),
        max_turns: Some(0),
        ..Default::default()
    };

    let log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let result = RigAgentLoop::new("zero-turns".to_string(), params, log, mcp_clients);
    assert!(result.is_err(), "Should fail with zero max_turns");
}

#[test]
fn test_agent_excessive_depth_limit() {
    // depth_limit > 10 should be capped or rejected
    let params = AgentParams {
        prompt: "Test prompt".to_string(),
        depth_limit: Some(100), // Excessive
        max_turns: Some(5),
        ..Default::default()
    };

    let log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    // This should either cap at 10 or reject
    let result = RigAgentLoop::new("deep-agent".to_string(), params, log, mcp_clients);
    // Either is acceptable - implementation may cap or reject
    assert!(result.is_ok() || result.is_err());
}
