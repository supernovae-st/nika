// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tests for TUI state module
//!
//! Extracted from state.rs to reduce file size.

use std::sync::Arc;

use super::*;
use crate::theme::{MissionPhase, TaskStatus};
use nika_engine::event::{AgentStopReason, AgentTurnKind, EventKind, FinishReason};

/// Use actual package version in tests to avoid version drift
const TEST_VERSION: &str = env!("CARGO_PKG_VERSION");

#[test]
fn test_panel_id_next_cycles() {
    assert_eq!(MonitorPanel::Progress.next(), MonitorPanel::Dag);
    assert_eq!(MonitorPanel::Agent.next(), MonitorPanel::Progress);
}

#[test]
fn test_panel_id_prev_cycles() {
    assert_eq!(MonitorPanel::Progress.prev(), MonitorPanel::Agent);
    assert_eq!(MonitorPanel::Dag.prev(), MonitorPanel::Progress);
}

#[test]
fn test_panel_id_all_returns_all_panels() {
    let all = MonitorPanel::all();
    assert_eq!(all.len(), 4);
    assert_eq!(all[0], MonitorPanel::Progress);
    assert_eq!(all[1], MonitorPanel::Dag);
    assert_eq!(all[2], MonitorPanel::NovaNet);
    assert_eq!(all[3], MonitorPanel::Agent);
}

#[test]
fn test_panel_id_number() {
    assert_eq!(MonitorPanel::Progress.number(), 1);
    assert_eq!(MonitorPanel::Dag.number(), 2);
    assert_eq!(MonitorPanel::NovaNet.number(), 3);
    assert_eq!(MonitorPanel::Agent.number(), 4);
}

#[test]
fn test_panel_id_title() {
    assert_eq!(MonitorPanel::Progress.title(), "MISSION CONTROL");
    assert_eq!(MonitorPanel::Dag.title(), "DAG EXECUTION");
    assert_eq!(MonitorPanel::NovaNet.title(), "NOVANET STATION");
    assert_eq!(MonitorPanel::Agent.title(), "AGENT REASONING");
}

#[test]
fn test_panel_id_icon() {
    assert_eq!(MonitorPanel::Progress.icon(), "◉");
    assert_eq!(MonitorPanel::Dag.icon(), "⎔");
    assert_eq!(MonitorPanel::NovaNet.icon(), "⊛");
    assert_eq!(MonitorPanel::Agent.icon(), "⊕");
}

#[test]
fn test_panel_id_complete_cycle() {
    let mut current = MonitorPanel::Progress;
    let mut count = 0;

    // Cycle through all panels
    for _ in 0..4 {
        current = current.next();
        count += 1;
    }

    // Should be back to Progress after 4 cycles
    assert_eq!(current, MonitorPanel::Progress);
    assert_eq!(count, 4);
}

#[test]
fn test_panel_id_reverse_cycle() {
    let mut current = MonitorPanel::Progress;
    let mut count = 0;

    // Reverse cycle through all panels
    for _ in 0..4 {
        current = current.prev();
        count += 1;
    }

    // Should be back to Progress after 4 reverse cycles
    assert_eq!(current, MonitorPanel::Progress);
    assert_eq!(count, 4);
}

#[test]
fn test_workflow_state_progress() {
    let mut ws = WorkflowState::new("test.yaml".to_string());
    ws.task_count = 10;
    ws.tasks_completed = 5;
    assert!((ws.progress_pct() - 50.0).abs() < f32::EPSILON);
}

#[test]
fn test_tui_state_focus_navigation() {
    let mut state = TuiState::new("test.yaml");
    assert_eq!(state.ui.focus, MonitorPanel::Progress);

    state.focus_next();
    assert_eq!(state.ui.focus, MonitorPanel::Dag);

    state.focus_panel(4);
    assert_eq!(state.ui.focus, MonitorPanel::Agent);

    state.focus_prev();
    assert_eq!(state.ui.focus, MonitorPanel::NovaNet);
}

#[test]
fn test_tui_state_cycle_tab() {
    use crate::views::{DagTab, MissionTab, NovanetTab, ReasoningTab};

    let mut state = TuiState::new("test.yaml");

    // Test Mission tab cycling (Progress → TaskIO → Output → Progress)
    state.ui.focus = MonitorPanel::Progress;
    assert_eq!(state.ui.mission_tab, MissionTab::Progress);
    state.cycle_tab();
    assert_eq!(state.ui.mission_tab, MissionTab::TaskIO);
    state.cycle_tab();
    assert_eq!(state.ui.mission_tab, MissionTab::Output);
    state.cycle_tab();
    assert_eq!(state.ui.mission_tab, MissionTab::Progress);

    // Test Dag tab cycling (Graph ↔ Yaml)
    state.ui.focus = MonitorPanel::Dag;
    assert_eq!(state.ui.dag_tab, DagTab::Graph);
    state.cycle_tab();
    assert_eq!(state.ui.dag_tab, DagTab::Yaml);
    state.cycle_tab();
    assert_eq!(state.ui.dag_tab, DagTab::Graph);

    // Test NovaNet tab cycling (Summary ↔ FullJson)
    state.ui.focus = MonitorPanel::NovaNet;
    assert_eq!(state.ui.novanet_tab, NovanetTab::Summary);
    state.cycle_tab();
    assert_eq!(state.ui.novanet_tab, NovanetTab::FullJson);
    state.cycle_tab();
    assert_eq!(state.ui.novanet_tab, NovanetTab::Summary);

    // Test Reasoning tab cycling (Turns → Thinking → Steps → Turns)
    state.ui.focus = MonitorPanel::Agent;
    assert_eq!(state.ui.reasoning_tab, ReasoningTab::Turns);
    state.cycle_tab();
    assert_eq!(state.ui.reasoning_tab, ReasoningTab::Thinking);
    state.cycle_tab();
    assert_eq!(state.ui.reasoning_tab, ReasoningTab::Steps);
    state.cycle_tab();
    assert_eq!(state.ui.reasoning_tab, ReasoningTab::Turns);
}

#[test]
fn test_tui_state_handle_workflow_started() {
    let mut state = TuiState::new("test.yaml");

    state.handle_event(
        &EventKind::WorkflowStarted {
            task_count: 5,
            generation_id: "gen-123".to_string(),
            workflow_hash: "abc".to_string(),
            nika_version: TEST_VERSION.to_string(),
        },
        0,
    );

    assert_eq!(state.workflow.task_count, 5);
    assert_eq!(state.workflow.phase, MissionPhase::Countdown);
    assert!(state.workflow.started_at.is_some());
}

#[test]
fn test_tui_state_handle_task_lifecycle() {
    let mut state = TuiState::new("test.yaml");

    // Schedule task
    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("task1"),
            dependencies: vec![],
        },
        0,
    );
    assert!(state.tasks.contains_key("task1"));
    assert_eq!(state.tasks["task1"].status, TaskStatus::Pending);

    // Start task
    state.handle_event(
        &EventKind::TaskStarted {
            verb: "infer".into(),
            task_id: Arc::from("task1"),
            inputs: Arc::new(serde_json::json!({})),
        },
        100,
    );
    assert_eq!(state.tasks["task1"].status, TaskStatus::Running);
    assert_eq!(state.current_task, Some("task1".to_string()));

    // Complete task
    state.handle_event(
        &EventKind::TaskCompleted {
            task_id: Arc::from("task1"),
            output: Arc::new(serde_json::json!({"result": "ok"})),
            duration_ms: 500,
        },
        600,
    );
    assert_eq!(state.tasks["task1"].status, TaskStatus::Success);
    assert_eq!(state.workflow.tasks_completed, 1);
}

#[test]
fn test_tui_state_handle_mcp_events() {
    let mut state = TuiState::new("test.yaml");

    let test_params = serde_json::json!({"entity": "qr-code"});
    state.handle_event(
        &EventKind::McpInvoke {
            task_id: Arc::from("task1"),
            call_id: "test-call-1".to_string(),
            mcp_server: "novanet".to_string(),
            tool: Some("novanet_describe".to_string()),
            resource: None,
            params: Some(Arc::new(test_params.clone())),
        },
        100,
    );

    assert_eq!(state.mcp.calls.len(), 1);
    assert_eq!(state.mcp.calls[0].call_id, "test-call-1");
    assert_eq!(
        state.mcp.calls[0].tool,
        Some("novanet_describe".to_string())
    );
    assert!(!state.mcp.calls[0].completed);
    assert_eq!(state.mcp.calls[0].params, Some(test_params));

    let test_response = serde_json::json!({"name": "QR Code", "locale": "en-US"});
    state.handle_event(
        &EventKind::McpResponse {
            task_id: Arc::from("task1"),
            call_id: "test-call-1".to_string(),
            output_len: 1024,
            duration_ms: 100,
            cached: false,
            is_error: false,
            response: Some(Arc::new(test_response.clone())),
        },
        200,
    );

    assert!(state.mcp.calls[0].completed);
    assert_eq!(state.mcp.calls[0].output_len, Some(1024));
    assert_eq!(state.mcp.calls[0].response, Some(test_response));
    assert_eq!(state.mcp.calls[0].duration_ms, Some(100));
    assert!(!state.mcp.calls[0].is_error);
}

#[test]
fn test_tui_state_handle_mcp_error_response() {
    let mut state = TuiState::new("test.yaml");

    state.handle_event(
        &EventKind::McpInvoke {
            task_id: Arc::from("task1"),
            call_id: "error-call-1".to_string(),
            mcp_server: "novanet".to_string(),
            tool: Some("novanet_search".to_string()),
            resource: None,
            params: Some(Arc::new(serde_json::json!({"invalid": "params"}))),
        },
        100,
    );

    state.handle_event(
        &EventKind::McpResponse {
            task_id: Arc::from("task1"),
            call_id: "error-call-1".to_string(),
            output_len: 50,
            duration_ms: 25,
            cached: false,
            is_error: true,
            response: Some(Arc::new(serde_json::json!({"error": "Invalid params"}))),
        },
        125,
    );

    assert!(state.mcp.calls[0].is_error);
    assert_eq!(state.mcp.calls[0].duration_ms, Some(25));
    assert_eq!(
        state.mcp.calls[0].response,
        Some(serde_json::json!({"error": "Invalid params"}))
    );
}

#[test]
fn test_tui_state_handle_mcp_parallel_calls() {
    let mut state = TuiState::new("test.yaml");

    // Simulate parallel MCP calls (for_each scenario)
    state.handle_event(
        &EventKind::McpInvoke {
            task_id: Arc::from("task1"),
            call_id: "call-fr".to_string(),
            mcp_server: "novanet".to_string(),
            tool: Some("novanet_context".to_string()),
            resource: None,
            params: Some(Arc::new(serde_json::json!({"locale": "fr-FR"}))),
        },
        100,
    );
    state.handle_event(
        &EventKind::McpInvoke {
            task_id: Arc::from("task1"),
            call_id: "call-en".to_string(),
            mcp_server: "novanet".to_string(),
            tool: Some("novanet_context".to_string()),
            resource: None,
            params: Some(Arc::new(serde_json::json!({"locale": "en-US"}))),
        },
        110,
    );

    assert_eq!(state.mcp.calls.len(), 2);
    assert!(!state.mcp.calls[0].completed);
    assert!(!state.mcp.calls[1].completed);

    // Response for second call arrives first
    state.handle_event(
        &EventKind::McpResponse {
            task_id: Arc::from("task1"),
            call_id: "call-en".to_string(),
            output_len: 500,
            duration_ms: 50,
            cached: false,
            is_error: false,
            response: Some(Arc::new(serde_json::json!({"content": "English content"}))),
        },
        160,
    );

    // First call still pending, second completed
    assert!(!state.mcp.calls[0].completed);
    assert!(state.mcp.calls[1].completed);
    assert_eq!(state.mcp.calls[1].call_id, "call-en");

    // Response for first call arrives
    state.handle_event(
        &EventKind::McpResponse {
            task_id: Arc::from("task1"),
            call_id: "call-fr".to_string(),
            output_len: 600,
            duration_ms: 120,
            cached: false,
            is_error: false,
            response: Some(Arc::new(serde_json::json!({"content": "French content"}))),
        },
        220,
    );

    // Both completed, correct correlation
    assert!(state.mcp.calls[0].completed);
    assert_eq!(state.mcp.calls[0].call_id, "call-fr");
    assert_eq!(state.mcp.calls[0].duration_ms, Some(120));
    assert!(state.mcp.calls[1].completed);
    assert_eq!(state.mcp.calls[1].call_id, "call-en");
    assert_eq!(state.mcp.calls[1].duration_ms, Some(50));
}

#[test]
fn test_breakpoint_detection() {
    let mut state = TuiState::new("test.yaml");
    state
        .breakpoints
        .insert(Breakpoint::BeforeTask("task1".to_string()));

    let event = EventKind::TaskStarted {
        verb: "infer".into(),
        task_id: Arc::from("task1"),
        inputs: Arc::new(serde_json::json!({})),
    };
    assert!(state.should_break(&event));

    let event2 = EventKind::TaskStarted {
        verb: "infer".into(),
        task_id: Arc::from("task2"),
        inputs: Arc::new(serde_json::json!({})),
    };
    assert!(!state.should_break(&event2));
}

// ═══════════════════════════════════════════
// TIMELINE CACHE TESTS
// ═══════════════════════════════════════════

#[test]
fn test_timeline_cache_initialization() {
    let state = TuiState::new("test.yaml");
    assert!(state.cached_timeline_entries.is_empty());
    assert_eq!(state.timeline_version, 0);
    assert_eq!(state.timeline_cache_version, 0);
}

#[test]
fn test_timeline_cache_invalidation_on_task_scheduled() {
    let mut state = TuiState::new("test.yaml");
    let v1 = state.timeline_version;

    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("task1"),
            dependencies: vec![],
        },
        0,
    );

    assert_ne!(
        state.timeline_version, v1,
        "Version should change after TaskScheduled"
    );
}

#[test]
fn test_timeline_cache_invalidation_on_task_started() {
    let mut state = TuiState::new("test.yaml");
    // First schedule the task
    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("task1"),
            dependencies: vec![],
        },
        0,
    );
    let v1 = state.timeline_version;

    state.handle_event(
        &EventKind::TaskStarted {
            verb: "infer".into(),
            task_id: Arc::from("task1"),
            inputs: Arc::new(serde_json::json!({})),
        },
        10,
    );

    assert_ne!(
        state.timeline_version, v1,
        "Version should change after TaskStarted"
    );
}

#[test]
fn test_timeline_cache_invalidation_on_task_completed() {
    let mut state = TuiState::new("test.yaml");
    // First schedule and start the task
    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("task1"),
            dependencies: vec![],
        },
        0,
    );
    state.handle_event(
        &EventKind::TaskStarted {
            verb: "infer".into(),
            task_id: Arc::from("task1"),
            inputs: Arc::new(serde_json::json!({})),
        },
        10,
    );
    let v1 = state.timeline_version;

    state.handle_event(
        &EventKind::TaskCompleted {
            task_id: Arc::from("task1"),
            output: serde_json::json!({"result": "done"}).into(),
            duration_ms: 100,
        },
        110,
    );

    assert_ne!(
        state.timeline_version, v1,
        "Version should change after TaskCompleted"
    );
}

#[test]
fn test_timeline_cache_ensure_builds_entries() {
    let mut state = TuiState::new("test.yaml");
    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("task1"),
            dependencies: vec![],
        },
        0,
    );
    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("task2"),
            dependencies: vec![Arc::from("task1")],
        },
        0,
    );

    // Before ensure, cache should be stale
    assert!(state.cached_timeline_entries.is_empty());

    state.ensure_timeline_cache();

    // After ensure, cache should have 2 entries
    assert_eq!(state.cached_timeline_entries.len(), 2);
    assert_eq!(state.timeline_cache_version, state.timeline_version);
}

#[test]
fn test_timeline_cache_reuse_when_not_stale() {
    let mut state = TuiState::new("test.yaml");
    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("task1"),
            dependencies: vec![],
        },
        0,
    );

    // Build cache
    state.ensure_timeline_cache();
    let v1 = state.timeline_cache_version;
    let entries_ptr = state.cached_timeline_entries.as_ptr();

    // Call ensure again - should not rebuild
    state.ensure_timeline_cache();
    let v2 = state.timeline_cache_version;
    let entries_ptr2 = state.cached_timeline_entries.as_ptr();

    // Same version, same pointer (no rebuild)
    assert_eq!(v1, v2);
    assert_eq!(entries_ptr, entries_ptr2);
}

// ═══════════════════════════════════════════
// SETTINGS STATE TESTS
// ═══════════════════════════════════════════

#[test]
fn test_settings_field_next_cycles() {
    assert_eq!(SettingsField::AnthropicKey.next(), SettingsField::OpenAiKey);
    assert_eq!(SettingsField::OpenAiKey.next(), SettingsField::Provider);
    assert_eq!(SettingsField::Provider.next(), SettingsField::Model);
    assert_eq!(SettingsField::Model.next(), SettingsField::AnthropicKey);
}

#[test]
fn test_settings_field_prev_cycles() {
    assert_eq!(SettingsField::AnthropicKey.prev(), SettingsField::Model);
    assert_eq!(SettingsField::OpenAiKey.prev(), SettingsField::AnthropicKey);
    assert_eq!(SettingsField::Provider.prev(), SettingsField::OpenAiKey);
    assert_eq!(SettingsField::Model.prev(), SettingsField::Provider);
}

#[test]
fn test_settings_field_all() {
    let all = SettingsField::all();
    assert_eq!(all.len(), 4);
    assert_eq!(all[0], SettingsField::AnthropicKey);
    assert_eq!(all[3], SettingsField::Model);
}

#[test]
fn test_settings_field_labels() {
    assert_eq!(SettingsField::AnthropicKey.label(), "Anthropic API Key");
    assert_eq!(SettingsField::OpenAiKey.label(), "OpenAI API Key");
    assert_eq!(SettingsField::Provider.label(), "Default Provider");
    assert_eq!(SettingsField::Model.label(), "Default Model");
}

#[test]
fn test_settings_state_default() {
    let state = SettingsState::default();
    assert_eq!(state.focus, SettingsField::AnthropicKey);
    assert!(!state.editing);
    assert!(state.input_buffer.is_empty());
    assert_eq!(state.cursor, 0);
    assert!(!state.dirty);
}

#[test]
fn test_settings_state_focus_navigation() {
    let mut state = SettingsState::default();
    assert_eq!(state.focus, SettingsField::AnthropicKey);

    state.focus_next();
    assert_eq!(state.focus, SettingsField::OpenAiKey);

    state.focus_next();
    assert_eq!(state.focus, SettingsField::Provider);

    state.focus_prev();
    assert_eq!(state.focus, SettingsField::OpenAiKey);
}

#[test]
fn test_settings_state_edit_lifecycle() {
    use nika_engine::config::ApiKeys;

    let config = NikaConfig {
        api_keys: ApiKeys {
            anthropic: Some("sk-ant-test".to_string()),
            openai: None,
        },
        ..Default::default()
    };
    let mut state = SettingsState::new(config);

    // Start editing
    state.start_edit();
    assert!(state.editing);
    assert_eq!(state.input_buffer, "sk-ant-test");
    assert_eq!(state.cursor, 11); // Length of "sk-ant-test"

    // Modify buffer
    state.backspace();
    assert_eq!(state.input_buffer, "sk-ant-tes");

    state.insert_char('X');
    assert_eq!(state.input_buffer, "sk-ant-tesX");

    // Cancel edit - should restore
    state.cancel_edit();
    assert!(!state.editing);
    assert!(state.input_buffer.is_empty());
    assert!(!state.dirty);
}

#[test]
fn test_settings_state_confirm_edit() {
    let mut state = SettingsState {
        focus: SettingsField::OpenAiKey,
        ..Default::default()
    };

    state.start_edit();
    state.input_buffer = "sk-new-key".to_string();
    state.confirm_edit();

    assert!(!state.editing);
    assert!(state.dirty);
    assert_eq!(state.config.api_keys.openai, Some("sk-new-key".to_string()));
}

#[test]
fn test_settings_state_confirm_edit_empty_clears_value() {
    use nika_engine::config::ApiKeys;

    let config = NikaConfig {
        api_keys: ApiKeys {
            anthropic: Some("sk-ant-test".to_string()),
            openai: None,
        },
        ..Default::default()
    };
    let mut state = SettingsState::new(config);

    state.start_edit();
    state.input_buffer.clear(); // Clear to empty
    state.confirm_edit();

    assert!(state.config.api_keys.anthropic.is_none());
    assert!(state.dirty);
}

#[test]
fn test_settings_state_cursor_movement() {
    let mut state = SettingsState {
        editing: true,
        input_buffer: "hello".to_string(),
        cursor: 3, // At 'l'
        ..Default::default()
    };

    state.cursor_left();
    assert_eq!(state.cursor, 2);

    state.cursor_right();
    assert_eq!(state.cursor, 3);

    state.cursor_home();
    assert_eq!(state.cursor, 0);

    state.cursor_end();
    assert_eq!(state.cursor, 5);

    // Boundary checks
    state.cursor_home();
    state.cursor_left(); // Should stay at 0
    assert_eq!(state.cursor, 0);

    state.cursor_end();
    state.cursor_right(); // Should stay at end
    assert_eq!(state.cursor, 5);
}

#[test]
fn test_settings_state_key_status_displays_masked() {
    use nika_engine::config::ApiKeys;

    let config = NikaConfig {
        api_keys: ApiKeys {
            anthropic: Some("sk-ant-api03-xyz123abc456".to_string()),
            openai: None,
        },
        ..Default::default()
    };
    let state = SettingsState::new(config);

    let (is_set, display) = state.key_status(SettingsField::AnthropicKey);
    assert!(is_set);
    assert!(display.contains("***"));
    assert!(display.starts_with("sk-ant-api03"));

    let (is_set, display) = state.key_status(SettingsField::OpenAiKey);
    assert!(!is_set);
    assert_eq!(display, "Not set");
}

#[test]
fn test_settings_state_provider_auto_detection() {
    use nika_engine::config::ApiKeys;

    // With anthropic key → auto-detect claude
    let config = NikaConfig {
        api_keys: ApiKeys {
            anthropic: Some("sk-ant-test".to_string()),
            openai: None,
        },
        ..Default::default()
    };
    let state = SettingsState::new(config);

    let (is_set, display) = state.key_status(SettingsField::Provider);
    assert!(!is_set); // Not explicitly set
    assert!(display.contains("anthropic")); // Canonical name after alias resolution
    assert!(display.contains("auto"));
}

#[test]
fn test_tui_mode_settings_variant() {
    let mode = TuiMode::Settings;
    assert_eq!(mode, TuiMode::Settings);
    assert_ne!(mode, TuiMode::Normal);
    assert_ne!(mode, TuiMode::Help);
}

#[test]
fn test_tui_mode_all_variants() {
    // Test all TuiMode variants can be created and compared
    let normal = TuiMode::Normal;
    let streaming = TuiMode::Streaming;
    let _inspect = TuiMode::Inspect("task-1".to_string());
    let _edit = TuiMode::Edit("task-1".to_string());
    let search = TuiMode::Search;
    let help = TuiMode::Help;
    let metrics = TuiMode::Metrics;
    let settings = TuiMode::Settings;

    // Test basic equality
    assert_eq!(normal, TuiMode::Normal);
    assert_eq!(streaming, TuiMode::Streaming);
    assert_eq!(search, TuiMode::Search);
    assert_eq!(help, TuiMode::Help);
    assert_eq!(metrics, TuiMode::Metrics);
    assert_eq!(settings, TuiMode::Settings);

    // Test inequality
    assert_ne!(normal, streaming);
    assert_ne!(streaming, help);
    assert_ne!(search, metrics);
}

#[test]
fn test_tui_mode_with_data_variants() {
    let inspect1 = TuiMode::Inspect("task-1".to_string());
    let inspect2 = TuiMode::Inspect("task-1".to_string());
    let inspect3 = TuiMode::Inspect("task-2".to_string());

    let edit1 = TuiMode::Edit("task-1".to_string());
    let edit2 = TuiMode::Edit("task-1".to_string());
    let edit3 = TuiMode::Edit("task-2".to_string());

    // Test equality with same data
    assert_eq!(inspect1, inspect2);
    assert_eq!(edit1, edit2);

    // Test inequality with different data
    assert_ne!(inspect1, inspect3);
    assert_ne!(edit1, edit3);

    // Test inequality across variant types
    assert_ne!(inspect1, edit1);
}

#[test]
fn test_tui_mode_default_is_normal() {
    let mode: TuiMode = Default::default();
    assert_eq!(mode, TuiMode::Normal);
}

#[test]
fn test_tui_state_has_settings() {
    let state = TuiState::new("test.yaml");
    // Settings should be initialized with loaded config
    assert_eq!(state.settings.focus, SettingsField::AnthropicKey);
    assert!(!state.settings.editing);
}

// ═══════════════════════════════════════════
// RETRY TESTS (TIER 1.2)
// ═══════════════════════════════════════════

#[test]
fn test_is_failed_returns_true_on_abort() {
    let mut state = TuiState::new("test.yaml");
    state.workflow.phase = MissionPhase::Abort;
    assert!(state.is_failed());
}

#[test]
fn test_is_failed_returns_true_on_error_message() {
    let mut state = TuiState::new("test.yaml");
    state.workflow.error_message = Some("Something went wrong".to_string());
    assert!(state.is_failed());
}

#[test]
fn test_is_failed_returns_false_on_success() {
    let mut state = TuiState::new("test.yaml");
    state.workflow.phase = MissionPhase::MissionSuccess;
    assert!(!state.is_failed());
    assert!(state.is_success());
}

#[test]
fn test_is_running_returns_true_during_execution() {
    let mut state = TuiState::new("test.yaml");

    state.workflow.phase = MissionPhase::Countdown;
    assert!(state.is_running());

    state.workflow.phase = MissionPhase::Launch;
    assert!(state.is_running());

    state.workflow.phase = MissionPhase::Orbital;
    assert!(state.is_running());

    state.workflow.phase = MissionPhase::Rendezvous;
    assert!(state.is_running());
}

#[test]
fn test_is_running_returns_false_when_not_executing() {
    let mut state = TuiState::new("test.yaml");

    state.workflow.phase = MissionPhase::Preflight;
    assert!(!state.is_running());

    state.workflow.phase = MissionPhase::MissionSuccess;
    assert!(!state.is_running());

    state.workflow.phase = MissionPhase::Abort;
    assert!(!state.is_running());
}

#[test]
fn test_reset_for_retry_resets_workflow_state() {
    let mut state = TuiState::new("test.yaml");

    // Simulate workflow failure
    state.workflow.phase = MissionPhase::Abort;
    state.workflow.error_message = Some("Test error".to_string());
    state.workflow.task_count = 3;
    state.workflow.tasks_completed = 2;

    // Reset for retry
    let reset_tasks = state.reset_for_retry();

    // Verify reset
    assert_eq!(state.workflow.phase, MissionPhase::Preflight);
    assert!(state.workflow.error_message.is_none());
    assert!(state.workflow.final_output.is_none());
    assert_eq!(state.workflow.tasks_completed, 0);
    assert!(reset_tasks.is_empty()); // No tasks were failed in this simple test
}

#[test]
fn test_reset_for_retry_resets_failed_tasks() {
    let mut state = TuiState::new("test.yaml");

    // Add tasks
    state.tasks.insert(
        "task1".to_string(),
        TaskState {
            id: "task1".to_string(),
            task_type: Some("infer".to_string()),
            status: TaskStatus::Success,
            dependencies: vec![],
            started_at: None,
            duration_ms: Some(100),
            input: None,
            output: None,
            error: None,
            tokens: None,
            provider: None,
            model: None,
            prompt_len: None,
            finish_reason: None,
        },
    );
    state.tasks.insert(
        "task2".to_string(),
        TaskState {
            id: "task2".to_string(),
            task_type: Some("exec".to_string()),
            status: TaskStatus::Failed,
            dependencies: vec!["task1".to_string()],
            started_at: None,
            duration_ms: Some(50),
            input: None,
            output: None,
            error: Some("Command failed".to_string()),
            tokens: None,
            provider: None,
            model: None,
            prompt_len: None,
            finish_reason: None,
        },
    );

    // Set workflow to failed
    state.workflow.phase = MissionPhase::Abort;

    // Reset for retry
    let reset_tasks = state.reset_for_retry();

    // Verify task1 unchanged (was success)
    assert_eq!(state.tasks["task1"].status, TaskStatus::Success);

    // Verify task2 reset (was failed)
    assert_eq!(state.tasks["task2"].status, TaskStatus::Pending);
    assert!(state.tasks["task2"].error.is_none());
    assert!(state.tasks["task2"].duration_ms.is_none());

    // Verify reset list
    assert_eq!(reset_tasks.len(), 1);
    assert!(reset_tasks.contains(&"task2".to_string()));
}

// ═══════════════════════════════════════════
// MCP NAVIGATION TESTS (TIER 1.3)
// ═══════════════════════════════════════════

#[test]
fn test_mcp_navigation_empty_list() {
    let mut state = TuiState::new("test.yaml");
    assert!(state.mcp.calls.is_empty());
    assert!(state.mcp.selected_idx.is_none());

    // Navigation on empty list should not panic
    state.select_prev_mcp();
    state.select_next_mcp();
    assert!(state.mcp.selected_idx.is_none());
}

#[test]
fn test_mcp_navigation_select_prev() {
    let mut state = TuiState::new("test.yaml");

    // Add some MCP calls
    for i in 0..3 {
        state.mcp.calls.push_back(McpCall {
            call_id: format!("call-{}", i),
            seq: i,
            server: "novanet".to_string(),
            tool: Some(format!("tool{}", i)),
            resource: None,
            task_id: "task1".to_string(),
            completed: true,
            output_len: Some(100),
            timestamp_ms: 1000 + (i as u64) * 100,
            params: None,
            response: None,
            is_error: false,
            duration_ms: Some(10),
        });
    }

    // First prev should select last item
    state.select_prev_mcp();
    assert_eq!(state.mcp.selected_idx, Some(2));

    // Prev again should go to index 1
    state.select_prev_mcp();
    assert_eq!(state.mcp.selected_idx, Some(1));

    // Prev again should go to index 0
    state.select_prev_mcp();
    assert_eq!(state.mcp.selected_idx, Some(0));

    // Prev again should stay at 0 (boundary)
    state.select_prev_mcp();
    assert_eq!(state.mcp.selected_idx, Some(0));
}

#[test]
fn test_mcp_navigation_select_next() {
    let mut state = TuiState::new("test.yaml");

    // Add some MCP calls
    for i in 0..3 {
        state.mcp.calls.push_back(McpCall {
            call_id: format!("call-{}", i),
            seq: i,
            server: "novanet".to_string(),
            tool: Some(format!("tool{}", i)),
            resource: None,
            task_id: "task1".to_string(),
            completed: true,
            output_len: Some(100),
            timestamp_ms: 1000 + (i as u64) * 100,
            params: None,
            response: None,
            is_error: false,
            duration_ms: Some(10),
        });
    }

    // First next should select first item
    state.select_next_mcp();
    assert_eq!(state.mcp.selected_idx, Some(0));

    // Next again should go to index 1
    state.select_next_mcp();
    assert_eq!(state.mcp.selected_idx, Some(1));

    // Next again should go to index 2
    state.select_next_mcp();
    assert_eq!(state.mcp.selected_idx, Some(2));

    // Next again should stay at 2 (boundary)
    state.select_next_mcp();
    assert_eq!(state.mcp.selected_idx, Some(2));
}

#[test]
fn test_mcp_navigation_get_selected() {
    let mut state = TuiState::new("test.yaml");

    // Add MCP call
    state.mcp.calls.push_back(McpCall {
        call_id: "call-0".to_string(),
        seq: 0,
        server: "novanet".to_string(),
        tool: Some("novanet_describe".to_string()),
        resource: None,
        task_id: "task1".to_string(),
        completed: true,
        output_len: Some(100),
        timestamp_ms: 1000,
        params: None,
        response: None,
        is_error: false,
        duration_ms: Some(10),
    });

    // No selection yet
    assert!(state.get_selected_mcp().is_none());

    // Select
    state.select_mcp(0);
    let selected = state.get_selected_mcp().unwrap();
    assert_eq!(selected.tool.as_deref(), Some("novanet_describe"));
}

// ═══════════════════════════════════════════
// FILTER TESTS (TIER 1.5)
// ═══════════════════════════════════════════

#[test]
fn test_filter_push_adds_characters() {
    let mut state = TuiState::new("test.yaml");
    assert!(state.filter_query.is_empty());
    assert_eq!(state.filter_cursor, 0);

    state.filter_push('h');
    state.filter_push('e');
    state.filter_push('l');
    state.filter_push('l');
    state.filter_push('o');

    assert_eq!(state.filter_query, "hello");
    assert_eq!(state.filter_cursor, 5);
}

#[test]
fn test_filter_backspace_removes_before_cursor() {
    let mut state = TuiState::new("test.yaml");
    state.filter_query = "hello".to_string();
    state.filter_cursor = 5;

    state.filter_backspace();
    assert_eq!(state.filter_query, "hell");
    assert_eq!(state.filter_cursor, 4);

    state.filter_backspace();
    state.filter_backspace();
    assert_eq!(state.filter_query, "he");
    assert_eq!(state.filter_cursor, 2);

    // Backspace at start does nothing
    state.filter_cursor = 0;
    state.filter_backspace();
    assert_eq!(state.filter_query, "he");
    assert_eq!(state.filter_cursor, 0);
}

#[test]
fn test_filter_delete_removes_at_cursor() {
    let mut state = TuiState::new("test.yaml");
    state.filter_query = "hello".to_string();
    state.filter_cursor = 0;

    state.filter_delete();
    assert_eq!(state.filter_query, "ello");
    assert_eq!(state.filter_cursor, 0);

    // Delete at end does nothing
    state.filter_cursor = state.filter_query.len();
    state.filter_delete();
    assert_eq!(state.filter_query, "ello");
}

#[test]
fn test_filter_cursor_movement() {
    let mut state = TuiState::new("test.yaml");
    state.filter_query = "hello".to_string();
    state.filter_cursor = 2;

    state.filter_cursor_left();
    assert_eq!(state.filter_cursor, 1);

    state.filter_cursor_right();
    assert_eq!(state.filter_cursor, 2);

    // Boundary: left at start
    state.filter_cursor = 0;
    state.filter_cursor_left();
    assert_eq!(state.filter_cursor, 0);

    // Boundary: right at end
    state.filter_cursor = 5;
    state.filter_cursor_right();
    assert_eq!(state.filter_cursor, 5);
}

#[test]
fn test_filter_clear_resets_all() {
    let mut state = TuiState::new("test.yaml");
    state.filter_query = "hello".to_string();
    state.filter_cursor = 3;

    state.filter_clear();
    assert!(state.filter_query.is_empty());
    assert_eq!(state.filter_cursor, 0);
}

#[test]
fn test_has_filter() {
    let mut state = TuiState::new("test.yaml");
    assert!(!state.has_filter());

    state.filter_query = "test".to_string();
    assert!(state.has_filter());

    state.filter_clear();
    assert!(!state.has_filter());
}

#[test]
fn test_filtered_task_ids_no_filter() {
    let mut state = TuiState::new("test.yaml");
    state.task_order = vec![
        "task1".to_string(),
        "task2".to_string(),
        "task3".to_string(),
    ];

    // No filter - returns all
    let filtered = state.filtered_task_ids();
    assert_eq!(filtered.len(), 3);
}

#[test]
fn test_filtered_task_ids_matches_id() {
    let mut state = TuiState::new("test.yaml");
    state.task_order = vec![
        "generate".to_string(),
        "fetch_data".to_string(),
        "transform".to_string(),
    ];

    state.filter_query = "gen".to_string();
    let filtered = state.filtered_task_ids();
    assert_eq!(filtered.len(), 1);
    assert_eq!(*filtered[0], "generate");
}

#[test]
fn test_filtered_task_ids_matches_type() {
    let mut state = TuiState::new("test.yaml");
    state.task_order = vec!["task1".to_string(), "task2".to_string()];
    state.tasks.insert(
        "task1".to_string(),
        TaskState {
            id: "task1".to_string(),
            task_type: Some("infer".to_string()),
            status: TaskStatus::Pending,
            dependencies: vec![],
            started_at: None,
            duration_ms: None,
            input: None,
            output: None,
            error: None,
            tokens: None,
            provider: None,
            model: None,
            prompt_len: None,
            finish_reason: None,
        },
    );
    state.tasks.insert(
        "task2".to_string(),
        TaskState {
            id: "task2".to_string(),
            task_type: Some("exec".to_string()),
            status: TaskStatus::Pending,
            dependencies: vec![],
            started_at: None,
            duration_ms: None,
            input: None,
            output: None,
            error: None,
            tokens: None,
            provider: None,
            model: None,
            prompt_len: None,
            finish_reason: None,
        },
    );

    state.filter_query = "infer".to_string();
    let filtered = state.filtered_task_ids();
    assert_eq!(filtered.len(), 1);
    assert_eq!(*filtered[0], "task1");
}

#[test]
fn test_filtered_task_ids_case_insensitive() {
    let mut state = TuiState::new("test.yaml");
    state.task_order = vec!["GeneratePage".to_string()];

    state.filter_query = "page".to_string();
    let filtered = state.filtered_task_ids();
    assert_eq!(filtered.len(), 1);

    state.filter_query = "PAGE".to_string();
    let filtered = state.filtered_task_ids();
    assert_eq!(filtered.len(), 1);
}

#[test]
fn test_filtered_mcp_calls_no_filter() {
    let mut state = TuiState::new("test.yaml");
    state.mcp.calls.push_back(McpCall {
        call_id: "call-0".to_string(),
        seq: 0,
        server: "novanet".to_string(),
        tool: Some("novanet_describe".to_string()),
        resource: None,
        task_id: "task1".to_string(),
        completed: true,
        output_len: Some(100),
        timestamp_ms: 1000,
        params: None,
        response: None,
        is_error: false,
        duration_ms: Some(10),
    });

    // No filter - returns all
    let filtered = state.filtered_mcp_calls();
    assert_eq!(filtered.len(), 1);
}

#[test]
fn test_filtered_mcp_calls_matches_server() {
    let mut state = TuiState::new("test.yaml");
    state.mcp.calls.push_back(McpCall {
        call_id: "call-0".to_string(),
        seq: 0,
        server: "novanet".to_string(),
        tool: Some("novanet_describe".to_string()),
        resource: None,
        task_id: "task1".to_string(),
        completed: true,
        output_len: Some(100),
        timestamp_ms: 1000,
        params: None,
        response: None,
        is_error: false,
        duration_ms: Some(10),
    });
    state.mcp.calls.push_back(McpCall {
        call_id: "call-1".to_string(),
        seq: 1,
        server: "other_server".to_string(),
        tool: Some("other_tool".to_string()),
        resource: None,
        task_id: "task1".to_string(),
        completed: true,
        output_len: Some(100),
        timestamp_ms: 1100,
        params: None,
        response: None,
        is_error: false,
        duration_ms: Some(10),
    });

    state.filter_query = "nova".to_string();
    let filtered = state.filtered_mcp_calls();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].server, "novanet");
}

#[test]
fn test_filtered_mcp_calls_matches_tool() {
    let mut state = TuiState::new("test.yaml");
    state.mcp.calls.push_back(McpCall {
        call_id: "call-0".to_string(),
        seq: 0,
        server: "novanet".to_string(),
        tool: Some("novanet_describe".to_string()),
        resource: None,
        task_id: "task1".to_string(),
        completed: true,
        output_len: Some(100),
        timestamp_ms: 1000,
        params: None,
        response: None,
        is_error: false,
        duration_ms: Some(10),
    });
    state.mcp.calls.push_back(McpCall {
        call_id: "call-1".to_string(),
        seq: 1,
        server: "novanet".to_string(),
        tool: Some("novanet_search".to_string()),
        resource: None,
        task_id: "task1".to_string(),
        completed: true,
        output_len: Some(100),
        timestamp_ms: 1100,
        params: None,
        response: None,
        is_error: false,
        duration_ms: Some(10),
    });

    state.filter_query = "describe".to_string();
    let filtered = state.filtered_mcp_calls();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].tool.as_deref(), Some("novanet_describe"));
}

#[test]
fn test_filtered_mcp_calls_matches_resource() {
    let mut state = TuiState::new("test.yaml");
    state.mcp.calls.push_back(McpCall {
        call_id: "call-0".to_string(),
        seq: 0,
        server: "novanet".to_string(),
        tool: None,
        resource: Some("neo4j://entity/qr-code".to_string()),
        task_id: "task1".to_string(),
        completed: true,
        output_len: Some(100),
        timestamp_ms: 1000,
        params: None,
        response: None,
        is_error: false,
        duration_ms: Some(10),
    });

    state.filter_query = "qr-code".to_string();
    let filtered = state.filtered_mcp_calls();
    assert_eq!(filtered.len(), 1);
    assert!(filtered[0].resource.as_ref().unwrap().contains("qr-code"));
}

// ═══════════════════════════════════════════════════════════════════════════
// NOTIFICATION TESTS (TIER 3.4)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_notification_level_icons() {
    assert_eq!(NotificationLevel::Info.icon(), "ℹ");
    assert_eq!(NotificationLevel::Warning.icon(), "⚠");
    assert_eq!(NotificationLevel::Alert.icon(), "🔔");
    assert_eq!(NotificationLevel::Success.icon(), "✓");
    assert_eq!(NotificationLevel::Error.icon(), "✗");
}

#[test]
fn test_notification_constructors() {
    let n = Notification::info("Test info", 1000);
    assert_eq!(n.level, NotificationLevel::Info);
    assert_eq!(n.message, "Test info");
    assert_eq!(n.timestamp_ms, 1000);
    assert!(!n.dismissed);

    let n = Notification::warning("Test warning", 2000);
    assert_eq!(n.level, NotificationLevel::Warning);

    let n = Notification::alert("Test alert", 3000);
    assert_eq!(n.level, NotificationLevel::Alert);

    let n = Notification::success("Test success", 4000);
    assert_eq!(n.level, NotificationLevel::Success);

    let n = Notification::error("Test error", 5000);
    assert_eq!(n.level, NotificationLevel::Error);
}

#[test]
fn test_add_notification() {
    let mut state = TuiState::new("test.yaml");
    assert_eq!(state.notifs.items.len(), 0);

    state.add_notification(Notification::info("Test 1", 1000));
    assert_eq!(state.notifs.items.len(), 1);
    assert_eq!(state.notifs.items[0].message, "Test 1");

    state.add_notification(Notification::warning("Test 2", 2000));
    assert_eq!(state.notifs.items.len(), 2);
}

#[test]
fn test_notification_max_limit() {
    let mut state = TuiState::new("test.yaml");
    state.notifs.max_items = 3;

    // Add 5 notifications
    for i in 0..5 {
        state.add_notification(Notification::info(format!("Test {}", i), i * 1000));
    }

    // Should only keep last 3
    assert_eq!(state.notifs.items.len(), 3);
    assert_eq!(state.notifs.items[0].message, "Test 2");
    assert_eq!(state.notifs.items[1].message, "Test 3");
    assert_eq!(state.notifs.items[2].message, "Test 4");
}

#[test]
fn test_active_notifications() {
    let mut state = TuiState::new("test.yaml");

    state.add_notification(Notification::info("Active 1", 1000));
    state.add_notification(Notification::info("Active 2", 2000));
    state.notifs.items[0].dismissed = true;

    let active = state.active_notifications();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].message, "Active 2");
}

#[test]
fn test_active_notification_count() {
    let mut state = TuiState::new("test.yaml");

    state.add_notification(Notification::info("1", 1000));
    state.add_notification(Notification::info("2", 2000));
    state.add_notification(Notification::info("3", 3000));
    assert_eq!(state.active_notification_count(), 3);

    state.notifs.items[1].dismissed = true;
    assert_eq!(state.active_notification_count(), 2);
}

#[test]
fn test_dismiss_notification() {
    let mut state = TuiState::new("test.yaml");

    state.add_notification(Notification::info("1", 1000));
    state.add_notification(Notification::info("2", 2000));
    state.add_notification(Notification::info("3", 3000));

    // Dismiss most recent -- compact removes it, leaving ["1", "2"]
    state.dismiss_notification();
    assert_eq!(state.notifs.items.len(), 2);
    assert_eq!(state.notifs.items[0].message, "1");
    assert_eq!(state.notifs.items[1].message, "2");

    // Dismiss next most recent -- compact removes it, leaving ["1"]
    state.dismiss_notification();
    assert_eq!(state.notifs.items.len(), 1);
    assert_eq!(state.notifs.items[0].message, "1");
}

#[test]
fn test_dismiss_all_notifications() {
    let mut state = TuiState::new("test.yaml");

    state.add_notification(Notification::info("1", 1000));
    state.add_notification(Notification::info("2", 2000));
    state.add_notification(Notification::info("3", 3000));

    state.dismiss_all_notifications();

    assert_eq!(
        state.notifs.items.len(),
        0,
        "dismiss_all must clear all items from the list"
    );
    assert_eq!(state.active_notification_count(), 0);
}

#[test]
fn test_clear_notifications() {
    let mut state = TuiState::new("test.yaml");

    state.add_notification(Notification::info("1", 1000));
    state.add_notification(Notification::info("2", 2000));
    assert_eq!(state.notifs.items.len(), 2);

    state.clear_notifications();
    assert_eq!(state.notifs.items.len(), 0);
}

#[test]
fn test_workflow_completed_adds_notification() {
    let mut state = TuiState::new("test.yaml");
    state.workflow.task_count = 4;
    state.workflow.tasks_completed = 4;

    state.handle_event(
        &EventKind::WorkflowCompleted {
            final_output: std::sync::Arc::new(serde_json::Value::Null),
            total_duration_ms: 5000,
        },
        5000,
    );

    assert_eq!(state.notifs.items.len(), 1);
    assert_eq!(state.notifs.items[0].level, NotificationLevel::Success);
    assert!(state.notifs.items[0].message.contains("Magnificent"));
}

#[test]
fn test_workflow_failed_adds_notification() {
    let mut state = TuiState::new("test.yaml");

    state.handle_event(
        &EventKind::WorkflowFailed {
            error: "Something went wrong".to_string(),
            failed_task: None,
        },
        5000,
    );

    assert_eq!(state.notifs.items.len(), 1);
    assert_eq!(state.notifs.items[0].level, NotificationLevel::Error);
    assert!(state.notifs.items[0].message.contains("failed"));
}

#[test]
fn test_slow_task_adds_warning() {
    let mut state = TuiState::new("test.yaml");

    // First create the task
    state.tasks.insert(
        "slow-task".to_string(),
        TaskState::new("slow-task".to_string(), vec![]),
    );

    // Slow task (>10s but <30s) should add warning
    state.handle_event(
        &EventKind::TaskCompleted {
            task_id: "slow-task".into(),
            output: std::sync::Arc::new(serde_json::Value::Null),
            duration_ms: 15000,
        },
        15000,
    );

    assert_eq!(state.notifs.items.len(), 1);
    assert_eq!(state.notifs.items[0].level, NotificationLevel::Warning);
    assert!(state.notifs.items[0].message.contains("15.0s"));
}

#[test]
fn test_very_slow_task_adds_alert() {
    let mut state = TuiState::new("test.yaml");

    // First create the task
    state.tasks.insert(
        "very-slow-task".to_string(),
        TaskState::new("very-slow-task".to_string(), vec![]),
    );

    // Very slow task (>30s) should add alert
    state.handle_event(
        &EventKind::TaskCompleted {
            task_id: "very-slow-task".into(),
            output: std::sync::Arc::new(serde_json::Value::Null),
            duration_ms: 35000,
        },
        35000,
    );

    assert_eq!(state.notifs.items.len(), 1);
    assert_eq!(state.notifs.items[0].level, NotificationLevel::Alert);
    assert!(state.notifs.items[0].message.contains("35.0s"));
}

// ═══════════════════════════════════════════════════════════════════════════
// TIER 4.1: Dirty Flags Tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_dirty_flags_default() {
    let flags = DirtyFlags::default();
    assert!(!flags.all);
    assert!(!flags.progress);
    assert!(!flags.dag);
    assert!(!flags.novanet);
    assert!(!flags.reasoning);
    assert!(!flags.status);
    assert!(!flags.notifications);
    assert!(!flags.any());
}

#[test]
fn test_dirty_flags_mark_all() {
    let mut flags = DirtyFlags::default();
    flags.mark_all();
    assert!(flags.all);
    assert!(flags.any());
}

#[test]
fn test_dirty_flags_clear() {
    let mut flags = DirtyFlags {
        all: true,
        progress: true,
        dag: true,
        novanet: true,
        reasoning: true,
        status: true,
        notifications: true,
    };

    flags.clear();

    assert!(!flags.all);
    assert!(!flags.progress);
    assert!(!flags.dag);
    assert!(!flags.novanet);
    assert!(!flags.reasoning);
    assert!(!flags.status);
    assert!(!flags.notifications);
    assert!(!flags.any());
}

#[test]
fn test_dirty_flags_any() {
    let mut flags = DirtyFlags::default();
    assert!(!flags.any());

    flags.progress = true;
    assert!(flags.any());

    flags.progress = false;
    flags.dag = true;
    assert!(flags.any());
}

#[test]
fn test_dirty_flags_is_panel_dirty() {
    // When all is true, all panels are dirty
    let mut flags = DirtyFlags {
        all: true,
        ..Default::default()
    };
    assert!(flags.is_panel_dirty(MonitorPanel::Progress));
    assert!(flags.is_panel_dirty(MonitorPanel::Dag));
    assert!(flags.is_panel_dirty(MonitorPanel::NovaNet));
    assert!(flags.is_panel_dirty(MonitorPanel::Agent));

    // Individual flags
    flags.all = false;
    assert!(!flags.is_panel_dirty(MonitorPanel::Progress));

    flags.progress = true;
    assert!(flags.is_panel_dirty(MonitorPanel::Progress));
    assert!(!flags.is_panel_dirty(MonitorPanel::Dag));

    flags.dag = true;
    assert!(flags.is_panel_dirty(MonitorPanel::Dag));

    flags.novanet = true;
    assert!(flags.is_panel_dirty(MonitorPanel::NovaNet));

    flags.reasoning = true;
    assert!(flags.is_panel_dirty(MonitorPanel::Agent));
}

#[test]
fn test_workflow_started_marks_all_dirty() {
    let mut state = TuiState::new("test.yaml");

    state.handle_event(
        &EventKind::WorkflowStarted {
            task_count: 5,
            generation_id: "gen-123".to_string(),
            workflow_hash: "abc".to_string(),
            nika_version: TEST_VERSION.to_string(),
        },
        0,
    );

    assert!(state.dirty.all);
}

#[test]
fn test_workflow_completed_marks_progress_status_dirty() {
    let mut state = TuiState::new("test.yaml");
    state.dirty.clear();

    state.handle_event(
        &EventKind::WorkflowCompleted {
            final_output: std::sync::Arc::new(serde_json::Value::Null),
            total_duration_ms: 1000,
        },
        1000,
    );

    assert!(state.dirty.progress);
    assert!(state.dirty.status);
    assert!(state.dirty.notifications); // from add_notification
}

#[test]
fn test_task_events_mark_progress_dag_dirty() {
    let mut state = TuiState::new("test.yaml");

    // TaskScheduled
    state.dirty.clear();
    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: "task1".into(),
            dependencies: vec![],
        },
        100,
    );
    assert!(state.dirty.progress);
    assert!(state.dirty.dag);

    // TaskStarted
    state.dirty.clear();
    state.handle_event(
        &EventKind::TaskStarted {
            verb: "infer".into(),
            task_id: "task1".into(),
            inputs: Arc::new(serde_json::json!({})),
        },
        200,
    );
    assert!(state.dirty.progress);
    assert!(state.dirty.dag);

    // TaskCompleted
    state.dirty.clear();
    state.handle_event(
        &EventKind::TaskCompleted {
            task_id: "task1".into(),
            output: std::sync::Arc::new(serde_json::Value::Null),
            duration_ms: 500,
        },
        300,
    );
    assert!(state.dirty.progress);
    assert!(state.dirty.dag);
}

#[test]
fn test_task_failed_marks_status_dirty() {
    let mut state = TuiState::new("test.yaml");
    state.tasks.insert(
        "task1".to_string(),
        TaskState::new("task1".to_string(), vec![]),
    );
    state.dirty.clear();

    state.handle_event(
        &EventKind::TaskFailed {
            task_id: "task1".into(),
            error: "error".into(),
            duration_ms: 100,
            error_code: None,
        },
        100,
    );

    assert!(state.dirty.progress);
    assert!(state.dirty.dag);
    assert!(state.dirty.status);
}

#[test]
fn test_mcp_events_mark_novanet_dirty() {
    let mut state = TuiState::new("test.yaml");
    state.dirty.clear();

    state.handle_event(
        &EventKind::McpInvoke {
            task_id: "task1".into(),
            mcp_server: "novanet".to_string(),
            tool: Some("describe".to_string()),
            resource: None,
            call_id: "call1".to_string(),
            params: None,
        },
        100,
    );
    assert!(state.dirty.novanet);

    state.dirty.clear();
    state.handle_event(
        &EventKind::McpResponse {
            task_id: "task1".into(),
            output_len: 100,
            call_id: "call1".to_string(),
            duration_ms: 50,
            cached: false,
            is_error: false,
            response: None,
        },
        150,
    );
    assert!(state.dirty.novanet);
}

#[test]
fn test_agent_events_mark_reasoning_dirty() {
    let mut state = TuiState::new("test.yaml");
    state.dirty.clear();

    state.handle_event(
        &EventKind::AgentStart {
            task_id: "task1".into(),
            max_turns: 5,
            mcp_servers: vec![],
        },
        100,
    );
    assert!(state.dirty.reasoning);

    state.dirty.clear();
    state.handle_event(
        &EventKind::AgentTurn {
            task_id: "task1".into(),
            turn_index: 0,
            kind: AgentTurnKind::Started,
            metadata: None,
        },
        200,
    );
    assert!(state.dirty.reasoning);

    state.dirty.clear();
    state.handle_event(
        &EventKind::AgentComplete {
            task_id: "task1".into(),
            turns: 1,
            stop_reason: AgentStopReason::Natural,
        },
        300,
    );
    assert!(state.dirty.reasoning);
}

#[test]
fn test_add_notification_marks_notifications_dirty() {
    let mut state = TuiState::new("test.yaml");
    state.dirty.clear();

    state.add_notification(Notification::info("test", 100));
    assert!(state.dirty.notifications);
}

#[test]
fn test_dismiss_notification_marks_notifications_dirty() {
    let mut state = TuiState::new("test.yaml");
    state.add_notification(Notification::info("test", 100));
    state.dirty.clear();

    state.dismiss_notification();
    assert!(state.dirty.notifications);
}

#[test]
fn test_dismiss_all_marks_notifications_dirty() {
    let mut state = TuiState::new("test.yaml");
    state.add_notification(Notification::info("test", 100));
    state.dirty.clear();

    state.dismiss_all_notifications();
    assert!(state.dirty.notifications);
}

#[test]
fn test_clear_notifications_marks_dirty() {
    let mut state = TuiState::new("test.yaml");
    state.add_notification(Notification::info("test", 100));
    state.dirty.clear();

    state.clear_notifications();
    assert!(state.dirty.notifications);
}

// ═══════════════════════════════════════════════════════════════════════════════
// TIER 4.4: JSON FORMAT CACHE TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_json_cache_new() {
    let cache = JsonFormatCache::new();
    assert_eq!(cache.stats(), (0, 50)); // 0 entries, max 50
}

#[test]
fn test_json_cache_get_or_format_caches() {
    let mut cache = JsonFormatCache::new();

    // First call should format and cache
    let value = serde_json::json!({"name": "test"});
    let result1 = cache.get_or_format("key1", &value).to_string();
    assert!(result1.contains("name"));

    // Second call should return cached
    let result2 = cache.get_or_format("key1", &value).to_string();
    assert_eq!(result1, result2);
    assert_eq!(cache.stats().0, 1); // 1 entry
}

#[test]
fn test_json_cache_different_keys() {
    let mut cache = JsonFormatCache::new();

    let value1 = serde_json::json!({"a": 1});
    let value2 = serde_json::json!({"b": 2});

    cache.get_or_format("key1", &value1);
    cache.get_or_format("key2", &value2);

    assert_eq!(cache.stats().0, 2); // 2 entries
}

#[test]
fn test_json_cache_invalidate() {
    let mut cache = JsonFormatCache::new();
    let value = serde_json::json!({"test": true});

    cache.get_or_format("key1", &value);
    cache.get_or_format("key2", &value);
    assert_eq!(cache.stats().0, 2);

    cache.invalidate("key1");
    assert_eq!(cache.stats().0, 1);
}

#[test]
fn test_json_cache_invalidate_prefix() {
    let mut cache = JsonFormatCache::new();
    let value = serde_json::json!({"test": true});

    cache.get_or_format("task:abc", &value);
    cache.get_or_format("task:def", &value);
    cache.get_or_format("mcp:xyz", &value);
    assert_eq!(cache.stats().0, 3);

    cache.invalidate_prefix("task:");
    assert_eq!(cache.stats().0, 1); // Only mcp:xyz remains
}

#[test]
fn test_json_cache_clear() {
    let mut cache = JsonFormatCache::new();
    let value = serde_json::json!({"test": true});

    cache.get_or_format("key1", &value);
    cache.get_or_format("key2", &value);
    assert_eq!(cache.stats().0, 2);

    cache.clear();
    assert_eq!(cache.stats().0, 0);
}

#[test]
fn test_json_cache_eviction_on_limit() {
    let mut cache = JsonFormatCache::with_capacity(5); // Small limit for testing

    let value = serde_json::json!({"test": true});

    // Fill cache to limit
    for i in 0..5 {
        cache.get_or_format(&format!("key{}", i), &value);
    }
    assert_eq!(cache.stats().0, 5);

    // Adding one more should trigger eviction
    cache.get_or_format("key_new", &value);
    // Should have fewer entries due to eviction (removes ~10%)
    assert!(cache.stats().0 < 6);
}

#[test]
fn test_workflow_start_clears_json_cache() {
    let mut state = TuiState::new("test.yaml");
    let value = serde_json::json!({"test": true});

    state.json_cache.get_or_format("key1", &value);
    assert_eq!(state.json_cache.stats().0, 1);

    // Workflow start should clear the cache
    state.handle_event(
        &EventKind::WorkflowStarted {
            task_count: 1,
            workflow_hash: "hash-123".into(),
            generation_id: "gen-123".into(),
            nika_version: "0.5.1".into(),
        },
        100,
    );

    assert_eq!(state.json_cache.stats().0, 0);
}

#[test]
fn test_task_started_invalidates_task_cache() {
    let mut state = TuiState::new("test.yaml");
    let value = serde_json::json!({"test": true});

    // Pre-populate cache
    state.json_cache.get_or_format("task:my_task", &value);
    state.json_cache.get_or_format("task:other_task", &value);
    assert_eq!(state.json_cache.stats().0, 2);

    // Task start should invalidate only that task's cache
    state.handle_event(
        &EventKind::TaskStarted {
            verb: "infer".into(),
            task_id: "my_task".into(),
            inputs: Arc::new(serde_json::json!({})),
        },
        100,
    );

    assert_eq!(state.json_cache.stats().0, 1); // other_task remains
}

#[test]
fn test_mcp_response_invalidates_mcp_cache() {
    let mut state = TuiState::new("test.yaml");
    let value = serde_json::json!({"test": true});

    // Pre-populate cache
    state.json_cache.get_or_format("mcp:call-123", &value);
    state.json_cache.get_or_format("mcp:call-456", &value);
    assert_eq!(state.json_cache.stats().0, 2);

    // MCP response should invalidate that call's cache
    state.handle_event(
        &EventKind::McpResponse {
            task_id: "task1".into(),
            output_len: 100,
            call_id: "call-123".into(),
            duration_ms: 50,
            cached: false,
            is_error: false,
            response: None,
        },
        100,
    );

    assert_eq!(state.json_cache.stats().0, 1); // call-456 remains
}

// ═══════════════════════════════════════════
// CHAT OVERLAY MESSAGE TYPE TESTS
// (ChatOverlayState is data-only, used by session persistence)
// ═══════════════════════════════════════════

#[test]
fn test_chat_overlay_message_new() {
    let msg = ChatOverlayMessage::new(ChatOverlayMessageRole::User, "test message");
    assert_eq!(msg.role, ChatOverlayMessageRole::User);
    assert_eq!(msg.content, "test message");
}

#[test]
fn test_chat_overlay_message_roles() {
    let user = ChatOverlayMessage::new(ChatOverlayMessageRole::User, "hello");
    let nika = ChatOverlayMessage::new(ChatOverlayMessageRole::Nika, "hi");
    let system = ChatOverlayMessage::new(ChatOverlayMessageRole::System, "welcome");
    let tool = ChatOverlayMessage::new(ChatOverlayMessageRole::Tool, "result");

    assert_eq!(user.role, ChatOverlayMessageRole::User);
    assert_eq!(nika.role, ChatOverlayMessageRole::Nika);
    assert_eq!(system.role, ChatOverlayMessageRole::System);
    assert_eq!(tool.role, ChatOverlayMessageRole::Tool);
}

// ═══ PanelScrollState Tests ═══

#[test]
fn test_panel_scroll_state_new() {
    let state = PanelScrollState::new();
    assert_eq!(state.offset, 0);
    assert_eq!(state.cursor, 0);
    assert_eq!(state.total, 0);
    assert_eq!(state.visible, 0);
}

#[test]
fn test_panel_scroll_state_with_total() {
    let state = PanelScrollState::with_total(100);
    assert_eq!(state.total, 100);
    assert_eq!(state.cursor, 0);
    assert_eq!(state.offset, 0);
}

#[test]
fn test_panel_scroll_state_cursor_down() {
    let mut state = PanelScrollState::with_total(10);
    state.visible = 5;

    state.cursor_down();
    assert_eq!(state.cursor, 1);

    // Move to end
    for _ in 0..10 {
        state.cursor_down();
    }
    assert_eq!(state.cursor, 9); // Can't go beyond total - 1
}

#[test]
fn test_panel_scroll_state_cursor_up() {
    let mut state = PanelScrollState::with_total(10);
    state.visible = 5;
    state.cursor = 5;

    state.cursor_up();
    assert_eq!(state.cursor, 4);

    // Move to start
    for _ in 0..10 {
        state.cursor_up();
    }
    assert_eq!(state.cursor, 0); // Can't go below 0
}

#[test]
fn test_panel_scroll_state_ensure_cursor_visible() {
    let mut state = PanelScrollState::with_total(100);
    state.visible = 10;
    state.cursor = 50;

    state.ensure_cursor_visible();

    // Cursor should be within visible range with margin
    let margin = SCROLL_MARGIN.min(state.visible / 2);
    assert!(state.cursor >= state.offset + margin || state.cursor < margin);
    assert!(state.cursor < state.offset + state.visible);
}

#[test]
fn test_panel_scroll_state_cursor_first_last() {
    let mut state = PanelScrollState::with_total(100);
    state.visible = 10;
    state.cursor = 50;

    state.cursor_first();
    assert_eq!(state.cursor, 0);
    assert_eq!(state.offset, 0);

    state.cursor_last();
    assert_eq!(state.cursor, 99);
}

#[test]
fn test_panel_scroll_state_page_up_down() {
    let mut state = PanelScrollState::with_total(100);
    state.visible = 10;

    state.page_down();
    assert_eq!(state.cursor, 10);

    state.page_down();
    assert_eq!(state.cursor, 20);

    state.page_up();
    assert_eq!(state.cursor, 10);
}

#[test]
fn test_panel_scroll_state_selected() {
    let state = PanelScrollState::with_total(10);
    assert_eq!(state.selected(), Some(0));

    let empty_state = PanelScrollState::new();
    assert_eq!(empty_state.selected(), None);
}

#[test]
fn test_panel_scroll_state_is_selected() {
    let mut state = PanelScrollState::with_total(10);
    state.cursor = 5;

    assert!(state.is_selected(5));
    assert!(!state.is_selected(3));
}

#[test]
fn test_panel_scroll_state_visible_range() {
    let mut state = PanelScrollState::with_total(100);
    state.visible = 10;
    state.offset = 20;

    let range = state.visible_range();
    assert_eq!(range, 20..30);
}

#[test]
fn test_panel_scroll_state_at_boundaries() {
    let mut state = PanelScrollState::with_total(100);
    state.visible = 10;

    assert!(state.at_top());
    assert!(!state.at_bottom());

    state.offset = 90;
    assert!(!state.at_top());
    assert!(state.at_bottom());
}

#[test]
fn test_panel_scroll_state_set_total_clamps_cursor() {
    let mut state = PanelScrollState::with_total(100);
    state.cursor = 90;
    state.visible = 10;

    // Reduce total, cursor should clamp
    state.set_total(50);
    assert_eq!(state.cursor, 49);
}

#[test]
fn test_panel_scroll_state_percentage() {
    let mut state = PanelScrollState::with_total(100);
    state.visible = 10;

    assert!((state.percentage() - 0.0).abs() < f64::EPSILON);

    state.offset = 45; // Middle
    assert!((state.percentage() - 0.5).abs() < f64::EPSILON);

    state.offset = 90; // End
    assert!((state.percentage() - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_panel_scroll_state_scroll_down() {
    let mut state = PanelScrollState::with_total(100);
    state.visible = 10;
    state.offset = 0;

    // Scroll down should move offset
    state.scroll_down();
    assert_eq!(state.offset, 1);

    // Can scroll to end
    for _ in 0..90 {
        state.scroll_down();
    }
    assert_eq!(state.offset, 90);

    // Should not scroll past end
    state.scroll_down();
    assert_eq!(state.offset, 90);
}

#[test]
fn test_panel_scroll_state_scroll_up() {
    let mut state = PanelScrollState::with_total(100);
    state.visible = 10;
    state.offset = 50;

    // Scroll up should move offset
    state.scroll_up();
    assert_eq!(state.offset, 49);

    // Can scroll to top
    for _ in 0..49 {
        state.scroll_up();
    }
    assert_eq!(state.offset, 0);

    // Should not scroll past top
    state.scroll_up();
    assert_eq!(state.offset, 0);
}

#[test]
fn test_panel_scroll_state_scroll_to_top() {
    let mut state = PanelScrollState::with_total(100);
    state.visible = 10;
    state.offset = 50;
    state.cursor = 50;

    // Scroll to top should reset both offset and cursor
    state.scroll_to_top();
    assert_eq!(state.offset, 0);
    assert_eq!(state.cursor, 0);
}

#[test]
fn test_panel_scroll_state_scroll_to_bottom() {
    let mut state = PanelScrollState::with_total(100);
    state.visible = 10;
    state.offset = 0;
    state.cursor = 0;

    // Scroll to bottom
    state.scroll_to_bottom();
    assert_eq!(state.offset, 90); // 100 - 10
    assert_eq!(state.cursor, 99); // Last item
}

#[test]
fn test_panel_scroll_state_scroll_to_bottom_less_than_viewport() {
    let mut state = PanelScrollState::with_total(5); // Less than viewport
    state.visible = 10;
    state.offset = 0;
    state.cursor = 0;

    // Scroll to bottom should work even with small total
    state.scroll_to_bottom();
    assert_eq!(state.offset, 0); // Can't scroll
    assert_eq!(state.cursor, 4); // Last item
}

#[test]
fn test_panel_scroll_state_set_visible() {
    let mut state = PanelScrollState::with_total(100);
    state.visible = 10;
    state.offset = 50;
    state.cursor = 50;

    // Change visible size
    state.set_visible(20);
    assert_eq!(state.visible, 20);

    // Cursor should still be visible
    assert!(state.cursor >= state.offset);
    assert!(state.cursor < state.offset + state.visible);
}

#[test]
fn test_panel_scroll_state_set_visible_with_cursor_adjustment() {
    let mut state = PanelScrollState::with_total(100);
    state.visible = 5;
    state.offset = 0;
    state.cursor = 0;

    // Increase visible - ensures cursor remains visible
    state.set_visible(50);
    assert_eq!(state.visible, 50);
    assert_eq!(state.cursor, 0);
}

#[test]
fn test_panel_scroll_state_scroll_behavior_with_zero_total() {
    let mut state = PanelScrollState::new();
    state.visible = 10;
    // total = 0 (default)

    // Operations should be safe
    state.scroll_up();
    assert_eq!(state.offset, 0);

    state.scroll_down();
    assert_eq!(state.offset, 0);

    state.scroll_to_top();
    assert_eq!(state.offset, 0);
    assert_eq!(state.cursor, 0);

    state.scroll_to_bottom();
    assert_eq!(state.offset, 0);
    assert_eq!(state.cursor, 0);
}

#[test]
fn test_panel_scroll_state_scroll_behavior_with_zero_visible() {
    let mut state = PanelScrollState::with_total(100);
    state.visible = 0;
    state.offset = 50;

    // Operations should be safe with zero visible
    state.scroll_up();
    assert_eq!(state.offset, 49);

    state.scroll_down();
    assert_eq!(state.offset, 50); // Can still scroll

    state.scroll_to_bottom();
    // offset should adjust to try to show items (but visible is 0)
    assert!(state.cursor == 99);
}

#[test]
fn test_panel_scroll_state_percentage_with_small_content() {
    let mut state = PanelScrollState::with_total(5);
    state.visible = 10; // Viewport larger than content

    // When total <= visible, percentage should be 0
    assert!((state.percentage() - 0.0).abs() < f64::EPSILON);

    state.offset = 1;
    // Should still be 0 (no scrolling possible)
    assert!((state.percentage() - 0.0).abs() < f64::EPSILON);
}

#[test]
fn test_panel_scroll_state_all_methods_preserve_invariants() {
    let mut state = PanelScrollState::with_total(100);
    state.visible = 20;

    // After any operation, these should hold:
    // 1. offset + visible >= total should never show empty space
    // 2. cursor should always be < total
    // 3. offset should never exceed total - visible

    for cursor_val in 0..100 {
        state.cursor = cursor_val;
        state.ensure_cursor_visible();

        assert!(state.cursor < state.total || state.total == 0);
        let max_offset = state.total.saturating_sub(state.visible);
        assert!(state.offset <= max_offset);
    }
}

// ═══════════════════════════════════════════
// P3 Fix: Error dismissal tests
// ═══════════════════════════════════════════

#[test]
fn test_dismiss_error_clears_message() {
    let mut state = TuiState::new("test.yaml");
    state.workflow.error_message = Some("Test error".to_string());

    let dismissed = state.dismiss_error();

    assert!(dismissed);
    assert!(state.workflow.error_message.is_none());
    assert!(state.dirty.progress);
    assert!(state.dirty.status);
}

#[test]
fn test_dismiss_error_returns_false_when_no_error() {
    let mut state = TuiState::new("test.yaml");
    assert!(state.workflow.error_message.is_none());

    let dismissed = state.dismiss_error();

    assert!(!dismissed);
    // dirty flags should not change when no error to dismiss
}

#[test]
fn test_dismiss_error_preserves_other_workflow_state() {
    let mut state = TuiState::new("test.yaml");
    state.workflow.error_message = Some("Test error".to_string());
    state.workflow.phase = MissionPhase::Abort;
    state.workflow.task_count = 5;
    state.workflow.tasks_completed = 3;

    state.dismiss_error();

    // Error dismissed but other state preserved
    assert!(state.workflow.error_message.is_none());
    assert_eq!(state.workflow.phase, MissionPhase::Abort); // Phase not changed
    assert_eq!(state.workflow.task_count, 5);
    assert_eq!(state.workflow.tasks_completed, 3);
}

// ═══════════════════════════════════════════════════════════════════════════════
// P0 Task 1: DirtyFlags render pipeline tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_dirty_flags_cleared_after_render_cycle() {
    let mut state = TuiState::new("test.nika.yaml");

    // TuiState::new() starts with dirty.all = true (first frame needs full redraw)
    assert!(state.dirty.all);
    // Clear initial dirty flags (simulates first render completion)
    state.clear_dirty();
    assert!(!state.dirty.any());

    // Mark some flags dirty (simulates handle_event())
    state.dirty.progress = true;
    state.dirty.dag = true;
    assert!(state.dirty.any());

    // Simulate render completion
    state.clear_dirty();

    // All flags cleared
    assert!(!state.dirty.any());
    assert!(!state.dirty.progress);
    assert!(!state.dirty.dag);
}

#[test]
fn test_dirty_all_takes_precedence() {
    let mut state = TuiState::new("test.nika.yaml");

    // Clear initial dirty.all from constructor
    state.clear_dirty();

    state.dirty.dag = true;
    assert!(state.dirty.any());
    assert!(!state.dirty.all);

    state.dirty.mark_all();
    assert!(state.dirty.all);

    state.clear_dirty();
    assert!(!state.dirty.any());
}

// ═══════════════════════════════════════════════════════════════════════════════
// P0 Task 2: DAG cache invalidation tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_dag_version_tracks_timeline() {
    let mut state = TuiState::new("test.nika.yaml");

    let v0 = state.dag_version();
    // Simulate a task event to bump timeline_version
    state.handle_event(
        &nika_engine::event::EventKind::TaskStarted {
            task_id: std::sync::Arc::from("task1"),
            verb: std::sync::Arc::from("infer"),
            inputs: Arc::new(serde_json::json!({})),
        },
        100,
    );
    let v1 = state.dag_version();
    assert!(v1 > v0, "dag_version should increase after task event");
}

// ═══════════════════════════════════════════════════════════════════════════════
// P0 Task 3: JSON format cache tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_json_cache_avoids_reformat() {
    let mut state = TuiState::new("test.nika.yaml");

    // Create test data
    let data = serde_json::json!({"key": "value", "nested": {"a": 1}});

    // First call should format and cache
    let key = "test:data";
    let result1 = state.json_cache.get_or_format(key, &data).to_string();
    assert!(result1.contains("key"));
    assert!(result1.contains("value"));

    // Get cache stats
    let (entries, _max) = state.json_cache.stats();
    assert_eq!(entries, 1);

    // Second call should return cached value
    let result2 = state.json_cache.get_or_format(key, &data).to_string();
    assert_eq!(result1, result2);

    let (entries2, _) = state.json_cache.stats();
    assert_eq!(entries2, 1, "Should reuse cached entry, not add new one");
}

#[test]
fn test_json_cache_invalidation_on_task_change() {
    let mut state = TuiState::new("test.nika.yaml");

    // Simulate task output
    let output = serde_json::json!({"result": "success"});
    let key = "task:task1";

    // Cache the output
    let _ = state.json_cache.get_or_format(key, &output);
    let (entries, _) = state.json_cache.stats();
    assert_eq!(entries, 1);

    // Invalidate on task completion (simulating handle_event)
    state.json_cache.invalidate(key);

    let (entries_after, _) = state.json_cache.stats();
    assert_eq!(entries_after, 0, "Cache entry should be removed");
}

// ═══════════════════════════════════════════════════════════════════════════════
// EVENT HANDLER: Happy Path Sequence
// WorkflowStarted -> TaskScheduled -> TaskStarted -> TaskCompleted -> WorkflowCompleted
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_happy_path_full_sequence() {
    let mut state = TuiState::new("test.nika.yaml");

    // 1. WorkflowStarted
    state.handle_event(
        &EventKind::WorkflowStarted {
            task_count: 2,
            generation_id: "gen-happy".to_string(),
            workflow_hash: "hash-happy".to_string(),
            nika_version: TEST_VERSION.to_string(),
        },
        0,
    );
    assert_eq!(state.workflow.phase, MissionPhase::Countdown);
    assert_eq!(state.workflow.task_count, 2);
    assert!(state.workflow.started_at.is_some());
    assert_eq!(state.workflow.generation_id, Some("gen-happy".to_string()));

    // 2. TaskScheduled x2
    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("task-a"),
            dependencies: vec![],
        },
        10,
    );
    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("task-b"),
            dependencies: vec![Arc::from("task-a")],
        },
        10,
    );
    assert_eq!(state.tasks.len(), 2);
    assert_eq!(state.task_order, vec!["task-a", "task-b"]);

    // 3. TaskStarted (first task -> phase goes to Launch)
    state.handle_event(
        &EventKind::TaskStarted {
            task_id: Arc::from("task-a"),
            verb: "infer".into(),
            inputs: Arc::new(serde_json::json!({"prompt": "hello"})),
        },
        100,
    );
    assert_eq!(state.workflow.phase, MissionPhase::Launch);
    assert_eq!(state.current_task, Some("task-a".to_string()));
    assert_eq!(state.tasks["task-a"].status, TaskStatus::Running);
    assert!(state.tasks["task-a"].input.is_some());

    // 4. TaskCompleted (first task)
    state.handle_event(
        &EventKind::TaskCompleted {
            task_id: Arc::from("task-a"),
            output: Arc::new(serde_json::json!({"result": "world"})),
            duration_ms: 500,
        },
        600,
    );
    assert_eq!(state.tasks["task-a"].status, TaskStatus::Success);
    assert_eq!(state.tasks["task-a"].duration_ms, Some(500));
    assert_eq!(state.workflow.tasks_completed, 1);

    // 5. TaskStarted (second task -> phase goes to Orbital)
    state.handle_event(
        &EventKind::TaskStarted {
            task_id: Arc::from("task-b"),
            verb: "exec".into(),
            inputs: Arc::new(serde_json::json!({})),
        },
        700,
    );
    assert_eq!(state.workflow.phase, MissionPhase::Orbital);
    assert_eq!(state.current_task, Some("task-b".to_string()));

    // 6. TaskCompleted (second task)
    state.handle_event(
        &EventKind::TaskCompleted {
            task_id: Arc::from("task-b"),
            output: Arc::new(serde_json::json!({"status": "done"})),
            duration_ms: 200,
        },
        900,
    );
    assert_eq!(state.workflow.tasks_completed, 2);

    // 7. WorkflowCompleted
    let final_output = serde_json::json!({"all": "done"});
    state.handle_event(
        &EventKind::WorkflowCompleted {
            final_output: Arc::new(final_output.clone()),
            total_duration_ms: 900,
        },
        900,
    );
    assert_eq!(state.workflow.phase, MissionPhase::MissionSuccess);
    assert!(state.workflow.final_output.is_some());
    assert_eq!(state.workflow.total_duration_ms, Some(900));
    assert!(state.current_task.is_none());
    assert!(state.is_success());
    assert!(!state.is_failed());
    assert!(!state.is_running());
}

// ═══════════════════════════════════════════════════════════════════════════════
// EVENT HANDLER: Error Path Sequence
// WorkflowStarted -> TaskStarted -> TaskFailed -> WorkflowFailed
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_error_path_full_sequence() {
    let mut state = TuiState::new("test.nika.yaml");

    // WorkflowStarted
    state.handle_event(
        &EventKind::WorkflowStarted {
            task_count: 1,
            generation_id: "gen-err".to_string(),
            workflow_hash: "hash-err".to_string(),
            nika_version: TEST_VERSION.to_string(),
        },
        0,
    );

    // TaskScheduled
    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("failing-task"),
            dependencies: vec![],
        },
        10,
    );

    // TaskStarted
    state.handle_event(
        &EventKind::TaskStarted {
            task_id: Arc::from("failing-task"),
            verb: "infer".into(),
            inputs: Arc::new(serde_json::json!({})),
        },
        100,
    );
    assert_eq!(state.workflow.phase, MissionPhase::Launch);

    // TaskFailed
    state.dirty.clear();
    state.handle_event(
        &EventKind::TaskFailed {
            task_id: Arc::from("failing-task"),
            error: "provider timeout".to_string(),
            duration_ms: 30_000,
            error_code: None,
        },
        30_100,
    );
    assert_eq!(state.tasks["failing-task"].status, TaskStatus::Failed);
    assert_eq!(
        state.tasks["failing-task"].error,
        Some("provider timeout".to_string())
    );
    assert_eq!(state.tasks["failing-task"].duration_ms, Some(30_000));
    // Dirty flags for TaskFailed
    assert!(state.dirty.progress);
    assert!(state.dirty.dag);
    assert!(state.dirty.status);

    // WorkflowFailed
    state.dirty.clear();
    state.handle_event(
        &EventKind::WorkflowFailed {
            error: "Task 'failing-task' failed: provider timeout".to_string(),
            failed_task: Some(Arc::from("failing-task")),
        },
        30_100,
    );
    assert_eq!(state.workflow.phase, MissionPhase::Abort);
    assert!(state.workflow.error_message.is_some());
    assert!(state.is_failed());
    assert!(!state.is_success());
    assert!(!state.is_running());
    // Dirty flags for WorkflowFailed
    assert!(state.dirty.progress);
    assert!(state.dirty.status);
    assert!(state.dirty.notifications);
}

// ═══════════════════════════════════════════════════════════════════════════════
// EVENT HANDLER: WorkflowFailed kills orphaned Running tasks
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_workflow_failed_kills_orphaned_running_tasks() {
    let mut state = TuiState::new("test.nika.yaml");

    // Start workflow with two tasks
    state.handle_event(
        &EventKind::WorkflowStarted {
            task_count: 2,
            generation_id: "gen-orphan".to_string(),
            workflow_hash: "hash-orphan".to_string(),
            nika_version: TEST_VERSION.to_string(),
        },
        0,
    );

    // Schedule and start "task-a" (this one will fail explicitly)
    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("task-a"),
            dependencies: vec![],
        },
        10,
    );
    state.handle_event(
        &EventKind::TaskStarted {
            task_id: Arc::from("task-a"),
            verb: "infer".into(),
            inputs: Arc::new(serde_json::json!({})),
        },
        20,
    );

    // Schedule and start "orphan-task" — it never receives TaskFailed
    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("orphan-task"),
            dependencies: vec![],
        },
        30,
    );
    state.handle_event(
        &EventKind::TaskStarted {
            task_id: Arc::from("orphan-task"),
            verb: "exec".into(),
            inputs: Arc::new(serde_json::json!({})),
        },
        40,
    );
    assert_eq!(state.tasks["orphan-task"].status, TaskStatus::Running);

    // Fail task-a
    state.handle_event(
        &EventKind::TaskFailed {
            task_id: Arc::from("task-a"),
            error: "connection reset".to_string(),
            duration_ms: 1000,
            error_code: None,
        },
        1000,
    );
    // orphan-task is still Running at this point
    assert_eq!(state.tasks["orphan-task"].status, TaskStatus::Running);

    // WorkflowFailed — should kill orphan-task without it ever receiving TaskFailed
    let workflow_error = "Task 'task-a' failed: connection reset";
    state.handle_event(
        &EventKind::WorkflowFailed {
            error: workflow_error.to_string(),
            failed_task: Some(Arc::from("task-a")),
        },
        1100,
    );

    // orphan-task must transition to Failed, not remain Running
    assert_eq!(state.tasks["orphan-task"].status, TaskStatus::Failed);
    assert_eq!(
        state.tasks["orphan-task"].error,
        Some(workflow_error.to_string())
    );
    // Overall workflow state
    assert_eq!(state.workflow.phase, MissionPhase::Abort);
}

// ═══════════════════════════════════════════════════════════════════════════════
// EVENT HANDLER: MCP Lifecycle with Phase Transitions
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_mcp_lifecycle_phase_transitions() {
    let mut state = TuiState::new("test.nika.yaml");

    // Start workflow and task
    state.handle_event(
        &EventKind::WorkflowStarted {
            task_count: 1,
            generation_id: "gen-mcp".to_string(),
            workflow_hash: "hash-mcp".to_string(),
            nika_version: TEST_VERSION.to_string(),
        },
        0,
    );
    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("invoke-task"),
            dependencies: vec![],
        },
        10,
    );
    state.handle_event(
        &EventKind::TaskStarted {
            task_id: Arc::from("invoke-task"),
            verb: "invoke".into(),
            inputs: Arc::new(serde_json::json!({})),
        },
        100,
    );

    // MCP invoke -> phase changes to Rendezvous
    state.dirty.clear();
    state.handle_event(
        &EventKind::McpInvoke {
            task_id: Arc::from("invoke-task"),
            call_id: "mcp-1".to_string(),
            mcp_server: "novanet".to_string(),
            tool: Some("novanet_describe".to_string()),
            resource: None,
            params: Some(Arc::new(serde_json::json!({"entity": "qr-code"}))),
        },
        200,
    );
    assert_eq!(state.workflow.phase, MissionPhase::Rendezvous);
    assert_eq!(state.mcp.calls.len(), 1);
    assert_eq!(state.mcp.seq, 1);
    assert!(state.dirty.novanet);
    // Verify MCP metric tracking
    assert_eq!(*state.metrics.mcp_calls.get("novanet_describe").unwrap(), 1);

    // MCP response -> phase returns to Orbital
    state.dirty.clear();
    state.handle_event(
        &EventKind::McpResponse {
            task_id: Arc::from("invoke-task"),
            call_id: "mcp-1".to_string(),
            output_len: 512,
            duration_ms: 150,
            cached: false,
            is_error: false,
            response: Some(Arc::new(serde_json::json!({"name": "QR Code"}))),
        },
        350,
    );
    assert_eq!(state.workflow.phase, MissionPhase::Orbital);
    assert!(state.mcp.calls[0].completed);
    assert_eq!(state.mcp.calls[0].duration_ms, Some(150));
    assert!(state.dirty.novanet);
    assert_eq!(state.metrics.mcp_cache_misses, 1);
    assert_eq!(state.metrics.mcp_cache_hits, 0);
    // Latency history updated
    assert_eq!(state.metrics.latency_history.len(), 1);
    assert_eq!(state.metrics.latency_history[0], 150);
}

#[test]
fn test_mcp_response_does_not_overwrite_pause_phase() {
    let mut state = TuiState::new("test.nika.yaml");
    state.workflow.phase = MissionPhase::Pause;
    state.workflow.paused = true;

    // Push a pending MCP call so the handler can find it
    state.mcp.add_call(crate::state::types::McpCall {
        call_id: "c1".to_string(),
        seq: 0,
        server: "s".to_string(),
        tool: Some("t".to_string()),
        resource: None,
        task_id: "tid".to_string(),
        completed: false,
        output_len: None,
        timestamp_ms: 0,
        params: None,
        response: None,
        is_error: false,
        duration_ms: None,
    });

    state.handle_event(
        &EventKind::McpResponse {
            task_id: "tid".into(),
            call_id: "c1".to_string(),
            output_len: 42,
            duration_ms: 100,
            cached: false,
            is_error: false,
            response: None,
        },
        1,
    );

    assert_eq!(
        state.workflow.phase,
        MissionPhase::Pause,
        "Pause phase must not be overwritten by a late MCP response"
    );
}

#[test]
fn test_mcp_invoke_does_not_overwrite_pause_phase() {
    let mut state = TuiState::new("test.nika.yaml");
    state.workflow.phase = MissionPhase::Pause;
    state.workflow.paused = true;

    state.handle_event(
        &EventKind::McpInvoke {
            task_id: "tid".into(),
            mcp_server: "novanet".to_string(),
            tool: Some("tool".to_string()),
            resource: None,
            call_id: "c1".to_string(),
            params: None,
        },
        1,
    );

    assert_eq!(
        state.workflow.phase,
        MissionPhase::Pause,
        "Pause phase must not be overwritten by MCP invoke"
    );
}

#[test]
fn test_mcp_invoke_does_not_overwrite_abort_phase() {
    let mut state = TuiState::new("test.nika.yaml");
    state.workflow.phase = MissionPhase::Abort;

    state.handle_event(
        &EventKind::McpInvoke {
            task_id: "tid".into(),
            mcp_server: "novanet".to_string(),
            tool: Some("tool".to_string()),
            resource: None,
            call_id: "c2".to_string(),
            params: None,
        },
        1,
    );

    assert_eq!(
        state.workflow.phase,
        MissionPhase::Abort,
        "Abort phase must not be overwritten by MCP invoke"
    );
}

#[test]
fn test_mcp_invoke_does_not_overwrite_mission_success_phase() {
    let mut state = TuiState::new("test.nika.yaml");
    state.workflow.phase = MissionPhase::MissionSuccess;

    state.handle_event(
        &EventKind::McpInvoke {
            task_id: "tid".into(),
            mcp_server: "novanet".to_string(),
            tool: Some("tool".to_string()),
            resource: None,
            call_id: "c3".to_string(),
            params: None,
        },
        1,
    );

    assert_eq!(
        state.workflow.phase,
        MissionPhase::MissionSuccess,
        "MissionSuccess phase must not be overwritten by MCP invoke"
    );
}

#[test]
fn test_task_started_does_not_overwrite_pause_phase() {
    let mut state = TuiState::new("test.nika.yaml");
    state.workflow.phase = MissionPhase::Pause;
    state.workflow.paused = true;

    state.handle_event(
        &EventKind::TaskStarted {
            task_id: "late-task".into(),
            verb: "infer".into(),
            inputs: Arc::new(serde_json::json!({})),
        },
        1,
    );

    assert_eq!(
        state.workflow.phase,
        MissionPhase::Pause,
        "Pause phase must not be overwritten by late TaskStarted"
    );
}

#[test]
fn test_task_started_does_not_overwrite_abort_phase() {
    let mut state = TuiState::new("test.nika.yaml");
    state.workflow.phase = MissionPhase::Abort;

    state.handle_event(
        &EventKind::TaskStarted {
            task_id: "late-task".into(),
            verb: "infer".into(),
            inputs: Arc::new(serde_json::json!({})),
        },
        1,
    );

    assert_eq!(
        state.workflow.phase,
        MissionPhase::Abort,
        "Abort phase must not be overwritten by late TaskStarted"
    );
}

#[test]
fn test_mcp_cached_response_tracks_hit() {
    let mut state = TuiState::new("test.nika.yaml");

    state.handle_event(
        &EventKind::McpInvoke {
            task_id: Arc::from("task1"),
            call_id: "cached-call".to_string(),
            mcp_server: "novanet".to_string(),
            tool: Some("novanet_describe".to_string()),
            resource: None,
            params: None,
        },
        100,
    );
    state.handle_event(
        &EventKind::McpResponse {
            task_id: Arc::from("task1"),
            call_id: "cached-call".to_string(),
            output_len: 256,
            duration_ms: 5,
            cached: true,
            is_error: false,
            response: None,
        },
        105,
    );

    assert_eq!(state.metrics.mcp_cache_hits, 1);
    assert_eq!(state.metrics.mcp_cache_misses, 0);
}

// ═══════════════════════════════════════════════════════════════════════════════
// EVENT HANDLER: WorkflowAborted
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_workflow_aborted_event() {
    let mut state = TuiState::new("test.nika.yaml");

    state.handle_event(
        &EventKind::WorkflowStarted {
            task_count: 3,
            generation_id: "gen-abort".to_string(),
            workflow_hash: "hash-abort".to_string(),
            nika_version: TEST_VERSION.to_string(),
        },
        0,
    );

    // Put "running-task" into Running state so the abort loop has something to mark
    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("running-task"),
            dependencies: vec![],
        },
        50,
    );
    state.handle_event(
        &EventKind::TaskStarted {
            task_id: Arc::from("running-task"),
            verb: "infer".into(),
            inputs: Arc::new(serde_json::json!({})),
        },
        100,
    );
    assert_eq!(state.tasks["running-task"].status, TaskStatus::Running);

    state.dirty.clear();
    state.handle_event(
        &EventKind::WorkflowAborted {
            reason: "User cancelled".to_string(),
            duration_ms: 5000,
            running_tasks: vec![Arc::from("running-task")],
        },
        5000,
    );

    assert_eq!(state.workflow.phase, MissionPhase::Abort);
    assert!(state
        .workflow
        .error_message
        .as_ref()
        .unwrap()
        .contains("Aborted"));
    assert_eq!(state.workflow.total_duration_ms, Some(5000));
    assert!(state.current_task.is_none());
    assert!(state.dirty.progress);
    assert!(state.dirty.status);
    assert!(state.dirty.notifications);
    // Notification mentions interrupted tasks
    assert!(state
        .notifs
        .items
        .back()
        .unwrap()
        .message
        .contains("1 tasks interrupted"));
    // Running task must be marked Skipped so it stops spinning in the TUI
    assert_eq!(state.tasks["running-task"].status, TaskStatus::Skipped);
}

#[test]
fn test_workflow_aborted_no_running_tasks() {
    let mut state = TuiState::new("test.nika.yaml");

    state.handle_event(
        &EventKind::WorkflowAborted {
            reason: "Timeout".to_string(),
            duration_ms: 60_000,
            running_tasks: vec![],
        },
        60_000,
    );

    // Notification should NOT mention interrupted tasks
    let msg = &state.notifs.items.back().unwrap().message;
    assert!(!msg.contains("interrupted"));
    assert!(msg.contains("Timeout"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// EVENT HANDLER: Pause/Resume
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_workflow_paused_resumed_events() {
    let mut state = TuiState::new("test.nika.yaml");

    // Set up a running workflow
    state.workflow.phase = MissionPhase::Orbital;
    state.current_task = Some("active-task".to_string());

    // Pause
    state.dirty.clear();
    state.handle_event(&EventKind::WorkflowPaused, 1000);

    assert!(state.workflow.paused);
    assert_eq!(state.workflow.phase, MissionPhase::Pause);
    assert_eq!(
        state.workflow.phase_before_pause,
        Some(MissionPhase::Orbital)
    );
    assert!(state.dirty.progress);
    assert!(state.dirty.status);

    // Resume
    state.dirty.clear();
    state.handle_event(&EventKind::WorkflowResumed, 2000);

    assert!(!state.workflow.paused);
    assert_eq!(state.workflow.phase, MissionPhase::Orbital);
    assert!(state.workflow.phase_before_pause.is_none());
    assert!(state.dirty.progress);
    assert!(state.dirty.status);
}

#[test]
fn test_workflow_resumed_without_saved_phase_infers_from_state() {
    let mut state = TuiState::new("test.nika.yaml");

    // Pause without saved phase (edge case)
    state.workflow.paused = true;
    state.workflow.phase = MissionPhase::Pause;
    state.workflow.phase_before_pause = None;

    // Resume with current_task set -> Orbital
    state.current_task = Some("task1".to_string());
    state.handle_event(&EventKind::WorkflowResumed, 1000);
    assert_eq!(state.workflow.phase, MissionPhase::Orbital);

    // Resume without current_task -> Countdown
    state.workflow.paused = true;
    state.workflow.phase = MissionPhase::Pause;
    state.workflow.phase_before_pause = None;
    state.current_task = None;
    state.handle_event(&EventKind::WorkflowResumed, 2000);
    assert_eq!(state.workflow.phase, MissionPhase::Countdown);
}

// ═══════════════════════════════════════════════════════════════════════════════
// EVENT HANDLER: TaskSkipped
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_task_skipped_event() {
    let mut state = TuiState::new("test.nika.yaml");

    state.tasks.insert(
        "dep-task".to_string(),
        TaskState::new("dep-task".to_string(), vec![]),
    );
    state.tasks.insert(
        "skip-task".to_string(),
        TaskState::new("skip-task".to_string(), vec!["dep-task".to_string()]),
    );

    state.dirty.clear();
    state.handle_event(
        &EventKind::TaskSkipped {
            task_id: Arc::from("skip-task"),
            reason: "dependency 'dep-task' failed".to_string(),
        },
        500,
    );

    assert_eq!(state.tasks["skip-task"].status, TaskStatus::Skipped);
    assert!(state.tasks["skip-task"]
        .error
        .as_ref()
        .unwrap()
        .contains("skipped"));
    assert!(state.dirty.progress);
    assert!(state.dirty.dag);
    assert!(state.dirty.status);
}

// ═══════════════════════════════════════════════════════════════════════════════
// EVENT HANDLER: Phase Transitions (Countdown -> Launch -> Orbital)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_phase_transition_countdown_to_launch() {
    let mut state = TuiState::new("test.nika.yaml");

    state.handle_event(
        &EventKind::WorkflowStarted {
            task_count: 2,
            generation_id: "gen-phase".to_string(),
            workflow_hash: "hash-phase".to_string(),
            nika_version: TEST_VERSION.to_string(),
        },
        0,
    );
    assert_eq!(state.workflow.phase, MissionPhase::Countdown);

    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("t1"),
            dependencies: vec![],
        },
        5,
    );

    // First task start -> Launch
    state.handle_event(
        &EventKind::TaskStarted {
            task_id: Arc::from("t1"),
            verb: "infer".into(),
            inputs: Arc::new(serde_json::json!({})),
        },
        10,
    );
    assert_eq!(state.workflow.phase, MissionPhase::Launch);
}

#[test]
fn test_phase_transition_launch_to_orbital() {
    let mut state = TuiState::new("test.nika.yaml");

    // Skip past Countdown
    state.workflow.phase = MissionPhase::Launch;

    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("t2"),
            dependencies: vec![],
        },
        0,
    );

    // Subsequent task start -> Orbital (not Launch again)
    state.handle_event(
        &EventKind::TaskStarted {
            task_id: Arc::from("t2"),
            verb: "exec".into(),
            inputs: Arc::new(serde_json::json!({})),
        },
        100,
    );
    assert_eq!(state.workflow.phase, MissionPhase::Orbital);
}

// ═══════════════════════════════════════════════════════════════════════════════
// EVENT HANDLER: Edge Cases - Duplicate Events & Unknown Task IDs
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_duplicate_task_scheduled_overwrites() {
    let mut state = TuiState::new("test.nika.yaml");

    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("dup-task"),
            dependencies: vec![],
        },
        0,
    );
    assert_eq!(state.tasks.len(), 1);
    assert_eq!(state.task_order.len(), 1);

    // Duplicate schedule (should overwrite task, append to order)
    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("dup-task"),
            dependencies: vec![Arc::from("dep1")],
        },
        10,
    );
    // HashMap overwrites; task_order deduplicates (no ghost entry on retry)
    assert_eq!(state.tasks.len(), 1);
    assert_eq!(state.tasks["dup-task"].dependencies, vec!["dep1"]);
    assert_eq!(state.task_order.len(), 1); // Deduped — still one entry
}

#[test]
fn test_task_started_unknown_task_id_no_panic() {
    let mut state = TuiState::new("test.nika.yaml");

    // TaskStarted for a task that was never scheduled
    state.handle_event(
        &EventKind::TaskStarted {
            task_id: Arc::from("ghost-task"),
            verb: "infer".into(),
            inputs: Arc::new(serde_json::json!({})),
        },
        100,
    );

    // Should not panic; current_task is set even if task not in map
    assert_eq!(state.current_task, Some("ghost-task".to_string()));
    assert!(!state.tasks.contains_key("ghost-task"));
}

#[test]
fn test_task_completed_unknown_task_id_no_panic() {
    let mut state = TuiState::new("test.nika.yaml");

    // TaskCompleted for a task that was never scheduled
    state.handle_event(
        &EventKind::TaskCompleted {
            task_id: Arc::from("ghost-task"),
            output: Arc::new(serde_json::json!(null)),
            duration_ms: 100,
        },
        200,
    );

    // Should not panic; tasks_completed still increments
    assert_eq!(state.workflow.tasks_completed, 1);
}

#[test]
fn test_task_failed_unknown_task_id_no_panic() {
    let mut state = TuiState::new("test.nika.yaml");

    // TaskFailed for a task that was never scheduled
    state.handle_event(
        &EventKind::TaskFailed {
            task_id: Arc::from("ghost-task"),
            error: "unknown error".to_string(),
            duration_ms: 50,
            error_code: None,
        },
        100,
    );

    // Should not panic
    assert!(!state.tasks.contains_key("ghost-task"));
    assert!(state.dirty.status);
}

#[test]
fn test_mcp_response_for_unknown_call_id_no_panic() {
    let mut state = TuiState::new("test.nika.yaml");

    // MCP response without a matching invoke
    state.handle_event(
        &EventKind::McpResponse {
            task_id: Arc::from("task1"),
            call_id: "nonexistent-call".to_string(),
            output_len: 100,
            duration_ms: 50,
            cached: false,
            is_error: false,
            response: None,
        },
        100,
    );

    // Should not panic; no call updated but metrics still track
    assert_eq!(state.metrics.mcp_cache_misses, 1);
    assert_eq!(state.metrics.latency_history.len(), 1);
}

// ═══════════════════════════════════════════════════════════════════════════════
// EVENT HANDLER: Provider Events
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_provider_called_updates_task_and_metrics() {
    let mut state = TuiState::new("test.nika.yaml");

    state.tasks.insert(
        "infer-task".to_string(),
        TaskState::new("infer-task".to_string(), vec![]),
    );

    state.dirty.clear();
    state.handle_event(
        &EventKind::ProviderCalled {
            task_id: Arc::from("infer-task"),
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            prompt_len: 1500,
            endpoint_url: None,
        },
        200,
    );

    assert_eq!(
        state.tasks["infer-task"].provider,
        Some("anthropic".to_string())
    );
    assert_eq!(
        state.tasks["infer-task"].model,
        Some("claude-sonnet-4-6".to_string())
    );
    assert_eq!(state.tasks["infer-task"].prompt_len, Some(1500));
    assert_eq!(state.metrics.provider_calls, 1);
    assert_eq!(
        state.metrics.last_model,
        Some("claude-sonnet-4-6".to_string())
    );
    assert!(state.dirty.progress);
}

#[test]
fn test_provider_responded_updates_metrics() {
    let mut state = TuiState::new("test.nika.yaml");

    state.tasks.insert(
        "infer-task".to_string(),
        TaskState::new("infer-task".to_string(), vec![]),
    );

    state.dirty.clear();
    state.handle_event(
        &EventKind::ProviderResponded {
            task_id: Arc::from("infer-task"),
            request_id: Some("req-123".to_string()),
            input_tokens: 1000,
            output_tokens: 500,
            cache_read_tokens: 200,
            ttft_ms: Some(250),
            finish_reason: FinishReason::Stop,
            cost_usd: 0.005,
        },
        500,
    );

    assert_eq!(state.metrics.input_tokens, 1000);
    assert_eq!(state.metrics.output_tokens, 500);
    assert_eq!(state.metrics.cache_read_tokens, 200);
    assert_eq!(state.metrics.total_tokens, 1500);
    assert!((state.metrics.cost_usd - 0.005).abs() < f64::EPSILON);
    assert_eq!(
        state.tasks["infer-task"].finish_reason,
        Some("stop".to_string())
    );
    // TTFT tracked in latency history
    assert_eq!(state.metrics.latency_history.len(), 1);
    assert_eq!(state.metrics.latency_history[0], 250);
    // Token velocity tracked
    assert!(!state.metrics.token_velocity.is_empty());
    assert!(state.dirty.progress);
}

// ═══════════════════════════════════════════════════════════════════════════════
// EVENT HANDLER: Agent Events
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_agent_spawned_event() {
    let mut state = TuiState::new("test.nika.yaml");

    state.dirty.clear();
    state.handle_event(
        &EventKind::AgentSpawned {
            parent_task_id: Arc::from("parent-agent"),
            child_task_id: Arc::from("child-agent"),
            depth: 1,
        },
        500,
    );

    assert_eq!(state.agent.spawned_agents.len(), 1);
    assert_eq!(state.agent.spawned_agents[0].parent_task_id, "parent-agent");
    assert_eq!(state.agent.spawned_agents[0].child_task_id, "child-agent");
    assert_eq!(state.agent.spawned_agents[0].depth, 1);
    assert!(state.dirty.reasoning);
    assert!(state.dirty.notifications);
    // Notification mentions the spawn
    assert!(state
        .notifs
        .items
        .back()
        .unwrap()
        .message
        .contains("child-agent"));
}

#[test]
fn test_is_subagent_and_agent_icon() {
    let mut state = TuiState::new("test.nika.yaml");

    // Before spawning, not a subagent
    assert!(!state.is_subagent("child-agent"));
    assert_eq!(state.agent_icon("child-agent"), ">>");

    // After spawning
    state.handle_event(
        &EventKind::AgentSpawned {
            parent_task_id: Arc::from("parent"),
            child_task_id: Arc::from("child-agent"),
            depth: 1,
        },
        100,
    );

    assert!(state.is_subagent("child-agent"));
    assert!(!state.is_subagent("parent"));
    assert_eq!(state.agent_icon("child-agent"), ">");
    assert_eq!(state.agent_icon("parent"), ">>");
}

#[test]
fn test_context_assembled_updates_mcp_state() {
    use nika_engine::event::{ContextSource, ExcludedItem};

    let mut state = TuiState::new("test.nika.yaml");
    state.dirty.clear();

    state.handle_event(
        &EventKind::ContextAssembled {
            task_id: "t1".into(),
            sources: vec![
                ContextSource {
                    node: "system_prompt".into(),
                    tokens: 200,
                },
                ContextSource {
                    node: "entity-data".into(),
                    tokens: 350,
                },
            ],
            excluded: vec![ExcludedItem {
                node: "large-doc".into(),
                reason: "over budget".into(),
            }],
            total_tokens: 550,
            budget_used_pct: 55.0,
            truncated: false,
        },
        100,
    );

    assert_eq!(state.mcp.context_assembly.sources.len(), 2);
    assert_eq!(state.mcp.context_assembly.excluded.len(), 1);
    assert_eq!(state.mcp.context_assembly.total_tokens, 550);
    assert!((state.mcp.context_assembly.budget_used_pct - 55.0).abs() < f32::EPSILON);
    assert!(!state.mcp.context_assembly.truncated);
    assert!(state.dirty.novanet, "novanet panel must be marked dirty");
}

#[test]
fn test_template_resolved_tracks_resolutions() {
    let mut state = TuiState::new("test.nika.yaml");
    state.dirty.clear();

    state.handle_event(
        &EventKind::TemplateResolved {
            task_id: "t1".into(),
            template: "{{with.data}}".into(),
            result: "hello world".into(),
        },
        200,
    );

    assert_eq!(state.agent.recent_templates.len(), 1);
    assert_eq!(state.agent.recent_templates[0].template, "{{with.data}}");
    assert_eq!(state.agent.recent_templates[0].result, "hello world");
    assert_eq!(state.agent.recent_templates[0].task_id, "t1");
    assert!(state.dirty.novanet, "novanet panel must be marked dirty");
}

#[test]
fn test_template_resolved_caps_at_10() {
    let mut state = TuiState::new("test.nika.yaml");

    for i in 0..15 {
        state.handle_event(
            &EventKind::TemplateResolved {
                task_id: "t1".into(),
                template: format!("tmpl_{}", i),
                result: format!("res_{}", i),
            },
            i as u64,
        );
    }

    assert_eq!(
        state.agent.recent_templates.len(),
        10,
        "recent_templates must cap at 10"
    );
    // Oldest should be evicted
    assert_eq!(state.agent.recent_templates[0].template, "tmpl_5");
}

#[test]
fn test_agent_task_completed_clears_agent_state() {
    let mut state = TuiState::new("test.nika.yaml");

    // Set up agent state
    state.handle_event(
        &EventKind::AgentStart {
            task_id: Arc::from("agent-task"),
            max_turns: 5,
            mcp_servers: vec![],
        },
        100,
    );
    state.handle_event(
        &EventKind::AgentTurn {
            task_id: Arc::from("agent-task"),
            turn_index: 0,
            kind: AgentTurnKind::Continue,
            metadata: None,
        },
        200,
    );
    assert_eq!(state.agent.turns.len(), 1);
    assert_eq!(state.agent.max_turns, Some(5));

    // Set up the task as an agent type
    state.tasks.insert(
        "agent-task".to_string(),
        TaskState {
            id: "agent-task".to_string(),
            task_type: Some("agent".to_string()),
            status: TaskStatus::Running,
            dependencies: vec![],
            started_at: None,
            duration_ms: None,
            input: None,
            output: None,
            error: None,
            tokens: None,
            provider: None,
            model: None,
            prompt_len: None,
            finish_reason: None,
        },
    );

    // Complete agent task -> clears agent state
    state.handle_event(
        &EventKind::TaskCompleted {
            task_id: Arc::from("agent-task"),
            output: Arc::new(serde_json::json!({"answer": "42"})),
            duration_ms: 5000,
        },
        5100,
    );

    assert!(state.agent.turns.is_empty());
    assert!(state.agent.max_turns.is_none());
}

#[test]
fn test_non_agent_task_completed_does_not_clear_agent_state() {
    let mut state = TuiState::new("test.nika.yaml");

    // Set up agent state (simulating parallel workflow)
    state.agent.max_turns = Some(5);
    state.agent.turns.push(AgentTurnState {
        index: 0,
        status: "thinking".to_string(),
        tokens: None,
        tool_calls: vec![],
        thinking: None,
        response_text: None,
    });

    // Complete a non-agent task
    state.tasks.insert(
        "infer-task".to_string(),
        TaskState {
            id: "infer-task".to_string(),
            task_type: Some("infer".to_string()),
            status: TaskStatus::Running,
            dependencies: vec![],
            started_at: None,
            duration_ms: None,
            input: None,
            output: None,
            error: None,
            tokens: None,
            provider: None,
            model: None,
            prompt_len: None,
            finish_reason: None,
        },
    );

    state.handle_event(
        &EventKind::TaskCompleted {
            task_id: Arc::from("infer-task"),
            output: Arc::new(serde_json::json!({})),
            duration_ms: 100,
        },
        200,
    );

    // Agent state should NOT be cleared
    assert_eq!(state.agent.turns.len(), 1);
    assert_eq!(state.agent.max_turns, Some(5));
}

// ═══════════════════════════════════════════════════════════════════════════════
// EVENT HANDLER: MCP Connection Events
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_mcp_connected_event() {
    let mut state = TuiState::new("test.nika.yaml");

    state.dirty.clear();
    state.handle_event(
        &EventKind::McpConnected {
            server_name: "novanet".to_string(),
        },
        100,
    );

    assert!(state.dirty.status);
    assert_eq!(
        state.notifs.items.back().unwrap().level,
        NotificationLevel::Success
    );
    assert!(state
        .notifs
        .items
        .back()
        .unwrap()
        .message
        .contains("novanet"));
}

#[test]
fn test_mcp_error_event() {
    let mut state = TuiState::new("test.nika.yaml");

    state.dirty.clear();
    state.handle_event(
        &EventKind::McpError {
            server_name: "novanet".to_string(),
            error: "connection refused".to_string(),
        },
        100,
    );

    assert!(state.dirty.status);
    assert_eq!(
        state.notifs.items.back().unwrap().level,
        NotificationLevel::Error
    );
    assert!(state
        .notifs
        .items
        .back()
        .unwrap()
        .message
        .contains("connection refused"));
}

#[test]
fn test_mcp_retry_event() {
    let mut state = TuiState::new("test.nika.yaml");

    state.dirty.clear();
    state.handle_event(
        &EventKind::McpRetry {
            task_id: Arc::from("task1"),
            server_name: "novanet".to_string(),
            operation: "novanet_describe".to_string(),
            attempt: 2,
            max_attempts: 3,
            error: "timeout".to_string(),
        },
        200,
    );

    assert!(state.dirty.status);
    let msg = &state.notifs.items.back().unwrap().message;
    assert!(msg.contains("2/3"));
    assert!(msg.contains("novanet_describe"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// EVENT HANDLER: Slow MCP Response Notification
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_slow_mcp_response_adds_warning() {
    let mut state = TuiState::new("test.nika.yaml");

    // First invoke
    state.handle_event(
        &EventKind::McpInvoke {
            task_id: Arc::from("task1"),
            call_id: "slow-call".to_string(),
            mcp_server: "novanet".to_string(),
            tool: Some("slow_tool".to_string()),
            resource: None,
            params: None,
        },
        100,
    );

    let notifs_before = state.notifs.items.len();

    // Slow response (> 5s)
    state.handle_event(
        &EventKind::McpResponse {
            task_id: Arc::from("task1"),
            call_id: "slow-call".to_string(),
            output_len: 100,
            duration_ms: 6_000,
            cached: false,
            is_error: false,
            response: None,
        },
        6_100,
    );

    assert!(state.notifs.items.len() > notifs_before);
    let last = state.notifs.items.back().unwrap();
    assert_eq!(last.level, NotificationLevel::Warning);
    assert!(last.message.contains("slow_tool"));
}

// ═══════════════════════════════════════════════════════════════════════════════
/// EVENT HANDLER: Latency History Cap (max 50)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_latency_history_capped_at_50_via_mcp() {
    let mut state = TuiState::new("test.nika.yaml");

    // Feed 60 MCP response events directly into latency_history
    // The MCP response handler caps at 50 by removing the oldest entry
    for i in 0u64..60 {
        state.handle_event(
            &EventKind::McpInvoke {
                task_id: Arc::from("task1"),
                call_id: format!("call-{}", i),
                mcp_server: "novanet".to_string(),
                tool: Some("tool".to_string()),
                resource: None,
                params: None,
            },
            i * 100,
        );
        state.handle_event(
            &EventKind::McpResponse {
                task_id: Arc::from("task1"),
                call_id: format!("call-{}", i),
                output_len: 10,
                duration_ms: (i + 1) * 10,
                cached: false,
                is_error: false,
                response: None,
            },
            i * 100 + 50,
        );
    }

    // MCP response handler keeps last 50 values
    assert_eq!(
        state.metrics.latency_history.len(),
        50,
        "MCP latency history should cap at 50 entries"
    );
    // Oldest entries evicted: first remaining should be entry 11 (= (10+1)*10 = 110)
    assert_eq!(state.metrics.latency_history[0], 110);
    // Last entry should be entry 60 (= (59+1)*10 = 600)
    assert_eq!(state.metrics.latency_history[49], 600);
}

// ═══════════════════════════════════════════════════════════════════════════════
// EVENT HANDLER: Animation helpers
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_spinner_char_cycles() {
    let mut state = TuiState::new("test.nika.yaml");

    // Spinner should return a valid braille character
    let c = state.spinner_char();
    assert!(matches!(
        c,
        '\u{280B}'
            | '\u{2819}'
            | '\u{2839}'
            | '\u{2838}'
            | '\u{283C}'
            | '\u{2834}'
            | '\u{2826}'
            | '\u{2827}'
            | '\u{2807}'
            | '\u{280F}'
    ));

    // Advancing frame should eventually produce different chars
    let first = state.spinner_char();
    state.frame = FRAME_DIV_NORMAL; // Advance past one division
    let second = state.spinner_char();
    assert_ne!(first, second);
}

#[test]
fn test_rocket_char_returns_valid() {
    let state = TuiState::new("test.nika.yaml");
    let c = state.rocket_char();
    // Should be one of the rocket animation chars
    assert!(c == '\u{1F680}' || c == '\u{1F525}' || c == '\u{1F4A8}' || c == '\u{2728}');
}

// ═══════════════════════════════════════════════════════════════════════════════
// NAVIGATION: UTF-8 Cursor Fix Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_filter_push_ascii_cursor_at_1() {
    let mut state = TuiState::new("test.nika.yaml");
    state.filter_push('a');
    assert_eq!(state.filter_query, "a");
    assert_eq!(state.filter_cursor, 1); // ASCII: 1 byte
}

#[test]
fn test_filter_push_accented_char_cursor_at_2() {
    let mut state = TuiState::new("test.nika.yaml");
    state.filter_push('\u{00E9}'); // e-acute
    assert_eq!(state.filter_query, "\u{00E9}");
    assert_eq!(state.filter_cursor, 2); // 2-byte UTF-8
}

#[test]
fn test_filter_push_emoji_cursor_at_4() {
    let mut state = TuiState::new("test.nika.yaml");
    state.filter_push('\u{1F98B}'); // butterfly emoji
    assert_eq!(state.filter_query, "\u{1F98B}");
    assert_eq!(state.filter_cursor, 4); // 4-byte UTF-8
}

#[test]
fn test_filter_push_mixed_utf8() {
    let mut state = TuiState::new("test.nika.yaml");

    state.filter_push('n'); // 1 byte
    assert_eq!(state.filter_cursor, 1);

    state.filter_push('\u{00E9}'); // 2 bytes
    assert_eq!(state.filter_cursor, 3); // 1 + 2

    state.filter_push('\u{1F98B}'); // 4 bytes
    assert_eq!(state.filter_cursor, 7); // 1 + 2 + 4

    assert_eq!(state.filter_query, "n\u{00E9}\u{1F98B}");
    assert_eq!(state.filter_query.len(), 7);
}

#[test]
fn test_filter_backspace_after_emoji() {
    let mut state = TuiState::new("test.nika.yaml");

    state.filter_push('a'); // cursor at 1
    state.filter_push('\u{1F98B}'); // cursor at 5

    assert_eq!(state.filter_cursor, 5);

    state.filter_backspace();
    assert_eq!(state.filter_query, "a");
    assert_eq!(state.filter_cursor, 1); // Back to after 'a'
}

#[test]
fn test_filter_backspace_after_accented() {
    let mut state = TuiState::new("test.nika.yaml");

    state.filter_push('c');
    state.filter_push('a');
    state.filter_push('f');
    state.filter_push('\u{00E9}'); // cafe with accent
    assert_eq!(state.filter_cursor, 5); // 3 + 2

    state.filter_backspace(); // remove e-acute
    assert_eq!(state.filter_query, "caf");
    assert_eq!(state.filter_cursor, 3);
}

#[test]
fn test_filter_cursor_left_through_multibyte() {
    let mut state = TuiState::new("test.nika.yaml");

    state.filter_push('a'); // cursor 1
    state.filter_push('\u{00E9}'); // cursor 3
    state.filter_push('\u{1F98B}'); // cursor 7

    // Move left through emoji (4 bytes)
    state.filter_cursor_left();
    assert_eq!(state.filter_cursor, 3);

    // Move left through accented char (2 bytes)
    state.filter_cursor_left();
    assert_eq!(state.filter_cursor, 1);

    // Move left through ASCII (1 byte)
    state.filter_cursor_left();
    assert_eq!(state.filter_cursor, 0);

    // Can't go further left
    state.filter_cursor_left();
    assert_eq!(state.filter_cursor, 0);
}

#[test]
fn test_filter_cursor_right_through_multibyte() {
    let mut state = TuiState::new("test.nika.yaml");

    state.filter_push('a'); // 1 byte
    state.filter_push('\u{00E9}'); // 2 bytes
    state.filter_push('\u{1F98B}'); // 4 bytes

    // Move to beginning
    state.filter_cursor = 0;

    // Move right through ASCII (1 byte)
    state.filter_cursor_right();
    assert_eq!(state.filter_cursor, 1);

    // Move right through accented char (2 bytes)
    state.filter_cursor_right();
    assert_eq!(state.filter_cursor, 3);

    // Move right through emoji (4 bytes)
    state.filter_cursor_right();
    assert_eq!(state.filter_cursor, 7);

    // Can't go further right
    state.filter_cursor_right();
    assert_eq!(state.filter_cursor, 7);
}

#[test]
fn test_filter_delete_multibyte_at_cursor() {
    let mut state = TuiState::new("test.nika.yaml");

    state.filter_push('a');
    state.filter_push('\u{00E9}');
    state.filter_push('b');
    // "a" + e-acute + "b" -> 4 bytes total

    // Position cursor at the start of e-acute
    state.filter_cursor = 1;
    state.filter_delete(); // Should remove the 2-byte e-acute

    assert_eq!(state.filter_query, "ab");
    assert_eq!(state.filter_cursor, 1); // Cursor stays at 1
}

#[test]
fn test_filter_insert_at_middle_multibyte() {
    let mut state = TuiState::new("test.nika.yaml");

    state.filter_push('a');
    state.filter_push('b');
    assert_eq!(state.filter_query, "ab");

    // Move cursor to position 1 (between a and b)
    state.filter_cursor = 1;
    state.filter_push('\u{00E9}');

    assert_eq!(state.filter_query, "a\u{00E9}b");
    assert_eq!(state.filter_cursor, 3); // 1 + 2 for the inserted char
}

// ═══════════════════════════════════════════════════════════════════════════════
// WORKFLOW OPS: toggle_pause and dirty flags
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_toggle_pause_sets_dirty_flags() {
    let mut state = TuiState::new("test.nika.yaml");
    state.workflow.phase = MissionPhase::Orbital;
    state.current_task = Some("task1".to_string());

    state.dirty.clear();
    state.toggle_pause();

    assert!(state.is_paused());
    assert_eq!(state.workflow.phase, MissionPhase::Pause);
    assert!(state.dirty.progress);
    assert!(state.dirty.status);
}

#[test]
fn test_toggle_pause_twice_restores() {
    let mut state = TuiState::new("test.nika.yaml");
    state.workflow.phase = MissionPhase::Orbital;
    state.current_task = Some("task1".to_string());

    // First toggle -> paused
    state.toggle_pause();
    assert!(state.is_paused());
    assert_eq!(state.workflow.phase, MissionPhase::Pause);

    // Second toggle -> unpaused, Orbital because current_task is set
    state.dirty.clear();
    state.toggle_pause();
    assert!(!state.is_paused());
    assert_eq!(state.workflow.phase, MissionPhase::Orbital);
    assert!(state.dirty.progress);
    assert!(state.dirty.status);
}

#[test]
fn test_toggle_pause_resume_restores_saved_phase() {
    let mut state = TuiState::new("test.nika.yaml");
    state.workflow.phase = MissionPhase::Orbital;
    state.current_task = None;

    state.toggle_pause();
    assert!(state.is_paused());

    // Resume restores saved phase (Orbital), regardless of current_task
    state.toggle_pause();
    assert!(!state.is_paused());
    assert_eq!(state.workflow.phase, MissionPhase::Orbital);
}

#[test]
fn test_toggle_pause_resume_fallback_no_saved_phase() {
    let mut state = TuiState::new("test.nika.yaml");
    // Manually force paused without saving phase_before_pause
    state.workflow.paused = true;
    state.workflow.phase = MissionPhase::Pause;
    state.workflow.phase_before_pause = None;
    state.current_task = None;

    // Resume should fall back to Countdown heuristic
    state.toggle_pause();
    assert!(!state.is_paused());
    assert_eq!(state.workflow.phase, MissionPhase::Countdown);
}

#[test]
fn test_is_paused_reflects_workflow_state() {
    let mut state = TuiState::new("test.nika.yaml");
    assert!(!state.is_paused());

    state.workflow.paused = true;
    assert!(state.is_paused());

    state.workflow.paused = false;
    assert!(!state.is_paused());
}

#[test]
fn test_toggle_pause_saves_and_restores_phase_before_pause() {
    let mut state = TuiState::new("test.nika.yaml");
    // Simulate a workflow in Rendezvous phase (active MCP call)
    state.workflow.phase = MissionPhase::Rendezvous;

    // First toggle: pause
    state.toggle_pause();
    assert_eq!(state.workflow.phase, MissionPhase::Pause);
    assert!(state.workflow.paused);

    // Second toggle: resume — must restore Rendezvous, not guess Orbital/Countdown
    state.toggle_pause();
    assert!(!state.workflow.paused);
    assert_eq!(
        state.workflow.phase,
        MissionPhase::Rendezvous,
        "resume must restore saved phase, not heuristic guess"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// WORKFLOW OPS: reset_for_retry sets dirty via mark_all
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_reset_for_retry_marks_all_dirty() {
    let mut state = TuiState::new("test.nika.yaml");
    state.workflow.phase = MissionPhase::Abort;

    state.dirty.clear();
    assert!(!state.dirty.any());

    state.reset_for_retry();

    assert!(state.dirty.all, "reset_for_retry should mark_all dirty");
    assert!(state.dirty.any());
}

#[test]
fn test_reset_for_retry_clears_agent_and_metrics() {
    let mut state = TuiState::new("test.nika.yaml");

    // Populate some state
    state.agent.turns.push(AgentTurnState {
        index: 0,
        status: "done".to_string(),
        tokens: Some(1000),
        tool_calls: vec![],
        thinking: None,
        response_text: None,
    });
    state.metrics.total_tokens = 5000;
    state.metrics.provider_calls = 3;
    state.current_task = Some("task1".to_string());
    state.mcp.seq = 5;

    state.reset_for_retry();

    assert!(state.agent.turns.is_empty());
    assert_eq!(state.metrics.total_tokens, 0);
    assert_eq!(state.metrics.provider_calls, 0);
    assert!(state.current_task.is_none());
    assert_eq!(state.mcp.seq, 0);
}

// ═══════════════════════════════════════════════════════════════════════════════
// WORKFLOW OPS: has_breakpoint
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_has_breakpoint_checks_all_types() {
    let mut state = TuiState::new("test.nika.yaml");

    assert!(!state.has_breakpoint("task1"));

    state
        .breakpoints
        .insert(Breakpoint::BeforeTask("task1".to_string()));
    assert!(state.has_breakpoint("task1"));
    assert!(!state.has_breakpoint("task2"));

    state.breakpoints.clear();
    state
        .breakpoints
        .insert(Breakpoint::AfterTask("task1".to_string()));
    assert!(state.has_breakpoint("task1"));

    state.breakpoints.clear();
    state
        .breakpoints
        .insert(Breakpoint::OnError("task1".to_string()));
    assert!(state.has_breakpoint("task1"));

    state.breakpoints.clear();
    state
        .breakpoints
        .insert(Breakpoint::OnMcp("task1".to_string()));
    assert!(state.has_breakpoint("task1"));

    // OnAgentTurn is NOT checked by has_breakpoint
    state.breakpoints.clear();
    state
        .breakpoints
        .insert(Breakpoint::OnAgentTurn("task1".to_string(), 0));
    assert!(!state.has_breakpoint("task1"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// WORKFLOW OPS: should_break edge cases
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_should_break_empty_breakpoints_always_false() {
    let state = TuiState::new("test.nika.yaml");
    assert!(state.breakpoints.is_empty());

    let events = [
        EventKind::TaskStarted {
            task_id: Arc::from("t1"),
            verb: "infer".into(),
            inputs: Arc::new(serde_json::json!({})),
        },
        EventKind::TaskCompleted {
            task_id: Arc::from("t1"),
            output: Arc::new(serde_json::json!(null)),
            duration_ms: 100,
        },
        EventKind::TaskFailed {
            task_id: Arc::from("t1"),
            error: "err".to_string(),
            duration_ms: 100,
            error_code: None,
        },
    ];

    for event in &events {
        assert!(!state.should_break(event));
    }
}

#[test]
fn test_should_break_after_task() {
    let mut state = TuiState::new("test.nika.yaml");
    state
        .breakpoints
        .insert(Breakpoint::AfterTask("task1".to_string()));

    // TaskCompleted triggers AfterTask
    assert!(state.should_break(&EventKind::TaskCompleted {
        task_id: Arc::from("task1"),
        output: Arc::new(serde_json::json!(null)),
        duration_ms: 100,
    }));

    // TaskStarted does NOT trigger AfterTask
    assert!(!state.should_break(&EventKind::TaskStarted {
        task_id: Arc::from("task1"),
        verb: "infer".into(),
        inputs: Arc::new(serde_json::json!({})),
    }));
}

#[test]
fn test_should_break_on_error() {
    let mut state = TuiState::new("test.nika.yaml");
    state
        .breakpoints
        .insert(Breakpoint::OnError("task1".to_string()));

    assert!(state.should_break(&EventKind::TaskFailed {
        task_id: Arc::from("task1"),
        error: "boom".to_string(),
        duration_ms: 50,
        error_code: None,
    }));
}

#[test]
fn test_should_break_on_mcp() {
    let mut state = TuiState::new("test.nika.yaml");
    state
        .breakpoints
        .insert(Breakpoint::OnMcp("task1".to_string()));

    assert!(state.should_break(&EventKind::McpInvoke {
        task_id: Arc::from("task1"),
        call_id: "c1".to_string(),
        mcp_server: "novanet".to_string(),
        tool: Some("describe".to_string()),
        resource: None,
        params: None,
    }));
}

#[test]
fn test_should_break_on_agent_turn() {
    let mut state = TuiState::new("test.nika.yaml");
    state
        .breakpoints
        .insert(Breakpoint::OnAgentTurn("task1".to_string(), 2));

    // Matching turn index
    assert!(state.should_break(&EventKind::AgentTurn {
        task_id: Arc::from("task1"),
        turn_index: 2,
        kind: AgentTurnKind::Continue,
        metadata: None,
    }));

    // Non-matching turn index
    assert!(!state.should_break(&EventKind::AgentTurn {
        task_id: Arc::from("task1"),
        turn_index: 0,
        kind: AgentTurnKind::Continue,
        metadata: None,
    }));
}

#[test]
fn test_should_break_unrelated_event_returns_false() {
    let mut state = TuiState::new("test.nika.yaml");
    state
        .breakpoints
        .insert(Breakpoint::BeforeTask("task1".to_string()));

    // WorkflowStarted is not a breakpoint-able event
    assert!(!state.should_break(&EventKind::WorkflowStarted {
        task_count: 1,
        generation_id: "gen".to_string(),
        workflow_hash: "hash".to_string(),
        nika_version: TEST_VERSION.to_string(),
    }));
}

// ═══════════════════════════════════════════════════════════════════════════════
// WORKFLOW OPS: clear_dirty and dag_version
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_clear_dirty_resets_all_flags() {
    let mut state = TuiState::new("test.nika.yaml");
    state.dirty.mark_all();
    state.dirty.progress = true;
    state.dirty.status = true;
    assert!(state.dirty.any());

    state.clear_dirty();
    assert!(!state.dirty.any());
}

#[test]
fn test_dag_version_matches_timeline_version() {
    let mut state = TuiState::new("test.nika.yaml");
    assert_eq!(state.dag_version(), 0);

    state.invalidate_timeline_cache();
    assert_eq!(state.dag_version(), 1);
}

#[test]
fn test_threshold_notifications_all_fire_on_large_jump() {
    let mut state = TuiState::new("test.nika.yaml");

    // One huge provider response that crosses ALL thresholds at once
    // 96k tokens = 96% of 100k context window
    state.handle_event(
        &EventKind::ProviderResponded {
            task_id: "t".into(),
            request_id: None,
            input_tokens: 90_000,
            output_tokens: 6_000,
            cache_read_tokens: 0,
            cost_usd: 0.01,
            ttft_ms: None,
            finish_reason: FinishReason::Stop,
        },
        1,
    );

    // All 4 thresholds must have their guards set
    assert!(state.metrics.notified_50pct, "50% guard must be set");
    assert!(state.metrics.notified_70pct, "70% guard must be set");
    assert!(state.metrics.notified_85pct, "85% guard must be set");
    assert!(state.metrics.notified_95pct, "95% guard must be set");
    // And 4 notifications in the queue (one per threshold)
    assert_eq!(
        state.notifs.items.len(),
        4,
        "all 4 threshold notifications must have fired"
    );
}

#[test]
fn test_agent_complete_does_not_double_push_token_history() {
    let mut state = TuiState::new("test.nika.yaml");

    // Simulate 1 provider call (pushes 1 entry to token_history)
    state.handle_event(
        &EventKind::ProviderResponded {
            task_id: "t".into(),
            request_id: None,
            input_tokens: 100,
            output_tokens: 200,
            cache_read_tokens: 0,
            cost_usd: 0.0,
            ttft_ms: None,
            finish_reason: FinishReason::Stop,
        },
        1,
    );
    let history_after_provider = state.metrics.token_history.len();

    // Simulate agent complete
    state.handle_event(
        &EventKind::AgentComplete {
            task_id: "t".into(),
            turns: 1,
            stop_reason: AgentStopReason::NaturalCompletion,
        },
        2,
    );

    assert_eq!(
        state.metrics.token_history.len(),
        history_after_provider,
        "AgentComplete must not push an extra entry to token_history"
    );
}

#[test]
fn test_mcp_calls_cap_enforced_on_invoke() {
    let mut state = TuiState::new("test.nika.yaml");

    // Push more MCP invoke events than MAX_CALLS (200)
    for i in 0..201usize {
        state.handle_event(
            &EventKind::McpInvoke {
                task_id: "t".into(),
                mcp_server: "novanet".to_string(),
                tool: Some(format!("tool_{}", i)),
                resource: None,
                call_id: format!("call_{}", i),
                params: None,
            },
            i as u64,
        );
    }

    assert!(
        state.mcp.calls.len() <= 200,
        "mcp.calls len {} must be <= 200 (MAX_CALLS)",
        state.mcp.calls.len()
    );
}

#[test]
fn test_workflow_failed_kills_running_tasks() {
    let mut state = TuiState::new("test.nika.yaml");

    // Schedule and start a task
    state.handle_event(
        &EventKind::TaskScheduled {
            task_id: Arc::from("t1"),
            dependencies: vec![],
        },
        1,
    );
    state.handle_event(
        &EventKind::TaskStarted {
            task_id: Arc::from("t1"),
            verb: "infer".into(),
            inputs: Arc::new(serde_json::json!({})),
        },
        2,
    );
    assert_eq!(state.tasks["t1"].status, TaskStatus::Running);

    // Workflow failure must kill the running task
    state.handle_event(
        &EventKind::WorkflowFailed {
            error: "timeout".to_string(),
            failed_task: None,
        },
        3,
    );

    assert_eq!(
        state.tasks["t1"].status,
        TaskStatus::Failed,
        "WorkflowFailed must transition Running tasks to Failed"
    );
    assert_eq!(
        state.tasks["t1"].error.as_deref(),
        Some("timeout"),
        "failed task must carry the workflow error message"
    );
}
