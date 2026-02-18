//! Agent Loop Integration Tests
//!
//! Tests the AgentLoop implementation for agentic execution.
//! - AgentLoop creation and validation
//! - Execution with mock provider and MCP clients
//! - Stop conditions and max_turns behavior

use rustc_hash::FxHashMap;
use std::sync::Arc;

use nika::ast::AgentParams;
use nika::event::EventLog;
use nika::mcp::McpClient;
use nika::provider::MockProvider;
use nika::runtime::{AgentLoop, AgentLoopResult, AgentStatus};

// ===============================================================
// AgentLoop Creation Tests
// ===============================================================

#[test]
fn test_agent_loop_creation_with_valid_params() {
    // Arrange
    let params = AgentParams {
        prompt: "Test prompt for agent".to_string(),
        mcp: vec!["novanet".to_string()],
        max_turns: Some(3),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    // Act
    let result = AgentLoop::new("test_task".to_string(), params, event_log, mcp_clients);

    // Assert
    assert!(result.is_ok(), "Should create AgentLoop with valid params");
}

#[test]
fn test_agent_loop_creation_with_empty_prompt_fails() {
    // Arrange
    let params = AgentParams::default(); // Empty prompt

    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();

    // Act
    let result = AgentLoop::new("test_task".to_string(), params, event_log, mcp_clients);

    // Assert
    assert!(result.is_err(), "Should fail with empty prompt");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("NIKA-113"),
        "Should be AgentValidationError: {err}"
    );
}

#[test]
fn test_agent_loop_creation_with_zero_max_turns_fails() {
    // Arrange
    let params = AgentParams {
        prompt: "Test prompt".to_string(),
        max_turns: Some(0),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();

    // Act
    let result = AgentLoop::new("test_task".to_string(), params, event_log, mcp_clients);

    // Assert
    assert!(result.is_err(), "Should fail with zero max_turns");
}

#[test]
fn test_agent_loop_creation_with_excessive_max_turns_fails() {
    // Arrange
    let params = AgentParams {
        prompt: "Test prompt".to_string(),
        max_turns: Some(101), // > 100 is invalid
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();

    // Act
    let result = AgentLoop::new("test_task".to_string(), params, event_log, mcp_clients);

    // Assert
    assert!(result.is_err(), "Should fail with excessive max_turns");
}

// ===============================================================
// AgentLoop Execution Tests (async)
// ===============================================================

#[tokio::test]
async fn test_agent_loop_runs_to_natural_completion() {
    // Arrange
    let params = AgentParams {
        prompt: "Simple task that should complete immediately".to_string(),
        max_turns: Some(5),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let agent_loop = AgentLoop::new(
        "test_natural".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
    )
    .unwrap();

    let provider = Arc::new(MockProvider::default());

    // Act
    let result = agent_loop.run(provider).await;

    // Assert
    assert!(result.is_ok(), "Agent loop should complete: {:?}", result);
    let result = result.unwrap();
    assert_eq!(
        result.status,
        AgentStatus::NaturalCompletion,
        "Should complete naturally (mock provider returns no tool calls)"
    );
    assert_eq!(result.turns, 1, "Should complete in one turn");
    assert!(result.total_tokens > 0, "Should track token usage");
}

#[tokio::test]
async fn test_agent_loop_respects_max_turns() {
    // This test requires a provider that always returns tool calls
    // For now, with MockProvider returning no tool calls, this will complete naturally
    // In a real scenario with tool calls, it would respect max_turns

    let params = AgentParams {
        prompt: "Task that needs multiple turns".to_string(),
        max_turns: Some(1), // Force early stop
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let agent_loop = AgentLoop::new(
        "test_max_turns".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
    )
    .unwrap();

    let provider = Arc::new(MockProvider::default());

    // Act
    let result = agent_loop.run(provider).await;

    // Assert
    assert!(result.is_ok(), "Agent loop should complete");
    let result = result.unwrap();
    // With MockProvider (no tool calls), it completes naturally in 1 turn
    assert!(result.turns <= 1, "Should not exceed max_turns");
}

#[tokio::test]
async fn test_agent_loop_detects_stop_condition() {
    // Arrange - MockProvider returns "Mock response" which we include in stop conditions
    let params = AgentParams {
        prompt: "Task with stop condition".to_string(),
        max_turns: Some(10),
        stop_conditions: vec!["Mock response".to_string()],
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let agent_loop = AgentLoop::new(
        "test_stop_condition".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
    )
    .unwrap();

    let provider = Arc::new(MockProvider::default());

    // Act
    let result = agent_loop.run(provider).await;

    // Assert
    assert!(result.is_ok(), "Agent loop should complete");
    let result = result.unwrap();
    assert_eq!(
        result.status,
        AgentStatus::StopConditionMet,
        "Should detect stop condition in mock response"
    );
}

#[tokio::test]
async fn test_agent_loop_emits_events() {
    // Arrange
    let params = AgentParams {
        prompt: "Task for event tracking".to_string(),
        max_turns: Some(3),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let agent_loop = AgentLoop::new(
        "test_events".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
    )
    .unwrap();

    let provider = Arc::new(MockProvider::default());

    // Act
    let result = agent_loop.run(provider).await;

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

// ===============================================================
// AgentLoop with MCP Tools Tests
// ===============================================================

#[tokio::test]
async fn test_agent_loop_with_mock_mcp_client() {
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

    let agent_loop = AgentLoop::new(
        "test_with_mcp".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
    )
    .unwrap();

    let provider = Arc::new(MockProvider::default());

    // Act
    let result = agent_loop.run(provider).await;

    // Assert
    assert!(result.is_ok(), "Agent loop should complete with MCP client");
}

#[tokio::test]
async fn test_agent_loop_fails_with_missing_mcp_client() {
    // Arrange - request MCP server that isn't in the clients map
    let params = AgentParams {
        prompt: "Task that uses missing MCP".to_string(),
        mcp: vec!["nonexistent".to_string()],
        max_turns: Some(5),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default(); // Empty

    let agent_loop = AgentLoop::new(
        "test_missing_mcp".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
    )
    .unwrap();

    let provider = Arc::new(MockProvider::default());

    // Act
    let result = agent_loop.run(provider).await;

    // Assert - Should fail when building tool definitions since MCP client is missing
    assert!(result.is_err(), "Should fail when MCP client is not found");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("NIKA-100"),
        "Should be McpNotConnected error: {err}"
    );
}

// ===============================================================
// AgentStatus Tests
// ===============================================================

#[test]
fn test_agent_status_debug_display() {
    // Verify AgentStatus enum has proper Debug implementation
    let status = AgentStatus::NaturalCompletion;
    let debug = format!("{:?}", status);
    assert!(debug.contains("NaturalCompletion"));

    let status = AgentStatus::MaxTurnsReached;
    let debug = format!("{:?}", status);
    assert!(debug.contains("MaxTurnsReached"));
}

#[test]
fn test_agent_status_equality() {
    assert_eq!(
        AgentStatus::NaturalCompletion,
        AgentStatus::NaturalCompletion
    );
    assert_eq!(AgentStatus::StopConditionMet, AgentStatus::StopConditionMet);
    assert_eq!(AgentStatus::MaxTurnsReached, AgentStatus::MaxTurnsReached);
    assert_eq!(AgentStatus::Failed, AgentStatus::Failed);

    assert_ne!(AgentStatus::NaturalCompletion, AgentStatus::Failed);
}

// ===============================================================
// AgentLoopResult Tests
// ===============================================================

#[test]
fn test_agent_loop_result_debug() {
    let result = AgentLoopResult {
        status: AgentStatus::NaturalCompletion,
        turns: 3,
        final_output: serde_json::json!({"result": "done"}),
        total_tokens: 150,
    };

    let debug = format!("{:?}", result);
    assert!(debug.contains("NaturalCompletion"));
    assert!(debug.contains("turns"));
}
