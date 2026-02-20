//! TUI Render Snapshot Tests
//!
//! Visual rendering tests using ratatui TestBackend and insta snapshots.
//! These tests verify that widgets render correctly with various states.
//!
//! Run with: `cargo test --test tui_render_snapshot_test --features tui`

#![cfg(feature = "tui")]

use insta::assert_snapshot;
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

use nika::tui::widgets::{AgentTurns, Gauge, McpEntry, McpLog, Timeline, TimelineEntry, TurnEntry};
use nika::tui::TaskStatus;

// =============================================================================
// HELPERS
// =============================================================================

/// Render a widget to a string for snapshot testing
fn render_widget_to_string<W: Widget>(widget: W, width: u16, height: u16) -> String {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    widget.render(area, &mut buffer);
    buffer_to_string(&buffer)
}

/// Convert buffer to string representation
fn buffer_to_string(buffer: &Buffer) -> String {
    let mut result = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            result.push_str(cell.symbol());
        }
        result.push('\n');
    }
    result
}

// =============================================================================
// GAUGE WIDGET TESTS
// =============================================================================

#[test]
fn test_gauge_empty_progress() {
    let gauge = Gauge::new(0.0).label("Progress");
    let output = render_widget_to_string(gauge, 40, 1);
    assert_snapshot!(output);
}

#[test]
fn test_gauge_half_progress() {
    let gauge = Gauge::new(0.5).label("Building");
    let output = render_widget_to_string(gauge, 40, 1);
    assert_snapshot!(output);
}

#[test]
fn test_gauge_full_progress() {
    let gauge = Gauge::new(1.0).label("Complete");
    let output = render_widget_to_string(gauge, 40, 1);
    assert_snapshot!(output);
}

#[test]
fn test_gauge_for_progress_counts() {
    let gauge = Gauge::for_progress(3, 10);
    let output = render_widget_to_string(gauge, 40, 1);
    assert_snapshot!(output);
}

#[test]
fn test_gauge_narrow_width() {
    let gauge = Gauge::new(0.7).label("Narrow");
    let output = render_widget_to_string(gauge, 20, 1);
    assert_snapshot!(output);
}

#[test]
fn test_gauge_wide_width() {
    let gauge = Gauge::new(0.33).label("Wide render");
    let output = render_widget_to_string(gauge, 80, 1);
    assert_snapshot!(output);
}

// =============================================================================
// TIMELINE WIDGET TESTS
// =============================================================================

#[test]
fn test_timeline_empty() {
    let entries: Vec<TimelineEntry> = vec![];
    let timeline = Timeline::new(&entries);
    let output = render_widget_to_string(timeline, 60, 4);
    assert_snapshot!(output);
}

#[test]
fn test_timeline_single_pending() {
    let entries = vec![TimelineEntry::new("step1", TaskStatus::Pending)];
    let timeline = Timeline::new(&entries);
    let output = render_widget_to_string(timeline, 60, 4);
    assert_snapshot!(output);
}

#[test]
fn test_timeline_mixed_statuses() {
    let entries = vec![
        TimelineEntry::new("init", TaskStatus::Success).with_duration(100),
        TimelineEntry::new("build", TaskStatus::Success).with_duration(500),
        TimelineEntry::new("test", TaskStatus::Running).current(),
        TimelineEntry::new("deploy", TaskStatus::Pending),
    ];
    let timeline = Timeline::new(&entries).elapsed(1500).with_frame(0);
    let output = render_widget_to_string(timeline, 60, 4);
    assert_snapshot!(output);
}

#[test]
fn test_timeline_all_completed() {
    let entries = vec![
        TimelineEntry::new("fetch", TaskStatus::Success).with_duration(200),
        TimelineEntry::new("parse", TaskStatus::Success).with_duration(150),
        TimelineEntry::new("infer", TaskStatus::Success).with_duration(800),
        TimelineEntry::new("save", TaskStatus::Success).with_duration(50),
    ];
    let timeline = Timeline::new(&entries).elapsed(1200);
    let output = render_widget_to_string(timeline, 60, 4);
    assert_snapshot!(output);
}

#[test]
fn test_timeline_with_failure() {
    let entries = vec![
        TimelineEntry::new("setup", TaskStatus::Success),
        TimelineEntry::new("build", TaskStatus::Failed),
        TimelineEntry::new("test", TaskStatus::Pending),
    ];
    let timeline = Timeline::new(&entries).elapsed(500);
    let output = render_widget_to_string(timeline, 60, 4);
    assert_snapshot!(output);
}

#[test]
fn test_timeline_with_breakpoint() {
    let entries = vec![
        TimelineEntry::new("step1", TaskStatus::Success),
        TimelineEntry::new("step2", TaskStatus::Paused).with_breakpoint(true),
        TimelineEntry::new("step3", TaskStatus::Pending),
    ];
    let timeline = Timeline::new(&entries).elapsed(300);
    let output = render_widget_to_string(timeline, 60, 5);
    assert_snapshot!(output);
}

#[test]
fn test_timeline_many_tasks() {
    let entries = (1..=10)
        .map(|i| {
            let status = if i <= 7 {
                TaskStatus::Success
            } else if i == 8 {
                TaskStatus::Running
            } else {
                TaskStatus::Pending
            };
            TimelineEntry::new(format!("t{}", i), status)
        })
        .collect::<Vec<_>>();
    let timeline = Timeline::new(&entries).elapsed(5000);
    let output = render_widget_to_string(timeline, 80, 4);
    assert_snapshot!(output);
}

// =============================================================================
// MCP LOG WIDGET TESTS
// =============================================================================

#[test]
fn test_mcp_log_empty() {
    let entries: Vec<McpEntry> = vec![];
    let mcp_log = McpLog::new(&entries);
    let output = render_widget_to_string(mcp_log, 50, 5);
    assert_snapshot!(output);
}

#[test]
fn test_mcp_log_single_pending() {
    let entries = vec![McpEntry::new(1, "novanet").with_tool("novanet_describe")];
    let mcp_log = McpLog::new(&entries);
    let output = render_widget_to_string(mcp_log, 50, 5);
    assert_snapshot!(output);
}

#[test]
fn test_mcp_log_single_completed() {
    let entries = vec![McpEntry::new(1, "novanet")
        .with_tool("novanet_describe")
        .completed(2048)];
    let mcp_log = McpLog::new(&entries);
    let output = render_widget_to_string(mcp_log, 50, 5);
    assert_snapshot!(output);
}

#[test]
fn test_mcp_log_multiple_mixed() {
    let entries = vec![
        McpEntry::new(1, "novanet")
            .with_tool("novanet_describe")
            .completed(1024),
        McpEntry::new(2, "novanet")
            .with_tool("novanet_traverse")
            .completed(4096),
        McpEntry::new(3, "perplexity").with_tool("perplexity_search"),
    ];
    let mcp_log = McpLog::new(&entries).max_entries(5);
    let output = render_widget_to_string(mcp_log, 50, 5);
    assert_snapshot!(output);
}

#[test]
fn test_mcp_log_with_resource() {
    let entries = vec![McpEntry::new(1, "filesystem")
        .with_resource("file:///src/main.rs")
        .completed(5000)];
    let mcp_log = McpLog::new(&entries);
    let output = render_widget_to_string(mcp_log, 50, 5);
    assert_snapshot!(output);
}

#[test]
fn test_mcp_log_narrow_truncation() {
    let entries = vec![McpEntry::new(1, "novanet")
        .with_tool("novanet_context_build_log_very_long_name")
        .completed(10000)];
    let mcp_log = McpLog::new(&entries);
    let output = render_widget_to_string(mcp_log, 30, 5);
    assert_snapshot!(output);
}

// =============================================================================
// AGENT TURNS WIDGET TESTS
// =============================================================================

#[test]
fn test_agent_turns_empty() {
    let entries: Vec<TurnEntry> = vec![];
    let turns = AgentTurns::new(&entries);
    let output = render_widget_to_string(turns, 50, 5);
    assert_snapshot!(output);
}

#[test]
fn test_agent_turns_single_thinking() {
    let entries = vec![TurnEntry::new(0, "thinking").with_tokens(500).current()];
    let turns = AgentTurns::new(&entries);
    let output = render_widget_to_string(turns, 50, 5);
    assert_snapshot!(output);
}

#[test]
fn test_agent_turns_tool_sequence() {
    let entries = vec![
        TurnEntry::new(0, "thinking").with_tokens(800),
        TurnEntry::new(1, "tool_use")
            .with_tokens(200)
            .with_tool_calls(vec!["read_file".to_string()]),
        TurnEntry::new(2, "tool_result").with_tokens(150),
        TurnEntry::new(3, "response").with_tokens(600).current(),
    ];
    let turns = AgentTurns::new(&entries);
    let output = render_widget_to_string(turns, 50, 5);
    assert_snapshot!(output);
}

#[test]
fn test_agent_turns_completed() {
    let entries = vec![
        TurnEntry::new(0, "thinking").with_tokens(1000),
        TurnEntry::new(1, "tool_use").with_tokens(200),
        TurnEntry::new(2, "tool_result").with_tokens(500),
        TurnEntry::new(3, "complete").with_tokens(100),
    ];
    let turns = AgentTurns::new(&entries);
    let output = render_widget_to_string(turns, 50, 5);
    assert_snapshot!(output);
}

#[test]
fn test_agent_turns_error_state() {
    let entries = vec![
        TurnEntry::new(0, "thinking").with_tokens(500),
        TurnEntry::new(1, "tool_use").with_tokens(100),
        TurnEntry::new(2, "error").with_tokens(50),
    ];
    let turns = AgentTurns::new(&entries);
    let output = render_widget_to_string(turns, 50, 5);
    assert_snapshot!(output);
}

#[test]
fn test_agent_turns_many_turns() {
    let entries = (0..8)
        .map(|i| {
            let status = match i % 4 {
                0 => "thinking",
                1 => "tool_use",
                2 => "tool_result",
                _ => "response",
            };
            TurnEntry::new(i, status).with_tokens((i + 1) * 200)
        })
        .collect::<Vec<_>>();
    let turns = AgentTurns::new(&entries);
    let output = render_widget_to_string(turns, 50, 10);
    assert_snapshot!(output);
}

#[test]
fn test_agent_turns_reversed() {
    let entries = vec![
        TurnEntry::new(0, "thinking").with_tokens(500),
        TurnEntry::new(1, "response").with_tokens(300).current(),
    ];
    let turns = AgentTurns::new(&entries).reverse(true);
    let output = render_widget_to_string(turns, 50, 5);
    assert_snapshot!(output);
}

// =============================================================================
// COMPOSITE RENDERING TESTS
// =============================================================================

#[test]
fn test_composite_mission_panel_elements() {
    // Render multiple widgets as they would appear in mission panel
    let mut result = String::new();

    // Gauge
    let gauge = Gauge::for_progress(2, 4);
    result.push_str("=== GAUGE ===\n");
    result.push_str(&render_widget_to_string(gauge, 40, 1));
    result.push('\n');

    // Timeline
    let entries = vec![
        TimelineEntry::new("init", TaskStatus::Success),
        TimelineEntry::new("build", TaskStatus::Running).current(),
        TimelineEntry::new("test", TaskStatus::Pending),
        TimelineEntry::new("deploy", TaskStatus::Pending),
    ];
    let timeline = Timeline::new(&entries).elapsed(2500);
    result.push_str("=== TIMELINE ===\n");
    result.push_str(&render_widget_to_string(timeline, 60, 4));

    assert_snapshot!(result);
}

#[test]
fn test_composite_novanet_panel_elements() {
    // Render MCP log as it appears in NovaNet panel
    let entries = vec![
        McpEntry::new(1, "novanet")
            .with_tool("novanet_describe")
            .completed(2048),
        McpEntry::new(2, "novanet")
            .with_tool("novanet_traverse")
            .completed(4096),
        McpEntry::new(3, "novanet").with_tool("novanet_generate"),
    ];
    let mcp_log = McpLog::new(&entries).max_entries(10);
    let output = render_widget_to_string(mcp_log, 60, 10);
    assert_snapshot!(output);
}

#[test]
fn test_composite_agent_panel_elements() {
    // Render agent turns as in Agent Reasoning panel
    let entries = vec![
        TurnEntry::new(0, "thinking").with_tokens(1200),
        TurnEntry::new(1, "tool_use")
            .with_tokens(300)
            .with_tool_calls(vec!["novanet_describe".to_string()]),
        TurnEntry::new(2, "tool_result").with_tokens(500),
        TurnEntry::new(3, "thinking").with_tokens(800),
        TurnEntry::new(4, "response").with_tokens(400).current(),
    ];
    let turns = AgentTurns::new(&entries);
    let output = render_widget_to_string(turns, 60, 10);
    assert_snapshot!(output);
}

// =============================================================================
// EDGE CASE TESTS
// =============================================================================

#[test]
fn test_minimum_size_rendering() {
    // Test widgets at minimum renderable size
    let gauge = Gauge::new(0.5);
    let gauge_output = render_widget_to_string(gauge, 5, 1);
    assert_snapshot!("minimum_gauge", gauge_output);

    let entries = vec![TimelineEntry::new("x", TaskStatus::Running)];
    let timeline = Timeline::new(&entries);
    let timeline_output = render_widget_to_string(timeline, 10, 3);
    assert_snapshot!("minimum_timeline", timeline_output);
}

#[test]
fn test_unicode_handling() {
    // Test that unicode characters render correctly
    let entries = vec![
        TurnEntry::new(0, "thinking").with_tokens(500),
        TurnEntry::new(1, "tool_use").with_tokens(200),
    ];
    let turns = AgentTurns::new(&entries);
    let output = render_widget_to_string(turns, 50, 5);

    // Verify output contains expected unicode
    assert!(
        output.contains("🤔") || output.contains("thinking"),
        "Should show thinking indicator"
    );
    assert_snapshot!(output);
}

#[test]
fn test_long_task_names_truncation() {
    // Test that long names are properly truncated
    let entries = vec![TimelineEntry::new(
        "very_long_task_name_that_should_be_truncated",
        TaskStatus::Running,
    )];
    let timeline = Timeline::new(&entries);
    let output = render_widget_to_string(timeline, 30, 4);
    assert_snapshot!(output);
}

// =============================================================================
// WORKFLOW STATE SNAPSHOTS
// =============================================================================

#[test]
fn test_workflow_idle_state() {
    // Simulate idle workflow state rendering
    let mut result = String::new();

    result.push_str("Workflow: idle-workflow.nika.yaml\n");
    result.push_str("Status: Idle\n\n");

    let gauge = Gauge::new(0.0).label("Ready");
    result.push_str(&render_widget_to_string(gauge, 40, 1));

    assert_snapshot!(result);
}

#[test]
fn test_workflow_running_state() {
    // Simulate running workflow state rendering
    let mut result = String::new();

    result.push_str("Workflow: generate-content.nika.yaml\n");
    result.push_str("Status: Running\n\n");

    // Progress gauge
    let gauge = Gauge::for_progress(2, 5);
    result.push_str(&render_widget_to_string(gauge, 50, 1));
    result.push('\n');

    // Timeline
    let entries = vec![
        TimelineEntry::new("fetch_ctx", TaskStatus::Success),
        TimelineEntry::new("generate", TaskStatus::Running).current(),
        TimelineEntry::new("validate", TaskStatus::Pending),
        TimelineEntry::new("publish", TaskStatus::Pending),
        TimelineEntry::new("notify", TaskStatus::Pending),
    ];
    let timeline = Timeline::new(&entries).elapsed(15000);
    result.push_str(&render_widget_to_string(timeline, 70, 4));

    assert_snapshot!(result);
}

#[test]
fn test_workflow_completed_state() {
    // Simulate completed workflow state rendering
    let mut result = String::new();

    result.push_str("Workflow: batch-process.nika.yaml\n");
    result.push_str("Status: Completed\n\n");

    // Progress gauge
    let gauge = Gauge::for_progress(4, 4);
    result.push_str(&render_widget_to_string(gauge, 50, 1));
    result.push('\n');

    // Timeline
    let entries = vec![
        TimelineEntry::new("init", TaskStatus::Success).with_duration(200),
        TimelineEntry::new("process", TaskStatus::Success).with_duration(5000),
        TimelineEntry::new("verify", TaskStatus::Success).with_duration(1000),
        TimelineEntry::new("cleanup", TaskStatus::Success).with_duration(100),
    ];
    let timeline = Timeline::new(&entries).elapsed(6300);
    result.push_str(&render_widget_to_string(timeline, 70, 4));

    assert_snapshot!(result);
}

#[test]
fn test_workflow_failed_state() {
    // Simulate failed workflow state rendering
    let mut result = String::new();

    result.push_str("Workflow: risky-operation.nika.yaml\n");
    result.push_str("Status: Failed\n\n");

    // Progress gauge (failed at 60%)
    let gauge = Gauge::new(0.6).label("Failed at step 3");
    result.push_str(&render_widget_to_string(gauge, 50, 1));
    result.push('\n');

    // Timeline
    let entries = vec![
        TimelineEntry::new("setup", TaskStatus::Success),
        TimelineEntry::new("fetch", TaskStatus::Success),
        TimelineEntry::new("transform", TaskStatus::Failed),
        TimelineEntry::new("load", TaskStatus::Pending),
        TimelineEntry::new("verify", TaskStatus::Pending),
    ];
    let timeline = Timeline::new(&entries).elapsed(3500);
    result.push_str(&render_widget_to_string(timeline, 70, 4));

    assert_snapshot!(result);
}

// =============================================================================
// TERMINAL SIZE RESPONSIVENESS
// =============================================================================

#[test]
fn test_small_terminal_80x24() {
    // Standard small terminal
    let gauge = Gauge::for_progress(2, 5);
    let output = render_widget_to_string(gauge, 80, 1);
    assert_snapshot!("gauge_80x24", output);

    let entries = vec![
        TimelineEntry::new("t1", TaskStatus::Success),
        TimelineEntry::new("t2", TaskStatus::Running),
        TimelineEntry::new("t3", TaskStatus::Pending),
    ];
    let timeline = Timeline::new(&entries);
    let output = render_widget_to_string(timeline, 80, 4);
    assert_snapshot!("timeline_80x24", output);
}

#[test]
fn test_wide_terminal_120() {
    // Wide terminal (common for dev workstations)
    let entries = vec![
        TimelineEntry::new("init", TaskStatus::Success),
        TimelineEntry::new("fetch", TaskStatus::Success),
        TimelineEntry::new("parse", TaskStatus::Success),
        TimelineEntry::new("validate", TaskStatus::Running),
        TimelineEntry::new("transform", TaskStatus::Pending),
        TimelineEntry::new("save", TaskStatus::Pending),
    ];
    let timeline = Timeline::new(&entries).elapsed(10000);
    let output = render_widget_to_string(timeline, 120, 4);
    assert_snapshot!(output);
}

#[test]
fn test_narrow_terminal_40() {
    // Narrow terminal (split panes)
    let entries = vec![
        TimelineEntry::new("a", TaskStatus::Success),
        TimelineEntry::new("b", TaskStatus::Running),
        TimelineEntry::new("c", TaskStatus::Pending),
    ];
    let timeline = Timeline::new(&entries);
    let output = render_widget_to_string(timeline, 40, 4);
    assert_snapshot!(output);
}
