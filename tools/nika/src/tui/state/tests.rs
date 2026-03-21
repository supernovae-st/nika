//! Tests for TUI state module
//!
//! Extracted from state.rs to reduce file size.

use super::*;

/// Use actual package version in tests to avoid version drift
const TEST_VERSION: &str = env!("CARGO_PKG_VERSION");

#[test]
fn test_panel_id_next_cycles() {
    assert_eq!(PanelId::Progress.next(), PanelId::Dag);
    assert_eq!(PanelId::Agent.next(), PanelId::Progress);
}

#[test]
fn test_panel_id_prev_cycles() {
    assert_eq!(PanelId::Progress.prev(), PanelId::Agent);
    assert_eq!(PanelId::Dag.prev(), PanelId::Progress);
}

#[test]
fn test_panel_id_all_returns_all_panels() {
    let all = PanelId::all();
    assert_eq!(all.len(), 4);
    assert_eq!(all[0], PanelId::Progress);
    assert_eq!(all[1], PanelId::Dag);
    assert_eq!(all[2], PanelId::NovaNet);
    assert_eq!(all[3], PanelId::Agent);
}

#[test]
fn test_panel_id_number() {
    assert_eq!(PanelId::Progress.number(), 1);
    assert_eq!(PanelId::Dag.number(), 2);
    assert_eq!(PanelId::NovaNet.number(), 3);
    assert_eq!(PanelId::Agent.number(), 4);
}

#[test]
fn test_panel_id_title() {
    assert_eq!(PanelId::Progress.title(), "MISSION CONTROL");
    assert_eq!(PanelId::Dag.title(), "DAG EXECUTION");
    assert_eq!(PanelId::NovaNet.title(), "NOVANET STATION");
    assert_eq!(PanelId::Agent.title(), "AGENT REASONING");
}

#[test]
fn test_panel_id_icon() {
    assert_eq!(PanelId::Progress.icon(), "◉");
    assert_eq!(PanelId::Dag.icon(), "⎔");
    assert_eq!(PanelId::NovaNet.icon(), "⊛");
    assert_eq!(PanelId::Agent.icon(), "⊕");
}

#[test]
fn test_panel_id_complete_cycle() {
    let mut current = PanelId::Progress;
    let mut count = 0;

    // Cycle through all panels
    for _ in 0..4 {
        current = current.next();
        count += 1;
    }

    // Should be back to Progress after 4 cycles
    assert_eq!(current, PanelId::Progress);
    assert_eq!(count, 4);
}

#[test]
fn test_panel_id_reverse_cycle() {
    let mut current = PanelId::Progress;
    let mut count = 0;

    // Reverse cycle through all panels
    for _ in 0..4 {
        current = current.prev();
        count += 1;
    }

    // Should be back to Progress after 4 reverse cycles
    assert_eq!(current, PanelId::Progress);
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
    assert_eq!(state.ui.focus, PanelId::Progress);

    state.focus_next();
    assert_eq!(state.ui.focus, PanelId::Dag);

    state.focus_panel(4);
    assert_eq!(state.ui.focus, PanelId::Agent);

    state.focus_prev();
    assert_eq!(state.ui.focus, PanelId::NovaNet);
}

#[test]
fn test_tui_state_cycle_tab() {
    use crate::tui::views::{DagTab, MissionTab, NovanetTab, ReasoningTab};

    let mut state = TuiState::new("test.yaml");

    // Test Mission tab cycling (Progress → TaskIO → Output → Progress)
    state.ui.focus = PanelId::Progress;
    assert_eq!(state.ui.mission_tab, MissionTab::Progress);
    state.cycle_tab();
    assert_eq!(state.ui.mission_tab, MissionTab::TaskIO);
    state.cycle_tab();
    assert_eq!(state.ui.mission_tab, MissionTab::Output);
    state.cycle_tab();
    assert_eq!(state.ui.mission_tab, MissionTab::Progress);

    // Test Dag tab cycling (Graph ↔ Yaml)
    state.ui.focus = PanelId::Dag;
    assert_eq!(state.ui.dag_tab, DagTab::Graph);
    state.cycle_tab();
    assert_eq!(state.ui.dag_tab, DagTab::Yaml);
    state.cycle_tab();
    assert_eq!(state.ui.dag_tab, DagTab::Graph);

    // Test NovaNet tab cycling (Summary ↔ FullJson)
    state.ui.focus = PanelId::NovaNet;
    assert_eq!(state.ui.novanet_tab, NovanetTab::Summary);
    state.cycle_tab();
    assert_eq!(state.ui.novanet_tab, NovanetTab::FullJson);
    state.cycle_tab();
    assert_eq!(state.ui.novanet_tab, NovanetTab::Summary);

    // Test Reasoning tab cycling (Turns → Thinking → Steps → Turns)
    state.ui.focus = PanelId::Agent;
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
            inputs: serde_json::json!({}),
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
            params: Some(test_params.clone()),
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
            response: Some(test_response.clone()),
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
            params: Some(serde_json::json!({"invalid": "params"})),
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
            response: Some(serde_json::json!({"error": "Invalid params"})),
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
            params: Some(serde_json::json!({"locale": "fr-FR"})),
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
            params: Some(serde_json::json!({"locale": "en-US"})),
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
            response: Some(serde_json::json!({"content": "English content"})),
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
            response: Some(serde_json::json!({"content": "French content"})),
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
        inputs: serde_json::json!({}),
    };
    assert!(state.should_break(&event));

    let event2 = EventKind::TaskStarted {
        verb: "infer".into(),
        task_id: Arc::from("task2"),
        inputs: serde_json::json!({}),
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
            inputs: serde_json::json!({}),
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
            inputs: serde_json::json!({}),
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
    use crate::config::ApiKeys;

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
    use crate::config::ApiKeys;

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
    use crate::config::ApiKeys;

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
    use crate::config::ApiKeys;

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
    assert!(display.contains("claude"));
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
        state.mcp.calls.push(McpCall {
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
        state.mcp.calls.push(McpCall {
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
    state.mcp.calls.push(McpCall {
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
    state.mcp.calls.push(McpCall {
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
    state.mcp.calls.push(McpCall {
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
    state.mcp.calls.push(McpCall {
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
    state.mcp.calls.push(McpCall {
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
    state.mcp.calls.push(McpCall {
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
    state.mcp.calls.push(McpCall {
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

    // Dismiss most recent
    state.dismiss_notification();
    assert!(state.notifs.items[2].dismissed);
    assert!(!state.notifs.items[1].dismissed);
    assert!(!state.notifs.items[0].dismissed);

    // Dismiss next most recent
    state.dismiss_notification();
    assert!(state.notifs.items[1].dismissed);
    assert!(!state.notifs.items[0].dismissed);
}

#[test]
fn test_dismiss_all_notifications() {
    let mut state = TuiState::new("test.yaml");

    state.add_notification(Notification::info("1", 1000));
    state.add_notification(Notification::info("2", 2000));
    state.add_notification(Notification::info("3", 3000));

    state.dismiss_all_notifications();

    assert!(state.notifs.items.iter().all(|n| n.dismissed));
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
    assert!(flags.is_panel_dirty(PanelId::Progress));
    assert!(flags.is_panel_dirty(PanelId::Dag));
    assert!(flags.is_panel_dirty(PanelId::NovaNet));
    assert!(flags.is_panel_dirty(PanelId::Agent));

    // Individual flags
    flags.all = false;
    assert!(!flags.is_panel_dirty(PanelId::Progress));

    flags.progress = true;
    assert!(flags.is_panel_dirty(PanelId::Progress));
    assert!(!flags.is_panel_dirty(PanelId::Dag));

    flags.dag = true;
    assert!(flags.is_panel_dirty(PanelId::Dag));

    flags.novanet = true;
    assert!(flags.is_panel_dirty(PanelId::NovaNet));

    flags.reasoning = true;
    assert!(flags.is_panel_dirty(PanelId::Agent));
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
            inputs: serde_json::json!({}),
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
            kind: "started".to_string(),
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
            stop_reason: "natural".to_string(),
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
            inputs: serde_json::json!({}),
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
        &crate::event::EventKind::TaskStarted {
            task_id: std::sync::Arc::from("task1"),
            verb: std::sync::Arc::from("infer"),
            inputs: serde_json::json!({}),
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
