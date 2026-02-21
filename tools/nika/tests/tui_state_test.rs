//! TUI State Update Tests
//!
//! Verifies that events correctly update TuiState.
//! These tests ensure the data flow from events to state works correctly.
//!
//! Run: `cargo test --test tui_state_test --features tui`

#![cfg(feature = "tui")]

use std::sync::Arc;

use serde_json::json;

use nika::event::{AgentTurnMetadata, ContextSource, EventKind, ExcludedItem};
use nika::tui::{MissionPhase, TaskStatus, TuiState};

// ═══════════════════════════════════════════════════════════════════════════
// WORKFLOW EVENT TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_workflow_started_updates_state() {
    let mut state = TuiState::new("test.yaml");

    let event = EventKind::WorkflowStarted {
        task_count: 5,
        generation_id: "gen-123".to_string(),
        workflow_hash: "abc123".to_string(),
        nika_version: "0.5.0".to_string(),
    };

    state.handle_event(&event, 0);

    assert_eq!(state.workflow.task_count, 5);
    assert_eq!(state.workflow.phase, MissionPhase::Countdown);
    assert_eq!(state.workflow.generation_id, Some("gen-123".to_string()));
    assert!(state.workflow.started_at.is_some());
}

#[test]
fn test_workflow_completed_updates_state() {
    let mut state = TuiState::new("test.yaml");

    // First start the workflow
    state.handle_event(
        &EventKind::WorkflowStarted {
            task_count: 2,
            generation_id: "gen-123".to_string(),
            workflow_hash: "abc".to_string(),
            nika_version: "0.5.0".to_string(),
        },
        0,
    );

    // Complete workflow
    let output = Arc::new(json!({"result": "success"}));
    state.handle_event(
        &EventKind::WorkflowCompleted {
            final_output: output.clone(),
            total_duration_ms: 1234,
        },
        1234,
    );

    assert_eq!(state.workflow.phase, MissionPhase::MissionSuccess);
    assert!(state.workflow.final_output.is_some());
    assert_eq!(state.workflow.total_duration_ms, Some(1234));
}

#[test]
fn test_workflow_failed_updates_state() {
    let mut state = TuiState::new("test.yaml");

    state.handle_event(
        &EventKind::WorkflowStarted {
            task_count: 2,
            generation_id: "gen-123".to_string(),
            workflow_hash: "abc".to_string(),
            nika_version: "0.5.0".to_string(),
        },
        0,
    );

    state.handle_event(
        &EventKind::WorkflowFailed {
            error: "Task failed: connection timeout".to_string(),
            failed_task: Some(Arc::from("step1")),
        },
        500,
    );

    assert_eq!(state.workflow.phase, MissionPhase::Abort);
    assert_eq!(
        state.workflow.error_message,
        Some("Task failed: connection timeout".to_string())
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// TASK EVENT TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_task_scheduled_creates_task_state() {
    let mut state = TuiState::new("test.yaml");

    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("step1"),
            dependencies: vec![],
        },
        0,
    );

    assert!(state.tasks.contains_key("step1"));
    let task = state.tasks.get("step1").unwrap();
    assert_eq!(task.status, TaskStatus::Pending);
    assert!(task.dependencies.is_empty());
}

#[test]
fn test_task_scheduled_with_dependencies() {
    let mut state = TuiState::new("test.yaml");

    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("step2"),
            dependencies: vec![Arc::from("step1"), Arc::from("step0")],
        },
        0,
    );

    let task = state.tasks.get("step2").unwrap();
    assert_eq!(task.dependencies, vec!["step1", "step0"]);
}

#[test]
fn test_task_started_updates_status() {
    let mut state = TuiState::new("test.yaml");

    // Schedule first
    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("step1"),
            dependencies: vec![],
        },
        0,
    );

    // Start task
    state.handle_event(
        &EventKind::TaskStarted {
            task_id: Arc::from("step1"),
            verb: "infer".into(),
            inputs: json!({"prompt": "Hello"}),
        },
        100,
    );

    let task = state.tasks.get("step1").unwrap();
    assert_eq!(task.status, TaskStatus::Running);
    assert!(task.started_at.is_some());
    assert!(task.input.is_some());
    assert_eq!(state.current_task, Some("step1".to_string()));
}

#[test]
fn test_task_completed_updates_state() {
    let mut state = TuiState::new("test.yaml");

    // Setup: workflow started + task scheduled + task started
    state.handle_event(
        &EventKind::WorkflowStarted {
            task_count: 1,
            generation_id: "gen-1".to_string(),
            workflow_hash: "abc".to_string(),
            nika_version: "0.5.0".to_string(),
        },
        0,
    );
    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("step1"),
            dependencies: vec![],
        },
        0,
    );
    state.handle_event(
        &EventKind::TaskStarted {
            task_id: Arc::from("step1"),
            verb: "infer".into(),
            inputs: json!({}),
        },
        100,
    );

    // Complete task
    let output = Arc::new(json!({"result": "done"}));
    state.handle_event(
        &EventKind::TaskCompleted {
            task_id: Arc::from("step1"),
            output: output.clone(),
            duration_ms: 500,
        },
        600,
    );

    let task = state.tasks.get("step1").unwrap();
    assert_eq!(task.status, TaskStatus::Success);
    assert_eq!(task.duration_ms, Some(500));
    assert!(task.output.is_some());
    assert_eq!(state.workflow.tasks_completed, 1);
}

#[test]
fn test_task_failed_updates_state() {
    let mut state = TuiState::new("test.yaml");

    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("step1"),
            dependencies: vec![],
        },
        0,
    );
    state.handle_event(
        &EventKind::TaskStarted {
            task_id: Arc::from("step1"),
            verb: "infer".into(),
            inputs: json!({}),
        },
        100,
    );

    state.handle_event(
        &EventKind::TaskFailed {
            task_id: Arc::from("step1"),
            error: "Connection refused".to_string(),
            duration_ms: 200,
        },
        300,
    );

    let task = state.tasks.get("step1").unwrap();
    assert_eq!(task.status, TaskStatus::Failed);
    assert_eq!(task.error, Some("Connection refused".to_string()));
}

// ═══════════════════════════════════════════════════════════════════════════
// PROGRESS CALCULATION TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_progress_calculation_empty() {
    let state = TuiState::new("test.yaml");
    assert_eq!(state.workflow.progress_pct(), 0.0);
}

#[test]
fn test_progress_calculation_partial() {
    let mut state = TuiState::new("test.yaml");

    state.handle_event(
        &EventKind::WorkflowStarted {
            task_count: 4,
            generation_id: "gen-1".to_string(),
            workflow_hash: "abc".to_string(),
            nika_version: "0.5.0".to_string(),
        },
        0,
    );

    // Complete 2 of 4 tasks
    state.workflow.tasks_completed = 2;

    assert_eq!(state.workflow.progress_pct(), 50.0);
}

#[test]
fn test_progress_calculation_complete() {
    let mut state = TuiState::new("test.yaml");

    state.handle_event(
        &EventKind::WorkflowStarted {
            task_count: 3,
            generation_id: "gen-1".to_string(),
            workflow_hash: "abc".to_string(),
            nika_version: "0.5.0".to_string(),
        },
        0,
    );

    state.workflow.tasks_completed = 3;

    assert_eq!(state.workflow.progress_pct(), 100.0);
}

// ═══════════════════════════════════════════════════════════════════════════
// MCP CALL TRACKING TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_mcp_invoke_creates_call_record() {
    let mut state = TuiState::new("test.yaml");

    state.handle_event(
        &EventKind::McpInvoke {
            task_id: Arc::from("step1"),
            call_id: "call-001".to_string(),
            mcp_server: "novanet".to_string(),
            tool: Some("novanet_describe".to_string()),
            resource: None,
            params: Some(json!({"entity": "qr-code"})),
        },
        100,
    );

    assert_eq!(state.mcp_calls.len(), 1);
    let call = &state.mcp_calls[0];
    assert_eq!(call.call_id, "call-001");
    assert_eq!(call.server, "novanet");
    assert_eq!(call.tool, Some("novanet_describe".to_string()));
    assert_eq!(call.task_id, "step1");
    assert!(!call.completed);
    assert!(call.params.is_some());
}

#[test]
fn test_mcp_response_updates_call_record() {
    let mut state = TuiState::new("test.yaml");

    // First create invoke
    state.handle_event(
        &EventKind::McpInvoke {
            task_id: Arc::from("step1"),
            call_id: "call-001".to_string(),
            mcp_server: "novanet".to_string(),
            tool: Some("novanet_describe".to_string()),
            resource: None,
            params: None,
        },
        100,
    );

    // Then response
    state.handle_event(
        &EventKind::McpResponse {
            task_id: Arc::from("step1"),
            call_id: "call-001".to_string(),
            output_len: 1024,
            duration_ms: 250,
            cached: false,
            is_error: false,
            response: Some(json!({"key": "qr-code", "display_name": "QR Code"})),
        },
        350,
    );

    let call = &state.mcp_calls[0];
    assert!(call.completed);
    assert_eq!(call.output_len, Some(1024));
    assert_eq!(call.duration_ms, Some(250));
    assert!(!call.is_error);
    assert!(call.response.is_some());
}

#[test]
fn test_mcp_error_response() {
    let mut state = TuiState::new("test.yaml");

    state.handle_event(
        &EventKind::McpInvoke {
            task_id: Arc::from("step1"),
            call_id: "call-002".to_string(),
            mcp_server: "novanet".to_string(),
            tool: Some("novanet_traverse".to_string()),
            resource: None,
            params: None,
        },
        100,
    );

    state.handle_event(
        &EventKind::McpResponse {
            task_id: Arc::from("step1"),
            call_id: "call-002".to_string(),
            output_len: 50,
            duration_ms: 100,
            cached: false,
            is_error: true,
            response: Some(json!({"error": "Entity not found"})),
        },
        200,
    );

    let call = &state.mcp_calls[0];
    assert!(call.is_error);
}

#[test]
fn test_multiple_mcp_calls_sequenced() {
    let mut state = TuiState::new("test.yaml");

    // 3 MCP calls
    for i in 0..3 {
        state.handle_event(
            &EventKind::McpInvoke {
                task_id: Arc::from("step1"),
                call_id: format!("call-{:03}", i),
                mcp_server: "novanet".to_string(),
                tool: Some(format!("tool_{}", i)),
                resource: None,
                params: None,
            },
            i as u64 * 100,
        );
    }

    assert_eq!(state.mcp_calls.len(), 3);
    assert_eq!(state.mcp_calls[0].seq, 0);
    assert_eq!(state.mcp_calls[1].seq, 1);
    assert_eq!(state.mcp_calls[2].seq, 2);
}

// ═══════════════════════════════════════════════════════════════════════════
// AGENT TURN TRACKING TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_agent_start_initializes_state() {
    let mut state = TuiState::new("test.yaml");

    state.handle_event(
        &EventKind::AgentStart {
            task_id: Arc::from("agent_task"),
            max_turns: 10,
            mcp_servers: vec!["novanet".to_string(), "perplexity".to_string()],
        },
        100,
    );

    assert_eq!(state.agent_max_turns, Some(10));
    assert!(state.agent_turns.is_empty()); // Turns start on AgentTurn events
}

#[test]
fn test_agent_turn_creates_turn_record() {
    let mut state = TuiState::new("test.yaml");

    state.handle_event(
        &EventKind::AgentStart {
            task_id: Arc::from("agent_task"),
            max_turns: 5,
            mcp_servers: vec![],
        },
        100,
    );

    state.handle_event(
        &EventKind::AgentTurn {
            task_id: Arc::from("agent_task"),
            turn_index: 0,
            kind: "started".to_string(),
            metadata: None,
        },
        200,
    );

    assert_eq!(state.agent_turns.len(), 1);
    assert_eq!(state.agent_turns[0].index, 0);
}

#[test]
fn test_agent_turn_with_metadata() {
    let mut state = TuiState::new("test.yaml");

    state.handle_event(
        &EventKind::AgentStart {
            task_id: Arc::from("agent_task"),
            max_turns: 5,
            mcp_servers: vec![],
        },
        100,
    );

    let metadata = AgentTurnMetadata {
        thinking: Some("Let me analyze this...".to_string()),
        response_text: "I'll help you with that.".to_string(),
        input_tokens: 500,
        output_tokens: 100,
        cache_read_tokens: 0,
        stop_reason: "end_turn".to_string(),
    };

    state.handle_event(
        &EventKind::AgentTurn {
            task_id: Arc::from("agent_task"),
            turn_index: 0,
            kind: "natural_completion".to_string(),
            metadata: Some(metadata),
        },
        500,
    );

    let turn = &state.agent_turns[0];
    assert!(turn.thinking.is_some());
    assert_eq!(turn.thinking.as_ref().unwrap(), "Let me analyze this...");
    assert_eq!(turn.tokens, Some(600)); // input + output
}

#[test]
fn test_agent_complete_clears_turns() {
    let mut state = TuiState::new("test.yaml");

    // Start agent
    state.handle_event(
        &EventKind::AgentStart {
            task_id: Arc::from("agent_task"),
            max_turns: 5,
            mcp_servers: vec![],
        },
        100,
    );

    // Add a turn
    state.handle_event(
        &EventKind::AgentTurn {
            task_id: Arc::from("agent_task"),
            turn_index: 0,
            kind: "started".to_string(),
            metadata: None,
        },
        200,
    );

    // Schedule next task (simulates moving to next task)
    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("next_task"),
            dependencies: vec![Arc::from("agent_task")],
        },
        1000,
    );

    // Agent turns should still exist until task completes
    assert!(!state.agent_turns.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// CONTEXT ASSEMBLY TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_context_assembled_updates_state() {
    let mut state = TuiState::new("test.yaml");

    state.handle_event(
        &EventKind::ContextAssembled {
            task_id: Arc::from("step1"),
            sources: vec![
                ContextSource {
                    node: "entity:qr-code".to_string(),
                    tokens: 500,
                },
                ContextSource {
                    node: "locale:fr-FR".to_string(),
                    tokens: 200,
                },
            ],
            excluded: vec![ExcludedItem {
                node: "entity:large-doc".to_string(),
                reason: "Exceeded token budget".to_string(),
            }],
            total_tokens: 700,
            budget_used_pct: 70.0,
            truncated: false,
        },
        100,
    );

    assert_eq!(state.context_assembly.sources.len(), 2);
    assert_eq!(state.context_assembly.excluded.len(), 1);
    assert_eq!(state.context_assembly.total_tokens, 700);
    assert_eq!(state.context_assembly.budget_used_pct, 70.0);
    assert!(!state.context_assembly.truncated);
}

// ═══════════════════════════════════════════════════════════════════════════
// PROVIDER EVENTS TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_provider_responded_updates_metrics() {
    let mut state = TuiState::new("test.yaml");

    state.handle_event(
        &EventKind::ProviderResponded {
            task_id: Arc::from("step1"),
            request_id: Some("req-123".to_string()),
            input_tokens: 1000,
            output_tokens: 500,
            cache_read_tokens: 200,
            ttft_ms: Some(150),
            finish_reason: "end_turn".to_string(),
            cost_usd: 0.015,
        },
        500,
    );

    assert_eq!(state.metrics.input_tokens, 1000);
    assert_eq!(state.metrics.output_tokens, 500);
    assert_eq!(state.metrics.total_tokens, 1500);
    assert!((state.metrics.cost_usd - 0.015).abs() < 0.0001);
}

// ═══════════════════════════════════════════════════════════════════════════
// PHASE TRANSITION TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_mission_phase_transitions() {
    let mut state = TuiState::new("test.yaml");

    // Initial: Preflight
    assert_eq!(state.workflow.phase, MissionPhase::Preflight);

    // WorkflowStarted -> Countdown
    state.handle_event(
        &EventKind::WorkflowStarted {
            task_count: 3,
            generation_id: "gen-1".to_string(),
            workflow_hash: "abc".to_string(),
            nika_version: "0.5.0".to_string(),
        },
        0,
    );
    assert_eq!(state.workflow.phase, MissionPhase::Countdown);

    // First TaskStarted -> Launch
    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("t1"),
            dependencies: vec![],
        },
        0,
    );
    state.handle_event(
        &EventKind::TaskStarted {
            task_id: Arc::from("t1"),
            verb: "infer".into(),
            inputs: json!({}),
        },
        100,
    );
    assert_eq!(state.workflow.phase, MissionPhase::Launch);

    // Subsequent TaskStarted -> Orbital
    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("t2"),
            dependencies: vec![Arc::from("t1")],
        },
        0,
    );
    state.handle_event(
        &EventKind::TaskStarted {
            task_id: Arc::from("t2"),
            verb: "infer".into(),
            inputs: json!({}),
        },
        500,
    );
    assert_eq!(state.workflow.phase, MissionPhase::Orbital);

    // MCP call -> Rendezvous
    state.handle_event(
        &EventKind::McpInvoke {
            task_id: Arc::from("t2"),
            call_id: "c1".to_string(),
            mcp_server: "novanet".to_string(),
            tool: Some("tool".to_string()),
            resource: None,
            params: None,
        },
        600,
    );
    assert_eq!(state.workflow.phase, MissionPhase::Rendezvous);
}

// ═══════════════════════════════════════════════════════════════════════════
// NOTIFICATION TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_slow_task_triggers_warning() {
    let mut state = TuiState::new("test.yaml");

    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("slow"),
            dependencies: vec![],
        },
        0,
    );
    state.handle_event(
        &EventKind::TaskStarted {
            task_id: Arc::from("slow"),
            verb: "infer".into(),
            inputs: json!({}),
        },
        0,
    );

    // Task takes 15 seconds (>10s warning threshold)
    state.handle_event(
        &EventKind::TaskCompleted {
            task_id: Arc::from("slow"),
            output: Arc::new(json!({})),
            duration_ms: 15000,
        },
        15000,
    );

    // Should have a warning notification
    assert!(!state.notifications.is_empty());
    let notification = &state.notifications[0];
    assert!(notification.message.contains("15.0s"));
}

#[test]
fn test_very_slow_task_triggers_alert() {
    let mut state = TuiState::new("test.yaml");

    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("very_slow"),
            dependencies: vec![],
        },
        0,
    );
    state.handle_event(
        &EventKind::TaskStarted {
            task_id: Arc::from("very_slow"),
            verb: "infer".into(),
            inputs: json!({}),
        },
        0,
    );

    // Task takes 35 seconds (>30s alert threshold)
    state.handle_event(
        &EventKind::TaskCompleted {
            task_id: Arc::from("very_slow"),
            output: Arc::new(json!({})),
            duration_ms: 35000,
        },
        35000,
    );

    assert!(!state.notifications.is_empty());
    let notification = &state.notifications[0];
    assert!(notification.message.contains("35.0s"));
}
