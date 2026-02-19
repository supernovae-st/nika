//! Integration tests for v0.4 Reasoning Capture feature
//!
//! Tests extended thinking capture from Claude agents.
//! Verifies that thinking content flows through:
//! 1. AgentParams (extended_thinking flag)
//! 2. RigAgentLoop (streaming extraction)
//! 3. AgentTurnMetadata (thinking field)
//! 4. EventLog (AgentTurn events)
//! 5. TUI state (display)

use rustc_hash::FxHashMap;
use std::sync::Arc;

use nika::ast::AgentParams;
use nika::event::{EventKind, EventLog};
use nika::mcp::McpClient;
use nika::runtime::{RigAgentLoop, RigAgentStatus};

// ═══════════════════════════════════════════════════════════════════════════
// AgentParams with extended_thinking Tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_agent_params_with_extended_thinking_true() {
    let params = AgentParams {
        prompt: "Analyze QR code marketing effectiveness".to_string(),
        extended_thinking: Some(true),
        max_turns: Some(3),
        ..Default::default()
    };

    assert_eq!(params.extended_thinking, Some(true));
}

#[test]
fn test_agent_params_with_extended_thinking_false() {
    let params = AgentParams {
        prompt: "Simple task".to_string(),
        extended_thinking: Some(false),
        max_turns: Some(3),
        ..Default::default()
    };

    assert_eq!(params.extended_thinking, Some(false));
}

#[test]
fn test_agent_params_extended_thinking_defaults_to_none() {
    let params = AgentParams {
        prompt: "Task without thinking config".to_string(),
        max_turns: Some(3),
        ..Default::default()
    };

    assert!(params.extended_thinking.is_none());
}

// ═══════════════════════════════════════════════════════════════════════════
// RigAgentLoop with Extended Thinking Tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_rig_agent_loop_creation_with_extended_thinking() {
    let params = AgentParams {
        prompt: "Analyze step by step".to_string(),
        extended_thinking: Some(true),
        max_turns: Some(5),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let result = RigAgentLoop::new("test_thinking".to_string(), params, event_log, mcp_clients);

    assert!(
        result.is_ok(),
        "Should create RigAgentLoop with extended_thinking"
    );
}

#[tokio::test]
async fn test_rig_agent_loop_mock_completes_with_thinking_enabled() {
    let params = AgentParams {
        prompt: "Think through this problem step by step".to_string(),
        extended_thinking: Some(true),
        max_turns: Some(3),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let agent_loop = RigAgentLoop::new(
        "test_thinking_mock".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
    )
    .unwrap();

    // Run mock - extended_thinking: true uses run_mock() which doesn't call real API
    let result = agent_loop.run_mock().await;

    assert!(result.is_ok(), "Mock agent should complete: {:?}", result);
    let result = result.unwrap();
    assert_eq!(result.status, RigAgentStatus::NaturalCompletion);
}

// ═══════════════════════════════════════════════════════════════════════════
// Event Log with Thinking Content Tests
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_agent_turn_events_emitted_with_thinking_param() {
    let params = AgentParams {
        prompt: "Reason through this carefully".to_string(),
        extended_thinking: Some(true),
        max_turns: Some(2),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let agent_loop = RigAgentLoop::new(
        "test_events_thinking".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
    )
    .unwrap();

    let _ = agent_loop.run_mock().await;

    let events = event_log.events();
    let agent_turns: Vec<_> = events
        .iter()
        .filter(|e| matches!(&e.kind, EventKind::AgentTurn { .. }))
        .collect();

    assert!(
        !agent_turns.is_empty(),
        "Should emit AgentTurn events: {:?}",
        events
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// YAML Workflow Integration Tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_workflow_yaml_parses_extended_thinking() {
    use nika::ast::{TaskAction, Workflow};

    let yaml = r#"
schema: nika/workflow@0.3
provider: claude

tasks:
  - id: analyze_with_reasoning
    agent:
      prompt: |
        Analyze why QR codes are effective for marketing.
        Think through this step by step before answering.
      extended_thinking: true
      model: claude-sonnet-4-20250514
      max_turns: 3
    output:
      format: text
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse workflow");

    assert_eq!(workflow.tasks.len(), 1);

    match &workflow.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert_eq!(
                agent.extended_thinking,
                Some(true),
                "Should parse extended_thinking: true"
            );
            assert!(agent.prompt.contains("step by step"));
        }
        _ => panic!("Expected Agent action"),
    }
}

#[test]
fn test_workflow_yaml_extended_thinking_false() {
    use nika::ast::{TaskAction, Workflow};

    let yaml = r#"
schema: nika/workflow@0.3

tasks:
  - id: quick_task
    agent:
      prompt: Generate a simple response.
      extended_thinking: false
      max_turns: 1
    output:
      format: text
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse workflow");

    match &workflow.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert_eq!(agent.extended_thinking, Some(false));
        }
        _ => panic!("Expected Agent action"),
    }
}

#[test]
fn test_workflow_yaml_without_extended_thinking() {
    use nika::ast::{TaskAction, Workflow};

    let yaml = r#"
schema: nika/workflow@0.3

tasks:
  - id: standard_task
    agent:
      prompt: A normal task without thinking config.
      max_turns: 3
    output:
      format: text
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse workflow");

    match &workflow.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert!(
                agent.extended_thinking.is_none(),
                "Should be None when not specified"
            );
        }
        _ => panic!("Expected Agent action"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TUI State Tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_tui_state_agent_turn_state_with_thinking() {
    use nika::tui::AgentTurnState;

    let turn = AgentTurnState {
        index: 1,
        status: "completed".to_string(),
        tokens: Some(150),
        tool_calls: vec![],
        thinking: Some("Let me analyze this step by step...".to_string()),
        response_text: Some("Here is my response.".to_string()),
    };

    assert!(turn.thinking.is_some());
    assert!(turn.thinking.unwrap().contains("step by step"));
}

#[test]
fn test_tui_state_agent_turn_state_without_thinking() {
    use nika::tui::AgentTurnState;

    let turn = AgentTurnState {
        index: 1,
        status: "completed".to_string(),
        tokens: Some(100),
        tool_calls: vec![],
        thinking: None,
        response_text: Some("Response without thinking.".to_string()),
    };

    assert!(turn.thinking.is_none());
}

// ═══════════════════════════════════════════════════════════════════════════
// Error Handling Tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_thinking_not_supported_error_code() {
    use nika::error::NikaError;

    let error = NikaError::ThinkingNotSupported {
        provider: "openai".to_string(),
    };

    assert!(error.to_string().contains("NIKA-117"));
    assert!(error.to_string().contains("openai"));
}

#[test]
fn test_thinking_capture_failed_error_code() {
    use nika::error::NikaError;

    let error = NikaError::ThinkingCaptureFailed {
        reason: "Stream interrupted".to_string(),
    };

    assert!(error.to_string().contains("NIKA-116"));
    assert!(error.to_string().contains("Stream interrupted"));
}

// ═══════════════════════════════════════════════════════════════════════════
// Use Case: Reasoning Capture Workflow
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_uc_reasoning_capture_workflow() {
    // Simulates the v04-reasoning-capture.yaml workflow
    let params = AgentParams {
        prompt: r#"
Analyze why QR codes are effective for marketing.
Think through this step by step before answering.
Consider:
1. Accessibility
2. Cost effectiveness
3. User engagement
4. Analytics capabilities
"#
        .to_string(),
        extended_thinking: Some(true),
        model: Some("claude-sonnet-4-20250514".to_string()),
        max_turns: Some(1),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let agent_loop = RigAgentLoop::new(
        "analyze_with_reasoning".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
    )
    .unwrap();

    let result = agent_loop.run_mock().await;

    assert!(
        result.is_ok(),
        "Reasoning capture workflow should complete"
    );

    // Verify events were captured
    let events = event_log.events();
    assert!(!events.is_empty(), "Should emit events");
}

#[tokio::test]
async fn test_reasoning_capture_with_mcp_tools() {
    let params = AgentParams {
        prompt: "Think through how to use novanet tools effectively".to_string(),
        extended_thinking: Some(true),
        mcp: vec!["novanet".to_string()],
        max_turns: Some(3),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mut mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();
    mcp_clients.insert("novanet".to_string(), Arc::new(McpClient::mock("novanet")));

    let agent_loop = RigAgentLoop::new(
        "thinking_with_tools".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
    )
    .unwrap();

    let result = agent_loop.run_mock().await;

    assert!(
        result.is_ok(),
        "Should complete with thinking + tools: {:?}",
        result
    );
}
